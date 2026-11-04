use terminal_git_graph::gfx::raster::*;
use terminal_git_graph::gfx::CellPx;
use terminal_git_graph::graph::lanes::{GraphRow, LaneSeg};
use terminal_git_graph::graph::render::RenderOpts;

fn style() -> RasterStyle {
    RasterStyle {
        cell: CellPx { w: 10, h: 20 },
        palette: [
            Rgb(230, 60, 60),
            Rgb(60, 200, 90),
            Rgb(70, 120, 240),
            Rgb(240, 200, 40),
            Rgb(200, 80, 220),
            Rgb(40, 210, 220),
            Rgb(250, 140, 40),
            Rgb(160, 160, 160),
        ],
        line_px: 2.0,
        node_radius_px: 4.0,
    }
}

fn opts(is_head: bool) -> RenderOpts {
    RenderOpts {
        is_head,
        ascii: false,
    }
}

fn seg(lane: usize, color: usize) -> LaneSeg {
    LaneSeg { lane, color }
}

fn row(lane: usize, color: usize, width: usize) -> GraphRow {
    GraphRow {
        hash: "abc".into(),
        lane,
        color,
        pass: vec![],
        joins: vec![],
        forks: vec![],
        from_above: true,
        continues: true,
        is_merge: false,
        width,
    }
}

fn px(buf: &[u8], w: u32, x: u32, y: u32) -> [u8; 4] {
    let i = ((y * w + x) * 4) as usize;
    [buf[i], buf[i + 1], buf[i + 2], buf[i + 3]]
}

/// Center x of lane `i` for the 10x20 test cell.
fn lane_x(i: u32) -> u32 {
    i * 20 + 5
}

#[test]
fn output_length_matches_row_size() {
    let s = style();
    let r = row(0, 0, 3);
    let (w, h) = row_size(&r, &s);
    assert_eq!((w, h), (60, 20));
    assert_eq!(
        rasterize_row(&r, &opts(false), &s).len(),
        (w * h * 4) as usize
    );
}

#[test]
fn tip_row_has_no_stub_above_node() {
    let s = style();
    let mut r = row(0, 0, 1);
    r.from_above = false;
    let (w, _) = row_size(&r, &s);
    let buf = rasterize_row(&r, &opts(false), &s);
    assert_eq!(px(&buf, w, lane_x(0), 0)[3], 0);
}

#[test]
fn chained_row_draws_lane_above_node() {
    let s = style();
    let r = row(0, 0, 1);
    let (w, _) = row_size(&r, &s);
    let buf = rasterize_row(&r, &opts(false), &s);
    assert_eq!(px(&buf, w, lane_x(0), 0)[3], 255);
}

#[test]
fn background_is_transparent() {
    let s = style();
    let r = row(0, 0, 2);
    let (w, h) = row_size(&r, &s);
    let buf = rasterize_row(&r, &opts(false), &s);
    for (x, y) in [(0, 0), (w - 1, 0), (0, h - 1), (w - 1, h - 1)] {
        assert_eq!(px(&buf, w, x, y)[3], 0, "corner ({x},{y})");
    }
}

#[test]
fn node_center_is_opaque_lane_color() {
    let s = style();
    let r = row(1, 2, 2);
    let (w, _) = row_size(&r, &s);
    let buf = rasterize_row(&r, &opts(false), &s);
    let Rgb(cr, cg, cb) = s.palette[2];
    assert_eq!(px(&buf, w, lane_x(1), 10), [cr, cg, cb, 255]);
}

#[test]
fn head_row_node_is_a_ring() {
    let s = style();
    let r = row(0, 0, 1);
    let (w, _) = row_size(&r, &s);
    let buf = rasterize_row(&r, &opts(true), &s);
    assert_eq!(px(&buf, w, lane_x(0), 10)[3], 0, "ring center");
    // Pixel (8, 10) has its center ~3.5 px from the node center: on the ring.
    assert_eq!(px(&buf, w, 8, 10)[3], 255, "ring edge");
}

#[test]
fn pass_lane_draws_continuous_vertical_line() {
    let s = style();
    let mut r = row(0, 0, 2);
    r.pass = vec![seg(1, 1)];
    let (w, h) = row_size(&r, &s);
    let buf = rasterize_row(&r, &opts(false), &s);
    for y in 0..h {
        assert!(px(&buf, w, lane_x(1), y)[3] > 0, "y={y}");
    }
}

#[test]
fn join_curve_connects_neighbor_lane_at_top() {
    let s = style();
    let mut r = row(0, 0, 2);
    r.joins = vec![seg(1, 3)];
    let (w, _) = row_size(&r, &s);
    let buf = rasterize_row(&r, &opts(false), &s);
    assert!(px(&buf, w, lane_x(1), 0)[3] > 0);
}

#[test]
fn fork_curve_reaches_bottom_in_target_lane() {
    let s = style();
    let mut r = row(0, 0, 3);
    r.forks = vec![seg(2, 4)];
    r.is_merge = true;
    let (w, h) = row_size(&r, &s);
    let buf = rasterize_row(&r, &opts(false), &s);
    assert!(px(&buf, w, lane_x(2), h - 1)[3] > 0);
}

#[test]
fn merge_row_alpha_is_reduced() {
    let s = style();
    let mut r = row(0, 0, 2);
    r.forks = vec![seg(1, 1)];
    let (w, _) = row_size(&r, &s);
    let plain = rasterize_row(&r, &opts(false), &s);
    r.is_merge = true;
    let merged = rasterize_row(&r, &opts(false), &s);
    let a_plain = px(&plain, w, lane_x(0), 10)[3];
    let a_merged = px(&merged, w, lane_x(0), 10)[3];
    assert!(
        a_merged > 0 && a_merged < a_plain,
        "{a_merged} vs {a_plain}"
    );
}

#[test]
fn rasterization_is_deterministic_and_key_stable() {
    let s = style();
    let mut r = row(1, 2, 3);
    r.pass = vec![seg(0, 1)];
    r.forks = vec![seg(2, 5)];
    let o = opts(false);
    assert_eq!(rasterize_row(&r, &o, &s), rasterize_row(&r, &o, &s));
    assert_eq!(row_key(&r, &o, &s), row_key(&r, &o, &s));

    let mut recolored = r.clone();
    recolored.color = 3;
    assert_ne!(row_key(&r, &o, &s), row_key(&recolored, &o, &s));
}

#[test]
fn seamless_tiling_between_rows() {
    let s = style();
    let mut upper = row(0, 0, 2);
    upper.pass = vec![seg(1, 1)];
    let mut lower = row(0, 0, 2);
    lower.pass = vec![seg(1, 1)];
    let (w, h) = row_size(&upper, &s);
    let a = rasterize_row(&upper, &opts(false), &s);
    let b = rasterize_row(&lower, &opts(false), &s);
    for x in 0..w {
        assert_eq!(px(&a, w, x, h - 1)[3], px(&b, w, x, 0)[3], "x={x}");
    }
}

#[test]
fn uncommitted_row_is_dashed() {
    let s = style();
    let buf = rasterize_uncommitted(1, 2, &s);
    let w = 40;
    assert_eq!(buf.len(), (w * 20 * 4) as usize);
    let column: Vec<u8> = (0..20).map(|y| px(&buf, w, lane_x(1), y)[3]).collect();
    assert!(column.contains(&0), "{column:?}");
    assert!(column.iter().any(|&a| a > 0), "{column:?}");
}
