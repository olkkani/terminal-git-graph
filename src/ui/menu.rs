//! Context menu: a bordered list of items opened at a click (or centred for the keyboard),
//! moved back on screen when it would not fit, and scrolled when taller than the screen.

use std::cell::Cell;
use std::path::PathBuf;

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear};
use ratatui::Frame;

use crate::actions::Action;
use crate::input::Hit;
use crate::ui::layout::{display_width, fit_to_width};

/// What a menu item does.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuId {
    /// Runs a git action (asking for input or confirmation first when it needs it).
    Do(Action),
    /// Copies the text to the clipboard.
    Copy(String),
    /// Opens the web page at this URL (empty when the remote has none).
    OpenRemote(String),
    /// Switches the graph to the repository at this path (Manygit's header dropdown).
    SwitchRepo(PathBuf),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct MenuItem {
    pub label: String,
    pub enabled: bool,
    /// Drawn in red.
    pub danger: bool,
    pub id: MenuId,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Menu {
    pub items: Vec<MenuItem>,
    /// Index of the highlighted item; `None` when no item is enabled.
    pub selected: Option<usize>,
    /// Top-left corner requested by a right click; `None` centres the menu.
    pub anchor: Option<(u16, u16)>,
    /// The row or label the menu is for.
    pub target: Hit,
    /// First item in view; adjusted while drawing to keep the selection visible.
    offset: Cell<usize>,
}

impl Menu {
    pub fn new(items: Vec<MenuItem>, anchor: Option<(u16, u16)>, target: Hit) -> Menu {
        let selected = items.iter().position(|i| i.enabled);
        Menu {
            items,
            selected,
            anchor,
            target,
            offset: Cell::new(0),
        }
    }

    /// Moves to the previous enabled item, staying put when there is none.
    pub fn up(&mut self) {
        if let Some(cur) = self.selected {
            if let Some(i) = (0..cur).rev().find(|&i| self.items[i].enabled) {
                self.selected = Some(i);
            }
        }
    }

    /// Moves to the next enabled item, staying put when there is none.
    pub fn down(&mut self) {
        if let Some(cur) = self.selected {
            if let Some(i) = (cur + 1..self.items.len()).find(|&i| self.items[i].enabled) {
                self.selected = Some(i);
            }
        }
    }

    /// The highlighted item, which is always enabled.
    pub fn chosen(&self) -> Option<&MenuItem> {
        self.selected.map(|i| &self.items[i])
    }

    /// Where the menu is drawn on `screen`, shifted left/up so it stays inside.
    pub fn rect(&self, screen: Rect) -> Rect {
        let label_w = self
            .items
            .iter()
            .map(|i| display_width(&i.label))
            .max()
            .unwrap_or(0);
        // Borders plus one space of padding on each side.
        let width = (label_w as u16 + 4).min(screen.width);
        let height = (self.items.len() as u16 + 2).min(screen.height);
        let (x, y) = self.anchor.unwrap_or((
            screen.x + (screen.width - width) / 2,
            screen.y + (screen.height - height) / 2,
        ));
        Rect {
            x: x.clamp(screen.x, screen.right() - width),
            y: y.clamp(screen.y, screen.bottom() - height),
            width,
            height,
        }
    }

    /// Indices of the items in view on `screen`, scrolling just enough to keep the
    /// highlighted item visible.
    fn visible(&self, screen: Rect) -> std::ops::Range<usize> {
        let height = self.rect(screen).height.saturating_sub(2) as usize;
        let count = self.items.len();
        let mut offset = self.offset.get().min(count.saturating_sub(height));
        if let Some(sel) = self.selected {
            if sel < offset {
                offset = sel;
            } else if height > 0 && sel >= offset + height {
                offset = sel + 1 - height;
            }
        }
        self.offset.set(offset);
        offset..(offset + height).min(count)
    }

    /// Index of the item at `pos`, if the position is on one.
    pub fn item_at(&self, screen: Rect, pos: (u16, u16)) -> Option<usize> {
        let rect = self.rect(screen);
        let (x, y) = pos;
        if x <= rect.x || x + 1 >= rect.right() || y <= rect.y || y + 1 >= rect.bottom() {
            return None;
        }
        let visible = self.visible(screen);
        let i = visible.start + (y - rect.y - 1) as usize;
        visible.contains(&i).then_some(i)
    }

    pub(crate) fn draw(&self, frame: &mut Frame, screen: Rect) {
        let rect = self.rect(screen);
        let block = Block::new().borders(Borders::ALL);
        let inner = block.inner(rect);
        frame.render_widget(Clear, rect);
        frame.render_widget(block, rect);
        let visible = self.visible(screen);
        // Arrows on the border show that items are hidden above or below.
        let arrow_x = rect.right().saturating_sub(3);
        if visible.start > 0 && rect.width > 4 {
            frame
                .buffer_mut()
                .set_string(arrow_x, rect.y, "▲", Style::new());
        }
        if visible.end < self.items.len() && rect.width > 4 {
            let y = rect.bottom() - 1;
            frame.buffer_mut().set_string(arrow_x, y, "▼", Style::new());
        }
        for (y, n) in (inner.y..).zip(visible) {
            let item = &self.items[n];
            let mut style = Style::new();
            if item.danger {
                style = style.fg(Color::Red);
            }
            if !item.enabled {
                style = style.add_modifier(Modifier::DIM);
            }
            if self.selected == Some(n) {
                style = style.add_modifier(Modifier::REVERSED);
            }
            let text = fit_to_width(&format!(" {}", item.label), inner.width as usize);
            frame
                .buffer_mut()
                .set_line(inner.x, y, &Line::styled(text, style), inner.width);
        }
    }
}
