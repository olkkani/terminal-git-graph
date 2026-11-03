//! Context menu: a bordered list of items opened at a click (or centred for the keyboard),
//! moved back on screen when it would not fit.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders, Clear};
use ratatui::Frame;

use crate::input::Hit;
use crate::ui::layout::{display_width, fit_to_width};

/// What a menu item does. Placeholder until the git actions are connected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuId {
    Noop,
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
}

impl Menu {
    pub fn new(items: Vec<MenuItem>, anchor: Option<(u16, u16)>, target: Hit) -> Menu {
        let selected = items.iter().position(|i| i.enabled);
        Menu {
            items,
            selected,
            anchor,
            target,
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

    /// Index of the item at `pos`, if the position is on one.
    pub fn item_at(&self, screen: Rect, pos: (u16, u16)) -> Option<usize> {
        let rect = self.rect(screen);
        let (x, y) = pos;
        if x <= rect.x || x + 1 >= rect.right() || y <= rect.y || y + 1 >= rect.bottom() {
            return None;
        }
        let i = (y - rect.y - 1) as usize;
        (i < self.items.len()).then_some(i)
    }

    pub(crate) fn draw(&self, frame: &mut Frame, screen: Rect) {
        let rect = self.rect(screen);
        let block = Block::new().borders(Borders::ALL);
        let inner = block.inner(rect);
        frame.render_widget(Clear, rect);
        frame.render_widget(block, rect);
        for (n, item) in self.items.iter().enumerate().take(inner.height as usize) {
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
            let y = inner.y + n as u16;
            frame
                .buffer_mut()
                .set_line(inner.x, y, &Line::styled(text, style), inner.width);
        }
    }
}
