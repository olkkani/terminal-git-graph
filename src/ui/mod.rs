pub mod header;
pub mod layout;
pub mod table;
pub mod theme;

use ratatui::layout::{Constraint, Layout};
use ratatui::text::Line;
use ratatui::Frame;

use crate::app::App;
use layout::{truncate_to_width, MIN_HEIGHT, MIN_WIDTH};

pub(crate) fn draw(app: &App, frame: &mut Frame) {
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        let msg = truncate_to_width("Too small", area.width as usize);
        frame.render_widget(Line::raw(msg), area);
        return;
    }
    let [top, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
    header::draw(app, frame, top);
    table::draw(app, frame, body);
}
