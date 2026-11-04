mod support;

use std::fs;
use std::io::{self, Write};

use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use support::TestRepo;
use terminal_git_graph::app::{App, Command};
use terminal_git_graph::config::RendererSetting;
use terminal_git_graph::gfx::kitty;
use terminal_git_graph::gfx::session::{decide_mode, present_frame, GfxMode, GfxSession};
use terminal_git_graph::gfx::CellPx;
use terminal_git_graph::git::Git;
use terminal_git_graph::manygit::view::Manygit;
use terminal_git_graph::manygit::RepoEntry;
use terminal_git_graph::terminal::{clear_graphics, Screen};
use terminal_git_graph::ui::dialog::Dialog;
use terminal_git_graph::ui::theme::{selected_bg, ColorDepth};

const CELL: CellPx = CellPx { w: 8, h: 16 };

fn contains(haystack: &[u8], needle: &[u8]) -> bool {
    haystack.windows(needle.len()).any(|w| w == needle)
}

fn app_for(repo: &TestRepo) -> App {
    App::new(Git::at(repo.path()), ColorDepth::TrueColor).expect("open app")
}

fn branched_repo() -> TestRepo {
    let repo = TestRepo::new();
    repo.commit("base");
    repo.git(&["checkout", "-b", "side"]);
    repo.commit("on side");
    repo.git(&["checkout", "main"]);
    repo.commit("on main");
    repo
}

/// Draws and runs the post-draw hook until it asks for no further redraw.
fn frame(
    term: &mut Terminal<TestBackend>,
    screen: &mut impl Screen,
    session: &mut GfxSession,
    out: &mut impl Write,
) {
    for _ in 0..4 {
        term.draw(|f| screen.draw(f)).unwrap();
        let size = term.size().unwrap();
        let area = Rect::new(0, 0, size.width, size.height);
        if !present_frame(session, screen, out, area) {
            return;
        }
    }
    panic!("present_frame kept asking for redraws");
}

fn line(term: &Terminal<TestBackend>, y: u16) -> String {
    let buf = term.backend().buffer();
    (0..buf.area.width).map(|x| buf[(x, y)].symbol()).collect()
}

struct FailingWriter;

impl Write for FailingWriter {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("broken pipe"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("broken pipe"))
    }
}

#[test]
fn decide_mode_table() {
    use RendererSetting::*;
    let c = Some(CELL);
    assert_eq!(decide_mode(Text, c, false), (GfxMode::Text, None));
    assert_eq!(decide_mode(Auto, c, false), (GfxMode::Graphics(CELL), None));
    assert_eq!(decide_mode(Auto, None, false), (GfxMode::Text, None));
    assert_eq!(
        decide_mode(Graphics, c, false),
        (GfxMode::Graphics(CELL), None)
    );
    let (mode, warn) = decide_mode(Graphics, None, false);
    assert_eq!(mode, GfxMode::Text);
    assert!(warn.unwrap().ends_with("falling back to text rendering."));
    assert_eq!(decide_mode(Auto, c, true), (GfxMode::Text, None));
    let (mode, warn) = decide_mode(Graphics, c, true);
    assert_eq!(mode, GfxMode::Text);
    assert!(warn.is_some());
}

#[test]
fn unanswered_probe_falls_back_within_budget() {
    // No probe reply is just `None`; deciding needs no terminal query.
    let start = std::time::Instant::now();
    assert_eq!(
        decide_mode(RendererSetting::Auto, None, false),
        (GfxMode::Text, None)
    );
    assert!(start.elapsed() < std::time::Duration::from_millis(300));
}

