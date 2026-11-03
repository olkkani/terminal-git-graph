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

/// `head` adds HEAD to All branches so a detached HEAD outside every ref still shows.
fn scope_args(scope: Scope, remotes: bool, head: bool) -> Vec<&'static str> {
    match scope {
        Scope::AllBranches => {
            let mut v = vec!["--branches", "--tags"];
            if remotes {
                v.push("--remotes");
            }
            if head {
                v.push("HEAD");
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
    let args = [
        "log",
        "--date-order",
        "-z",
        "--format=%H%x1f%P%x1f%an%x1f%ae%x1f%ct%x1f%s",
        "--skip",
        &skip,
        "-n",
        &limit,
    ];
    Ok(run_scoped(git, &args, q.scope, q.remotes)?
        .map(|out| out.split('\0').filter_map(parse_commit).collect())
        .unwrap_or_default())
}

/// Runs `base` + the scope's revisions + `--`. `None` when an unborn HEAD leaves nothing to list;
/// for All branches an unborn HEAD is dropped and the branches are still listed.
fn run_scoped(
    git: &Git,
    base: &[&str],
    scope: Scope,
    remotes: bool,
) -> Result<Option<String>, GitError> {
    for head in [true, false] {
        let mut args = base.to_vec();
        args.extend(scope_args(scope, remotes, head));
        args.push("--");
        match git.run(&args) {
            Ok(out) => return Ok(Some(out)),
            Err(e) if is_unborn_error(&e) && scope == Scope::AllBranches && head => continue,
            Err(e) if is_unborn_error(&e) => return Ok(None),
            Err(e) => return Err(e),
        }
    }
    Ok(None)
}

fn parse_commit(record: &str) -> Option<Commit> {
    let record = record.trim_start_matches('\n');
    if record.is_empty() {
        return None;
    }
    let mut f = record.split('\x1f');
    let hash = f.next()?.to_string();
    let parents = f.next()?.split_whitespace().map(str::to_string).collect();
    let author = f.next()?.to_string();
    let email = f.next()?.to_string();
    let timestamp = f.next()?.parse().ok()?;
    let subject = f.next()?.to_string();
    Some(Commit {
        hash,
        parents,
        author,
        email,
        timestamp,
        subject,
    })
}

pub fn count_commits(git: &Git, scope: Scope, remotes: bool) -> Result<usize, GitError> {
    match run_scoped(git, &["rev-list", "--count"], scope, remotes)? {
        Some(out) => parse_count(&out),
        None => Ok(0),
    }
}

fn parse_count(out: &str) -> Result<usize, GitError> {
    out.trim()
        .parse()
        .map_err(|_| GitError::Parse(format!("rev-list --count printed {:?}", out.trim())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn count_parses_number_and_rejects_garbage() {
        assert_eq!(parse_count("42\n").unwrap(), 42);
        assert!(parse_count("").is_err());
        assert!(parse_count("not a number\n").is_err());
    }
}
