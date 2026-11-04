//! Rasterizes graph rows into RGBA images for the Kitty graphics renderer.
//!
//! One image covers one text row (`cell.h` pixels tall); lane `i` spans cell
//! columns `2i` and `2i+1`. Anti-aliasing comes from a distance field: each
//! pixel's coverage is derived from the distance between its center and the
//! shape, and shapes are composited with the "over" operator.

use super::CellPx;
use crate::graph::lanes::{GraphRow, LaneSeg, PALETTE_LEN};
use crate::graph::render::RenderOpts;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rgb(pub u8, pub u8, pub u8);

pub struct RasterStyle {
    pub cell: CellPx,
    pub palette: [Rgb; PALETTE_LEN],
    pub line_px: f32,
    pub node_radius_px: f32,
}

/// Opacity multiplier applied to merge rows so they read as dimmed.
const MERGE_ALPHA: f32 = 0.5;
/// Line segments used to approximate each Bézier curve.
const CURVE_STEPS: usize = 24;

/// Image size in pixels: `(row.width * 2 * cell.w, cell.h)`.
pub fn row_size(row: &GraphRow, style: &RasterStyle) -> (u32, u32) {
    size(row.width, style)
}

fn size(width: usize, style: &RasterStyle) -> (u32, u32) {
    (width as u32 * 2 * style.cell.w as u32, style.cell.h as u32)
}

/// Renders one row as straight (non-premultiplied) RGBA on a transparent background.
pub fn rasterize_row(row: &GraphRow, opts: &RenderOpts, style: &RasterStyle) -> Vec<u8> {
    let (w, h) = row_size(row, style);
    let mut canvas = Canvas::new(w, h);
    let half = style.line_px / 2.0;
    let bottom = h as f32;
    let cy = bottom / 2.0;
    let nx = lane_x(row.lane, style);
    let color = |c: usize| style.palette[c % PALETTE_LEN];

    for p in &row.pass {
        let x = lane_x(p.lane, style);
        canvas.stroke(&[(x, 0.0), (x, bottom)], half, color(p.color));
    }
    // The text renderer's node glyph fills the whole cell, so the graphic node is
    // joined to both cell edges; `GraphRow` does not record whether the lane comes
    // from above, so the upper half is always drawn.
    canvas.stroke(&[(nx, 0.0), (nx, cy)], half, color(row.color));
    if row.continues {
        canvas.stroke(&[(nx, cy), (nx, bottom)], half, color(row.color));
    }
    for s in &row.joins {
        let pts = curve(nx, cy, lane_x(s.lane, style), 0.0);
        canvas.stroke(&pts, half, color(s.color));
    }
    for s in &row.forks {
        let pts = curve(nx, cy, lane_x(s.lane, style), bottom);
        canvas.stroke(&pts, half, color(s.color));
    }

    let r = style.node_radius_px;
    if opts.is_head {
        canvas.punch_disk(nx, cy, r - half);
        canvas.ring(nx, cy, r, half, color(row.color));
    } else {
        canvas.disk(nx, cy, r, color(row.color));
    }

    let fade = if row.is_merge { MERGE_ALPHA } else { 1.0 };
    canvas.into_rgba(fade)
}

/// The dashed connector between the working-tree entry and HEAD, with a small hollow circle.
pub fn rasterize_uncommitted(head_lane: usize, width: usize, style: &RasterStyle) -> Vec<u8> {
    let (w, h) = size(width, style);
    let mut canvas = Canvas::new(w, h);
    let half = style.line_px / 2.0;
    let x = lane_x(head_lane, style);
    let gray = Rgb(150, 150, 150);

    // Three square-ended dashes per row (period h/3, 50% on) tile seamlessly across rows.
    let period = h as f32 / 3.0;
    for k in 0..3 {
        let top = k as f32 * period + period * 0.25;
        canvas.dash(x, top, top + period * 0.5, half, gray);
    }
    let cy = h as f32 / 2.0;
    let r = style.node_radius_px * 0.6;
    canvas.punch_disk(x, cy, r);
    canvas.ring(x, cy, r, half * 0.75, gray);
    canvas.into_rgba(1.0)
}

