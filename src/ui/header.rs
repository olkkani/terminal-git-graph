//! Header line: current branch, loaded commit count and the scope/remotes toolbar.

use ratatui::layout::Rect;
use ratatui::style::{Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use crate::app::App;
use crate::git::log::Scope;
use crate::git::refs::Head;
use crate::ui::layout::{display_width, truncate_to_width};

pub(crate) fn draw(app: &App, frame: &mut Frame, area: Rect) {
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
    let width = area.width as usize;
    let left = truncate_to_width(&format!("{branch}  {count}"), width);
    let mut spans = vec![Span::styled(
        left.clone(),
        Style::new().add_modifier(Modifier::BOLD),
    )];

    let toggle = |label: &'static str, on: bool| {
        let style = if on {
            Style::new().add_modifier(Modifier::REVERSED)
        } else {
            Style::new()
        };
        Span::styled(format!(" {label} "), style)
    };
    let toolbar = [
        toggle("All", app.scope == Scope::AllBranches),
        toggle("Current", app.scope == Scope::Current),
        Span::raw(" "),
        toggle("Remotes", app.remotes),
    ];
    let toolbar_w: usize = toolbar.iter().map(|s| display_width(&s.content)).sum();
    let used = display_width(&left);
    if used + 1 + toolbar_w <= width {
        spans.push(Span::raw(" ".repeat(width - used - toolbar_w)));
        spans.extend(toolbar);
    }
    frame.render_widget(Line::from(spans), area);
}
