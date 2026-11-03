mod support;

use std::fs;
use std::path::Path;
use std::process::Command;

use support::TestRepo;
use tempfile::TempDir;
use terminal_git_graph::actions::{execute, needs_confirm, Action, ResetMode};
use terminal_git_graph::git::Git;

fn s(v: &str) -> String {
    v.to_string()
}

fn head(repo: &TestRepo) -> String {
    repo.git(&["rev-parse", "HEAD"]).trim().to_string()
}

fn current_branch(repo: &TestRepo) -> String {
    repo.git(&["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_string()
}

fn ref_exists(repo: &TestRepo, name: &str) -> bool {
    Command::new("git")
        .args(["rev-parse", "--verify", "--quiet", name])
        .current_dir(repo.path())
        .output()
        .expect("spawn git")
        .status
        .success()
}

fn git_in(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .expect("spawn git");
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8(out.stdout).unwrap()
}

/// Clones `bare`, commits a new file there and pushes it back to `main`. Returns the new hash.
fn push_from_other_clone(bare: &TempDir) -> String {
    let other = tempfile::tempdir().unwrap();
    let url = bare.path().to_str().unwrap();
    git_in(other.path(), &["clone", url, "."]);
    git_in(other.path(), &["config", "user.name", "Other"]);
    git_in(other.path(), &["config", "user.email", "other@example.com"]);
    git_in(other.path(), &["config", "commit.gpgsign", "false"]);
    // Write the current log so repeated calls always produce a change.
    let log = git_in(other.path(), &["log", "--oneline"]);
    fs::write(other.path().join("other.txt"), log).unwrap();
    git_in(other.path(), &["add", "other.txt"]);
    git_in(other.path(), &["commit", "-m", "from other"]);
    git_in(other.path(), &["push", "origin", "main"]);
    git_in(other.path(), &["rev-parse", "HEAD"])
        .trim()
        .to_string()
}

#[test]
fn needs_confirm_table() {
    let no_confirm = [Action::CheckoutBranch(s("main")), Action::Fetch];
    for a in &no_confirm {
        assert!(!needs_confirm(a), "{a:?}");
    }
    let confirm = [
        Action::CheckoutRemote {
            remote: s("origin"),
            branch: s("b"),
            local_name: s("b"),
        },
        Action::CheckoutCommit(s("abc")),
        Action::RenameBranch {
            old: s("a"),
            new: s("b"),
        },
        Action::DeleteBranch {
            name: s("a"),
            force: false,
        },
        Action::DeleteRemoteBranch {
            remote: s("origin"),
            name: s("a"),
        },
        Action::MergeInto(s("a")),
        Action::RebaseOnto(s("a")),
        Action::PushBranch {
            remote: s("origin"),
            name: s("a"),
        },
        Action::AddTag {
            hash: s("abc"),
            name: s("t"),
            message: None,
        },
        Action::DeleteTag(s("t")),
        Action::PushTag {
            remote: s("origin"),
            name: s("t"),
        },
        Action::CreateBranch {
            hash: s("abc"),
            name: s("b"),
        },
        Action::CherryPick(s("abc")),
        Action::Revert(s("abc")),
        Action::Reset {
            hash: s("abc"),
            mode: ResetMode::Mixed,
        },
        Action::Commit {
            message: s("m"),
            stage_all: true,
        },
        Action::Stash,
        Action::ResetWorktree,
        Action::FetchAll,
        Action::Pull,
        Action::PushCurrent,
    ];
    for a in &confirm {
        assert!(needs_confirm(a), "{a:?}");
    }
}

#[test]
fn checkout_local_branch() {
    let repo = TestRepo::new();
    repo.commit("a");
    repo.branch("feature");
    let git = Git::at(repo.path());
    execute(&git, &Action::CheckoutBranch(s("feature"))).unwrap();
    assert_eq!(current_branch(&repo), "feature");
}

#[test]
fn checkout_remote_creates_tracking_branch() {
    let repo = TestRepo::new();
    repo.commit("a");
    repo.branch("topic");
    let _bare = repo.add_bare_remote("origin");
    repo.git(&["branch", "-D", "topic"]);
    let git = Git::at(repo.path());
    execute(
        &git,
        &Action::CheckoutRemote {
            remote: s("origin"),
            branch: s("topic"),
            local_name: s("my-topic"),
        },
    )
    .unwrap();
    assert_eq!(current_branch(&repo), "my-topic");
    let upstream = repo.git(&["rev-parse", "--abbrev-ref", "my-topic@{upstream}"]);
    assert_eq!(upstream.trim(), "origin/topic");
}

#[test]
fn checkout_commit_detaches_head() {
    let repo = TestRepo::new();
    let first = repo.commit("a");
    repo.commit("b");
    let git = Git::at(repo.path());
    execute(&git, &Action::CheckoutCommit(first.clone())).unwrap();
    assert_eq!(current_branch(&repo), "HEAD");
    assert_eq!(head(&repo), first);
}

#[test]
fn rename_branch() {
    let repo = TestRepo::new();
    repo.commit("a");
    repo.branch("old");
    let git = Git::at(repo.path());
    execute(
        &git,
        &Action::RenameBranch {
            old: s("old"),
            new: s("new"),
        },
    )
    .unwrap();
    assert!(!ref_exists(&repo, "refs/heads/old"));
    assert!(ref_exists(&repo, "refs/heads/new"));
}

#[test]
fn delete_branch_refuses_unmerged_without_force() {
    let repo = TestRepo::new();
    repo.commit("a");
    repo.branch("topic");
    repo.checkout("topic");
    repo.commit("unmerged");
    repo.checkout("main");
    let git = Git::at(repo.path());
    let err = execute(
        &git,
        &Action::DeleteBranch {
            name: s("topic"),
            force: false,
        },
    )
    .unwrap_err();
    assert!(
        err.message().contains("not fully merged"),
        "{}",
        err.message()
    );
    assert!(ref_exists(&repo, "refs/heads/topic"));
    execute(
        &git,
        &Action::DeleteBranch {
            name: s("topic"),
            force: true,
        },
    )
    .unwrap();
    assert!(!ref_exists(&repo, "refs/heads/topic"));
}

#[test]
fn delete_remote_branch() {
    let repo = TestRepo::new();
    repo.commit("a");
    repo.branch("topic");
    let bare = repo.add_bare_remote("origin");
    let git = Git::at(repo.path());
    execute(
        &git,
        &Action::DeleteRemoteBranch {
            remote: s("origin"),
            name: s("topic"),
        },
    )
    .unwrap();
    let remote_heads = git_in(bare.path(), &["branch", "--list"]);
    assert!(!remote_heads.contains("topic"), "{remote_heads}");
    assert!(!ref_exists(&repo, "refs/remotes/origin/topic"));
}

#[test]
fn merge_into_current() {
    let repo = TestRepo::new();
    repo.commit("a");
    repo.branch("topic");
    repo.checkout("topic");
    let topic = repo.commit("t");
    repo.checkout("main");
    repo.commit("m");
    let git = Git::at(repo.path());
    execute(&git, &Action::MergeInto(s("topic"))).unwrap();
    assert_eq!(current_branch(&repo), "main");
    let parents = repo.git(&["rev-list", "--parents", "-n", "1", "HEAD"]);
    assert!(parents.contains(&topic), "{parents}");
    assert_eq!(parents.split_whitespace().count(), 3);
}

#[test]
fn rebase_onto() {
    let repo = TestRepo::new();
    repo.commit("a");
    repo.branch("topic");
    repo.checkout("topic");
    repo.commit("t");
    repo.checkout("main");
    let main_tip = repo.commit("m");
    repo.checkout("topic");
    let git = Git::at(repo.path());
    execute(&git, &Action::RebaseOnto(s("main"))).unwrap();
    assert_eq!(current_branch(&repo), "topic");
    assert_eq!(
        repo.git(&["rev-parse", "HEAD~1"]).trim(),
        main_tip,
        "topic should sit on top of main"
    );
}

#[test]
fn push_branch_updates_remote() {
    let repo = TestRepo::new();
    repo.commit("a");
    let bare = repo.add_bare_remote("origin");
    let tip = repo.commit("b");
    let git = Git::at(repo.path());
    execute(
        &git,
        &Action::PushBranch {
            remote: s("origin"),
            name: s("main"),
        },
    )
    .unwrap();
    assert_eq!(git_in(bare.path(), &["rev-parse", "main"]).trim(), tip);

    // Empty remote falls back to the only remote.
    let tip2 = repo.commit("c");
    execute(
        &git,
        &Action::PushBranch {
            remote: s(""),
            name: s("main"),
        },
    )
    .unwrap();
    assert_eq!(git_in(bare.path(), &["rev-parse", "main"]).trim(), tip2);
}

#[test]
fn push_falls_back_to_upstream_remote() {
    let repo = TestRepo::new();
    repo.commit("a");
    let _other = repo.add_bare_remote("aaa");
    let bare = repo.add_bare_remote("origin");
    repo.git(&["branch", "--set-upstream-to=origin/main", "main"]);
    let tip = repo.commit("b");
    let git = Git::at(repo.path());
    execute(&git, &Action::PushCurrent).unwrap();
    assert_eq!(git_in(bare.path(), &["rev-parse", "main"]).trim(), tip);
}

#[test]
fn add_annotated_and_lightweight_tag() {
    let repo = TestRepo::new();
    let h = repo.commit("a");
    repo.commit("b");
    let git = Git::at(repo.path());
    execute(
        &git,
        &Action::AddTag {
            hash: h.clone(),
            name: s("light"),
            message: None,
        },
    )
    .unwrap();
    execute(
        &git,
        &Action::AddTag {
            hash: h.clone(),
            name: s("heavy"),
            message: Some(s("release notes")),
        },
    )
    .unwrap();
    assert_eq!(repo.git(&["cat-file", "-t", "light"]).trim(), "commit");
    assert_eq!(repo.git(&["cat-file", "-t", "heavy"]).trim(), "tag");
    assert_eq!(repo.git(&["rev-parse", "light^{commit}"]).trim(), h);
    assert_eq!(repo.git(&["rev-parse", "heavy^{commit}"]).trim(), h);
    let msg = repo.git(&["tag", "-l", "-n1", "heavy"]);
    assert!(msg.contains("release notes"), "{msg}");
}

#[test]
fn delete_and_push_tag() {
    let repo = TestRepo::new();
    repo.commit("a");
    let bare = repo.add_bare_remote("origin");
    repo.tag("v1");
    let git = Git::at(repo.path());
    execute(
        &git,
        &Action::PushTag {
            remote: s("origin"),
            name: s("v1"),
        },
    )
    .unwrap();
    assert_eq!(git_in(bare.path(), &["tag", "--list"]).trim(), "v1");
    execute(&git, &Action::DeleteTag(s("v1"))).unwrap();
    assert!(!ref_exists(&repo, "refs/tags/v1"));
}

#[test]
fn create_branch_at_commit() {
    let repo = TestRepo::new();
    let h = repo.commit("a");
    repo.commit("b");
    let git = Git::at(repo.path());
    execute(
        &git,
        &Action::CreateBranch {
            hash: h.clone(),
            name: s("at-a"),
        },
    )
    .unwrap();
    assert_eq!(repo.git(&["rev-parse", "at-a"]).trim(), h);
    assert_eq!(current_branch(&repo), "main");
}

#[test]
fn cherry_pick() {
    let repo = TestRepo::new();
    repo.commit("a");
    repo.branch("topic");
    repo.checkout("topic");
    let picked = repo.commit_file("picked.txt", "p\n", "picked");
    repo.checkout("main");
    let git = Git::at(repo.path());
    execute(&git, &Action::CherryPick(picked)).unwrap();
    assert!(repo.path().join("picked.txt").exists());
    assert_eq!(repo.git(&["log", "-1", "--format=%s"]).trim(), "picked");
}

#[test]
fn revert() {
    let repo = TestRepo::new();
    repo.commit("a");
    let h = repo.commit_file("gone.txt", "g\n", "add gone");
    let git = Git::at(repo.path());
    execute(&git, &Action::Revert(h)).unwrap();
    assert!(!repo.path().join("gone.txt").exists());
    let subject = repo.git(&["log", "-1", "--format=%s"]);
    assert!(subject.starts_with("Revert"), "{subject}");
}

#[test]
fn reset_soft_mixed_hard() {
    let repo = TestRepo::new();
    let base = repo.commit_file("f.txt", "1\n", "one");
    let git = Git::at(repo.path());

    // Soft: HEAD moves, change stays staged.
    repo.commit_file("f.txt", "2\n", "two");
    execute(
        &git,
        &Action::Reset {
            hash: base.clone(),
            mode: ResetMode::Soft,
        },
    )
    .unwrap();
    assert_eq!(head(&repo), base);
    assert_eq!(
        repo.git(&["diff", "--cached", "--name-only"]).trim(),
        "f.txt"
    );
    assert_eq!(
        fs::read_to_string(repo.path().join("f.txt")).unwrap(),
        "2\n"
    );
    repo.git(&["commit", "-m", "two again"]);

    // Mixed: HEAD moves, index reset, working tree keeps change.
    execute(
        &git,
        &Action::Reset {
            hash: base.clone(),
            mode: ResetMode::Mixed,
        },
    )
    .unwrap();
    assert_eq!(head(&repo), base);
    assert_eq!(repo.git(&["diff", "--cached", "--name-only"]).trim(), "");
    assert_eq!(repo.git(&["diff", "--name-only"]).trim(), "f.txt");
    repo.git(&["commit", "-am", "two once more"]);

    // Hard: HEAD, index and working tree all match the target.
    execute(
        &git,
        &Action::Reset {
            hash: base.clone(),
            mode: ResetMode::Hard,
        },
    )
    .unwrap();
    assert_eq!(head(&repo), base);
    assert_eq!(repo.git(&["status", "--porcelain"]).trim(), "");
    assert_eq!(
        fs::read_to_string(repo.path().join("f.txt")).unwrap(),
        "1\n"
    );
}

#[test]
fn commit_stage_all_includes_untracked() {
    let repo = TestRepo::new();
    repo.commit_file("f.txt", "1\n", "one");
    fs::write(repo.path().join("f.txt"), "2\n").unwrap();
    fs::write(repo.path().join("new.txt"), "n\n").unwrap();
    let git = Git::at(repo.path());
    execute(
        &git,
        &Action::Commit {
            message: s("all"),
            stage_all: true,
        },
    )
    .unwrap();
    assert_eq!(repo.git(&["status", "--porcelain"]).trim(), "");
    let files = repo.git(&["show", "--name-only", "--format=", "HEAD"]);
    assert!(
        files.contains("f.txt") && files.contains("new.txt"),
        "{files}"
    );
    assert_eq!(repo.git(&["log", "-1", "--format=%s"]).trim(), "all");
}

#[test]
fn commit_staged_only_leaves_other_changes() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "1\n", "one");
    repo.commit_file("b.txt", "1\n", "two");
    fs::write(repo.path().join("a.txt"), "2\n").unwrap();
    fs::write(repo.path().join("b.txt"), "2\n").unwrap();
    fs::write(repo.path().join("new.txt"), "n\n").unwrap();
    repo.git(&["add", "a.txt"]);
    let git = Git::at(repo.path());
    execute(
        &git,
        &Action::Commit {
            message: s("only a"),
            stage_all: false,
        },
    )
    .unwrap();
    let files = repo.git(&["show", "--name-only", "--format=", "HEAD"]);
    assert_eq!(files.trim(), "a.txt");
    let status = repo.git(&["status", "--porcelain"]);
    assert!(status.contains(" M b.txt"), "{status}");
    assert!(status.contains("?? new.txt"), "{status}");
}

