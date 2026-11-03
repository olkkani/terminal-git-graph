//! Turns a `GraphRow` into terminal cells (two per lane).

use crate::graph::lanes::GraphRow;

#[derive(Clone, PartialEq, Debug)]
pub struct Cell {
    pub ch: char,
    pub color: Option<usize>,
    pub dim: bool,
}

pub struct RenderOpts {
    pub is_head: bool,
    pub ascii: bool,
}

struct Glyphs {
    node: char,
    head: char,
    vertical: char,
    horizontal: char,
    top_left: char,
    top_right: char,
    bottom_left: char,
    bottom_right: char,
    dashed: char,
}

const UNICODE: Glyphs = Glyphs {
    node: '●',
    head: '◯',
    vertical: '│',
    horizontal: '─',
    top_left: '╭',
    top_right: '╮',
    bottom_left: '╰',
    bottom_right: '╯',
    dashed: '┆',
};

const ASCII: Glyphs = Glyphs {
    node: '*',
    head: 'O',
    vertical: '|',
    horizontal: '-',
    top_left: '/',
    top_right: '\\',
    bottom_left: '\\',
    bottom_right: '/',
    dashed: ':',
};

fn glyphs(ascii: bool) -> &'static Glyphs {
    if ascii {
        &ASCII
    } else {
        &UNICODE
    }
}

fn blank() -> Cell {
    Cell { ch: ' ', color: None, dim: false }
}

/// Renders one row; the result has `2 * row.width` cells (lane `i` uses cells `2i` and `2i+1`).
pub fn render_row(row: &GraphRow, opts: &RenderOpts) -> Vec<Cell> {
    let g = glyphs(opts.ascii);
    let mut cells = vec![blank(); 2 * row.width];
    let put = |cells: &mut Vec<Cell>, i: usize, ch: char, color: usize| {
        cells[i] = Cell { ch, color: Some(color), dim: false };
    };

    for p in &row.pass {
        put(&mut cells, 2 * p.lane, g.vertical, p.color);
    }
    // Horizontal runs between the node and each joining/forking lane; drawn over
    // vertical lines they cross, then the curve caps the far end.
    let targets = row.joins.iter().map(|s| (s, false)).chain(row.forks.iter().map(|s| (s, true)));
    for (seg, is_fork) in targets {
        let (lo, hi) = (row.lane.min(seg.lane), row.lane.max(seg.lane));
        for i in 2 * lo + 1..2 * hi {
            put(&mut cells, i, g.horizontal, seg.color);
        }
        let right = seg.lane > row.lane;
        let ch = match (is_fork, right) {
            (true, true) => g.top_right,
            (true, false) => g.top_left,
            (false, true) => g.bottom_right,
            (false, false) => g.bottom_left,
        };
        put(&mut cells, 2 * seg.lane, ch, seg.color);
    }
    let node = if opts.is_head { g.head } else { g.node };
    put(&mut cells, 2 * row.lane, node, row.color);

    if row.is_merge {
        for c in &mut cells {
            c.dim = true;
        }
    }
    cells
}

/// The dashed connector row drawn between the working-tree entry and HEAD.
pub fn render_uncommitted(head_lane: usize, width: usize, ascii: bool) -> Vec<Cell> {
    let mut cells = vec![blank(); 2 * width];
    cells[2 * head_lane].ch = glyphs(ascii).dashed;
    cells
}
