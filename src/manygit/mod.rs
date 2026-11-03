//! Manygit: discover git repositories under a folder and summarize their state.

pub mod scan;
pub mod status;

use std::path::PathBuf;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RepoEntry {
    pub path: PathBuf,
    pub name: String,
    /// Parent folder of the repository.
    pub group: PathBuf,
}
