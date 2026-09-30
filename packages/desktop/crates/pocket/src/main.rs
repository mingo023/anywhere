mod capture;
mod changes;
mod diff;
mod explore;
mod forms;
mod inbox;
mod mermaid;
mod overlay;
mod sessions;
mod status;
mod syntax;
mod termview;
mod view;

use agents::{Agents, Event, Outbox, Summary};
use daemon::{Daemon, Msg};
use futures::StreamExt;
use git::Repo;
use gpui_kit::component::input::{EditorState, InputEvent, InputState, TextDecorationCollection, TextareaState};
use gpui_kit::component::text::TextViewState;
use gpui_kit::component::Root;
use gpui_kit::*;
use serde_json::json;
use sessions::Sessions;
use status::{Card, Status};
use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;
use store::Store;
use workspace::{Tab, Workspace};

actions!(desktop, [OpenPalette, GoToFile, OpenSession, StartSession, NextWaiting, ToggleRail, ToggleFocus, NewWorktree, ProjectSettings, NewTab]);

#[derive(Clone, Copy, PartialEq)]
pub enum Screen {
    Sessions,
    Inbox,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Side {
    Sessions,
    Explorer,
    Changes,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Layout {
    Sidebars,
    Compact,
    Focus,
}

/// A sidebar column whose right edge the user drags.
#[derive(Clone, Copy, PartialEq)]
pub enum Column {
    Projects,
    Sessions,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Overlay {
    Palette,
    NewSession,
    AddRepo,
    More,
    Confirm,
}

#[derive(Clone)]
pub enum Confirm {
    RemoveProject(String),
    DeleteWorktree { project: String, tree: String, branch: String, dirty: usize },
    Discard(Vec<String>),
}

/// The sidebar row whose `⋯` menu is open.
#[derive(Clone, PartialEq)]
pub enum RowMenu {
    Project(String),
    Tree { project: String, tree: String },
}

/// A comment sent to an agent about lines of a file, kept so the diff can show it until resolved.
pub struct Comment {
    pub path: String,
    pub lines: (usize, usize),
    pub old_side: bool,
    pub label: String,
    pub text: String,
    pub at: i64,
}

enum Intent {
    Tab(String),
    Split(String, bool),
    Setup(String),
}

pub struct Desktop {
    daemon: Daemon,
    outbox: Outbox,
    viewing: Option<Vec<String>>,
    statuses: HashMap<String, Status>,
    alerted: HashSet<String>,
    sessions: Sessions,
    agents: Agents,
    store: Store,
    project: Option<String>,
    screen: Screen,
    side: Side,
    layout: Layout,
    /// Indexed by `Column`; `None` keeps the design's width.
    widths: [Option<f32>; 2],
    panel: bool,
    tab_menu: bool,
    session: Option<String>,
    workspaces: HashMap<String, Workspace>,
    intents: VecDeque<Intent>,
    closed: HashSet<String>,
    repos: HashMap<String, Repo>,
    diff_file: Option<String>,
    diff: Vec<git::Line>,
    diff_rows: Vec<diff::Row>,
    diff_list: ListState,
    diff_split: bool,
    diff_hl: Vec<syntax::Spans>,
    diff_open: HashSet<usize>,
    diff_source: (String, String),
    diff_syntax: (Vec<syntax::Spans>, Vec<syntax::Spans>),
    selection: Option<(usize, usize)>,
    dragging: bool,
    composing: bool,
    comment_input: Entity<TextareaState>,
    comment_target: Option<String>,
    target_menu: bool,
    commit_input: Entity<TextareaState>,
    /// What the commit button says while git works.
    busy: Option<&'static str>,
    writing: bool,
    commit_error: Option<String>,
    commit_menu: bool,
    changes_menu: bool,
    changes_tree: bool,
    changes_folded: HashSet<String>,
    changes_scroll: UniformListScrollHandle,
    initials: String,
    tree: HashMap<PathBuf, Vec<(bool, PathBuf)>>,
    git_run: u64,
    git_done: u64,
    inbox: usize,
    error: Option<String>,
    root: FocusHandle,
    term_focus: FocusHandle,
    inbox_focus: FocusHandle,
    focused: Option<String>,
    filter: Entity<InputState>,
    session_search: Entity<InputState>,
    sized: HashMap<String, (u16, u16)>,
    marked: Option<usize>,
    overlay: Option<Overlay>,
    palette_ix: usize,
    palette_all: bool,
    palette_files: Vec<String>,
    worktrees: HashMap<String, Vec<git::Worktree>>,
    worktree: Option<String>,
    /// Terminal → worktree while the terminal runs the worktree's setup before its agent.
    setups: HashMap<String, String>,
    confirm: Option<Confirm>,
    row_menu: Option<RowMenu>,
    file: Option<String>,
    file_preview: Option<explore::Preview>,
    file_diff: Vec<git::Line>,
    code: Entity<EditorState>,
    code_marks: TextDecorationCollection,
    code_stale: bool,
    code_file: Option<String>,
    code_text: SharedString,
    md: Entity<TextViewState>,
    diagrams: Entity<mermaid::Diagrams>,
    md_source: bool,
    /// The copied path and the timer that turns its check back; a new copy replaces, and so cancels, the old one.
    path_copied: Option<(String, Task<()>)>,
    comments: Vec<Comment>,
    viewed: HashSet<String>,
    new_form: forms::NewForm,
    repo_form: forms::RepoForm,
    capturing: bool,
    _subs: Vec<Subscription>,
}

fn under(cwd: &str, project: &str) -> bool {
    cwd == project || cwd.strip_prefix(project).is_some_and(|rest| rest.starts_with('/'))
}

impl Desktop {
    fn new(daemon: Daemon, outbox: Outbox, store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions, files and actions…"));
        let session_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions…"));
        let comment_input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Ask the agent about these lines…").rows(3));
        let commit_input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Message (⌘↩ to commit)").auto_grow(1, 8));
        let code = cx.new(|cx| EditorState::new(window, cx).line_number(true).searchable(true).soft_wrap(false));
        let code_marks = code.update(cx, |s, cx| s.create_decorations_collection(Vec::new(), cx));
        let md = cx.new(|cx| TextViewState::markdown("", cx));
        let diagrams = cx.new(|_| mermaid::Diagrams::new(&md));
        let (new_form, new_subs) = forms::NewForm::new(window, cx);
        let (repo_form, repo_subs) = forms::RepoForm::new(window, cx);
        let mut _subs = vec![
            cx.subscribe(&filter, |this, _, ev: &InputEvent, cx| {
                if let InputEvent::Change = ev {
                    this.palette_ix = 0;
                }
                cx.notify()
            }),
            cx.subscribe(&session_search, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe_in(&comment_input, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { secondary: true, .. } => this.submit_comment(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
            cx.subscribe_in(&commit_input, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { secondary: true, .. } => this.commit(changes::CommitKind::Commit, window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
            // The setting changes in System Settings, so the window coming back is when it may have.
            cx.observe_window_activation(window, |_, _, cx| follow_reduce_motion(cx)),
        ];
        _subs.extend(new_subs);
        _subs.extend(repo_subs);
        Self {
            daemon,
            outbox,
            viewing: None,
            statuses: HashMap::new(),
            alerted: HashSet::new(),
            sessions: Sessions::default(),
            agents: Agents::default(),
            project: store.projects.first().cloned(),
            store,
            screen: Screen::Sessions,
            side: Side::Sessions,
            layout: Layout::Sidebars,
            widths: [None; 2],
            panel: false,
            tab_menu: false,
            session: None,
            workspaces: HashMap::new(),
            intents: VecDeque::new(),
            closed: HashSet::new(),
            repos: HashMap::new(),
            diff_file: None,
            diff: Vec::new(),
            diff_rows: Vec::new(),
            diff_list: ListState::new(0, ListAlignment::Top, px(400.)),
            diff_split: false,
            diff_hl: Vec::new(),
            diff_open: HashSet::new(),
            diff_source: Default::default(),
            diff_syntax: Default::default(),
            selection: None,
            dragging: false,
            composing: false,
            comment_input,
            comment_target: None,
            target_menu: false,
            commit_input,
            busy: None,
            writing: false,
            commit_error: None,
            commit_menu: false,
            changes_menu: false,
            changes_tree: false,
            changes_folded: HashSet::new(),
            changes_scroll: UniformListScrollHandle::new(),
            initials: String::new(),
            tree: HashMap::new(),
            git_run: 0,
            git_done: 0,
            inbox: 0,
            error: None,
            root: cx.focus_handle(),
            term_focus: cx.focus_handle(),
            inbox_focus: cx.focus_handle(),
            focused: None,
            filter,
            session_search,
            sized: HashMap::new(),
            marked: None,
            overlay: None,
            palette_ix: 0,
            palette_all: true,
            palette_files: Vec::new(),
            worktrees: HashMap::new(),
            worktree: None,
            setups: HashMap::new(),
            confirm: None,
            row_menu: None,
            file: None,
            file_preview: None,
            file_diff: Vec::new(),
            code,
            code_marks,
            code_stale: false,
            code_file: None,
            code_text: SharedString::default(),
            md,
            diagrams,
            md_source: false,
            path_copied: None,
            comments: Vec::new(),
            viewed: HashSet::new(),
            new_form,
            repo_form,
            capturing: false,
            _subs,
        }
    }

    fn on_agents(&mut self, ev: Event, cx: &mut Context<Self>) {
        if let (Event::Connected, Some(ids)) = (&ev, &self.viewing) {
            self.outbox.view(ids);
        }
        if let Event::Agents(list) = &ev {
            // Notifications outlive the app, so any listed agent may still have one from before a restart.
            self.alerted.extend(list.iter().map(|a| a.id.clone()));
        }
        self.agents.apply(ev);
        let agents = &self.agents;
        self.setups.retain(|term, _| !agents.list.iter().any(|a| &a.terminal_id == term));
        if self.screen == Screen::Inbox {
            (self.inbox, self.focused) = inbox::reselect(&inbox::notes(&self.agents), self.focused.as_deref(), self.inbox);
        }
        self.sync_alerts(cx);
        cx.notify();
    }

    fn sync_alerts(&mut self, cx: &mut App) {
        let now: HashMap<String, Status> = self.agents.list.iter().filter_map(|a| Some((a.id.clone(), Status::of(a)?))).collect();
        let (show, dismiss) = status::alerts(&self.statuses, &now, &self.alerted, self.viewing.as_deref().unwrap_or_default());
        for id in dismiss {
            cx.dismiss_system_notification(&id);
            self.alerted.remove(&id);
        }
        for id in show.into_iter().filter(|_| !self.capturing) {
            let Some(a) = self.agents.get(&id) else { continue };
            let title = if a.title.is_empty() { theme::provider_name(&a.provider).to_string() } else { a.title.clone() };
            cx.show_system_notification(SystemNotification { tag: id.clone().into(), title: title.into(), body: now[&id].label().into(), actions: Vec::new() });
            self.alerted.insert(id);
        }
        self.statuses = now;
    }

    /// Shows an agent's session: its worktree, the tab holding its terminal, and that pane focused.
    pub fn focus_agent(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(term) = self.agents.get(id).map(|a| a.terminal_id.clone()) else { return };
        let Some(cwd) = self.sessions.get(&term).map(|s| s.info.cwd.clone()) else { return };
        let Some(tree) = self.tree_of(&cwd) else { return };
        self.show_tree(&cwd, &tree);
        self.side = Side::Sessions;
        self.session = Some(id.to_string());
        let w = self.workspace(&tree);
        if let Some(i) = w.tab_of(&term) {
            w.active = i;
        }
        self.focus_pane(term, window, cx);
        self.refresh_git(cx);
    }

    fn show_tree(&mut self, cwd: &str, tree: &str) {
        if let Some(p) = self.project_of(cwd, &self.projects()).cloned() {
            self.project = Some(p);
        }
        self.screen = Screen::Sessions;
        self.worktree = Some(tree.to_string());
    }

    /// The terminals on screen: the worktree's active tab, unless a preview covers it.
    fn visible_panes(&mut self) -> Vec<String> {
        let preview = (self.side == Side::Changes && self.diff_file.is_some()) || (self.side == Side::Explorer && self.file.is_some());
        let Some(tree) = self.cwd().filter(|_| self.screen == Screen::Sessions && !preview) else { return Vec::new() };
        match self.workspace(&tree).active() {
            Some(Tab::Term(rows)) => rows.concat(),
            _ => Vec::new(),
        }
    }

    fn sync_view(&mut self, window: &Window, cx: &mut App) {
        let panes = if window.is_window_active() { self.visible_panes() } else { Vec::new() };
        let ids = status::view_set(&panes, &self.agents.list);
        if self.viewing.as_ref() != Some(&ids) {
            self.outbox.view(&ids);
            self.viewing = Some(ids);
            self.sync_alerts(cx);
        }
    }

    fn on_msg(&mut self, m: Msg, window: &mut Window, cx: &mut Context<Self>) {
        match m.ev.as_str() {
            "terminals" => {
                let items = m.items.into_iter().filter(|i| !self.closed.contains(&i.id)).collect();
                for id in self.sessions.sync(items) {
                    self.daemon.send(json!({"op": "attach", "id": id}));
                }
                let sessions = &self.sessions;
                self.setups.retain(|id, _| sessions.get(id).is_some());
                if self.project.is_none() {
                    self.project = self.projects().into_iter().next();
                }
            }
            "spawned" => {
                self.error = None;
                match self.intents.pop_front() {
                    Some(Intent::Tab(tree)) => self.adopt(m.id.clone(), tree, None, window, cx),
                    Some(Intent::Split(tree, down)) => self.adopt(m.id.clone(), tree, Some(down), window, cx),
                    Some(Intent::Setup(tree)) => {
                        self.setups.insert(m.id.clone(), tree.clone());
                        self.adopt(m.id.clone(), tree, None, window, cx)
                    }
                    None => {}
                }
                self.daemon.send(json!({"op": "list"}));
            }
            "error" => {
                if m.id.is_empty() {
                    self.intents.pop_front();
                }
                self.error = Some(m.error);
            }
            "exit" => {
                self.sessions.apply(&m);
                self.setups.remove(&m.id);
                self.close_clean_exits(&m.id, cx);
            }
            _ => self.sessions.apply(&m),
        }
        cx.notify();
    }

    /// Closes panes whose shell exited cleanly, as Terminal.app does; a failed one stays so its error can be read.
    fn close_clean_exits(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.sessions.get(id).is_some_and(|s| s.exit == Some(0)) {
            self.close_pane(id, cx);
        }
    }

    fn adopt(&mut self, id: String, tree: String, split: Option<bool>, window: &mut Window, cx: &mut Context<Self>) {
        let w = self.workspace(&tree);
        match split {
            Some(down) => w.split(id.clone(), down),
            None => w.add_tab(id.clone()),
        }
        self.show_tree(&tree, &tree);
        self.focus_pane(id, window, cx);
    }

    pub fn projects(&self) -> Vec<String> {
        let mut out = self.store.projects.clone();
        for cwd in self.sessions.items.iter().map(|s| &s.info.cwd) {
            if self.project_of(cwd, &out).is_none() {
                out.push(cwd.clone());
            }
        }
        out
    }

    /// The project owning `cwd`: the one whose folder or worktree holds it most closely.
    pub fn project_of<'a>(&self, cwd: &str, projects: &'a [String]) -> Option<&'a String> {
        let reach = |p: &String| {
            let trees = self.worktrees.get(p).into_iter().flatten().map(|w| w.path.as_str());
            std::iter::once(p.as_str()).chain(trees).filter(|root| under(cwd, root)).map(str::len).max()
        };
        projects.iter().filter_map(|p| Some((reach(p)?, p))).max_by_key(|(n, _)| *n).map(|(_, p)| p)
    }

    pub fn worktree_of(&self, cwd: &str) -> Option<&git::Worktree> {
        self.worktrees.values().flatten().filter(|w| under(cwd, &w.path)).max_by_key(|w| w.path.len())
    }

    /// The worktree a folder belongs to: the git worktree holding it, else the project it is in.
    pub fn tree_of(&self, cwd: &str) -> Option<String> {
        self.worktree_of(cwd).map(|w| w.path.clone()).or_else(|| self.project_of(cwd, &self.projects()).cloned())
    }

    pub fn cards(&self, project: &str) -> Vec<Card> {
        let projects = self.projects();
        let mut out: Vec<Card> = self
            .agents
            .list
            .iter()
            .filter_map(|a| Some((a, self.sessions.get(&a.terminal_id)?)))
            .filter(|(_, s)| self.project_of(&s.info.cwd, &projects).is_some_and(|p| p == project))
            .map(|(a, s)| status::card(a, &s.info.cwd))
            .collect();
        out.sort_by_key(|c| std::cmp::Reverse(c.at));
        out
    }

    /// The live agent in `terminal`.
    pub fn summary(&self, terminal: &str) -> Option<&Summary> {
        self.agents.list.iter().find(|a| a.terminal_id == terminal && a.status != "closed")
    }

    pub fn cwd_of(&self, id: &str) -> Option<String> {
        self.agents.get(id).map(|a| a.cwd.clone()).or_else(|| self.sessions.get(id).map(|s| s.info.cwd.clone()))
    }

    /// The worktree on screen: the one picked, else the project's main one.
    pub fn cwd(&self) -> Option<String> {
        self.worktree.clone().or_else(|| self.tree_of(self.project.as_deref()?))
    }

    pub fn repo(&self) -> Option<&Repo> {
        self.repos.get(&self.cwd()?)
    }

    fn workspace(&mut self, tree: &str) -> &mut Workspace {
        let (mut mine, mut theirs) = (Vec::new(), Vec::new());
        for s in &self.sessions.items {
            match self.tree_of(&s.info.cwd) {
                Some(t) if t == tree => mine.push(s.info.id.clone()),
                Some(_) => theirs.push(s.info.id.clone()),
                None => {}
            }
        }
        let w = self.workspaces.entry(tree.to_string()).or_default();
        w.sync(&mine, &theirs);
        w
    }

    pub fn select_project(&mut self, p: String, cx: &mut Context<Self>) {
        self.screen = Screen::Sessions;
        if self.project.as_ref() != Some(&p) {
            self.set_project(Some(p));
            self.refresh_git(cx);
        }
        cx.notify();
    }

    fn set_project(&mut self, p: Option<String>) {
        self.project = p;
        self.session = None;
        self.worktree = None;
        self.diff_file = None;
        self.file = None;
        self.tree.clear();
    }

    /// Shows worktree `tree` of project `p`; `None` is the project's main worktree.
    pub fn select_tree(&mut self, p: String, tree: Option<String>, cx: &mut Context<Self>) {
        self.select_project(p, cx);
        if self.worktree != tree {
            self.worktree = tree;
            self.refresh_git(cx);
        }
        self.session = None;
        cx.notify();
    }

    pub fn select_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        let w = self.workspace(&tree);
        w.active = i;
        match w.active() {
            Some(Tab::Term(rows)) => {
                let pane = rows[0][0].clone();
                self.focus_pane(pane, window, cx);
            }
            Some(Tab::Changes) => self.load_diff(cx),
            None => {}
        }
        cx.notify();
    }

    pub fn focus_pane(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.focused = Some(id);
        window.focus(&self.term_focus, cx);
        cx.notify();
    }

    fn send_spawn(&mut self, op: serde_json::Value, intent: Intent, cx: &mut Context<Self>) {
        self.daemon.send(op);
        self.intents.push_back(intent);
        self.error = None;
        cx.notify();
    }


    /// Opens a login shell in the worktree's folder, as a new tab or a split of the active one.
    pub fn new_shell(&mut self, split: Option<bool>, cx: &mut Context<Self>) {
        self.run_in_tree(split, daemon::shell_op, cx);
    }

    pub fn new_agent_tab(&mut self, provider: &str, cx: &mut Context<Self>) {
        self.tab_menu = false;
        let argv = [provider.to_string()];
        self.run_in_tree(None, |cwd| daemon::agent_op(&argv, cwd), cx);
    }

    fn run_in_tree(&mut self, split: Option<bool>, op: impl FnOnce(&str) -> serde_json::Value, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        let intent = match split {
            Some(down) => Intent::Split(tree.clone(), down),
            None => Intent::Tab(tree.clone()),
        };
        self.send_spawn(op(&tree), intent, cx);
    }

    pub fn close_pane(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.sessions.get(id).is_some_and(|s| s.exit.is_none()) {
            self.daemon.send(json!({"op": "close", "id": id}));
        }
        self.sessions.remove(id);
        self.closed.insert(id.to_string());
        self.sized.remove(id);
        self.setups.remove(id);
        for w in self.workspaces.values_mut() {
            w.remove(id);
        }
        cx.notify();
    }

    /// Ends a session: a running agent goes with its terminal, an ended one only leaves the list.
    pub fn close_session(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(a) = self.agents.get(id) else { return };
        if a.status == "closed" {
            self.outbox.close(id);
        } else {
            let term = a.terminal_id.clone();
            self.close_pane(&term, cx);
        }
    }

    pub fn close_tab(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        for id in self.workspace(&tree).close_tab(i) {
            self.close_pane(&id, cx);
        }
        cx.notify();
    }

    fn project_terminals(&self, p: &str) -> Vec<String> {
        let projects = self.projects();
        self.sessions.items.iter().filter(|s| self.project_of(&s.info.cwd, &projects).is_some_and(|o| o == p)).map(|s| s.info.id.clone()).collect()
    }

    fn tree_terminals(&self, tree: &str) -> Vec<String> {
        self.sessions.items.iter().filter(|s| self.tree_of(&s.info.cwd).as_deref() == Some(tree)).map(|s| s.info.id.clone()).collect()
    }

    /// Returns whether a menu was open.
    fn close_menus(&mut self) -> bool {
        let open = self.tab_menu || self.row_menu.is_some() || self.commit_menu || self.changes_menu;
        (self.tab_menu, self.row_menu, self.commit_menu, self.changes_menu) = (false, None, false, false);
        open
    }

    fn keep_project(&mut self, p: &str, cx: &mut Context<Self>) {
        self.store.add(p);
        self.store.save();
        self.refresh_git(cx);
        cx.notify();
    }

    /// Closes the project's terminals and takes it off the sidebar; its folder is untouched.
    fn remove_project(&mut self, p: &str, cx: &mut Context<Self>) {
        for id in self.project_terminals(p) {
            self.close_pane(&id, cx);
        }
        self.store.remove(p);
        self.store.save();
        self.worktrees.remove(p);
        if self.project.as_deref() == Some(p) {
            self.set_project(None);
            if let Some(next) = self.projects().into_iter().next() {
                self.select_tree(next, None, cx);
            }
        }
        cx.notify();
    }

    fn ask_remove_project(&mut self, p: String, cx: &mut Context<Self>) {
        if self.project_terminals(&p).is_empty() {
            self.remove_project(&p, cx);
        } else {
            self.confirm = Some(Confirm::RemoveProject(p));
            self.overlay = Some(Overlay::Confirm);
        }
        cx.notify();
    }

    /// Asks first when deleting would close terminals or lose uncommitted changes.
    fn ask_delete_worktree(&mut self, project: String, tree: String, cx: &mut Context<Self>) {
        let branch = self.worktrees.get(&project).into_iter().flatten().find(|w| w.path == tree).map(|w| w.branch.clone()).unwrap_or_default();
        let dir = tree.clone();
        let task = cx.background_executor().spawn(async move { git::read(&dir).map(|r| r.files.len()) });
        cx.spawn(async move |this, cx| {
            let dirty = task.await;
            this.update(cx, |d, cx| {
                if dirty == Some(0) && d.tree_terminals(&tree).is_empty() {
                    d.delete_worktree(project, tree, cx);
                } else {
                    d.confirm = Some(Confirm::DeleteWorktree { project, tree, branch, dirty: dirty.unwrap_or(0) });
                    d.overlay = Some(Overlay::Confirm);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Closes the worktree's terminals and deletes its folder; its branch stays.
    fn delete_worktree(&mut self, project: String, tree: String, cx: &mut Context<Self>) {
        let dir = tree.clone();
        let task = cx.background_executor().spawn(async move { git::remove_worktree(&project, &dir) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                match res {
                    Err(e) => d.error = Some(e),
                    Ok(()) => {
                        for id in d.tree_terminals(&tree) {
                            d.close_pane(&id, cx);
                        }
                        d.workspaces.remove(&tree);
                        if d.worktree.as_ref() == Some(&tree) {
                            (d.worktree, d.session) = (None, None);
                        }
                    }
                }
                d.refresh_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }


    pub fn fit(&mut self, id: &str, cols: u16, rows: u16) {
        // Sessions are shared with the user's own window; a capture must not reflow them.
        if self.capturing {
            return;
        }
        if self.sized.get(id) != Some(&(cols, rows)) && self.sessions.get(id).is_some_and(|s| s.exit.is_none()) {
            self.daemon.send(json!({"op": "resize", "id": id, "cols": cols, "rows": rows}));
            self.sized.insert(id.to_string(), (cols, rows));
        }
    }

    fn git_cwds(&self) -> Vec<String> {
        let mut cwds: Vec<String> = self.project.iter().cloned().collect();
        if let Some(p) = &self.project {
            cwds.extend(self.cards(p).into_iter().map(|c| c.cwd));
            cwds.extend(self.worktrees.get(p).into_iter().flatten().map(|w| w.path.clone()));
        }
        cwds.extend(self.cwd());
        cwds.sort();
        cwds.dedup();
        cwds
    }

    pub fn refresh_git(&mut self, cx: &mut Context<Self>) {
        let cwds = self.git_cwds();
        let projects = self.store.projects.clone();
        let file = self.file.clone().map(|f| {
            let changed = self.file_status(&f).is_some();
            (f, changed)
        });
        let diff = self.cwd().zip(self.diff_file.clone());
        let shown = self.diff.clone();
        let open = self.diff_open.clone();
        let mut dirs: Vec<PathBuf> = self.tree.keys().cloned().collect();
        dirs.extend(self.explore_root().map(PathBuf::from));
        self.git_run += 1;
        let run = self.git_run;
        let task = cx.background_executor().spawn(async move {
            let repos: Vec<(String, Option<Repo>)> = cwds.into_iter().map(|c| {
                let r = git::read(&c);
                (c, r)
            }).collect();
            let diff = diff.map(|(cwd, path)| diff::read_diff(&cwd, path, open, &shown));
            let initials = repos.first().map(|(c, _)| git::user_initials(c)).unwrap_or_default();
            let tree: HashMap<PathBuf, Vec<(bool, PathBuf)>> = dirs.into_iter().map(|d| {
                let listing = view::list_dir(&d);
                (d, listing)
            }).collect();
            let worktrees: HashMap<String, Vec<git::Worktree>> = projects.into_iter().map(|p| {
                let w = git::worktrees(&p);
                (p, w)
            }).collect();
            let file = file.map(|(f, changed)| explore::load(&f, changed));
            (repos, diff, initials, tree, worktrees, file)
        });
        cx.spawn(async move |this, cx| {
            let (repos, diff, initials, tree, worktrees, file) = task.await;
            this.update(cx, |d, cx| {
                if run != d.git_run {
                    return;
                }
                d.git_done = run;
                let repos: HashMap<String, Repo> = repos.into_iter().filter_map(|(c, r)| Some((c, r?))).collect();
                let tree = explore::merge_tree(tree, &d.tree, d.explore_root().as_deref().map(std::path::Path::new));
                let mut changed = repos != d.repos || tree != d.tree || initials != d.initials || worktrees != d.worktrees;
                (d.repos, d.tree, d.initials, d.worktrees) = (repos, tree, initials, worktrees);
                if let Some((_, preview, lines)) = file.filter(|(p, _, _)| d.file.as_ref() == Some(p)) {
                    let preview = Some(preview);
                    let fresh = preview != d.file_preview || lines != d.file_diff;
                    changed |= fresh;
                    d.code_stale |= fresh;
                    (d.file_preview, d.file_diff) = (preview, lines);
                }
                if let Some(load) = diff {
                    changed |= d.apply_diff(load);
                }
                if changed {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn open_changes(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        let path = path.or_else(|| self.diff_file.clone()).or_else(|| self.repo()?.files.first().map(|f| f.path.clone()));
        if path != self.diff_file {
            self.selection = None;
            self.diff_open.clear();
            self.set_diff(Vec::new(), true);
        }
        self.diff_file = path;
        self.screen = Screen::Sessions;
        self.side = Side::Changes;
        self.load_diff(cx);
        cx.notify();
    }

    fn load_diff(&mut self, cx: &mut Context<Self>) {
        if self.diff_file.is_none() {
            self.diff_file = self.repo().and_then(|r| r.files.first()).map(|f| f.path.clone());
        }
        let Some((cwd, path)) = self.cwd().zip(self.diff_file.clone()) else { return };
        let (open, shown) = (self.diff_open.clone(), self.diff.clone());
        let task = cx.background_executor().spawn(async move { diff::read_diff(&cwd, path, open, &shown) });
        cx.spawn(async move |this, cx| {
            let load = task.await;
            this.update(cx, |d, cx| {
                if d.apply_diff(load) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Starts a comment on line `i` of the diff, or with `extend` stretches the open one to it.
    pub fn select_line(&mut self, i: usize, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        let kept = self.selection.map(|(a, _)| a).filter(|&a| extend && self.same_hunk(a, i));
        if kept.is_none() {
            self.comment_input.update(cx, |s, cx| s.set_value("", window, cx));
            self.composing = false;
        }
        self.selection = Some((kept.unwrap_or(i), i));
        self.dragging = true;
        self.layout_diff(false);
        cx.notify();
    }

    pub fn drag_to(&mut self, i: usize, pressed: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((anchor, end)) = self.selection.filter(|_| self.dragging) else { return };
        if !pressed {
            return self.end_drag(window, cx);
        }
        if end != i && self.same_hunk(anchor, i) {
            self.selection = Some((anchor, i));
            cx.notify();
        }
    }

    pub fn end_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.dragging) {
            return;
        }
        if self.selection.is_some_and(|(a, b)| a != b) {
            self.composing = true;
        }
        self.layout_diff(false);
        if self.composing {
            self.comment_input.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    pub fn open_comment(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if !self.picked(i) {
            self.selection = Some((i, i));
            self.comment_input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.composing = true;
        self.layout_diff(false);
        self.comment_input.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub fn cancel_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.selection = None;
        self.composing = false;
        self.target_menu = false;
        self.layout_diff(false);
        self.comment_input.update(cx, |s, cx| s.set_value("", window, cx));
        window.focus(&self.root, cx);
        cx.notify();
    }

    pub fn submit_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.comment_input.read(cx).value().trim().to_string();
        let (Some(target), Some(path), Some(lines)) = (self.comment_target(), self.diff_file.clone(), self.selection_label()) else { return };
        if text.is_empty() {
            return;
        }
        let Some(terminal) = self.agents.get(&target).map(|a| a.terminal_id.clone()) else { return };
        self.daemon.send(json!({"op": "prompt", "id": terminal, "text": format!("{path} {}: {text}", lines.to_lowercase())}));
        if let Some(comment) = self.new_comment(path, lines, text) {
            self.comments.push(comment);
        }
        self.cancel_comment(window, cx);
    }

    /// The session a comment goes to: the one picked, else the open one, else the project's newest.
    pub fn comment_target(&self) -> Option<String> {
        let cards = self.cards(self.project.as_deref()?);
        let live = |id: &String| cards.iter().any(|c| &c.id == id);
        self.comment_target.clone().filter(live).or_else(|| self.session.clone().filter(live)).or_else(|| cards.first().map(|c| c.id.clone()))
    }

    fn open_selected(&mut self, _: &OpenSession, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen != Screen::Inbox {
            return;
        }
        if let Some(n) = inbox::notes(&self.agents).into_iter().nth(self.inbox) {
            self.focus_agent(&n.agent, window, cx);
        }
    }

    fn next_waiting(&mut self, _: &NextWaiting, window: &mut Window, cx: &mut Context<Self>) {
        let projects = self.projects();
        let waiting: Vec<String> = projects.iter().flat_map(|p| self.cards(p)).filter(|c| c.status == Status::NeedsYou).map(|c| c.id).collect();
        let next = waiting.iter().position(|id| self.session.as_ref() == Some(id)).map_or(0, |i| (i + 1) % waiting.len().max(1));
        if let Some(id) = waiting.get(next).cloned() {
            self.overlay = None;
            self.side = Side::Sessions;
            self.focus_agent(&id, window, cx);
        }
    }

    fn toggle_rail(&mut self, _: &ToggleRail, _: &mut Window, cx: &mut Context<Self>) {
        if self.layout == Layout::Compact {
            self.panel = !self.panel;
            cx.notify();
        }
    }

    pub fn new_tab(&mut self, _: &NewTab, _: &mut Window, cx: &mut Context<Self>) {
        self.tab_menu = false;
        self.new_shell(None, cx);
    }

    fn toggle_focus(&mut self, _: &ToggleFocus, _: &mut Window, cx: &mut Context<Self>) {
        if self.panel {
            self.panel = false;
        } else {
            self.layout = match self.layout {
                Layout::Sidebars => Layout::Compact,
                Layout::Compact => Layout::Focus,
                Layout::Focus => Layout::Sidebars,
            };
        }
        cx.notify();
    }

    fn on_term_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.focused.clone() else { return };
        let Some(s) = self.sessions.get(&id) else { return };
        let app_cursor = s.term.as_ref().is_some_and(|t| t.app_cursor());
        if let Some(bytes) = keys::key_bytes(&ev.keystroke, app_cursor) {
            self.daemon.input(&id, &bytes);
            cx.stop_propagation();
        }
    }
}

/// Typed text arrives here rather than as key-downs so IMEs (Telex, dead keys, CJK) can compose it.
impl EntityInputHandler for Desktop {
    fn text_for_range(&mut self, _: Range<usize>, _: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        None
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.marked.map(|len| 0..len)
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
    }

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, _: &mut Context<Self>) {
        self.marked = None;
        if let Some(id) = &self.focused {
            self.daemon.input(id, text.as_bytes());
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        _: &mut Context<Self>,
    ) {
        self.marked = (!text.is_empty()).then(|| text.encode_utf16().count());
    }

    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        None
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

/// GPUI leaves `reduce_motion` to the app; this mirrors the macOS setting into it.
fn follow_reduce_motion(cx: &mut App) {
    #[cfg(target_os = "macos")]
    cx.set_reduce_motion(objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion());
}

fn main() {
    let capture = capture::Capture::from_args();
    let path = daemon::sock_path();
    let (daemon, mut rx) = Daemon::connect(&path).unwrap_or_else(|e| {
        eprintln!("pocket-desktop: cannot reach pocketd at {}: {e}", path.display());
        std::process::exit(1)
    });
    let home = path.parent().unwrap_or(&path).to_path_buf();
    let (outbox, mut agent_rx) = agents::connect(&home);
    let store = Store::load(&home);
    gpui_kit::application().with_assets(theme::Assets).run(move |cx| {
        gpui_kit::init(cx);
        theme::init(cx);
        follow_reduce_motion(cx);
        cx.bind_keys([
            KeyBinding::new("cmd-k", OpenPalette, None),
            KeyBinding::new("cmd-p", GoToFile, None),
            KeyBinding::new("cmd-n", StartSession, None),
            KeyBinding::new("cmd-j", NextWaiting, None),
            KeyBinding::new("cmd-\\", ToggleRail, None),
            KeyBinding::new("cmd-.", ToggleFocus, None),
            KeyBinding::new("cmd-t", NewTab, None),
            KeyBinding::new("cmd-shift-n", NewWorktree, None),
            KeyBinding::new("cmd-,", ProjectSettings, None),
            KeyBinding::new("cmd-enter", OpenSession, None),
        ]);
        cx.bind_keys(keys::bindings());
        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        let mut opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Coding Pocket".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(14.), px(14.))),
            }),
            ..Default::default()
        };
        if capture.is_some() {
            capture::Capture::hide(&mut opts);
        }
        let window = cx.open_window(opts, move |window, cx| {
            let view = cx.new(|cx| {
                cx.spawn_in(window, async move |this, cx| {
                    while let Some(m) = rx.next().await {
                        if this.update_in(cx, |d: &mut Desktop, window, cx| d.on_msg(m, window, cx)).is_err() {
                            break;
                        }
                    }
                })
                .detach();
                cx.spawn(async move |this, cx| {
                    while let Some(ev) = agent_rx.next().await {
                        if this.update(cx, |d: &mut Desktop, cx| d.on_agents(ev, cx)).is_err() {
                            break;
                        }
                    }
                })
                .detach();
                let poll = daemon.clone();
                cx.spawn(async move |this, cx| {
                    for tick in 0u64.. {
                        poll.send(json!({"op": "list"}));
                        let refresh = |d: &mut Desktop, cx: &mut Context<Desktop>| {
                            if d.git_done == d.git_run {
                                d.refresh_git(cx);
                            }
                        };
                        if tick % 2 == 0 && this.update(cx, refresh).is_err() {
                            break;
                        }
                        cx.background_executor().timer(Duration::from_secs(1)).await;
                    }
                })
                .detach();
                let handle = window.window_handle();
                let this = cx.weak_entity();
                cx.on_system_notification_response(move |r, cx| {
                    cx.activate(true);
                    let _ = handle.update(cx, |_, window, cx| this.update(cx, |d, cx| d.focus_agent(&r.tag, window, cx)));
                });
                Desktop::new(daemon, outbox, store, window, cx)
            });
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("open window");
        match capture {
            Some(c) => c.run(window, cx),
            None => cx.activate(true),
        }
    });
}
