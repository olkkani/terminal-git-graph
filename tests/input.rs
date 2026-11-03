mod support;

use std::time::{Duration, Instant};

use crossterm::event::{KeyCode, KeyEvent, KeyModifiers, MouseButton, MouseEvent, MouseEventKind};
use ratatui::backend::TestBackend;
use ratatui::layout::Rect;
use ratatui::Terminal;
use support::TestRepo;
use terminal_git_graph::app::{App, Command};
use terminal_git_graph::git::refs::load_refs;
use terminal_git_graph::git::Git;
use terminal_git_graph::input::{
    hit_test, mouse_command, translate_key, ClickKind, ClickTracker, Hit, InputMode, ToolbarButton,
    DOUBLE_CLICK_MS,
};
use terminal_git_graph::ui::dialog::{Dialog, DialogOutcome, DialogState};
use terminal_git_graph::ui::menu::{Menu, MenuId, MenuItem};
use terminal_git_graph::ui::theme::ColorDepth;
use unicode_width::UnicodeWidthStr;

const W: u16 = 80;
const H: u16 = 12;

fn app_for(repo: &TestRepo) -> App {
    App::new(Git::at(repo.path()), ColorDepth::TrueColor).expect("open app")
}

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}

fn screen_area() -> Rect {
    Rect::new(0, 0, W, H)
}

/// Draws the app (as the event loop does before reading input) and returns the screen rows.
fn render(app: &App, width: u16, height: u16) -> Vec<String> {
    let mut term = Terminal::new(TestBackend::new(width, height)).unwrap();
    term.draw(|f| app.draw(f)).unwrap();
    let buf = term.backend().buffer();
    (0..height)
        .map(|y| {
            let mut line = String::new();
            let mut x = 0;
            while x < width {
                let sym = buf[(x, y)].symbol();
                line.push_str(sym);
                x += sym.width().max(1) as u16;
            }
            line
        })
        .collect()
}

fn mouse(kind: MouseEventKind, column: u16, row: u16) -> MouseEvent {
    MouseEvent {
        kind,
        column,
        row,
        modifiers: KeyModifiers::NONE,
    }
}

fn item(label: &str, enabled: bool) -> MenuItem {
    MenuItem {
        label: label.to_string(),
        enabled,
        danger: false,
        id: MenuId::Copy(label.to_string()),
    }
}

/// Three linear commits; rows are newest first, so row 2 is "one".
fn three_commits() -> (TestRepo, Vec<String>) {
    let repo = TestRepo::new();
    let hashes = vec![repo.commit("one"), repo.commit("two"), repo.commit("three")];
    (repo, hashes)
}

