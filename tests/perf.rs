//! Performance checks on a 50,000-commit repository. They are slow to set up, so they
//! are ignored by default: run them with `cargo test --release -- --ignored`.

mod support;

use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use terminal_git_graph::app::{App, Command};
use terminal_git_graph::git::log::{load_commits, LogQuery, Scope};
use terminal_git_graph::git::Git;
use terminal_git_graph::graph::lanes::LaneBuilder;
use terminal_git_graph::ui::theme::ColorDepth;
use terminal_git_graph::watch::watch;

const COMMITS: usize = 50_000;

fn header(term: &Terminal<TestBackend>) -> String {
    let buf = term.backend().buffer();
    (0..buf.area.width).map(|x| buf[(x, 0)].symbol()).collect()
}

/// Cold start as `tgg` does it, before the first frame: refs, status, the commit count
/// and first page (`App::new`), the repository watcher, then the first draw.
#[test]
#[ignore = "slow: run with cargo test --release -- --ignored"]
fn first_screen_under_one_second_on_50k_commits() {
    let repo = support::big_repo::generate(COMMITS);
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();

    let start = Instant::now();
    let app = App::new(Git::at(repo.path()), ColorDepth::TrueColor).expect("open app");
    let loaded = start.elapsed();
    let (tx, _rx) = channel();
    let _watching = watch(app.git(), tx).expect("watch repository");
    let watched = start.elapsed();
    term.draw(|f| app.draw(f)).unwrap();
    let total = start.elapsed();

    assert!(
        header(&term).contains(&format!("300/{COMMITS} commits")),
        "{}",
        header(&term)
    );
    assert!(
        total < Duration::from_secs(1),
        "first screen took {total:?} (load {loaded:?}, watch {:?}, draw {:?})",
        watched - loaded,
        total - watched
    );
}

#[test]
#[ignore = "slow: run with cargo test --release -- --ignored"]
fn next_page_under_300ms() {
    let repo = support::big_repo::generate(COMMITS);
    let mut app = App::new(Git::at(repo.path()), ColorDepth::TrueColor).expect("open app");
    let mut term = Terminal::new(TestBackend::new(120, 40)).unwrap();

    let start = Instant::now();
    // End loads the next page and selects its last row.
    app.apply(Command::Bottom).expect("load next page");
    let took = start.elapsed();

    term.draw(|f| app.draw(f)).unwrap();
    assert!(
        header(&term).contains(&format!("600/{COMMITS} commits")),
        "{}",
        header(&term)
    );
    assert!(took < Duration::from_millis(300), "next page took {took:?}");
}

#[test]
#[ignore = "slow: run with cargo test --release -- --ignored"]
fn lane_building_50k_rows_under_one_second() {
    let repo = support::big_repo::generate(COMMITS);
    let commits = load_commits(
        &Git::at(repo.path()),
        &LogQuery {
            scope: Scope::AllBranches,
            remotes: true,
            skip: 0,
            limit: COMMITS,
        },
    )
    .expect("load all commits");
    assert_eq!(commits.len(), COMMITS);

    let start = Instant::now();
    let mut lanes = LaneBuilder::new();
    let rows: Vec<_> = commits.iter().map(|c| lanes.push(c)).collect();
    let took = start.elapsed();

    assert_eq!(rows.len(), COMMITS);
    assert!(rows.iter().any(|r| r.is_merge));
    assert!(took < Duration::from_secs(1), "lane building took {took:?}");
}
