//! Terminal setup/teardown and the event loop.

use std::error::Error;
use std::io::{self, stdout, Stdout, Write};
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver};
use std::sync::Once;
use std::time::{Duration, Instant};

use crossterm::cursor::Show;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEvent, KeyEventKind, MouseEvent,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::{Frame, Terminal};

use crate::app::{action_error_message, App, Command};
use crate::gfx::detect::cell_px_from_window;
use crate::gfx::present::RowPlacement;
use crate::gfx::raster::RasterStyle;
use crate::gfx::session::{self, present_frame, GfxMode, GfxSession};
use crate::gfx::CellPx;
use crate::git::{Git, GitError};
use crate::input::{self, translate_key, ClickTracker};
use crate::manygit::view::Manygit;
use crate::ui::dialog::Dialog;
use crate::watch::{self, WatchEvent, WatchHandle};

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Enters raw mode, the alternate screen and mouse capture; a panic restores the terminal
/// first. If entering fails partway, what was already changed is undone.
pub fn enter() -> io::Result<Tui> {
    static PANIC_HOOK: Once = Once::new();
    PANIC_HOOK.call_once(|| {
        let previous = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |info| {
            let _ = restore();
            previous(info);
        }));
    });
    enable_raw_mode()?;
    let entered = execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)
        .and_then(|()| Terminal::new(CrosstermBackend::new(stdout())));
    if entered.is_err() {
        let _ = restore();
    }
    entered
}

/// Deletes all graph images first, then leaves raw mode and the alternate screen; each
/// step is attempted even if another fails.
pub fn restore() -> io::Result<()> {
    let images = clear_graphics(&mut stdout(), session::graphics_used());
    let raw = disable_raw_mode();
    let screen = execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen, Show);
    images.and(raw).and(screen)
}

/// Removes every image placement and image from the terminal when graphics were used;
/// writes nothing otherwise.
pub fn clear_graphics(out: &mut impl Write, used: bool) -> io::Result<()> {
    if !used {
        return Ok(());
    }
    out.write_all(&session::exit_sequence())?;
    out.flush()
}

/// The graph images a screen wants shown after a draw.
pub struct GfxFrame {
    pub placements: Vec<RowPlacement>,
    pub style: RasterStyle,
    /// Renders the RGBA pixels of the row image with a placement's key.
    pub render: Box<dyn FnMut(u64) -> Vec<u8>>,
}

/// What the event loop drives: a single repository's graph or the Manygit list.
pub trait Screen {
    fn should_quit(&self) -> bool;
    fn draw(&self, frame: &mut Frame);
    /// Reads anything still loading after a draw; returns whether something loaded.
    fn load_pending(&mut self) -> Result<bool, GitError>;
    /// Takes a finished git action's result; returns whether one finished.
    fn poll_job(&mut self) -> bool;
    /// Runs periodic work due at `now`; returns whether the view changed.
    fn tick(&mut self, now: Instant) -> bool;
    fn on_watch(&mut self, ev: WatchEvent);
    /// The repository to watch for changes, if a graph is shown.
    fn watched_dir(&self) -> Option<&Path>;
    fn key_command(&self, key: KeyEvent) -> Option<Command>;
    fn mouse_command(
        &self,
        area: Rect,
        ev: MouseEvent,
        clicks: &mut ClickTracker,
        now: Instant,
    ) -> Option<Command>;
    fn apply(&mut self, cmd: Command) -> Result<(), GitError>;
    fn show_dialog(&mut self, dialog: Dialog);
    /// Graph images for the screen last drawn in `area`; `None` while the graph is
    /// not shown or is covered (menu, dialog, too small), which draws it as text.
    fn gfx_frame(&mut self, _area: Rect, _cell: CellPx) -> Option<GfxFrame> {
        None
    }
    /// Whether graph cells are drawn blank, for images to show through.
    fn set_gfx_active(&mut self, _on: bool) {}
}

impl Screen for App {
    fn should_quit(&self) -> bool {
        App::should_quit(self)
    }
    fn draw(&self, frame: &mut Frame) {
        App::draw(self, frame);
    }
    fn load_pending(&mut self) -> Result<bool, GitError> {
        App::load_pending(self)
    }
    fn poll_job(&mut self) -> bool {
        App::poll_job(self)
    }
    fn tick(&mut self, _now: Instant) -> bool {
        false
    }
    fn on_watch(&mut self, ev: WatchEvent) {
        App::on_watch(self, ev);
    }
    fn watched_dir(&self) -> Option<&Path> {
        Some(self.git().dir())
    }
    fn key_command(&self, key: KeyEvent) -> Option<Command> {
        key_command(key, self)
    }
    fn mouse_command(
        &self,
        area: Rect,
        ev: MouseEvent,
        clicks: &mut ClickTracker,
        now: Instant,
    ) -> Option<Command> {
        input::mouse_command(self, area, ev, clicks, now)
    }
    fn apply(&mut self, cmd: Command) -> Result<(), GitError> {
        App::apply(self, cmd)
    }
    fn show_dialog(&mut self, dialog: Dialog) {
        self.open_dialog(dialog);
    }
    fn gfx_frame(&mut self, area: Rect, cell: CellPx) -> Option<GfxFrame> {
        App::gfx_frame(self, area, cell)
    }
    fn set_gfx_active(&mut self, on: bool) {
        App::set_gfx_active(self, on);
    }
}

