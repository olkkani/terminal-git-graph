use std::path::{Path, PathBuf};

use crate::git::Git;

#[derive(Debug)]
pub enum RepoTarget {
    Repo { git: Git, bare: bool },
    Folder(PathBuf),
}

#[derive(Debug, thiserror::Error)]
pub enum RepoError {
    #[error("path not found: {0}")]
    NotFound(PathBuf),
}

pub fn detect(path: &Path) -> Result<RepoTarget, RepoError> {
    if !path.exists() {
        return Err(RepoError::NotFound(path.to_path_buf()));
    }
    let probe = Git::at(path);
    let folder = || Ok(RepoTarget::Folder(path.to_path_buf()));
    let Ok(bare_out) = probe.run(&["rev-parse", "--is-bare-repository"]) else {
        return folder();
    };
    if bare_out.trim() == "true" {
        return Ok(RepoTarget::Repo {
            git: probe,
            bare: true,
        });
    }
    match probe.run(&["rev-parse", "--show-toplevel"]) {
        Ok(top) => Ok(RepoTarget::Repo {
            git: Git::at(top.trim()),
            bare: false,
        }),
        Err(_) => folder(),
    }
}
