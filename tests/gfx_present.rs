use std::io::{self, Write};

use terminal_git_graph::gfx::present::{Presenter, RowPlacement};
use terminal_git_graph::gfx::CellPx;

const CELL: CellPx = CellPx { w: 8, h: 16 };

fn row(row: u16, key: u64) -> RowPlacement {
    RowPlacement {
        col: 2,
        row,
        cols: 3,
        key,
        rgba_w: 2,
        rgba_h: 1,
    }
}

fn render(key: u64) -> Vec<u8> {
    vec![key as u8; 8]
}

fn count(out: &[u8], needle: &str) -> usize {
    String::from_utf8_lossy(out).matches(needle).count()
}

fn present(p: &mut Presenter, cell: CellPx, plan: &[RowPlacement]) -> Vec<u8> {
    let mut out = Vec::new();
    p.present(&mut out, cell, plan, &mut render).unwrap();
    out
}

#[test]
fn first_present_transmits_then_places_each_visible_row() {
    let mut p = Presenter::new(512);
    let out = present(&mut p, CELL, &[row(0, 1), row(1, 2), row(2, 3)]);
    assert_eq!(count(&out, "a=t,"), 3);
    assert_eq!(count(&out, "a=p,"), 3);
    assert!(out.starts_with(b"\x1b7"));
    assert!(out.ends_with(b"\x1b8"));
}

#[test]
fn same_plan_twice_writes_nothing_second_time() {
    let mut p = Presenter::new(512);
    let plan = [row(0, 1), row(1, 2)];
    present(&mut p, CELL, &plan);
    assert!(present(&mut p, CELL, &plan).is_empty());
}

#[test]
fn scroll_by_one_row_transmits_only_the_new_row() {
    let mut p = Presenter::new(512);
    present(&mut p, CELL, &[row(0, 1), row(1, 2), row(2, 3)]);
    let out = present(&mut p, CELL, &[row(0, 2), row(1, 3), row(2, 4)]);
    assert_eq!(count(&out, "a=t,"), 1);
    assert_eq!(count(&out, "a=p,"), 3);
}

#[test]
fn identical_rows_share_one_image() {
    let mut p = Presenter::new(512);
    let out = present(&mut p, CELL, &[row(0, 9), row(1, 9)]);
    assert_eq!(count(&out, "a=t,"), 1);
    assert_eq!(count(&out, "a=p,"), 2);
}

#[test]
fn changed_plan_deletes_all_placements_first() {
    let mut p = Presenter::new(512);
    present(&mut p, CELL, &[row(0, 1)]);
    let out = String::from_utf8(present(&mut p, CELL, &[row(0, 2)])).unwrap();
    let del = out.find("a=d,d=a").expect("delete all placements");
    assert!(del < out.find("a=p,").unwrap());
}

#[test]
fn clear_deletes_placements_but_keeps_cache() {
    let mut p = Presenter::new(512);
    let plan = [row(0, 1), row(1, 2)];
    present(&mut p, CELL, &plan);
    let mut out = Vec::new();
    p.clear(&mut out).unwrap();
    assert_eq!(count(&out, "a=d,d=a"), 1);
    assert_eq!(count(&out, "d=I"), 0);
    let out = present(&mut p, CELL, &plan);
    assert_eq!(count(&out, "a=t,"), 0);
    assert_eq!(count(&out, "a=p,"), 2);
}

#[test]
fn cell_size_change_resets_everything() {
    let mut p = Presenter::new(512);
    let plan = [row(0, 1)];
    present(&mut p, CELL, &plan);
    let out = present(&mut p, CellPx { w: 9, h: 18 }, &plan);
    assert_eq!(count(&out, "d=I"), 1);
    assert_eq!(count(&out, "a=t,"), 1);
    assert_eq!(count(&out, "a=p,"), 1);
}

#[test]
fn reset_deletes_images_and_empties_cache() {
    let mut p = Presenter::new(512);
    let plan = [row(0, 1), row(1, 2)];
    present(&mut p, CELL, &plan);
    let mut out = Vec::new();
    p.reset(&mut out).unwrap();
    assert_eq!(count(&out, "d=I"), 2);
    assert_eq!(count(&present(&mut p, CELL, &plan), "a=t,"), 2);
}

#[test]
fn lru_evicts_oldest_over_limit() {
    let mut p = Presenter::new(2);
    present(&mut p, CELL, &[row(0, 1)]);
    present(&mut p, CELL, &[row(0, 2)]);
    let out = present(&mut p, CELL, &[row(0, 3)]);
    assert_eq!(count(&out, "d=I"), 1);
    let out = present(&mut p, CELL, &[row(0, 1)]);
    assert_eq!(count(&out, "a=t,"), 1);
}

struct Failing;
impl Write for Failing {
    fn write(&mut self, _: &[u8]) -> io::Result<usize> {
        Err(io::Error::other("boom"))
    }
    fn flush(&mut self) -> io::Result<()> {
        Err(io::Error::other("boom"))
    }
}

#[test]
fn write_error_is_returned() {
    let mut p = Presenter::new(512);
    assert!(p
        .present(&mut Failing, CELL, &[row(0, 1)], &mut render)
        .is_err());
}

#[test]
fn forget_placements_replaces_same_plan_without_retransmitting() {
    let mut p = Presenter::new(512);
    let plan = [row(0, 1), row(1, 2)];
    present(&mut p, CELL, &plan);
    p.forget_placements();
    let out = present(&mut p, CELL, &plan);
    assert_eq!(count(&out, "a=p,"), 2);
    assert_eq!(count(&out, "a=t,"), 0);
    assert!(present(&mut p, CELL, &plan).is_empty());
}
