use std::fs;
use std::path::{Path, PathBuf};

use super::RepoEntry;

const SKIPPED_DIRS: [&str; 5] = ["node_modules", "build", "dist", "vendor", "target"];

#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    #[error("refusing to scan the home folder or one of its ancestors")]
    RootIsHomeOrAncestor,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Finds git repositories under `root`, at most `depth` folder levels down
/// (children of `root` are level 1). Results are sorted by group, then name.
pub fn scan(root: &Path, depth: u8, home: &Path) -> Result<Vec<RepoEntry>, ScanError> {
    let canon_root = fs::canonicalize(root)?;
    let canon_home = fs::canonicalize(home).unwrap_or_else(|_| home.to_path_buf());
    if canon_home.starts_with(&canon_root) {
        return Err(ScanError::RootIsHomeOrAncestor);
    }
    let mut found = Vec::new();
    walk(root, depth, &mut found);
    found.sort_by(|a, b| (&a.group, &a.name).cmp(&(&b.group, &b.name)));
    Ok(found)
}

fn walk(dir: &Path, levels_left: u8, out: &mut Vec<RepoEntry>) {
    if levels_left == 0 {
        return;
    }
    let Ok(read) = fs::read_dir(dir) else { return };
    let mut subdirs: Vec<PathBuf> = Vec::new();
    for entry in read.flatten() {
        let name = entry.file_name();
        let Some(name) = name.to_str() else { continue };
        if name.starts_with('.') || SKIPPED_DIRS.contains(&name) {
            continue;
        }
        let path = entry.path();
        if path.is_dir() {
            subdirs.push(path);
        }
    }
    for path in subdirs {
        if path.join(".git").exists() {
            out.push(RepoEntry {
                name: path
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                group: dir.to_path_buf(),
                path,
            });
        } else {
            walk(&path, levels_left - 1, out);
        }
    }
}