#[test]
fn key_table_maps_to_commands() {
    use Command::*;
    use InputMode::*;
    let shift = |c: char| KeyEvent::new(KeyCode::Char(c), KeyModifiers::SHIFT);
    let cases: Vec<(InputMode, KeyEvent, Option<Command>)> = vec![
        (Graph, key(KeyCode::Char('j')), Some(Down)),
        (Graph, key(KeyCode::Down), Some(Down)),
        (Graph, key(KeyCode::Char('k')), Some(Up)),
        (Graph, key(KeyCode::Up), Some(Up)),
        (Graph, key(KeyCode::Char('g')), Some(Top)),
        (Graph, shift('G'), Some(Bottom)),
        (Graph, key(KeyCode::PageUp), Some(PageUp)),
        (Graph, key(KeyCode::PageDown), Some(PageDown)),
        (Graph, key(KeyCode::Enter), Some(Activate)),
        (Graph, key(KeyCode::Esc), Some(Back)),
        (Graph, key(KeyCode::Char('m')), Some(OpenMenu)),
        (Graph, key(KeyCode::Char('a')), Some(ToggleScope)),
        (Graph, key(KeyCode::Char('r')), Some(ToggleRemotes)),
        (Graph, key(KeyCode::Char('f')), Some(Fetch)),
        (Graph, shift('?'), Some(Help)),
        (Graph, key(KeyCode::Char('q')), Some(Quit)),
        (Graph, key(KeyCode::Char('x')), None),
        (Graph, key(KeyCode::Left), Some(LabelPrev)),
        (Graph, key(KeyCode::Char('h')), Some(LabelPrev)),
        (Graph, key(KeyCode::Right), Some(LabelNext)),
        (Graph, key(KeyCode::Char('l')), Some(LabelNext)),
        (Detail, key(KeyCode::Right), None),
        (Detail, key(KeyCode::Char('j')), Some(FileDown)),
        (Detail, key(KeyCode::Char('k')), Some(FileUp)),
        (Detail, key(KeyCode::Enter), Some(OpenDiff)),
        (Detail, key(KeyCode::Esc), Some(CloseDetail)),
        (Detail, key(KeyCode::Char('m')), Some(OpenMenu)),
        (Diff, key(KeyCode::Char('j')), Some(DiffScrollDown)),
        (Diff, key(KeyCode::PageDown), Some(DiffPageDown)),
        (Diff, key(KeyCode::Esc), Some(CloseDiff)),
        (Diff, key(KeyCode::Enter), None),
        (Menu, key(KeyCode::Char('j')), Some(MenuDown)),
        (Menu, key(KeyCode::Up), Some(MenuUp)),
        (Menu, key(KeyCode::Enter), Some(MenuSelect)),
        (Menu, key(KeyCode::Esc), Some(MenuClose)),
        (Menu, key(KeyCode::Char('m')), Some(MenuClose)),
        (Menu, key(KeyCode::Char('a')), None),
        (Dialog, key(KeyCode::Enter), Some(DialogAccept)),
        (Dialog, key(KeyCode::Esc), Some(DialogCancel)),
        (Dialog, key(KeyCode::Backspace), Some(DialogBackspace)),
        // Typed into an input dialog instead of quitting.
        (Dialog, key(KeyCode::Char('q')), Some(DialogChar('q'))),
        (Dialog, key(KeyCode::Down), None),
    ];
    for (mode, k, want) in cases {
        assert_eq!(translate_key(k, mode), want, "{mode:?} {k:?}");
    }
    let ctrl_c = KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL);
    for mode in [Graph, Detail, Diff, Menu, Dialog] {
        assert_eq!(translate_key(ctrl_c, mode), Some(Quit), "{mode:?}");
    }
}

#[test]
fn single_then_double_click_within_window() {
    let mut clicks = ClickTracker::new();
    let t0 = Instant::now();
    let within = t0 + Duration::from_millis(DOUBLE_CLICK_MS - 50);
    assert_eq!(clicks.click((5, 5), t0), ClickKind::Single);
    assert_eq!(clicks.click((5, 5), within), ClickKind::Double);
    // A third quick click starts a new pair.
    assert_eq!(clicks.click((5, 5), within), ClickKind::Single);
    // Too slow for a double click.
    let late = within + Duration::from_millis(DOUBLE_CLICK_MS + 1);
    assert_eq!(clicks.click((5, 5), late), ClickKind::Single);
}

#[test]
fn clicks_at_different_positions_are_not_double() {
    let mut clicks = ClickTracker::new();
    let t0 = Instant::now();
    assert_eq!(clicks.click((5, 5), t0), ClickKind::Single);
    assert_eq!(
        clicks.click((6, 5), t0 + Duration::from_millis(10)),
        ClickKind::Single
    );
}

#[test]
fn click_on_row_selects_it() {
    let (repo, hashes) = three_commits();
    let mut app = app_for(&repo);
    render(&app, W, H);
    // Header on screen row 0 and the column header on row 1, so table row 2 is screen
    // row 4; x is inside the subject.
    let pos = (40, 4);
    assert_eq!(hit_test(&app, screen_area(), pos), Some(Hit::Row(2)));
    // Below the last row there is nothing to hit.
    assert_eq!(hit_test(&app, screen_area(), (40, 5)), None);

    let mut clicks = ClickTracker::new();
    let now = Instant::now();
    let ev = mouse(MouseEventKind::Down(MouseButton::Left), pos.0, pos.1);
    let cmd = mouse_command(&app, screen_area(), ev, &mut clicks, now);
    assert_eq!(cmd, Some(Command::SelectRow(2)));
    app.apply(cmd.unwrap()).unwrap();
    assert_eq!(app.selected_hash(), Some(hashes[0].as_str()));

    // A second click on the same spot is a double click and opens the detail.
    let cmd = mouse_command(&app, screen_area(), ev, &mut clicks, now);
    assert_eq!(cmd, Some(Command::Activate));
    app.apply(cmd.unwrap()).unwrap();
    assert!(app.detail_open());
}

