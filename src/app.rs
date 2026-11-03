//! Graph view state: loaded commits and lanes, refs, working-tree status, selection,
//! and the commit detail / file diff opened from it.

use std::cell::Cell;

use crate::git::detail::{
    load_file_diff, load_worktree_changes, load_worktree_diff, CommitDetail, DetailCache,
    FileChange,
};
use crate::git::log::{count_commits, load_commits, Commit, LogQuery, Scope, PAGE_SIZE};
use crate::git::refs::{load_refs, Head, RefKind, Refs};
use crate::git::status::{load_status, WorkStatus};
use crate::git::{Git, GitError};
use crate::graph::lanes::{GraphRow, LaneBuilder};
use crate::input::{Hit, InputMode};
use crate::ui;
use crate::ui::dialog::{Dialog, DialogState};
use crate::ui::menu::Menu;
use crate::ui::theme::ColorDepth;
use crate::watch::WatchEvent;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Command {
    Quit,
    Up,
    Down,
    PageUp,
    PageDown,
    Top,
    Bottom,
    /// Selects a displayed row by index (the uncommitted row, when shown, is 0) and
    /// focuses the table.
    SelectRow(usize),
    /// Enter: opens the detail, or focuses its file list when it is already open.
    Activate,
    /// Esc in the table: closes the detail, or quits when none is open.
    Back,
    ToggleScope,
    ToggleRemotes,
    Refresh,
    /// Opens the detail of the selected row; it then follows the selection.
    OpenDetail,
    CloseDetail,
    /// Moves keyboard focus between the table and the detail's file list.
    FocusFiles,
    FileUp,
    FileDown,
    FileTop,
    FileBottom,
    /// Opens the diff of the selected file.
    OpenDiff,
    CloseDiff,
    DiffScrollUp,
    DiffScrollDown,
    /// Scrolls the diff by its visible height.
    DiffPageUp,
    DiffPageDown,
    DiffTop,
    DiffBottom,
    /// Selects a file in the detail and focuses the file list.
    SelectFile(usize),
    /// Opens the menu for the selected row (the keyboard's right click).
    OpenMenu,
    /// Right click: selects the row under `hit` and opens its menu at `pos`.
    OpenMenuAt {
        pos: (u16, u16),
        hit: Hit,
    },
    MenuUp,
    MenuDown,
    /// Runs the highlighted menu item.
    MenuSelect,
    /// Runs the menu item at an index (a click on it), if enabled.
    MenuPick(usize),
    MenuClose,
    DialogAccept,
    DialogCancel,
    DialogChar(char),
    DialogBackspace,
    Fetch,
    Help,
}

/// Where keyboard input goes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Focus {
    Table,
    Files,
    Diff,
}

/// What a detail view shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum DetailTarget {
    Commit(String),
    Uncommitted,
}

pub(crate) enum DetailData {
    /// Not read yet: only the header (subject/hash) can be drawn.
    Loading,
    Commit(CommitDetail),
    Uncommitted(Vec<FileChange>),
}

pub(crate) struct DetailView {
    pub(crate) target: DetailTarget,
    pub(crate) data: DetailData,
    /// Selected file index.
    pub(crate) file: usize,
    pub(crate) files_focused: bool,
    pub(crate) diff: Option<DiffView>,
}

impl DetailView {
    pub(crate) fn files(&self) -> Option<&[FileChange]> {
        match &self.data {
            DetailData::Loading => None,
            DetailData::Commit(d) => Some(&d.files),
            DetailData::Uncommitted(files) => Some(files),
        }
    }
}

