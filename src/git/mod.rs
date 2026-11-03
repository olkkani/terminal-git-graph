pub mod detail;
pub mod log;
pub mod refs;
pub mod status;

use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

#[derive(Debug, thiserror::Error)]
pub enum GitError {
    #[error("git {args:?} failed: {}", .stderr)]
    Failed {
        args: Vec<String>,
        code: Option<i32>,
        stdout: String,
        stderr: String,
    },
    #[error("failed to run git: {0}")]
    Spawn(std::io::Error),
    #[error("invalid name: {0:?}")]
    InvalidName(String),
}

impl GitError {
    /// stderr and stdout joined and trimmed. Conflict messages appear on stdout.
    pub fn message(&self) -> String {
        match self {
            GitError::Failed { stdout, stderr, .. } => {
                let parts: Vec<&str> = [stderr.trim(), stdout.trim()]
                    .into_iter()
                    .filter(|s| !s.is_empty())
                    .collect();
                parts.join("\n")
            }
            GitError::Spawn(e) => e.to_string(),
            GitError::InvalidName(n) => format!("invalid name: {n:?}"),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Git {
    dir: PathBuf,
    calls: Arc<AtomicUsize>,
}

impl Git {
    pub fn at(dir: impl Into<PathBuf>) -> Git {
        Git {
            dir: dir.into(),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }

    pub fn dir(&self) -> &Path {
        &self.dir
    }

    pub fn run(&self, args: &[&str]) -> Result<String, GitError> {
        self.run_with_env(args, &[])
    }

    pub fn run_with_env(&self, args: &[&str], env: &[(&str, &str)]) -> Result<String, GitError> {
        self.calls.fetch_add(1, Ordering::Relaxed);
        let mut cmd = Command::new("git");
        cmd.args([
            "-c",
            "color.ui=never",
            "-c",
            "color.diff=never",
            "-c",
            "color.status=never",
            "-c",
            "color.branch=never",
            "-c",
            "core.pager=cat",
            "-c",
            "diff.external=",
            "-c",
            "core.quotepath=off",
        ])
        .args(args)
        .current_dir(&self.dir)
        .stdin(Stdio::null())
        .env("LC_ALL", "C")
        .env("GIT_TERMINAL_PROMPT", "0");
        for (k, v) in env {
            cmd.env(k, v);
        }
        let output = cmd.output().map_err(GitError::Spawn)?;
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        if output.status.success() {
            Ok(stdout)
        } else {
            Err(GitError::Failed {
                args: args.iter().map(|s| s.to_string()).collect(),
                code: output.status.code(),
                stdout,
                stderr: String::from_utf8_lossy(&output.stderr).into_owned(),
            })
        }
    }

    pub fn call_count(&self) -> usize {
        self.calls.load(Ordering::Relaxed)
    }
}

/// Rejects names that git could mistake for options or that contain whitespace/control characters.
pub fn validate_name(name: &str) -> Result<(), GitError> {
    if name.is_empty()
        || name.starts_with('-')
        || name.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(GitError::InvalidName(name.to_string()));
    }
    Ok(())
}