#[test]
fn stash_removes_tracked_changes_and_keeps_untracked() {
    let repo = TestRepo::new();
    repo.commit_file("f.txt", "1\n", "one");
    fs::write(repo.path().join("f.txt"), "2\n").unwrap();
    fs::write(repo.path().join("new.txt"), "n\n").unwrap();
    let git = Git::at(repo.path());
    execute(&git, &Action::Stash).unwrap();
    assert_eq!(
        fs::read_to_string(repo.path().join("f.txt")).unwrap(),
        "1\n"
    );
    assert!(repo.path().join("new.txt").exists());
    assert_eq!(repo.git(&["stash", "list"]).lines().count(), 1);
}

#[test]
fn reset_worktree_reverts_tracked_and_keeps_untracked() {
    let repo = TestRepo::new();
    let h = repo.commit_file("f.txt", "1\n", "one");
    fs::write(repo.path().join("f.txt"), "2\n").unwrap();
    fs::write(repo.path().join("staged.txt"), "s\n").unwrap();
    repo.git(&["add", "staged.txt"]);
    fs::write(repo.path().join("new.txt"), "n\n").unwrap();
    let git = Git::at(repo.path());
    execute(&git, &Action::ResetWorktree).unwrap();
    assert_eq!(head(&repo), h);
    assert_eq!(
        fs::read_to_string(repo.path().join("f.txt")).unwrap(),
        "1\n"
    );
    assert!(!repo.path().join("staged.txt").exists());
    assert!(repo.path().join("new.txt").exists());
    assert_eq!(repo.git(&["status", "--porcelain"]).trim(), "?? new.txt");
}

