//! Graph view state: loaded commits and lanes, refs, working-tree status, selection,
//! and the commit detail / file diff opened from it.

use std::cell::Cell;
use std::io;
use std::path::PathBuf;
use std::sync::mpsc::{Receiver, TryRecvError};

use crate::actions::{needs_confirm, Action};
use crate::clipboard::{self, CopyVia};
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
use crate::jobs::JobRunner;
use crate::menus::{confirm_dialog, input_dialog, menu_for, with_value, MenuCtx, Target};
use crate::ui;
use crate::ui::dialog::{Dialog, DialogOutcome, DialogState};
use crate::ui::menu::{Menu, MenuId, MenuItem};
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
    /// Opens the menu for the focused ref label, else for the selected row (the
    /// keyboard's right click).
    OpenMenu,
    /// Focuses the previous ref label of the selected row; from none, the last one.
    LabelPrev,
    /// Focuses the next ref label of the selected row; from none, the first one.
    LabelNext,
    /// Esc with a focused ref label: drops the focus.
    ClearLabelFocus,
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
    /// Double click on a ref label: checks out a local branch, asks a local name for a
    /// remote branch, otherwise opens the detail like a double click on the row.
    ActivateLabel {
        row: usize,
        index: usize,
    },
    Fetch,
    Help,
    /// Manygit: leaves the graph for the repository list.
    BackToList,
    /// Manygit: opens the dropdown of repositories to switch to.
    RepoMenu,
    /// Manygit list: fetches every repository that has a remote.
    FetchAllRepos,
}

/// The key bindings shown by `Help`.
const HELP: &str = "\
j/k, Up/Down   Move
PgUp/PgDn      Page
g/G, Home/End  First/last
Enter          Open detail / file list / diff
Tab            Switch table and file list
Esc            Back
h/l, Left/Rt   Focus a ref label of the row
m              Menu of that label, else the row (right click)
a              All branches / current branch
r              Show/hide remotes
f              Fetch
s              Switch repository (Manygit)
R, F5          Refresh
?              This help
q              Quit";

/// Where a graph opened from the Manygit list sits among its siblings.
struct RepoNav {
    name: String,
    /// Every listed repository as (name, path), in list order.
    repos: Vec<(String, PathBuf)>,
}

/// What a graph opened from the Manygit list asks the list to do.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NavRequest {
    ToList,
    Switch(PathBuf),
}

/// A menu action waiting on the dialog it opened.
enum Pending {
    /// Waits for the name or message the action still lacks.
    Input(Action),
    Confirm(Action),
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
    /// Reading failed; the error was shown once and is not retried.
    Failed,
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
            DetailData::Loading | DetailData::Failed => None,
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
    /// Draw the graph with ASCII instead of box-drawing characters.
    pub(crate) ascii: bool,
    pub(crate) nerd_font: bool,
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
    /// The last draw showed only the "too small" message; dialogs and menus were hidden.
    pub(crate) too_small: Cell<bool>,
    pub(crate) detail: Option<DetailView>,
    cache: DetailCache,
    /// Diff body height at the last draw, bounding the diff scroll.
    pub(crate) diff_height: Cell<usize>,
    /// Why the last filesystem-triggered refresh failed; cleared by the next success.
    pub(crate) refresh_error: Option<String>,
    pub(crate) menu: Option<Menu>,
    /// Keyboard-focused ref label as (row, index into `Refs::labels_for`); it only
    /// counts while that row is selected.
    label_focus: Option<(usize, usize)>,
    /// A modal dialog; while open it takes all input.
    pub(crate) dialog: Option<DialogState>,
    pending: Option<Pending>,
    jobs: JobRunner,
    /// The running git action's result channel.
    job: Option<Receiver<Result<String, GitError>>>,
    copier: fn(&str) -> io::Result<CopyVia>,
    quit: bool,
    /// Set when the graph was opened from the Manygit list.
    nav: Option<RepoNav>,
    nav_request: Option<NavRequest>,
}