#[test]
fn graph_cells_are_blank_when_graphics_active() {
    let repo = branched_repo();
    let mut app = app_for(&repo);
    let mut term = Terminal::new(TestBackend::new(80, 10)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    // Header, column header, then the rows; the selected HEAD row is first.
    assert!(line(&term, 2).contains('●') || line(&term, 2).contains('◯'));

    app.set_gfx_active(true);
    term.draw(|f| app.draw(f)).unwrap();
    for y in 2..5 {
        let text = line(&term, y);
        assert!(text.starts_with("     "), "row {y}: {text:?}");
        assert!(!text.contains('●') && !text.contains('◯') && !text.contains('│'));
    }
    assert!(line(&term, 2).contains("on main"));
    // The selection background still covers the (blank) graph cells.
    let buf = term.backend().buffer();
    assert_eq!(buf[(0, 2)].bg, selected_bg(ColorDepth::TrueColor));
}

#[test]
fn placements_cover_exactly_visible_rows() {
    let repo = TestRepo::new();
    for i in 0..30 {
        repo.commit(&format!("commit {i}"));
    }
    fs::write(repo.path().join("dirty.txt"), "x").unwrap();
    let mut app = app_for(&repo);
    let (w, h) = (80, 12);
    let area = Rect::new(0, 0, w, h);
    let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();

    term.draw(|f| app.draw(f)).unwrap();
    let top = app.graph_placements(area, CELL);
    // Header line 0, column header line 1, rows 2..12.
    assert_eq!(
        top.iter().map(|p| p.row).collect::<Vec<_>>(),
        (2..12).collect::<Vec<_>>()
    );
    assert!(line(&term, 2).contains("Uncommitted"));
    // The uncommitted row is one lane wide; commits differ from it.
    assert_eq!(top[0].cols, 2);
    assert_eq!((top[0].rgba_w, top[0].rgba_h), (16, 16));
    assert_ne!(top[0].key, top[1].key);
    assert!(top.iter().all(|p| p.col == 0));

    app.apply(Command::Bottom).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let bottom = app.graph_placements(area, CELL);
    assert_eq!(
        bottom.iter().map(|p| p.row).collect::<Vec<_>>(),
        (2..12).collect::<Vec<_>>()
    );
    assert!(line(&term, 11).contains("commit 0"));
    assert!(!(2..12).any(|y| line(&term, y).contains("Uncommitted")));
}

#[test]
fn present_shows_rows_once_graphics_active() {
    let repo = branched_repo();
    let mut app = app_for(&repo);
    let mut term = Terminal::new(TestBackend::new(80, 10)).unwrap();
    let mut session = GfxSession::new(GfxMode::Graphics(CELL));
    let mut out = Vec::new();
    frame(&mut term, &mut app, &mut session, &mut out);
    assert!(contains(&out, b"a=t,f=32"));
    assert!(contains(&out, b"a=p,"));
    assert!(line(&term, 2).starts_with("     "));
}

#[test]
fn menu_open_clears_placements_and_draws_text_graph() {
    let repo = branched_repo();
    let mut app = app_for(&repo);
    let mut term = Terminal::new(TestBackend::new(80, 10)).unwrap();
    let mut session = GfxSession::new(GfxMode::Graphics(CELL));
    let mut out = Vec::new();
    frame(&mut term, &mut app, &mut session, &mut out);

    app.apply(Command::OpenMenu).unwrap();
    assert!(app.menu().is_some());
    out.clear();
    frame(&mut term, &mut app, &mut session, &mut out);
    assert_eq!(out, kitty::delete_all_placements());
    let text: String = (2..5).map(|y| line(&term, y)).collect();
    assert!(text.contains('●') || text.contains('◯'), "{text}");

    // Closing the menu brings the images back.
    app.apply(Command::MenuClose).unwrap();
    out.clear();
    frame(&mut term, &mut app, &mut session, &mut out);
    assert!(contains(&out, b"a=p,"));
    assert!(line(&term, 2).starts_with("     "));
}

#[test]
fn dialog_and_too_small_clear_placements() {
    let repo = branched_repo();
    let mut app = app_for(&repo);
    let mut term = Terminal::new(TestBackend::new(80, 10)).unwrap();
    let mut session = GfxSession::new(GfxMode::Graphics(CELL));
    let mut out = Vec::new();
    frame(&mut term, &mut app, &mut session, &mut out);

    app.open_dialog(Dialog::Info {
        title: "Hi".to_string(),
        body: "there".to_string(),
    });
    out.clear();
    frame(&mut term, &mut app, &mut session, &mut out);
    assert_eq!(out, kitty::delete_all_placements());

    app.apply(Command::DialogCancel).unwrap();
    frame(&mut term, &mut app, &mut session, &mut out);
    term.backend_mut().resize(10, 2);
    term.resize(Rect::new(0, 0, 10, 2)).unwrap();
    out.clear();
    frame(&mut term, &mut app, &mut session, &mut out);
    assert!(app.too_small());
    assert_eq!(out, kitty::delete_all_placements());
}

#[test]
fn write_error_switches_to_text_mode_without_quitting() {
    let repo = branched_repo();
    let mut app = app_for(&repo);
    let mut term = Terminal::new(TestBackend::new(80, 10)).unwrap();
    let mut session = GfxSession::new(GfxMode::Graphics(CELL));
    frame(&mut term, &mut app, &mut session, &mut FailingWriter);
    assert_eq!(session.mode(), GfxMode::Text);
    assert!(!app.should_quit());
    assert!(app.dialog().is_some());
    // The text graph is back and nothing more is written.
    let text: String = (0..10).map(|y| line(&term, y)).collect();
    assert!(text.contains('●') || text.contains('◯'), "{text}");
    let mut out = Vec::new();
    frame(&mut term, &mut app, &mut session, &mut out);
    assert!(out.is_empty());
}

#[test]
fn resize_with_new_cell_size_resets_presenter() {
    let repo = branched_repo();
    let mut app = app_for(&repo);
    let mut term = Terminal::new(TestBackend::new(80, 10)).unwrap();
    let mut session = GfxSession::new(GfxMode::Graphics(CELL));
    let mut out = Vec::new();
    frame(&mut term, &mut app, &mut session, &mut out);

    out.clear();
    session.on_resize(&mut app, &mut out, None);
    session.on_resize(&mut app, &mut out, Some(CELL));
    assert!(out.is_empty(), "unknown or unchanged size keeps the images");
    assert_eq!(session.mode(), GfxMode::Graphics(CELL));

    let bigger = CellPx { w: 10, h: 20 };
    session.on_resize(&mut app, &mut out, Some(bigger));
    assert!(contains(&out, &kitty::delete_all_placements()));
    assert!(contains(&out, b"a=d,d=I"));
    assert_eq!(session.mode(), GfxMode::Graphics(bigger));

    // The next frame transmits images at the new size.
    out.clear();
    frame(&mut term, &mut app, &mut session, &mut out);
    assert!(contains(&out, b"s=20,v=20"));
}

#[test]
fn exit_paths_emit_delete_sequences() {
    // `restore` (used by the normal and error exits of `run` and by the panic hook)
    // writes this first.
    let mut out = Vec::new();
    clear_graphics(&mut out, true).unwrap();
    assert!(out.starts_with(&kitty::delete_all_placements()));
    assert!(contains(&out, b"a=d,d=A"));

    let mut out = Vec::new();
    clear_graphics(&mut out, false).unwrap();
    assert!(out.is_empty(), "text mode never sends graphics commands");
}

#[test]
fn manygit_list_screen_has_no_placements() {
    let repo = branched_repo();
    let root = repo.path().parent().unwrap().to_path_buf();
    let entry = RepoEntry {
        path: repo.path().to_path_buf(),
        name: "solo".to_string(),
        group: root.clone(),
    };
    let mut list = Manygit::new(root, vec![entry], false, ColorDepth::TrueColor);
    let mut term = Terminal::new(TestBackend::new(80, 10)).unwrap();
    let mut session = GfxSession::new(GfxMode::Graphics(CELL));
    let mut out = Vec::new();
    frame(&mut term, &mut list, &mut session, &mut out);
    assert!(out.is_empty());
    assert!(list.gfx_frame(Rect::new(0, 0, 80, 10), CELL).is_none());

    // An opened graph does get images.
    list.apply(Command::Activate).unwrap();
    frame(&mut term, &mut list, &mut session, &mut out);
    assert!(contains(&out, b"a=p,"));
}
