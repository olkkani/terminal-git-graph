mod support;

use std::process::{Command, Output, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use support::TestRepo;

/// Runs `tgg` with `args`; stdout and stderr are pipes, so stdout is never a terminal.
/// Fails the test instead of hanging if `tgg` does not exit within a few seconds.
fn tgg(args: &[&str]) -> Output {
    let mut child = Command::new(env!("CARGO_BIN_EXE_tgg"))
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("run tgg");
    let deadline = Instant::now() + Duration::from_secs(10);
    while child.try_wait().expect("wait for tgg").is_none() {
        if Instant::now() > deadline {
            let _ = child.kill();
            panic!("tgg did not exit");
        }
        thread::sleep(Duration::from_millis(20));
    }
    child.wait_with_output().expect("collect tgg output")
}

fn stdout(out: &Output) -> String {
    String::from_utf8_lossy(&out.stdout).into_owned()
}

fn stderr(out: &Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn version_flag_prints_package_version() {
    let out = tgg(&["--version"]);
    assert!(out.status.success());
    assert!(
        stdout(&out).contains(env!("CARGO_PKG_VERSION")),
        "stdout: {}",
        stdout(&out)
    );
}

#[test]
fn help_mentions_path_argument() {
    let out = tgg(&["--help"]);
    assert!(out.status.success());
    assert!(stdout(&out).contains("[PATH]"), "stdout: {}", stdout(&out));
}

#[test]
fn missing_path_exits_2_with_message() {
    let dir = tempfile::tempdir().unwrap();
    let missing = dir.path().join("nope");
    let out = tgg(&[missing.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("path not found"),
        "stderr: {}",
        stderr(&out)
    );
    assert!(stderr(&out).contains("nope"), "stderr: {}", stderr(&out));
}

#[test]
fn file_path_exits_2_with_message() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("plain.txt");
    std::fs::write(&file, "x").unwrap();
    let out = tgg(&[file.to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(2));
    assert!(
        stderr(&out).contains("not a folder"),
        "stderr: {}",
        stderr(&out)
    );
}

#[test]
fn non_tty_stdout_exits_with_error_instead_of_hanging() {
    let repo = TestRepo::new();
    repo.commit("first");
    let out = tgg(&[repo.path().to_str().unwrap()]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        stderr(&out).contains("terminal"),
        "stderr: {}",
        stderr(&out)
    );
}
