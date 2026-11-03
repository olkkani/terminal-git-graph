//! Terminal setup/teardown and the event loop.

use std::error::Error;
use std::io::{self, stdout, Stdout};
use std::sync::mpsc::Receiver;
use std::time::Duration;

use crossterm::cursor::Show;
use crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers, MouseEventKind,
};
use crossterm::execute;
use crossterm::terminal::{
    disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
};
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use crate::app::{App, Command, Focus};
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
/// arriving on `watch` refresh the view.
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
    while !app.should_quit() {
        term.draw(|f| app.draw(f))?;
        // Draw the detail header first, then read the files and redraw.
        if app.load_pending()? {
            continue;
        }
        // Wait for either input or a filesystem change; a change redraws.
        let mut changed = false;
        while !changed && !event::poll(INPUT_POLL)? {
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
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp => Some(up(app)),
                MouseEventKind::ScrollDown => Some(down(app)),
                _ => None,
            },
            _ => None,
        };
        if let Some(cmd) = cmd {
            app.apply(cmd)?;
        }
    }
    Ok(())
}

fn up(app: &App) -> Command {
    match app.focus() {
        Focus::Table => Command::Up,
        Focus::Files => Command::FileUp,
        Focus::Diff => Command::DiffScrollUp,
    }
}

fn down(app: &App) -> Command {
    match app.focus() {
        Focus::Table => Command::Down,
        Focus::Files => Command::FileDown,
        Focus::Diff => Command::DiffScrollDown,
    }
}

/// Maps a key press to a command; arrows, paging keys, Enter and Esc depend on the
/// focused view.
pub fn key_command(key: KeyEvent, app: &App) -> Option<Command> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Command::Quit);
    }
    Some(match key.code {
        KeyCode::Char('q') => Command::Quit,
        KeyCode::Esc => match app.focus() {
            Focus::Diff => Command::CloseDiff,
            _ if app.detail_open() => Command::CloseDetail,
            _ => Command::Quit,
        },
        KeyCode::Enter => match app.focus() {
            Focus::Table if app.detail_open() => Command::FocusFiles,
            Focus::Table => Command::OpenDetail,
            Focus::Files => Command::OpenDiff,
            Focus::Diff => return None,
        },
        KeyCode::Tab if app.detail_open() => Command::FocusFiles,
        KeyCode::Up | KeyCode::Char('k') => up(app),
        KeyCode::Down | KeyCode::Char('j') => down(app),
        KeyCode::PageUp => match app.focus() {
            Focus::Table => Command::PageUp,
            Focus::Files => Command::FileTop,
            Focus::Diff => Command::DiffPageUp,
        },
        KeyCode::PageDown => match app.focus() {
            Focus::Table => Command::PageDown,
            Focus::Files => Command::FileBottom,
            Focus::Diff => Command::DiffPageDown,
        },
        KeyCode::Home | KeyCode::Char('g') => match app.focus() {
            Focus::Table => Command::Top,
            Focus::Files => Command::FileTop,
            Focus::Diff => Command::DiffTop,
        },
        KeyCode::End | KeyCode::Char('G') => match app.focus() {
            Focus::Table => Command::Bottom,
            Focus::Files => Command::FileBottom,
            Focus::Diff => Command::DiffBottom,
        },
        KeyCode::Char('a') => Command::ToggleScope,
        KeyCode::Char('r') => Command::ToggleRemotes,
        KeyCode::F(5) | KeyCode::Char('R') => Command::Refresh,
        _ => return None,
    })
}
