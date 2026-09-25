mod agents;
mod daemon;
mod diff;
mod git;
mod inbox;
mod keys;
mod sessions;
mod store;
mod term;
mod termview;
mod theme;
mod view;
mod workspace;

use agents::{Agents, Summary};
use daemon::{Daemon, Msg};
use futures::StreamExt;
use git::Repo;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::component::{Root, Theme};
use gpui_kit::*;
use serde_json::json;
use sessions::Sessions;
use std::borrow::Cow;
use std::collections::{HashMap, HashSet, VecDeque};
use std::ops::Range;
use std::path::PathBuf;
use std::time::Duration;
use store::Store;
use workspace::{Tab, Workspace};

actions!(desktop, [FocusSearch, OpenSession]);

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
pub enum Status {
    NeedsYou,
    Working,
    Done,
    Failed,
}

pub struct Card {
    pub id: String,
    pub provider: String,
    pub model: String,
    pub title: String,
    pub cwd: String,
    pub at: i64,
    pub status: Status,
}

enum Intent {
    Session,
    Tab(String),
    Split(String, bool),
}

pub struct Comment {
    pub path: String,
    pub line: usize,
    pub text: String,
}

pub struct Desktop {
    daemon: Daemon,
    sessions: Sessions,
    agents: Agents,
    store: Store,
    project: Option<String>,
    screen: Screen,
    side: Side,
    sidebar: bool,
    session: Option<String>,
    workspaces: HashMap<String, Workspace>,
    intents: VecDeque<Intent>,
    closed: HashSet<String>,
    repos: HashMap<String, Repo>,
    diff_file: Option<String>,
    diff: Vec<git::Line>,
    diff_split: bool,
    comments: Vec<Comment>,
    commenting: Option<(String, usize)>,
    comment_input: Entity<InputState>,
    initials: String,
    tree: HashMap<PathBuf, Vec<(bool, PathBuf)>>,
    git_run: u64,
    inbox: usize,
    read: HashSet<String>,
    error: Option<String>,
    root: FocusHandle,
    term_focus: FocusHandle,
    inbox_focus: FocusHandle,
    focused: Option<String>,
    filter: Entity<InputState>,
    sized: HashMap<String, (u16, u16)>,
    marked: Option<usize>,
    _subs: Vec<Subscription>,
}

fn under(cwd: &str, project: &str) -> bool {
    cwd == project || cwd.strip_prefix(project).is_some_and(|rest| rest.starts_with('/'))
}

impl Desktop {
    fn new(daemon: Daemon, store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search conversations…"));
        let comment_input = cx.new(|cx| InputState::new(window, cx).placeholder("Leave a comment, ↵ to save"));
        let _subs = vec![
            cx.subscribe(&filter, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe_in(&comment_input, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { .. } = ev {
                    this.save_comment(window, cx);
                }
            }),
        ];
        Self {
            daemon,
            sessions: Sessions::default(),
            agents: Agents::default(),
            project: store.projects.first().cloned(),
            store,
            screen: Screen::Sessions,
            side: Side::Sessions,
            sidebar: true,
            session: None,
            workspaces: HashMap::new(),
            intents: VecDeque::new(),
            closed: HashSet::new(),
            repos: HashMap::new(),
            diff_file: None,
            diff: Vec::new(),
            diff_split: false,
            comments: Vec::new(),
            commenting: None,
            comment_input,
            initials: String::new(),
            tree: HashMap::new(),
            git_run: 0,
            inbox: 0,
            read: HashSet::new(),
            error: None,
            root: cx.focus_handle(),
            term_focus: cx.focus_handle(),
            inbox_focus: cx.focus_handle(),
            focused: None,
            filter,
            sized: HashMap::new(),
            marked: None,
            _subs,
        }
    }

    fn on_msg(&mut self, m: Msg, window: &mut Window, cx: &mut Context<Self>) {
        match m.ev.as_str() {
            "sessions" => {
                let items = m.items.into_iter().filter(|i| !self.closed.contains(&i.id)).collect();
                for id in self.sessions.sync(items) {
                    self.daemon.send(json!({"op": "attach", "id": id}));
                }
                if self.project.is_none() {
                    self.project = self.projects().into_iter().next();
                }
            }
            "spawned" => {
                self.error = None;
                match self.intents.pop_front() {
                    Some(Intent::Session) | None => self.select_session(m.id.clone(), window, cx),
                    Some(Intent::Tab(parent)) => self.adopt(m.id.clone(), parent, None, window, cx),
                    Some(Intent::Split(parent, down)) => self.adopt(m.id.clone(), parent, Some(down), window, cx),
                }
                self.daemon.send(json!({"op": "list"}));
            }
            "error" => {
                if m.id.is_empty() {
                    self.intents.pop_front();
                }
                self.error = Some(m.error);
            }
            _ => self.sessions.apply(&m),
        }
        cx.notify();
    }

