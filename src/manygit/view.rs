//! The Manygit screen: repositories grouped by folder with their sync state, a menu per
//! repository, Fetch all, and the graphs opened from the list. A visited repository's
//! graph is kept, keyed by path, so reopening it draws at once.

use std::cell::Cell;
use std::collections::HashMap;
use std::io;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{self, Receiver, TryRecvError};
use std::thread;
use std::time::{Duration, Instant};

use crossterm::event::{KeyEvent, MouseButton, MouseEvent, MouseEventKind};
use ratatui::layout::Rect;
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::Frame;

use super::status::{repo_status, RepoStatus, SyncState};
use super::RepoEntry;
use crate::actions::{execute, needs_confirm, Action};
use crate::app::{action_error_message, App, Command, NavRequest};
use crate::clipboard::{self, CopyVia};
use crate::gfx::CellPx;
use crate::git::{Git, GitError};
use crate::input::{self, menu_mouse, translate_key, ClickKind, ClickTracker, Hit, InputMode};
use crate::menus::{confirm_dialog, input_dialog, with_value};
use crate::remote_url::{open_in_browser, to_https};
use crate::terminal::GfxFrame;
use crate::ui::dialog::{Dialog, DialogOutcome, DialogState};
use crate::ui::layout::{
    display_width, fit_to_width, truncate_to_width, MIN_HEIGHT, MIN_WIDTH, TOO_SMALL,
};
use crate::ui::menu::{Menu, MenuId, MenuItem};
use crate::ui::theme::{muted, selected_bg, ColorDepth};
use crate::watch::WatchEvent;

/// How often the list re-reads every repository's state while it is shown.
pub const MANYGIT_REFRESH_SECS: u64 = 15;

/// Whether the list is due for a status refresh.
pub fn should_refresh(last: Instant, now: Instant) -> bool {
    now.checked_duration_since(last)
        .is_some_and(|d| d >= Duration::from_secs(MANYGIT_REFRESH_SECS))
}

/// Widest the name column of the list gets, in cells.
const NAME_MAX: usize = 24;

const LIST_HELP: &str = "\
j/k, Up/Down   Move
PgUp/PgDn      Page
g/G, Home/End  First/last
Enter          Open the graph
m              Repository menu (right click)
F              Fetch all repositories
R, F5          Refresh
?              This help
q, Esc         Quit";

fn item(label: &str, enabled: bool, id: MenuId) -> MenuItem {
    MenuItem {
        label: label.to_string(),
        enabled,
        danger: false,
        id,
    }
}

/// The repository menu, without "Copy path" (the list adds it, as it needs the path).
/// `remotes` are the remote URLs, the default remote first. Pull/Push/Fetch come first,
/// led by what the ahead/behind state asks for: Behind → Pull, Ahead → Push, else Fetch.
pub fn repo_menu(status: &RepoStatus, remotes: &[String]) -> Vec<MenuItem> {
    let has_remote = !remotes.is_empty();
    let has_upstream = matches!(
        status.sync,
        SyncState::InSync | SyncState::Ahead(_) | SyncState::Behind(_) | SyncState::Diverged(..)
    );
    let pull = item("Pull", has_upstream, MenuId::Do(Action::Pull));
    let push = item(
        "Push",
        has_remote && status.branch.is_some(),
        MenuId::Do(Action::PushCurrent),
    );
    let fetch = item("Fetch", has_remote, MenuId::Do(Action::Fetch));
    let mut items = match status.sync {
        SyncState::Behind(_) => vec![pull, push, fetch],
        SyncState::Ahead(_) => vec![push, pull, fetch],
        _ => vec![fetch, pull, push],
    };
    let page = remotes.first().and_then(|url| to_https(url));
    items.extend([
        item(
            "Commit",
            status.dirty,
            MenuId::Do(Action::Commit {
                message: String::new(),
                stage_all: true,
            }),
        ),
        item("Stash", status.dirty, MenuId::Do(Action::Stash)),
        MenuItem {
            danger: true,
            ..item("Reset", status.dirty, MenuId::Do(Action::ResetWorktree))
        },
        item(
            "Open remote page",
            page.is_some(),
            MenuId::OpenRemote(page.unwrap_or_default()),
        ),
    ]);
    items
}

