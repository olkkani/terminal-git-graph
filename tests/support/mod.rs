#![allow(dead_code)]

pub mod big_repo;

use std::cell::Cell;
use std::fs;
use std::path::Path;
use std::process::Command;

use tempfile::TempDir;

/// Base timestamp for deterministic commit dates (2001-09-09T01:46:40Z).
const BASE_TIME: i64 = 1_000_000_000;

pub struct TestRepo {
    dir: TempDir,
    clock: Cell<i64>,
    counter: Cell<u32>,
}

impl TestRepo {
    pub fn new() -> Self {
        let repo = TestRepo {
            dir: tempfile::tempdir().expect("create tempdir"),
            clock: Cell::new(BASE_TIME),
            counter: Cell::new(0),
        };
        repo.git(&["init", "-b", "main"]);
        repo.git(&["config", "user.name", "Test User"]);
        repo.git(&["config", "user.email", "test@example.com"]);
        repo.git(&["config", "commit.gpgsign", "false"]);
        repo.git(&["config", "tag.gpgsign", "false"]);
        repo
    }

    pub fn path(&self) -> &Path {
        self.dir.path()
    }

    /// Runs git in the repo; panics on failure; returns stdout.
    pub fn git(&self, args: &[&str]) -> String {
        run_git(self.path(), args, self.next_date())
    }

    pub fn commit(&self, subject: &str) -> String {
        let n = self.counter.get() + 1;
        self.counter.set(n);
        self.commit_file(&format!("file-{n}.txt"), &format!("{subject}\n"), subject)
    }

    pub fn commit_file(&self, path: &str, content: &str, subject: &str) -> String {
        let full = self.path().join(path);
        if let Some(parent) = full.parent() {
            fs::create_dir_all(parent).expect("create parent dirs");
        }
        fs::write(&full, content).expect("write file");
        self.git(&["add", "--", path]);
        self.git(&["commit", "-m", subject]);
        self.git(&["rev-parse", "HEAD"]).trim().to_string()
    }

    /// Creates a branch at the current commit without checking it out.
    pub fn branch(&self, name: &str) {
        self.git(&["branch", name]);
    }

    pub fn checkout(&self, name: &str) {
        self.git(&["checkout", name]);
    }

    pub fn merge(&self, name: &str, no_ff: bool) -> String {
        let message = format!("Merge {name}");
        let mode = if no_ff { "--no-ff" } else { "--ff" };
        self.git(&["merge", mode, "-m", &message, name])
    }

    pub fn tag(&self, name: &str) {
        self.git(&["tag", name]);
    }

    /// Creates a bare repo, registers it as a remote, and pushes all refs.
    pub fn add_bare_remote(&self, name: &str) -> TempDir {
        let bare = tempfile::tempdir().expect("create bare tempdir");
        run_git(
            bare.path(),
            &["init", "--bare", "-b", "main"],
            self.next_date(),
        );
        let url = bare.path().to_str().expect("utf-8 path");
        self.git(&["remote", "add", name, url]);
        self.git(&["push", name, "--all"]);
        self.git(&["push", name, "--tags"]);
        bare
    }

    /// Advances the fake clock by 60s per call.
    fn next_date(&self) -> String {
        let t = self.clock.get() + 60;
        self.clock.set(t);
        format!("{t} +0000")
    }
}

fn run_git(dir: &Path, args: &[&str], date: String) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_AUTHOR_DATE", &date)
        .env("GIT_COMMITTER_DATE", &date)
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("spawn git");
    assert!(
        output.status.success(),
        "git {:?} failed: {}",
        args,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).expect("utf-8 stdout")
}