/// Cache key over everything that affects the pixels (not the hash or selection).
pub fn row_key(row: &GraphRow, opts: &RenderOpts, style: &RasterStyle) -> u64 {
    let mut k = Fnv::new();
    for v in [row.lane, row.color, row.width] {
        k.u64(v as u64);
    }
    for list in [&row.pass, &row.joins, &row.forks] {
        k.u64(list.len() as u64);
        for LaneSeg { lane, color } in list.iter() {
            k.u64(*lane as u64);
            k.u64(*color as u64);
        }
    }
    k.u64(row.continues as u64);
    k.u64(row.is_merge as u64);
    k.u64(opts.is_head as u64);
    k.u64(style.cell.w as u64);
    k.u64(style.cell.h as u64);
    for Rgb(r, g, b) in style.palette {
        k.u64(u64::from_le_bytes([r, g, b, 0, 0, 0, 0, 0]));
    }
    k.u64(style.line_px.to_bits() as u64);
    k.u64(style.node_radius_px.to_bits() as u64);
    k.finish()
}

fn lane_x(lane: usize, style: &RasterStyle) -> f32 {
    let w = style.cell.w as f32;
    lane as f32 * 2.0 * w + w / 2.0
}

/// Cubic Bézier from the node to `(tx, ty)`: leaves the node horizontally and
/// meets the row edge vertically so it lines up with the lane in the next row.
fn curve(nx: f32, cy: f32, tx: f32, ty: f32) -> Vec<(f32, f32)> {
    let p0 = (nx, cy);
    let p1 = (nx + (tx - nx) * 0.75, cy);
    let p2 = (tx, cy + (ty - cy) * 0.35);
    let p3 = (tx, ty);
    (0..=CURVE_STEPS)
        .map(|i| {
            let t = i as f32 / CURVE_STEPS as f32;
            let u = 1.0 - t;
            let (a, b, c, d) = (u * u * u, 3.0 * u * u * t, 3.0 * u * t * t, t * t * t);
            (
                a * p0.0 + b * p1.0 + c * p2.0 + d * p3.0,
                a * p0.1 + b * p1.1 + c * p2.1 + d * p3.1,
            )
        })
        .collect()
}

fn dist_to_segment(p: (f32, f32), a: (f32, f32), b: (f32, f32)) -> f32 {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let len2 = dx * dx + dy * dy;
    let t = if len2 == 0.0 {
        0.0
    } else {
        (((p.0 - a.0) * dx + (p.1 - a.1) * dy) / len2).clamp(0.0, 1.0)
    };
    let (ex, ey) = (p.0 - a.0 - t * dx, p.1 - a.1 - t * dy);
    (ex * ex + ey * ey).sqrt()
}

/// Coverage of a pixel whose center is `d` inside (`d > 0`) or outside a shape edge.
fn coverage(d: f32) -> f32 {
    (d + 0.5).clamp(0.0, 1.0)
}

/// Premultiplied RGBA float canvas.
struct Canvas {
    w: u32,
    h: u32,
    px: Vec<[f32; 4]>,
}

impl Canvas {
    fn new(w: u32, h: u32) -> Self {
        Self {
            w,
            h,
            px: vec![[0.0; 4]; (w * h) as usize],
        }
    }

    /// Visits pixels whose centers lie in the given box, passing the pixel center.
    fn each(
        &mut self,
        x0: f32,
        y0: f32,
        x1: f32,
        y1: f32,
        mut f: impl FnMut((f32, f32), &mut [f32; 4]),
    ) {
        let clip = |v: f32, max: u32| (v.max(0.0) as u32).min(max);
        let (xa, xb) = (clip(x0.floor(), self.w), clip(x1.ceil(), self.w));
        let (ya, yb) = (clip(y0.floor(), self.h), clip(y1.ceil(), self.h));
        for y in ya..yb {
            for x in xa..xb {
                let i = (y * self.w + x) as usize;
                f((x as f32 + 0.5, y as f32 + 0.5), &mut self.px[i]);
            }
        }
    }

