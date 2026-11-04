//! File diff view: `+` lines green, `-` lines red, hunk headers highlighted.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::Line;
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use crate::app::{App, DiffView};
use crate::ui::layout::{printable, truncate_to_width};

pub(crate) fn draw(app: &App, diff: &DiffView, frame: &mut Frame, area: Rect) {
    let title = truncate_to_width(
        &printable(&diff.path),
        (area.width as usize).saturating_sub(2),
    );
    let block = Block::new()
        .borders(Borders::TOP)
        .title(format!(" {title} "));
    let inner = block.inner(area);
    frame.render_widget(block, area);
    app.diff_height.set(inner.height as usize);
    if diff.lines.is_empty() {
        frame.render_widget(Line::raw("No changes"), inner);
        return;
    }
    let visible = diff
        .lines
        .iter()
        .skip(diff.scroll)
        .take(inner.height as usize);
    for (n, text) in visible.enumerate() {
        let line = Line::styled(printable(text), line_style(text));
        frame
            .buffer_mut()
            .set_line(inner.x, inner.y + n as u16, &line, inner.width);
    }
}

fn line_style(text: &str) -> Style {
    if text.starts_with("@@") {
        Style::new().fg(Color::Cyan).add_modifier(Modifier::BOLD)
    } else if text.starts_with('+') {
        Style::new().fg(Color::Green)
    } else if text.starts_with('-') {
        Style::new().fg(Color::Red)
    } else {
        Style::new()
    }
}
