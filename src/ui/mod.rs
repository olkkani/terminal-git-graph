pub mod detail;
pub mod diff;
pub mod header;
pub mod layout;
pub mod table;
pub mod theme;

use ratatui::layout::{Constraint, Layout};
use ratatui::text::Line;
use ratatui::Frame;

use crate::app::App;
use layout::{truncate_to_width, DETAIL_SPLIT_MIN_HEIGHT, MIN_HEIGHT, MIN_WIDTH};

pub(crate) fn draw(app: &App, frame: &mut Frame) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let msg = truncate_to_width("Too small", area.width as usize);
        frame.render_widget(Line::raw(msg), area);
        return;
    }
    let [top, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    header::draw(app, frame, top);
    let Some(view) = &app.detail else {
        table::draw(app, frame, body);
        return;
    };
    let panel = if area.height >= DETAIL_SPLIT_MIN_HEIGHT {
        let [table_area, panel] =
            Layout::vertical([Constraint::Percentage(40), Constraint::Min(0)]).areas(body);
        table::draw(app, frame, table_area);
        panel
    } else {
        body
    };
    match &view.diff {
        Some(d) => diff::draw(app, d, frame, panel),
        None => detail::draw(app, view, frame, panel),
    }
}
