//! The graph table: `[graph][labels][subject][date][author][hash]`, one row per commit.

use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::{App, Row};
use crate::git::refs::{Head, RefKind, RefLabel};
use crate::graph::render::{render_row, render_uncommitted, Cell, RenderOpts};
use crate::input::Hit;
use crate::ui::glyphs::{badge_glyphs, BadgeGlyphs};
use crate::ui::layout::{columns_for, display_width, fit_to_width, truncate_to_width};
use crate::ui::theme;

const DATE_W: usize = 4;
const AUTHOR_W: usize = 12;
const HASH_W: usize = 7;
/// The graph column is at least as wide as its heading.
const GRAPH_LABEL: &str = "Graph";

/// Short age of `ts` relative to `now` (both Unix seconds): "now", "5m", "3h", "10d", "2mo", "1y".
pub fn relative_time(now: i64, ts: i64) -> String {
    const MINUTE: i64 = 60;
    const HOUR: i64 = 60 * MINUTE;
    const DAY: i64 = 24 * HOUR;
    const MONTH: i64 = 30 * DAY;
    const YEAR: i64 = 365 * DAY;
    let s = (now - ts).max(0);
    match s {
        _ if s < MINUTE => "now".to_string(),
        _ if s < HOUR => format!("{}m", s / MINUTE),
        _ if s < DAY => format!("{}h", s / HOUR),
        _ if s < MONTH => format!("{}d", s / DAY),
        _ if s < YEAR => format!("{}mo", s / MONTH),
        _ => format!("{}y", s / YEAR),
    }
}

pub(crate) fn draw(app: &App, frame: &mut Frame, area: Rect) {
    if app.row_count() == 0 {
        frame.render_widget(
            Line::raw(truncate_to_width("No commits yet", area.width as usize)),
            area,
        );
        return;
    }
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64);
    let head = app.head_hash();
    let cols = columns_for(area.width);
    let RowsLayout {
        visible,
        graphs,
        graph_w,
        desc_w,
    } = rows_layout(app, area);
    column_header(app, frame, area, graph_w, desc_w);

    for (n, (i, cells)) in visible.zip(graphs).enumerate() {
        let y = area.y + 1 + n as u16;
        let row_area = Rect {
            y,
            height: 1,
            ..area
        };
        let mut spans = graph_spans(&cells, graph_w, app);
        spans.push(Span::raw(" "));
        let mut bg = None;
        match app.row(i) {
            Some(Row::Commit(c)) => {
                let commit = &app.commits[c];
                let g = &app.graph[c];
                if Some(commit.hash.as_str()) == head {
                    bg = Some(theme::head_bg(app.depth));
                }
                let labels = app.refs.labels_for(&commit.hash);
                let focus = (i == app.selected).then(|| app.focused_label()).flatten();
                let (badges, used, _) = badge_spans(app, &labels, g.color, desc_w / 2, focus);
                spans.extend(badges);
                let subject_style = if g.is_merge {
                    Style::new().add_modifier(Modifier::DIM)
                } else {
                    Style::new()
                };
                spans.push(Span::styled(
                    fit_to_width(&commit.subject, desc_w - used),
                    subject_style,
                ));
                spans.push(Span::raw(" "));
                let muted = Style::new().fg(theme::muted(app.depth));
                spans.push(Span::styled(
                    fit_to_width(&relative_time(now, commit.timestamp), DATE_W),
                    muted,
                ));
                if cols.author {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(fit_to_width(&commit.author, AUTHOR_W), muted));
                }
                if cols.hash {
                    spans.push(Span::raw(" "));
                    spans.push(Span::styled(
                        fit_to_width(&commit.hash[..commit.hash.len().min(HASH_W)], HASH_W),
                        muted,
                    ));
                }
            }
            _ => {
                let total = app.status.changed_total();
                let untracked = app.status.untracked;
                let text = format!(
                    "Uncommitted changes ({total}: {} tracked, {untracked} untracked)",
                    total - untracked
                );
                spans.push(Span::styled(
                    fit_to_width(&text, desc_w),
                    Style::new().add_modifier(Modifier::ITALIC),
                ));
            }
        }
        if i == app.selected {
            bg = Some(theme::selected_bg(app.depth));
        }
        let style = bg.map_or(Style::new(), |c| Style::new().bg(c));
        frame.buffer_mut().set_style(row_area, style);
        frame
            .buffer_mut()
            .set_line(area.x, y, &Line::from(spans).style(style), area.width);
    }
}

