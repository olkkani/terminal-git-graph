use super::{Git, GitError};

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkStatus {
    pub staged: usize,
    pub unstaged: usize,
    pub untracked: usize,
    pub branch: Option<String>,
    pub has_upstream: bool,
    pub ahead: usize,
    pub behind: usize,
    /// Tracked files with any change (a file staged and unstaged counts once).
    tracked_changed: usize,
}

impl WorkStatus {
    pub fn changed_total(&self) -> usize {
        self.tracked_changed + self.untracked
    }

    pub fn is_clean(&self) -> bool {
        self.changed_total() == 0
    }
}

/// Parses `git status --porcelain=v2 --branch` output.
pub fn parse_status_v2(out: &str) -> WorkStatus {
    let mut s = WorkStatus::default();
    for line in out.lines() {
        if let Some(rest) = line.strip_prefix("# branch.head ") {
            s.branch = (rest != "(detached)").then(|| rest.to_string());
        } else if line.starts_with("# branch.upstream ") {
            s.has_upstream = true;
        } else if let Some(rest) = line.strip_prefix("# branch.ab ") {
            for tok in rest.split_whitespace() {
                if let Some(n) = tok.strip_prefix('+') {
                    s.ahead = n.parse().unwrap_or(0);
                } else if let Some(n) = tok.strip_prefix('-') {
                    s.behind = n.parse().unwrap_or(0);
                }
            }
        } else if line.starts_with("? ") {
            s.untracked += 1;
        } else if line.starts_with("u ") {
            s.unstaged += 1;
            s.tracked_changed += 1;
        } else if line.starts_with("1 ") || line.starts_with("2 ") {
            let mut xy = line[2..].chars();
            let (x, y) = (xy.next(), xy.next());
            if x.is_some_and(|c| c != '.') {
                s.staged += 1;
            }
            if y.is_some_and(|c| c != '.') {
                s.unstaged += 1;
            }
            s.tracked_changed += 1;
        }
    }
    s
}

pub fn load_status(git: &Git) -> Result<WorkStatus, GitError> {
    let out = git.run(&[
        "--no-optional-locks",
        "status",
        "--porcelain=v2",
        "--branch",
    ])?;
    Ok(parse_status_v2(&out))
}
