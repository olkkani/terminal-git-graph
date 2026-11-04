//! Choosing the renderer at startup and showing graph images after each draw.

use std::io::Write;
use std::sync::atomic::{AtomicBool, Ordering};

use ratatui::layout::Rect;

use super::present::Presenter;
use super::raster::{RasterStyle, Rgb};
use super::{kitty, CellPx};
use crate::config::RendererSetting;
use crate::graph::lanes::PALETTE_LEN;
use crate::terminal::Screen;
use crate::ui::dialog::Dialog;
use crate::ui::theme;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GfxMode {
    Text,
    Graphics(CellPx),
}

/// The renderer to start with, and a warning when graphics were forced but cannot work.
pub fn decide_mode(
    setting: RendererSetting,
    probed: Option<CellPx>,
    env_blocked: bool,
) -> (GfxMode, Option<String>) {
    let available = if env_blocked { None } else { probed };
    match (setting, available) {
        (RendererSetting::Text, _) => (GfxMode::Text, None),
        (_, Some(cell)) => (GfxMode::Graphics(cell), None),
        (RendererSetting::Auto, None) => (GfxMode::Text, None),
        (RendererSetting::Graphics, None) => (
            GfxMode::Text,
            Some(
                "renderer = \"graphics\" is set, but this terminal does not support Kitty \
                 graphics; falling back to text rendering."
                    .to_string(),
            ),
        ),
    }
}

/// Lane line width and node size for a cell size, with the text graph's lane colors.
pub fn raster_style(cell: CellPx) -> RasterStyle {
    RasterStyle {
        cell,
        palette: std::array::from_fn::<Rgb, PALETTE_LEN, _>(theme::lane_rgb),
        line_px: (cell.w as f32 / 5.0).max(1.0),
        node_radius_px: cell.w as f32 * 0.45,
    }
}

/// Set once graphics mode starts; exit paths then delete what the terminal holds.
static GRAPHICS_USED: AtomicBool = AtomicBool::new(false);

pub fn graphics_used() -> bool {
    GRAPHICS_USED.load(Ordering::Relaxed)
}

/// Deletes every placement, then every image and its data.
pub fn exit_sequence() -> Vec<u8> {
    let mut out = kitty::delete_all_placements();
    out.extend_from_slice(b"\x1b_Ga=d,d=A,q=2\x1b\\");
    out
}

/// Row images kept in the terminal at once.
const MAX_CACHED: usize = 256;

pub struct GfxSession {
    mode: GfxMode,
    presenter: Presenter,
    /// Whether the screen was last told to leave the graph cells blank.
    active: bool,
    /// Whether placements may be on screen.
    placed: bool,
}

impl GfxSession {
    pub fn new(mode: GfxMode) -> GfxSession {
        if matches!(mode, GfxMode::Graphics(_)) {
            GRAPHICS_USED.store(true, Ordering::Relaxed);
        }
        GfxSession {
            mode,
            presenter: Presenter::new(MAX_CACHED),
            active: false,
            placed: false,
        }
    }

    pub fn mode(&self) -> GfxMode {
        self.mode
    }

    /// Takes a new cell size (`None` when unknown, which keeps the current one);
    /// a change drops every image so rows are re-rendered at the new size.
    pub fn on_resize(
        &mut self,
        screen: &mut impl Screen,
        out: &mut impl Write,
        cell: Option<CellPx>,
    ) {
        let (GfxMode::Graphics(old), Some(new)) = (self.mode, cell) else {
            return;
        };
        if old == new {
            return;
        }
        self.mode = GfxMode::Graphics(new);
        self.placed = false;
        if let Err(e) = self.presenter.reset(out) {
            self.fail(screen, out, &e);
        }
    }

    /// A write failed: drop graphics for the rest of the run and say so.
    fn fail(&mut self, screen: &mut impl Screen, out: &mut impl Write, e: &std::io::Error) {
        let _ = self.presenter.reset(out);
        self.mode = GfxMode::Text;
        self.active = false;
        self.placed = false;
        screen.set_gfx_active(false);
        screen.show_dialog(Dialog::Error {
            title: "Graphics disabled".to_string(),
            message: format!("Drawing graph images failed: {e}. Falling back to text rendering."),
        });
    }
}

/// Runs right after each draw of a screen of size `area`. Shows the graph images
/// when the screen offers them, else removes them. Returns whether the screen must
/// be drawn again because its graph column switched between text and blank cells.
pub fn present_frame(
    session: &mut GfxSession,
    screen: &mut impl Screen,
    out: &mut impl Write,
    area: Rect,
) -> bool {
    let GfxMode::Graphics(cell) = session.mode else {
        return false;
    };
    match screen.gfx_frame(area, cell) {
        // A menu, dialog, too-small message or non-graph screen: text graph, no images.
        None => {
            if session.placed {
                session.placed = false;
                if let Err(e) = session.presenter.clear(out) {
                    session.fail(screen, out, &e);
                    return true;
                }
            }
            if session.active {
                session.active = false;
                screen.set_gfx_active(false);
                return true;
            }
            false
        }
        Some(mut frame) => {
            if !session.active {
                // Blank the graph cells first; the images go over the next draw.
                session.active = true;
                screen.set_gfx_active(true);
                return true;
            }
            match session.presenter.present(
                out,
                frame.style.cell,
                &frame.placements,
                &mut frame.render,
            ) {
                Ok(()) => {
                    session.placed = true;
                    false
                }
                Err(e) => {
                    session.fail(screen, out, &e);
                    true
                }
            }
        }
    }
}
