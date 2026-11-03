mod support;

use std::fs;
use std::sync::mpsc::{channel, Receiver};
use std::thread::sleep;
use std::time::{Duration, Instant};

use support::TestRepo;
use terminal_git_graph::app::{App, Command};
use terminal_git_graph::git::Git;
use terminal_git_graph::input::Hit;
use terminal_git_graph::ui::theme::ColorDepth;
use terminal_git_graph::watch::{watch, WatchEvent, WatchHandle, DEBOUNCE_MS};

const WAIT: Duration = Duration::from_secs(3);

/// A repo with one commit and a watcher that has had time to start.
fn watched() -> (TestRepo, WatchHandle, Receiver<WatchEvent>) {
    let repo = TestRepo::new();
    repo.commit("first");
    let (tx, rx) = channel();
    let handle = watch(&Git::at(repo.path()), tx).expect("start watcher");
    // Let the OS watcher settle so setup writes are not reported.
    sleep(Duration::from_millis(500));
    while rx.try_recv().is_ok() {}
    (repo, handle, rx)
}

/// Waits up to WAIT for `want`, returning whether it arrived.
fn wait_for(rx: &Receiver<WatchEvent>, want: WatchEvent) -> bool {
    let deadline = Instant::now() + WAIT;
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(ev) if ev == want => return true,
            Ok(_) => {}
            Err(_) => return false,
        }
    }
    false
}

/// Collects every event that arrives within WAIT.
fn collect(rx: &Receiver<WatchEvent>) -> Vec<WatchEvent> {
    let deadline = Instant::now() + WAIT;
    let mut events = Vec::new();
    while let Some(left) = deadline.checked_duration_since(Instant::now()) {
        match rx.recv_timeout(left) {
            Ok(ev) => events.push(ev),
            Err(_) => break,
        }
    }
    events
}

#[test]
fn commit_from_another_process_emits_refs_changed() {
    let (repo, _handle, rx) = watched();
    repo.commit("second");
    assert!(wait_for(&rx, WatchEvent::RefsChanged));
}

#[test]
fn checkout_emits_refs_changed() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("side");
    let (tx, rx) = channel();
    let _handle = watch(&Git::at(repo.path()), tx).expect("start watcher");
    sleep(Duration::from_millis(500));
    while rx.try_recv().is_ok() {}
    repo.checkout("side");
    assert!(wait_for(&rx, WatchEvent::RefsChanged));
}

#[test]
fn edit_file_emits_worktree_changed() {
    let (repo, _handle, rx) = watched();
    fs::write(repo.path().join("file-1.txt"), "edited\n").unwrap();
    assert!(wait_for(&rx, WatchEvent::WorktreeChanged));
}

#[test]
fn index_lock_noise_is_ignored() {
    let (repo, _handle, rx) = watched();
    let git_dir = repo.path().join(".git");
    fs::write(git_dir.join("index.lock"), "x").unwrap();
    fs::remove_file(git_dir.join("index.lock")).unwrap();
    fs::write(git_dir.join("some-internal-file"), "x").unwrap();
    assert_eq!(collect(&rx), Vec::<WatchEvent>::new());
}

#[test]
fn burst_of_changes_is_debounced() {
    let (repo, _handle, rx) = watched();
    for i in 0..10 {
        fs::write(repo.path().join("file-1.txt"), format!("edit {i}\n")).unwrap();
        sleep(Duration::from_millis(DEBOUNCE_MS / 10));
    }
    let events = collect(&rx);
    assert!(!events.is_empty());
    assert!(events.len() <= 3, "got {events:?}");
}

#[test]
fn reload_keeps_selected_commit() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.commit("second");
    repo.commit("third");
    let mut app = App::new(Git::at(repo.path()), ColorDepth::Indexed256).unwrap();
    app.apply(Command::Down).unwrap();
    let before = app.selected_hash().map(str::to_string);
    repo.branch("other");
    app.on_watch(WatchEvent::RefsChanged);
    assert_eq!(app.selected_hash().map(str::to_string), before);
}

#[test]
fn reload_after_new_commit_shows_it_on_top() {
    let repo = TestRepo::new();
    repo.commit("first");
    let mut app = App::new(Git::at(repo.path()), ColorDepth::Indexed256).unwrap();
    let new = repo.commit("second");
    app.on_watch(WatchEvent::RefsChanged);
    app.apply(Command::Top).unwrap();
    assert_eq!(app.selected_hash(), Some(new.as_str()));
}