    fn adopt(&mut self, id: String, parent: String, split: Option<bool>, window: &mut Window, cx: &mut Context<Self>) {
        self.store.children.push((id.clone(), parent.clone()));
        self.store.save();
        let w = self.workspace(&parent);
        match split {
            Some(down) => w.split(id.clone(), down),
            None => w.add_tab(id.clone()),
        }
        self.focus_pane(id, window, cx);
    }

    pub fn projects(&self) -> Vec<String> {
        let mut out = self.store.projects.clone();
        let live = self.agents.list.iter().filter(|a| a.status != "closed").map(|a| &a.cwd);
        let sessions = self.sessions.items.iter().filter(|s| self.store.parent(&s.info.id).is_none()).map(|s| &s.info.cwd);
        for cwd in live.chain(sessions) {
            if !out.iter().any(|p| under(cwd, p)) {
                out.push(cwd.clone());
            }
        }
        out
    }

    pub fn project_of<'a>(&self, cwd: &str, projects: &'a [String]) -> Option<&'a String> {
        projects.iter().filter(|p| under(cwd, p)).max_by_key(|p| p.len())
    }

    pub fn cards(&self, project: &str) -> Vec<Card> {
        let projects = self.projects();
        let mine = |cwd: &str| self.project_of(cwd, &projects).is_some_and(|p| p == project);
        let exit = |id: &str| self.sessions.get(id).and_then(|s| s.exit);
        let mut out: Vec<Card> = self
            .agents
            .list
            .iter()
            .filter(|a| mine(&a.cwd) && self.store.parent(&a.id).is_none())
            .map(|a| Card {
                id: a.id.clone(),
                provider: a.provider.clone(),
                model: a.model.as_deref().map(|_| agents::model_label(a)).unwrap_or_default(),
                title: if a.title.is_empty() { "New session".into() } else { a.title.clone() },
                cwd: a.cwd.clone(),
                at: a.updated_at,
                status: match () {
                    _ if self.agents.needs_you(&a.id) => Status::NeedsYou,
                    _ if a.status == "running" || a.status == "compacting" => Status::Working,
                    _ if exit(&a.id).is_some_and(|c| c != 0) || self.agents.last_result(&a.id).is_some_and(|r| !r.ok) => Status::Failed,
                    _ => Status::Done,
                },
            })
            .collect();
        for s in &self.sessions.items {
            if mine(&s.info.cwd) && self.store.parent(&s.info.id).is_none() && self.agents.get(&s.info.id).is_none() {
                out.push(Card {
                    id: s.info.id.clone(),
                    provider: s.info.cmd.clone(),
                    model: String::new(),
                    title: view::command_line(&s.info),
                    cwd: s.info.cwd.clone(),
                    at: 0,
                    status: if s.exit.is_some_and(|c| c != 0) { Status::Failed } else { Status::Done },
                });
            }
        }
        out.sort_by_key(|c| std::cmp::Reverse(c.at));
        out
    }

    pub fn summary(&self, id: &str) -> Option<&Summary> {
        self.agents.get(id)
    }

    pub fn cwd_of(&self, id: &str) -> Option<String> {
        self.agents.get(id).map(|a| a.cwd.clone()).or_else(|| self.sessions.get(id).map(|s| s.info.cwd.clone()))
    }

    pub fn cwd(&self) -> Option<String> {
        self.session.as_deref().and_then(|id| self.cwd_of(id)).or_else(|| self.project.clone())
    }

    pub fn repo(&self) -> Option<&Repo> {
        self.repos.get(&self.cwd()?)
    }

    fn workspace(&mut self, id: &str) -> &mut Workspace {
        self.workspaces.entry(id.to_string()).or_insert_with(|| Workspace::new(id, self.store.children_of(id)))
    }

    pub fn select_project(&mut self, p: String, cx: &mut Context<Self>) {
        self.screen = Screen::Sessions;
        if self.project.as_ref() != Some(&p) {
            self.project = Some(p);
            self.session = None;
            self.diff_file = None;
            self.tree.clear();
            self.refresh_git(cx);
        }
        cx.notify();
    }

    pub fn select_session(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = Screen::Sessions;
        if let Some(p) = self.cwd_of(&id).and_then(|cwd| self.project_of(&cwd, &self.projects()).cloned()) {
            self.project = Some(p);
        }
        let pane = match self.workspace(&id).active() {
            Some(Tab::Term(rows)) => rows.first().and_then(|r| r.first()).cloned(),
            _ => None,
        };
        self.session = Some(id);
        if let Some(pane) = pane {
            self.focus_pane(pane, window, cx);
        }
        self.refresh_git(cx);
        cx.notify();
    }

    pub fn select_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.session.clone() else { return };
        let w = self.workspace(&id);
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

    fn spawn(&mut self, cmdline: &str, cwd: &str, intent: Intent, cx: &mut Context<Self>) {
        let Some(op) = daemon::spawn_op(cmdline, cwd) else { return };
        self.daemon.send(op);
        self.intents.push_back(intent);
        self.error = None;
        cx.notify();
    }

    pub fn new_session(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.project.clone() else { return };
        self.spawn("claude", &cwd, Intent::Session, cx);
    }

    /// Opens a login shell in the session's folder, as a new tab or a split of the active one.
    pub fn new_shell(&mut self, split: Option<bool>, cx: &mut Context<Self>) {
        let Some(parent) = self.session.clone() else { return };
        let Some(cwd) = self.cwd_of(&parent) else { return };
        let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/zsh".into());
        let intent = match split {
            Some(down) => Intent::Split(parent, down),
            None => Intent::Tab(parent),
        };
        self.spawn(&format!("{shell} -l"), &cwd, intent, cx);
    }

    pub fn close_pane(&mut self, id: &str, cx: &mut Context<Self>) {
        if self.sessions.get(id).is_some_and(|s| s.exit.is_none()) {
            self.daemon.send(json!({"op": "close", "id": id}));
        }
        self.sessions.remove(id);
        self.closed.insert(id.to_string());
        self.sized.remove(id);
        self.workspaces.remove(id);
        for w in self.workspaces.values_mut() {
            w.remove(id);
        }
        self.store.children.retain(|(c, _)| c != id);
        self.store.save();
        cx.notify();
    }

    pub fn close_tab(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(parent) = self.session.clone() else { return };
        for id in self.workspace(&parent).close_tab(i) {
            if id != parent {
                self.close_pane(&id, cx);
            } else if self.sessions.get(&id).is_some_and(|s| s.exit.is_none()) {
                self.daemon.send(json!({"op": "close", "id": id}));
            }
        }
        cx.notify();
    }

    pub fn add_project(&mut self, cx: &mut Context<Self>) {
        let paths = cx.prompt_for_paths(PathPromptOptions { files: false, directories: true, multiple: false, prompt: None });
        cx.spawn(async move |this, cx| {
            let Ok(Ok(Some(paths))) = paths.await else { return };
            let Some(dir) = paths.into_iter().next() else { return };
            let dir = dir.to_string_lossy().to_string();
            this.update(cx, |d, cx| {
                if !d.store.projects.contains(&dir) {
                    d.store.projects.push(dir.clone());
                    d.store.save();
                }
                d.select_project(dir, cx);
            })
            .ok();
        })
        .detach();
    }

    pub fn fit(&mut self, id: &str, cols: u16, rows: u16) {
        if self.sized.get(id) != Some(&(cols, rows)) && self.sessions.get(id).is_some_and(|s| s.exit.is_none()) {
            self.daemon.send(json!({"op": "resize", "id": id, "cols": cols, "rows": rows}));
            self.sized.insert(id.to_string(), (cols, rows));
        }
    }

    fn git_cwds(&self) -> Vec<String> {
        let mut cwds: Vec<String> = self.project.iter().cloned().collect();
        if let Some(p) = &self.project {
            cwds.extend(self.cards(p).into_iter().map(|c| c.cwd));
        }
        cwds.extend(self.cwd());
        cwds.sort();
        cwds.dedup();
        cwds
    }

    pub fn refresh_git(&mut self, cx: &mut Context<Self>) {
        let cwds = self.git_cwds();
        let diff = self.cwd().zip(self.diff_file.clone());
        let mut dirs: Vec<PathBuf> = self.tree.keys().cloned().collect();
        dirs.extend(self.project.as_ref().map(PathBuf::from));
        self.git_run += 1;
        let run = self.git_run;
        let task = cx.background_executor().spawn(async move {
            let repos: Vec<(String, Option<Repo>)> = cwds.into_iter().map(|c| {
                let r = git::read(&c);
                (c, r)
            }).collect();
            let diff = diff.map(|(cwd, path)| (git::file_diff(&cwd, &path), path));
            let initials = repos.first().map(|(c, _)| git::user_initials(c)).unwrap_or_default();
            let tree = dirs.into_iter().map(|d| {
                let listing = view::list_dir(&d);
                (d, listing)
            }).collect();
            (repos, diff, initials, tree)
        });
        cx.spawn(async move |this, cx| {
            let (repos, diff, initials, tree) = task.await;
            this.update(cx, |d, cx| {
                if run != d.git_run {
                    return;
                }
                d.repos = repos.into_iter().filter_map(|(c, r)| Some((c, r?))).collect();
                d.tree = tree;
                if let Some((lines, _)) = diff.filter(|(_, p)| d.diff_file.as_ref() == Some(p)) {
                    d.diff = lines;
                }
                d.initials = initials;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn open_changes(&mut self, path: Option<String>, cx: &mut Context<Self>) {
        let path = path.or_else(|| self.diff_file.clone()).or_else(|| self.repo()?.files.first().map(|f| f.path.clone()));
        if path != self.diff_file {
            self.diff.clear();
            self.commenting = None;
        }
        self.diff_file = path;
        if let Some(id) = self.session.clone() {
            self.workspace(&id).open_changes();
        }
        self.load_diff(cx);
    }

    fn load_diff(&mut self, cx: &mut Context<Self>) {
        if self.diff_file.is_none() {
            self.diff_file = self.repo().and_then(|r| r.files.first()).map(|f| f.path.clone());
        }
        self.refresh_git(cx);
    }

    pub fn stage(&mut self, path: String, staged: bool, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        let task = cx.background_executor().spawn(async move { git::set_staged(&cwd, &path, staged) });
        cx.spawn(async move |this, cx| {
            task.await;
            this.update(cx, |d, cx| d.refresh_git(cx)).ok();
        })
        .detach();
    }

    pub fn start_comment(&mut self, path: String, line: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.commenting = Some((path, line));
        self.comment_input.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    fn save_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.comment_input.read(cx).value().trim().to_string();
        let Some((path, line)) = self.commenting.take() else { return };
        if !text.is_empty() {
            self.comments.push(Comment { path, line, text });
        }
        self.comment_input.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    pub fn send_comment(&mut self, i: usize, cx: &mut Context<Self>) {
        let Some(id) = self.session.clone() else { return };
        let c = self.comments.remove(i);
        self.daemon.send(json!({"op": "prompt", "id": id, "text": format!("{} line {}: {}", c.path, c.line, c.text)}));
        cx.notify();
    }

    fn open_selected(&mut self, _: &OpenSession, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen != Screen::Inbox {
            return;
        }
        if let Some(n) = self.notes().into_iter().nth(self.inbox) {
            self.select_session(n.agent, window, cx);
        }
    }

    fn focus_search(&mut self, _: &FocusSearch, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = Screen::Sessions;
        self.side = Side::Sessions;
        self.sidebar = true;
        self.filter.update(cx, |s, cx| s.focus(window, cx));
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

fn light_theme(cx: &mut App) {
    let t = Theme::global_mut(cx);
    t.font_family = theme::SANS.into();
    t.font_size = px(14.);
    t.foreground = rgb(theme::INK).into();
    t.muted_foreground = rgb(theme::MUTED).into();
    t.background = rgb(theme::APP).into();
    t.caret = rgb(theme::INK).into();
}

fn main() {
    let path = daemon::sock_path();
    let (daemon, mut rx) = Daemon::connect(&path).unwrap_or_else(|e| {
        eprintln!("pocket-desktop: cannot reach pocketd at {}: {e}", path.display());
        std::process::exit(1)
    });
    let home = path.parent().unwrap_or(&path).to_path_buf();
    let mut agent_rx = agents::connect(&home);
    let store = Store::load(&home);
    gpui_kit::application().with_assets(theme::Assets).run(move |cx| {
        gpui_kit::init(cx);
        cx.text_system().add_fonts(theme::FONTS.iter().map(|f| Cow::Borrowed(*f)).collect()).expect("bundled fonts load");
        light_theme(cx);
        cx.bind_keys([KeyBinding::new("cmd-k", FocusSearch, None), KeyBinding::new("cmd-enter", OpenSession, None)]);
        let bounds = Bounds::centered(None, size(px(1440.), px(900.)), cx);
        let opts = WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            titlebar: Some(TitlebarOptions {
                title: Some("Coding Pocket".into()),
                appears_transparent: true,
                traffic_light_position: Some(point(px(15.), px(20.))),
            }),
            ..Default::default()
        };
        cx.open_window(opts, move |window, cx| {
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
                        if this.update(cx, |d: &mut Desktop, cx| {
                            d.agents.apply(ev);
                            cx.notify();
                        })
                        .is_err()
                        {
                            break;
                        }
                    }
                })
                .detach();
                let poll = daemon.clone();
                cx.spawn(async move |this, cx| {
                    for tick in 0u64.. {
                        poll.send(json!({"op": "list"}));
                        if tick % 2 == 0 && this.update(cx, |d: &mut Desktop, cx| d.refresh_git(cx)).is_err() {
                            break;
                        }
                        cx.background_executor().timer(Duration::from_secs(1)).await;
                    }
                })
                .detach();
                Desktop::new(daemon, store, window, cx)
            });
            cx.new(|cx| Root::new(view, window, cx))
        })
        .expect("open window");
        cx.activate(true);
    });
}
