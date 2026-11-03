mod support;

use support::TestRepo;
use terminal_git_graph::git::log::{count_commits, load_commits, LogQuery, Scope, PAGE_SIZE};
use terminal_git_graph::git::refs::{load_refs, Head, RefKind};
use terminal_git_graph::git::Git;

fn query(scope: Scope, remotes: bool, skip: usize, limit: usize) -> LogQuery {
    LogQuery { scope, remotes, skip, limit }
}

#[test]
fn loads_commits_newest_first_with_parents() {
    let repo = TestRepo::new();
    let a = repo.commit("a");
    let b = repo.commit("b");
    let c = repo.commit("c");
    let git = Git::at(repo.path());
    let commits = load_commits(&git, &query(Scope::AllBranches, true, 0, PAGE_SIZE)).unwrap();
    let hashes: Vec<&str> = commits.iter().map(|c| c.hash.as_str()).collect();
    assert_eq!(hashes, vec![c.as_str(), b.as_str(), a.as_str()]);
    assert_eq!(commits[0].parents, vec![b.clone()]);
    assert_eq!(commits[1].parents, vec![a.clone()]);
    assert!(commits[2].parents.is_empty());
    assert_eq!(commits[0].subject, "c");
    assert_eq!(commits[0].author, "Test User");
    assert_eq!(commits[0].email, "test@example.com");
    let ct: i64 = repo.git(&["log", "-1", "--format=%ct"]).trim().parse().unwrap();
    assert_eq!(commits[0].timestamp, ct);
}

#[test]
fn paging_with_skip_and_limit() {
    let repo = TestRepo::new();
    for i in 1..=25 {
        repo.commit(&format!("c{i}"));
    }
    let git = Git::at(repo.path());
    let page = load_commits(&git, &query(Scope::AllBranches, true, 10, 10)).unwrap();
    let subjects: Vec<&str> = page.iter().map(|c| c.subject.as_str()).collect();
    let expected: Vec<String> = (6..=15).rev().map(|i| format!("c{i}")).collect();
    assert_eq!(subjects, expected);
}

#[test]
fn count_matches_rev_list() {
    let repo = TestRepo::new();
    for i in 1..=7 {
        repo.commit(&format!("c{i}"));
    }
    repo.branch("side");
    repo.checkout("side");
    repo.commit("s1");
    repo.checkout("main");
    let git = Git::at(repo.path());
    assert_eq!(count_commits(&git, Scope::AllBranches, true).unwrap(), 8);
    assert_eq!(count_commits(&git, Scope::Current, true).unwrap(), 7);
    let expected: usize = repo
        .git(&["rev-list", "--count", "--branches", "--tags"])
        .trim()
        .parse()
        .unwrap();
    assert_eq!(count_commits(&git, Scope::AllBranches, false).unwrap(), expected);
}

#[test]
fn current_scope_excludes_other_branches() {
    let repo = TestRepo::new();
    repo.commit("base");
    repo.branch("side");
    repo.checkout("side");
    repo.commit("only-on-side");
    repo.checkout("main");
    repo.commit("on-main");
    let git = Git::at(repo.path());
    let cur = load_commits(&git, &query(Scope::Current, true, 0, 100)).unwrap();
    assert!(cur.iter().all(|c| c.subject != "only-on-side"));
    assert_eq!(cur.len(), 2);
    let all = load_commits(&git, &query(Scope::AllBranches, true, 0, 100)).unwrap();
    assert!(all.iter().any(|c| c.subject == "only-on-side"));
}

#[test]
fn remotes_off_hides_commits_only_reachable_from_remotes() {
    let repo = TestRepo::new();
    repo.commit("base");
    let _bare = repo.add_bare_remote("origin");
    repo.git(&["fetch", "origin"]);
    // Move a commit onto a remote-tracking ref only.
    repo.commit("remote-only");
    repo.git(&["update-ref", "refs/remotes/origin/extra", "HEAD"]);
    repo.git(&["reset", "--hard", "HEAD~1"]);
    let git = Git::at(repo.path());
    let with = load_commits(&git, &query(Scope::AllBranches, true, 0, 100)).unwrap();
    let without = load_commits(&git, &query(Scope::AllBranches, false, 0, 100)).unwrap();
    assert!(with.iter().any(|c| c.subject == "remote-only"));
    assert!(without.iter().all(|c| c.subject != "remote-only"));
    assert_eq!(with.len(), 2);
    assert_eq!(without.len(), 1);
}

#[test]
fn subject_with_tab_quote_and_unicode_roundtrips() {
    let repo = TestRepo::new();
    let subject = "tab\there \"quoted\" 'single' 한글 🚀 ünï";
    repo.commit(subject);
    let git = Git::at(repo.path());
    let commits = load_commits(&git, &query(Scope::AllBranches, true, 0, 10)).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(commits[0].subject, subject);
}