/// Runs `git fetch --all` (no login prompts) in each repository that has a remote,
/// one after another; repositories without one are skipped and not in the result.
pub fn fetch_all(paths: &[PathBuf]) -> Vec<(PathBuf, Result<String, GitError>)> {
    paths
        .iter()
        .filter(|p| !remote_urls(&Git::at(p.as_path())).is_empty())
        .map(|p| (p.clone(), execute(&Git::at(p.as_path()), &Action::FetchAll)))
        .collect()
}

/// Remote URLs from the repository's config, `origin` first, then in config order.
fn remote_urls(git: &Git) -> Vec<String> {
    // Exits 1 when no remote is configured.
    let Ok(out) = git.run(&["config", "--get-regexp", r"^remote\..*\.url$"]) else {
        return Vec::new();
    };
    let mut remotes: Vec<(bool, String)> = out
        .lines()
        .filter_map(|line| {
            let (key, url) = line.split_once(' ')?;
            Some((key != "remote.origin.url", url.trim().to_string()))
        })
        .collect();
    // Stable: keeps config order after origin.
    remotes.sort_by_key(|(not_origin, _)| *not_origin);
    remotes.into_iter().map(|(_, url)| url).collect()
}

type JobResult = Vec<(PathBuf, Result<String, GitError>)>;

/// A list action waiting on the dialog it opened.
enum Pending {
    Input(PathBuf, Action),
    Confirm(PathBuf, Action),
    FetchAll(Vec<PathBuf>),
}

/// A drawn line of the list.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ListLine {
    /// The folder of the entry at this index (the first of its group).
    Group(usize),
    Repo(usize),
}

pub struct Manygit {
    root: PathBuf,
    entries: Vec<RepoEntry>,
    /// Same order as `entries`.
    statuses: Vec<RepoStatus>,
    show_tags: bool,
    depth: ColorDepth,
    /// Opened graphs draw with ASCII characters.
    ascii: bool,
    nerd_font: bool,
    /// Opened graphs draw their graph column blank, for images to show over it.
    gfx_active: bool,
    /// Selected entry.
    selected: usize,
    /// First visible line; adjusted while drawing to keep the selection in view.
    offset: Cell<usize>,
    /// List height at the last draw, used as the PageUp/PageDown step.
    page_height: Cell<usize>,
    /// The last list draw showed only the "too small" message.
    too_small: Cell<bool>,
    last_refresh: Instant,
    /// A background status refresh still running; its result replaces `statuses`.
    refreshing: Option<Receiver<Vec<RepoStatus>>>,
    menu: Option<Menu>,
    dialog: Option<DialogState>,
    pending: Option<Pending>,
    job: Option<Receiver<JobResult>>,
    /// Graphs of visited repositories, by path.
    apps: HashMap<PathBuf, App>,
    /// The repository whose graph is shown; `None` shows the list.
    open: Option<PathBuf>,
    /// A reopened graph was drawn from its cache and re-reads the repository next.
    stale: bool,
    copier: fn(&str) -> io::Result<CopyVia>,
    opener: fn(&str) -> io::Result<()>,
    quit: bool,
}

impl Manygit {
    /// The list of `entries` found under `root`, with their state read once.
    pub fn new(
        root: PathBuf,
        entries: Vec<RepoEntry>,
        show_tags: bool,
        depth: ColorDepth,
    ) -> Manygit {
        let mut m = Manygit {
            root,
            entries,
            statuses: Vec::new(),
            show_tags,
            depth,
            ascii: false,
            nerd_font: false,
            gfx_active: false,
            selected: 0,
            offset: Cell::new(0),
            page_height: Cell::new(1),
            too_small: Cell::new(false),
            last_refresh: Instant::now(),
            refreshing: None,
            menu: None,
            dialog: None,
            pending: None,
            job: None,
            apps: HashMap::new(),
            open: None,
            stale: false,
            copier: clipboard::copy,
            opener: open_in_browser,
            quit: false,
        };
        m.refresh(Instant::now());
        m
    }