/// The dim first line naming the columns in view, each at its column's position.
fn column_header(app: &App, frame: &mut Frame, area: Rect, graph_w: usize, desc_w: usize) {
    let cols = columns_for(area.width);
    let mut text = fit_to_width(GRAPH_LABEL, graph_w);
    text.push(' ');
    text.push_str(&fit_to_width("Description", desc_w));
    text.push(' ');
    text.push_str(&fit_to_width("Date", DATE_W));
    if cols.author {
        text.push(' ');
        text.push_str(&fit_to_width("Author", AUTHOR_W));
    }
    if cols.hash {
        text.push(' ');
        text.push_str(&fit_to_width("Commit", HASH_W));
    }
    let style = Style::new()
        .fg(theme::muted(app.depth))
        .add_modifier(Modifier::DIM);
    frame
        .buffer_mut()
        .set_line(area.x, area.y, &Line::styled(text, style), area.width);
}

/// Rows in view (below the column header) and the column widths they are drawn with.
struct RowsLayout {
    visible: std::ops::Range<usize>,
    graphs: Vec<Vec<Cell>>,
    graph_w: usize,
    /// Width shared by the labels and the subject.
    desc_w: usize,
}

fn rows_layout(app: &App, area: Rect) -> RowsLayout {
    let visible = visible_rows(app, area.height.saturating_sub(1) as usize);
    let head = app.head_hash();
    let head_lane = app
        .commits
        .iter()
        .position(|c| Some(c.hash.as_str()) == head)
        .map_or(0, |i| app.graph[i].lane);

    let graphs: Vec<Vec<Cell>> = visible
        .clone()
        .map(|i| match app.row(i) {
            Some(Row::Commit(c)) => render_row(
                &app.graph[c],
                &RenderOpts {
                    is_head: Some(app.commits[c].hash.as_str()) == head,
                    ascii: app.ascii,
                },
            ),
            _ => render_uncommitted(head_lane, head_lane + 1, app.ascii),
        })
        .collect();

    let width = area.width as usize;
    let cols = columns_for(area.width);
    let graph_w = graphs
        .iter()
        .map(Vec::len)
        .max()
        .unwrap_or(0)
        .max(GRAPH_LABEL.len())
        .min(width / 3);
    let fixed = graph_w
        + 1
        + 1
        + DATE_W
        + if cols.author { 1 + AUTHOR_W } else { 0 }
        + if cols.hash { 1 + HASH_W } else { 0 };
    let desc_w = width.saturating_sub(fixed);

    RowsLayout {
        visible,
        graphs,
        graph_w,
        desc_w,
    }
}

/// What is at `pos` in the table drawn in `area`: a ref label or else the row. The
/// column header line hits nothing.
pub(crate) fn hit(app: &App, area: Rect, pos: (u16, u16)) -> Option<Hit> {
    let (x, y) = pos;
    if x < area.x || x >= area.right() || y <= area.y || y >= area.bottom() {
        return None;
    }
    let layout = rows_layout(app, area);
    let i = layout.visible.start + (y - area.y - 1) as usize;
    if !layout.visible.contains(&i) {
        return None;
    }
    if let Some(Row::Commit(c)) = app.row(i) {
        let labels = app.refs.labels_for(&app.commits[c].hash);
        let (_, _, places) = badge_spans(app, &labels, app.graph[c].color, layout.desc_w / 2, None);
        // Badges start after the graph and one space.
        let col = (x - area.x) as usize;
        let start = layout.graph_w + 1;
        for (index, cells) in places {
            if (start + cells.start..start + cells.end).contains(&col) {
                return Some(Hit::Label { row: i, index });
            }
        }
    }
    Some(Hit::Row(i))
}

/// Indices of the rows in view, scrolling just enough to keep the selection visible.
fn visible_rows(app: &App, height: usize) -> std::ops::Range<usize> {
    app.page_height.set(height);
    let count = app.row_count();
    let mut offset = app.offset.get().min(count.saturating_sub(height));
    if app.selected < offset {
        offset = app.selected;
    } else if height > 0 && app.selected >= offset + height {
        offset = app.selected + 1 - height;
    }
    app.offset.set(offset);
    offset..(offset + height).min(count)
}

fn graph_spans(cells: &[Cell], width: usize, app: &App) -> Vec<Span<'static>> {
    let mut spans: Vec<Span> = cells
        .iter()
        .take(width)
        .map(|c| {
            let mut style = Style::new();
            if let Some(color) = c.color {
                style = style.fg(theme::lane_color(color, app.depth));
            }
            if c.dim {
                style = style.add_modifier(Modifier::DIM);
            }
            Span::styled(c.ch.to_string(), style)
        })
        .collect();
    let pad = width - spans.len().min(width);
    spans.push(Span::raw(" ".repeat(pad)));
    spans
}

