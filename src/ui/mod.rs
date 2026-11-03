pub mod detail;
pub mod dialog;
pub mod diff;
pub mod header;
pub mod layout;
pub mod menu;
pub mod table;
pub mod theme;

use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::Line;
use ratatui::Frame;

use crate::app::App;
use layout::{truncate_to_width, DETAIL_SPLIT_MIN_HEIGHT, MIN_HEIGHT, MIN_WIDTH, TOO_SMALL};

/// Where each part of the screen goes.
pub(crate) struct Areas {
    pub(crate) header: Rect,
    /// Hidden while a short terminal shows the detail full screen.
    pub(crate) table: Option<Rect>,
    /// The detail or diff, when open.
    pub(crate) panel: Option<Rect>,
}

/// Splits the screen; `None` when it is too small to draw.
pub(crate) fn areas(app: &App, area: Rect) -> Option<Areas> {
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        return None;
    }
    let [header, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    let (table, panel) = if app.detail.is_none() {
        (Some(body), None)
    } else if area.height >= DETAIL_SPLIT_MIN_HEIGHT {
        let [table, panel] =
            Layout::vertical([Constraint::Percentage(40), Constraint::Min(0)]).areas(body);
        (Some(table), Some(panel))
    } else {
        (None, Some(body))
    };
    Some(Areas {
        header,
        table,
        panel,
    })
}

pub(crate) fn draw(app: &App, frame: &mut Frame) {
    let area = frame.area();
    let areas = areas(app, area);
    app.too_small.set(areas.is_none());
    let Some(areas) = areas else {
        let msg = truncate_to_width(TOO_SMALL, area.width as usize);
        frame.render_widget(Line::raw(msg), area);
        return;
    };
    header::draw(app, frame, areas.header);
    if let Some(table_area) = areas.table {
        table::draw(app, frame, table_area);
    }
    if let (Some(view), Some(panel)) = (&app.detail, areas.panel) {
        match &view.diff {
            Some(d) => diff::draw(app, d, frame, panel),
            None => detail::draw(app, view, frame, panel),
        }
    }
    if let Some(menu) = &app.menu {
        menu.draw(frame, area);
    }
    if let Some(dialog) = &app.dialog {
        dialog.draw(frame, area);
    }
}
