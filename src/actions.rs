//! Executes user-chosen git actions. Pure git side: no UI, no prompts.

use crate::git::{validate_name, Git, GitError};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ResetMode {
    Soft,
    Mixed,
    Hard,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Action {
    CheckoutBranch(String),
    CheckoutRemote {
        remote: String,
        branch: String,
        local_name: String,
    },
    CheckoutCommit(String),
    RenameBranch {
        old: String,
        new: String,
    },
    DeleteBranch {
        name: String,
        force: bool,
    },
    DeleteRemoteBranch {
        remote: String,
        name: String,
    },
    /// Merges the target ref into the current branch.
    MergeInto(String),
    /// Rebases the current branch onto the target ref.
    RebaseOnto(String),
    /// An empty `remote` means: the branch's upstream remote, else the only remote.
    PushBranch {
        remote: String,
        name: String,
    },
    AddTag {
        hash: String,
        name: String,
        message: Option<String>,
    },
    DeleteTag(String),
    /// An empty `remote` means: the only remote.
    PushTag {
        remote: String,
        name: String,
    },
    CreateBranch {
        hash: String,
        name: String,
    },
    CherryPick(String),
    Revert(String),
    Reset {
        hash: String,
        mode: ResetMode,
    },
    Commit {
        message: String,
        stage_all: bool,
    },
    /// Stashes tracked changes only.
    Stash,
    /// `git reset --hard HEAD`: tracked files only, untracked files are kept.
    ResetWorktree,
    Fetch,
    FetchAll,
    /// `git pull --ff-only`.
    Pull,
    PushCurrent,
}

/// Every mutating action needs confirmation except local checkout and Fetch.
pub fn needs_confirm(a: &Action) -> bool {
    !matches!(a, Action::CheckoutBranch(_) | Action::Fetch)
}

/// Runs the action and returns git's stdout. Names are validated before any git call.
pub fn execute(git: &Git, a: &Action) -> Result<String, GitError> {
    validate_all(&names_of(a))?;
    match a {
        Action::CheckoutBranch(name) => git.run(&["checkout", name]),
        Action::CheckoutRemote {
            remote,
            branch,
            local_name,
        } => {
            let start = format!("{remote}/{branch}");
            git.run(&["checkout", "--track", "-b", local_name, &start])
        }
        Action::CheckoutCommit(hash) => git.run(&["checkout", "--detach", hash]),
        Action::RenameBranch { old, new } => git.run(&["branch", "-m", old, new]),
        Action::DeleteBranch { name, force } => {
            let flag = if *force { "-D" } else { "-d" };
            git.run(&["branch", flag, name])
        }
        Action::DeleteRemoteBranch { remote, name } => {
            run_network(git, &["push", remote, "--delete", name])
        }
        Action::MergeInto(target) => git.run(&["merge", "--no-edit", target]),
        Action::RebaseOnto(target) => git.run(&["rebase", target]),
        Action::PushBranch { remote, name } => {
            let remote = resolve_remote(git, remote, Some(name))?;
            run_network(git, &["push", &remote, name])
        }
        Action::AddTag {
            hash,
            name,
            message,
        } => match message {
            Some(msg) => git.run(&["tag", "-a", name, "-m", msg, hash]),
            None => git.run(&["tag", name, hash]),
        },
        Action::DeleteTag(name) => git.run(&["tag", "-d", name]),
        Action::PushTag { remote, name } => {
            let remote = resolve_remote(git, remote, None)?;
            let refspec = format!("refs/tags/{name}");
            run_network(git, &["push", &remote, &refspec])
        }
        Action::CreateBranch { hash, name } => git.run(&["branch", name, hash]),
        Action::CherryPick(hash) => git.run(&["cherry-pick", hash]),
        Action::Revert(hash) => git.run(&["revert", "--no-edit", hash]),
        Action::Reset { hash, mode } => {
            let flag = match mode {
                ResetMode::Soft => "--soft",
                ResetMode::Mixed => "--mixed",
                ResetMode::Hard => "--hard",
            };
            git.run(&["reset", flag, hash])
        }
        Action::Commit { message, stage_all } => {
            if *stage_all {
                git.run(&["add", "-A"])?;
            }
            git.run(&["commit", "-m", message])
        }
        Action::Stash => git.run(&["stash", "push"]),
        Action::ResetWorktree => git.run(&["reset", "--hard", "HEAD"]),
        Action::Fetch => run_network(git, &["fetch"]),
        Action::FetchAll => run_network(git, &["fetch", "--all"]),
        Action::Pull => run_network(git, &["pull", "--ff-only"]),
        Action::PushCurrent => {
            let branch = git.run(&["symbolic-ref", "--short", "HEAD"])?;
            let branch = branch.trim();
            validate_name(branch)?;
            let remote = resolve_remote(git, "", Some(branch))?;
            run_network(git, &["push", &remote, branch])
        }
    }
}

/// ssh in batch mode: a passphrase or unknown-host prompt fails with git's error text
/// instead of drawing on /dev/tty over the TUI.
pub const BATCH_SSH_COMMAND: &str = "ssh -o BatchMode=yes";

/// The environment for a network git call. Login prompts are always off; ssh is put in
/// batch mode unless the user chose their own ssh command (`user_ssh_env`: whether
/// `GIT_SSH_COMMAND` or `GIT_SSH` is set, or `core.sshCommand` in git's config).
pub fn network_env(git: &Git, user_ssh_env: bool) -> Vec<(&'static str, &'static str)> {
    let mut env = vec![("GIT_TERMINAL_PROMPT", "0")];
    // `git config --get` exits 1 when the key is unset.
    let configured = git
        .run(&["config", "--get", "core.sshCommand"])
        .is_ok_and(|out| !out.trim().is_empty());
    if !user_ssh_env && !configured {
        env.push(("GIT_SSH_COMMAND", BATCH_SSH_COMMAND));
    }
    env
}

fn run_network(git: &Git, args: &[&str]) -> Result<String, GitError> {
    let user_ssh_env =
        std::env::var_os("GIT_SSH_COMMAND").is_some() || std::env::var_os("GIT_SSH").is_some();
    git.run_with_env(args, &network_env(git, user_ssh_env))
}

/// The user-supplied name/ref arguments of an action. An empty push remote is resolved later.
fn names_of(a: &Action) -> Vec<&str> {
    match a {
        Action::CheckoutBranch(n)
        | Action::CheckoutCommit(n)
        | Action::MergeInto(n)
        | Action::RebaseOnto(n)
        | Action::DeleteTag(n)
        | Action::CherryPick(n)
        | Action::Revert(n) => vec![n],
        Action::CheckoutRemote {
            remote,
            branch,
            local_name,
        } => vec![remote, branch, local_name],
        Action::RenameBranch { old, new } => vec![old, new],
        Action::DeleteBranch { name, .. } => vec![name],
        Action::DeleteRemoteBranch { remote, name } => vec![remote, name],
        Action::PushBranch { remote, name } | Action::PushTag { remote, name } => {
            if remote.is_empty() {
                vec![name]
            } else {
                vec![remote, name]
            }
        }
        Action::AddTag { hash, name, .. } | Action::CreateBranch { hash, name } => {
            vec![hash, name]
        }
        Action::Reset { hash, .. } => vec![hash],
        Action::Commit { .. }
        | Action::Stash
        | Action::ResetWorktree
        | Action::Fetch
        | Action::FetchAll
        | Action::Pull
        | Action::PushCurrent => vec![],
    }
}

fn validate_all(names: &[&str]) -> Result<(), GitError> {
    names.iter().try_for_each(|n| validate_name(n))
}

/// Uses `remote` if given, else the branch's upstream remote, else the only configured remote.
fn resolve_remote(git: &Git, remote: &str, branch: Option<&str>) -> Result<String, GitError> {
    if !remote.is_empty() {
        return Ok(remote.to_string());
    }
    if let Some(branch) = branch {
        let key = format!("branch.{branch}.remote");
        // `git config --get` exits 1 when the key is unset.
        if let Ok(out) = git.run(&["config", "--get", &key]) {
            let upstream = out.trim();
            if !upstream.is_empty() && upstream != "." {
                validate_name(upstream)?;
                return Ok(upstream.to_string());
            }
        }
    }
    let remotes = git.run(&["remote"])?;
    let remotes: Vec<&str> = remotes
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    match remotes.as_slice() {
        [only] => {
            validate_name(only)?;
            Ok(only.to_string())
        }
        [] => Err(GitError::Parse("no remote to push to".to_string())),
        _ => Err(GitError::Parse(
            "no upstream set and more than one remote; choose a remote".to_string(),
        )),
    }
}
