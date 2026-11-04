//! Performance checks on a 50,000-commit repository. They are slow to set up, so they
//! are ignored by default: run them with `cargo test --release -- --ignored`.

mod support;

use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

use ratatui::backend::TestBackend;
use ratatui::Terminal;
use terminal_git_graph::app::{App, Command};
use terminal_git_graph::gfx::present::{Presenter, RowPlacement};
use terminal_git_graph::gfx::raster::{rasterize_row, row_key, row_size, RasterStyle};
use terminal_git_graph::gfx::session::raster_style;
use terminal_git_graph::gfx::CellPx;
use terminal_git_graph::git::log::{load_commits, LogQuery, Scope};
use terminal_git_graph::git::Git;
use terminal_git_graph::graph::lanes::{GraphRow, LaneBuilder, LaneSeg};
use terminal_git_graph::graph::render::RenderOpts;
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

const CELL: CellPx = CellPx { w: 10, h: 20 };

/// 40 rows that all differ (lane, color, joins/forks, merge flag), like a busy screen.
fn distinct_rows() -> Vec<GraphRow> {
    (0..40)
        .map(|i| {
            let lane = i % 6;
            let seg = |l: usize| LaneSeg {
                lane: l,
                color: (l + i) % 8,
            };
            GraphRow {
                hash: format!("{i:040x}"),
                lane,
                color: i % 8,
                pass: (0..i % 4).filter(|&l| l != lane).map(seg).collect(),
                joins: if i % 3 == 0 {
                    vec![seg(lane + 1)]
                } else {
                    vec![]
                },
                forks: if i % 5 == 0 {
                    vec![seg(lane + 2)]
                } else {
                    vec![]
                },
                from_above: i % 7 != 0,
                continues: true,
                is_merge: i % 4 == 0,
                width: 8,
            }
        })
        .collect()
}

fn opts() -> RenderOpts {
    RenderOpts {
        is_head: false,
        ascii: false,
    }
}

fn plan_for(rows: &[GraphRow], style: &RasterStyle, first: usize, n: usize) -> Vec<RowPlacement> {
    rows[first..first + n]
        .iter()
        .enumerate()
        .map(|(y, r)| {
            let (w, h) = row_size(r, style);
            RowPlacement {
                col: 2,
                row: y as u16,
                cols: r.width as u16 * 2,
                key: row_key(r, &opts(), style),
                rgba_w: w,
                rgba_h: h,
            }
        })
        .collect()
}

fn present_plan(
    p: &mut Presenter,
    out: &mut Vec<u8>,
    rows: &[GraphRow],
    style: &RasterStyle,
    plan: &[RowPlacement],
) {
    let mut render = |key: u64| {
        let row = rows
            .iter()
            .find(|r| row_key(r, &opts(), style) == key)
            .expect("row for key");
        rasterize_row(row, &opts(), style)
    };
    p.present(out, CELL, plan, &mut render).unwrap();
}

#[test]
#[ignore = "slow: run with cargo test --release -- --ignored"]
fn raster_40_rows_under_5ms() {
    let rows = distinct_rows();
    let style = raster_style(CELL);

    let start = Instant::now();
    let bytes: usize = rows
        .iter()
        .map(|r| rasterize_row(r, &opts(), &style).len())
        .sum();
    let took = start.elapsed();

    assert!(bytes > 0);
    assert!(
        took < Duration::from_millis(5),
        "rasterizing 40 rows took {took:?}"
    );
}

#[test]
#[ignore = "slow: run with cargo test --release -- --ignored"]
fn present_scroll_step_under_2ms() {
    // 41 distinct rows: the second screen shifts down one row, so all images are cached.
    let mut rows = distinct_rows();
    rows.push(rows[0].clone());
    rows[40].hash = "x".repeat(40);
    rows[40].lane = 7;
    let style = raster_style(CELL);
    let mut p = Presenter::new(512);
    let mut out = Vec::new();
    // Warm the cache with both screens' images.
    present_plan(
        &mut p,
        &mut out,
        &rows,
        &style,
        &plan_for(&rows, &style, 0, 40),
    );
    present_plan(
        &mut p,
        &mut out,
        &rows,
        &style,
        &plan_for(&rows, &style, 1, 40),
    );
    present_plan(
        &mut p,
        &mut out,
        &rows,
        &style,
        &plan_for(&rows, &style, 0, 40),
    );

    out.clear();
    let plan = plan_for(&rows, &style, 1, 40);
    let start = Instant::now();
    present_plan(&mut p, &mut out, &rows, &style, &plan);
    let took = start.elapsed();

    assert!(!out.is_empty());
    assert!(took < Duration::from_millis(2), "scroll step took {took:?}");
}

#[test]
#[ignore = "slow: run with cargo test --release -- --ignored"]
fn graphics_first_frame_adds_under_100ms_vs_text() {
    let rows = distinct_rows();
    let style = raster_style(CELL);
    let plan = plan_for(&rows, &style, 0, 40);
    let mut p = Presenter::new(512);
    let mut out = Vec::new();

    // Rasterizing and transmitting 40 rows into an in-memory writer is all the
    // graphics path adds over the text frame.
    let start = Instant::now();
    present_plan(&mut p, &mut out, &rows, &style, &plan);
    let took = start.elapsed();

    assert!(out.len() > 40 * 100);
    assert!(
        took < Duration::from_millis(100),
        "first graphics frame took {took:?}"
    );
}
