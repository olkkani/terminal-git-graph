use std::path::Path;

use crate::git::Git;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncState {
    InSync,
    Ahead(usize),
    Behind(usize),
    Diverged(usize, usize),
    /// A remote exists but the branch has no upstream.
    NotPushed,
    NoRemote,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoStatus {
    pub branch: Option<String>,
    pub dirty: bool,
    pub sync: SyncState,
    pub latest_tag: Option<String>,
}

/// Summarizes a repository. Git failures degrade to empty/neutral values. It is polled,
/// so it never takes git's optional locks (no index refresh is written).
pub fn repo_status(path: &Path, with_tag: bool) -> RepoStatus {
    let git = Git::at(path);
    let mut branch = None;
    let mut dirty = false;
    let mut has_upstream = false;
    let (mut ahead, mut behind) = (0usize, 0usize);

    if let Ok(out) = git.run(&["--no-optional-locks", "status", "--porcelain=v2", "--branch"]) {
        for line in out.lines() {
            if let Some(head) = line.strip_prefix("# branch.head ") {
                if head != "(detached)" {
                    branch = Some(head.to_string());
                }
            } else if line.starts_with("# branch.upstream ") {
                has_upstream = true;
            } else if let Some(ab) = line.strip_prefix("# branch.ab ") {
                let mut it = ab.split_whitespace();
                ahead = it.next().and_then(|s| s.trim_start_matches('+').parse().ok()).unwrap_or(0);
                behind = it.next().and_then(|s| s.trim_start_matches('-').parse().ok()).unwrap_or(0);
            } else if !line.starts_with('#') && !line.is_empty() {
                dirty = true;
            }
        }
    }

    let sync = if has_upstream {
        match (ahead, behind) {
            (0, 0) => SyncState::InSync,
            (a, 0) => SyncState::Ahead(a),
            (0, b) => SyncState::Behind(b),
            (a, b) => SyncState::Diverged(a, b),
        }
    } else if git.run(&["remote"]).map(|o| o.trim().is_empty()).unwrap_or(true) {
        SyncState::NoRemote
    } else {
        SyncState::NotPushed
    };

    let latest_tag = if with_tag {
        git.run(&["describe", "--tags", "--abbrev=0"])
            .ok()
            .map(|t| t.trim().to_string())
            .filter(|t| !t.is_empty())
    } else {
        None
    };

    RepoStatus { branch, dirty, sync, latest_tag }
}
