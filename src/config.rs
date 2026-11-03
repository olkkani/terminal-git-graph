//! The one settings file: `~/.config/terminal-git-graph/config.toml`. There is no
//! settings UI; a missing file means defaults.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use serde::Deserialize;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Config {
    /// How many folder levels Manygit searches, 1..=5.
    pub scan_depth: u8,
    /// Show each repository's latest tag in the Manygit list.
    pub show_tags: bool,
    pub theme: String,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            scan_depth: 3,
            show_tags: false,
            theme: "default".to_string(),
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
}

impl Default for RawConfig {
    fn default() -> Self {
        let d = Config::default();
        RawConfig {
            scan_depth: d.scan_depth.into(),
            show_tags: d.show_tags,
            theme: d.theme,
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
            Ok(raw) => (
                Config {
                    scan_depth: raw.scan_depth.clamp(1, 5) as u8,
                    show_tags: raw.show_tags,
                    theme: raw.theme,
                },
                None,
            ),
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
