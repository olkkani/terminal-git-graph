mod support;

use support::TestRepo;
use terminal_git_graph::git::{validate_name, Git, GitError};

#[test]
fn run_returns_stdout() {
    let repo = TestRepo::new();
    let sha = repo.commit("first");
    let git = Git::at(repo.path());
    assert_eq!(git.run(&["rev-parse", "HEAD"]).unwrap().trim(), sha);
}

#[test]
fn failed_command_exposes_stderr_and_code() {
    let repo = TestRepo::new();
    repo.commit("first");
    let git = Git::at(repo.path());
    let err = git.run(&["checkout", "no-such-branch"]).unwrap_err();
    match &err {
        GitError::Failed { code, stderr, .. } => {
            assert!(code.is_some());
            assert!(!stderr.is_empty());
        }
        other => panic!("expected Failed, got {other:?}"),
    }
    assert!(!err.message().is_empty());
}

#[test]
fn merge_conflict_message_comes_from_stdout() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "base\n", "base");
    repo.branch("topic");
    repo.commit_file("a.txt", "main side\n", "main change");
    repo.checkout("topic");
    repo.commit_file("a.txt", "topic side\n", "topic change");
    repo.checkout("main");
    let git = Git::at(repo.path());
    let err = git.run(&["merge", "topic"]).unwrap_err();
    assert!(err.message().contains("CONFLICT"), "{}", err.message());
}

#[test]
fn validate_name_rejects_dash_empty_and_whitespace() {
    for bad in ["", "-x", "--force", "a b", "a\tb", "a\nb", "a\u{7f}b"] {
        assert!(
            matches!(validate_name(bad), Err(GitError::InvalidName(_))),
            "{bad:?} should be rejected"
        );
    }
}

#[test]
fn validate_name_accepts_feature_slash_names() {
    for good in ["feature/deploy", "main", "release-1.0", "v1.2.3"] {
        assert!(validate_name(good).is_ok(), "{good:?} should be accepted");
    }
}

#[test]
fn call_count_increments() {
    let repo = TestRepo::new();
    let git = Git::at(repo.path());
    assert_eq!(git.call_count(), 0);
    git.run(&["status"]).unwrap();
    git.run(&["status"]).unwrap();
    assert_eq!(git.call_count(), 2);
}
