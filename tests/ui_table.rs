mod support;

use std::io::Write;
use std::process::{Command as Process, Stdio};
use std::time::{SystemTime, UNIX_EPOCH};

use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui::Terminal;
use support::TestRepo;
use terminal_git_graph::app::{App, Command};
use terminal_git_graph::git::Git;
use terminal_git_graph::ui::layout::{columns_for, truncate_to_width, Columns};
use terminal_git_graph::ui::table::relative_time;
use terminal_git_graph::ui::theme::{lane_color, ColorDepth};
use unicode_width::UnicodeWidthStr;

fn app_for(repo: &TestRepo) -> App {
    App::new(Git::at(repo.path()), ColorDepth::TrueColor).expect("open app")
}

/// Screen text per row, skipping the hidden cells that follow a wide character.
fn lines(buf: &Buffer) -> Vec<String> {
    let area = buf.area;
    (0..area.height)
        .map(|y| {
            let mut line = String::new();
            let mut x = 0;
            while x < area.width {
                let sym = buf[(x, y)].symbol();
                line.push_str(sym);
                x += sym.width().max(1) as u16;
            }
            line
        })
        .collect()
}

fn render(app: &App, width: u16, height: u16) -> Vec<String> {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    lines(term.backend().buffer())
}

fn screen(app: &App, width: u16, height: u16) -> String {
    render(app, width, height).join("\n")
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64
}

fn timestamp(repo: &TestRepo, rev: &str) -> i64 {
    repo.git(&["log", "-1", "--format=%ct", rev])
        .trim()
        .parse()
        .unwrap()
}

/// Creates `n` linear commits on `main` quickly via fast-import.
fn fast_import_linear(repo: &TestRepo, n: usize) {
    let mut stream = String::new();
    for i in 1..=n {
        let msg = format!("commit {i}");
        stream.push_str("commit refs/heads/main\n");
        stream.push_str(&format!(
            "committer T <t@example.com> {} +0000\n",
            1_000_000_000 + i
        ));
        stream.push_str(&format!("data {}\n{}\n", msg.len(), msg));
    }
    let mut child = Process::new("git")
        .args(["fast-import", "--quiet"])
        .current_dir(repo.path())
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(stream.as_bytes())
        .unwrap();
    assert!(child.wait().unwrap().success());
    repo.git(&["reset", "--hard", "-q", "main"]);
}

#[test]
fn renders_rows_with_labels_and_head_check() {
    let repo = TestRepo::new();
    repo.commit("first commit");
    repo.commit("second commit");
    repo.tag("v1.0");
    let app = app_for(&repo);
    let out = screen(&app, 80, 10);
    assert!(out.contains("✓main"), "{out}");
    assert!(out.contains("v1.0"), "{out}");
    assert!(out.contains("second commit"), "{out}");
    assert!(out.contains("first commit"), "{out}");
    let date = relative_time(now(), timestamp(&repo, "HEAD"));
    let head_line = out.lines().find(|l| l.contains("second commit")).unwrap();
    assert!(head_line.contains(&date), "{head_line}");
    assert!(
        head_line.contains('◯'),
        "HEAD row uses the ring: {head_line}"
    );
    // Header shows the current branch.
    assert!(out.lines().next().unwrap().contains("main"), "{out}");
}

#[test]
fn narrow_width_hides_author_and_hash() {
    let repo = TestRepo::new();
    let hash = repo.commit("hello");
    let out = screen(&app_for(&repo), 50, 6);
    assert!(out.contains("hello"), "{out}");
    assert!(!out.contains("Test User"), "{out}");
    assert!(!out.contains(&hash[..7]), "{out}");
    assert_eq!(
        columns_for(50),
        Columns {
            author: false,
            hash: false
        }
    );
}

#[test]
fn wide_width_shows_author_and_hash() {
    let repo = TestRepo::new();
    let hash = repo.commit("hello");
    let out = screen(&app_for(&repo), 120, 6);
    assert!(out.contains("Test User"), "{out}");
    assert!(out.contains(&hash[..7]), "{out}");
    assert_eq!(
        columns_for(120),
        Columns {
            author: true,
            hash: true
        }
    );
    assert_eq!(
        columns_for(80),
        Columns {
            author: true,
            hash: false
        }
    );
}

#[test]
fn cjk_and_emoji_subject_truncated_by_display_width() {
    assert_eq!(truncate_to_width("한글한글", 5), "한글…");
    assert_eq!(truncate_to_width("한글한글", 8), "한글한글");
    assert_eq!(truncate_to_width("abc", 0), "");
    assert_eq!(truncate_to_width("🚀🚀🚀", 4), "🚀…");

    let repo = TestRepo::new();
    repo.commit("short");
    repo.commit("한글 제목 🚀 이모지가 섞인 아주 긴 커밋 메시지입니다 정말로 깁니다");
    let width = 50;
    let rows = render(&app_for(&repo), width, 6);
    let date = relative_time(now(), timestamp(&repo, "HEAD"));
    let cjk = rows.iter().find(|l| l.contains("한글")).expect("cjk row");
    let ascii = rows
        .iter()
        .find(|l| l.contains("short"))
        .expect("ascii row");
    assert!(cjk.contains('…'), "{cjk}");
    assert_eq!(cjk.width(), width as usize, "{cjk}");
    // The date column starts at the same display column in both rows.
    let col = |l: &str| l[..l.rfind(&date).unwrap()].width();
    assert_eq!(col(cjk), col(ascii), "{cjk}\n{ascii}");
}

