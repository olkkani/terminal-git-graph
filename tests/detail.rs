mod support;

use std::fs;

use support::TestRepo;
use terminal_git_graph::git::detail::{
    load_detail, load_file_diff, load_worktree_changes, load_worktree_diff, DetailCache,
    MAX_UNTRACKED_BYTES,
};
use terminal_git_graph::git::Git;

#[test]
fn detail_lists_files_with_numstat() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    let h = repo.commit_file("a.txt", "one\ntwo\nthree\n", "second\n\nbody line");
    repo.commit_file("b.txt", "x\n", "third");
    let git = Git::at(repo.path());
    let d = load_detail(&git, &h).unwrap();
    assert_eq!(d.hash, h);
    assert_eq!(d.parents.len(), 1);
    assert_eq!(d.author, "Test User");
    assert_eq!(d.email, "test@example.com");
    assert!(d.message.starts_with("second"));
    assert!(d.message.contains("body line"));
    assert_eq!(d.files.len(), 1);
    assert_eq!(d.files[0].path, "a.txt");
    assert_eq!(d.files[0].status, 'M');
    assert_eq!((d.files[0].added, d.files[0].deleted), (Some(2), Some(0)));
}

#[test]
fn root_commit_lists_its_files() {
    let repo = TestRepo::new();
    let h = repo.commit_file("a.txt", "one\n", "root");
    let d = load_detail(&Git::at(repo.path()), &h).unwrap();
    assert!(d.parents.is_empty());
    assert_eq!(d.files.len(), 1);
    assert_eq!(d.files[0].status, 'A');
}

#[test]
fn merge_commit_diffs_against_first_parent() {
    let repo = TestRepo::new();
    repo.commit_file("base.txt", "b\n", "base");
    repo.git(&["checkout", "-b", "side"]);
    repo.commit_file("side.txt", "s\n", "side");
    repo.checkout("main");
    repo.commit_file("main.txt", "m\n", "main");
    repo.merge("side", true);
    let h = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    let d = load_detail(&Git::at(repo.path()), &h).unwrap();
    assert_eq!(d.parents.len(), 2);
    let paths: Vec<&str> = d.files.iter().map(|f| f.path.as_str()).collect();
    assert_eq!(paths, vec!["side.txt"]);
}

#[test]
fn binary_file_counts_are_none() {
    let repo = TestRepo::new();
    fs::write(repo.path().join("bin.dat"), [0u8, 1, 2, 0, 255]).unwrap();
    repo.git(&["add", "bin.dat"]);
    repo.git(&["commit", "-m", "bin"]);
    let h = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    let d = load_detail(&Git::at(repo.path()), &h).unwrap();
    assert_eq!(d.files.len(), 1);
    assert_eq!((d.files[0].added, d.files[0].deleted), (None, None));
}

#[test]
fn file_diff_is_unified_diff() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    let h = repo.commit_file("a.txt", "one\ntwo\n", "second");
    let diff = load_file_diff(&Git::at(repo.path()), &h, "a.txt").unwrap();
    assert!(diff.contains("@@"));
    assert!(diff.contains("+two"));
}

#[test]
fn worktree_changes_include_untracked_files() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    fs::write(repo.path().join("a.txt"), "one\nmore\n").unwrap();
    fs::write(repo.path().join("new.txt"), "hello\n").unwrap();
    let changes = load_worktree_changes(&Git::at(repo.path())).unwrap();
    let a = changes.iter().find(|c| c.path == "a.txt").unwrap();
    assert_eq!(a.status, 'M');
    let n = changes.iter().find(|c| c.path == "new.txt").unwrap();
    assert_eq!(n.status, '?');
}

#[test]
fn untracked_content_is_returned() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    fs::write(repo.path().join("new.txt"), "hello\n").unwrap();
    let git = Git::at(repo.path());
    let changes = load_worktree_changes(&git).unwrap();
    let n = changes.iter().find(|c| c.path == "new.txt").unwrap();
    assert_eq!(load_worktree_diff(&git, n).unwrap(), "hello\n");
}

#[test]
fn tracked_worktree_diff_is_unified() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    fs::write(repo.path().join("a.txt"), "one\nmore\n").unwrap();
    let git = Git::at(repo.path());
    let changes = load_worktree_changes(&git).unwrap();
    let diff = load_worktree_diff(&git, &changes[0]).unwrap();
    assert!(diff.contains("+more"));
}

#[test]
fn untracked_content_truncated_at_limit() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    fs::write(repo.path().join("big.txt"), vec![b'x'; 2 * 1024 * 1024]).unwrap();
    let git = Git::at(repo.path());
    let changes = load_worktree_changes(&git).unwrap();
    let c = changes.iter().find(|c| c.path == "big.txt").unwrap();
    let text = load_worktree_diff(&git, c).unwrap();
    assert!(text.ends_with("[truncated]"));
    assert!(text.len() <= MAX_UNTRACKED_BYTES + "[truncated]".len() + 1);
}

#[test]
fn detail_cache_hit_makes_no_second_git_call() {
    let repo = TestRepo::new();
    let h = repo.commit_file("a.txt", "one\n", "first");
    let git = Git::at(repo.path());
    let mut cache = DetailCache::new();
    cache.get_or_load(&git, &h).unwrap();
    let calls = git.call_count();
    assert_eq!(cache.get_or_load(&git, &h).unwrap().hash, h);
    assert_eq!(git.call_count(), calls);
}

#[test]
fn user_git_config_does_not_affect_diff_output() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    let h = repo.commit_file("a.txt", "one\ntwo\n", "second");
    repo.git(&["config", "color.ui", "always"]);
    repo.git(&["config", "color.diff", "always"]);
    repo.git(&["config", "diff.external", "false"]);
    let git = Git::at(repo.path());
    let d = load_file_diff(&git, &h, "a.txt").unwrap();
    assert!(d.contains("@@") && !d.contains('\x1b'), "{d:?}");
    fs::write(repo.path().join("a.txt"), "one\ntwo\nthree\n").unwrap();
    let changes = load_worktree_changes(&git).unwrap();
    let w = load_worktree_diff(&git, &changes[0]).unwrap();
    assert!(w.contains("@@") && !w.contains('\x1b'), "{w:?}");
}