#[test]
fn fetch_brings_new_remote_commits() {
    let repo = TestRepo::new();
    repo.commit("a");
    let bare = repo.add_bare_remote("origin");
    let git = Git::at(repo.path());

    let new = push_from_other_clone(&bare);
    execute(&git, &Action::Fetch).unwrap();
    assert_eq!(repo.git(&["rev-parse", "origin/main"]).trim(), new);

    let newer = push_from_other_clone(&bare);
    execute(&git, &Action::FetchAll).unwrap();
    assert_eq!(repo.git(&["rev-parse", "origin/main"]).trim(), newer);
}

#[test]
fn pull_ff_only() {
    let repo = TestRepo::new();
    repo.commit("a");
    let bare = repo.add_bare_remote("origin");
    repo.git(&["fetch", "origin"]);
    repo.git(&["branch", "--set-upstream-to=origin/main", "main"]);
    let new = push_from_other_clone(&bare);
    let git = Git::at(repo.path());
    execute(&git, &Action::Pull).unwrap();
    assert_eq!(head(&repo), new);

    // Diverged history is refused instead of creating a merge.
    push_from_other_clone(&bare);
    let local = repo.commit("local");
    assert!(execute(&git, &Action::Pull).is_err());
    assert_eq!(head(&repo), local);
}

