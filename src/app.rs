//! Graph view state: loaded commits and lanes, refs, working-tree status and selection.

use std::cell::Cell;

use crate::git::log::{count_commits, load_commits, Commit, LogQuery, Scope, PAGE_SIZE};
use crate::git::refs::{load_refs, Head, RefKind, Refs};
use crate::git::status::{load_status, WorkStatus};
use crate::git::{Git, GitError};
use crate::graph::lanes::{GraphRow, LaneBuilder};
use crate::ui;
use crate::ui::theme::ColorDepth;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Quit,
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    /// Selects a displayed row by index (the uncommitted row, when shown, is 0).
    SelectRow(usize),
    ToggleScope,
    ToggleRemotes,
    Refresh,
}

/// A displayed table row.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Row {
    Uncommitted,
    Commit(usize),
}

pub struct App {
    git: Git,
    pub(crate) depth: ColorDepth,
    bare: bool,
    pub(crate) scope: Scope,
    pub(crate) remotes: bool,
    pub(crate) refs: Refs,
    pub(crate) status: WorkStatus,
    pub(crate) commits: Vec<Commit>,
    pub(crate) graph: Vec<GraphRow>,
    lanes: LaneBuilder,
    /// Commit count for the current scope (all pages).
    pub(crate) total: usize,
    exhausted: bool,
    pub(crate) selected: usize,
    /// First visible row; adjusted while drawing to keep the selection in view.
    pub(crate) offset: Cell<usize>,
    /// Table height at the last draw, used as the PageUp/PageDown step.
    pub(crate) page_height: Cell<usize>,
    quit: bool,
}

impl App {
    pub fn new(git: Git, depth: ColorDepth) -> Result<App, GitError> {
        let bare = git.run(&["rev-parse", "--is-bare-repository"])?.trim() == "true";
        let mut app = App {
            git,
            depth,
            bare,
            scope: Scope::AllBranches,
            remotes: true,
            refs: Refs {
                labels: Vec::new(),
                head: Head::Unborn(String::new()),
            },
            status: WorkStatus::default(),
            commits: Vec::new(),
            graph: Vec::new(),
            lanes: LaneBuilder::new(),
            total: 0,
            exhausted: false,
            selected: 0,
            offset: Cell::new(0),
            page_height: Cell::new(1),
            quit: false,
        };
        app.reload()?;
        Ok(app)
    }

    pub fn apply(&mut self, cmd: Command) -> Result<(), GitError> {
        let page = self.page_height.get().max(1);
        match cmd {
            Command::Quit => self.quit = true,
            Command::Up => self.selected = self.selected.saturating_sub(1),
            Command::Down => self.move_to(self.selected + 1)?,
            Command::PageUp => self.selected = self.selected.saturating_sub(page),
            Command::PageDown => self.move_to(self.selected + page)?,
            Command::Top => self.selected = 0,
            Command::Bottom => {
                self.load_page()?;
                self.selected = self.row_count().saturating_sub(1);
            }
            Command::SelectRow(i) => {
                if i < self.row_count() {
                    self.selected = i;
                }
            }
            Command::ToggleScope => {
                self.scope = match self.scope {
                    Scope::AllBranches => Scope::Current,
                    Scope::Current => Scope::AllBranches,
                };
                self.reload()?;
            }
            Command::ToggleRemotes => {
                self.remotes = !self.remotes;
                self.reload()?;
            }
            Command::Refresh => self.reload()?,
        }
        Ok(())
    }

    pub fn draw(&self, frame: &mut ratatui::Frame) {
        ui::draw(self, frame);
    }

    pub fn selected_hash(&self) -> Option<&str> {
        match self.row(self.selected)? {
            Row::Commit(i) => Some(&self.commits[i].hash),
            Row::Uncommitted => None,
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit
    }

    pub(crate) fn has_uncommitted(&self) -> bool {
        !self.bare && !self.status.is_clean()
    }

    pub(crate) fn row_count(&self) -> usize {
        self.commits.len() + usize::from(self.has_uncommitted())
    }

    pub(crate) fn row(&self, index: usize) -> Option<Row> {
        let first_commit = usize::from(self.has_uncommitted());
        if index < first_commit {
            Some(Row::Uncommitted)
        } else if index < self.row_count() {
            Some(Row::Commit(index - first_commit))
        } else {
            None
        }
    }

    pub(crate) fn head_hash(&self) -> Option<&str> {
        match &self.refs.head {
            Head::Branch(name) => self
                .refs
                .labels
                .iter()
                .find(|l| l.kind == RefKind::LocalBranch && &l.name == name)
                .map(|l| l.target.as_str()),
            Head::Detached(hash) => Some(hash),
            Head::Unborn(_) => None,
        }
    }

    /// Moves the selection, loading the next page when the move reaches the last row.
    fn move_to(&mut self, target: usize) -> Result<(), GitError> {
        if target + 1 >= self.row_count() {
            self.load_page()?;
        }
        self.selected = target.min(self.row_count().saturating_sub(1));
        Ok(())
    }

    /// Reloads refs, status and the first page, keeping the selected commit if still loaded.
    fn reload(&mut self) -> Result<(), GitError> {
        let keep = self.selected_hash().map(str::to_string);
        self.refs = load_refs(&self.git)?;
        self.status = if self.bare {
            WorkStatus::default()
        } else {
            load_status(&self.git)?
        };
        self.total = count_commits(&self.git, self.scope, self.remotes)?;
        self.commits.clear();
        self.graph.clear();
        self.lanes = LaneBuilder::new();
        self.exhausted = false;
        self.load_page()?;
        self.selected = keep
            .and_then(|h| self.commits.iter().position(|c| c.hash == h))
            .map_or(0, |i| i + usize::from(self.has_uncommitted()));
        Ok(())
    }

    fn load_page(&mut self) -> Result<(), GitError> {
        if self.exhausted {
            return Ok(());
        }
        let page = load_commits(
            &self.git,
            &LogQuery {
                scope: self.scope,
                remotes: self.remotes,
                skip: self.commits.len(),
                limit: PAGE_SIZE,
            },
        )?;
        self.exhausted = page.len() < PAGE_SIZE || self.commits.len() + page.len() >= self.total;
        for c in page {
            self.graph.push(self.lanes.push(&c));
            self.commits.push(c);
        }
        Ok(())
    }
}
