mod support;

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;

use support::TestRepo;
use terminal_git_graph::actions::Action;
use terminal_git_graph::app::Command;
use terminal_git_graph::clipboard::CopyVia;
use terminal_git_graph::input::ClickTracker;
use terminal_git_graph::manygit::status::{RepoStatus, SyncState};
use terminal_git_graph::manygit::view::{
    fetch_all, repo_menu, should_refresh, Manygit, MANYGIT_REFRESH_SECS,
};
use terminal_git_graph::manygit::RepoEntry;
use terminal_git_graph::ui::dialog::Dialog;
use terminal_git_graph::ui::menu::{MenuId, MenuItem};
use terminal_git_graph::ui::theme::ColorDepth;

fn entry(root: &Path, group: &str, name: &str, repo: &TestRepo) -> RepoEntry {
    RepoEntry {
        path: repo.path().to_path_buf(),
        name: name.to_string(),
        group: root.join(group),
    }
}

/// A repo with one commit, an `origin` remote and `main` tracking it.
fn tracked() -> (TestRepo, tempfile::TempDir) {
    let repo = TestRepo::new();
    repo.commit("base");
    let remote = repo.add_bare_remote("origin");
    repo.git(&["branch", "--set-upstream-to=origin/main", "main"]);
    (repo, remote)
}

fn manygit(root: &Path, entries: Vec<RepoEntry>) -> Manygit {
    Manygit::new(root.to_path_buf(), entries, false, ColorDepth::TrueColor)
}

fn render(m: &Manygit, width: u16, height: u16) -> Vec<String> {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| m.draw(f)).unwrap();
    let buf = term.backend().buffer();
    (0..height)
        .map(|y| (0..width).map(|x| buf[(x, y)].symbol()).collect())
        .collect()
}

fn press(m: &mut Manygit, code: KeyCode) {
    if let Some(cmd) = m.key_command(KeyEvent::from(code)) {
        m.apply(cmd).unwrap();
    }
}

