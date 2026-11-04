mod support;

use std::io;

use support::TestRepo;
use terminal_git_graph::app::{App, Command};
use terminal_git_graph::clipboard::{osc52_sequence, CopyVia};
use terminal_git_graph::git::Git;
use terminal_git_graph::input::Hit;
use terminal_git_graph::ui::dialog::Dialog;
use terminal_git_graph::ui::theme::ColorDepth;

#[test]
fn osc52_sequence_for_hi() {
    assert_eq!(osc52_sequence("hi"), "\x1b]52;c;aGk=\x07");
}

fn failing_copy(_: &str) -> io::Result<CopyVia> {
    Err(io::Error::other("no clipboard"))
}

fn working_copy(_: &str) -> io::Result<CopyVia> {
    Ok(CopyVia::Osc52)
}

fn pick_copy_hash(app: &mut App) {
    app.apply(Command::OpenMenuAt {
        pos: (0, 0),
        hit: Hit::Row(0),
    })
    .unwrap();
    let i = app
        .menu()
        .expect("menu")
        .items
        .iter()
        .position(|i| i.label == "Copy hash")
        .unwrap();
    app.apply(Command::MenuPick(i)).unwrap();
}

#[test]
fn copy_falls_back_to_dialog_value_when_all_fail() {
    let repo = TestRepo::new();
    let hash = repo.commit("first");
    let mut app = App::new(Git::at(repo.path()), ColorDepth::TrueColor).unwrap();
    app.set_copier(failing_copy);
    pick_copy_hash(&mut app);
    match &app.dialog().expect("dialog").dialog {
        Dialog::Error { message, .. } => assert!(message.contains(&hash), "{message}"),
        d => panic!("expected error dialog, got {d:?}"),
    }
}

#[test]
fn successful_copy_shows_no_dialog() {
    let repo = TestRepo::new();
    repo.commit("first");
    let mut app = App::new(Git::at(repo.path()), ColorDepth::TrueColor).unwrap();
    app.set_copier(working_copy);
    pick_copy_hash(&mut app);
    assert!(app.dialog().is_none());
    assert!(app.menu().is_none());
}
