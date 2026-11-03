mod support;

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::{Color, Modifier};
use ratatui::Terminal;
use support::TestRepo;
use terminal_git_graph::app::App;
use terminal_git_graph::git::Git;
use terminal_git_graph::input::{hit_test, Hit};
use terminal_git_graph::ui::glyphs::badge_glyphs;
use terminal_git_graph::ui::theme::ColorDepth;
use unicode_width::{UnicodeWidthChar, UnicodeWidthStr};

fn app_for(repo: &TestRepo) -> App {
    App::new(Git::at(repo.path()), ColorDepth::TrueColor).expect("open app")
}

fn draw(app: &App, w: u16, h: u16) -> Buffer {
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    term.backend().buffer().clone()
}

fn row_text(buf: &Buffer, y: u16) -> String {
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

/// Column and row of the first cell whose symbol is `sym`.
fn find_cell(buf: &Buffer, sym: &str) -> (u16, u16) {
    for y in 0..buf.area.height {
        for x in 0..buf.area.width {
            if buf[(x, y)].symbol() == sym {
                return (x, y);
            }
        }
    }
    panic!("no {sym:?} on screen");
}

fn all_glyphs(nerd_font: bool, ascii: bool) -> Vec<&'static str> {
    let g = badge_glyphs(nerd_font, ascii);
    vec![g.left_cap, g.right_cap, g.check, g.local, g.remote, g.tag]
}

#[test]
fn checked_out_branch_badge_is_filled_with_check() {
    let repo = TestRepo::new();
    repo.commit("first");
    let app = app_for(&repo);
    let buf = draw(&app, 60, 6);
    let (x, y) = find_cell(&buf, "✓");
    let cell = &buf[(x, y)];
    assert!(matches!(cell.bg, Color::Rgb(..)), "filled: {:?}", cell.bg);
    assert!(cell.modifier.contains(Modifier::BOLD));
    assert!(row_text(&buf, y).contains("✓main"), "{}", row_text(&buf, y));
}

#[test]
fn remote_and_tag_badges_have_distinct_icons() {
    let repo = TestRepo::new();
    repo.commit("first");
    let _remote = repo.add_bare_remote("origin");
    repo.tag("v1.0");
    repo.commit("second");
    let app = app_for(&repo);
    let buf = draw(&app, 80, 8);
    let g = badge_glyphs(false, false);
    let rows: Vec<String> = (0..8).map(|y| row_text(&buf, y)).collect();
    let on = |name: &str| {
        rows.iter()
            .find(|r| r.contains(name))
            .unwrap_or_else(|| panic!("no {name}: {rows:?}"))
            .clone()
    };
    assert!(on("origin/main").contains(g.remote), "{rows:?}");
    assert!(on("v1.0").contains(g.tag), "{rows:?}");
    assert!(on("✓main").contains(g.local), "{rows:?}");
    assert!(g.local != g.remote && g.remote != g.tag && g.local != g.tag);
}

#[test]
fn all_new_glyphs_are_single_width() {
    for nerd in [false, true] {
        for glyph in all_glyphs(nerd, false) {
            if glyph.is_empty() {
                continue;
            }
            assert_eq!(glyph.width(), 1, "{glyph:?}");
            assert_eq!(glyph.width_cjk(), 1, "{glyph:?}");
        }
    }
}

#[test]
fn ascii_glyphs_are_ascii_only() {
    for nerd in [false, true] {
        for glyph in all_glyphs(nerd, true) {
            assert!(glyph.is_ascii(), "{glyph:?}");
        }
    }
}

#[test]
fn nerd_font_adds_round_caps() {
    let plain = badge_glyphs(false, false);
    let nerd = badge_glyphs(true, false);
    assert!(plain.left_cap.is_empty() && plain.right_cap.is_empty());
    assert!(!nerd.left_cap.is_empty() && !nerd.right_cap.is_empty());

    let repo = TestRepo::new();
    repo.commit("first");
    let mut app = app_for(&repo);
    app.set_nerd_font(true);
    let buf = draw(&app, 60, 6);
    let (_, y) = find_cell(&buf, nerd.left_cap);
    let line = row_text(&buf, y);
    assert!(line.contains(nerd.right_cap), "{line}");
    assert!(line.contains(nerd.left_cap), "{line}");
}

#[test]
fn badge_click_areas_match_drawn_width() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("alpha");
    for nerd in [false, true] {
        let mut app = app_for(&repo);
        app.set_nerd_font(nerd);
        let (w, h) = (60, 6);
        let area = ratatui::layout::Rect::new(0, 0, w, h);
        let buf = draw(&app, w, h);
        let y = (0..h)
            .find(|&y| row_text(&buf, y).contains("alpha"))
            .expect("row with alpha");
        let line = row_text(&buf, y);
        let name_x = line[..line.find("alpha").unwrap()].width() as u16;
        let is_label = |x: u16| matches!(hit_test(&app, area, (x, y)), Some(Hit::Label { .. }));
        assert!(is_label(name_x));
        let (mut first, mut last) = (name_x, name_x);
        while first > 0 && is_label(first - 1) {
            first -= 1;
        }
        while last + 1 < w && is_label(last + 1) {
            last += 1;
        }
        let g = badge_glyphs(nerd, false);
        let drawn: String = (first..=last).map(|x| buf[(x, y)].symbol()).collect();
        let expected = if nerd {
            format!("{}{} alpha{}", g.left_cap, g.local, g.right_cap)
        } else {
            format!(" {} alpha ", g.local)
        };
        assert_eq!(drawn, expected, "nerd={nerd}");
    }
}

#[test]
fn long_badge_is_cut_with_ellipsis_within_budget() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("a-very-long-branch-name-that-cannot-possibly-fit-in-the-column");
    let app = app_for(&repo);
    let (w, h) = (60u16, 6u16);
    let buf = draw(&app, w, h);
    let y = (0..h)
        .find(|&y| row_text(&buf, y).contains("a-very"))
        .expect("row with the long badge");
    let line = row_text(&buf, y);
    assert!(line.contains('…'), "{line}");
    assert_eq!(
        line.chars().map(|c| c.width().unwrap_or(0)).sum::<usize>(),
        w as usize
    );
    // The badge stays within half of the description width.
    let area = ratatui::layout::Rect::new(0, 0, w, h);
    let cut_x = line[..line.find('…').unwrap()].width() as u16;
    assert!(matches!(
        hit_test(&app, area, (cut_x, y)),
        Some(Hit::Label { .. })
    ));
    assert!(cut_x < w / 2 + 12, "badge too wide: {line}");
}