    fn over(dst: &mut [f32; 4], c: Rgb, a: f32) {
        if a <= 0.0 {
            return;
        }
        let src = [
            c.0 as f32 / 255.0 * a,
            c.1 as f32 / 255.0 * a,
            c.2 as f32 / 255.0 * a,
            a,
        ];
        for k in 0..4 {
            dst[k] = src[k] + dst[k] * (1.0 - a);
        }
    }

    /// Strokes a polyline `half` px either side of its center line.
    fn stroke(&mut self, pts: &[(f32, f32)], half: f32, c: Rgb) {
        let pad = half + 1.0;
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for &(x, y) in pts {
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        self.each(x0 - pad, y0 - pad, x1 + pad, y1 + pad, |p, dst| {
            let d = pts
                .windows(2)
                .map(|s| dist_to_segment(p, s[0], s[1]))
                .fold(f32::MAX, f32::min);
            Self::over(dst, c, coverage(half - d));
        });
    }

    /// A vertical bar with square ends: anti-aliased in x, area coverage in y.
    fn dash(&mut self, x: f32, top: f32, bottom: f32, half: f32, c: Rgb) {
        self.each(
            x - half - 1.0,
            top - 1.0,
            x + half + 1.0,
            bottom + 1.0,
            |p, dst| {
                let overlap = ((p.1 + 0.5).min(bottom) - (p.1 - 0.5).max(top)).clamp(0.0, 1.0);
                Self::over(dst, c, coverage(half - (p.0 - x).abs()) * overlap);
            },
        );
    }

    fn disk(&mut self, cx: f32, cy: f32, r: f32, c: Rgb) {
        let pad = r + 1.0;
        self.each(cx - pad, cy - pad, cx + pad, cy + pad, |p, dst| {
            let d = ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt();
            Self::over(dst, c, coverage(r - d));
        });
    }

    fn ring(&mut self, cx: f32, cy: f32, r: f32, half: f32, c: Rgb) {
        let pad = r + half + 1.0;
        self.each(cx - pad, cy - pad, cx + pad, cy + pad, |p, dst| {
            let d = ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt();
            Self::over(dst, c, coverage(half - (d - r).abs()));
        });
    }

    /// Erases everything inside a disk (used to hollow out ring nodes).
    fn punch_disk(&mut self, cx: f32, cy: f32, r: f32) {
        let pad = r + 1.0;
        self.each(cx - pad, cy - pad, cx + pad, cy + pad, |p, dst| {
            let d = ((p.0 - cx).powi(2) + (p.1 - cy).powi(2)).sqrt();
            let keep = 1.0 - coverage(r - d);
            for v in dst.iter_mut() {
                *v *= keep;
            }
        });
    }

    /// Converts to straight-alpha RGBA bytes, scaling opacity by `fade`.
    fn into_rgba(self, fade: f32) -> Vec<u8> {
        let to_u8 = |v: f32| (v.clamp(0.0, 1.0) * 255.0).round() as u8;
        let mut out = Vec::with_capacity(self.px.len() * 4);
        for [r, g, b, a] in self.px {
            if a <= 0.0 {
                out.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                out.extend_from_slice(&[to_u8(r / a), to_u8(g / a), to_u8(b / a), to_u8(a * fade)]);
            }
        }
        out
    }
}

/// FNV-1a: a fixed, process-independent hash for cache keys.
struct Fnv(u64);

impl Fnv {
    fn new() -> Self {
        Self(0xcbf2_9ce4_8422_2325)
    }

    fn u64(&mut self, v: u64) {
        for b in v.to_le_bytes() {
            self.0 ^= b as u64;
            self.0 = self.0.wrapping_mul(0x0100_0000_01b3);
        }
    }

    fn finish(&self) -> u64 {
        self.0
    }
}
