mod support;

use std::time::{Instant, SystemTime, UNIX_EPOCH};

use crossterm::event::{KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::layout::Rect;
use ratatui::style::Modifier;
use ratatui::Terminal;
use support::TestRepo;
use terminal_git_graph::app::{App, Command};
use terminal_git_graph::git::Git;
use terminal_git_graph::input::{hit_test, mouse_command, ClickTracker, Hit, ToolbarButton};
use terminal_git_graph::ui::table::relative_time;
use terminal_git_graph::ui::theme::ColorDepth;
use unicode_width::UnicodeWidthStr;

fn app_for(repo: &TestRepo) -> App {
    App::new(Git::at(repo.path()), ColorDepth::TrueColor).expect("open app")
}

fn buffer(app: &App, width: u16, height: u16) -> Buffer {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    term.backend().buffer().clone()
}

/// Screen text per row, skipping the hidden cells that follow a wide character.
fn lines(buf: &Buffer) -> Vec<String> {
    (0..buf.area.height)
        .map(|y| {
            let mut line = String::new();
            let mut x = 0;
            while x < buf.area.width {
                let sym = buf[(x, y)].symbol();
                line.push_str(sym);
                x += sym.width().max(1) as u16;
            }
            line
        })
        .collect()
}

fn render(app: &App, width: u16, height: u16) -> Vec<String> {
    lines(&buffer(app, width, height))
}

/// Display column where `text` starts in `line`.
fn col(line: &str, text: &str) -> u16 {
    let byte = line
        .find(text)
        .unwrap_or_else(|| panic!("{text:?} in {line:?}"));
    line[..byte].width() as u16
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

#[test]
fn column_header_shows_only_visible_columns() {
    let repo = TestRepo::new();
    repo.commit("hello");
    let app = app_for(&repo);

    let rows = render(&app, 50, 6);
    let header = &rows[1];
    for name in ["Graph", "Description", "Date"] {
        assert!(header.contains(name), "{name} in {header:?}");
    }
    for name in ["Author", "Commit"] {
        assert!(!header.contains(name), "no {name} in {header:?}");
    }
    assert!(rows[2].contains("hello"), "{rows:?}");

    let header = &render(&app, 120, 6)[1];
    for name in ["Graph", "Description", "Date", "Author", "Commit"] {
        assert!(header.contains(name), "{name} in {header:?}");
    }
}

#[test]
fn column_header_aligns_with_row_columns() {
    let repo = TestRepo::new();
    repo.commit("first");
    let hash = repo.commit("hello");
    repo.branch("feature");
    let app = app_for(&repo);
    let rows = render(&app, 120, 6);
    let header = &rows[1];
    let row = rows.iter().find(|l| l.contains("hello")).expect("row");
    let ts: i64 = repo
        .git(&["log", "-1", "--format=%ct"])
        .trim()
        .parse()
        .unwrap();
    let date = relative_time(now(), ts);
    let date_x = row[..row.rfind(&date).unwrap()].width() as u16;
    assert_eq!(col(header, "Date"), date_x, "{header:?}\n{row:?}");
    assert_eq!(
        col(header, "Author"),
        col(row, "Test User"),
        "{header:?}\n{row:?}"
    );
    assert_eq!(
        col(header, "Commit"),
        col(row, &hash[..7]),
        "{header:?}\n{row:?}"
    );
    // Description starts on the first badge cell, right after the graph's gap.
    let y = rows.iter().position(|l| l.contains("hello")).unwrap() as u16;
    let x = col(header, "Description");
    let area = Rect::new(0, 0, 120, 6);
    assert!(
        matches!(hit_test(&app, area, (x, y)), Some(Hit::Label { .. })),
        "{header:?}\n{row:?}"
    );
    assert_eq!(hit_test(&app, area, (x - 1, y)), Some(Hit::Row(0)));
    assert_eq!(col(header, "Graph"), 0, "{header:?}");
}

#[test]
fn segmented_control_highlights_active_scope() {
    let repo = TestRepo::new();
    repo.commit("hello");
    let mut app = app_for(&repo);
    let reversed = |app: &App, text: &str| {
        let buf = buffer(app, 100, 6);
        let x = col(&lines(&buf)[0], text);
        buf[(x, 0)].modifier.contains(Modifier::REVERSED)
    };
    let header = &render(&app, 100, 6)[0];
    assert!(header.contains("All branches | Current"), "{header:?}");
    assert!(reversed(&app, "All branches"));
    assert!(!reversed(&app, "Current"));

    app.apply(Command::ToggleScope).unwrap();
    assert!(!reversed(&app, "All branches"));
    assert!(reversed(&app, "Current"));
}

#[test]
fn remotes_checkbox_reflects_state() {
    let repo = TestRepo::new();
    repo.commit("hello");
    let mut app = app_for(&repo);
    let header = &render(&app, 100, 6)[0];
    assert!(header.contains("☑ Remotes"), "{header:?}");
    assert!(!header.contains("☐"), "{header:?}");

    app.apply(Command::ToggleRemotes).unwrap();
    let header = &render(&app, 100, 6)[0];
    assert!(header.contains("☐ Remotes"), "{header:?}");
    assert!(!header.contains("☑"), "{header:?}");

    for glyph in ["☑", "☐"] {
        assert_eq!(unicode_width::UnicodeWidthStr::width(glyph), 1);
        assert_eq!(unicode_width::UnicodeWidthStr::width_cjk(glyph), 1);
    }
}

#[test]
fn toolbar_click_targets_follow_new_layout() {
    let repo = TestRepo::new();
    repo.commit("hello");
    let app = app_for(&repo);
    let (w, h) = (100, 6);
    let header = render(&app, w, h)[0].clone();
    assert!(
        header.trim_end().ends_with("1 commit  Fetch"),
        "count and Fetch on the right: {header:?}"
    );
    let area = Rect::new(0, 0, w, h);
    let hit = |x| hit_test(&app, area, (x, 0));
    let scope = Some(Hit::Toolbar(ToolbarButton::Scope));
    for text in ["All branches", "Current"] {
        let x = col(&header, text);
        assert_eq!(hit(x), scope, "{text}");
        assert_eq!(hit(x + text.len() as u16 - 1), scope, "end of {text}");
    }
    let x = col(&header, "Remotes");
    assert_eq!(hit(x), Some(Hit::Toolbar(ToolbarButton::Remotes)));
    assert_eq!(
        hit(x - 2),
        Some(Hit::Toolbar(ToolbarButton::Remotes)),
        "checkbox"
    );
    let x = col(&header, "Fetch");
    assert_eq!(hit(x), Some(Hit::Toolbar(ToolbarButton::Fetch)));
    assert_eq!(hit(x + 4), Some(Hit::Toolbar(ToolbarButton::Fetch)));
    assert_eq!(hit(col(&header, "1 commit")), None, "count is not a button");
    assert_eq!(hit(0), None, "branch is not a button");

    let mut clicks = ClickTracker::new();
    let ev = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: col(&header, "Remotes"),
        row: 0,
        modifiers: KeyModifiers::NONE,
    };
    assert_eq!(
        mouse_command(&app, area, ev, &mut clicks, Instant::now()),
        Some(Command::ToggleRemotes)
    );
}

#[test]
fn row_click_ignores_column_header() {
    let repo = TestRepo::new();
    repo.commit("one");
    repo.commit("two");
    let app = app_for(&repo);
    let (w, h) = (80, 6);
    render(&app, w, h);
    let area = Rect::new(0, 0, w, h);
    for x in [0, 10, 40, 70] {
        assert_eq!(hit_test(&app, area, (x, 1)), None, "column header at x={x}");
    }
    assert_eq!(hit_test(&app, area, (40, 2)), Some(Hit::Row(0)));
    assert_eq!(hit_test(&app, area, (40, 3)), Some(Hit::Row(1)));
    assert_eq!(hit_test(&app, area, (40, 4)), None);

    let mut clicks = ClickTracker::new();
    let ev = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 40,
        row: 1,
        modifiers: KeyModifiers::NONE,
    };
    assert_eq!(
        mouse_command(&app, area, ev, &mut clicks, Instant::now()),
        None
    );
}

#[test]
fn narrow_header_drops_items_without_overflow() {
    let repo = TestRepo::new();
    repo.commit("hello");
    let app = app_for(&repo);
    let (w, h) = (30, 6);
    let header = &render(&app, w, h)[0];
    assert_eq!(header.width(), w as usize, "{header:?}");
    assert!(header.starts_with("main"), "{header:?}");
    assert!(header.trim_end().ends_with("1 commit"), "{header:?}");
    for gone in ["Fetch", "Remotes", "Current", "All branches"] {
        assert!(!header.contains(gone), "no {gone} in {header:?}");
    }
    let area = Rect::new(0, 0, w, h);
    for x in 0..w {
        assert_eq!(hit_test(&app, area, (x, 0)), None, "x={x}");
    }
    // Every width from the minimum up draws without panicking.
    for w in 20..=110 {
        let header = &render(&app, w, h)[0];
        assert_eq!(header.width(), w as usize, "{header:?}");
    }
}