impl App {
    pub fn new(git: Git, depth: ColorDepth) -> Result<App, GitError> {
        let bare = git.run(&["rev-parse", "--is-bare-repository"])?.trim() == "true";
        let mut app = App {
            git,
            depth,
            ascii: false,
            nerd_font: false,
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
            too_small: Cell::new(false),
            detail: None,
            cache: DetailCache::new(),
            diff_height: Cell::new(1),
            refresh_error: None,
            menu: None,
            label_focus: None,
            dialog: None,
            pending: None,
            jobs: JobRunner::new(),
            job: None,
            copier: clipboard::copy,
            quit: false,
            nav: None,
            nav_request: None,
        };
        app.reload()?;
        Ok(app)
    }

    pub fn apply(&mut self, cmd: Command) -> Result<(), GitError> {
        let cmd = self.resolve(cmd);
        if let Some(state) = &mut self.dialog {
            if cmd == Command::Quit {
                self.quit = true;
            } else if let Some(outcome) = state.handle(cmd) {
                self.dialog = None;
                match (outcome, self.pending.take()) {
                    (DialogOutcome::Accepted(value), Some(Pending::Input(a))) => {
                        self.begin(with_value(a, value.unwrap_or_default()))
                    }
                    (DialogOutcome::Accepted(_), Some(Pending::Confirm(a))) => self.start_action(a),
                    _ => {}
                }
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
            Command::OpenMenu => {
                let hit = match self.focused_label() {
                    Some(index) => Hit::Label {
                        row: self.selected,
                        index,
                    },
                    None => Hit::Row(self.selected),
                };
                self.open_menu(hit, None);
            }
            Command::LabelPrev | Command::LabelNext => {
                let shown = self.shown_labels(self.selected);
                let pos = self
                    .focused_label()
                    .and_then(|i| shown.iter().position(|&s| s == i));
                let next = match (cmd, pos) {
                    (Command::LabelNext, None) => shown.first().copied(),
                    (Command::LabelNext, Some(p)) => shown.get(p + 1).copied(),
                    (_, None) => shown.last().copied(),
                    (_, Some(p)) => p.checked_sub(1).map(|p| shown[p]),
                };
                self.label_focus = next.map(|i| (self.selected, i));
            }
            Command::ClearLabelFocus => self.label_focus = None,
            Command::OpenMenuAt { pos, hit } => {
                if let Hit::Row(row) | Hit::Label { row, .. } = hit {
                    if row < self.row_count() {
                        self.select_row(row);
                        self.open_menu(hit, Some(pos));
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
                let chosen = self.menu.as_ref().and_then(Menu::chosen);
                if let Some(id) = chosen.map(|item| item.id.clone()) {
                    self.menu = None;
                    match id {
                        MenuId::Do(a) => self.begin(a),
                        MenuId::Copy(text) => self.copy(text),
                        MenuId::SwitchRepo(path) => {
                            self.nav_request = Some(NavRequest::Switch(path))
                        }
                        // Only the Manygit list offers it.
                        MenuId::OpenRemote(_) => {}
                    }
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
            Command::ActivateLabel { row, index } => {
                self.select_row(row);
                match self.menu_target(Hit::Label { row, index }) {
                    Some(Target::LocalBranch(name)) => self.begin(Action::CheckoutBranch(name)),
                    Some(Target::RemoteBranch { remote, name }) => {
                        self.begin(Action::CheckoutRemote {
                            remote,
                            branch: name,
                            local_name: String::new(),
                        })
                    }
                    _ => return self.apply(Command::Activate),
                }
            }
            Command::Fetch => self.begin(Action::Fetch),
            Command::Help => self.open_dialog(Dialog::Info {
                title: "Keys".to_string(),
                body: HELP.to_string(),
            }),
            Command::BackToList => {
                if self.nav.is_some() {
                    self.nav_request = Some(NavRequest::ToList);
                }
            }
            Command::RepoMenu => self.open_repo_menu(),
            Command::FetchAllRepos => {}
            // Dialog input is handled above while one is open; resolve() replaced
            // Activate/Back.
            Command::DialogAccept
            | Command::DialogCancel
            | Command::DialogChar(_)
            | Command::DialogBackspace
            | Command::Activate
            | Command::Back => {}
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
        // A menu targets a row index, which a reload may have pointed at another commit.
        if result.is_ok() {
            self.menu = None;
        }
        self.refresh_error = result.err().map(|e| e.message());
        self.follow_selection();
    }

    /// The message of the last failed filesystem-triggered refresh, if it has not succeeded since.
    pub fn refresh_error(&self) -> Option<&str> {
        self.refresh_error.as_deref()
    }

    /// Reads a detail that is still loading. The event loop calls this after drawing,
    /// so the detail header shows before the file list. Returns whether anything loaded.
    /// A git failure marks the detail failed and shows git's message; it never ends the app.
    pub fn load_pending(&mut self) -> Result<bool, GitError> {
        let Some(view) = &mut self.detail else {
            return Ok(false);
        };
        if !matches!(view.data, DetailData::Loading) {
            return Ok(false);
        }
        let loaded = match &view.target {
            DetailTarget::Commit(hash) => self
                .cache
                .get_or_load(&self.git, hash)
                .map(|d| DetailData::Commit(d.clone())),
            DetailTarget::Uncommitted => {
                load_worktree_changes(&self.git).map(DetailData::Uncommitted)
            }
        };
        match loaded {
            Ok(data) => {
                view.data = data;
                let count = view.files().map_or(0, <[FileChange]>::len);
                view.file = view.file.min(count.saturating_sub(1));
            }
            Err(e) => {
                view.data = DetailData::Failed;
                self.open_dialog(Dialog::Error {
                    title: "Cannot read details".to_string(),
                    message: e.message(),
                });
            }
        }
        Ok(true)
    }

    /// Turns the context-dependent `Activate`/`Back` into the command they mean now.
    pub fn resolve(&self, cmd: Command) -> Command {
        match cmd {
            Command::Back if self.focused_label().is_some() => Command::ClearLabelFocus,
            // Enter on a focused local branch checks it out, like a double click.
            Command::Activate if self.focused_local_branch().is_some() => Command::ActivateLabel {
                row: self.selected,
                index: self.focused_label().unwrap_or_default(),
            },
            Command::Activate if self.detail_open() => Command::FocusFiles,
            Command::Activate => Command::OpenDetail,
            Command::Back if self.detail_open() => Command::CloseDetail,
            Command::Back if self.nav.is_some() => Command::BackToList,
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

    /// Shows a modal dialog, closing any open menu and dropping any action that was
    /// waiting on the previous dialog.
    pub fn open_dialog(&mut self, dialog: Dialog) {
        self.menu = None;
        self.pending = None;
        self.dialog = Some(DialogState::new(dialog));
    }

    /// Runs `a` on the job thread right away, without asking. Only one action runs at
    /// a time; starting another meanwhile shows an error.
    pub fn start_action(&mut self, a: Action) {
        if self.job.is_some() {
            self.open_dialog(Dialog::Error {
                title: "Busy".to_string(),
                message: "Another git action is still running.".to_string(),
            });
            return;
        }
        self.job = Some(self.jobs.spawn(self.git.clone(), a));
    }

    pub fn job_running(&self) -> bool {
        self.job.is_some()
    }

    /// Takes the running action's result if it has arrived: success reloads the view,
    /// failure shows git's message. Returns whether the action finished.
    pub fn poll_job(&mut self) -> bool {
        let Some(rx) = &self.job else {
            return false;
        };
        let result = match rx.try_recv() {
            Ok(result) => result,
            Err(TryRecvError::Empty) => return false,
            Err(TryRecvError::Disconnected) => Err(GitError::Parse(
                "the git action stopped without a result".to_string(),
            )),
        };
        self.job = None;
        match result {
            Ok(_) => {
                self.refresh_error = self.reload().err().map(|e| e.message());
                self.follow_selection();
            }
            Err(e) => self.open_dialog(Dialog::Error {
                title: "Git error".to_string(),
                message: action_error_message(&e),
            }),
        }
        true
    }

    /// Draws the graph with ASCII characters, for terminals without box drawing.
    pub fn set_ascii(&mut self, ascii: bool) {
        self.ascii = ascii;
    }

    /// Ends ref badges in round caps; needs a font with those glyphs.
    pub fn set_nerd_font(&mut self, on: bool) {
        self.nerd_font = on;
    }

    /// Replaces the clipboard writer (tests use one that cannot touch the real clipboard).
    pub fn set_copier(&mut self, copier: fn(&str) -> io::Result<CopyVia>) {
        self.copier = copier;
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

    /// Whether the last draw was too small to show anything but a message. Input is
    /// then ignored, except quitting, so a hidden dialog cannot be answered blindly.
    pub fn too_small(&self) -> bool {
        self.too_small.get()
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

    pub fn git(&self) -> &Git {
        &self.git
    }

    /// Marks the graph as opened from the Manygit list: the header gets "‹ Manygit" and
    /// the repository name, whose dropdown lists `repos` as (name, path).
    pub fn embed(&mut self, name: String, repos: Vec<(String, PathBuf)>) {
        self.nav = Some(RepoNav { name, repos });
    }

    /// The name shown in the header when opened from the Manygit list.
    pub(crate) fn repo_name(&self) -> Option<&str> {
        self.nav.as_ref().map(|n| n.name.as_str())
    }

    /// Takes what the graph asked the Manygit list to do, if anything.
    pub fn take_nav_request(&mut self) -> Option<NavRequest> {
        self.nav_request.take()
    }

    /// Opens the dropdown of listed repositories below the header name; the open one
    /// is shown but disabled.
    fn open_repo_menu(&mut self) {
        let Some(nav) = &self.nav else { return };
        let items = nav
            .repos
            .iter()
            .map(|(name, path)| MenuItem {
                label: name.clone(),
                enabled: path.as_path() != self.git.dir(),
                danger: false,
                id: MenuId::SwitchRepo(path.clone()),
            })
            .collect();
        let anchor = ui::header::repo_name_x(self).map(|x| (x, 1));
        self.menu = Some(Menu::new(
            items,
            anchor,
            Hit::Toolbar(crate::input::ToolbarButton::RepoName),
        ));
    }

    /// Asks for a missing value, then confirmation when the action needs it, then runs it.
    fn begin(&mut self, a: Action) {
        // A remote branch whose local branch already exists: check that one out.
        let a = match a {
            Action::CheckoutRemote {
                branch, local_name, ..
            } if local_name.is_empty()
                && self
                    .refs
                    .labels
                    .iter()
                    .any(|l| l.kind == RefKind::LocalBranch && l.name == branch) =>
            {
                Action::CheckoutBranch(branch)
            }
            a => a,
        };
        if let Some(dialog) = input_dialog(&a) {
            self.open_dialog(dialog);
            self.pending = Some(Pending::Input(a));
        } else if needs_confirm(&a) {
            self.open_dialog(confirm_dialog(&a));
            self.pending = Some(Pending::Confirm(a));
        } else {
            self.start_action(a);
        }
    }

    /// Copies `text`; when no clipboard works, shows it so it can be copied by hand.
    fn copy(&mut self, text: String) {
        if (self.copier)(&text).is_err() {
            self.open_dialog(Dialog::Error {
                title: "Copy failed".to_string(),
                message: format!("Could not reach the clipboard. The value is:\n{text}"),
            });
        }
    }

    /// Opens the context menu for a row or ref label, if it points at one.
    fn open_menu(&mut self, hit: Hit, anchor: Option<(u16, u16)>) {
        if let Some(target) = self.menu_target(hit) {
            let items = menu_for(&target, &self.menu_ctx(&target));
            self.menu = Some(Menu::new(items, anchor, hit));
        }
    }

    /// The focused ref label's index, if it belongs to the selected row.
    pub(crate) fn focused_label(&self) -> Option<usize> {
        self.label_focus
            .filter(|&(row, _)| row == self.selected)
            .map(|(_, index)| index)
    }

    /// The focused label's index when it is a local branch.
    fn focused_local_branch(&self) -> Option<usize> {
        let index = self.focused_label()?;
        let Row::Commit(i) = self.row(self.selected)? else {
            return None;
        };
        let label = self
            .refs
            .labels_for(&self.commits[i].hash)
            .get(index)
            .copied()?;
        (label.kind == RefKind::LocalBranch).then_some(index)
    }

    /// Indices (into `Refs::labels_for`) of the labels drawn on `row`.
    fn shown_labels(&self, row: usize) -> Vec<usize> {
        let Some(Row::Commit(i)) = self.row(row) else {
            return Vec::new();
        };
        self.refs
            .labels_for(&self.commits[i].hash)
            .iter()
            .enumerate()
            .filter(|(_, l)| self.remotes || l.kind != RefKind::RemoteBranch)
            .map(|(index, _)| index)
            .collect()
    }

    fn menu_target(&self, hit: Hit) -> Option<Target> {
        match hit {
            Hit::Row(row) => Some(match self.row(row)? {
                Row::Uncommitted => Target::Uncommitted,
                Row::Commit(i) => Target::Commit(self.commits[i].hash.clone()),
            }),
            Hit::Label { row, index } => {
                let Row::Commit(i) = self.row(row)? else {
                    return None;
                };
                let label = *self.refs.labels_for(&self.commits[i].hash).get(index)?;
                Some(match label.kind {
                    RefKind::Head => Target::Commit(label.target.clone()),
                    RefKind::LocalBranch => Target::LocalBranch(label.name.clone()),
                    RefKind::RemoteBranch => {
                        let (remote, name) = label.name.split_once('/')?;
                        Target::RemoteBranch {
                            remote: remote.to_string(),
                            name: name.to_string(),
                        }
                    }
                    RefKind::Tag => Target::Tag(label.name.clone()),
                })
            }
            Hit::File(_) | Hit::Toolbar(_) => None,
        }
    }

    fn menu_ctx(&self, target: &Target) -> MenuCtx {
        let subject = match target {
            Target::Commit(hash) => self
                .commits
                .iter()
                .find(|c| &c.hash == hash)
                .map(|c| c.subject.clone())
                .unwrap_or_default(),
            _ => String::new(),
        };
        // Without the list, push items are simply disabled.
        let remotes = self
            .git
            .run(&["remote"])
            .map(|out| {
                out.lines()
                    .map(str::trim)
                    .filter(|l| !l.is_empty())
                    .map(str::to_string)
                    .collect()
            })
            .unwrap_or_default();
        MenuCtx {
            head: self.refs.head.clone(),
            has_staged: self.status.staged > 0,
            remotes,
            subject,
        }
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
        let old_selected = self.selected;
        // Reload as many pages as were loaded, so a selection deep in the list survives.
        let limit = self.commits.len().max(PAGE_SIZE).div_ceil(PAGE_SIZE) * PAGE_SIZE;
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
                limit,
            },
        )?;
        self.refs = refs;
        self.status = status;
        self.total = total;
        self.commits.clear();
        self.graph.clear();
        self.lanes = LaneBuilder::new();
        self.append_page(page, limit);
        self.selected = keep
            .and_then(|h| self.commits.iter().position(|c| c.hash == h))
            .map_or(0, |i| i + usize::from(self.has_uncommitted()));
        // Keep the selected row at the same screen line; drawing clamps the rest.
        let offset = self.offset.get();
        self.offset
            .set((offset + self.selected).saturating_sub(old_selected));
        self.invalidate_uncommitted_detail();
        self.menu = None;
        self.label_focus = None;
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
        self.append_page(page, PAGE_SIZE);
        Ok(())
    }

    /// Appends commits read with `limit`; fewer than `limit` means the log ended.
    fn append_page(&mut self, page: Vec<Commit>, limit: usize) {
        self.exhausted = page.len() < limit || self.commits.len() + page.len() >= self.total;
        for c in page {
            self.graph.push(self.lanes.push(&c));
            self.commits.push(c);
        }
    }
}

/// What a failed action shows: git's own message, or why the action refused to run.
pub(crate) fn action_error_message(e: &GitError) -> String {
    match e {
        // Raised by the action itself (e.g. no remote to push to), not by unreadable git output.
        GitError::Parse(reason) => {
            let mut chars = reason.chars();
            chars
                .next()
                .map(|first| first.to_uppercase().chain(chars).collect())
                .unwrap_or_default()
        }
        e => e.message(),
    }
}
