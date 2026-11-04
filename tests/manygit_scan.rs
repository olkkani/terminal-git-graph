mod support;

use std::fs;
use std::path::{Path, PathBuf};

use support::TestRepo;
use terminal_git_graph::manygit::scan::{scan, ScanError};
use terminal_git_graph::manygit::status::{repo_status, SyncState};

fn mkrepo(path: &Path) {
    fs::create_dir_all(path.join(".git")).unwrap();
}

fn names(entries: &[terminal_git_graph::manygit::RepoEntry]) -> Vec<String> {
    let mut v: Vec<String> = entries.iter().map(|e| e.name.clone()).collect();
    v.sort();
    v
}

fn home() -> PathBuf {
    PathBuf::from("/nonexistent-home-for-tests")
}

#[test]
fn finds_nested_repos_up_to_depth() {
    let tmp = tempfile::tempdir().unwrap();
    let mut p = tmp.path().to_path_buf();
    for d in 1..=5 {
        mkrepo(&p.join(format!("repo{d}")));
        p = p.join(format!("d{d}"));
    }
    for depth in 1..=5u8 {
        let found = scan(tmp.path(), depth, &home()).unwrap();
        let got = names(&found);
        let want: Vec<String> = (1..=depth).map(|d| format!("repo{d}")).collect();
        assert_eq!(got, want, "depth {depth}");
    }
}

#[test]
fn does_not_descend_into_repos() {
    let tmp = tempfile::tempdir().unwrap();
    mkrepo(&tmp.path().join("outer"));
    mkrepo(&tmp.path().join("outer/inner"));
    let found = scan(tmp.path(), 3, &home()).unwrap();
    assert_eq!(names(&found), vec!["outer"]);
}

#[test]
fn skips_hidden_and_build_folders() {
    let tmp = tempfile::tempdir().unwrap();
    for skipped in [
        ".hidden",
        "node_modules",
        "build",
        "dist",
        "vendor",
        "target",
    ] {
        mkrepo(&tmp.path().join(skipped).join("r"));
    }
    mkrepo(&tmp.path().join("ok"));
    let found = scan(tmp.path(), 3, &home()).unwrap();
    assert_eq!(names(&found), vec!["ok"]);
}

#[test]
fn refuses_home_and_its_ancestors() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path().join("users/me");
    fs::create_dir_all(&home).unwrap();
    assert!(matches!(
        scan(&home, 3, &home),
        Err(ScanError::RootIsHomeOrAncestor)
    ));
    assert!(matches!(
        scan(&tmp.path().join("users"), 3, &home),
        Err(ScanError::RootIsHomeOrAncestor)
    ));
    assert!(matches!(
        scan(tmp.path(), 3, &home),
        Err(ScanError::RootIsHomeOrAncestor)
    ));
    // A subfolder of home is fine.
    fs::create_dir_all(home.join("code")).unwrap();
    assert!(scan(&home.join("code"), 3, &home).is_ok());
}

#[test]
fn linked_worktree_dot_git_file_is_detected() {
    let tmp = tempfile::tempdir().unwrap();
    let wt = tmp.path().join("wt");
    fs::create_dir_all(&wt).unwrap();
    fs::write(wt.join(".git"), "gitdir: /somewhere/.git/worktrees/wt\n").unwrap();
    let found = scan(tmp.path(), 2, &home()).unwrap();
    assert_eq!(names(&found), vec!["wt"]);
}

#[test]
fn entries_grouped_by_parent_folder() {
    let tmp = tempfile::tempdir().unwrap();
    mkrepo(&tmp.path().join("a/one"));
    mkrepo(&tmp.path().join("a/two"));
    mkrepo(&tmp.path().join("b/three"));
    let found = scan(tmp.path(), 3, &home()).unwrap();
    assert_eq!(found.len(), 3);
    let one = found.iter().find(|e| e.name == "one").unwrap();
    let two = found.iter().find(|e| e.name == "two").unwrap();
    let three = found.iter().find(|e| e.name == "three").unwrap();
    assert_eq!(one.group, two.group);
    assert_ne!(one.group, three.group);
    assert_eq!(one.group, one.path.parent().unwrap());
    // Sorted by group then name.
    let order: Vec<&str> = found.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(order, vec!["one", "two", "three"]);
}