#[test]
fn names_starting_with_dash_are_rejected_before_running_git() {
    let repo = TestRepo::new();
    repo.commit("a");
    let git = Git::at(repo.path());
    let bad = [
        Action::CheckoutBranch(s("-b")),
        Action::CheckoutRemote {
            remote: s("origin"),
            branch: s("x"),
            local_name: s("--orphan"),
        },
        Action::CheckoutCommit(s("--detach")),
        Action::RenameBranch {
            old: s("main"),
            new: s("-x"),
        },
        Action::DeleteBranch {
            name: s("-D"),
            force: false,
        },
        Action::DeleteRemoteBranch {
            remote: s("--mirror"),
            name: s("x"),
        },
        Action::MergeInto(s("--abort")),
        Action::RebaseOnto(s("-i")),
        Action::PushBranch {
            remote: s("origin"),
            name: s("--force"),
        },
        Action::AddTag {
            hash: s("HEAD"),
            name: s("-f"),
            message: None,
        },
        Action::DeleteTag(s("-l")),
        Action::PushTag {
            remote: s("origin"),
            name: s("--tags"),
        },
        Action::CreateBranch {
            hash: s("-f"),
            name: s("x"),
        },
        Action::CherryPick(s("--abort")),
        Action::Revert(s("--abort")),
        Action::Reset {
            hash: s("--hard"),
            mode: ResetMode::Soft,
        },
    ];
    for a in &bad {
        let err = execute(&git, a).unwrap_err();
        assert!(
            matches!(err, terminal_git_graph::git::GitError::InvalidName(_)),
            "{a:?}: {err:?}"
        );
    }
    assert_eq!(git.call_count(), 0);
}

