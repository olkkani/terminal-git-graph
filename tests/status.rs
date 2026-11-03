mod support;

use support::TestRepo;
use terminal_git_graph::git::status::{load_status, parse_status_v2};
use terminal_git_graph::git::Git;

#[test]
fn parses_branch_ahead_behind() {
    let out = "# branch.oid abc\n# branch.head main\n# branch.upstream origin/main\n# branch.ab +2 -1\n";
    let s = parse_status_v2(out);
    assert_eq!(s.branch.as_deref(), Some("main"));
    assert!(s.has_upstream);
    assert_eq!((s.ahead, s.behind), (2, 1));
}

#[test]
fn no_upstream_is_flagged() {
    let s = parse_status_v2("# branch.oid abc\n# branch.head feature\n");
    assert_eq!(s.branch.as_deref(), Some("feature"));
    assert!(!s.has_upstream);
    assert_eq!((s.ahead, s.behind), (0, 0));
}

#[test]
fn detached_head_has_no_branch() {
    let s = parse_status_v2("# branch.oid abc\n# branch.head (detached)\n");
    assert_eq!(s.branch, None);
}

#[test]
fn counts_staged_unstaged_untracked() {
    let out = "\
1 M. N... 100644 100644 100644 aaa bbb staged.txt
1 .M N... 100644 100644 100644 aaa bbb unstaged.txt
? new.txt
? other.txt
";
    let s = parse_status_v2(out);
    assert_eq!((s.staged, s.unstaged, s.untracked), (1, 1, 2));
    assert_eq!(s.changed_total(), 4);
    assert!(!s.is_clean());
}

#[test]
fn renamed_entry_counts_once() {
    let out = "2 R. N... 100644 100644 100644 aaa bbb R100 new.txt\told.txt\n";
    let s = parse_status_v2(out);
    assert_eq!((s.staged, s.unstaged), (1, 0));
    assert_eq!(s.changed_total(), 1);
}

#[test]
fn file_in_both_index_and_worktree_counts_once() {
    let out = "1 MM N... 100644 100644 100644 aaa bbb both.txt\n";
    let s = parse_status_v2(out);
    assert_eq!((s.staged, s.unstaged), (1, 1));
    assert_eq!(s.changed_total(), 1);
}

#[test]
fn unmerged_entry_counts_once() {
    let out = "u UU N... 100644 100644 100644 100644 a b c conflict.txt\n";
    let s = parse_status_v2(out);
    assert_eq!(s.changed_total(), 1);
    assert!(!s.is_clean());
}

#[test]
fn clean_repo_is_clean() {
    let s = parse_status_v2("# branch.oid abc\n# branch.head main\n");
    assert!(s.is_clean());
    assert_eq!(s.changed_total(), 0);
}

#[test]
fn load_status_on_real_repo() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "first");
    let git = Git::at(repo.path());
    assert!(load_status(&git).unwrap().is_clean());
    std::fs::write(repo.path().join("a.txt"), "changed\n").unwrap();
    std::fs::write(repo.path().join("b.txt"), "new\n").unwrap();
    let s = load_status(&git).unwrap();
    assert_eq!(s.changed_total(), 2);
    assert_eq!((s.unstaged, s.untracked), (1, 1));
    assert_eq!(s.branch.as_deref(), Some("main"));
}