fn clone_of(repo: &TestRepo, remote: &tempfile::TempDir) -> tempfile::TempDir {
    let _ = repo;
    let clone = tempfile::tempdir().unwrap();
    let out = std::process::Command::new("git")
        .args([
            "clone",
            remote.path().to_str().unwrap(),
            clone.path().to_str().unwrap(),
        ])
        .output()
        .unwrap();
    assert!(out.status.success());
    for (k, v) in [
        ("user.name", "T"),
        ("user.email", "t@example.com"),
        ("commit.gpgsign", "false"),
    ] {
        std::process::Command::new("git")
            .args(["config", k, v])
            .current_dir(clone.path())
            .output()
            .unwrap();
    }
    clone
}

fn git_in(dir: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

#[test]
fn status_ahead_behind_diverged() {
    let repo = TestRepo::new();
    repo.commit("base");
    let remote = repo.add_bare_remote("origin");
    repo.git(&["branch", "--set-upstream-to=origin/main", "main"]);
    assert_eq!(repo_status(repo.path(), false).sync, SyncState::InSync);

    repo.commit("local 1");
    assert_eq!(repo_status(repo.path(), false).sync, SyncState::Ahead(1));

    // Another clone pushes two commits.
    let other = clone_of(&repo, &remote);
    fs::write(other.path().join("x"), "1").unwrap();
    git_in(other.path(), &["add", "x"]);
    git_in(other.path(), &["commit", "-m", "r1"]);
    fs::write(other.path().join("y"), "1").unwrap();
    git_in(other.path(), &["add", "y"]);
    git_in(other.path(), &["commit", "-m", "r2"]);
    git_in(other.path(), &["push", "origin", "main"]);
    repo.git(&["fetch", "origin"]);
    assert_eq!(
        repo_status(repo.path(), false).sync,
        SyncState::Diverged(1, 2)
    );

    repo.git(&["reset", "--hard", "origin/main"]);
    repo.git(&["reset", "--hard", "HEAD~2"]);
    assert_eq!(repo_status(repo.path(), false).sync, SyncState::Behind(2));
}

#[test]
fn status_not_pushed_when_no_upstream() {
    let repo = TestRepo::new();
    repo.commit("base");
    let _remote = repo.add_bare_remote("origin");
    repo.git(&["checkout", "-b", "feature"]);
    repo.commit("f");
    let st = repo_status(repo.path(), false);
    assert_eq!(st.branch.as_deref(), Some("feature"));
    assert_eq!(st.sync, SyncState::NotPushed);
}

#[test]
fn status_no_remote() {
    let repo = TestRepo::new();
    repo.commit("base");
    let st = repo_status(repo.path(), false);
    assert_eq!(st.branch.as_deref(), Some("main"));
    assert_eq!(st.sync, SyncState::NoRemote);
    assert!(!st.dirty);
}

#[test]
fn status_dirty_includes_untracked() {
    let repo = TestRepo::new();
    repo.commit("base");
    assert!(!repo_status(repo.path(), false).dirty);
    fs::write(repo.path().join("untracked.txt"), "x").unwrap();
    assert!(repo_status(repo.path(), false).dirty);
}

#[test]
fn latest_tag_only_when_requested() {
    let repo = TestRepo::new();
    repo.commit("base");
    repo.tag("v1.0");
    repo.commit("next");
    assert_eq!(repo_status(repo.path(), false).latest_tag, None);
    assert_eq!(
        repo_status(repo.path(), true).latest_tag.as_deref(),
        Some("v1.0")
    );
}

#[test]
fn status_polling_leaves_the_index_untouched() {
    let repo = TestRepo::new();
    repo.commit("base");
    let index = repo.path().join(".git/index");
    let past = std::time::SystemTime::now() - std::time::Duration::from_secs(100);
    // A tracked file whose stat no longer matches the index makes a plain
    // `git status` rewrite the index to refresh its stat cache.
    fs::File::options()
        .write(true)
        .open(repo.path().join("file-1.txt"))
        .unwrap()
        .set_modified(past + std::time::Duration::from_secs(50))
        .unwrap();
    fs::File::options()
        .write(true)
        .open(&index)
        .unwrap()
        .set_modified(past)
        .unwrap();
    let before = fs::metadata(&index).unwrap().modified().unwrap();

    repo_status(repo.path(), true);

    assert_eq!(fs::metadata(&index).unwrap().modified().unwrap(), before);
    assert!(!repo.path().join(".git/index.lock").exists());
}
