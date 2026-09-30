pub(crate) mod alerts;
pub(crate) mod chrome;
pub(crate) mod project;

use crate::actions::NextWaiting;
use crate::desktop::chrome::{Confirm, Layout, Overlay, RowMenu, Screen, Side};
use crate::explorer::{mermaid, preview};
use crate::git_ui::changes;
use crate::git_ui::diff::{self, Comment};
use crate::modals::{add_project, new_session};
use crate::status::Status;
use crate::syntax;
use crate::terminals::Intent;
use crate::terminals::sessions::Sessions;
use agents::{Agents, Outbox};
use daemon::Daemon;
use git::Repo;
use gpui_kit::component::input::{EditorState, InputEvent, InputState, TextDecorationCollection, TextareaState};
use gpui_kit::component::text::TextViewState;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::PathBuf;
use store::Store;
use theme::*;
use workspace::{Doc, Tab, Workspace};

pub struct Desktop {
    pub(crate) daemon: Daemon,
    pub(crate) outbox: Outbox,
    pub(crate) viewing: Option<Vec<String>>,
    pub(crate) statuses: HashMap<String, Status>,
    pub(crate) alerted: HashSet<String>,
    pub(crate) sessions: Sessions,
    pub(crate) agents: Agents,
    pub(crate) store: Store,
    pub(crate) project: Option<String>,
    pub(crate) screen: Screen,
    pub(crate) side: Side,
    pub(crate) layout: Layout,
    /// Indexed by `Column`; `None` keeps the design's width.
    pub(crate) widths: [Option<f32>; 2],
    pub(crate) panel: bool,
    pub(crate) tab_menu: bool,
    pub(crate) session: Option<String>,
    pub(crate) workspaces: HashMap<String, Workspace>,
    pub(crate) intents: VecDeque<Intent>,
    pub(crate) closed: HashSet<String>,
    pub(crate) repos: HashMap<String, Repo>,
    pub(crate) diff_file: Option<String>,
    pub(crate) diff: Vec<git::Line>,
    pub(crate) diff_rows: Vec<diff::Row>,
    pub(crate) diff_list: ListState,
    pub(crate) diff_split: bool,
    pub(crate) diff_hl: Vec<syntax::Spans>,
    pub(crate) diff_open: HashSet<usize>,
    pub(crate) diff_source: (String, String),
    pub(crate) diff_syntax: (Vec<syntax::Spans>, Vec<syntax::Spans>),
    pub(crate) selection: Option<(usize, usize)>,
    pub(crate) dragging: bool,
    pub(crate) composing: bool,
    pub(crate) comment_input: Entity<TextareaState>,
    pub(crate) comment_target: Option<String>,
    pub(crate) target_menu: bool,
    pub(crate) commit_input: Entity<TextareaState>,
    /// What the commit button says while git works.
    pub(crate) busy: Option<&'static str>,
    pub(crate) writing: bool,
    pub(crate) commit_error: Option<String>,
    pub(crate) commit_menu: bool,
    pub(crate) changes_menu: bool,
    pub(crate) changes_tree: bool,
    pub(crate) changes_folded: HashSet<String>,
    pub(crate) changes_scroll: UniformListScrollHandle,
    pub(crate) tab_scroll: ScrollHandle,
    /// The worktree and tab last scrolled into view, so a tab is revealed once when it becomes active rather than every frame.
    pub(crate) tab_revealed: Option<(String, usize)>,
    pub(crate) initials: String,
    pub(crate) tree: HashMap<PathBuf, Vec<(bool, PathBuf)>>,
    pub(crate) git_run: u64,
    pub(crate) git_done: u64,
    pub(crate) inbox: usize,
    pub(crate) error: Option<String>,
    pub(crate) root: FocusHandle,
    pub(crate) term_focus: FocusHandle,
    pub(crate) inbox_focus: FocusHandle,
    pub(crate) focused: Option<String>,
    pub(crate) filter: Entity<InputState>,
    pub(crate) session_search: Entity<InputState>,
    pub(crate) sized: HashMap<String, (u16, u16)>,
    pub(crate) marked: Option<usize>,
    pub(crate) overlay: Option<Overlay>,
    pub(crate) palette_ix: usize,
    pub(crate) palette_all: bool,
    pub(crate) palette_files: Vec<String>,
    pub(crate) worktrees: HashMap<String, Vec<git::Worktree>>,
    pub(crate) worktree: Option<String>,
    /// Terminal → worktree while the terminal runs the worktree's setup before its agent.
    pub(crate) setups: HashMap<String, String>,
    pub(crate) confirm: Option<Confirm>,
    pub(crate) row_menu: Option<RowMenu>,
    pub(crate) file: Option<String>,
    pub(crate) file_preview: Option<preview::Preview>,
    pub(crate) file_diff: Vec<git::Line>,
    pub(crate) code: Entity<EditorState>,
    pub(crate) code_marks: TextDecorationCollection,
    pub(crate) code_stale: bool,
    pub(crate) code_file: Option<String>,
    pub(crate) code_text: SharedString,
    pub(crate) md: Entity<TextViewState>,
    pub(crate) diagrams: Entity<mermaid::Diagrams>,
    pub(crate) md_source: bool,
    /// The copied path and the timer that turns its check back; a new copy replaces, and so cancels, the old one.
    pub(crate) path_copied: Option<(String, Task<()>)>,
    pub(crate) comments: Vec<Comment>,
    pub(crate) viewed: HashSet<String>,
    pub(crate) new_form: new_session::NewForm,
    pub(crate) repo_form: add_project::RepoForm,
    pub(crate) capturing: bool,
    pub(crate) _subs: Vec<Subscription>,
}