    pub fn apply(&mut self, cmd: Command) -> Result<(), GitError> {
        if let Some(app) = self.graph_mut() {
            app.apply(cmd)?;
            match app.take_nav_request() {
                Some(NavRequest::ToList) => self.open = None,
                Some(NavRequest::Switch(path)) => {
                    if let Some(i) = self.entries.iter().position(|e| e.path == path) {
                        self.open_repo(i);
                    }
                }
                None => {}
            }
            return Ok(());
        }
        self.apply_list(cmd);
        Ok(())
    }

    /// The shown graph, `None` while the list is shown.
    pub fn graph(&self) -> Option<&App> {
        self.apps.get(self.open.as_ref()?)
    }

    fn graph_mut(&mut self) -> Option<&mut App> {
        self.apps.get_mut(self.open.as_ref()?)
    }

    /// The repository whose graph is shown.
    pub fn current_path(&self) -> Option<&Path> {
        self.open.as_deref()
    }

    pub fn input_mode(&self) -> InputMode {
        if let Some(app) = self.graph() {
            app.input_mode()
        } else if self.dialog.is_some() {
            InputMode::Dialog
        } else if self.menu.is_some() {
            InputMode::Menu
        } else {
            InputMode::List
        }
    }

    /// Maps a key press to the command it means now.
    pub fn key_command(&self, key: KeyEvent) -> Option<Command> {
        if self.too_small() {
            return input::too_small_key(key);
        }
        let cmd = translate_key(key, self.input_mode())?;
        Some(match self.graph() {
            Some(app) => app.resolve(cmd),
            None => cmd,
        })
    }

    /// Maps a mouse event on a screen of size `area` to a command.
    pub fn mouse_command(
        &self,
        area: Rect,
        ev: MouseEvent,
        clicks: &mut ClickTracker,
        now: Instant,
    ) -> Option<Command> {
        if let Some(app) = self.graph() {
            return input::mouse_command(app, area, ev, clicks, now);
        }
        if self.too_small.get() {
            return None;
        }
        let pos = (ev.column, ev.row);
        match self.input_mode() {
            InputMode::Menu => menu_mouse(self.menu.as_ref()?, area, ev),
            InputMode::List => match ev.kind {
                MouseEventKind::ScrollUp => Some(Command::Up),
                MouseEventKind::ScrollDown => Some(Command::Down),
                MouseEventKind::Down(MouseButton::Left) => {
                    let kind = clicks.click(pos, now);
                    let i = self.repo_at(area, pos)?;
                    Some(match kind {
                        ClickKind::Single => Command::SelectRow(i),
                        ClickKind::Double => Command::Activate,
                    })
                }
                MouseEventKind::Down(MouseButton::Right) => {
                    let i = self.repo_at(area, pos)?;
                    Some(Command::OpenMenuAt {
                        pos,
                        hit: Hit::Row(i),
                    })
                }
                _ => None,
            },
            _ => None,
        }
    }

    /// Whether the shown screen was last drawn too small; input is then ignored but quit.
    pub fn too_small(&self) -> bool {
        self.graph().map_or(self.too_small.get(), App::too_small)
    }

    pub fn menu(&self) -> Option<&Menu> {
        match self.graph() {
            Some(app) => app.menu(),
            None => self.menu.as_ref(),
        }
    }

    pub fn dialog(&self) -> Option<&DialogState> {
        match self.graph() {
            Some(app) => app.dialog(),
            None => self.dialog.as_ref(),
        }
    }

