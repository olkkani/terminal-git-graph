//! Glyph sets for the pill-shaped ref badges.

/// Characters a badge is drawn with. Every one is a single cell wide (or empty).
pub struct BadgeGlyphs {
    pub left_cap: &'static str,
    pub right_cap: &'static str,
    pub check: &'static str,
    pub local: &'static str,
    pub remote: &'static str,
    pub tag: &'static str,
}

/// With `nerd_font` the pill ends in round caps; without it the badge is a square pill
/// padded by a space on each side. `ascii` limits everything to ASCII (or nothing).
pub fn badge_glyphs(nerd_font: bool, ascii: bool) -> BadgeGlyphs {
    if ascii {
        return BadgeGlyphs {
            left_cap: "",
            right_cap: "",
            check: "*",
            local: "",
            remote: "~",
            tag: "#",
        };
    }
    // Round caps come from Geometric Shapes: the Nerd Font private-use caps are
    // ambiguous-width, which the single-width rule forbids.
    let (left_cap, right_cap) = if nerd_font { ("◖", "◗") } else { ("", "") };
    BadgeGlyphs {
        left_cap,
        right_cap,
        check: "✓",
        local: "⎇",
        remote: "⇄",
        tag: "⚑",
    }
}
