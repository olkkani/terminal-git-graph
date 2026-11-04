//! Header line: in Manygit, "‹ Manygit" and the repository name; then the current
//! branch, loaded commit count, the last refresh error and the Fetch/scope/remotes toolbar.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::App;
use crate::git::log::Scope;
use crate::git::refs::Head;
use crate::input::ToolbarButton;
use crate::ui::glyphs::badge_glyphs;
use crate::ui::layout::{display_width, truncate_to_width};
use crate::ui::theme;

/// Widest the refresh error may get in the header, in cells.
const REFRESH_ERROR_MAX: usize = 40;

pub(crate) fn draw(app: &App, frame: &mut Frame, area: Rect) {
    let spans: Vec<Span> = pieces(app, area.width as usize)
        .into_iter()
        .map(|(s, _)| s)
        .collect();
    frame.render_widget(Line::from(spans), area);
}

/// The toolbar button at column `x` of the header drawn in `area`.
pub(crate) fn button_at(app: &App, area: Rect, x: u16) -> Option<ToolbarButton> {
    let mut start = area.x as usize;
    for (span, button) in pieces(app, area.width as usize) {
        let end = start + display_width(&span.content);
        if (start..end).contains(&(x as usize)) {
            return button;
        }
        start = end;
    }
    None
}

/// A piece of the header line and the button it belongs to.
type Piece = (Span<'static>, Option<ToolbarButton>);

/// The whole header line, `width` cells wide: navigation, branch and status on the
/// left, then the scope and remotes controls, and the commit count and Fetch on the right.
fn pieces(app: &App, width: usize) -> Vec<Piece> {
    let mut line = nav_spans(app, width);
    let nav_w = spans_width(&line);
    let (left, left_w) = left_spans(app, width.saturating_sub(nav_w));
    line.extend(left.into_iter().map(|s| (s, None)));
    let room = width.saturating_sub(nav_w + left_w);
    let (controls, right) = toolbar(app, room);
    let pad = room.saturating_sub(spans_width(&controls) + spans_width(&right));
    line.extend(controls);
    line.push((Span::raw(" ".repeat(pad)), None));
    line.extend(right);
    line
}

/// Column where the repository name starts, when the graph was opened from Manygit.
pub(crate) fn repo_name_x(app: &App) -> Option<u16> {
    app.repo_name()?;
    Some(display_width(BACK) as u16)
}

const BACK: &str = "‹ Manygit ";

fn spans_width(spans: &[Piece]) -> usize {
    spans.iter().map(|(s, _)| display_width(&s.content)).sum()
}

/// "‹ Manygit" and the repository name (with a dropdown mark) when the graph was opened
/// from the Manygit list, cut to `width`.
fn nav_spans(app: &App, width: usize) -> Vec<Piece> {
    let Some(name) = app.repo_name() else {
        return Vec::new();
    };
    let back = truncate_to_width(BACK, width);
    let name = truncate_to_width(&format!("{name} ▾ "), width - display_width(&back));
    vec![
        (
            Span::styled(back, Style::new().fg(Color::Cyan)),
            Some(ToolbarButton::Back),
        ),
        (
            Span::styled(name, Style::new().add_modifier(Modifier::UNDERLINED)),
            Some(ToolbarButton::RepoName),
        ),
    ]
}

/// Branch, running job and refresh error, with their width.
fn left_spans(app: &App, width: usize) -> (Vec<Span<'static>>, usize) {
    let branch = match &app.refs.head {
        Head::Branch(name) | Head::Unborn(name) => name.clone(),
        Head::Detached(hash) => format!("detached {}", &hash[..hash.len().min(7)]),
    };
    let glyphs = badge_glyphs(app.nerd_font, app.ascii);
    let lane = theme::lane_color(0, app.depth);
    let style = Style::new().fg(lane).add_modifier(Modifier::BOLD);
    let icon = if glyphs.local.is_empty() {
        String::new()
    } else {
        format!("{} ", glyphs.local)
    };
    let caps = display_width(glyphs.left_cap) + display_width(glyphs.right_cap);
    let chrome = display_width(&icon) + caps;
    // No room for the pill itself (a long Manygit repository name in a narrow pane).
    if width < chrome {
        return (Vec::new(), 0);
    }
    let name = truncate_to_width(&branch, width.saturating_sub(chrome));
    let body = format!("{icon}{name}");
    let mut used = display_width(&body) + caps;
    let mut spans = Vec::new();
    spans.push(Span::styled(glyphs.left_cap, style));
    spans.push(Span::styled(body, style));
    spans.push(Span::styled(glyphs.right_cap, style));

    if app.job_running() {
        let text = truncate_to_width("  Running…", width.saturating_sub(used));
        used += display_width(&text);
        spans.push(Span::styled(text, Style::new().fg(Color::Yellow)));
    }
    if let Some(err) = &app.refresh_error {
        // First line only, kept short so the header stays one line.
        let first = err.lines().next().unwrap_or_default();
        let room = width.saturating_sub(used).min(REFRESH_ERROR_MAX);
        let text = truncate_to_width(&format!("  ! {first}"), room);
        used += display_width(&text);
        spans.push(Span::styled(text, Style::new().fg(Color::Red)));
    }
    (spans, used)
}

/// The controls (after the left part) and the commit count with Fetch (right-aligned),
/// when they fit in `room` cells keeping a space before each group. Fetch is left out
/// first when space is short, then the controls, then the count.
fn toolbar(app: &App, room: usize) -> (Vec<Piece>, Vec<Piece>) {
    let segment = |label: &'static str, on: bool| {
        let style = if on {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        (
            Span::styled(format!(" {label} "), style),
            Some(ToolbarButton::Scope),
        )
    };
    let check = match (app.ascii, app.remotes) {
        (true, true) => "[x]",
        (true, false) => "[ ]",
        (false, true) => "☑",
        (false, false) => "☐",
    };
    let controls = vec![
        (Span::raw("  "), None),
        segment("All branches", app.scope == Scope::AllBranches),
        (Span::raw("|"), Some(ToolbarButton::Scope)),
        segment("Current", app.scope == Scope::Current),
        (Span::raw(" "), None),
        (
            Span::raw(format!(" {check} Remotes ")),
            Some(ToolbarButton::Remotes),
        ),
    ];
    let loaded = app.commits.len();
    let count = if loaded >= app.total {
        format!("{loaded} commit{}", if loaded == 1 { "" } else { "s" })
    } else {
        format!("{loaded}/{} commits", app.total)
    };
    let count = (Span::raw(count), None);
    let fetch = [
        (Span::raw(" "), None),
        (Span::raw(" Fetch "), Some(ToolbarButton::Fetch)),
    ];
    let full: Vec<Piece> = std::iter::once(count.clone()).chain(fetch).collect();
    let options = [
        (controls.clone(), full),
        (controls, vec![count.clone()]),
        (Vec::new(), vec![count]),
    ];
    options
        .into_iter()
        .find(|(c, r)| spans_width(c) + spans_width(r) < room)
        .unwrap_or_default()
}
