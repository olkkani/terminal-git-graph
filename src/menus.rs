//! Context menu contents per target, and the dialogs an action asks before it runs.

use crate::actions::{Action, ResetMode};
use crate::git::refs::Head;
use crate::ui::dialog::Dialog;
use crate::ui::menu::{MenuId, MenuItem};

/// What a context menu was opened on.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Target {
    LocalBranch(String),
    RemoteBranch {
        remote: String,
        name: String,
    },
    Tag(String),
    /// Full hash.
    Commit(String),
    Uncommitted,
}

/// Repository state the menu items depend on.
#[derive(Clone, Debug, PartialEq)]
pub struct MenuCtx {
    pub head: Head,
    pub has_staged: bool,
    /// Configured remote names.
    pub remotes: Vec<String>,
    /// Subject of the targeted commit; unused for other targets.
    pub subject: String,
}

impl MenuCtx {
    /// The checked-out branch, `None` when HEAD is detached.
    fn current_branch(&self) -> Option<&str> {
        match &self.head {
            Head::Branch(name) | Head::Unborn(name) => Some(name),
            Head::Detached(_) => None,
        }
    }
}

fn item(label: &str, enabled: bool, id: MenuId) -> MenuItem {
    MenuItem {
        label: label.to_string(),
        enabled,
        danger: false,
        id,
    }
}

fn danger(label: &str, enabled: bool, id: MenuId) -> MenuItem {
    MenuItem {
        danger: true,
        ..item(label, enabled, id)
    }
}

fn act(a: Action) -> MenuId {
    MenuId::Do(a)
}

pub fn menu_for(t: &Target, ctx: &MenuCtx) -> Vec<MenuItem> {
    let current = ctx.current_branch();
    let attached = current.is_some();
    let has_remote = !ctx.remotes.is_empty();
    match t {
        Target::LocalBranch(name) => {
            let other = attached && current != Some(name.as_str());
            vec![
                item("Checkout", true, act(Action::CheckoutBranch(name.clone()))),
                item(
                    "Rename",
                    true,
                    act(Action::RenameBranch {
                        old: name.clone(),
                        new: String::new(),
                    }),
                ),
                danger(
                    "Delete",
                    current != Some(name.as_str()),
                    act(Action::DeleteBranch {
                        name: name.clone(),
                        force: false,
                    }),
                ),
                item(
                    "Merge into current",
                    other,
                    act(Action::MergeInto(name.clone())),
                ),
                item(
                    "Rebase onto current",
                    other,
                    act(Action::RebaseOnto(name.clone())),
                ),
                item(
                    "Push",
                    attached && has_remote,
                    act(Action::PushBranch {
                        remote: String::new(),
                        name: name.clone(),
                    }),
                ),
                item("Copy name", true, MenuId::Copy(name.clone())),
            ]
        }
        Target::RemoteBranch { remote, name } => {
            let full = format!("{remote}/{name}");
            vec![
                item(
                    "Checkout",
                    true,
                    act(Action::CheckoutRemote {
                        remote: remote.clone(),
                        branch: name.clone(),
                        local_name: String::new(),
                    }),
                ),
                item("Merge", attached, act(Action::MergeInto(full.clone()))),
                danger(
                    "Delete on remote",
                    true,
                    act(Action::DeleteRemoteBranch {
                        remote: remote.clone(),
                        name: name.clone(),
                    }),
                ),
                item("Copy name", true, MenuId::Copy(full)),
            ]
        }
        Target::Tag(name) => vec![
            danger("Delete", true, act(Action::DeleteTag(name.clone()))),
            item(
                "Push",
                has_remote,
                act(Action::PushTag {
                    remote: String::new(),
                    name: name.clone(),
                }),
            ),
            item("Copy name", true, MenuId::Copy(name.clone())),
        ],
        Target::Commit(hash) => {
            let reset = |label: &str, mode| {
                danger(
                    label,
                    true,
                    act(Action::Reset {
                        hash: hash.clone(),
                        mode,
                    }),
                )
            };
            vec![
                item(
                    "Add tag",
                    true,
                    act(Action::AddTag {
                        hash: hash.clone(),
                        name: String::new(),
                        message: None,
                    }),
                ),
                item(
                    "Create branch",
                    true,
                    act(Action::CreateBranch {
                        hash: hash.clone(),
                        name: String::new(),
                    }),
                ),
                item("Checkout", true, act(Action::CheckoutCommit(hash.clone()))),
                item("Cherry pick", true, act(Action::CherryPick(hash.clone()))),
                item("Revert", true, act(Action::Revert(hash.clone()))),
                item("Merge", attached, act(Action::MergeInto(hash.clone()))),
                item("Rebase", attached, act(Action::RebaseOnto(hash.clone()))),
                reset("Reset (Soft)", ResetMode::Soft),
                reset("Reset (Mixed)", ResetMode::Mixed),
                reset("Reset (Hard)", ResetMode::Hard),
                item("Copy hash", true, MenuId::Copy(hash.clone())),
                item("Copy subject", true, MenuId::Copy(ctx.subject.clone())),
            ]
        }
        Target::Uncommitted => vec![
            item(
                "Commit (stage all)",
                true,
                act(Action::Commit {
                    message: String::new(),
                    stage_all: true,
                }),
            ),
            item(
                "Commit (staged only)",
                ctx.has_staged,
                act(Action::Commit {
                    message: String::new(),
                    stage_all: false,
                }),
            ),
            item("Stash", true, act(Action::Stash)),
            danger("Reset", true, act(Action::ResetWorktree)),
        ],
    }
}

