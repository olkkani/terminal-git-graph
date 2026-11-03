mod support;

use std::fs;

use crossterm::event::{KeyCode, KeyEvent};
use ratatui::backend::TestBackend;
use ratatui::Terminal;
use support::TestRepo;
use terminal_git_graph::app::{App, Command};
use terminal_git_graph::git::Git;
use terminal_git_graph::manygit::view::Manygit;
use terminal_git_graph::manygit::RepoEntry;
use terminal_git_graph::terminal::{apply_command, Screen};
use terminal_git_graph::ui::theme::ColorDepth;
use terminal_git_graph::watch::WatchEvent;

fn screen(view: &impl Screen, width: u16, height: u16) -> String {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| view.draw(f)).unwrap();
    let buf = term.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buf[(x, y)].symbol()).collect::<String>())
        .collect::<Vec<_>>()
        .join("\n")
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

fn manygit_with(repo: &TestRepo, root: &std::path::Path) -> Manygit {
    let entry = RepoEntry {
        path: repo.path().to_path_buf(),
        name: "solo".to_string(),
        group: root.to_path_buf(),
    };
    Manygit::new(
        root.to_path_buf(),
        vec![entry],
        false,
        ColorDepth::TrueColor,
    )
}

fn has_unicode_graph(s: &str) -> bool {
    s.contains(['●', '◯', '│', '╭', '╮', '╰', '╯'])
}

#[test]
fn ascii_app_draws_graph_without_unicode_glyphs() {
    let repo = branched_repo();
    let mut app = App::new(Git::at(repo.path()), ColorDepth::TrueColor).unwrap();
    assert!(has_unicode_graph(&screen(&app, 80, 10)));

    app.set_ascii(true);
    let s = screen(&app, 80, 10);
    assert!(!has_unicode_graph(&s), "{s}");
    assert!(s.contains('*') && s.contains('O'), "{s}");
}

#[test]
fn ascii_manygit_passes_ascii_to_opened_graphs() {
    let root = tempfile::tempdir().unwrap();
    let repo = branched_repo();
    let mut m = manygit_with(&repo, root.path());
    m.set_ascii(true);
    let cmd = m.key_command(KeyEvent::from(KeyCode::Enter)).unwrap();
    m.apply(cmd).unwrap();
    let s = screen(&m, 80, 10);
    assert!(s.contains("on main"), "{s}");
    assert!(!has_unicode_graph(&s), "{s}");
}

#[test]
fn manygit_watch_event_reloads_the_open_graph() {
    let root = tempfile::tempdir().unwrap();
    let repo = branched_repo();
    let mut m = manygit_with(&repo, root.path());
    let cmd = m.key_command(KeyEvent::from(KeyCode::Enter)).unwrap();
    m.apply(cmd).unwrap();
    assert_eq!(Screen::watched_dir(&m), Some(repo.path()));

    repo.commit("arrived later");
    Screen::on_watch(&mut m, WatchEvent::RefsChanged);
    let s = screen(&m, 80, 10);
    assert!(s.contains("arrived later"), "{s}");
}

#[test]
fn manygit_list_watches_nothing() {
    let root = tempfile::tempdir().unwrap();
    let repo = branched_repo();
    let m = manygit_with(&repo, root.path());
    assert_eq!(Screen::watched_dir(&m), None);
}

#[test]
fn git_error_from_a_command_shows_a_dialog_instead_of_quitting() {
    let repo = branched_repo();
    let mut app = App::new(Git::at(repo.path()), ColorDepth::TrueColor).unwrap();
    fs::remove_dir_all(repo.path().join(".git")).unwrap();

    apply_command(&mut app, Command::Refresh);
    assert!(app.dialog().is_some());
    assert!(!app.should_quit());
}