impl Desktop {
    pub(crate) fn new(daemon: Daemon, outbox: Outbox, store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions, files and actions…"));
        let session_search = cx.new(|cx| InputState::new(window, cx).placeholder("Search sessions…"));
        let comment_input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Ask the agent about these lines…").rows(3));
        let commit_input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Message (⌘↩ to commit)").auto_grow(1, 8));
        let code = cx.new(|cx| EditorState::new(window, cx).line_number(true).searchable(true).soft_wrap(false));
        let code_marks = code.update(cx, |s, cx| s.create_decorations_collection(Vec::new(), cx));
        let md = cx.new(|cx| TextViewState::markdown("", cx));
        let diagrams = cx.new(|_| mermaid::Diagrams::new(&md));
        let (new_form, new_subs) = new_session::NewForm::new(window, cx);
        let (repo_form, repo_subs) = add_project::RepoForm::new(window, cx);
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
            tab_scroll: ScrollHandle::new(),
            tab_revealed: None,
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

    pub(crate) fn show_tree(&mut self, cwd: &str, tree: &str) {
        if let Some(p) = self.project_of(cwd, &self.projects()).cloned() {
            self.project = Some(p);
        }
        self.screen = Screen::Sessions;
        self.worktree = Some(tree.to_string());
    }

    pub fn select_project(&mut self, p: String, cx: &mut Context<Self>) {
        self.screen = Screen::Sessions;
        if self.project.as_ref() != Some(&p) {
            self.set_project(Some(p));
            self.refresh_git(cx);
            self.load_active(cx);
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
            self.load_active(cx);
        }
        self.session = None;
        cx.notify();
    }

    pub fn select_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        let w = self.workspace(&tree);
        w.active = i;
        match w.active().cloned() {
            Some(Tab::Term(rows)) => self.focus_pane(rows[0][0].clone(), window, cx),
            Some(Tab::Doc(doc)) => self.show_doc(doc, cx),
            None => {}
        }
        cx.notify();
    }

    /// The file or changes the worktree's active tab shows.
    pub fn active_doc(&self) -> Option<Doc> {
        match self.workspaces.get(&self.cwd()?)?.active()? {
            Tab::Doc(doc) => Some(doc.clone()),
            Tab::Term(_) => None,
        }
    }

    /// Opens `doc` in its tab of the worktree on screen.
    pub fn open_doc(&mut self, doc: Doc, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        self.screen = Screen::Sessions;
        self.workspace(&tree).open_doc(doc.clone());
        self.show_doc(doc, cx);
    }

    fn show_doc(&mut self, doc: Doc, cx: &mut Context<Self>) {
        match doc {
            Doc::File(path) => {
                if self.file.as_ref() != Some(&path) {
                    self.file = Some(path);
                    self.file_preview = None;
                    self.file_diff.clear();
                    self.code_stale = true;
                    self.md_source = false;
                }
                self.load_file(cx);
            }
            Doc::Diff(path) => {
                if self.diff_file.as_ref() != Some(&path) {
                    self.selection = None;
                    self.diff_open.clear();
                    self.set_diff(Vec::new(), true);
                    self.diff_file = Some(path);
                }
                self.load_diff(cx);
            }
        }
        cx.notify();
    }

    pub fn loaded(&self, doc: &Doc) -> bool {
        match doc {
            Doc::File(p) => self.file.as_ref() == Some(p),
            Doc::Diff(p) => self.diff_file.as_ref() == Some(p),
        }
    }

