//! Terminal setup/teardown and the event loop.

use std::error::Error;
use std::io::{self, stdout, Stdout};

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

use crate::app::{App, Command};

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

/// Runs the app until it quits, always restoring the terminal.
pub fn run(app: &mut App) -> Result<(), Box<dyn Error>> {
    let mut term = enter()?;
    let result = event_loop(&mut term, app);
    restore()?;
    result
}

fn event_loop(term: &mut Tui, app: &mut App) -> Result<(), Box<dyn Error>> {
    while !app.should_quit() {
        term.draw(|f| app.draw(f))?;
        let cmd = match event::read()? {
            Event::Key(key) if key.kind == KeyEventKind::Press => key_command(key),
            Event::Mouse(m) => match m.kind {
                MouseEventKind::ScrollUp => Some(Command::Up),
                MouseEventKind::ScrollDown => Some(Command::Down),
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

fn key_command(key: KeyEvent) -> Option<Command> {
    if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('c') {
        return Some(Command::Quit);
    }
    Some(match key.code {
        KeyCode::Char('q') | KeyCode::Esc => Command::Quit,
        KeyCode::Up | KeyCode::Char('k') => Command::Up,
        KeyCode::Down | KeyCode::Char('j') => Command::Down,
        KeyCode::PageUp => Command::PageUp,
        KeyCode::PageDown => Command::PageDown,
        KeyCode::Home | KeyCode::Char('g') => Command::Top,
        KeyCode::End | KeyCode::Char('G') => Command::Bottom,
        KeyCode::Char('a') => Command::ToggleScope,
        KeyCode::Char('r') => Command::ToggleRemotes,
        KeyCode::F(5) | KeyCode::Char('R') => Command::Refresh,
        _ => return None,
    })
}
