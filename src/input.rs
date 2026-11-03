//! Turns key presses and mouse events into commands for the current input mode.

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;

use crate::app::{App, Command};
use crate::ui;
use crate::ui::menu::Menu;

/// Which part of the UI takes input.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputMode {
    /// The commit table.
    Graph,
    /// The detail's file list.
    Detail,
    Diff,
    Menu,
    /// A modal dialog: nothing else gets input.
    Dialog,
    /// The Manygit repository list.
    List,
}

/// Two clicks on the same cell within this many milliseconds are a double click.
pub const DOUBLE_CLICK_MS: u64 = 400;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ClickKind {
    Single,
    Double,
}

/// Tells single clicks from double clicks.
#[derive(Debug, Default)]
pub struct ClickTracker {
    /// The last single click, waiting for a second one.
    last: Option<((u16, u16), Instant)>,
}

impl ClickTracker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records a click; the second of two quick clicks on one cell is a double click,
    /// and the click after that starts over.
    pub fn click(&mut self, pos: (u16, u16), now: Instant) -> ClickKind {
        let window = Duration::from_millis(DOUBLE_CLICK_MS);
        match self.last.take() {
            Some((p, t)) if p == pos && now.saturating_duration_since(t) <= window => {
                ClickKind::Double
            }
            _ => {
                self.last = Some((pos, now));
                ClickKind::Single
            }
        }
    }
}

/// What a screen position points at.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hit {
    /// A table row (the uncommitted row, when shown, is 0).
    Row(usize),
    /// The `index`-th ref label of a row, in `Refs::labels_for` order.
    Label {
        row: usize,
        index: usize,
    },
    /// A file in the detail's file list.
    File(usize),
    Toolbar(ToolbarButton),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolbarButton {
    /// All ↔ Current.
    Scope,
    Remotes,
    Fetch,
    /// "‹ Manygit": back to the repository list.
    Back,
    /// The repository name: the dropdown of other repositories.
    RepoName,
}

/// Maps a key press to a command in `mode`.
pub fn translate_key(key: KeyEvent, mode: InputMode) -> Option<Command> {
    if key.modifiers.contains(KeyModifiers::CONTROL) {
        return (key.code == KeyCode::Char('c')).then_some(Command::Quit);
    }
    match mode {
        InputMode::Dialog => {
            return match key.code {
                KeyCode::Enter => Some(Command::DialogAccept),
                KeyCode::Esc => Some(Command::DialogCancel),
                KeyCode::Backspace => Some(Command::DialogBackspace),
                KeyCode::Char(c) => Some(Command::DialogChar(c)),
                _ => None,
            };
        }
        InputMode::Menu => {
            return match key.code {
                KeyCode::Up | KeyCode::Char('k') => Some(Command::MenuUp),
                KeyCode::Down | KeyCode::Char('j') => Some(Command::MenuDown),
                KeyCode::Enter => Some(Command::MenuSelect),
                KeyCode::Esc | KeyCode::Char('m') | KeyCode::Char('q') => Some(Command::MenuClose),
                _ => None,
            };
        }
        InputMode::List if key.code == KeyCode::Char('F') => return Some(Command::FetchAllRepos),
        InputMode::Graph | InputMode::Detail | InputMode::Diff | InputMode::List => {}
    }
    // Movement depends on the focused view; [graph, file list, diff].
    let by_mode = |[graph, detail, diff]: [Command; 3]| match mode {
        InputMode::Detail => detail,
        InputMode::Diff => diff,
        _ => graph,
    };
    Some(match key.code {
        KeyCode::Char('q') => Command::Quit,
        KeyCode::Esc => by_mode([Command::Back, Command::CloseDetail, Command::CloseDiff]),
        KeyCode::Enter => match mode {
            InputMode::Detail => Command::OpenDiff,
            InputMode::Diff => return None,
            _ => Command::Activate,
        },
        KeyCode::Tab => Command::FocusFiles,
        KeyCode::Left | KeyCode::Char('h') if mode == InputMode::Graph => Command::LabelPrev,
        KeyCode::Right | KeyCode::Char('l') if mode == InputMode::Graph => Command::LabelNext,
        KeyCode::Up | KeyCode::Char('k') => {
            by_mode([Command::Up, Command::FileUp, Command::DiffScrollUp])
        }
        KeyCode::Down | KeyCode::Char('j') => {
            by_mode([Command::Down, Command::FileDown, Command::DiffScrollDown])
        }
        KeyCode::PageUp => by_mode([Command::PageUp, Command::FileTop, Command::DiffPageUp]),
        KeyCode::PageDown => by_mode([
            Command::PageDown,
            Command::FileBottom,
            Command::DiffPageDown,
        ]),
        KeyCode::Home | KeyCode::Char('g') => {
            by_mode([Command::Top, Command::FileTop, Command::DiffTop])
        }
        KeyCode::End | KeyCode::Char('G') => {
            by_mode([Command::Bottom, Command::FileBottom, Command::DiffBottom])
        }
        KeyCode::Char('m') => Command::OpenMenu,
        KeyCode::Char('a') => Command::ToggleScope,
        KeyCode::Char('r') => Command::ToggleRemotes,
        KeyCode::Char('f') => Command::Fetch,
        KeyCode::Char('s') if mode == InputMode::Graph => Command::RepoMenu,
        KeyCode::Char('?') => Command::Help,
        KeyCode::F(5) | KeyCode::Char('R') => Command::Refresh,
        _ => return None,
    })
}

