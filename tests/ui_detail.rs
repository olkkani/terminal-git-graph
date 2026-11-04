mod support;

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::backend::TestBackend;
use ratatui::buffer::Buffer;
use ratatui::style::Color;
use ratatui::Terminal;
use support::TestRepo;
use terminal_git_graph::app::{App, Command, Focus};
use terminal_git_graph::git::Git;
use terminal_git_graph::terminal::key_command;
use terminal_git_graph::ui::layout::DETAIL_SPLIT_MIN_HEIGHT;
use terminal_git_graph::ui::theme::ColorDepth;
use unicode_width::UnicodeWidthStr;

fn app_for(repo: &TestRepo) -> App {
    App::new(Git::at(repo.path()), ColorDepth::TrueColor).expect("open app")
}

/// Opens the detail of the selected row and finishes loading it.
fn open_detail(app: &mut App) {
    app.apply(Command::OpenDetail).unwrap();
    assert!(app.load_pending().unwrap());
}

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

fn draw(app: &App, width: u16, height: u16) -> Buffer {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    term.backend().buffer().clone()
}

fn screen(app: &App, width: u16, height: u16) -> String {
    lines(&draw(app, width, height)).join("\n")
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

#[test]
fn selecting_commit_shows_message_hash_parents_author_date() {
    let repo = TestRepo::new();
    let parent = repo.commit("first");
    let head = repo.commit_file("a.txt", "x\n", "second subject\n\nbody text");
    let mut app = app_for(&repo);

    // Before the detail loads, the header is drawn and the file list waits.
    app.apply(Command::OpenDetail).unwrap();
    let out = screen(&app, 80, 40);
    assert!(out.contains("second subject"), "{out}");
    assert!(out.contains(&head), "{out}");
    assert!(out.contains("Loading…"), "{out}");

    assert!(app.load_pending().unwrap());
    let out = screen(&app, 80, 40);
    assert!(!out.contains("Loading…"), "{out}");
    assert!(out.contains("body text"), "{out}");
    assert!(out.contains(&head), "{out}");
    assert!(out.contains(&parent[..7]), "{out}");
    assert!(out.contains("Test User <test@example.com>"), "{out}");
    assert!(out.contains("2001-09-09"), "{out}");

    // The detail follows the table selection.
    app.apply(Command::Down).unwrap();
    assert!(app.load_pending().unwrap());
    let out = screen(&app, 80, 40);
    assert!(out.contains(&parent), "{out}");
    assert!(out.contains("file-1.txt"), "{out}");
}

#[test]
fn file_list_shows_status_and_plus_minus() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "base");
    repo.commit_file("a.txt", "one\ntwo\nthree\n", "grow a");
    let mut app = app_for(&repo);
    open_detail(&mut app);
    let out = screen(&app, 80, 40);
    assert!(out.contains("Files (1)"), "{out}");
    let row = out
        .lines()
        .find(|l| l.contains("a.txt"))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(row.contains("M +2 -0 a.txt"), "{row}");
}

#[test]
fn short_terminal_switches_to_fullscreen_detail() {
    assert_eq!(DETAIL_SPLIT_MIN_HEIGHT, 30);
    let repo = TestRepo::new();
    repo.commit("older one");
    let head = repo.commit("newest");
    let mut app = app_for(&repo);
    open_detail(&mut app);

    let split = screen(&app, 80, DETAIL_SPLIT_MIN_HEIGHT);
    assert!(split.contains("older one"), "{split}");
    assert!(split.contains(&head), "{split}");

    let full = screen(&app, 80, DETAIL_SPLIT_MIN_HEIGHT - 1);
    assert!(!full.contains("older one"), "{full}");
    assert!(full.contains(&head), "{full}");
}

