mod support;

use std::fs;
use std::path::Path;
use std::process::Command;

use support::TestRepo;
use terminal_git_graph::repo::{detect, RepoError, RepoTarget};

fn canon(p: &Path) -> std::path::PathBuf {
    fs::canonicalize(p).unwrap()
}

#[test]
fn subdirectory_resolves_to_toplevel() {
    let repo = TestRepo::new();
    repo.commit_file("sub/dir/f.txt", "x\n", "c");
    match detect(&repo.path().join("sub/dir")).unwrap() {
        RepoTarget::Repo { git, bare } => {
            assert!(!bare);
            assert_eq!(canon(git.dir()), canon(repo.path()));
        }
        RepoTarget::Folder(_) => panic!("expected Repo"),
    }
}

#[test]
fn linked_worktree_is_a_repo() {
    let repo = TestRepo::new();
    repo.commit("first");
    let parent = tempfile::tempdir().unwrap();
    let wt = parent.path().join("wt");
    repo.git(&["worktree", "add", "-b", "other", wt.to_str().unwrap()]);
    match detect(&wt).unwrap() {
        RepoTarget::Repo { git, bare } => {
            assert!(!bare);
            assert_eq!(canon(git.dir()), canon(&wt));
        }
        RepoTarget::Folder(_) => panic!("expected Repo"),
    }
}

#[test]
fn bare_repo_dir_is_bare_true() {
    let repo = TestRepo::new();
    repo.commit("first");
    let remote = repo.add_bare_remote("origin");
    match detect(remote.path()).unwrap() {
        RepoTarget::Repo { git, bare } => {
            assert!(bare);
            assert_eq!(canon(git.dir()), canon(remote.path()));
        }
        RepoTarget::Folder(_) => panic!("expected Repo"),
    }
}

#[test]
fn plain_folder_is_folder() {
    let dir = tempfile::tempdir().unwrap();
    match detect(dir.path()).unwrap() {
        RepoTarget::Folder(p) => assert_eq!(canon(&p), canon(dir.path())),
        RepoTarget::Repo { .. } => panic!("expected Folder"),
    }
}

#[test]
fn workspace_with_dot_bare_and_worktrees_is_folder() {
    let ws = tempfile::tempdir().unwrap();
    let bare = ws.path().join(".bare");
    let status = Command::new("git")
        .args(["init", "--bare", "-b", "main"])
        .arg(&bare)
        .status()
        .unwrap();
    assert!(status.success());
    fs::create_dir(ws.path().join("develop")).unwrap();
    match detect(ws.path()).unwrap() {
        RepoTarget::Folder(_) => {}
        RepoTarget::Repo { .. } => panic!("workspace root must be a Folder"),
    }
}

#[test]
fn missing_path_is_not_found() {
    let dir = tempfile::tempdir().unwrap();
    let err = detect(&dir.path().join("nope")).unwrap_err();
    assert!(matches!(err, RepoError::NotFound(_)));
}