#[test]
fn tiny_terminal_shows_too_small_message_without_panic() {
    let repo = TestRepo::new();
    repo.commit("hello");
    let app = app_for(&repo);
    let out = screen(&app, 10, 3);
    assert!(out.contains("Too small"), "{out}");

    let mut term = Terminal::new(TestBackend::new(20, 6)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    assert!(lines(term.backend().buffer()).join("\n").contains("hello"));
    term.backend_mut().resize(10, 6);
    term.draw(|f| app.draw(f)).unwrap();
    assert!(lines(term.backend().buffer())
        .join("\n")
        .contains("Too small"));
}

#[test]
fn empty_repo_shows_no_commits_message() {
    let repo = TestRepo::new();
    let app = app_for(&repo);
    assert!(screen(&app, 60, 6).contains("No commits yet"));
    assert_eq!(app.selected_hash(), None);
}

#[test]
fn scrolling_to_end_loads_next_page() {
    let repo = TestRepo::new();
    fast_import_linear(&repo, 350);
    let root = repo
        .git(&["rev-list", "--max-parents=0", "HEAD"])
        .trim()
        .to_string();
    let mut app = app_for(&repo);
    assert!(screen(&app, 60, 10).contains("300/350 commits"));
    app.apply(Command::Bottom).unwrap();
    let out = screen(&app, 60, 10);
    assert!(
        out.contains("350 commits") && !out.contains("300/350"),
        "{out}"
    );
    assert_eq!(app.selected_hash(), Some(root.as_str()));
    app.apply(Command::Top).unwrap();
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(app.selected_hash(), Some(head.as_str()));
}

#[test]
fn scope_and_remotes_toggles_reload() {
    let repo = TestRepo::new();
    repo.commit("on main");
    repo.branch("feature");
    repo.checkout("feature");
    repo.commit("on feature");
    repo.branch("gone");
    repo.checkout("gone");
    repo.commit("remote only");
    let _remote = repo.add_bare_remote("origin");
    repo.checkout("main");
    repo.git(&["branch", "-D", "gone"]);

    let mut app = app_for(&repo);
    let out = screen(&app, 80, 10);
    assert!(
        out.contains("on feature") && out.contains("remote only"),
        "{out}"
    );
    assert!(out.contains("origin/gone"), "{out}");

    app.apply(Command::ToggleRemotes).unwrap();
    let out = screen(&app, 80, 10);
    assert!(out.contains("on feature"), "{out}");
    assert!(!out.contains("remote only"), "{out}");

    app.apply(Command::ToggleScope).unwrap();
    let out = screen(&app, 80, 10);
    assert!(out.contains("on main"), "{out}");
    assert!(!out.contains("on feature"), "{out}");

    app.apply(Command::ToggleScope).unwrap();
    app.apply(Command::ToggleRemotes).unwrap();
    assert!(screen(&app, 80, 10).contains("remote only"));
}

#[test]
fn uncommitted_row_is_first_with_count() {
    let repo = TestRepo::new();
    let head = repo.commit("base");
    std::fs::write(repo.path().join("file-1.txt"), "changed\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "new\n").unwrap();
    let mut app = app_for(&repo);
    let rows = render(&app, 60, 6);
    assert!(rows[1].contains("Uncommitted changes (2)"), "{rows:?}");
    assert!(rows[2].contains("base"), "{rows:?}");
    assert_eq!(app.selected_hash(), None);
    app.apply(Command::Down).unwrap();
    assert_eq!(app.selected_hash(), Some(head.as_str()));
    app.apply(Command::SelectRow(0)).unwrap();
    assert_eq!(app.selected_hash(), None);
}

#[test]
fn bare_repo_has_no_uncommitted_row() {
    let repo = TestRepo::new();
    repo.commit("base");
    let bare = repo.add_bare_remote("origin");
    let app = App::new(Git::at(bare.path()), ColorDepth::TrueColor).unwrap();
    let out = screen(&app, 60, 6);
    assert!(!out.contains("Uncommitted"), "{out}");
    assert!(out.contains("base"), "{out}");
}

#[test]
fn color_depth_detect_table() {
    let cases = [
        (
            Some("truecolor"),
            Some("xterm-256color"),
            ColorDepth::TrueColor,
        ),
        (Some("24bit"), None, ColorDepth::TrueColor),
        (Some("TrueColor"), None, ColorDepth::TrueColor),
        (None, Some("xterm-direct"), ColorDepth::TrueColor),
        (None, Some("xterm-256color"), ColorDepth::Indexed256),
        (Some(""), None, ColorDepth::Indexed256),
        (None, None, ColorDepth::Indexed256),
    ];
    for (colorterm, term, want) in cases {
        assert_eq!(
            ColorDepth::detect(colorterm, term),
            want,
            "{colorterm:?} {term:?}"
        );
    }
    assert!(matches!(
        lane_color(0, ColorDepth::TrueColor),
        Color::Rgb(..)
    ));
    assert!(matches!(
        lane_color(0, ColorDepth::Indexed256),
        Color::Indexed(16..=255)
    ));
}

#[test]
fn relative_time_table() {
    let now = 1_700_000_000;
    let cases = [
        (0, "now"),
        (59, "now"),
        (60, "1m"),
        (5 * 60, "5m"),
        (3 * 3600, "3h"),
        (10 * 86400, "10d"),
        (60 * 86400, "2mo"),
        (400 * 86400, "1y"),
        (-30, "now"),
    ];
    for (ago, want) in cases {
        assert_eq!(relative_time(now, now - ago), want, "{ago}s ago");
    }
}