#[test]
fn open_diff_shows_hunk_and_scrolls() {
    let repo = TestRepo::new();
    repo.commit("base");
    let body: String = (1..=50).map(|i| format!("row {i:02}\n")).collect();
    repo.commit_file("big.txt", &body, "add big");
    let mut app = app_for(&repo);
    open_detail(&mut app);
    app.apply(Command::FocusFiles).unwrap();
    app.apply(Command::OpenDiff).unwrap();
    assert_eq!(app.focus(), Focus::Diff);

    let buf = draw(&app, 60, 20);
    let rows = lines(&buf);
    assert!(rows.iter().any(|l| l.contains("@@")), "{rows:?}");
    let y = rows
        .iter()
        .position(|l| l.contains("+row 01"))
        .unwrap_or_else(|| panic!("{rows:?}"));
    let x = rows[y].find("+row 01").unwrap() as u16;
    assert_eq!(buf[(x, y as u16)].fg, Color::Green);
    let hunk_y = rows.iter().position(|l| l.contains("@@")).unwrap();
    let hunk_x = rows[hunk_y].find("@@").unwrap() as u16;
    assert_eq!(buf[(hunk_x, hunk_y as u16)].fg, Color::Cyan);

    for _ in 0..10 {
        app.apply(Command::DiffScrollDown).unwrap();
    }
    let out = screen(&app, 60, 20);
    assert!(!out.contains("@@"), "{out}");
    assert!(!out.contains("+row 01"), "{out}");
    assert!(out.contains("+row 10"), "{out}");

    for _ in 0..10 {
        app.apply(Command::DiffScrollUp).unwrap();
    }
    assert!(screen(&app, 60, 20).contains("@@"));
}

#[test]
fn esc_returns_from_diff_to_detail() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "base");
    repo.commit_file("a.txt", "two\n", "change a");
    let mut app = app_for(&repo);
    assert_eq!(
        key_command(key(KeyCode::Enter), &app),
        Some(Command::OpenDetail)
    );
    open_detail(&mut app);
    assert_eq!(
        key_command(key(KeyCode::Tab), &app),
        Some(Command::FocusFiles)
    );
    app.apply(Command::FocusFiles).unwrap();
    assert_eq!(app.focus(), Focus::Files);
    assert_eq!(
        key_command(key(KeyCode::Enter), &app),
        Some(Command::OpenDiff)
    );
    app.apply(Command::OpenDiff).unwrap();
    assert!(screen(&app, 60, 20).contains("-one"));

    let esc = key_command(key(KeyCode::Esc), &app);
    assert_eq!(esc, Some(Command::CloseDiff));
    app.apply(esc.unwrap()).unwrap();
    assert_eq!(app.focus(), Focus::Files);
    let out = screen(&app, 60, 20);
    assert!(out.contains("Files (1)") && !out.contains("-one"), "{out}");

    let esc = key_command(key(KeyCode::Esc), &app);
    assert_eq!(esc, Some(Command::CloseDetail));
    app.apply(esc.unwrap()).unwrap();
    assert_eq!(app.focus(), Focus::Table);
    assert!(!screen(&app, 60, 20).contains("Files ("));
    assert_eq!(key_command(key(KeyCode::Esc), &app), Some(Command::Quit));
}

#[test]
fn uncommitted_row_detail_lists_changed_files() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "base");
    std::fs::write(repo.path().join("a.txt"), "one\ntwo\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "fresh\n").unwrap();
    let mut app = app_for(&repo);
    assert_eq!(app.selected_hash(), None);
    open_detail(&mut app);
    let out = screen(&app, 80, 40);
    assert!(out.contains("Files (2)"), "{out}");
    assert!(out.contains("M +1 -0 a.txt"), "{out}");
    assert!(out.contains("? new.txt"), "{out}");
}

#[test]
fn untracked_file_diff_shows_content() {
    let repo = TestRepo::new();
    repo.commit("base");
    std::fs::write(repo.path().join("new.txt"), "hello untracked\n").unwrap();
    let mut app = app_for(&repo);
    open_detail(&mut app);
    app.apply(Command::FocusFiles).unwrap();
    app.apply(Command::OpenDiff).unwrap();
    let out = screen(&app, 60, 20);
    assert!(out.contains("new.txt"), "{out}");
    assert!(out.contains("hello untracked"), "{out}");
}

#[test]
fn merge_commit_detail_shows_both_parents() {
    let repo = TestRepo::new();
    repo.commit_file("base.txt", "b\n", "base");
    repo.git(&["checkout", "-b", "side"]);
    let side = repo.commit_file("side.txt", "s\n", "side");
    repo.checkout("main");
    let main = repo.commit_file("main.txt", "m\n", "main");
    repo.merge("side", true);
    let mut app = app_for(&repo);
    open_detail(&mut app);
    let out = screen(&app, 80, 40);
    let parents = out
        .lines()
        .find(|l| l.contains("Parents"))
        .unwrap_or_else(|| panic!("{out}"));
    assert!(parents.contains(&main[..7]), "{parents}");
    assert!(parents.contains(&side[..7]), "{parents}");
    assert!(out.contains("side.txt"), "{out}");
}