    pub fn should_quit(&self) -> bool {
        self.quit || self.graph().is_some_and(App::should_quit)
    }

    pub fn job_running(&self) -> bool {
        self.job.is_some() || self.graph().is_some_and(App::job_running)
    }

    /// Replaces the clipboard writer (tests use one that cannot touch the real clipboard).
    pub fn set_copier(&mut self, copier: fn(&str) -> io::Result<CopyVia>) {
        self.copier = copier;
    }

    /// Opened graphs draw with ASCII characters instead of box drawing.
    pub fn set_ascii(&mut self, ascii: bool) {
        self.ascii = ascii;
    }

    /// Opened graphs end ref badges in round caps.
    pub fn set_nerd_font(&mut self, on: bool) {
        self.nerd_font = on;
    }

    /// Opened graphs draw their graph column blank (see `App::set_gfx_active`).
    pub fn set_gfx_active(&mut self, on: bool) {
        self.gfx_active = on;
        for app in self.apps.values_mut() {
            app.set_gfx_active(on);
        }
    }

    /// The shown graph's row images; the list has none.
    pub fn gfx_frame(&self, area: Rect, cell: CellPx) -> Option<GfxFrame> {
        self.graph()?.gfx_frame(area, cell)
    }

    /// Passes a filesystem change to the shown graph; the list ignores it.
    pub fn on_watch(&mut self, ev: WatchEvent) {
        if let Some(app) = self.graph_mut() {
            app.on_watch(ev);
        }
    }

    /// Replaces the browser opener (tests use one that opens nothing).
    pub fn set_opener(&mut self, opener: fn(&str) -> io::Result<()>) {
        self.opener = opener;
    }

    /// While the list is shown: takes a finished background refresh, or starts one on a
    /// thread once the refresh interval has passed. A tick is skipped while a refresh is
    /// still running. Returns whether new statuses arrived.
    pub fn tick(&mut self, now: Instant) -> bool {
        if self.open.is_some() {
            return false;
        }
        if let Some(rx) = &self.refreshing {
            return match rx.try_recv() {
                Ok(statuses) => {
                    self.statuses = statuses;
                    self.refreshing = None;
                    true
                }
                Err(TryRecvError::Empty) => false,
                Err(TryRecvError::Disconnected) => {
                    self.refreshing = None;
                    false
                }
            };
        }
        if should_refresh(self.last_refresh, now) {
            let paths: Vec<PathBuf> = self.entries.iter().map(|e| e.path.clone()).collect();
            let show_tags = self.show_tags;
            let (tx, rx) = mpsc::channel();
            thread::spawn(move || {
                let statuses = paths.iter().map(|p| repo_status(p, show_tags)).collect();
                // The receiver is gone if a newer refresh replaced it.
                let _ = tx.send(statuses);
            });
            self.refreshing = Some(rx);
            self.last_refresh = now;
        }
        false
    }

    /// Whether a background status refresh is running.
    pub fn refreshing(&self) -> bool {
        self.refreshing.is_some()
    }

    /// Takes finished work: a list action's result (refreshing the list, or showing
    /// git's message), and the shown graph's action. Returns whether anything finished.
    pub fn poll_job(&mut self) -> bool {
        let graph_done = self.graph_mut().is_some_and(App::poll_job);
        let Some(rx) = &self.job else {
            return graph_done;
        };
        let results = match rx.try_recv() {
            Ok(results) => results,
            Err(TryRecvError::Empty) => return graph_done,
            Err(TryRecvError::Disconnected) => vec![(
                PathBuf::new(),
                Err(GitError::Parse(
                    "the git action stopped without a result".to_string(),
                )),
            )],
        };
        self.job = None;
        self.refresh(Instant::now());
        let several = results.len() > 1;
        let failures: Vec<String> = results
            .iter()
            .filter_map(|(path, r)| {
                let e = r.as_ref().err()?;
                let message = action_error_message(e);
                Some(if several {
                    let name = self.entries.iter().find(|e| &e.path == path);
                    let name = name.map_or("?", |e| e.name.as_str());
                    format!("{name}: {}", message.lines().next().unwrap_or_default())
                } else {
                    message
                })
            })
            .collect();
        if !failures.is_empty() {
            self.open_dialog(Dialog::Error {
                title: "Git error".to_string(),
                message: failures.join("\n"),
            });
        }
        true
    }