#[test]
fn double_click_on_label_activates_that_label() {
    let repo = TestRepo::new();
    repo.commit("first");
    repo.branch("alpha");
    let app = app_for(&repo);
    let rows = render(&app, W, H);
    let byte = rows[2].find("alpha").expect("alpha label");
    let pos = (rows[2][..byte].width() as u16, 2);
    let hit = hit_test(&app, screen_area(), pos);
    let Some(Hit::Label { row, index }) = hit else {
        panic!("expected a label at {pos:?}, got {hit:?}");
    };
    let mut clicks = ClickTracker::new();
    let now = Instant::now();
    let ev = mouse(MouseEventKind::Down(MouseButton::Left), pos.0, pos.1);
    assert_eq!(
        mouse_command(&app, screen_area(), ev, &mut clicks, now),
        Some(Command::SelectRow(row))
    );
    assert_eq!(
        mouse_command(&app, screen_area(), ev, &mut clicks, now),
        Some(Command::ActivateLabel { row, index })
    );
}

#[test]
fn click_on_label_targets_that_label() {
    let repo = TestRepo::new();
    let hash = repo.commit("first");
    // Short names so all three badges fit next to the graph column.
    repo.branch("dev");
    repo.branch("fix");
    let app = app_for(&repo);
    let rows = render(&app, W, H);
    let refs = load_refs(&Git::at(repo.path())).unwrap();
    let names: Vec<&str> = refs
        .labels_for(&hash)
        .iter()
        .map(|l| l.name.as_str())
        .collect();
    assert_eq!(names.len(), 3, "{names:?}");

    let row = &rows[2];
    for (index, name) in names.iter().enumerate() {
        let byte = row
            .find(name)
            .unwrap_or_else(|| panic!("{name} in {row:?}"));
        let x = row[..byte].width() as u16;
        // First and last cell of the badge.
        for x in [x, x + name.len() as u16 - 1] {
            assert_eq!(
                hit_test(&app, screen_area(), (x, 2)),
                Some(Hit::Label { row: 0, index }),
                "{name} at x={x} in {row:?}"
            );
        }
    }
    // The second label specifically (several labels on one row).
    let second = row.find(names[1]).unwrap();
    let x = row[..second].width() as u16 + 1;
    assert_eq!(
        hit_test(&app, screen_area(), (x, 2)),
        Some(Hit::Label { row: 0, index: 1 })
    );
    // The subject after the badges is the row itself.
    let subject = row.find("first").unwrap();
    assert_eq!(
        hit_test(&app, screen_area(), (row[..subject].width() as u16, 2)),
        Some(Hit::Row(0))
    );
}

#[test]
fn right_click_opens_menu_at_position() {
    let (repo, hashes) = three_commits();
    let mut app = app_for(&repo);
    render(&app, W, H);
    let mut clicks = ClickTracker::new();
    let ev = mouse(MouseEventKind::Down(MouseButton::Right), 30, 3);
    let cmd = mouse_command(&app, screen_area(), ev, &mut clicks, Instant::now());
    assert_eq!(
        cmd,
        Some(Command::OpenMenuAt {
            pos: (30, 3),
            hit: Hit::Row(1)
        })
    );
    app.apply(cmd.unwrap()).unwrap();
    assert_eq!(app.input_mode(), InputMode::Menu);
    let menu = app.menu().expect("menu open");
    assert_eq!(menu.anchor, Some((30, 3)));
    assert_eq!(menu.target, Hit::Row(1));
    // The clicked row becomes the selection.
    assert_eq!(app.selected_hash(), Some(hashes[1].as_str()));
    // The menu box is drawn at the click's column; the commit menu is taller than this
    // screen, so it is moved up to stay inside.
    let rect = app.menu().unwrap().rect(screen_area());
    let rows = render(&app, W, H);
    assert_eq!(rect.x, 30);
    assert!(
        rows[rect.y as usize].chars().nth(30) == Some('┌'),
        "{rows:#?}"
    );

    // While the menu is open, graph keys don't reach the table.
    assert_eq!(
        translate_key(key(KeyCode::Char('j')), app.input_mode()),
        Some(Command::MenuDown)
    );
    app.apply(Command::MenuClose).unwrap();
    assert!(app.menu().is_none());
    assert_eq!(app.input_mode(), InputMode::Graph);

    // Near the bottom-right corner the menu is moved back on screen.
    let menu = Menu::new(
        vec![
            item("Checkout", true),
            item("Delete", true),
            item("Copy name", true),
        ],
        Some((W - 2, H - 1)),
        Hit::Row(0),
    );
    let rect = menu.rect(screen_area());
    assert!(rect.right() <= W && rect.bottom() <= H, "{rect:?}");
    assert_eq!(rect.height, 5);
}