#[test]
fn paging_keys_in_diff_scroll_the_diff_and_keep_the_commit() {
    let repo = TestRepo::new();
    repo.commit("base");
    repo.commit("older");
    let body: String = (1..=50).map(|i| format!("row {i:02}\n")).collect();
    let head = repo.commit_file("big.txt", &body, "add big");
    let mut app = app_for(&repo);
    open_detail(&mut app);
    app.apply(Command::FocusFiles).unwrap();
    app.apply(Command::OpenDiff).unwrap();
    assert!(screen(&app, 60, 20).contains("@@"));

    let cmd = key_command(key(KeyCode::PageDown), &app).unwrap();
    app.apply(cmd).unwrap();
    assert_eq!(app.focus(), Focus::Diff);
    assert_eq!(app.selected_hash(), Some(head.as_str()));
    let out = screen(&app, 60, 20);
    assert!(!out.contains("@@") && out.contains("+row"), "{out}");

    let cmd = key_command(key(KeyCode::End), &app).unwrap();
    app.apply(cmd).unwrap();
    assert_eq!(app.focus(), Focus::Diff);
    assert_eq!(app.selected_hash(), Some(head.as_str()));
    let out = screen(&app, 60, 20);
    assert!(out.contains("+row 50"), "{out}");

    let cmd = key_command(key(KeyCode::Home), &app).unwrap();
    app.apply(cmd).unwrap();
    assert!(screen(&app, 60, 20).contains("@@"));

    app.apply(key_command(key(KeyCode::PageDown), &app).unwrap())
        .unwrap();
    app.apply(key_command(key(KeyCode::PageUp), &app).unwrap())
        .unwrap();
    assert_eq!(app.selected_hash(), Some(head.as_str()));
    assert!(screen(&app, 60, 20).contains("@@"));
}

#[test]
fn paging_keys_in_files_jump_to_first_and_last_file() {
    let repo = TestRepo::new();
    repo.commit("base");
    repo.commit("older");
    for (name, text) in [
        ("a.txt", "alpha\n"),
        ("b.txt", "bravo\n"),
        ("c.txt", "charlie\n"),
    ] {
        std::fs::write(repo.path().join(name), text).unwrap();
    }
    repo.git(&["add", "--", "a.txt", "b.txt", "c.txt"]);
    repo.git(&["commit", "-m", "three files"]);
    let head = repo.git(&["rev-parse", "HEAD"]).trim().to_string();
    let mut app = app_for(&repo);
    open_detail(&mut app);
    app.apply(Command::FocusFiles).unwrap();

    app.apply(key_command(key(KeyCode::PageDown), &app).unwrap())
        .unwrap();
    assert_eq!(app.focus(), Focus::Files);
    assert_eq!(app.selected_hash(), Some(head.as_str()));
    app.apply(Command::OpenDiff).unwrap();
    assert!(screen(&app, 60, 20).contains("+charlie"));
    app.apply(Command::CloseDiff).unwrap();

    app.apply(key_command(key(KeyCode::Home), &app).unwrap())
        .unwrap();
    assert_eq!(app.selected_hash(), Some(head.as_str()));
    app.apply(Command::OpenDiff).unwrap();
    assert!(screen(&app, 60, 20).contains("+alpha"));
    app.apply(Command::CloseDiff).unwrap();

    app.apply(key_command(key(KeyCode::End), &app).unwrap())
        .unwrap();
    app.apply(key_command(key(KeyCode::PageUp), &app).unwrap())
        .unwrap();
    assert_eq!(app.selected_hash(), Some(head.as_str()));
    app.apply(Command::OpenDiff).unwrap();
    assert!(screen(&app, 60, 20).contains("+alpha"));
}

#[test]
fn unreadable_commit_detail_shows_error_and_is_not_retried() {
    let repo = TestRepo::new();
    repo.commit("first");
    let head = repo.commit("second");
    let mut app = app_for(&repo);
    // The commit object goes away after the log was read (e.g. gc after an amend).
    let object = repo
        .path()
        .join(".git/objects")
        .join(&head[..2])
        .join(&head[2..]);
    std::fs::remove_file(object).unwrap();

    app.apply(Command::OpenDetail).unwrap();
    assert!(app.load_pending().unwrap());
    match &app.dialog().expect("error dialog").dialog {
        terminal_git_graph::ui::dialog::Dialog::Error { title, .. } => {
            assert_eq!(title, "Cannot read details")
        }
        other => panic!("unexpected dialog {other:?}"),
    }
    assert!(
        !app.load_pending().unwrap(),
        "failed detail must not reload"
    );
    assert!(!app.should_quit());
    assert!(screen(&app, 80, 30).contains("Could not read the files."));
}

