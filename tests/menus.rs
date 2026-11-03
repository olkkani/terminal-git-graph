mod support;

use std::fs;
use std::sync::mpsc::TryRecvError;
use std::time::{Duration, Instant};

use support::TestRepo;
use terminal_git_graph::actions::{Action, ResetMode};
use terminal_git_graph::app::{App, Command};
use terminal_git_graph::git::refs::{load_refs, Head};
use terminal_git_graph::git::Git;
use terminal_git_graph::input::Hit;
use terminal_git_graph::jobs::JobRunner;
use terminal_git_graph::menus::{menu_for, MenuCtx, Target};
use terminal_git_graph::ui::dialog::Dialog;
use terminal_git_graph::ui::menu::{MenuId, MenuItem};
use terminal_git_graph::ui::theme::ColorDepth;

fn s(v: &str) -> String {
    v.to_string()
}

fn ctx(head: Head) -> MenuCtx {
    MenuCtx {
        head,
        has_staged: true,
        remotes: vec![s("origin")],
        subject: s("the subject"),
    }
}

fn on_main() -> MenuCtx {
    ctx(Head::Branch(s("main")))
}

fn labels(items: &[MenuItem]) -> Vec<&str> {
    items.iter().map(|i| i.label.as_str()).collect()
}

fn item<'a>(items: &'a [MenuItem], label: &str) -> &'a MenuItem {
    items
        .iter()
        .find(|i| i.label == label)
        .unwrap_or_else(|| panic!("no {label:?} in {:?}", labels(items)))
}

fn local(name: &str) -> Target {
    Target::LocalBranch(s(name))
}

fn remote(name: &str) -> Target {
    Target::RemoteBranch {
        remote: s("origin"),
        name: s(name),
    }
}

#[test]
fn menu_labels_match_spec_for_each_target() {
    let c = on_main();
    assert_eq!(
        labels(&menu_for(&local("feature"), &c)),
        [
            "Checkout",
            "Rename",
            "Delete",
            "Merge into current",
            "Rebase onto current",
            "Push",
            "Copy name"
        ]
    );
    assert_eq!(
        labels(&menu_for(&remote("feature"), &c)),
        ["Checkout", "Merge", "Delete on remote", "Copy name"]
    );
    assert_eq!(
        labels(&menu_for(&Target::Tag(s("v1")), &c)),
        ["Delete", "Push", "Copy name"]
    );
    assert_eq!(
        labels(&menu_for(&Target::Commit(s("abc1234def")), &c)),
        [
            "Add tag",
            "Create branch",
            "Checkout",
            "Cherry pick",
            "Revert",
            "Merge",
            "Rebase",
            "Reset (Soft)",
            "Reset (Mixed)",
            "Reset (Hard)",
            "Copy hash",
            "Copy subject"
        ]
    );
    assert_eq!(
        labels(&menu_for(&Target::Uncommitted, &c)),
        [
            "Commit (stage all)",
            "Commit (staged only)",
            "Stash",
            "Reset"
        ]
    );
}

#[test]
fn menu_items_carry_their_actions() {
    let c = on_main();
    let items = menu_for(&local("feature"), &c);
    assert_eq!(
        item(&items, "Checkout").id,
        MenuId::Do(Action::CheckoutBranch(s("feature")))
    );
    assert_eq!(
        item(&items, "Merge into current").id,
        MenuId::Do(Action::MergeInto(s("feature")))
    );
    assert_eq!(item(&items, "Copy name").id, MenuId::Copy(s("feature")));

    let items = menu_for(&remote("feature"), &c);
    assert_eq!(
        item(&items, "Delete on remote").id,
        MenuId::Do(Action::DeleteRemoteBranch {
            remote: s("origin"),
            name: s("feature")
        })
    );
    assert_eq!(
        item(&items, "Copy name").id,
        MenuId::Copy(s("origin/feature"))
    );

    let items = menu_for(&Target::Commit(s("abc1234def")), &c);
    assert_eq!(
        item(&items, "Reset (Mixed)").id,
        MenuId::Do(Action::Reset {
            hash: s("abc1234def"),
            mode: ResetMode::Mixed
        })
    );
    assert_eq!(item(&items, "Copy hash").id, MenuId::Copy(s("abc1234def")));
    assert_eq!(
        item(&items, "Copy subject").id,
        MenuId::Copy(s("the subject"))
    );
}

