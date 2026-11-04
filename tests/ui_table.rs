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
    let out = screen(&app, 19, 3);
    assert!(out.contains("Terminal too small"), "{out}");

    let mut term = Terminal::new(TestBackend::new(20, 6)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    assert!(lines(term.backend().buffer()).join("\n").contains("hello"));
    term.backend_mut().resize(19, 6);
    term.draw(|f| app.draw(f)).unwrap();
    assert!(lines(term.backend().buffer())
        .join("\n")
        .contains("Terminal too small"));
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
    repo.commit("one");
    repo.commit("two");
    let head = repo.commit("base");
    for n in 1..=3 {
        std::fs::write(repo.path().join(format!("file-{n}.txt")), "changed\n").unwrap();
    }
    std::fs::write(repo.path().join("new.txt"), "new\n").unwrap();
    std::fs::write(repo.path().join("new2.txt"), "new\n").unwrap();
    let mut app = app_for(&repo);
    let rows = render(&app, 100, 6);
    assert!(
        rows[2].contains("Uncommitted changes (5: 3 tracked, 2 untracked)"),
        "{rows:?}"
    );
    assert!(rows[3].contains("base"), "{rows:?}");
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

#[test]
fn header_shows_refresh_error_truncated() {
    use terminal_git_graph::watch::WatchEvent;
    let repo = TestRepo::new();
    repo.commit("first");
    let mut app = app_for(&repo);
    let dot_git = repo.path().join(".git");
    let away = repo.path().join("git-away");
    std::fs::rename(&dot_git, &away).unwrap();
    app.on_watch(WatchEvent::RefsChanged);
    std::fs::rename(&away, &dot_git).unwrap();
    let err = app.refresh_error().expect("refresh failed").to_string();

    let rows = render(&app, 80, 6);
    let header = &rows[0];
    let rest = header
        .strip_prefix("(⑂ main)  ! ")
        .unwrap_or_else(|| panic!("{header:?}"));
    // The first line of git's message, cut to 40 cells (including "  ! ") with an ellipsis.
    let shown = &rest[..rest.find('…').expect("truncated")];
    assert!(err.starts_with(shown), "{shown:?} vs {err:?}");
    assert_eq!(format!("  ! {shown}…").width(), 40);
    // The commit count still fits after it.
    assert!(header.trim_end().ends_with("1 commit"), "{header:?}");

    // A successful refresh clears it.
    app.on_watch(WatchEvent::RefsChanged);
    assert!(!render(&app, 80, 6)[0].contains('!'));
}

#[test]
fn focused_label_is_highlighted() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("feature");
    let mut app = app_for(&repo);
    let (w, h) = (60, 6);
    let reversed = |app: &App| {
        let mut term = Terminal::new(TestBackend::new(w, h)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        let buf = term.backend().buffer().clone();
        let y = (0..h)
            .find(|&y| {
                (0..w)
                    .map(|x| buf[(x, y)].symbol())
                    .collect::<String>()
                    .contains("feature")
            })
            .expect("row with the label");
        let line: String = (0..w).map(|x| buf[(x, y)].symbol()).collect();
        let x = line.find("feature").unwrap() as u16;
        buf[(x, y)]
            .modifier
            .contains(ratatui::style::Modifier::REVERSED)
    };
    assert!(!reversed(&app));
    // labels_for order is not fixed: step until "feature" is the focused one.
    let mut found = false;
    for _ in 0..3 {
        app.apply(Command::LabelNext).unwrap();
        if reversed(&app) {
            found = true;
            break;
        }
    }
    assert!(found, "focused label drawn reversed");
    app.apply(Command::ClearLabelFocus).unwrap();
    assert!(!reversed(&app));
}

#[test]
fn reload_keeps_selection_and_scroll_below_first_page() {
    let repo = TestRepo::new();
    fast_import_linear(&repo, 650);
    let mut app = app_for(&repo);
    app.apply(Command::Bottom).unwrap(); // loads page 2
    for _ in 0..50 {
        app.apply(Command::Up).unwrap();
    }
    let before = app.selected_hash().map(str::to_string);
    let shown = screen(&app, 60, 10);
    assert!(shown.contains("commit 101"), "{shown}");

    repo.git(&["branch", "other"]);
    app.on_watch(terminal_git_graph::watch::WatchEvent::RefsChanged);
    assert_eq!(app.selected_hash().map(str::to_string), before);
    assert_eq!(screen(&app, 60, 10), shown);
}

#[test]
fn detached_head_row_shows_head_badge() {
    let repo = TestRepo::new();
    let first = repo.commit("first");
    repo.commit("second");
    repo.git(&["checkout", "--detach", &first]);
    let app = app_for(&repo);
    let rows = render(&app, 60, 6);
    let row = rows
        .iter()
        .find(|l| l.contains("first"))
        .expect("row of first");
    // The badge ends in a parenthesis, then one space before the subject.
    assert!(row.contains("HEAD) first"), "{rows:?}");
}

const HEART: &str = "❤️";
const THUMB: &str = "👍🏽";
const FAMILY: &str = "👨‍👩‍👧";

#[test]
fn emoji_graphemes_measure_like_ratatui() {
    use terminal_git_graph::ui::layout::display_width;
    for s in [HEART, THUMB, FAMILY, "a❤️b👍🏽c👨‍👩‍👧d", "한글🚀"] {
        assert_eq!(
            display_width(s),
            ratatui::text::Line::raw(s).width(),
            "{s:?}"
        );
    }
    assert_eq!(display_width(FAMILY), 2);
}

#[test]
fn truncation_never_cuts_a_grapheme() {
    use terminal_git_graph::ui::layout::display_width;
    use unicode_segmentation::UnicodeSegmentation;
    let s = format!("a{HEART}b{THUMB}c{FAMILY}d");
    let whole: Vec<&str> = s.graphemes(true).collect();
    for width in 0..12 {
        let cut = truncate_to_width(&s, width);
        assert!(display_width(&cut) <= width, "{width}: {cut:?}");
        let kept: Vec<&str> = cut.trim_end_matches('…').graphemes(true).collect();
        assert_eq!(kept, whole[..kept.len()], "{width}: {cut:?}");
    }
    assert_eq!(truncate_to_width(&s, 5), format!("a{HEART}b…"));
}

#[test]
fn emoji_subjects_keep_columns_aligned() {
    let repo = TestRepo::new();
    repo.commit("plain subject");
    repo.commit(&format!("love {HEART} it"));
    repo.commit(&format!("ok {THUMB} done"));
    repo.commit(&format!("fam {FAMILY} {FAMILY} {HEART}{THUMB}{FAMILY} end"));
    repo.commit(&FAMILY.repeat(60));
    let app = app_for(&repo);
    let buf = {
        let mut term = Terminal::new(TestBackend::new(120, 10)).unwrap();
        term.draw(|f| app.draw(f)).unwrap();
        term.backend().buffer().clone()
    };
    // x of the author column ("Test User") on every commit row.
    let author_x: Vec<u16> = (0..buf.area.height)
        .filter_map(|y| {
            (0..buf.area.width.saturating_sub(4))
                .find(|&x| (0..4).map(|i| buf[(x + i, y)].symbol()).collect::<String>() == "Test")
        })
        .collect();
    assert_eq!(author_x.len(), 5, "{:?}", lines(&buf));
    assert!(
        author_x.iter().all(|&x| x == author_x[0]),
        "{author_x:?}\n{:#?}",
        lines(&buf)
    );
}

#[test]
fn hidden_dialog_ignores_input_while_too_small() {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use terminal_git_graph::terminal::key_command;
    let repo = TestRepo::new();
    repo.commit("hello");
    let mut app = app_for(&repo);
    app.open_dialog(terminal_git_graph::ui::dialog::Dialog::Info {
        title: "t".to_string(),
        body: "b".to_string(),
    });
    let enter = KeyEvent::new(KeyCode::Enter, KeyModifiers::NONE);
    screen(&app, 19, 3);
    assert!(app.too_small());
    assert_eq!(key_command(enter, &app), None);
    assert_eq!(
        key_command(KeyEvent::new(KeyCode::Char('q'), KeyModifiers::NONE), &app),
        Some(Command::Quit)
    );
    // Back at a usable size the dialog gets its keys again.
    screen(&app, 60, 10);
    assert_eq!(key_command(enter, &app), Some(Command::DialogAccept));
}