    /// Loads the doc a tab revealed by closing or switching worktrees shows, unless it is loaded already.
    pub(crate) fn load_active(&mut self, cx: &mut Context<Self>) {
        if let Some(doc) = self.active_doc().filter(|d| !self.loaded(d)) {
            self.show_doc(doc, cx);
        }
    }

    /// Returns whether a menu was open.
    pub(crate) fn close_menus(&mut self) -> bool {
        let open = self.tab_menu || self.row_menu.is_some() || self.commit_menu || self.changes_menu;
        (self.tab_menu, self.row_menu, self.commit_menu, self.changes_menu) = (false, None, false, false);
        open
    }

    pub(crate) fn next_waiting(&mut self, _: &NextWaiting, window: &mut Window, cx: &mut Context<Self>) {
        let projects = self.projects();
        let waiting: Vec<String> = projects.iter().flat_map(|p| self.cards(p)).filter(|c| c.status == Status::NeedsYou).map(|c| c.id).collect();
        let next = waiting.iter().position(|id| self.session.as_ref() == Some(id)).map_or(0, |i| (i + 1) % waiting.len().max(1));
        if let Some(id) = waiting.get(next).cloned() {
            self.overlay = None;
            self.side = Side::Sessions;
            self.focus_agent(&id, window, cx);
        }
    }

    fn main_view(&mut self, cx: &mut Context<Self>) -> Div {
        let body = match (self.screen, self.side) {
            (Screen::Inbox, _) => self.inbox_detail(cx),
            _ => match self.cwd().filter(|t| !self.workspace(t).tabs.is_empty()) {
                Some(tree) => self.session_page(&tree, cx),
                None => self.blank_page(cx),
            },
        };
        ui::page(div()).flex_1().min_w_0().h_full().flex().flex_col().overflow_hidden().child(body)
    }

    fn blank_page(&self, cx: &mut Context<Self>) -> Div {
        let text = if self.project.is_none() { "Add a project to begin." } else { "Pick a session, or start a new one." };
        div().flex_1().flex().flex_col().child(self.page_bar(vec!["Sessions".into()], Vec::new(), div(), cx)).child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(14.))
                .text_size(px(14.))
                .text_color(rgba(TEXT_3))
                .child(text)
                .when(self.project.is_some(), |d| {
                    d.child(
                        ui::button("blank-new", ui::Variant::Primary, Some("sparkle"), "New session")
                            .child(ui::button_kbd("⌘N"))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx))),
                    )
                }),
        )
    }
}

impl Render for Desktop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_code(window, cx);
        self.sync_view(window, cx);
        let lead = match self.layout {
            Layout::Sidebars => Some(self.aside(cx)),
            Layout::Compact => Some(self.nav(cx)),
            Layout::Focus => None,
        };
        let column = (self.layout == Layout::Sidebars).then(|| self.column_view(cx));
        let panel = (self.layout == Layout::Compact && self.panel).then(|| self.panel_view(cx));
        let page = self.main_view(cx);
        let overlay = self.overlay_view(window, cx);
        div()
            .relative()
            .size_full()
            .flex()
            .bg(rgba(WINDOW))
            .font_family(SANS)
            .line_height(relative(1.2))
            .text_color(rgba(TEXT))
            .track_focus(&self.root)
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                if ev.keystroke.key != "escape" {
                    return;
                }
                if this.close_picker() || this.close_menus() {
                    cx.notify();
                } else if this.overlay.is_some_and(|o| o != Overlay::Palette) {
                    this.close_overlay(window, cx);
                } else {
                    return;
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(|this, _: &crate::actions::OpenPalette, window, cx| this.open(Overlay::Palette, window, cx)))
            .on_action(cx.listener(|this, _: &crate::actions::StartSession, window, cx| this.open(Overlay::NewSession, window, cx)))
            .on_action(cx.listener(Self::go_to_file))
            .on_action(cx.listener(Self::new_worktree))
            .on_action(cx.listener(Self::project_settings))
            .on_action(cx.listener(Self::next_waiting))
            .on_action(cx.listener(Self::toggle_rail))
            .on_action(cx.listener(Self::toggle_focus))
            .on_action(cx.listener(Self::new_tab))
            .on_action(cx.listener(Self::open_selected))
            .children(lead)
            .children(column)
            .child(page)
            .children(panel)
            .children(overlay)
    }
}

/// GPUI leaves `reduce_motion` to the app; this mirrors the macOS setting into it.
pub(crate) fn follow_reduce_motion(cx: &mut App) {
    #[cfg(target_os = "macos")]
    cx.set_reduce_motion(objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion());
}