/// What is at `pos` on a screen of size `area`, as last drawn.
pub fn hit_test(app: &App, area: Rect, pos: (u16, u16)) -> Option<Hit> {
    let areas = ui::areas(app, area)?;
    let inside = |r: Rect| r.contains(ratatui::layout::Position::new(pos.0, pos.1));
    if inside(areas.header) {
        return ui::header::button_at(app, areas.header, pos.0).map(Hit::Toolbar);
    }
    if let Some(table) = areas.table.filter(|r| inside(*r)) {
        return ui::table::hit(app, table, pos);
    }
    let panel = areas.panel.filter(|r| inside(*r))?;
    let view = app.detail.as_ref().filter(|v| v.diff.is_none())?;
    ui::detail::file_at(app, view, panel, pos).map(Hit::File)
}

/// Maps a mouse event on a screen of size `area` to a command: click selects, double
/// click activates, right click opens the menu, the wheel moves. Dialogs ignore the mouse.
pub fn mouse_command(
    app: &App,
    area: Rect,
    ev: MouseEvent,
    clicks: &mut ClickTracker,
    now: Instant,
) -> Option<Command> {
    let pos = (ev.column, ev.row);
    match app.input_mode() {
        InputMode::Dialog | InputMode::List => None,
        InputMode::Menu => menu_mouse(app.menu.as_ref()?, area, ev),
        mode => match ev.kind {
            MouseEventKind::ScrollUp => translate_key(KeyEvent::from(KeyCode::Up), mode),
            MouseEventKind::ScrollDown => translate_key(KeyEvent::from(KeyCode::Down), mode),
            MouseEventKind::Down(MouseButton::Left) => {
                let kind = clicks.click(pos, now);
                Some(match (hit_test(app, area, pos)?, kind) {
                    (Hit::Row(row) | Hit::Label { row, .. }, ClickKind::Single) => {
                        Command::SelectRow(row)
                    }
                    (Hit::Row(_), ClickKind::Double) => Command::Activate,
                    (Hit::Label { row, index }, ClickKind::Double) => {
                        Command::ActivateLabel { row, index }
                    }
                    (Hit::File(i), ClickKind::Single) => Command::SelectFile(i),
                    (Hit::File(_), ClickKind::Double) => Command::OpenDiff,
                    (Hit::Toolbar(ToolbarButton::Scope), _) => Command::ToggleScope,
                    (Hit::Toolbar(ToolbarButton::Remotes), _) => Command::ToggleRemotes,
                    (Hit::Toolbar(ToolbarButton::Fetch), _) => Command::Fetch,
                    (Hit::Toolbar(ToolbarButton::Back), _) => Command::BackToList,
                    (Hit::Toolbar(ToolbarButton::RepoName), _) => Command::RepoMenu,
                })
            }
            MouseEventKind::Down(MouseButton::Right) => match hit_test(app, area, pos)? {
                hit @ (Hit::Row(_) | Hit::Label { .. }) => Some(Command::OpenMenuAt { pos, hit }),
                _ => None,
            },
            _ => None,
        },
    }
}

/// A click with a menu open: picks the item under it, or closes the menu when outside.
pub(crate) fn menu_mouse(menu: &Menu, area: Rect, ev: MouseEvent) -> Option<Command> {
    let pos = (ev.column, ev.row);
    match ev.kind {
        MouseEventKind::Down(MouseButton::Left) => {
            if let Some(i) = menu.item_at(area, pos) {
                Some(Command::MenuPick(i))
            } else if menu
                .rect(area)
                .contains(ratatui::layout::Position::new(pos.0, pos.1))
            {
                None
            } else {
                Some(Command::MenuClose)
            }
        }
        MouseEventKind::Down(_) => Some(Command::MenuClose),
        _ => None,
    }
}