#[test]
fn worktree_change_keeps_selected_commit() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.commit("second");
    repo.commit("third");
    let mut app = App::new(Git::at(repo.path()), ColorDepth::Indexed256).unwrap();
    app.apply(Command::Down).unwrap();
    let before = app.selected_hash().map(str::to_string);
    assert!(before.is_some());

    // The uncommitted row appears above the commits.
    fs::write(repo.path().join("new.txt"), "new\n").unwrap();
    app.on_watch(WatchEvent::WorktreeChanged);
    assert_eq!(app.selected_hash().map(str::to_string), before);

    // ...and goes away again.
    fs::remove_file(repo.path().join("new.txt")).unwrap();
    app.on_watch(WatchEvent::WorktreeChanged);
    assert_eq!(app.selected_hash().map(str::to_string), before);
}

#[test]
fn checkout_in_linked_worktree_emits_refs_changed() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("side");
    let dir = tempfile::tempdir().unwrap();
    let wt = dir.path().join("wt");
    let wt_str = wt.to_str().unwrap();
    repo.git(&["worktree", "add", "-b", "linked", wt_str]);
    let (tx, rx) = channel();
    let _handle = watch(&Git::at(&wt), tx).expect("start watcher");
    sleep(Duration::from_millis(500));
    while rx.try_recv().is_ok() {}
    repo.git(&["-C", wt_str, "checkout", "side"]);
    assert!(wait_for(&rx, WatchEvent::RefsChanged));
}

#[test]
fn packed_refs_change_emits_refs_changed() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("side");
    repo.git(&["pack-refs", "--all"]);
    let packed = repo.path().join(".git").join("packed-refs");
    let content = fs::read_to_string(&packed).unwrap();
    let (tx, rx) = channel();
    let _handle = watch(&Git::at(repo.path()), tx).expect("start watcher");
    sleep(Duration::from_millis(500));
    while rx.try_recv().is_ok() {}
    fs::write(&packed, content).unwrap();
    assert!(wait_for(&rx, WatchEvent::RefsChanged));
}

#[test]
fn on_watch_in_bare_repo_skips_status() {
    let repo = TestRepo::new();
    repo.commit("first");
    let remote = repo.add_bare_remote("origin");
    let mut app = App::new(Git::at(remote.path()), ColorDepth::Indexed256).unwrap();
    app.on_watch(WatchEvent::WorktreeChanged);
    assert_eq!(app.refresh_error(), None);
    app.on_watch(WatchEvent::RefsChanged);
    assert_eq!(app.refresh_error(), None);
    assert!(app.selected_hash().is_some());
}

#[test]
fn failed_refresh_keeps_view_and_reports_error() {
    let repo = TestRepo::new();
    repo.commit("first");
    let top = repo.commit("second");
    let mut app = App::new(Git::at(repo.path()), ColorDepth::Indexed256).unwrap();
    let dot_git = repo.path().join(".git");
    let away = repo.path().join("git-away");

    // Git cannot read the repo mid-operation: both refreshes fail, the app stays up.
    fs::rename(&dot_git, &away).unwrap();
    app.on_watch(WatchEvent::RefsChanged);
    assert!(app.refresh_error().is_some());
    assert_eq!(app.selected_hash(), Some(top.as_str()));
    app.on_watch(WatchEvent::WorktreeChanged);
    assert!(app.refresh_error().is_some());
    assert_eq!(app.selected_hash(), Some(top.as_str()));

    // The next successful refresh clears the error.
    fs::rename(&away, &dot_git).unwrap();
    app.on_watch(WatchEvent::RefsChanged);
    assert_eq!(app.refresh_error(), None);
    assert_eq!(app.selected_hash(), Some(top.as_str()));
}

#[test]
fn worktree_change_closes_open_menu() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.commit("second");
    let mut app = App::new(Git::at(repo.path()), ColorDepth::Indexed256).unwrap();
    app.apply(Command::OpenMenuAt {
        pos: (5, 2),
        hit: Hit::Row(1),
    })
    .unwrap();
    assert!(app.menu().is_some());

    // The uncommitted row shifts every row down by one.
    fs::write(repo.path().join("new.txt"), "new\n").unwrap();
    app.on_watch(WatchEvent::WorktreeChanged);
    assert!(app.menu().is_none());
}

#[test]
fn refs_change_closes_open_menu() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.commit("second");
    let mut app = App::new(Git::at(repo.path()), ColorDepth::Indexed256).unwrap();
    app.apply(Command::OpenMenuAt {
        pos: (5, 2),
        hit: Hit::Row(1),
    })
    .unwrap();
    assert!(app.menu().is_some());

    repo.commit("third");
    app.on_watch(WatchEvent::RefsChanged);
    assert!(app.menu().is_none());
}