#[test]
fn menu_arrows_skip_disabled_items_and_enter_confirms() {
    let mut menu = Menu::new(
        vec![
            item("A", false),
            item("B", true),
            item("C", false),
            item("D", true),
            item("E", false),
        ],
        None,
        Hit::Row(0),
    );
    // The first enabled item starts selected.
    assert_eq!(menu.selected, Some(1));
    assert_eq!(
        translate_key(key(KeyCode::Down), InputMode::Menu),
        Some(Command::MenuDown)
    );
    menu.down();
    assert_eq!(menu.selected, Some(3));
    // Nothing enabled below: stays.
    menu.down();
    assert_eq!(menu.selected, Some(3));
    menu.up();
    assert_eq!(menu.selected, Some(1));
    menu.up();
    assert_eq!(menu.selected, Some(1));
    menu.down();
    assert_eq!(
        translate_key(key(KeyCode::Enter), InputMode::Menu),
        Some(Command::MenuSelect)
    );
    assert_eq!(menu.chosen().map(|i| i.label.as_str()), Some("D"));

    let none = Menu::new(vec![item("A", false)], None, Hit::Row(0));
    assert_eq!(none.selected, None);
    assert!(none.chosen().is_none());
}

#[test]
fn confirm_dialog_enter_accepts_esc_cancels() {
    let confirm = || Dialog::Confirm {
        title: "Delete branch".to_string(),
        body: "Delete feature?".to_string(),
        danger: true,
    };
    let enter = translate_key(key(KeyCode::Enter), InputMode::Dialog).unwrap();
    let esc = translate_key(key(KeyCode::Esc), InputMode::Dialog).unwrap();

    let mut state = DialogState::new(confirm());
    assert_eq!(state.handle(enter), Some(DialogOutcome::Accepted(None)));
    let mut state = DialogState::new(confirm());
    assert_eq!(state.handle(esc), Some(DialogOutcome::Cancelled));

    // In the app the dialog is modal: other input is ignored until it closes.
    let (repo, _) = three_commits();
    let mut app = app_for(&repo);
    render(&app, W, H);
    app.open_dialog(confirm());
    assert_eq!(app.input_mode(), InputMode::Dialog);
    app.apply(Command::Down).unwrap();
    app.apply(Command::OpenMenu).unwrap();
    assert_eq!(app.selected_hash(), app_for(&repo).selected_hash());
    assert!(app.menu().is_none());
    let mut clicks = ClickTracker::new();
    let click = mouse(MouseEventKind::Down(MouseButton::Left), 40, 3);
    assert_eq!(
        mouse_command(&app, screen_area(), click, &mut clicks, Instant::now()),
        None
    );
    let scroll = mouse(MouseEventKind::ScrollDown, 40, 3);
    assert_eq!(
        mouse_command(&app, screen_area(), scroll, &mut clicks, Instant::now()),
        None
    );
    assert!(render(&app, W, H).join("\n").contains("Delete feature?"));
    app.apply(esc).unwrap();
    assert!(app.dialog().is_none());
    assert_eq!(app.input_mode(), InputMode::Graph);
}

#[test]
fn input_dialog_rejects_value_starting_with_dash() {
    let mut state = DialogState::new(Dialog::Input {
        title: "Create branch".to_string(),
        prompt: "Branch name".to_string(),
        value: String::new(),
        name: true,
    });
    for c in "-x".chars() {
        assert_eq!(state.handle(Command::DialogChar(c)), None);
    }
    assert_eq!(state.handle(Command::DialogAccept), None);
    assert!(state.error.is_some(), "rejected with a message");
    assert!(matches!(&state.dialog, Dialog::Input { value, .. } if value == "-x"));

    state.handle(Command::DialogBackspace);
    state.handle(Command::DialogBackspace);
    state.handle(Command::DialogChar('x'));
    assert_eq!(state.error, None, "editing clears the message");
    assert_eq!(
        state.handle(Command::DialogAccept),
        Some(DialogOutcome::Accepted(Some("x".to_string())))
    );
}