#[test]
fn merge_conflict_returns_error_with_conflict_text() {
    let repo = TestRepo::new();
    repo.commit_file("f.txt", "base\n", "base");
    repo.branch("topic");
    repo.checkout("topic");
    repo.commit_file("f.txt", "topic\n", "topic change");
    repo.checkout("main");
    repo.commit_file("f.txt", "main\n", "main change");
    let git = Git::at(repo.path());
    let err = execute(&git, &Action::MergeInto(s("topic"))).unwrap_err();
    assert!(err.message().contains("CONFLICT"), "{}", err.message());
}

#[test]
fn network_env_uses_batch_ssh_unless_user_chose_ssh() {
    use terminal_git_graph::actions::{network_env, BATCH_SSH_COMMAND};
    let repo = TestRepo::new();
    repo.commit("first");
    let git = Git::at(repo.path());
    let batch = ("GIT_SSH_COMMAND", BATCH_SSH_COMMAND);
    assert!(network_env(&git, false).contains(&batch));
    assert!(network_env(&git, false).contains(&("GIT_TERMINAL_PROMPT", "0")));
    assert!(!network_env(&git, true).contains(&batch));
    repo.git(&["config", "core.sshCommand", "ssh -i key"]);
    assert!(!network_env(&git, false).contains(&batch));
}

