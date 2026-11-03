use terminal_git_graph::graph::lanes::{GraphRow, LaneSeg};
use terminal_git_graph::graph::render::{render_row, render_uncommitted, Cell, RenderOpts};

fn seg(lane: usize, color: usize) -> LaneSeg {
    LaneSeg { lane, color }
}

fn row(lane: usize, width: usize) -> GraphRow {
    GraphRow {
        hash: "h".into(),
        lane,
        color: 0,
        pass: vec![],
        joins: vec![],
        forks: vec![],
        continues: true,
        is_merge: false,
        width,
    }
}

fn opts() -> RenderOpts {
    RenderOpts {
        is_head: false,
        ascii: false,
    }
}

fn text(cells: &[Cell]) -> String {
    cells.iter().map(|c| c.ch).collect()
}

#[test]
fn linear_row_is_node_and_blank() {
    let cells = render_row(&row(0, 1), &opts());
    assert_eq!(text(&cells), "● ");
    assert_eq!(cells[0].color, Some(0));
    assert_eq!(cells[1].color, None);
}

#[test]
fn merge_row_draws_curve_to_second_parent() {
    let mut r = row(0, 2);
    r.forks = vec![seg(1, 3)];
    r.is_merge = true;
    let cells = render_row(&r, &opts());
    assert_eq!(text(&cells), "●─╮ ");
    assert_eq!(cells[2].color, Some(3));
}

#[test]
fn fork_to_the_left_uses_opposite_curve() {
    let mut r = row(1, 2);
    r.forks = vec![seg(0, 2)];
    assert_eq!(text(&render_row(&r, &opts())), "╭─● ");
}

#[test]
fn join_row_draws_curve_from_right() {
    let mut r = row(0, 2);
    r.joins = vec![seg(1, 2)];
    assert_eq!(text(&render_row(&r, &opts())), "●─╯ ");
}

#[test]
fn join_from_the_left_draws_up_right_curve() {
    let mut r = row(1, 2);
    r.joins = vec![seg(0, 2)];
    assert_eq!(text(&render_row(&r, &opts())), "╰─● ");
}

#[test]
fn head_row_uses_ring_glyph() {
    let cells = render_row(
        &row(0, 1),
        &RenderOpts {
            is_head: true,
            ascii: false,
        },
    );
    assert_eq!(text(&cells), "◯ ");
}

#[test]
fn merge_cells_are_dim() {
    let mut r = row(0, 2);
    r.forks = vec![seg(1, 3)];
    r.is_merge = true;
    assert!(render_row(&r, &opts()).iter().all(|c| c.dim));
    assert!(render_row(&row(0, 1), &opts()).iter().all(|c| !c.dim));
}

#[test]
fn pass_through_lane_draws_vertical_bar() {
    let mut r = row(1, 2);
    r.pass = vec![seg(0, 5)];
    let cells = render_row(&r, &opts());
    assert_eq!(text(&cells), "│ ● ");
    assert_eq!(cells[0].color, Some(5));
}

#[test]
fn fork_into_a_passing_lane_draws_curve_over_the_line() {
    let mut r = row(0, 2);
    r.pass = vec![seg(1, 4)];
    r.forks = vec![seg(1, 4)];
    assert_eq!(text(&render_row(&r, &opts())), "●─╮ ");
}

#[test]
fn uncommitted_row_uses_dashed_connector() {
    let cells = render_uncommitted(1, 2, false);
    assert_eq!(text(&cells), "  ┆ ");
    assert_eq!(cells.len(), 4);
}

#[test]
fn ascii_mode_emits_only_ascii() {
    let mut r = row(1, 4);
    r.pass = vec![seg(0, 1)];
    r.forks = vec![seg(3, 2)];
    r.joins = vec![seg(2, 3)];
    r.is_merge = true;
    let ascii = RenderOpts {
        is_head: true,
        ascii: true,
    };
    assert!(text(&render_row(&r, &ascii)).is_ascii());
    let mut l = row(1, 2);
    l.forks = vec![seg(0, 2)];
    l.joins = vec![seg(0, 2)];
    assert!(text(&render_row(
        &l,
        &RenderOpts {
            is_head: false,
            ascii: true
        }
    ))
    .is_ascii());
    assert!(text(&render_uncommitted(0, 3, true)).is_ascii());
}
