//! Width-dependent layout decisions and display-width text fitting.

use unicode_width::UnicodeWidthChar;

/// Width from which the author column is shown.
pub const AUTHOR_MIN_WIDTH: u16 = 70;
/// Width from which the short-hash column is shown.
pub const HASH_MIN_WIDTH: u16 = 100;
/// Below this size the UI shows only a "too small" message.
pub const MIN_WIDTH: u16 = 20;
pub const MIN_HEIGHT: u16 = 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Columns {
    pub author: bool,
    pub hash: bool,
}

pub fn columns_for(width: u16) -> Columns {
    Columns {
        author: width >= AUTHOR_MIN_WIDTH,
        hash: width >= HASH_MIN_WIDTH,
    }
}

fn char_width(c: char) -> usize {
    c.width().unwrap_or(0)
}

pub fn display_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

/// Cuts `s` to at most `width` display columns, ending in '…' when cut.
pub fn truncate_to_width(s: &str, width: usize) -> String {
    if display_width(s) <= width {
        return s.to_string();
    }
    if width == 0 {
        return String::new();
    }
    let mut out = String::new();
    let mut used = 0;
    for c in s.chars() {
        let w = char_width(c);
        if used + w > width - 1 {
            break;
        }
        out.push(c);
        used += w;
    }
    out.push('…');
    out
}

/// Truncates then pads with spaces to exactly `width` display columns.
pub fn fit_to_width(s: &str, width: usize) -> String {
    let mut out = truncate_to_width(s, width);
    let pad = width - display_width(&out);
    out.extend(std::iter::repeat_n(' ', pad));
    out
}
