use std::collections::{HashMap, VecDeque};
use std::fs::File;
use std::io::Read;

use super::{Git, GitError};

pub const MAX_UNTRACKED_BYTES: usize = 1_048_576;
const MAX_CACHED: usize = 256;
const TRUNCATED_MARKER: &str = "[truncated]";

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct FileChange {
    pub path: String,
    pub status: char,
    /// `None` for binary files.
    pub added: Option<u32>,
    pub deleted: Option<u32>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CommitDetail {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub timestamp: i64,
    pub message: String,
    pub files: Vec<FileChange>,
}

/// Merge commits are diffed against their first parent.
pub fn load_detail(git: &Git, hash: &str) -> Result<CommitDetail, GitError> {
    let meta = git.run(&[
        "show",
        "-s",
        "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%at%x1f%B",
        hash,
        "--",
    ])?;
    let mut parts = meta.splitn(6, '\x1f');
    let mut next = || parts.next().unwrap_or("");
    let full_hash = next().trim().to_string();
    let parents: Vec<String> = next().split_whitespace().map(str::to_string).collect();
    let author = next().to_string();
    let email = next().to_string();
    let timestamp = next().trim().parse().unwrap_or(0);
    let message = next().trim_end().to_string();

    let mut args = vec![
        "diff-tree",
        "-r",
        "--no-ext-diff",
        "--raw",
        "--numstat",
        "-z",
        "--no-commit-id",
    ];
    let out = match parents.first() {
        Some(p) => {
            args.extend([p.as_str(), full_hash.as_str()]);
            git.run(&args)?
        }
        None => {
            args.extend(["--root", full_hash.as_str()]);
            git.run(&args)?
        }
    };
    Ok(CommitDetail {
        hash: full_hash,
        parents,
        author,
        email,
        timestamp,
        message,
        files: parse_raw_numstat(&out),
    })
}

/// Parses NUL-separated `--raw --numstat` output (raw entries first, then numstat in the same order).
fn parse_raw_numstat(out: &str) -> Vec<FileChange> {
    let mut files: Vec<FileChange> = Vec::new();
    let mut counts: Vec<(Option<u32>, Option<u32>)> = Vec::new();
    let mut tokens = out.split('\0').filter(|t| !t.is_empty());
    while let Some(tok) = tokens.next() {
        if tok.starts_with(':') {
            let status = tok
                .rsplit(' ')
                .next()
                .and_then(|s| s.chars().next())
                .unwrap_or('M');
            if let Some(path) = tokens.next() {
                files.push(FileChange {
                    path: path.to_string(),
                    status,
                    added: None,
                    deleted: None,
                });
            }
        } else {
            let mut f = tok.splitn(3, '\t');
            let added = f.next().and_then(|s| s.parse().ok());
            let deleted = f.next().and_then(|s| s.parse().ok());
            counts.push((added, deleted));
        }
    }
    for (file, (added, deleted)) in files.iter_mut().zip(counts) {
        file.added = added;
        file.deleted = deleted;
    }
    files
}

pub fn load_file_diff(git: &Git, hash: &str, path: &str) -> Result<String, GitError> {
    let parents = git.run(&["show", "-s", "--format=%P", hash, "--"])?;
    match parents.split_whitespace().next() {
        Some(p) => git.run(&["diff", "--no-ext-diff", "--no-color", p, hash, "--", path]),
        None => git.run(&[
            "diff-tree",
            "-p",
            "--no-ext-diff",
            "--root",
            "--no-commit-id",
            hash,
            "--",
            path,
        ]),
    }
}

/// Staged, unstaged and untracked changes. A path changed in both areas appears once.
pub fn load_worktree_changes(git: &Git) -> Result<Vec<FileChange>, GitError> {
    let status = git.run(&["status", "--porcelain=v1", "-z", "-uall"])?;
    // No HEAD yet (fresh repo): counts are simply unavailable.
    let counts = git
        .run(&[
            "diff",
            "--no-ext-diff",
            "--no-color",
            "HEAD",
            "--numstat",
            "-z",
        ])
        .unwrap_or_default();
    let mut count_map: HashMap<String, (Option<u32>, Option<u32>)> = HashMap::new();
    let mut toks = counts.split('\0').filter(|t| !t.is_empty());
    while let Some(tok) = toks.next() {
        let mut f = tok.splitn(3, '\t');
        let added = f.next().and_then(|s| s.parse().ok());
        let deleted = f.next().and_then(|s| s.parse().ok());
        let path = f.next().unwrap_or("");
        if path.is_empty() {
            // Rename: "a\td\t\0old\0new\0"
            toks.next();
            if let Some(new) = toks.next() {
                count_map.insert(new.to_string(), (added, deleted));
            }
        } else {
            count_map.insert(path.to_string(), (added, deleted));
        }
    }

    let mut files = Vec::new();
    let mut entries = status.split('\0').filter(|t| !t.is_empty());
    while let Some(e) = entries.next() {
        let mut chars = e.chars();
        let (x, y) = (chars.next().unwrap_or(' '), chars.next().unwrap_or(' '));
        let path = e.get(3..).unwrap_or("").to_string();
        if x == 'R' || x == 'C' || y == 'R' || y == 'C' {
            entries.next(); // original path
        }
        let status = if x == '?' {
            '?'
        } else if y != ' ' {
            y
        } else {
            x
        };
        let (added, deleted) = count_map.get(&path).copied().unwrap_or((None, None));
        files.push(FileChange {
            path,
            status,
            added,
            deleted,
        });
    }
    Ok(files)
}

/// Untracked files are read from disk (capped at `MAX_UNTRACKED_BYTES`); tracked ones use `git diff HEAD`.
pub fn load_worktree_diff(git: &Git, change: &FileChange) -> Result<String, GitError> {
    if change.status != '?' {
        return git.run(&[
            "diff",
            "--no-ext-diff",
            "--no-color",
            "HEAD",
            "--",
            &change.path,
        ]);
    }
    let mut buf = Vec::new();
    File::open(git.dir().join(&change.path))
        .and_then(|f| f.take(MAX_UNTRACKED_BYTES as u64 + 1).read_to_end(&mut buf))
        .map_err(GitError::Spawn)?;
    let truncated = buf.len() > MAX_UNTRACKED_BYTES;
    buf.truncate(MAX_UNTRACKED_BYTES);
    let mut text = match std::str::from_utf8(&buf) {
        Ok(s) => s.to_string(),
        // A multi-byte char cut by the limit: drop the partial tail.
        Err(e) if truncated && e.error_len().is_none() => {
            String::from_utf8_lossy(&buf[..e.valid_up_to()]).into_owned()
        }
        Err(_) => String::from_utf8_lossy(&buf).into_owned(),
    };
    if truncated {
        text.push('\n');
        text.push_str(TRUNCATED_MARKER);
    }
    Ok(text)
}

/// LRU cache of commit details, holding at most 256 entries.
pub struct DetailCache {
    map: HashMap<String, CommitDetail>,
    order: VecDeque<String>,
}

impl Default for DetailCache {
    fn default() -> Self {
        Self::new()
    }
}

impl DetailCache {
    pub fn new() -> Self {
        DetailCache {
            map: HashMap::new(),
            order: VecDeque::new(),
        }
    }

    pub fn get_or_load(&mut self, git: &Git, hash: &str) -> Result<&CommitDetail, GitError> {
        if self.map.contains_key(hash) {
            if let Some(pos) = self.order.iter().position(|h| h == hash) {
                let h = self.order.remove(pos).unwrap();
                self.order.push_back(h);
            }
        } else {
            let detail = load_detail(git, hash)?;
            if self.map.len() >= MAX_CACHED {
                if let Some(old) = self.order.pop_front() {
                    self.map.remove(&old);
                }
            }
            self.order.push_back(hash.to_string());
            self.map.insert(hash.to_string(), detail);
        }
        Ok(&self.map[hash])
    }
}