/// The dialog asking for the value an action from a menu still lacks (an empty name or
/// message), if any.
pub fn input_dialog(a: &Action) -> Option<Dialog> {
    let input = |title: &str, prompt: String, value: &str| {
        Some(Dialog::Input {
            title: title.to_string(),
            prompt,
            value: value.to_string(),
        })
    };
    match a {
        Action::RenameBranch { old, new } if new.is_empty() => {
            input("Rename branch", format!("New name for {old}:"), old)
        }
        Action::CheckoutRemote {
            remote,
            branch,
            local_name,
        } if local_name.is_empty() => input(
            "Checkout remote branch",
            format!("Local branch name for {remote}/{branch}:"),
            branch,
        ),
        Action::AddTag { name, .. } if name.is_empty() => {
            input("Add tag", "Tag name:".to_string(), "")
        }
        Action::CreateBranch { name, .. } if name.is_empty() => {
            input("Create branch", "Branch name:".to_string(), "")
        }
        Action::Commit { message, .. } if message.is_empty() => {
            input("Commit", "Commit message:".to_string(), "")
        }
        _ => None,
    }
}

/// Puts the value typed into `input_dialog` into the action.
pub fn with_value(a: Action, value: String) -> Action {
    match a {
        Action::RenameBranch { old, .. } => Action::RenameBranch { old, new: value },
        Action::CheckoutRemote { remote, branch, .. } => Action::CheckoutRemote {
            remote,
            branch,
            local_name: value,
        },
        Action::AddTag { hash, message, .. } => Action::AddTag {
            hash,
            name: value,
            message,
        },
        Action::CreateBranch { hash, .. } => Action::CreateBranch { hash, name: value },
        Action::Commit { stage_all, .. } => Action::Commit {
            message: value,
            stage_all,
        },
        other => other,
    }
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(7)]
}

/// The confirmation shown before `a` runs; destructive actions are marked danger.
pub fn confirm_dialog(a: &Action) -> Dialog {
    let (title, body, danger) = match a {
        Action::CheckoutBranch(name) => ("Checkout", format!("Check out {name}?"), false),
        Action::CheckoutRemote {
            remote,
            branch,
            local_name,
        } => (
            "Checkout",
            format!("Create branch {local_name} tracking {remote}/{branch} and check it out?"),
            false,
        ),
        Action::CheckoutCommit(hash) => (
            "Checkout",
            format!("Check out commit {}? HEAD will be detached.", short(hash)),
            false,
        ),
        Action::RenameBranch { old, new } => {
            ("Rename branch", format!("Rename {old} to {new}?"), false)
        }
        Action::DeleteBranch { name, .. } => {
            ("Delete branch", format!("Delete branch {name}?"), true)
        }
        Action::DeleteRemoteBranch { remote, name } => (
            "Delete remote branch",
            format!("Delete branch {name} on {remote}?"),
            true,
        ),
        Action::MergeInto(target) => (
            "Merge",
            format!("Merge {} into the current branch?", short_ref(target)),
            false,
        ),
        Action::RebaseOnto(target) => (
            "Rebase",
            format!("Rebase the current branch onto {}?", short_ref(target)),
            false,
        ),
        Action::PushBranch { name, .. } => ("Push", format!("Push branch {name}?"), false),
        Action::AddTag { hash, name, .. } => (
            "Add tag",
            format!("Add tag {name} at {}?", short(hash)),
            false,
        ),
        Action::DeleteTag(name) => ("Delete tag", format!("Delete tag {name}?"), true),
        Action::PushTag { name, .. } => ("Push", format!("Push tag {name}?"), false),
        Action::CreateBranch { hash, name } => (
            "Create branch",
            format!("Create branch {name} at {}?", short(hash)),
            false,
        ),
        Action::CherryPick(hash) => (
            "Cherry pick",
            format!("Cherry-pick {} onto the current branch?", short(hash)),
            false,
        ),
        Action::Revert(hash) => ("Revert", format!("Revert {}?", short(hash)), false),
        Action::Reset { hash, mode } => {
            let (mode, extra) = match mode {
                ResetMode::Soft => ("soft", ""),
                ResetMode::Mixed => ("mixed", ""),
                ResetMode::Hard => (
                    "hard",
                    " Changes to tracked files are discarded. Untracked files are kept.",
                ),
            };
            (
                "Reset",
                format!(
                    "Reset the current branch to {} ({mode})?{extra}",
                    short(hash)
                ),
                true,
            )
        }
        Action::Commit { message, stage_all } => {
            let what = if *stage_all {
                "all changes"
            } else {
                "staged changes"
            };
            (
                "Commit",
                format!("Commit {what} with message:\n{message}"),
                false,
            )
        }
        Action::Stash => (
            "Stash",
            "Stash changes to tracked files?".to_string(),
            false,
        ),
        Action::ResetWorktree => (
            "Reset",
            "Discard all changes to tracked files? Untracked files are kept.".to_string(),
            true,
        ),
        Action::Fetch => ("Fetch", "Fetch from the default remote?".to_string(), false),
        Action::FetchAll => ("Fetch all", "Fetch from all remotes?".to_string(), false),
        Action::Pull => ("Pull", "Pull (fast-forward only)?".to_string(), false),
        Action::PushCurrent => ("Push", "Push the current branch?".to_string(), false),
    };
    Dialog::Confirm {
        title: title.to_string(),
        body,
        danger,
    }
}

/// A ref name as given, or a full commit hash shortened.
fn short_ref(target: &str) -> &str {
    if target.len() == 40 && target.chars().all(|c| c.is_ascii_hexdigit()) {
        short(target)
    } else {
        target
    }
}
