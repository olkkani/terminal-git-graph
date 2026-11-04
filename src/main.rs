use std::env;
use std::fmt::Display;
use std::fs;
use std::io::{stdout, IsTerminal};
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::Duration;

use clap::Parser;
use terminal_git_graph::app::App;
use terminal_git_graph::config::{default_config_path, Config, RendererSetting};
use terminal_git_graph::gfx::detect::{env_blocks_graphics, probe_terminal};
use terminal_git_graph::gfx::session::{decide_mode, GfxMode};
use terminal_git_graph::manygit::scan::scan;
use terminal_git_graph::manygit::view::Manygit;
use terminal_git_graph::repo::{self, RepoTarget};
use terminal_git_graph::terminal::{self, Screen};
use terminal_git_graph::ui::dialog::Dialog;
use terminal_git_graph::ui::theme::ColorDepth;

/// A terminal git graph viewer. Opens the graph of the repository at PATH, or lists
/// the repositories found under PATH when it is a plain folder.
#[derive(Parser)]
#[command(name = "tgg", version, about)]
struct Cli {
    /// A repository (shown as a graph) or a folder (its repositories are listed)
    #[arg(default_value = ".")]
    path: PathBuf,
}

/// Why `tgg` stopped: a message for stderr and the exit code.
struct Failure {
    code: u8,
    message: String,
}

impl Failure {
    /// A bad command-line argument; exit code 2, like clap's own usage errors.
    fn usage(message: impl Display) -> Failure {
        Failure {
            code: 2,
            message: message.to_string(),
        }
    }
}

impl<E: Display> From<E> for Failure {
    fn from(e: E) -> Failure {
        Failure {
            code: 1,
            message: e.to_string(),
        }
    }
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match start(&cli.path) {
        Ok(()) => ExitCode::SUCCESS,
        Err(f) => {
            eprintln!("tgg: {}", f.message);
            ExitCode::from(f.code)
        }
    }
}

fn start(path: &Path) -> Result<(), Failure> {
    if !path.exists() {
        return Err(Failure::usage(format!(
            "path not found: {}",
            path.display()
        )));
    }
    if !path.is_dir() {
        return Err(Failure::usage(format!("not a folder: {}", path.display())));
    }
    if !stdout().is_terminal() {
        return Err("standard output is not a terminal; run tgg in an interactive terminal".into());
    }
    let (config, warning) = Config::load(&default_config_path());
    let term = env::var("TERM").ok();
    let depth = ColorDepth::detect(env::var("COLORTERM").ok().as_deref(), term.as_deref());
    // The Linux console and dumb terminals have no box-drawing characters.
    let ascii = matches!(term.as_deref(), Some("linux" | "dumb"));
    // Asked once, before the event loop reads any input.
    let blocked = env_blocks_graphics(env::var_os("TMUX").is_some(), term.as_deref());
    let probed = if !blocked && config.renderer != RendererSetting::Text {
        probe_terminal(Duration::from_millis(300))
    } else {
        None
    };
    let (mode, gfx_warning) = decide_mode(config.renderer, probed, blocked);
    let warning = match (warning, gfx_warning) {
        (Some(a), Some(b)) => Some(format!("{a}\n\n{b}")),
        (a, b) => a.or(b),
    };
    match repo::detect(path)? {
        // A bare repository's graph has no uncommitted row or working-tree actions;
        // `App` finds that out itself.
        RepoTarget::Repo { git, .. } => {
            let mut app = App::new(git, depth)?;
            app.set_ascii(ascii);
            app.set_nerd_font(config.nerd_font);
            show(&mut app, warning, mode)
        }
        RepoTarget::Folder(folder) => {
            let root = fs::canonicalize(&folder).unwrap_or(folder);
            let home = env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
            let entries = scan(&root, config.scan_depth, &home)?;
            let mut list = Manygit::new(root, entries, config.show_tags, depth);
            list.set_ascii(ascii);
            list.set_nerd_font(config.nerd_font);
            show(&mut list, warning, mode)
        }
    }
}

/// Runs `screen` in `mode`, first showing the settings warning if there is one.
fn show(screen: &mut impl Screen, warning: Option<String>, mode: GfxMode) -> Result<(), Failure> {
    if let Some(body) = warning {
        screen.show_dialog(Dialog::Info {
            title: "Settings".to_string(),
            body,
        });
    }
    terminal::run(screen, mode)?;
    Ok(())
}