#[test]
fn network_actions_pass_batch_ssh_command() {
    if std::env::var_os("GIT_SSH_COMMAND").is_some() || std::env::var_os("GIT_SSH").is_some() {
        return; // The user's own ssh command wins; nothing to observe here.
    }
    let repo = TestRepo::new();
    repo.commit("first");
    repo.tag("v1");
    // A fake transport that reports the ssh command it was given, then fails.
    repo.git(&["config", "protocol.ext.allow", "always"]);
    repo.git(&[
        "remote",
        "add",
        "origin",
        "ext::sh -c echo% \"ssh=[$GIT_SSH_COMMAND]\"% >&2;% exit% 1",
    ]);
    repo.git(&["config", "branch.main.remote", "origin"]);
    repo.git(&["config", "branch.main.merge", "refs/heads/main"]);
    let git = Git::at(repo.path());
    let actions = [
        Action::Fetch,
        Action::FetchAll,
        Action::Pull,
        Action::PushBranch {
            remote: s(""),
            name: s("main"),
        },
        Action::PushCurrent,
        Action::PushTag {
            remote: s(""),
            name: s("v1"),
        },
        Action::DeleteRemoteBranch {
            remote: s("origin"),
            name: s("main"),
        },
    ];
    for a in actions {
        let err = execute(&git, &a).expect_err("fake remote fails");
        assert!(
            err.message().contains("ssh=[ssh -o BatchMode=yes]"),
            "{a:?}: {}",
            err.message()
        );
    }
}

#[test]
fn checkout_of_a_path_name_never_touches_the_file() {
    let repo = TestRepo::new();
    repo.commit_file("feature", "committed\n", "a");
    fs::write(repo.path().join("feature"), "local edit\n").unwrap();
    let git = Git::at(repo.path());
    assert!(execute(&git, &Action::CheckoutBranch(s("feature"))).is_err());
    assert_eq!(
        fs::read_to_string(repo.path().join("feature")).unwrap(),
        "local edit\n"
    );
    // A real branch with the same name as a file still checks out.
    repo.git(&["stash"]);
    repo.branch("feature");
    execute(&git, &Action::CheckoutBranch(s("feature"))).unwrap();
    assert_eq!(current_branch(&repo), "feature");
}

#[test]
fn first_push_sets_upstream() {
    let repo = TestRepo::new();
    repo.commit("a");
    let _bare = repo.add_bare_remote("origin");
    repo.branch("topic");
    repo.checkout("topic");
    repo.commit("t");
    let git = Git::at(repo.path());
    execute(&git, &Action::PushCurrent).unwrap();
    let upstream = repo.git(&["rev-parse", "--abbrev-ref", "topic@{upstream}"]);
    assert_eq!(upstream.trim(), "origin/topic");

    repo.branch("other");
    execute(
        &git,
        &Action::PushBranch {
            remote: s(""),
            name: s("other"),
        },
    )
    .unwrap();
    let upstream = repo.git(&["rev-parse", "--abbrev-ref", "other@{upstream}"]);
    assert_eq!(upstream.trim(), "origin/other");
}
