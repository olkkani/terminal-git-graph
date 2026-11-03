use std::env;
use std::process::ExitCode;
use std::sync::mpsc;

use terminal_git_graph::app::App;
use terminal_git_graph::repo::{self, RepoTarget};
use terminal_git_graph::terminal;
use terminal_git_graph::ui::theme::ColorDepth;
use terminal_git_graph::watch;

/// Opens the graph for the current directory's repository (CLI arguments come later).
fn main() -> ExitCode {
    let result = (|| -> Result<(), Box<dyn std::error::Error>> {
        let cwd = env::current_dir()?;
        let git = match repo::detect(&cwd)? {
            RepoTarget::Repo { git, .. } => git,
            RepoTarget::Folder(path) => {
                return Err(format!("not a git repository: {}", path.display()).into())
            }
        };
        let depth = ColorDepth::detect(
            env::var("COLORTERM").ok().as_deref(),
            env::var("TERM").ok().as_deref(),
        );
        let (tx, rx) = mpsc::channel();
        let _watch = watch::watch(&git, tx)?;
        let mut app = App::new(git, depth)?;
        terminal::run(&mut app, &rx)
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("tgg: {e}");
            ExitCode::FAILURE
        }
    }
}
