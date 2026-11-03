//! Watches the repository for ref and working-tree changes, debounced.

use std::path::{Component, Path, PathBuf};
use std::sync::mpsc::{channel, RecvTimeoutError, Sender};
use std::thread;
use std::time::{Duration, Instant};

use notify::{Event, EventKind, RecommendedWatcher, RecursiveMode, Watcher};

use crate::git::Git;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WatchEvent {
    /// HEAD, a ref or packed-refs changed: the graph needs reloading.
    RefsChanged,
    /// A working-tree file changed: only the status needs refreshing.
    WorktreeChanged,
}

/// Quiet time after the last relevant change before an event is sent.
pub const DEBOUNCE_MS: u64 = 200;

/// Keeps the watcher alive; dropping it stops watching.
pub struct WatchHandle {
    _watcher: RecommendedWatcher,
}

/// Starts watching `git`'s repository, sending debounced events to `tx`.
pub fn watch(git: &Git, tx: Sender<WatchEvent>) -> notify::Result<WatchHandle> {
    let paths = Paths::resolve(git)?;
    let (raw_tx, raw_rx) = channel::<notify::Result<Event>>();
    let mut watcher = notify::recommended_watcher(raw_tx)?;
    if let Some(top) = &paths.worktree {
        watcher.watch(top, RecursiveMode::Recursive)?;
    }
    if paths
        .worktree
        .as_ref()
        .is_none_or(|top| !paths.common.starts_with(top))
    {
        watcher.watch(&paths.common, RecursiveMode::Recursive)?;
    }
    thread::spawn(move || {
        let debounce = Duration::from_millis(DEBOUNCE_MS);
        // Ends when the watcher (and its sender) is dropped or the receiver goes away.
        loop {
            let Ok(first) = raw_rx.recv() else { return };
            let mut refs = false;
            // Records a raw event; returns whether it was relevant at all.
            let mut note = |ev: notify::Result<Event>| -> bool {
                let kinds = ev.map(|e| paths.classify(&e)).unwrap_or_default();
                refs |= kinds.contains(&WatchEvent::RefsChanged);
                !kinds.is_empty()
            };
            if !note(first) {
                continue;
            }
            let mut deadline = Instant::now() + debounce;
            loop {
                let left = deadline.saturating_duration_since(Instant::now());
                match raw_rx.recv_timeout(left) {
                    Ok(ev) => {
                        if note(ev) {
                            deadline = Instant::now() + debounce;
                        }
                    }
                    Err(RecvTimeoutError::Timeout) => break,
                    Err(RecvTimeoutError::Disconnected) => return,
                }
            }
            // A ref reload also refreshes the status.
            let out = if refs {
                WatchEvent::RefsChanged
            } else {
                WatchEvent::WorktreeChanged
            };
            if tx.send(out).is_err() {
                return;
            }
        }
    });
    Ok(WatchHandle { _watcher: watcher })
}

/// Canonical repository locations used to classify changed paths.
struct Paths {
    /// `git rev-parse --git-common-dir`: shared refs live here.
    common: PathBuf,
    /// `git rev-parse --git-dir`: a linked worktree keeps its own HEAD here.
    git_dir: PathBuf,
    /// Working-tree root; `None` for a bare repository.
    worktree: Option<PathBuf>,
}

impl Paths {
    fn resolve(git: &Git) -> notify::Result<Paths> {
        let rev_parse = |arg: &str| -> notify::Result<PathBuf> {
            let out = git
                .run(&["rev-parse", arg])
                .map_err(|e| notify::Error::generic(&e.message()))?;
            // Relative results are relative to the repo directory; canonicalize to match
            // the paths the OS reports (e.g. /private/var on macOS).
            Ok(git.dir().join(out.trim()).canonicalize()?)
        };
        let common = rev_parse("--git-common-dir")?;
        let git_dir = rev_parse("--git-dir")?;
        let bare = git
            .run(&["rev-parse", "--is-bare-repository"])
            .map_err(|e| notify::Error::generic(&e.message()))?
            .trim()
            == "true";
        let worktree = if bare {
            None
        } else {
            Some(rev_parse("--show-toplevel")?)
        };
        Ok(Paths {
            common,
            git_dir,
            worktree,
        })
    }

    fn classify(&self, event: &Event) -> Vec<WatchEvent> {
        if matches!(event.kind, EventKind::Access(_)) {
            return Vec::new();
        }
        event
            .paths
            .iter()
            .filter_map(|p| self.classify_path(p))
            .collect()
    }

    fn classify_path(&self, path: &Path) -> Option<WatchEvent> {
        if path == self.git_dir.join("HEAD") {
            return Some(WatchEvent::RefsChanged);
        }
        if let Ok(rel) = path.strip_prefix(&self.common) {
            let is_ref = rel == Path::new("HEAD")
                || rel == Path::new("packed-refs")
                || rel.starts_with("refs");
            return is_ref.then_some(WatchEvent::RefsChanged);
        }
        if path.starts_with(&self.git_dir) {
            return None;
        }
        let rel = path.strip_prefix(self.worktree.as_ref()?).ok()?;
        // Other .git internals (index, locks, a submodule's .git) are noise.
        if rel
            .components()
            .any(|c| c == Component::Normal(".git".as_ref()))
        {
            return None;
        }
        Some(WatchEvent::WorktreeChanged)
    }
}