#[test]
fn current_branch_delete_merge_rebase_disabled() {
    let items = menu_for(&local("main"), &on_main());
    for label in ["Delete", "Merge into current", "Rebase onto current"] {
        assert!(!item(&items, label).enabled, "{label} enabled");
    }
    for label in ["Checkout", "Rename", "Push", "Copy name"] {
        assert!(item(&items, label).enabled, "{label} disabled");
    }
    let other = menu_for(&local("feature"), &on_main());
    assert!(other.iter().all(|i| i.enabled), "{other:?}");
}

#[test]
fn detached_head_disables_current_branch_items() {
    let c = ctx(Head::Detached(s("abc1234def")));
    let items = menu_for(&local("feature"), &c);
    for label in ["Merge into current", "Rebase onto current", "Push"] {
        assert!(!item(&items, label).enabled, "local {label} enabled");
    }
    for label in ["Checkout", "Rename", "Delete", "Copy name"] {
        assert!(item(&items, label).enabled, "local {label} disabled");
    }
    assert!(!item(&menu_for(&remote("feature"), &c), "Merge").enabled);
    let items = menu_for(&Target::Commit(s("abc1234def")), &c);
    for label in ["Merge", "Rebase"] {
        assert!(!item(&items, label).enabled, "commit {label} enabled");
    }
    assert!(item(&items, "Checkout").enabled);
}

#[test]
fn push_disabled_without_remotes() {
    let c = MenuCtx {
        remotes: Vec::new(),
        ..on_main()
    };
    assert!(!item(&menu_for(&local("feature"), &c), "Push").enabled);
    assert!(!item(&menu_for(&Target::Tag(s("v1")), &c), "Push").enabled);
}

#[test]
fn commit_staged_only_disabled_without_staged() {
    let c = MenuCtx {
        has_staged: false,
        ..on_main()
    };
    let items = menu_for(&Target::Uncommitted, &c);
    assert!(!item(&items, "Commit (staged only)").enabled);
    assert!(item(&items, "Commit (stage all)").enabled);
    assert!(
        item(
            &menu_for(&Target::Uncommitted, &on_main()),
            "Commit (staged only)"
        )
        .enabled
    );
}

#[test]
fn reset_items_are_danger() {
    let c = on_main();
    let items = menu_for(&Target::Commit(s("abc1234def")), &c);
    for label in ["Reset (Soft)", "Reset (Mixed)", "Reset (Hard)"] {
        assert!(item(&items, label).danger, "{label} not danger");
    }
    assert!(!item(&items, "Checkout").danger);
    assert!(item(&menu_for(&Target::Uncommitted, &c), "Reset").danger);
}

// ---- App flows ----

fn app_for(repo: &TestRepo) -> App {
    App::new(Git::at(repo.path()), ColorDepth::TrueColor).expect("open app")
}