#[test]
fn free_text_input_accepts_leading_dash() {
    let mut state = DialogState::new(Dialog::Input {
        title: "Commit".to_string(),
        prompt: "Commit message:".to_string(),
        value: String::new(),
        name: false,
    });
    for c in "-wip".chars() {
        state.handle(Command::DialogChar(c));
    }
    assert_eq!(
        state.handle(Command::DialogAccept),
        Some(DialogOutcome::Accepted(Some("-wip".to_string())))
    );
}

#[test]
fn error_dialog_shows_message_verbatim() {
    let (repo, _) = three_commits();
    let mut app = app_for(&repo);
    let message = "CONFLICT (content): Merge conflict in a.txt\n\
                   Automatic merge failed; fix conflicts and then commit the result.";
    app.open_dialog(Dialog::Error {
        title: "Merge failed".to_string(),
        message: message.to_string(),
    });
    let out = render(&app, 100, 20).join("\n");
    assert!(out.contains("Merge failed"), "{out}");
    for line in message.lines() {
        assert!(out.contains(line), "{line:?} in\n{out}");
    }
    // Enter dismisses it.
    let enter = translate_key(key(KeyCode::Enter), InputMode::Dialog).unwrap();
    app.apply(enter).unwrap();
    assert!(app.dialog().is_none());
}

#[test]
fn scroll_wheel_moves_selection() {
    let (repo, hashes) = three_commits();
    let mut app = app_for(&repo);
    render(&app, W, H);
    let mut clicks = ClickTracker::new();
    let down = mouse(MouseEventKind::ScrollDown, 40, 3);
    let cmd = mouse_command(&app, screen_area(), down, &mut clicks, Instant::now());
    assert_eq!(cmd, Some(Command::Down));
    app.apply(cmd.unwrap()).unwrap();
    assert_eq!(app.selected_hash(), Some(hashes[1].as_str()));
    let up = mouse(MouseEventKind::ScrollUp, 40, 3);
    let cmd = mouse_command(&app, screen_area(), up, &mut clicks, Instant::now());
    app.apply(cmd.unwrap()).unwrap();
    assert_eq!(app.selected_hash(), Some(hashes[2].as_str()));
}

#[test]
fn clicks_on_toolbar_and_file_list() {
    let repo = TestRepo::new();
    repo.commit_file("a.txt", "one\n", "base");
    repo.commit_file("b.txt", "two\n", "add b");
    let mut app = app_for(&repo);
    let header = render(&app, W, H)[0].clone();
    let col = |text: &str| header[..header.find(text).unwrap()].width() as u16 + 1;
    let area = screen_area();
    let hit = |app: &App, x| hit_test(app, area, (x, 0));
    assert_eq!(
        hit(&app, col("Fetch")),
        Some(Hit::Toolbar(ToolbarButton::Fetch))
    );
    assert_eq!(
        hit(&app, col("All")),
        Some(Hit::Toolbar(ToolbarButton::Scope))
    );
    assert_eq!(
        hit(&app, col("Current")),
        Some(Hit::Toolbar(ToolbarButton::Scope))
    );
    assert_eq!(
        hit(&app, col("Remotes")),
        Some(Hit::Toolbar(ToolbarButton::Remotes))
    );
    assert_eq!(hit(&app, 0), None);

    // Full-screen detail (short terminal): "add b" lists one file.
    app.apply(Command::OpenDetail).unwrap();
    app.load_pending().unwrap();
    let rows = render(&app, W, H);
    let y = rows.iter().position(|r| r.contains("b.txt")).unwrap() as u16;
    assert_eq!(hit_test(&app, area, (10, y)), Some(Hit::File(0)));
    assert_eq!(hit_test(&app, area, (10, y + 1)), None);
    let mut clicks = ClickTracker::new();
    let ev = mouse(MouseEventKind::Down(MouseButton::Left), 10, y);
    let cmd = mouse_command(&app, area, ev, &mut clicks, Instant::now());
    assert_eq!(cmd, Some(Command::SelectFile(0)));
    app.apply(cmd.unwrap()).unwrap();
    assert_eq!(app.input_mode(), InputMode::Detail);
}