#[test]
fn dialog_wrap_keeps_emoji_graphemes_whole() {
    let family = "👨‍👩‍👧";
    let repo = TestRepo::new();
    repo.commit("first");
    let mut app = app_for(&repo);
    app.open_dialog(terminal_git_graph::ui::dialog::Dialog::Error {
        title: "Oops".to_string(),
        message: family.repeat(20),
    });
    let out = screen(&app, 40, 20);
    assert_eq!(out.matches(family).count(), 20, "{out}");
}

/// Cell where `text` starts on the screen row that contains `row_hint` (ASCII rows only).
fn cell_of<'a>(buf: &'a Buffer, row_hint: &str, text: &str) -> &'a ratatui::buffer::Cell {
    let rows = lines(buf);
    let y = rows
        .iter()
        .rposition(|l| l.contains(row_hint))
        .unwrap_or_else(|| panic!("no {row_hint:?} in {rows:?}"));
    let x = rows[y]
        .find(text)
        .unwrap_or_else(|| panic!("{text:?} in {:?}", rows[y]));
    &buf[(rows[y][..x].width() as u16, y as u16)]
}

fn detail_fixture() -> (TestRepo, App, String) {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "base");
    repo.commit_file("a.txt", "one\ntwo\nthree\n", "grow a\n\nbody");
    repo.commit_file("b.txt", "x\n", "add b");
    let mut app = app_for(&repo);
    open_detail(&mut app);
    let hash = repo.git(&["rev-parse", "HEAD"]);
    (repo, app, hash.trim().to_string())
}

#[test]
fn detail_labels_are_dimmed() {
    let (_r, app, _) = detail_fixture();
    let buf = draw(&app, 80, 40);
    let cell = cell_of(&buf, "Author", "Author");
    assert!(cell.modifier.contains(ratatui::style::Modifier::DIM));
    let value = cell_of(&buf, "Author", "Test User");
    assert!(!value.modifier.contains(ratatui::style::Modifier::DIM));
}

#[test]
fn commit_hash_uses_accent_color() {
    let (_r, app, hash) = detail_fixture();
    let buf = draw(&app, 80, 40);
    let cell = cell_of(&buf, "Commit", &hash);
    assert_eq!(cell.fg, Color::Cyan);
}

#[test]
fn subject_is_bold() {
    let (_r, app, _) = detail_fixture();
    let buf = draw(&app, 80, 40);
    let cell = cell_of(&buf, "add b", "add b");
    assert!(cell.modifier.contains(ratatui::style::Modifier::BOLD));
}

#[test]
fn file_status_letters_are_colored() {
    let repo = TestRepo::new();
    repo.commit_file("mod.txt", "m\n", "base m");
    repo.commit_file("gone.txt", "g\n", "base g");
    std::fs::write(repo.path().join("mod.txt"), "m2\n").unwrap();
    std::fs::write(repo.path().join("new.txt"), "n\n").unwrap();
    repo.git(&["rm", "-q", "gone.txt"]);
    repo.git(&["add", "-A"]);
    repo.git(&["commit", "-q", "-m", "all kinds"]);
    let mut app = app_for(&repo);
    open_detail(&mut app);
    let buf = draw(&app, 80, 40);
    assert_eq!(cell_of(&buf, "new.txt", "A ").fg, Color::Green);
    assert_eq!(cell_of(&buf, "mod.txt", "M ").fg, Color::Yellow);
    assert_eq!(cell_of(&buf, "gone.txt", "D ").fg, Color::Red);
}

#[test]
fn added_and_deleted_counts_are_green_and_red() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\ntwo\n", "base");
    repo.commit_file("a.txt", "one\nthree\nfour\n", "edit");
    let mut app = app_for(&repo);
    open_detail(&mut app);
    let buf = draw(&app, 80, 40);
    // File row and the summary row on the "Files" line.
    assert_eq!(cell_of(&buf, "a.txt", "+2").fg, Color::Green);
    assert_eq!(cell_of(&buf, "a.txt", "-1").fg, Color::Red);
    assert_eq!(cell_of(&buf, "Files (1)", "+2").fg, Color::Green);
    assert_eq!(cell_of(&buf, "Files (1)", "-1").fg, Color::Red);
}
