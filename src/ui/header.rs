//! Header line: current branch, loaded commit count, the last refresh error and the
//! Fetch/scope/remotes toolbar.

use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::App;
use crate::git::log::Scope;
use crate::git::refs::Head;
use crate::input::ToolbarButton;
use crate::ui::layout::{display_width, truncate_to_width};

/// Widest the refresh error may get in the header, in cells.
const REFRESH_ERROR_MAX: usize = 40;

pub(crate) fn draw(app: &App, frame: &mut Frame, area: Rect) {
    let width = area.width as usize;
    let (mut spans, used) = left_spans(app, width);
    if let Some(toolbar) = toolbar(app, width - used) {
        let toolbar_w: usize = toolbar.iter().map(|(s, _)| display_width(&s.content)).sum();
        spans.push(Span::raw(" ".repeat(width - used - toolbar_w)));
        spans.extend(toolbar.into_iter().map(|(s, _)| s));
    }
    frame.render_widget(Line::from(spans), area);
}

/// The toolbar button at column `x` of the header drawn in `area`.
pub(crate) fn button_at(app: &App, area: Rect, x: u16) -> Option<ToolbarButton> {
    let width = area.width as usize;
    let (_, used) = left_spans(app, width);
    let toolbar = toolbar(app, width - used)?;
    let toolbar_w: usize = toolbar.iter().map(|(s, _)| display_width(&s.content)).sum();
    let mut start = area.x as usize + width - toolbar_w;
    for (span, button) in toolbar {
        let end = start + display_width(&span.content);
        if (start..end).contains(&(x as usize)) {
            return button;
        }
        start = end;
    }
    None
}

/// Branch, commit count and refresh error, with their width.
fn left_spans(app: &App, width: usize) -> (Vec<Span<'static>>, usize) {
    let branch = match &app.refs.head {
        Head::Branch(name) | Head::Unborn(name) => name.clone(),
        Head::Detached(hash) => format!("detached {}", &hash[..hash.len().min(7)]),
    };
    let loaded = app.commits.len();
    let count = if loaded >= app.total {
        format!("{loaded} commit{}", if loaded == 1 { "" } else { "s" })
    } else {
        format!("{loaded}/{} commits", app.total)
    };
    let left = truncate_to_width(&format!("{branch}  {count}"), width);
    let mut used = display_width(&left);
    let mut spans = vec![Span::styled(
        left,
        Style::new().add_modifier(Modifier::BOLD),
    )];

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

/// Toolbar pieces with the button each belongs to, when they fit in `room` cells
/// (keeping one space before them). Fetch is left out first when space is short.
fn toolbar(app: &App, room: usize) -> Option<Vec<(Span<'static>, Option<ToolbarButton>)>> {
    let toggle = |label: &'static str, on: bool, button: ToolbarButton| {
        let style = if on {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        (Span::styled(format!(" {label} "), style), Some(button))
    };
    let mut toolbar = vec![
        (Span::raw(" Fetch "), Some(ToolbarButton::Fetch)),
        (Span::raw(" "), None),
        toggle("All", app.scope == Scope::AllBranches, ToolbarButton::Scope),
        toggle("Current", app.scope == Scope::Current, ToolbarButton::Scope),
        (Span::raw(" "), None),
        toggle("Remotes", app.remotes, ToolbarButton::Remotes),
    ];
    let fits = |t: &[(Span, Option<ToolbarButton>)]| {
        t.iter()
            .map(|(s, _)| display_width(&s.content))
            .sum::<usize>()
            < room
    };
    if fits(&toolbar) {
        return Some(toolbar);
    }
    toolbar.drain(..2);
    fits(&toolbar).then_some(toolbar)
}