fn label_text(app: &App, label: &RefLabel, glyphs: &BadgeGlyphs) -> String {
    match label.kind {
        RefKind::LocalBranch => {
            let checked = matches!(&app.refs.head, Head::Branch(b) if *b == label.name);
            let check = if checked { glyphs.check } else { "" };
            let mut text = format!("{check}{}", label.name);
            if app.remotes {
                for r in &label.remotes {
                    text.push_str(" | ");
                    text.push_str(r);
                }
            }
            text
        }
        RefKind::Head | RefKind::RemoteBranch | RefKind::Tag => label.name.clone(),
    }
}

/// Places of the drawn badges: index into the row's labels and the cells it covers.
type BadgePlaces = Vec<(usize, std::ops::Range<usize>)>;

/// Badge spans within `budget` columns (cut with '…' when they don't fit), their width
/// and where each badge sits relative to the first. The `focus`-th label is highlighted.
fn badge_spans(
    app: &App,
    labels: &[&RefLabel],
    color: usize,
    budget: usize,
    focus: Option<usize>,
) -> (Vec<Span<'static>>, usize, BadgePlaces) {
    let glyphs = badge_glyphs(app.nerd_font, app.ascii);
    let lane = theme::lane_color(color, app.depth);
    let mut spans = Vec::new();
    let mut used = 0;
    let mut places = Vec::new();
    for (index, label) in labels.iter().enumerate() {
        if label.kind == RefKind::RemoteBranch && !app.remotes {
            continue;
        }
        let checked = match label.kind {
            RefKind::Head => true,
            RefKind::LocalBranch => matches!(&app.refs.head, Head::Branch(b) if *b == label.name),
            _ => false,
        };
        let style = match label.kind {
            RefKind::Head | RefKind::LocalBranch if checked => Style::new()
                .fg(Color::Black)
                .bg(lane)
                .add_modifier(Modifier::BOLD),
            RefKind::Head | RefKind::LocalBranch => Style::new()
                .fg(Color::Black)
                .bg(theme::lane_light(color, app.depth)),
            RefKind::RemoteBranch => Style::new()
                .fg(lane)
                .bg(theme::lane_dark(color, app.depth))
                .add_modifier(Modifier::BOLD),
            RefKind::Tag => Style::new()
                .fg(theme::muted(app.depth))
                .bg(theme::muted_dark(app.depth)),
        };
        let style = if focus == Some(index) {
            style.add_modifier(Modifier::REVERSED | Modifier::UNDERLINED)
        } else {
            style
        };
        let icon = match label.kind {
            RefKind::Head | RefKind::LocalBranch => glyphs.local,
            RefKind::RemoteBranch => glyphs.remote,
            RefKind::Tag => glyphs.tag,
        };
        let icon = if icon.is_empty() {
            String::new()
        } else {
            format!("{icon} ")
        };
        // Round caps replace the padding space on each side.
        let (left, right) = if glyphs.left_cap.is_empty() {
            (" ", " ")
        } else {
            ("", "")
        };
        let body = format!("{left}{icon}{}{right}", label_text(app, label, &glyphs));
        let caps = display_width(glyphs.left_cap) + display_width(glyphs.right_cap);
        let w = display_width(&body) + caps;
        let remaining = budget - used;
        if w < remaining {
            places.push((index, used..used + w));
            if !glyphs.left_cap.is_empty() {
                spans.push(Span::styled(glyphs.left_cap, cap_style(&style)));
            }
            spans.push(Span::styled(body, style));
            if !glyphs.right_cap.is_empty() {
                spans.push(Span::styled(glyphs.right_cap, cap_style(&style)));
            }
            spans.push(Span::raw(" "));
            used += w + 1;
        } else {
            let cut = truncate_to_width(&body, remaining.saturating_sub(1));
            if !cut.is_empty() {
                let w = display_width(&cut);
                places.push((index, used..used + w));
                used += w + 1;
                spans.push(Span::styled(cut, style));
                spans.push(Span::raw(" "));
            }
            break;
        }
    }
    (spans, used, places)
}

/// A cap is drawn in the pill's color on the row's own background.
fn cap_style(pill: &Style) -> Style {
    let mut style = Style::new();
    style.fg = pill.bg;
    style
}
