//! Detail panel: message, full hash, parents, author, date and the changed-file list.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders};
use ratatui::Frame;

use crate::app::{App, DetailData, DetailTarget, DetailView};
use crate::git::detail::FileChange;
use crate::ui::layout::{display_width, printable, truncate_to_width};
use crate::ui::theme;

const SHORT_HASH: usize = 7;

pub(crate) fn draw(app: &App, view: &DetailView, frame: &mut Frame, area: Rect) {
    let block = block(view);
    let inner = block.inner(area);
    frame.render_widget(block, area);

    let mut lines = header_lines(app, view);
    let mut selected_line = None;
    match view.files() {
        None => lines.push(Line::raw("Loading…")),
        Some(files) => {
            lines.push(Line::styled(
                format!("Files ({})", files.len()),
                Style::new().add_modifier(Modifier::BOLD),
            ));
            for (i, file) in files.iter().enumerate() {
                let mut line = file_line(file, inner.width as usize);
                if i == view.file {
                    selected_line = Some(lines.len());
                    if view.files_focused {
                        line = line.style(Style::new().bg(theme::selected_bg(app.depth)));
                    }
                }
                lines.push(line);
            }
        }
    }

    let height = inner.height as usize;
    let offset = scroll_offset(selected_line, height);
    for (n, line) in lines.iter().skip(offset).take(height).enumerate() {
        let y = inner.y + n as u16;
        let row = Rect {
            y,
            height: 1,
            ..inner
        };
        frame.buffer_mut().set_style(row, line.style);
        frame.buffer_mut().set_line(inner.x, y, line, inner.width);
    }
}

/// Index of the file listed at `pos` in the detail drawn in `area`.
pub(crate) fn file_at(app: &App, view: &DetailView, area: Rect, pos: (u16, u16)) -> Option<usize> {
    let inner = block(view).inner(area);
    let (x, y) = pos;
    if x < inner.x || x >= inner.right() || y < inner.y || y >= inner.bottom() {
        return None;
    }
    let files = view.files()?;
    // The file list follows the header lines and the "Files (n)" line.
    let first = header_lines(app, view).len() + 1;
    let offset = scroll_offset(
        (!files.is_empty()).then_some(first + view.file),
        inner.height as usize,
    );
    let i = (offset + (y - inner.y) as usize).checked_sub(first)?;
    (i < files.len()).then_some(i)
}

fn block(view: &DetailView) -> Block<'static> {
    let title = match &view.target {
        DetailTarget::Commit(hash) => format!(" Commit {} ", short(hash)),
        DetailTarget::Uncommitted => " Uncommitted changes ".to_string(),
    };
    Block::new().borders(Borders::TOP).title(title)
}

/// First line shown: scrolled just enough to keep the selected file's line in view.
fn scroll_offset(selected_line: Option<usize>, height: usize) -> usize {
    selected_line.map_or(0, |l| (l + 1).saturating_sub(height))
}

fn header_lines(app: &App, view: &DetailView) -> Vec<Line<'static>> {
    let bold = Style::new().add_modifier(Modifier::BOLD);
    let muted = Style::new().fg(theme::muted(app.depth));
    let field = |label: &str, value: String| {
        Line::from(vec![
            Span::styled(format!("{label:<8}"), muted),
            Span::raw(printable(&value)),
        ])
    };
    let mut lines = Vec::new();
    match (&view.target, &view.data) {
        (DetailTarget::Commit(_), DetailData::Commit(d)) => {
            let mut message = d.message.lines();
            lines.push(Line::styled(printable(message.next().unwrap_or("")), bold));
            lines.extend(message.map(|l| Line::raw(printable(l))));
            lines.push(Line::raw(""));
            lines.push(field("Hash", d.hash.clone()));
            let parents = if d.parents.is_empty() {
                "(none)".to_string()
            } else {
                d.parents
                    .iter()
                    .map(|p| short(p))
                    .collect::<Vec<_>>()
                    .join(" ")
            };
            lines.push(field("Parents", parents));
            lines.push(field("Author", format!("{} <{}>", d.author, d.email)));
            lines.push(field("Date", utc_datetime(d.timestamp)));
            lines.push(Line::raw(""));
        }
        (DetailTarget::Commit(hash), _) => {
            let subject = app
                .commits
                .iter()
                .find(|c| &c.hash == hash)
                .map_or("", |c| c.subject.as_str());
            lines.push(Line::styled(printable(subject), bold));
            lines.push(Line::raw(""));
            lines.push(field("Hash", hash.clone()));
            lines.push(Line::raw(""));
        }
        (DetailTarget::Uncommitted, _) => {}
    }
    lines
}

/// `M +2 -0 path`; counts are left out when unknown (binary or untracked files).
fn file_line(file: &FileChange, width: usize) -> Line<'static> {
    let mut spans = vec![Span::raw(format!("{} ", file.status))];
    if let (Some(added), Some(deleted)) = (file.added, file.deleted) {
        spans.push(Span::styled(
            format!("+{added}"),
            Style::new().fg(Color::Green),
        ));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(
            format!("-{deleted}"),
            Style::new().fg(Color::Red),
        ));
        spans.push(Span::raw(" "));
    }
    let used: usize = spans.iter().map(|s| display_width(&s.content)).sum();
    spans.push(Span::raw(truncate_to_width(
        &printable(&file.path),
        width.saturating_sub(used),
    )));
    Line::from(spans)
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(SHORT_HASH)]
}

/// `YYYY-MM-DD HH:MM:SS UTC` for Unix seconds `ts`.
fn utc_datetime(ts: i64) -> String {
    let days = ts.div_euclid(86_400);
    let secs = ts.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02} {:02}:{:02}:{:02} UTC",
        secs / 3600,
        secs % 3600 / 60,
        secs % 60
    )
}

/// Proleptic Gregorian date for a day count since 1970-01-01 (H. Hinnant's algorithm).
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z.rem_euclid(146_097);
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}
