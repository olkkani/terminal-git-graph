//! Runs git actions on a background thread so the UI keeps responding.

use std::sync::mpsc::{self, Receiver};
use std::thread;

use crate::actions::{execute, Action};
use crate::git::{Git, GitError};

pub struct JobRunner;

impl JobRunner {
    pub fn new() -> Self {
        JobRunner
    }

    /// Starts `a` on its own thread; its result arrives once on the returned channel.
    pub fn spawn(&self, git: Git, a: Action) -> Receiver<Result<String, GitError>> {
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            // The receiver may be gone if the app quit; nothing to report then.
            let _ = tx.send(execute(&git, &a));
        });
        rx
    }
}

impl Default for JobRunner {
    fn default() -> Self {
        Self::new()
    }
}