/// Polls the running job until it finishes.
fn wait_job(app: &mut App) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !app.poll_job() {
        assert!(Instant::now() < deadline, "job did not finish");
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// The hit for the ref label named `name`, as a right click on it would report.
fn label_hit(app: &mut App, repo: &TestRepo, name: &str) -> Hit {
    let refs = load_refs(&Git::at(repo.path())).unwrap();
    let target = refs
        .labels
        .iter()
        .find(|l| l.name == name)
        .unwrap_or_else(|| panic!("no label {name}"))
        .target
        .clone();
    let row = (0..50)
        .find(|&r| {
            app.apply(Command::SelectRow(r)).unwrap();
            app.selected_hash() == Some(target.as_str())
        })
        .expect("row of label");
    let index = refs
        .labels_for(&target)
        .iter()
        .position(|l| l.name == name)
        .unwrap();
    Hit::Label { row, index }
}

fn open_menu(app: &mut App, hit: Hit) {
    app.apply(Command::OpenMenuAt { pos: (0, 0), hit }).unwrap();
    assert!(app.menu().is_some(), "menu did not open for {hit:?}");
}

fn pick(app: &mut App, label: &str) {
    let menu = app.menu().expect("menu open");
    let i = menu
        .items
        .iter()
        .position(|i| i.label == label)
        .unwrap_or_else(|| panic!("no {label}"));
    app.apply(Command::MenuPick(i)).unwrap();
}

fn current_branch(repo: &TestRepo) -> String {
    repo.git(&["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_string()
}

#[test]
fn remote_checkout_without_local_opens_input_dialog_with_default_name() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("feature");
    repo.checkout("feature");
    repo.commit("on feature");
    repo.checkout("main");
    let _bare = repo.add_bare_remote("origin");
    repo.git(&["branch", "-D", "feature"]);
    let mut app = app_for(&repo);

    let hit = label_hit(&mut app, &repo, "origin/feature");
    open_menu(&mut app, hit);
    pick(&mut app, "Checkout");
    match &app.dialog().expect("dialog").dialog {
        Dialog::Input { value, .. } => assert_eq!(value, "feature"),
        d => panic!("expected input dialog, got {d:?}"),
    }
    assert!(!app.job_running());

    // Accepting the name asks for confirmation, then checks out a tracking branch.
    app.apply(Command::DialogAccept).unwrap();
    assert!(matches!(
        app.dialog().map(|d| &d.dialog),
        Some(Dialog::Confirm { .. })
    ));
    app.apply(Command::DialogAccept).unwrap();
    wait_job(&mut app);
    assert!(app.dialog().is_none(), "{:?}", app.dialog());
    assert_eq!(current_branch(&repo), "feature");
}

#[test]
fn double_click_on_remote_label_asks_local_name() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("feature");
    let _bare = repo.add_bare_remote("origin");
    repo.git(&["branch", "-D", "feature"]);
    let mut app = app_for(&repo);
    let Hit::Label { row, index } = label_hit(&mut app, &repo, "origin/feature") else {
        unreachable!()
    };
    app.apply(Command::ActivateLabel { row, index }).unwrap();
    match &app.dialog().expect("dialog").dialog {
        Dialog::Input { value, .. } => assert_eq!(value, "feature"),
        d => panic!("expected input dialog, got {d:?}"),
    }
}

#[test]
fn double_click_on_local_label_checks_out_without_dialog() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("feature");
    repo.commit("second");
    let mut app = app_for(&repo);
    let Hit::Label { row, index } = label_hit(&mut app, &repo, "feature") else {
        unreachable!()
    };
    app.apply(Command::ActivateLabel { row, index }).unwrap();
    assert!(app.dialog().is_none());
    wait_job(&mut app);
    assert_eq!(current_branch(&repo), "feature");
    assert!(!app.detail_open());
}

#[test]
fn confirm_dialog_body_for_reset_worktree_mentions_untracked_kept() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    fs::write(repo.path().join("a.txt"), "changed\n").unwrap();
    fs::write(repo.path().join("new.txt"), "untracked\n").unwrap();
    let mut app = app_for(&repo);

    open_menu(&mut app, Hit::Row(0));
    pick(&mut app, "Reset");
    match &app.dialog().expect("dialog").dialog {
        Dialog::Confirm { body, danger, .. } => {
            assert!(body.contains("Untracked files are kept."), "{body}");
            assert!(danger);
        }
        d => panic!("expected confirm dialog, got {d:?}"),
    }
    app.apply(Command::DialogAccept).unwrap();
    wait_job(&mut app);
    assert_eq!(
        fs::read_to_string(repo.path().join("a.txt")).unwrap(),
        "one\n"
    );
    assert!(repo.path().join("new.txt").exists());
}

#[test]
fn cancelling_a_confirm_dialog_runs_nothing() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("feature");
    let mut app = app_for(&repo);
    let hit = label_hit(&mut app, &repo, "feature");
    open_menu(&mut app, hit);
    pick(&mut app, "Delete");
    app.apply(Command::DialogCancel).unwrap();
    assert!(app.dialog().is_none());
    assert!(!app.job_running());
    assert!(repo
        .git(&["branch", "--list", "feature"])
        .contains("feature"));
}

