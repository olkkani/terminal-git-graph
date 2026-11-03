//! A large synthetic history for performance tests, written with `git fast-import`.

use std::fmt::Write as _;
use std::io::Write as _;
use std::process::{Command, Stdio};

use super::{TestRepo, BASE_TIME};

/// Main-line commits between two merged topic branches.
const MERGE_EVERY: usize = 20;
/// Commits on each topic branch before it is merged back.
const TOPIC_LEN: usize = 3;
/// Commits between tags on main.
const TAG_EVERY: usize = 1000;
/// Files the commits cycle through, so the worktree stays small.
const FILES: usize = 100;

/// Builds a repository with exactly `commits` commits on `main`: a main line with a
/// three-commit topic branch merged back after every 20 main-line commits, and a tag
/// about every 1000 commits. The worktree is checked out and clean.
pub fn generate(commits: usize) -> TestRepo {
    let repo = TestRepo::new();
    let mut gen = Stream {
        out: String::new(),
        mark: 0,
        time: BASE_TIME,
    };
    let mut main: Option<usize> = None;
    let mut made = 0;
    let mut since_merge = 0;
    let mut next_tag = TAG_EVERY;
    while made < commits {
        if since_merge == MERGE_EVERY && commits - made > TOPIC_LEN {
            let base = main.expect("main has commits before the first merge");
            let mut topic = base;
            for _ in 0..TOPIC_LEN {
                topic = gen.commit("refs/heads/topic", Some(topic), None);
            }
            main = Some(gen.commit("refs/heads/main", main, Some(topic)));
            made += TOPIC_LEN + 1;
            since_merge = 0;
        } else {
            main = Some(gen.commit("refs/heads/main", main, None));
            made += 1;
            since_merge += 1;
        }
        if made >= next_tag {
            let mark = main.expect("main has a commit");
            let _ = write!(
                &mut gen.out,
                "reset refs/tags/v{next_tag}\nfrom :{mark}\n\n"
            );
            next_tag += TAG_EVERY;
        }
    }
    let _ = writeln!(&mut gen.out, "done");

    let mut child = Command::new("git")
        .args(["fast-import", "--quiet", "--done"])
        .current_dir(repo.path())
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .stdin(Stdio::piped())
        .spawn()
        .expect("spawn git fast-import");
    child
        .stdin
        .take()
        .expect("fast-import stdin")
        .write_all(gen.out.as_bytes())
        .expect("write fast-import stream");
    let status = child.wait().expect("wait for git fast-import");
    assert!(status.success(), "git fast-import failed");
    repo.git(&["reset", "--hard", "-q", "main"]);
    repo.clock.set(gen.time);
    repo
}

/// Writes fast-import commands; marks number the commits from 1.
struct Stream {
    out: String,
    mark: usize,
    time: i64,
}

impl Stream {
    /// Appends a commit on `branch` with parents `from` then `merge`; returns its mark.
    fn commit(&mut self, branch: &str, from: Option<usize>, merge: Option<usize>) -> usize {
        self.mark += 1;
        self.time += 60;
        let n = self.mark;
        let message = format!("commit {n}");
        let content = format!("{n}\n");
        let out = &mut self.out;
        let _ = writeln!(out, "commit {branch}\nmark :{n}");
        let _ = writeln!(
            out,
            "author Test User <test@example.com> {} +0000",
            self.time
        );
        let _ = writeln!(
            out,
            "committer Test User <test@example.com> {} +0000",
            self.time
        );
        let _ = writeln!(out, "data {}\n{message}", message.len());
        if let Some(p) = from {
            let _ = writeln!(out, "from :{p}");
        }
        if let Some(p) = merge {
            let _ = writeln!(out, "merge :{p}");
        }
        let _ = writeln!(out, "M 100644 inline file-{}.txt", n % FILES);
        let _ = writeln!(out, "data {}\n{content}", content.len());
        n
    }
}
