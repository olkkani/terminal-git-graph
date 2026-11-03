//! Terminal setup/teardown and the event loop.

use std::error::Error;
use std::io::{self, stdout, Stdout};
use std::sync::mpsc::Receiver;
use std::time::{Duration, Instant};

use crossterm::cursor::Show;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyEvent, KeyEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use crate::app::{App, Command};
use crate::input::{mouse_command, translate_key, ClickTracker};
use crate::watch::WatchEvent;

pub type Tui = Terminal<CrosstermBackend<Stdout>>;

/// Enters raw mode, the alternate screen and mouse capture; a panic restores the terminal first.
pub fn enter() -> io::Result<Tui> {
    let previous = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = restore();
        previous(info);
    }));
    enable_raw_mode()?;
    execute!(stdout(), EnterAlternateScreen, EnableMouseCapture)?;
    Terminal::new(CrosstermBackend::new(stdout()))
}

pub fn restore() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(stdout(), DisableMouseCapture, LeaveAlternateScreen, Show)
}

/// How long to wait for input before checking the watch channel again.
const INPUT_POLL: Duration = Duration::from_millis(50);

/// Runs the app until it quits, always restoring the terminal. Filesystem changes
/// arriving on `watch` and finished git actions refresh the view.
pub fn run(app: &mut App, watch: &Receiver<WatchEvent>) -> Result<(), Box<dyn Error>> {
    let mut term = enter()?;
    let result = event_loop(&mut term, app, watch);
    restore()?;
    result
}

fn event_loop(
    term: &mut Tui,
    app: &mut App,
    watch: &Receiver<WatchEvent>,
) -> Result<(), Box<dyn Error>> {
    let mut clicks = ClickTracker::new();
    while !app.should_quit() {
        term.draw(|f| app.draw(f))?;
        // Draw the detail header first, then read the files and redraw.
        if app.load_pending()? {
            continue;
        }
        // Wait for input, a filesystem change or a finished action; the latter two redraw.
        let mut changed = false;
        while !changed && !event::poll(INPUT_POLL)? {
            changed = app.poll_job();
            for ev in watch.try_iter() {
                app.on_watch(ev);
                changed = true;
            }
        }
        if changed {
            continue;
        }
        let cmd = match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => key_command(key, app),
            Event::Mouse(m) => {
                let size = term.size()?;
                let area = Rect::new(0, 0, size.width, size.height);
                mouse_command(app, area, m, &mut clicks, Instant::now())
            }
            _ => None,
        };
        if let Some(cmd) = cmd {
            app.apply(cmd)?;
        }
    }
    Ok(())
}

/// Maps a key press to the command it means for the app's current input mode.
pub fn key_command(key: KeyEvent, app: &App) -> Option<Command> {
    translate_key(key, app.input_mode()).map(|cmd| app.resolve(cmd))
}
