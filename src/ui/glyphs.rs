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

/// Badges are outlined by round parentheses. `ascii` limits the icons to ASCII (or nothing);
/// `nerd_font` swaps the icons for the git glyphs of a Nerd Font.
pub fn badge_glyphs(nerd_font: bool, ascii: bool) -> BadgeGlyphs {
    if ascii {
        return BadgeGlyphs {
            left_cap: "(",
            right_cap: ")",
            check: "*",
            local: "",
            remote: "~",
            tag: "#",
        };
    }
    if nerd_font {
        // lazygit's Nerd Font v3 icons: nf-md-source_branch for local and remote branches
        // alike (the remote shows in the name and color) and nf-fa-tag.
        // Private-use glyphs are ambiguous width by the Unicode tables, but Nerd Font
        // terminals draw them in one cell.
        return BadgeGlyphs {
            left_cap: "(",
            right_cap: ")",
            check: "✓",
            local: "\u{f062c}",
            remote: "\u{f062c}",
            tag: "\u{f02b}",
        };
    }
    BadgeGlyphs {
        left_cap: "(",
        right_cap: ")",
        check: "✓",
        // OCR FORK: a branch shape from the standard Unicode set. U+2387 (the old icon)
        // is only in the system UI fonts, where it is the Option key.
        local: "⑂",
        remote: "⇄",
        tag: "⚑",
    }
}