impl Screen for Manygit {
    fn should_quit(&self) -> bool {
        Manygit::should_quit(self)
    }
    fn draw(&self, frame: &mut Frame) {
        Manygit::draw(self, frame);
    }
    fn load_pending(&mut self) -> Result<bool, GitError> {
        Manygit::load_pending(self)
    }
    fn poll_job(&mut self) -> bool {
        Manygit::poll_job(self)
    }
    fn tick(&mut self, now: Instant) -> bool {
        Manygit::tick(self, now)
    }
    fn on_watch(&mut self, ev: WatchEvent) {
        Manygit::on_watch(self, ev);
    }
    fn watched_dir(&self) -> Option<&Path> {
        self.current_path()
    }
    fn key_command(&self, key: KeyEvent) -> Option<Command> {
        Manygit::key_command(self, key)
    }
    fn mouse_command(
        &self,
        area: Rect,
        ev: MouseEvent,
        clicks: &mut ClickTracker,
        now: Instant,
    ) -> Option<Command> {
        Manygit::mouse_command(self, area, ev, clicks, now)
    }
    fn apply(&mut self, cmd: Command) -> Result<(), GitError> {
        Manygit::apply(self, cmd)
    }
    fn show_dialog(&mut self, dialog: Dialog) {
        Manygit::show_dialog(self, dialog);
    }
    fn gfx_frame(&mut self, area: Rect, cell: CellPx) -> Option<GfxFrame> {
        Manygit::gfx_frame(self, area, cell)
    }
    fn set_gfx_active(&mut self, on: bool) {
        Manygit::set_gfx_active(self, on);
    }
}

/// Applies `cmd`; a git failure is shown in a dialog instead of ending the app.
pub fn apply_command(screen: &mut impl Screen, cmd: Command) {
    if let Err(e) = screen.apply(cmd) {
        screen.show_dialog(Dialog::Error {
            title: "Git error".to_string(),
            message: action_error_message(&e),
        });
    }
}

/// How long to wait for input before checking for changes again.
const INPUT_POLL: Duration = Duration::from_millis(50);

/// Runs `screen` until it quits, always restoring the terminal. Filesystem changes in
/// the shown repository, finished git actions and periodic refreshes redraw the view.
/// In graphics `mode` the graph column is shown as images.
pub fn run(screen: &mut impl Screen, mode: GfxMode) -> Result<(), Box<dyn Error>> {
    let mut term = enter()?;
    let result = event_loop(&mut term, screen, &mut GfxSession::new(mode));
    // A loop error says more than a failure to restore the terminal afterwards.
    let restored = restore();
    result?;
    restored?;
    Ok(())
}

/// The watcher of the shown repository. `live` is `None` when watching it failed;
/// the failure was shown once and is not retried for the same repository.
struct Watching {
    dir: PathBuf,
    live: Option<(WatchHandle, Receiver<WatchEvent>)>,
}

/// Starts watching the repository the screen shows, if it changed since last time.
fn sync_watch(screen: &mut impl Screen, watching: &mut Option<Watching>) {
    let want = screen.watched_dir().map(Path::to_path_buf);
    if watching.as_ref().map(|w| &w.dir) == want.as_ref() {
        return;
    }
    // Drop the old watcher before starting the next one.
    *watching = None;
    let Some(dir) = want else { return };
    let (tx, rx) = mpsc::channel();
    let live = match watch::watch(&Git::at(&dir), tx) {
        Ok(handle) => Some((handle, rx)),
        Err(e) => {
            screen.show_dialog(Dialog::Error {
                title: "Cannot watch for changes".to_string(),
                message: format!("{e}\nPress r to refresh by hand."),
            });
            None
        }
    };
    *watching = Some(Watching { dir, live });
}

fn event_loop(
    term: &mut Tui,
    screen: &mut impl Screen,
    gfx: &mut GfxSession,
) -> Result<(), Box<dyn Error>> {
    let mut clicks = ClickTracker::new();
    let mut watching = None;
    while !screen.should_quit() {
        sync_watch(screen, &mut watching);
        term.draw(|f| screen.draw(f))?;
        let size = term.size()?;
        let area = Rect::new(0, 0, size.width, size.height);
        if present_frame(gfx, screen, term.backend_mut(), area) {
            continue;
        }
        // Draw the detail header first, then read the files and redraw.
        match screen.load_pending() {
            Ok(true) => continue,
            Ok(false) => {}
            // Not fatal: show git's message and keep running.
            Err(e) => {
                screen.show_dialog(Dialog::Error {
                    title: "Git error".to_string(),
                    message: e.message(),
                });
                continue;
            }
        }
        // Wait for input, a filesystem change, a finished action or a due refresh;
        // all but input redraw.
        let mut changed = false;
        while !changed && !event::poll(INPUT_POLL)? {
            changed = screen.poll_job();
            changed |= screen.tick(Instant::now());
            if let Some((_, rx)) = watching.as_ref().and_then(|w| w.live.as_ref()) {
                for ev in rx.try_iter() {
                    screen.on_watch(ev);
                    changed = true;
                }
            }
        }
        if changed {
            continue;
        }
        let cmd = match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => screen.key_command(key),
            Event::Mouse(m) => {
                let size = term.size()?;
                let area = Rect::new(0, 0, size.width, size.height);
                screen.mouse_command(area, m, &mut clicks, Instant::now())
            }
            Event::Resize(..) => {
                let cell = crossterm::terminal::window_size()
                    .ok()
                    .and_then(|s| cell_px_from_window((s.width, s.height), (s.columns, s.rows)));
                gfx.on_resize(screen, term.backend_mut(), cell);
                None
            }
            _ => None,
        };
        if let Some(cmd) = cmd {
            apply_command(screen, cmd);
        }
    }
    Ok(())
}

/// Maps a key press to the command it means for the app's current input mode.
pub fn key_command(key: KeyEvent, app: &App) -> Option<Command> {
    if app.too_small() {
        return input::too_small_key(key);
    }
    translate_key(key, app.input_mode()).map(|cmd| app.resolve(cmd))
}
