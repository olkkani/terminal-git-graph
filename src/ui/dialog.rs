//! Modal dialogs: confirm, text input and error. While one is open it takes all input.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, Clear};
use ratatui::Frame;
use unicode_segmentation::UnicodeSegmentation;

use crate::app::Command;
use crate::git::validate_name;
use crate::ui::layout::{display_width, printable};

/// Widest a dialog gets, in cells.
const MAX_WIDTH: u16 = 76;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Dialog {
    Confirm {
        title: String,
        body: String,
        /// Drawn in red.
        danger: bool,
    },
    Input {
        title: String,
        prompt: String,
        value: String,
        /// The value names a ref (branch or tag), so git's name rules apply; otherwise
        /// it is free text such as a commit message.
        name: bool,
    },
    Error {
        title: String,
        message: String,
    },
    /// Read-only text, such as the key bindings.
    Info {
        title: String,
        body: String,
    },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DialogOutcome {
    /// Confirmed; an input dialog carries its value.
    Accepted(Option<String>),
    Cancelled,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DialogState {
    pub dialog: Dialog,
    /// Why the last accept was refused (input dialogs only).
    pub error: Option<String>,
}

impl DialogState {
    pub fn new(dialog: Dialog) -> DialogState {
        DialogState {
            dialog,
            error: None,
        }
    }

    /// Applies a dialog command; returns how the dialog closed, or `None` while it stays open.
    pub fn handle(&mut self, cmd: Command) -> Option<DialogOutcome> {
        match cmd {
            Command::DialogAccept => match &self.dialog {
                Dialog::Input { value, name, .. } => match check_value(value, *name) {
                    Ok(()) => Some(DialogOutcome::Accepted(Some(value.clone()))),
                    Err(e) => {
                        self.error = Some(e.to_string());
                        None
                    }
                },
                _ => Some(DialogOutcome::Accepted(None)),
            },
            Command::DialogCancel => Some(DialogOutcome::Cancelled),
            Command::DialogChar(c) => {
                if let Dialog::Input { value, .. } = &mut self.dialog {
                    value.push(c);
                    self.error = None;
                }
                None
            }
            Command::DialogBackspace => {
                if let Dialog::Input { value, .. } = &mut self.dialog {
                    value.pop();
                    self.error = None;
                }
                None
            }
            _ => None,
        }
    }

    pub(crate) fn draw(&self, frame: &mut Frame, screen: Rect) {
        let width = screen.width.saturating_sub(4).min(MAX_WIDTH);
        let inner_w = width.saturating_sub(2) as usize;
        let red = Style::new().fg(Color::Red);
        let (title, danger) = match &self.dialog {
            Dialog::Confirm { title, danger, .. } => (title, *danger),
            Dialog::Input { title, .. } => (title, false),
            Dialog::Error { title, .. } => (title, true),
            Dialog::Info { title, .. } => (title, false),
        };

        let mut lines: Vec<Line> = Vec::new();
        let buttons = match &self.dialog {
            Dialog::Confirm { body, danger, .. } => {
                lines.extend(wrap(body, inner_w).into_iter().map(Line::raw));
                let style = if *danger { red } else { Style::new() };
                Line::from(vec![
                    Span::styled("[Enter] Confirm", style.add_modifier(Modifier::BOLD)),
                    Span::raw("  [Esc] Cancel"),
                ])
            }
            Dialog::Input { prompt, value, .. } => {
                lines.extend(wrap(prompt, inner_w).into_iter().map(Line::raw));
                lines.extend(
                    wrap(&format!("> {value}_"), inner_w)
                        .into_iter()
                        .map(Line::raw),
                );
                if let Some(e) = &self.error {
                    lines.extend(wrap(e, inner_w).into_iter().map(|l| Line::styled(l, red)));
                }
                Line::raw("[Enter] OK  [Esc] Cancel")
            }
            Dialog::Error { message: text, .. } | Dialog::Info { body: text, .. } => {
                lines.extend(wrap(text, inner_w).into_iter().map(Line::raw));
                Line::raw("[Enter] Close")
            }
        };
        lines.push(Line::raw(""));
        lines.push(buttons);

        let height = (lines.len() as u16 + 2).min(screen.height);
        let rect = Rect {
            x: screen.x + (screen.width - width) / 2,
            y: screen.y + (screen.height - height) / 2,
            width,
            height,
        };
        let border = if danger { red } else { Style::new() };
        let block = Block::new()
            .borders(Borders::ALL)
            .border_style(border)
            .title(format!(" {} ", printable(title)));
        let inner = block.inner(rect);
        frame.render_widget(Clear, rect);
        frame.render_widget(block, rect);
        for (n, line) in lines.iter().take(inner.height as usize).enumerate() {
            frame
                .buffer_mut()
                .set_line(inner.x, inner.y + n as u16, line, inner.width);
        }
    }
}

/// Refuses empty values, and ref names git could read as an option or would reject.
fn check_value(value: &str, name: bool) -> Result<(), &'static str> {
    if value.is_empty() {
        Err("A value is required.")
    } else if name && value.starts_with('-') {
        Err("The name cannot start with '-'.")
    } else if name && validate_name(value).is_err() {
        Err("The name cannot contain spaces or control characters.")
    } else {
        Ok(())
    }
}

/// Splits `text` into lines of at most `width` display columns, breaking anywhere so
/// the text is shown as given.
fn wrap(text: &str, width: usize) -> Vec<String> {
    let mut out = Vec::new();
    for source in text.lines() {
        let mut line = String::new();
        let mut used = 0;
        for g in printable(source).graphemes(true) {
            let w = display_width(g);
            if used + w > width && !line.is_empty() {
                out.push(std::mem::take(&mut line));
                used = 0;
            }
            line.push_str(g);
            used += w;
        }
        out.push(line);
    }
    out
}