pub(crate) struct DiffView {
    pub(crate) path: String,
    pub(crate) lines: Vec<String>,
    /// First visible line.
    pub(crate) scroll: usize,
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
    pub(crate) detail: Option<DetailView>,
    cache: DetailCache,
    /// Diff body height at the last draw, bounding the diff scroll.
    pub(crate) diff_height: Cell<usize>,
    /// Why the last filesystem-triggered refresh failed; cleared by the next success.
    pub(crate) refresh_error: Option<String>,
    pub(crate) menu: Option<Menu>,
    /// A modal dialog; while open it takes all input.
    pub(crate) dialog: Option<DialogState>,
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
            detail: None,
            cache: DetailCache::new(),
            diff_height: Cell::new(1),
            refresh_error: None,
            menu: None,
            dialog: None,
            quit: false,
        };
        app.reload()?;
        Ok(app)
    }

    pub fn apply(&mut self, cmd: Command) -> Result<(), GitError> {
        let cmd = self.resolve(cmd);
        if let Some(state) = &mut self.dialog {
            if cmd == Command::Quit {
                self.quit = true;
            } else if state.handle(cmd).is_some() {
                // The outcome is acted on once dialogs are tied to git actions.
                self.dialog = None;
            }
            return Ok(());
        }
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
            Command::SelectRow(i) => self.select_row(i),
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
            Command::OpenDetail => {
                if self.detail.is_none() {
                    self.detail = self.current_target().map(|target| DetailView {
                        target,
                        data: DetailData::Loading,
                        file: 0,
                        files_focused: false,
                        diff: None,
                    });
                }
            }
            Command::CloseDetail => self.detail = None,
            Command::FocusFiles => {
                if let Some(view) = &mut self.detail {
                    view.files_focused = !view.files_focused;
                }
            }
            Command::FileUp => {
                if let Some(view) = &mut self.detail {
                    view.file = view.file.saturating_sub(1);
                }
            }
            Command::FileDown => {
                if let Some(view) = &mut self.detail {
                    let count = view.files().map_or(0, <[FileChange]>::len);
                    view.file = (view.file + 1).min(count.saturating_sub(1));
                }
            }
            Command::FileTop => {
                if let Some(view) = &mut self.detail {
                    view.file = 0;
                }
            }
            Command::FileBottom => {
                if let Some(view) = &mut self.detail {
                    let count = view.files().map_or(0, <[FileChange]>::len);
                    view.file = count.saturating_sub(1);
                }
            }
            Command::OpenDiff => self.open_diff()?,
            Command::CloseDiff => {
                if let Some(view) = &mut self.detail {
                    view.diff = None;
                }
            }
            Command::DiffScrollUp => {
                if let Some(diff) = self.detail.as_mut().and_then(|v| v.diff.as_mut()) {
                    diff.scroll = diff.scroll.saturating_sub(1);
                }
            }
            Command::DiffScrollDown => {
                let height = self.diff_height.get();
                if let Some(diff) = self.detail.as_mut().and_then(|v| v.diff.as_mut()) {
                    let max = diff.lines.len().saturating_sub(height);
                    diff.scroll = (diff.scroll + 1).min(max);
                }
            }
            Command::DiffPageUp => {
                let height = self.diff_height.get().max(1);
                if let Some(diff) = self.detail.as_mut().and_then(|v| v.diff.as_mut()) {
                    diff.scroll = diff.scroll.saturating_sub(height);
                }
            }
            Command::DiffPageDown => {
                let height = self.diff_height.get();
                if let Some(diff) = self.detail.as_mut().and_then(|v| v.diff.as_mut()) {
                    let max = diff.lines.len().saturating_sub(height);
                    diff.scroll = (diff.scroll + height.max(1)).min(max);
                }
            }
            Command::DiffTop => {
                if let Some(diff) = self.detail.as_mut().and_then(|v| v.diff.as_mut()) {
                    diff.scroll = 0;
                }
            }
            Command::DiffBottom => {
                let height = self.diff_height.get();
                if let Some(diff) = self.detail.as_mut().and_then(|v| v.diff.as_mut()) {
                    diff.scroll = diff.lines.len().saturating_sub(height);
                }
            }
            Command::SelectFile(i) => {
                if let Some(view) = &mut self.detail {
                    if i < view.files().map_or(0, <[FileChange]>::len) {
                        view.file = i;
                        view.files_focused = true;
                    }
                }
            }
            // Menu items are filled in once git actions exist; until then menus are empty.
            Command::OpenMenu => {
                if self.row(self.selected).is_some() {
                    self.menu = Some(Menu::new(Vec::new(), None, Hit::Row(self.selected)));
                }
            }
            Command::OpenMenuAt { pos, hit } => {
                if let Hit::Row(row) | Hit::Label { row, .. } = hit {
                    if row < self.row_count() {
                        self.select_row(row);
                        self.menu = Some(Menu::new(Vec::new(), Some(pos), hit));
                    }
                }
            }
            Command::MenuUp => {
                if let Some(menu) = &mut self.menu {
                    menu.up();
                }
            }
            Command::MenuDown => {
                if let Some(menu) = &mut self.menu {
                    menu.down();
                }
            }
            Command::MenuSelect => {
                // Running the chosen item comes with the git actions.
                if self.menu.as_ref().is_some_and(|m| m.chosen().is_some()) {
                    self.menu = None;
                }
            }
            Command::MenuPick(i) => {
                if let Some(menu) = &mut self.menu {
                    if menu.items.get(i).is_some_and(|item| item.enabled) {
                        menu.selected = Some(i);
                        return self.apply(Command::MenuSelect);
                    }
                }
            }
            Command::MenuClose => self.menu = None,
            // Dialog input is handled above while one is open; resolve() replaced
            // Activate/Back; Fetch and Help are not connected yet.
            Command::DialogAccept
            | Command::DialogCancel
            | Command::DialogChar(_)
            | Command::DialogBackspace
            | Command::Activate
            | Command::Back
            | Command::Fetch
            | Command::Help => {}
        }
        self.follow_selection();
        Ok(())
    }

    /// Applies a filesystem change: refs reload the first page keeping the selected
    /// commit; working-tree edits refresh only the status. Changes arrive while git is
    /// mid-operation, so a failure keeps the current view and is kept in
    /// `refresh_error` instead of being returned.
    pub fn on_watch(&mut self, ev: WatchEvent) {
        let result = match ev {
            WatchEvent::RefsChanged => self.reload(),
            WatchEvent::WorktreeChanged => self.refresh_status(),
        };
        self.refresh_error = result.err().map(|e| e.message());
        self.follow_selection();
    }

    /// The message of the last failed filesystem-triggered refresh, if it has not succeeded since.
    pub fn refresh_error(&self) -> Option<&str> {
        self.refresh_error.as_deref()
    }

    /// Reads a detail that is still loading. The event loop calls this after drawing,
    /// so the detail header shows before the file list. Returns whether anything loaded.
    pub fn load_pending(&mut self) -> Result<bool, GitError> {
        let Some(view) = &mut self.detail else {
            return Ok(false);
        };
        if !matches!(view.data, DetailData::Loading) {
            return Ok(false);
        }
        view.data = match &view.target {
            DetailTarget::Commit(hash) => {
                DetailData::Commit(self.cache.get_or_load(&self.git, hash)?.clone())
            }
            DetailTarget::Uncommitted => DetailData::Uncommitted(load_worktree_changes(&self.git)?),
        };
        let count = view.files().map_or(0, <[FileChange]>::len);
        view.file = view.file.min(count.saturating_sub(1));
        Ok(true)
    }

    /// Turns the context-dependent `Activate`/`Back` into the command they mean now.
    pub fn resolve(&self, cmd: Command) -> Command {
        match cmd {
            Command::Activate if self.detail_open() => Command::FocusFiles,
            Command::Activate => Command::OpenDetail,
            Command::Back if self.detail_open() => Command::CloseDetail,
            Command::Back => Command::Quit,
            c => c,
        }
    }

    /// Where input goes: an open dialog, then an open menu, then the focused view.
    pub fn input_mode(&self) -> InputMode {
        if self.dialog.is_some() {
            InputMode::Dialog
        } else if self.menu.is_some() {
            InputMode::Menu
        } else {
            match self.focus() {
                Focus::Table => InputMode::Graph,
                Focus::Files => InputMode::Detail,
                Focus::Diff => InputMode::Diff,
            }
        }
    }

    /// Shows a modal dialog, closing any open menu.
    pub fn open_dialog(&mut self, dialog: Dialog) {
        self.menu = None;
        self.dialog = Some(DialogState::new(dialog));
    }

    pub fn dialog(&self) -> Option<&DialogState> {
        self.dialog.as_ref()
    }

    pub fn menu(&self) -> Option<&Menu> {
        self.menu.as_ref()
    }

    pub fn focus(&self) -> Focus {
        match &self.detail {
            Some(view) if view.diff.is_some() => Focus::Diff,
            Some(view) if view.files_focused => Focus::Files,
            _ => Focus::Table,
        }
    }

    pub fn detail_open(&self) -> bool {
        self.detail.is_some()
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

    /// Selects row `i` and gives the table the focus.
    fn select_row(&mut self, i: usize) {
        if i < self.row_count() {
            self.selected = i;
            if let Some(view) = &mut self.detail {
                view.files_focused = false;
            }
        }
    }

    fn current_target(&self) -> Option<DetailTarget> {
        Some(match self.row(self.selected)? {
            Row::Commit(i) => DetailTarget::Commit(self.commits[i].hash.clone()),
            Row::Uncommitted => DetailTarget::Uncommitted,
        })
    }

    /// Points an open detail at the selected row, closing it when nothing is selected.
    fn follow_selection(&mut self) {
        let target = self.current_target();
        let Some(view) = &mut self.detail else {
            return;
        };
        match target {
            None => self.detail = None,
            Some(t) if t != view.target => {
                view.target = t;
                view.data = DetailData::Loading;
                view.file = 0;
                view.diff = None;
            }
            Some(_) => {}
        }
    }

    fn open_diff(&mut self) -> Result<(), GitError> {
        let Some(view) = &mut self.detail else {
            return Ok(());
        };
        let Some(change) = view.files().and_then(|f| f.get(view.file)) else {
            return Ok(());
        };
        let text = match &view.target {
            DetailTarget::Commit(hash) => load_file_diff(&self.git, hash, &change.path)?,
            DetailTarget::Uncommitted => load_worktree_diff(&self.git, change)?,
        };
        view.diff = Some(DiffView {
            path: change.path.clone(),
            lines: text.lines().map(str::to_string).collect(),
            scroll: 0,
        });
        Ok(())
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

    /// Re-reads the working-tree status only, keeping the same row selected when the
    /// uncommitted row appears or goes. Bare repos have no status.
    fn refresh_status(&mut self) -> Result<(), GitError> {
        if self.bare {
            return Ok(());
        }
        let status = load_status(&self.git)?;
        let before = usize::from(self.has_uncommitted());
        self.status = status;
        let after = usize::from(self.has_uncommitted());
        self.selected = (self.selected + after).saturating_sub(before);
        self.invalidate_uncommitted_detail();
        Ok(())
    }

    /// Reloads refs, status and the first page, keeping the selected commit if still loaded.
    /// Everything is read before the view changes, so a failure leaves it as it was.
    fn reload(&mut self) -> Result<(), GitError> {
        let keep = self.selected_hash().map(str::to_string);
        let refs = load_refs(&self.git)?;
        let status = if self.bare {
            WorkStatus::default()
        } else {
            load_status(&self.git)?
        };
        let total = count_commits(&self.git, self.scope, self.remotes)?;
        let page = load_commits(
            &self.git,
            &LogQuery {
                scope: self.scope,
                remotes: self.remotes,
                skip: 0,
                limit: PAGE_SIZE,
            },
        )?;
        self.refs = refs;
        self.status = status;
        self.total = total;
        self.commits.clear();
        self.graph.clear();
        self.lanes = LaneBuilder::new();
        self.append_page(page);
        self.selected = keep
            .and_then(|h| self.commits.iter().position(|c| c.hash == h))
            .map_or(0, |i| i + usize::from(self.has_uncommitted()));
        self.invalidate_uncommitted_detail();
        self.refresh_error = None;
        Ok(())
    }

    /// Re-reads an open uncommitted detail on the next load: the working tree may have changed.
    fn invalidate_uncommitted_detail(&mut self) {
        if let Some(view) = &mut self.detail {
            if view.target == DetailTarget::Uncommitted {
                view.data = DetailData::Loading;
                view.diff = None;
            }
        }
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
        self.append_page(page);
        Ok(())
    }

    fn append_page(&mut self, page: Vec<Commit>) {
        self.exhausted = page.len() < PAGE_SIZE || self.commits.len() + page.len() >= self.total;
        for c in page {
            self.graph.push(self.lanes.push(&c));
            self.commits.push(c);
        }
    }
}