#[test]
fn failed_action_shows_error_dialog_with_git_message() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("feature");
    repo.checkout("feature");
    repo.commit("only on feature");
    repo.checkout("main");
    let mut app = app_for(&repo);

    let hit = label_hit(&mut app, &repo, "feature");
    open_menu(&mut app, hit);
    pick(&mut app, "Delete");
    assert!(matches!(
        app.dialog().map(|d| &d.dialog),
        Some(Dialog::Confirm { danger: true, .. })
    ));
    app.apply(Command::DialogAccept).unwrap();
    wait_job(&mut app);
    match &app.dialog().expect("error dialog").dialog {
        Dialog::Error { message, .. } => {
            assert!(message.contains("not fully merged"), "{message}")
        }
        d => panic!("expected error dialog, got {d:?}"),
    }
}

#[test]
fn push_without_remote_shows_friendly_message() {
    let repo = TestRepo::new();
    repo.commit("first");
    let mut app = app_for(&repo);
    app.start_action(Action::PushCurrent);
    wait_job(&mut app);
    match &app.dialog().expect("error dialog").dialog {
        Dialog::Error { message, .. } => assert_eq!(message, "No remote to push to"),
        d => panic!("expected error dialog, got {d:?}"),
    }
}

#[test]
fn input_dialog_value_fills_the_action() {
    let repo = TestRepo::new();
    let first = repo.commit("first");
    repo.commit("second");
    let mut app = app_for(&repo);
    let row = (0..5)
        .find(|&r| {
            app.apply(Command::SelectRow(r)).unwrap();
            app.selected_hash() == Some(first.as_str())
        })
        .unwrap();
    open_menu(&mut app, Hit::Row(row));
    pick(&mut app, "Create branch");
    for c in "topic".chars() {
        app.apply(Command::DialogChar(c)).unwrap();
    }
    app.apply(Command::DialogAccept).unwrap();
    match &app.dialog().expect("confirm").dialog {
        Dialog::Confirm { body, .. } => assert!(body.contains("topic"), "{body}"),
        d => panic!("expected confirm dialog, got {d:?}"),
    }
    app.apply(Command::DialogAccept).unwrap();
    wait_job(&mut app);
    assert_eq!(repo.git(&["rev-parse", "topic"]).trim(), first);
}

#[test]
fn local_checkout_and_fetch_run_without_dialog() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("feature");
    repo.commit("second");
    let _bare = repo.add_bare_remote("origin");
    let mut app = app_for(&repo);

    let hit = label_hit(&mut app, &repo, "feature");
    open_menu(&mut app, hit);
    pick(&mut app, "Checkout");
    assert!(app.dialog().is_none(), "{:?}", app.dialog());
    assert!(app.job_running());
    wait_job(&mut app);
    assert!(app.dialog().is_none(), "{:?}", app.dialog());
    assert_eq!(current_branch(&repo), "feature");

    app.apply(Command::Fetch).unwrap();
    assert!(app.dialog().is_none(), "{:?}", app.dialog());
    assert!(app.job_running());
    wait_job(&mut app);
    assert!(app.dialog().is_none(), "{:?}", app.dialog());
}

#[test]
fn help_opens_key_binding_dialog() {
    let repo = TestRepo::new();
    repo.commit("first");
    let mut app = app_for(&repo);
    app.apply(Command::Help).unwrap();
    match &app.dialog().expect("dialog").dialog {
        Dialog::Info { body, .. } => assert!(body.contains("Fetch"), "{body}"),
        d => panic!("expected info dialog, got {d:?}"),
    }
    app.apply(Command::DialogAccept).unwrap();
    assert!(app.dialog().is_none());
}

#[test]
fn job_runner_delivers_result_without_blocking() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("feature");
    let rx = JobRunner::new().spawn(Git::at(repo.path()), Action::CheckoutBranch(s("feature")));
    // spawn returns at once; the result arrives on the channel.
    match rx.try_recv() {
        Ok(_) | Err(TryRecvError::Empty) => {}
        Err(TryRecvError::Disconnected) => panic!("job dropped its sender"),
    }
    let result = rx
        .recv_timeout(Duration::from_secs(20))
        .expect("job result");
    assert!(result.is_ok(), "{result:?}");
    assert_eq!(current_branch(&repo), "feature");
}
