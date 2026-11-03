use super::{Git, GitError};

pub const PAGE_SIZE: usize = 300;

#[derive(Clone, Debug, PartialEq)]
pub struct Commit {
    pub hash: String,
    pub parents: Vec<String>,
    pub author: String,
    pub email: String,
    pub timestamp: i64,
    pub subject: String,
}

impl Commit {
    /// Builds a commit with empty author/email; for tests and lane computation.
    pub fn new(hash: &str, parents: &[&str], timestamp: i64, subject: &str) -> Commit {
        Commit {
            hash: hash.to_string(),
            parents: parents.iter().map(|p| p.to_string()).collect(),
            author: String::new(),
            email: String::new(),
            timestamp,
            subject: subject.to_string(),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scope {
    AllBranches,
    Current,
}

#[derive(Clone, Debug)]
pub struct LogQuery {
    pub scope: Scope,
    pub remotes: bool,
    pub skip: usize,
    pub limit: usize,
}

fn scope_args(scope: Scope, remotes: bool) -> Vec<&'static str> {
    match scope {
        Scope::AllBranches => {
            let mut v = vec!["--branches", "--tags"];
            if remotes {
                v.push("--remotes");
            }
            v
        }
        Scope::Current => vec!["HEAD"],
    }
}

/// A repository whose HEAD has no commit yet makes `HEAD` unresolvable.
fn is_unborn_error(e: &GitError) -> bool {
    match e {
        GitError::Failed { stderr, .. } => {
            stderr.contains("does not have any commits yet")
                || stderr.contains("ambiguous argument 'HEAD'")
                || stderr.contains("bad revision 'HEAD'")
        }
        _ => false,
    }
}

pub fn load_commits(git: &Git, q: &LogQuery) -> Result<Vec<Commit>, GitError> {
    let skip = q.skip.to_string();
    let limit = q.limit.to_string();
    let mut args = vec![
        "log",
        "--date-order",
        "-z",
        "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%ct%x1f%s",
        "--skip",
        &skip,
        "-n",
        &limit,
    ];
    args.extend(scope_args(q.scope, q.remotes));
    args.push("--");
    match git.run(&args) {
        Ok(out) => Ok(out.split('\0').filter_map(parse_commit).collect()),
        Err(e) if is_unborn_error(&e) => Ok(Vec::new()),
        Err(e) => Err(e),
    }
}

fn parse_commit(record: &str) -> Option<Commit> {
    let record = record.trim_start_matches('\n');
    if record.is_empty() {
        return None;
    }
    let mut f = record.split('\x1f');
    let hash = f.next()?.to_string();
    let parents = f
        .next()?
        .split_whitespace()
        .map(str::to_string)
        .collect();
    let author = f.next()?.to_string();
    let email = f.next()?.to_string();
    let timestamp = f.next()?.parse().ok()?;
    let subject = f.next()?.to_string();
    Some(Commit { hash, parents, author, email, timestamp, subject })
}

pub fn count_commits(git: &Git, scope: Scope, remotes: bool) -> Result<usize, GitError> {
    let mut args = vec!["rev-list", "--count"];
    args.extend(scope_args(scope, remotes));
    args.push("--");
    match git.run(&args) {
        Ok(out) => Ok(out.trim().parse().unwrap_or(0)),
        Err(e) if is_unborn_error(&e) => Ok(0),
        Err(e) => Err(e),
    }
}