#[test]
fn tags_are_loaded_as_labels() {
    let repo = TestRepo::new();
    let a = repo.commit("a");
    repo.tag("v1");
    repo.git(&["tag", "-a", "v2", "-m", "annotated"]);
    let git = Git::at(repo.path());
    let refs = load_refs(&git).unwrap();
    let tags: Vec<_> = refs.labels.iter().filter(|l| l.kind == RefKind::Tag).collect();
    assert_eq!(tags.len(), 2);
    for t in &tags {
        assert_eq!(t.target, a, "tag {} should resolve to the commit", t.name);
    }
    assert!(tags.iter().any(|t| t.name == "v1"));
    assert!(tags.iter().any(|t| t.name == "v2"));
    assert_eq!(refs.labels_for(&a).len(), 3); // main + v1 + v2
}

#[test]
fn local_branch_folds_matching_remote() {
    let repo = TestRepo::new();
    let a = repo.commit("a");
    let _bare = repo.add_bare_remote("origin");
    repo.git(&["fetch", "origin"]);
    let git = Git::at(repo.path());
    let refs = load_refs(&git).unwrap();
    let main = refs
        .labels
        .iter()
        .find(|l| l.name == "main" && l.kind == RefKind::LocalBranch)
        .unwrap();
    assert_eq!(main.remotes, vec!["origin".to_string()]);
    assert_eq!(main.target, a);
    assert!(refs.labels.iter().all(|l| l.kind != RefKind::RemoteBranch));
}

#[test]
fn remote_only_branch_has_own_label() {
    let repo = TestRepo::new();
    repo.commit("a");
    let _bare = repo.add_bare_remote("origin");
    repo.git(&["branch", "feature"]);
    repo.git(&["push", "origin", "feature"]);
    repo.git(&["branch", "-D", "feature"]);
    repo.git(&["fetch", "origin"]);
    let git = Git::at(repo.path());
    let refs = load_refs(&git).unwrap();
    let remote: Vec<_> = refs
        .labels
        .iter()
        .filter(|l| l.kind == RefKind::RemoteBranch)
        .collect();
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0].name, "origin/feature");
}

#[test]
fn empty_repo_has_no_commits_and_unborn_head() {
    let repo = TestRepo::new();
    let git = Git::at(repo.path());
    for scope in [Scope::AllBranches, Scope::Current] {
        assert!(load_commits(&git, &query(scope, true, 0, 10)).unwrap().is_empty());
        assert_eq!(count_commits(&git, scope, true).unwrap(), 0);
    }
    let refs = load_refs(&git).unwrap();
    assert_eq!(refs.head, Head::Unborn("main".to_string()));
    assert!(refs.labels.is_empty());
}

#[test]
fn detached_head_is_reported() {
    let repo = TestRepo::new();
    let a = repo.commit("a");
    repo.commit("b");
    repo.git(&["checkout", "--detach", &a]);
    let git = Git::at(repo.path());
    let refs = load_refs(&git).unwrap();
    assert_eq!(refs.head, Head::Detached(a));
}

#[test]
fn branch_head_is_reported() {
    let repo = TestRepo::new();
    repo.commit("a");
    let git = Git::at(repo.path());
    assert_eq!(load_refs(&git).unwrap().head, Head::Branch("main".to_string()));
}

#[test]
fn file_named_head_does_not_hide_real_log() {
    let repo = TestRepo::new();
    repo.commit_file("HEAD", "x\n", "add file named HEAD");
    repo.commit("second");
    let git = Git::at(repo.path());
    let commits = load_commits(&git, &query(Scope::Current, true, 0, PAGE_SIZE)).unwrap();
    assert_eq!(commits.len(), 2);
    assert_eq!(count_commits(&git, Scope::Current, true).unwrap(), 2);
}

#[test]
fn unborn_head_is_empty_even_under_non_english_locale() {
    // Git::run forces LC_ALL=C, so a caller's locale must not break unborn detection.
    // env mutation is process-wide, but Git::run overrides LC_ALL for its children anyway.
    std::env::set_var("LC_ALL", "ko_KR.UTF-8");
    let repo = TestRepo::new();
    let git = Git::at(repo.path());
    let commits = load_commits(&git, &query(Scope::Current, true, 0, PAGE_SIZE)).unwrap();
    assert!(commits.is_empty());
    assert_eq!(count_commits(&git, Scope::Current, true).unwrap(), 0);
}

#[test]
fn all_branches_includes_detached_head_commit() {
    let repo = TestRepo::new();
    repo.commit("base");
    repo.git(&["checkout", "--detach"]);
    let detached = repo.commit("only reachable from HEAD");
    repo.checkout("main");
    repo.git(&["checkout", "--detach", &detached]);
    let git = Git::at(repo.path());
    let commits = load_commits(&git, &query(Scope::AllBranches, true, 0, PAGE_SIZE)).unwrap();
    assert!(commits.iter().any(|c| c.hash == detached), "{commits:?}");
    assert_eq!(count_commits(&git, Scope::AllBranches, true).unwrap(), 2);
}

#[test]
fn all_branches_with_unborn_head_still_lists_branches() {
    let repo = TestRepo::new();
    repo.commit("on main");
    repo.git(&["checkout", "--orphan", "fresh"]);
    let git = Git::at(repo.path());
    let commits = load_commits(&git, &query(Scope::AllBranches, true, 0, PAGE_SIZE)).unwrap();
    assert_eq!(commits.len(), 1);
    assert_eq!(count_commits(&git, Scope::AllBranches, true).unwrap(), 1);
}
