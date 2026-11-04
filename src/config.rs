//! The one settings file: `~/.config/terminal-git-graph/config.toml`. There is no
//! settings UI; a missing file means defaults.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

/// Which renderer the graph column uses; `Auto` picks graphics only when detected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RendererSetting {
    Auto,
    Text,
    Graphics,
}

fn parse_renderer(s: &str) -> Option<RendererSetting> {
    match s {
        "auto" => Some(RendererSetting::Auto),
        "text" => Some(RendererSetting::Text),
        "graphics" => Some(RendererSetting::Graphics),
        _ => None,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// How many folder levels Manygit searches, 1..=5.
    pub scan_depth: u8,
    /// Show each repository's latest tag in the Manygit list.
    pub show_tags: bool,
    pub theme: String,
    pub renderer: RendererSetting,
    /// Use Nerd Font glyphs.
    pub nerd_font: bool,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            scan_depth: 3,
            show_tags: false,
            theme: "default".to_string(),
            renderer: RendererSetting::Auto,
            nerd_font: true,
        }
    }
}

/// The file as written; the depth is wide so out-of-range values can be clamped.
#[derive(Deserialize)]
#[serde(default)]
struct RawConfig {
    scan_depth: i64,
    show_tags: bool,
    theme: String,
    renderer: String,
    nerd_font: bool,
}

impl Default for RawConfig {
    fn default() -> Self {
        let d = Config::default();
        RawConfig {
            scan_depth: d.scan_depth.into(),
            show_tags: d.show_tags,
            theme: d.theme,
            renderer: "auto".to_string(),
            nerd_font: d.nerd_font,
        }
    }
}

impl Config {
    /// Reads `path`. A missing file gives the defaults; an unreadable or invalid one
    /// gives the defaults plus a warning to show. The depth is clamped to 1..=5.
    pub fn load(path: &Path) -> (Config, Option<String>) {
        let text = match fs::read_to_string(path) {
            Ok(text) => text,
            Err(e) if e.kind() == io::ErrorKind::NotFound => return (Config::default(), None),
            Err(e) => return (Config::default(), Some(warning(path, &e.to_string()))),
        };
        match toml::from_str::<RawConfig>(&text) {
            Ok(raw) => {
                let (renderer, warn) = match parse_renderer(&raw.renderer) {
                    Some(r) => (r, None),
                    None => (
                        RendererSetting::Auto,
                        Some(format!(
                            "Invalid renderer {:?} in {} (expected auto, text or graphics). Using auto.",
                            raw.renderer,
                            path.display()
                        )),
                    ),
                };
                (
                    Config {
                        scan_depth: raw.scan_depth.clamp(1, 5) as u8,
                        show_tags: raw.show_tags,
                        theme: raw.theme,
                        renderer,
                        nerd_font: raw.nerd_font,
                    },
                    warn,
                )
            }
            Err(e) => (Config::default(), Some(warning(path, e.message()))),
        }
    }
}

fn warning(path: &Path, reason: &str) -> String {
    format!(
        "Ignoring {}: {}. Using default settings.",
        path.display(),
        reason.trim()
    )
}

/// `~/.config/terminal-git-graph/config.toml`.
pub fn default_config_path() -> PathBuf {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .unwrap_or_default();
    home.join(".config/terminal-git-graph/config.toml")
}