fn labels(items: &[MenuItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

fn pick(m: &mut Manygit, label: &str) {
    let i = m
        .menu()
        .expect("menu open")
        .items
        .iter()
        .position(|i| i.label == label)
        .unwrap_or_else(|| panic!("no {label}"));
    m.apply(Command::MenuPick(i)).unwrap();
}

fn wait_job(m: &mut Manygit) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !m.poll_job() {
        assert!(Instant::now() < deadline, "job did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn status(sync: SyncState) -> RepoStatus {
    RepoStatus {
        branch: Some("main".to_string()),
        dirty: true,
        sync,
        latest_tag: None,
    }
}

#[test]
fn list_shows_groups_and_status_markers() {
    let root = tempfile::tempdir().unwrap();

    let (ahead, _ahead_remote) = tracked();
    ahead.commit("local");

    let (behind, _behind_remote) = tracked();
    behind.commit("pushed later");
    behind.git(&["push", "origin", "main"]);
    behind.git(&["reset", "--hard", "HEAD~1"]);

    let unpushed = TestRepo::new();
    unpushed.commit("base");
    let _unpushed_remote = unpushed.add_bare_remote("origin");

    let lonely = TestRepo::new();
    lonely.commit("base");
    fs::write(lonely.path().join("new.txt"), "x").unwrap();

    let m = manygit(
        root.path(),
        vec![
            entry(root.path(), "work", "ahead-repo", &ahead),
            entry(root.path(), "work", "behind-repo", &behind),
            entry(root.path(), "play", "unpushed-repo", &unpushed),
            entry(root.path(), "play", "lonely-repo", &lonely),
        ],
    );
    let lines = render(&m, 60, 12);
    let screen = lines.join("\n");
    let line_of = |needle: &str| {
        lines
            .iter()
            .find(|l| l.contains(needle))
            .unwrap_or_else(|| panic!("no {needle}\n{screen}"))
            .clone()
    };
    // Group headers, then their repositories.
    let work = lines
        .iter()
        .position(|l| l.trim() == "work")
        .expect(&screen);
    let play = lines
        .iter()
        .position(|l| l.trim() == "play")
        .expect(&screen);
    let row = |name: &str| lines.iter().position(|l| l.contains(name)).unwrap();
    assert!(
        work < row("ahead-repo") && row("behind-repo") < play,
        "{screen}"
    );
    assert!(
        play < row("unpushed-repo") && play < row("lonely-repo"),
        "{screen}"
    );

    assert!(line_of("ahead-repo").contains("main ↑1"), "{screen}");
    assert!(line_of("behind-repo").contains("↓1"), "{screen}");
    assert!(line_of("unpushed-repo").contains("not pushed"), "{screen}");
    let lonely_line = line_of("lonely-repo");
    assert!(lonely_line.contains("no remote"), "{screen}");
    assert!(lonely_line.contains('*'), "dirty marker\n{screen}");
    assert!(!line_of("ahead-repo").contains('*'), "{screen}");
}

#[test]
fn enter_opens_graph_and_back_returns_to_list() {
    let root = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.commit("hello graph");
    let mut m = manygit(root.path(), vec![entry(root.path(), "g", "solo", &repo)]);
    assert!(m.graph().is_none());

    press(&mut m, KeyCode::Enter);
    assert_eq!(m.current_path(), Some(repo.path()));
    let screen = render(&m, 60, 10).join("\n");
    assert!(screen.contains("‹ Manygit"), "{screen}");
    assert!(screen.contains("solo"), "{screen}");
    assert!(screen.contains("hello graph"), "{screen}");

    // Esc closes an open detail first, then goes back to the list.
    press(&mut m, KeyCode::Enter);
    assert!(m.graph().unwrap().detail_open());
    press(&mut m, KeyCode::Esc);
    assert!(m.graph().is_some());
    press(&mut m, KeyCode::Esc);
    assert!(m.graph().is_none());
    assert!(!m.should_quit());
    let screen = render(&m, 60, 10).join("\n");
    assert!(
        screen.contains("solo") && !screen.contains("hello graph"),
        "{screen}"
    );

    // A click on "‹ Manygit" goes back too.
    press(&mut m, KeyCode::Enter);
    let area = Rect::new(0, 0, 60, 10);
    render(&m, 60, 10);
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: 1,
        row: 0,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    let cmd = m.mouse_command(area, click, &mut ClickTracker::new(), Instant::now());
    assert_eq!(cmd, Some(Command::BackToList));
    m.apply(cmd.unwrap()).unwrap();
    assert!(m.graph().is_none());

    // Esc in the list quits.
    press(&mut m, KeyCode::Esc);
    assert!(m.should_quit());
}

#[test]
fn header_name_dropdown_switches_repo() {
    let root = tempfile::tempdir().unwrap();
    let one = TestRepo::new();
    one.commit("in one");
    let two = TestRepo::new();
    two.commit("in two");
    let mut m = manygit(
        root.path(),
        vec![
            entry(root.path(), "g", "one", &one),
            entry(root.path(), "g", "two", &two),
        ],
    );
    press(&mut m, KeyCode::Enter);
    assert_eq!(m.current_path(), Some(one.path()));

    // Clicking the repository name in the header opens the dropdown.
    let lines = render(&m, 60, 10);
    let x = lines[0].find("one").expect(&lines[0]) as u16;
    let click = MouseEvent {
        kind: MouseEventKind::Down(MouseButton::Left),
        column: lines[0][..x as usize].chars().count() as u16,
        row: 0,
        modifiers: crossterm::event::KeyModifiers::NONE,
    };
    let area = Rect::new(0, 0, 60, 10);
    let cmd = m.mouse_command(area, click, &mut ClickTracker::new(), Instant::now());
    assert_eq!(cmd, Some(Command::RepoMenu));
    m.apply(cmd.unwrap()).unwrap();
    let menu = m.menu().expect("dropdown");
    assert_eq!(labels(&menu.items), vec!["one", "two"]);
    assert!(
        !menu.items[0].enabled,
        "the open repository is not a choice"
    );

    pick(&mut m, "two");
    assert_eq!(m.current_path(), Some(two.path()));
    let screen = render(&m, 60, 10).join("\n");
    assert!(
        screen.contains("in two") && !screen.contains("in one"),
        "{screen}"
    );

    // The keyboard opens it too.
    press(&mut m, KeyCode::Char('s'));
    pick(&mut m, "one");
    assert_eq!(m.current_path(), Some(one.path()));
}

#[test]
fn first_menu_action_follows_ahead_behind() {
    let remotes = vec!["git@github.com:o/r.git".to_string()];
    let first = |sync| repo_menu(&status(sync), &remotes)[0].clone();

    let behind = first(SyncState::Behind(2));
    assert_eq!(
        (behind.label.as_str(), behind.id),
        ("Pull", MenuId::Do(Action::Pull))
    );
    let ahead = first(SyncState::Ahead(1));
    assert_eq!(
        (ahead.label.as_str(), ahead.id),
        ("Push", MenuId::Do(Action::PushCurrent))
    );
    for sync in [
        SyncState::InSync,
        SyncState::Diverged(1, 1),
        SyncState::NotPushed,
    ] {
        let item = first(sync.clone());
        assert_eq!(item.label, "Fetch", "{sync:?}");
        assert_eq!(item.id, MenuId::Do(Action::Fetch));
    }

    let items = repo_menu(&status(SyncState::Behind(1)), &remotes);
    assert_eq!(
        labels(&items),
        vec![
            "Pull",
            "Push",
            "Fetch",
            "Commit",
            "Stash",
            "Reset",
            "Open remote page"
        ]
    );
    let open = items
        .iter()
        .find(|i| i.label == "Open remote page")
        .unwrap();
    assert_eq!(
        open.id,
        MenuId::OpenRemote("https://github.com/o/r".to_string())
    );
    assert!(items.iter().find(|i| i.label == "Reset").unwrap().danger);

    // Without a remote, nothing that talks to one is enabled.
    let items = repo_menu(&status(SyncState::NoRemote), &[]);
    assert_eq!(items[0].label, "Fetch");
    for label in ["Pull", "Push", "Fetch", "Open remote page"] {
        assert!(
            !items.iter().find(|i| i.label == label).unwrap().enabled,
            "{label}"
        );
    }
    assert!(items.iter().find(|i| i.label == "Commit").unwrap().enabled);

    // In the list, the menu of the selected repository adds Copy path.
    let root = tempfile::tempdir().unwrap();
    let (repo, _remote) = tracked();
    repo.commit("ahead");
    let mut m = manygit(root.path(), vec![entry(root.path(), "g", "r", &repo)]);
    press(&mut m, KeyCode::Char('m'));
    let menu = m.menu().expect("repo menu");
    assert_eq!(menu.items[0].label, "Push");
    let last = menu.items.last().unwrap();
    assert_eq!(last.label, "Copy path");
    assert_eq!(
        last.id,
        MenuId::Copy(repo.path().to_string_lossy().into_owned())
    );
}

fn failing_opener(_url: &str) -> io::Result<()> {
    Err(io::Error::new(io::ErrorKind::NotFound, "no browser"))
}

fn failing_copier(_text: &str) -> io::Result<CopyVia> {
    Err(io::Error::new(io::ErrorKind::NotFound, "no clipboard"))
}

#[test]
fn open_remote_page_shows_url_when_no_browser_opens() {
    let root = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.commit("base");
    repo.git(&["remote", "add", "origin", "git@github.com:o/r.git"]);
    let mut m = manygit(root.path(), vec![entry(root.path(), "g", "r", &repo)]);
    m.set_opener(failing_opener);
    m.set_copier(failing_copier);
    press(&mut m, KeyCode::Char('m'));
    pick(&mut m, "Open remote page");
    match &m.dialog().expect("dialog").dialog {
        Dialog::Error { message, .. } => {
            assert!(message.contains("https://github.com/o/r"), "{message}")
        }
        other => panic!("{other:?}"),
    }
}

#[test]
fn list_action_asks_then_runs_and_refreshes_status() {
    let root = tempfile::tempdir().unwrap();
    let (repo, _remote) = tracked();
    repo.commit("to push");
    let mut m = manygit(root.path(), vec![entry(root.path(), "g", "r", &repo)]);
    assert!(render(&m, 60, 6).join("\n").contains("↑1"));
    press(&mut m, KeyCode::Char('m'));
    pick(&mut m, "Push");
    assert!(matches!(
        m.dialog().expect("confirm").dialog,
        Dialog::Confirm { .. }
    ));
    m.apply(Command::DialogAccept).unwrap();
    wait_job(&mut m);
    assert!(m.dialog().is_none(), "{:?}", m.dialog());
    let screen = render(&m, 60, 6).join("\n");
    assert!(!screen.contains("↑1"), "{screen}");
}

#[test]
fn fetch_all_skips_repos_without_remote() {
    let root = tempfile::tempdir().unwrap();
    let (with_remote, remote) = tracked();
    let lonely = TestRepo::new();
    lonely.commit("base");

    // Someone else pushes, so a fetch has something to bring in.
    let other = tempfile::tempdir().unwrap();
    let out = std::process::Command::new("git")
        .args([
            "clone",
            remote.path().to_str().unwrap(),
            other.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    for args in [
        &[
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e",
            "commit",
            "--allow-empty",
            "-m",
            "x",
        ][..],
        &["push", "origin", "main"][..],
    ] {
        let out = std::process::Command::new("git")
            .args(args)
            .current_dir(other.path())
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "{}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let paths: Vec<PathBuf> = vec![lonely.path().into(), with_remote.path().into()];
    let results = fetch_all(&paths);
    let fetched: Vec<&Path> = results.iter().map(|(p, _)| p.as_path()).collect();
    assert_eq!(fetched, vec![with_remote.path()]);
    assert!(results[0].1.is_ok(), "{:?}", results[0].1);

    // From the list: F asks first, then the list shows the repo behind.
    let mut m = manygit(
        root.path(),
        vec![
            entry(root.path(), "g", "lonely", &lonely),
            entry(root.path(), "g", "tracked", &with_remote),
        ],
    );
    let other_commit = std::process::Command::new("git")
        .args([
            "-c",
            "user.name=T",
            "-c",
            "user.email=t@e",
            "commit",
            "--allow-empty",
            "-m",
            "y",
        ])
        .current_dir(other.path())
        .output()
        .unwrap();
    assert!(other_commit.status.success());
    let pushed = std::process::Command::new("git")
        .args(["push", "origin", "main"])
        .current_dir(other.path())
        .output()
        .unwrap();
    assert!(pushed.status.success());

    press(&mut m, KeyCode::Char('F'));
    match &m.dialog().expect("confirm").dialog {
        Dialog::Confirm { body, .. } => assert!(body.contains("1 repositor"), "{body}"),
        other => panic!("{other:?}"),
    }
    m.apply(Command::DialogAccept).unwrap();
    wait_job(&mut m);
    assert!(m.dialog().is_none(), "{:?}", m.dialog());
    let screen = render(&m, 60, 6).join("\n");
    assert!(screen.contains("↓2"), "{screen}");
}

#[test]
fn should_refresh_after_15_seconds_only() {
    assert_eq!(MANYGIT_REFRESH_SECS, 15);
    let t = Instant::now();
    assert!(!should_refresh(t, t));
    assert!(!should_refresh(t, t + Duration::from_millis(14_999)));
    assert!(should_refresh(t, t + Duration::from_secs(15)));
    assert!(should_refresh(t, t + Duration::from_secs(60)));
    // A clock that went backwards is not a reason to refresh.
    assert!(!should_refresh(t + Duration::from_secs(5), t));

    // The list refreshes on tick only once the interval has passed.
    let root = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.commit("base");
    let mut m = manygit(root.path(), vec![entry(root.path(), "g", "r", &repo)]);
    let now = Instant::now();
    assert!(!m.tick(now));
    assert!(!m.refreshing());
    fs::write(repo.path().join("new.txt"), "x").unwrap();
    // The refresh runs on a thread: the tick returns at once and the list is unchanged.
    let due = now + Duration::from_secs(MANYGIT_REFRESH_SECS);
    assert!(!m.tick(due));
    assert!(m.refreshing());
    assert!(!render(&m, 60, 6).join("\n").contains('*'));
    let deadline = Instant::now() + Duration::from_secs(5);
    while !m.tick(due) {
        assert!(m.refreshing(), "a running refresh is not restarted");
        assert!(Instant::now() < deadline, "refresh did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
    assert!(!m.refreshing());
    assert!(render(&m, 60, 6).join("\n").contains('*'));
}

#[test]
fn visited_repo_graph_is_cached_for_instant_reopen() {
    let root = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.commit("cached subject");
    let mut m = manygit(root.path(), vec![entry(root.path(), "g", "r", &repo)]);
    press(&mut m, KeyCode::Enter);
    press(&mut m, KeyCode::Down);
    let calls = m.graph().unwrap().git().call_count();
    press(&mut m, KeyCode::Esc);
    assert!(m.graph().is_none());

    press(&mut m, KeyCode::Enter);
    let screen = render(&m, 60, 10).join("\n");
    assert!(screen.contains("cached subject"), "{screen}");
    assert_eq!(m.graph().unwrap().git().call_count(), calls);

    // After that first draw, the cached view catches up with the repository.
    repo.commit("newer subject");
    assert!(m.load_pending().unwrap());
    let screen = render(&m, 60, 10).join("\n");
    assert!(screen.contains("newer subject"), "{screen}");
}

#[test]
fn too_small_list_hides_menu_and_ignores_its_keys() {
    let root = tempfile::tempdir().unwrap();
    let repo = TestRepo::new();
    repo.commit("base");
    let mut m = manygit(root.path(), vec![entry(root.path(), "g", "r", &repo)]);
    press(&mut m, KeyCode::Char('m'));
    assert!(m.menu().is_some());
    assert!(render(&m, 19, 3).join("\n").contains("Terminal too small"));
    assert_eq!(m.key_command(KeyEvent::from(KeyCode::Enter)), None);
    assert_eq!(m.key_command(KeyEvent::from(KeyCode::Down)), None);
    render(&m, 60, 6);
    assert!(m.key_command(KeyEvent::from(KeyCode::Enter)).is_some());
}
