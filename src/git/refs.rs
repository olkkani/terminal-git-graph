use super::{Git, GitError};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum RefKind {
    /// The `HEAD` badge of a detached HEAD.
    Head,
    LocalBranch,
    RemoteBranch,
    Tag,
}

#[derive(Clone, Debug, PartialEq)]
pub struct RefLabel {
    pub name: String,
    pub kind: RefKind,
    pub target: String,
    /// For local branches: remotes holding a same-named branch at the same commit.
    pub remotes: Vec<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Head {
    Branch(String),
    /// Commit hash.
    Detached(String),
    /// Branch name.
    Unborn(String),
}

#[derive(Clone, Debug)]
pub struct Refs {
    pub labels: Vec<RefLabel>,
    pub head: Head,
}

impl Refs {
    pub fn labels_for(&self, hash: &str) -> Vec<&RefLabel> {
        self.labels.iter().filter(|l| l.target == hash).collect()
    }
}

pub fn load_refs(git: &Git) -> Result<Refs, GitError> {
    let out = git.run(&[
        "for-each-ref",
        "--format=%(refname)%1f%(objectname)%1f%(*objectname)",
        "refs/heads",
        "refs/remotes",
        "refs/tags",
    ])?;

    let mut locals: Vec<RefLabel> = Vec::new();
    let mut remote_branches: Vec<(String, String, String)> = Vec::new(); // (remote, branch, target)
    let mut tags: Vec<RefLabel> = Vec::new();
    for line in out.lines() {
        let mut f = line.split('\x1f');
        let (Some(refname), Some(obj), Some(peeled)) = (f.next(), f.next(), f.next()) else {
            continue;
        };
        if let Some(name) = refname.strip_prefix("refs/heads/") {
            locals.push(RefLabel {
                name: name.to_string(),
                kind: RefKind::LocalBranch,
                target: obj.to_string(),
                remotes: Vec::new(),
            });
        } else if let Some(rest) = refname.strip_prefix("refs/remotes/") {
            let Some((remote, branch)) = rest.split_once('/') else {
                continue;
            };
            if branch == "HEAD" {
                continue;
            }
            remote_branches.push((remote.to_string(), branch.to_string(), obj.to_string()));
        } else if let Some(name) = refname.strip_prefix("refs/tags/") {
            let target = if peeled.is_empty() { obj } else { peeled };
            tags.push(RefLabel {
                name: name.to_string(),
                kind: RefKind::Tag,
                target: target.to_string(),
                remotes: Vec::new(),
            });
        }
    }

    let mut remote_labels: Vec<RefLabel> = Vec::new();
    for (remote, branch, target) in remote_branches {
        match locals
            .iter_mut()
            .find(|l| l.name == branch && l.target == target)
        {
            Some(local) => local.remotes.push(remote),
            None => remote_labels.push(RefLabel {
                name: format!("{remote}/{branch}"),
                kind: RefKind::RemoteBranch,
                target,
                remotes: Vec::new(),
            }),
        }
    }

    let head = load_head(git)?;
    let mut labels = Vec::new();
    if let Head::Detached(hash) = &head {
        labels.push(RefLabel {
            name: "HEAD".to_string(),
            kind: RefKind::Head,
            target: hash.clone(),
            remotes: Vec::new(),
        });
    }
    labels.extend(locals);
    labels.extend(remote_labels);
    labels.extend(tags);
    Ok(Refs { labels, head })
}

fn load_head(git: &Git) -> Result<Head, GitError> {
    let resolved = git
        .run(&["rev-parse", "--verify", "-q", "HEAD"])
        .ok()
        .map(|s| s.trim().to_string());
    match git.run(&["symbolic-ref", "-q", "HEAD"]) {
        Ok(sym) => {
            let branch = sym.trim().trim_start_matches("refs/heads/").to_string();
            Ok(match resolved {
                Some(_) => Head::Branch(branch),
                None => Head::Unborn(branch),
            })
        }
        // Exit status 1 with -q means HEAD is not symbolic: detached.
        Err(GitError::Failed { code: Some(1), .. }) => match resolved {
            Some(hash) => Ok(Head::Detached(hash)),
            None => Err(GitError::Failed {
                args: vec!["rev-parse".into(), "HEAD".into()],
                code: Some(1),
                stdout: String::new(),
                stderr: "cannot resolve HEAD".into(),
            }),
        },
        Err(e) => Err(e),
    }
}
