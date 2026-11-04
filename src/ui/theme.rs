//! Colors: the lane palette and row highlights, adapted to the terminal's color depth.

use ratatui::style::Color;

use crate::graph::lanes::PALETTE_LEN;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDepth {
    TrueColor,
    Indexed256,
}

impl ColorDepth {
    /// `COLORTERM=truecolor|24bit` or a `*-direct` terminfo name means 24-bit color.
    pub fn detect(colorterm: Option<&str>, term: Option<&str>) -> Self {
        let colorterm = colorterm.unwrap_or("").to_ascii_lowercase();
        let direct = term.is_some_and(|t| t.ends_with("-direct"));
        if colorterm == "truecolor" || colorterm == "24bit" || direct {
            ColorDepth::TrueColor
        } else {
            ColorDepth::Indexed256
        }
    }
}

type Rgb = (u8, u8, u8);

const PALETTE: [Rgb; PALETTE_LEN] = [
    (0x7a, 0xa2, 0xf7), // blue
    (0xf7, 0x76, 0x8e), // red
    (0x9e, 0xce, 0x6a), // green
    (0xff, 0x9e, 0x64), // orange
    (0xbb, 0x9a, 0xf7), // purple
    (0x73, 0xda, 0xca), // teal
    (0xf0, 0xa6, 0xca), // pink
    (0xe0, 0xaf, 0x68), // yellow
];

const HEAD_BG: Rgb = (0x26, 0x32, 0x38);
const SELECTED_BG: Rgb = (0x37, 0x47, 0x4f);
const MUTED: Rgb = (0x90, 0x90, 0x90);

/// A lane color as true RGB, for the graphics renderer.
pub fn lane_rgb(index: usize) -> crate::gfx::raster::Rgb {
    let (r, g, b) = PALETTE[index % PALETTE_LEN];
    crate::gfx::raster::Rgb(r, g, b)
}

pub fn lane_color(index: usize, depth: ColorDepth) -> Color {
    convert(PALETTE[index % PALETTE_LEN], depth)
}

pub fn head_bg(depth: ColorDepth) -> Color {
    convert(HEAD_BG, depth)
}

pub fn selected_bg(depth: ColorDepth) -> Color {
    convert(SELECTED_BG, depth)
}

pub fn muted(depth: ColorDepth) -> Color {
    convert(MUTED, depth)
}

/// Subdued lane color, for the outline of remote branch badges.
pub fn lane_dim(index: usize, depth: ColorDepth) -> Color {
    convert(dim_rgb(index), depth)
}

fn dim_rgb(index: usize) -> Rgb {
    mix(PALETTE[index % PALETTE_LEN], (0, 0, 0), 0.35)
}

/// `lane_dim` as true RGB, for the badge outlines of the graphics renderer.
pub fn lane_dim_rgb(index: usize) -> crate::gfx::raster::Rgb {
    let (r, g, b) = dim_rgb(index);
    crate::gfx::raster::Rgb(r, g, b)
}

/// `muted` as true RGB.
pub fn muted_rgb() -> crate::gfx::raster::Rgb {
    crate::gfx::raster::Rgb(MUTED.0, MUTED.1, MUTED.2)
}

/// `a` moved `t` of the way towards `b`.
fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let m = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t).round() as u8;
    (m(a.0, b.0), m(a.1, b.1), m(a.2, b.2))
}

fn convert(rgb: Rgb, depth: ColorDepth) -> Color {
    match depth {
        ColorDepth::TrueColor => Color::Rgb(rgb.0, rgb.1, rgb.2),
        ColorDepth::Indexed256 => Color::Indexed(nearest_index(rgb)),
    }
}

/// Channel levels of the xterm 6×6×6 color cube (indices 16..=231).
const CUBE_LEVELS: [u8; 6] = [0, 95, 135, 175, 215, 255];

/// Nearest xterm-256 index among the color cube and the grayscale ramp (232..=255).
fn nearest_index((r, g, b): Rgb) -> u8 {
    let dist = |c: Rgb| {
        let d = |a: u8, b: u8| (a as i32 - b as i32).pow(2);
        d(r, c.0) + d(g, c.1) + d(b, c.2)
    };
    let level = |v: u8| {
        (0..6)
            .min_by_key(|&i| (CUBE_LEVELS[i] as i32 - v as i32).abs())
            .unwrap()
    };
    let (ri, gi, bi) = (level(r), level(g), level(b));
    let cube = (CUBE_LEVELS[ri], CUBE_LEVELS[gi], CUBE_LEVELS[bi]);
    let cube_index = 16 + 36 * ri + 6 * gi + bi;

    let avg = (r as i32 + g as i32 + b as i32) / 3;
    let gray_step = ((avg - 8).max(0) / 10).min(23);
    let gray_v = (8 + 10 * gray_step) as u8;
    let gray_index = 232 + gray_step as usize;

    if dist((gray_v, gray_v, gray_v)) < dist(cube) {
        gray_index as u8
    } else {
        cube_index as u8
    }
}