    /// After drawing: a reopened graph re-reads the repository, otherwise the shown
    /// graph reads a detail that is still loading. Returns whether anything loaded.
    pub fn load_pending(&mut self) -> Result<bool, GitError> {
        let stale = std::mem::take(&mut self.stale);
        let Some(app) = self.graph_mut() else {
            return Ok(false);
        };
        if stale {
            app.on_watch(WatchEvent::RefsChanged);
            return Ok(true);
        }
        app.load_pending()
    }

    pub fn draw(&self, frame: &mut Frame) {
        if let Some(app) = self.graph() {
            app.draw(frame);
            return;
        }
        let area = frame.area();
        self.too_small
            .set(area.width < MIN_WIDTH || area.height < MIN_HEIGHT);
        if self.too_small.get() {
            let msg = truncate_to_width(TOO_SMALL, area.width as usize);
            frame.render_widget(Line::raw(msg), area);
            return;
        }
        let (header, body) = split(area);
        self.draw_header(frame, header);
        self.draw_list(frame, body);
        if let Some(menu) = &self.menu {
            menu.draw(frame, area);
        }
        if let Some(dialog) = &self.dialog {
            dialog.draw(frame, area);
        }
    }

    fn apply_list(&mut self, cmd: Command) {
        if let Some(state) = &mut self.dialog {
            if cmd == Command::Quit {
                self.quit = true;
            } else if let Some(outcome) = state.handle(cmd) {
                self.dialog = None;
                match (outcome, self.pending.take()) {
                    (DialogOutcome::Accepted(value), Some(Pending::Input(path, a))) => {
                        self.begin(path, with_value(a, value.unwrap_or_default()))
                    }
                    (DialogOutcome::Accepted(_), Some(Pending::Confirm(path, a))) => {
                        self.start(vec![path], Some(a))
                    }
                    (DialogOutcome::Accepted(_), Some(Pending::FetchAll(paths))) => {
                        self.start(paths, None)
                    }
                    _ => {}
                }
            }
            return;
        }
        let last = self.entries.len().saturating_sub(1);
        let page = self.page_height.get().max(1);
        match cmd {
            Command::Quit | Command::Back => self.quit = true,
            Command::Up => self.selected = self.selected.saturating_sub(1),
            Command::Down => self.selected = (self.selected + 1).min(last),
            Command::PageUp => self.selected = self.selected.saturating_sub(page),
            Command::PageDown => self.selected = (self.selected + page).min(last),
            Command::Top => self.selected = 0,
            Command::Bottom => self.selected = last,
            Command::SelectRow(i) if i < self.entries.len() => self.selected = i,
            Command::Activate | Command::OpenDetail if !self.entries.is_empty() => {
                self.open_repo(self.selected)
            }
            Command::OpenMenu => self.open_menu(None),
            Command::OpenMenuAt {
                pos,
                hit: Hit::Row(i),
            } if i < self.entries.len() => {
                self.selected = i;
                self.open_menu(Some(pos));
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
            Command::MenuSelect => self.menu_select(),
            Command::MenuPick(i) => {
                if let Some(menu) = &mut self.menu {
                    if menu.items.get(i).is_some_and(|item| item.enabled) {
                        menu.selected = Some(i);
                        self.menu_select();
                    }
                }
            }
            Command::MenuClose => self.menu = None,
            Command::Refresh => self.refresh(Instant::now()),
            Command::FetchAllRepos => self.ask_fetch_all(),
            Command::Help => self.open_dialog(Dialog::Info {
                title: "Keys".to_string(),
                body: LIST_HELP.to_string(),
            }),
            _ => {}
        }
    }

    /// Shows the graph of entry `i`: the cached one (re-read after it is drawn), else
    /// a newly loaded one. A repository that cannot be read shows git's message.
    fn open_repo(&mut self, i: usize) {
        self.selected = i;
        let entry = &self.entries[i];
        let path = entry.path.clone();
        if self.apps.contains_key(&path) {
            self.stale = true;
        } else {
            match App::new(Git::at(&path), self.depth) {
                Ok(mut app) => {
                    let repos = self
                        .entries
                        .iter()
                        .map(|e| (e.name.clone(), e.path.clone()))
                        .collect();
                    app.embed(entry.name.clone(), repos);
                    app.set_copier(self.copier);
                    app.set_ascii(self.ascii);
                    app.set_nerd_font(self.nerd_font);
                    app.set_gfx_active(self.gfx_active);
                    self.apps.insert(path.clone(), app);
                }
                Err(e) => {
                    let title = format!("Cannot open {}", entry.name);
                    self.open = None;
                    self.open_dialog(Dialog::Error {
                        title,
                        message: e.message(),
                    });
                    return;
                }
            }
        }
        self.open = Some(path);
    }

    fn open_menu(&mut self, anchor: Option<(u16, u16)>) {
        let Some(entry) = self.entries.get(self.selected) else {
            return;
        };
        let remotes = remote_urls(&Git::at(&entry.path));
        let mut items = repo_menu(&self.statuses[self.selected], &remotes);
        items.push(item(
            "Copy path",
            true,
            MenuId::Copy(entry.path.to_string_lossy().into_owned()),
        ));
        self.menu = Some(Menu::new(items, anchor, Hit::Row(self.selected)));
    }

    fn menu_select(&mut self) {
        let chosen = self.menu.as_ref().and_then(Menu::chosen);
        let Some(id) = chosen.map(|item| item.id.clone()) else {
            return;
        };
        self.menu = None;
        let path = self.entries[self.selected].path.clone();
        match id {
            MenuId::Do(a) => self.begin(path, a),
            MenuId::Copy(text) => {
                if (self.copier)(&text).is_err() {
                    self.open_dialog(Dialog::Error {
                        title: "Copy failed".to_string(),
                        message: format!("Could not reach the clipboard. The value is:\n{text}"),
                    });
                }
            }
            MenuId::OpenRemote(url) => {
                if (self.opener)(&url).is_err() {
                    self.open_dialog(Dialog::Error {
                        title: "Open failed".to_string(),
                        message: format!("Could not open a browser. The page is:\n{url}"),
                    });
                }
            }
            MenuId::SwitchRepo(_) => {}
        }
    }

    /// Asks for a missing value, then confirmation when the action needs it, then runs it.
    fn begin(&mut self, path: PathBuf, a: Action) {
        if let Some(dialog) = input_dialog(&a) {
            self.open_dialog(dialog);
            self.pending = Some(Pending::Input(path, a));
        } else if needs_confirm(&a) {
            self.open_dialog(confirm_dialog(&a));
            self.pending = Some(Pending::Confirm(path, a));
        } else {
            self.start(vec![path], Some(a));
        }
    }

    /// Asks before fetching every listed repository that has a remote.
    fn ask_fetch_all(&mut self) {
        let paths: Vec<PathBuf> = self
            .entries
            .iter()
            .zip(&self.statuses)
            .filter(|(_, s)| s.sync != SyncState::NoRemote)
            .map(|(e, _)| e.path.clone())
            .collect();
        if paths.is_empty() {
            self.open_dialog(Dialog::Info {
                title: "Fetch all".to_string(),
                body: "No repository has a remote.".to_string(),
            });
            return;
        }
        let n = paths.len();
        self.open_dialog(Dialog::Confirm {
            title: "Fetch all".to_string(),
            body: format!(
                "Fetch all remotes of {n} {}? Repositories without a remote are skipped.",
                if n == 1 { "repository" } else { "repositories" }
            ),
            danger: false,
        });
        self.pending = Some(Pending::FetchAll(paths));
    }

    /// Runs `a` in the one repository of `paths`, or Fetch all over `paths` when `a`
    /// is `None`, on a background thread. One list action runs at a time.
    fn start(&mut self, paths: Vec<PathBuf>, a: Option<Action>) {
        if self.job.is_some() {
            self.open_dialog(Dialog::Error {
                title: "Busy".to_string(),
                message: "Another git action is still running.".to_string(),
            });
            return;
        }
        let (tx, rx) = mpsc::channel();
        thread::spawn(move || {
            let results = match a {
                Some(a) => paths
                    .into_iter()
                    .map(|p| {
                        let r = execute(&Git::at(&p), &a);
                        (p, r)
                    })
                    .collect(),
                None => fetch_all(&paths),
            };
            // The receiver may be gone if the app quit; nothing to report then.
            let _ = tx.send(results);
        });
        self.job = Some(rx);
    }

    /// Shows a modal dialog over whatever is on screen: the shown graph, or the list.
    pub fn show_dialog(&mut self, dialog: Dialog) {
        match self.graph_mut() {
            Some(app) => app.open_dialog(dialog),
            None => self.open_dialog(dialog),
        }
    }

    fn open_dialog(&mut self, dialog: Dialog) {
        self.menu = None;
        self.pending = None;
        self.dialog = Some(DialogState::new(dialog));
    }

    fn refresh(&mut self, now: Instant) {
        // A background refresh started earlier would bring older data.
        self.refreshing = None;
        self.statuses = self
            .entries
            .iter()
            .map(|e| repo_status(&e.path, self.show_tags))
            .collect();
        self.last_refresh = now;
    }

    /// Group headers and repositories, in drawing order.
    fn lines(&self) -> Vec<ListLine> {
        let mut out = Vec::new();
        for (i, e) in self.entries.iter().enumerate() {
            if i == 0 || self.entries[i - 1].group != e.group {
                out.push(ListLine::Group(i));
            }
            out.push(ListLine::Repo(i));
        }
        out
    }

    /// The entry drawn at `pos` on a screen of size `area`.
    fn repo_at(&self, area: Rect, pos: (u16, u16)) -> Option<usize> {
        let (_, body) = split(area);
        if !body.contains(ratatui::layout::Position::new(pos.0, pos.1)) {
            return None;
        }
        let index = self.offset.get() + (pos.1 - body.y) as usize;
        match self.lines().get(index)? {
            ListLine::Repo(i) => Some(*i),
            ListLine::Group(_) => None,
        }
    }

    fn group_label(&self, group: &Path) -> String {
        match group.strip_prefix(&self.root) {
            Ok(rel) if !rel.as_os_str().is_empty() => rel.display().to_string(),
            _ => group.file_name().map_or_else(
                || group.display().to_string(),
                |n| n.to_string_lossy().into_owned(),
            ),
        }
    }

    fn draw_header(&self, frame: &mut Frame, area: Rect) {
        let width = area.width as usize;
        let n = self.entries.len();
        let title = format!("Manygit  {n} repo{}", if n == 1 { "" } else { "s" });
        let title = truncate_to_width(&title, width);
        let used = display_width(&title);
        let mut spans = vec![Span::styled(
            title,
            Style::new().add_modifier(Modifier::BOLD),
        )];
        if self.job.is_some() {
            let text = truncate_to_width("  Running…", width - used);
            spans.push(Span::styled(text, Style::new().fg(Color::Yellow)));
        }
        frame.render_widget(Line::from(spans), area);
    }

    fn draw_list(&self, frame: &mut Frame, area: Rect) {
        let height = area.height as usize;
        self.page_height.set(height);
        if self.entries.is_empty() {
            frame.render_widget(Line::raw("No repositories found"), area);
            return;
        }
        let lines = self.lines();
        let sel_line = lines
            .iter()
            .position(|l| *l == ListLine::Repo(self.selected))
            .unwrap_or(0);
        // Keep the selection in view, with its group header when it is the first.
        let mut offset = self.offset.get().min(lines.len().saturating_sub(height));
        let top = match sel_line.checked_sub(1).map(|p| lines[p]) {
            Some(ListLine::Group(_)) => sel_line - 1,
            _ => sel_line,
        };
        if top < offset {
            offset = top;
        } else if height > 0 && sel_line >= offset + height {
            offset = sel_line + 1 - height;
        }
        self.offset.set(offset);

        let name_w = self
            .entries
            .iter()
            .map(|e| display_width(&e.name))
            .max()
            .unwrap_or(0)
            .min(NAME_MAX);
        for (y, line) in (area.y..area.bottom()).zip(lines.iter().skip(offset)) {
            let row = Rect::new(area.x, y, area.width, 1);
            let line = match *line {
                ListLine::Group(i) => Line::styled(
                    truncate_to_width(
                        &self.group_label(&self.entries[i].group),
                        row.width as usize,
                    ),
                    Style::new()
                        .fg(muted(self.depth))
                        .add_modifier(Modifier::BOLD),
                ),
                ListLine::Repo(i) => self.repo_line(i, name_w, row.width as usize),
            };
            frame.render_widget(line, row);
        }
    }

    /// `  name  branch * ↑1 ↓2  tag`, cut to `width`, highlighted when selected.
    fn repo_line(&self, i: usize, name_w: usize, width: usize) -> Line<'static> {
        let status = &self.statuses[i];
        let mut parts: Vec<(String, Style)> = vec![
            (
                format!("  {}  ", fit_to_width(&self.entries[i].name, name_w)),
                Style::new().add_modifier(Modifier::BOLD),
            ),
            (
                status
                    .branch
                    .clone()
                    .unwrap_or_else(|| "(detached)".to_string()),
                Style::new(),
            ),
        ];
        if status.dirty {
            parts.push((" *".to_string(), Style::new().fg(Color::Red)));
        }
        let green = Style::new().fg(Color::Green);
        let yellow = Style::new().fg(Color::Yellow);
        let quiet = Style::new().fg(muted(self.depth));
        match status.sync {
            SyncState::InSync => {}
            SyncState::Ahead(a) => parts.push((format!(" ↑{a}"), green)),
            SyncState::Behind(b) => parts.push((format!(" ↓{b}"), yellow)),
            SyncState::Diverged(a, b) => {
                parts.push((format!(" ↑{a}"), green));
                parts.push((format!(" ↓{b}"), yellow));
            }
            SyncState::NotPushed => parts.push((" not pushed".to_string(), quiet)),
            SyncState::NoRemote => parts.push((" no remote".to_string(), quiet)),
        }
        if let Some(tag) = &status.latest_tag {
            parts.push((format!("  {tag}"), quiet));
        }

        let selected = i == self.selected;
        let mut spans = Vec::new();
        let mut used = 0;
        for (text, mut style) in parts {
            let text = truncate_to_width(&text, width - used);
            used += display_width(&text);
            if selected {
                style = style.bg(selected_bg(self.depth));
            }
            spans.push(Span::styled(text, style));
        }
        if selected {
            spans.push(Span::styled(
                " ".repeat(width - used),
                Style::new().bg(selected_bg(self.depth)),
            ));
        }
        Line::from(spans)
    }
}

/// Header line and list body.
fn split(area: Rect) -> (Rect, Rect) {
    let header = Rect::new(area.x, area.y, area.width, 1.min(area.height));
    let body = Rect::new(
        area.x,
        area.y + header.height,
        area.width,
        area.height - header.height,
    );
    (header, body)
}
