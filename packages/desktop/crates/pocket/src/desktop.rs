pub(crate) mod alerts;
pub(crate) mod chrome;
pub(crate) mod geometry;
pub(crate) mod dock;
pub(crate) mod jump;
pub(crate) mod project;
pub(crate) mod sounds;

use crate::browser::Browsers;
use crate::creating::Creates;
use crate::desktop::alerts::Alerts;
use crate::desktop::chrome::{Confirm, Layout, Overlay, RowMenu, Screen, Side};
use crate::desktop::dock::Badge;
use crate::desktop::geometry::Geometry;
use crate::desktop::jump::Chips;
use crate::desktop::sounds::Chime;
use crate::explorer::ExplorerState;
use crate::explorer::preview::PreviewState;
use crate::git_ui::changes::ChangesState;
use crate::git_ui::commit::CommitState;
use crate::git_ui::diff::DiffState;
use crate::git_ui::graph::GraphState;
use crate::inbox::InboxState;
use crate::modals::pair_phone::PairPhone;
use crate::modals::{add_project, new_session};
use crate::palette::PaletteState;
use crate::sidebar::SidebarState;
use crate::terminal_view::TerminalViewState;
use crate::terminals::Terminals;
use agents::{Agents, Outbox};
use daemon::Daemon;
use git::Repo;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::Instant;
use store::Store;
use theme::*;
use workspace::{Doc, Tab, Workspace};

pub struct Desktop {
    pub(crate) daemon: Daemon,
    pub(crate) outbox: Outbox,
    pub(crate) alerts: Alerts,
    pub(crate) chime: Chime,
    pub(crate) badge: Badge,
    pub(crate) terminals: Terminals,
    pub(crate) creates: Creates,
    pub(crate) agents: Agents,
    pub(crate) store: Store,
    pub(crate) project: Option<String>,
    pub(crate) screen: Screen,
    pub(crate) side: Side,
    pub(crate) layout: Layout,
    /// Indexed by `Column`; `None` keeps the design's width.
    pub(crate) widths: [Option<f32>; 2],
    pub(crate) panel: bool,
    pub(crate) session: Option<String>,
    pub(crate) workspaces: HashMap<String, Workspace>,
    pub(crate) repos: HashMap<String, Repo>,
    pub(crate) diff: DiffState,
    pub(crate) changes: ChangesState,
    pub(crate) graph: GraphState,
    pub(crate) commit: CommitState,
    pub(crate) terminal: TerminalViewState,
    pub(crate) browsers: Browsers,
    pub(crate) initials: String,
    pub(crate) explorer: ExplorerState,
    pub(crate) git_run: u64,
    pub(crate) git_done: u64,
    pub(crate) inbox: InboxState,
    pub(crate) error: Option<String>,
    pub(crate) root: FocusHandle,
    pub(crate) palette: PaletteState,
    pub(crate) sidebar: SidebarState,
    pub(crate) chips: Chips,
    pub(crate) overlay: Option<Overlay>,
    pub(crate) worktrees: HashMap<String, Vec<git::Worktree>>,
    pub(crate) worktree: Option<String>,
    pub(crate) confirm: Option<Confirm>,
    pub(crate) row_menu: Option<RowMenu>,
    pub(crate) preview: PreviewState,
    pub(crate) new_form: new_session::NewForm,
    pub(crate) repo_form: add_project::RepoForm,
    pub(crate) pair: PairPhone,
    pub(crate) capturing: bool,
    pub(crate) geometry: Geometry,
    pub(crate) _subs: Vec<Subscription>,
}

impl Desktop {
    pub(crate) fn new(daemon: Daemon, outbox: Outbox, store: Store, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (palette, palette_subs) = PaletteState::new(window, cx);
        let (sidebar, sidebar_subs) = SidebarState::new(window, cx);
        let (chips, chips_subs) = Chips::new(window, cx);
        let (diff, diff_subs) = DiffState::new(window, cx);
        let (changes, changes_subs) = ChangesState::new(window, cx);
        let (preview, preview_subs) = PreviewState::new(window, cx);
        let (new_form, new_subs) = new_session::NewForm::new(window, cx);
        let (repo_form, repo_subs) = add_project::RepoForm::new(window, cx);
        let (geometry, geometry_subs) = Geometry::new(window, cx);
        let (browsers, browser_subs) = Browsers::new(window, cx);
        let root = cx.focus_handle();
        window.focus(&root, cx);
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| this.update(cx, |d, cx| d.may_quit(cx)).unwrap_or(true));
        let mut _subs = vec![
            // The setting changes in System Settings, so the window coming back is when it may have.
            cx.observe_window_activation(window, |_, _, cx| follow_reduce_motion(cx)),
            cx.observe_window_appearance(window, |this, window, cx| this.set_appearance(window.appearance(), window, cx)),
            // Unfocused, GPUI dispatches keys from the window's root node, above this view's action handlers.
            cx.on_focus_lost(window, |this, window, cx| window.focus(&this.root, cx)),
        ];
        _subs.extend(palette_subs);
        _subs.extend(sidebar_subs);
        _subs.extend(chips_subs);
        _subs.extend(diff_subs);
        _subs.extend(changes_subs);
        _subs.extend(preview_subs);
        _subs.extend(new_subs);
        _subs.extend(repo_subs);
        _subs.extend(geometry_subs);
        _subs.extend(browser_subs);
        let (layout, widths) = (store.layout, [store.widths.projects, store.widths.sessions]);
        Self {
            daemon,
            outbox,
            alerts: Alerts::new(),
            chime: Chime::new(),
            badge: Badge::default(),
            terminals: Terminals::new(),
            creates: Creates::default(),
            agents: Agents::default(),
            project: store.projects.first().cloned(),
            store,
            screen: Screen::Sessions,
            side: Side::Sessions,
            layout,
            widths,
            panel: false,
            session: None,
            workspaces: HashMap::new(),
            repos: HashMap::new(),
            diff,
            changes,
            graph: GraphState::default(),
            commit: CommitState::default(),
            initials: String::new(),
            explorer: ExplorerState::new(),
            git_run: 0,
            git_done: 0,
            error: None,
            root,
            terminal: TerminalViewState::new(cx),
            browsers,
            inbox: InboxState::new(cx),
            palette,
            sidebar,
            chips,
            overlay: None,
            worktrees: HashMap::new(),
            worktree: None,
            confirm: None,
            row_menu: None,
            preview,
            new_form,
            repo_form,
            pair: PairPhone::default(),
            capturing: false,
            geometry,
            _subs,
        }
    }

    /// Shows an agent's session: its worktree, the tab holding its terminal, and that pane focused.
    pub fn focus_agent(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(term) = self.agents.get(id).map(|a| a.terminal_id.clone()) else { return };
        let Some(cwd) = self.terminals.sessions.get(&term).map(|s| s.info.cwd.clone()) else { return };
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

    pub(crate) fn set_appearance(&mut self, appearance: WindowAppearance, window: &mut Window, cx: &mut Context<Self>) {
        theme::set_appearance(appearance, cx);
        window.set_background_appearance(theme::window_background());
        self.recolor_diff(cx);
        self.recolor_commit(cx);
        cx.notify();
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
        self.diff.file = None;
        self.preview.file = None;
        self.explorer.tree.clear();
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
            Some(Tab::Web(_)) => window.focus(&self.browsers.focus, cx),
            None => {}
        }
        cx.notify();
    }

    /// The file or changes the worktree's active tab shows.
    pub fn active_doc(&self) -> Option<Doc> {
        match self.workspaces.get(&self.cwd()?)?.active()? {
            Tab::Doc(doc) => Some(doc.clone()),
            Tab::Term(_) | Tab::Web(_) => None,
        }
    }

    /// Opens `doc` in its tab of the worktree on screen; unless `pin`, in the preview tab the next open takes over.
    pub fn open_doc(&mut self, doc: Doc, pin: bool, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        self.screen = Screen::Sessions;
        self.workspace(&tree).open_doc(doc.clone(), pin);
        self.show_doc(doc, cx);
    }

    pub(crate) fn pin_doc(&mut self, doc: &Doc) {
        if let Some(tree) = self.cwd() {
            self.workspace(&tree).pin(doc);
        }
    }

    pub(crate) fn close_doc(&mut self, doc: &Doc, cx: &mut Context<Self>) {
        if let Some(i) = self.cwd().and_then(|t| self.workspaces.get(&t)?.doc_tab(doc)) {
            self.close_tab(i, cx);
        }
    }

    fn show_doc(&mut self, doc: Doc, cx: &mut Context<Self>) {
        match doc {
            Doc::File(path) => {
                self.preview.open(path);
                self.load_file(cx);
            }
            Doc::Diff(path) => {
                self.diff.select(path, None);
                self.load_diff(cx);
            }
            Doc::CommitFile { sha, path } => {
                self.diff.select(path, Some(sha));
                self.load_diff(cx);
            }
            Doc::Commit(sha) => self.show_commit(sha, cx),
        }
        cx.notify();
    }

    pub fn loaded(&self, doc: &Doc) -> bool {
        match doc {
            Doc::File(p) => self.preview.file.as_ref() == Some(p),
            Doc::Diff(p) => self.diff.file.as_ref() == Some(p) && self.diff.at.is_none(),
            Doc::CommitFile { sha, path } => self.diff.file.as_ref() == Some(path) && self.diff.at.as_ref() == Some(sha),
            Doc::Commit(sha) => self.commit.sha.as_ref() == Some(sha),
        }
    }

    /// Loads the doc a tab revealed by closing or switching worktrees shows, unless it is loaded already.
    pub(crate) fn load_active(&mut self, cx: &mut Context<Self>) {
        if let Some(doc) = self.active_doc().filter(|d| !self.loaded(d)) {
            self.show_doc(doc, cx);
        }
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.terminal.tab_menu || self.terminal.tab_actions.is_some() || self.row_menu.is_some() || self.changes.commit_menu || self.changes.menu
    }

    /// Returns whether a menu was open.
    pub(crate) fn close_menus(&mut self) -> bool {
        let open = self.menu_open();
        (self.terminal.tab_menu, self.terminal.tab_actions, self.row_menu, self.changes.commit_menu, self.changes.menu) = (false, None, None, false, false);
        self.sidebar.menu_at = None;
        open
    }

    /// The worktree whose tabs the main view shows, if it shows any.
    pub(crate) fn session_tree(&mut self) -> Option<String> {
        if self.terminals.link.is_down() || matches!(self.screen, Screen::Inbox) {
            return None;
        }
        self.cwd().filter(|t| !self.workspace(t).tabs.is_empty())
    }

    fn main_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let body = match self.session_tree() {
            _ if self.shown_create().is_some() => self.creating_page(cx),
            Some(tree) => self.session_page(&tree, window, cx),
            None if self.terminals.link.is_down() => self.link_page(cx),
            None if matches!(self.screen, Screen::Inbox) => self.inbox_detail(cx),
            None => self.blank_page(cx),
        };
        ui::page(div()).flex_1().min_w_0().h_full().flex().flex_col().overflow_hidden().child(body)
    }

    fn link_page(&self, cx: &mut Context<Self>) -> Div {
        let hint = self.terminals.link.hint(Instant::now());
        div().flex_1().flex().flex_col().child(self.page_bar(vec!["Sessions".into()], Vec::new(), div(), cx)).child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .text_size(px(14.))
                .text_color(TEXT_3)
                .child("Starting Pocket's terminal service…")
                .when(hint, |d| d.child("Not starting? In Terminal:").child(div().font_family(MONO).child("pocketd daemon install"))),
        )
    }

    fn blank_page(&self, cx: &mut Context<Self>) -> Div {
        let text = if self.project.is_none() { "Add a project to begin." } else { "Pick a session, or start a new one." };
        let observe = self.agents.observe_only();
        let (pad, toggle) = self.bar_start(cx);
        let bar = chrome::drag_area(ui::page_bar())
            .pl(px(pad))
            .children(toggle)
            .child(ui::breadcrumb(vec!["Sessions".into()]))
            .when(self.cwd().is_some() && !observe, |d| d.child(self.new_tab_controls(cx)));
        div().flex_1().flex().flex_col().child(bar).when(observe, |d| d.child(chrome::observe_banner())).child(
            div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(14.))
                .text_size(px(14.))
                .text_color(TEXT_3)
                .child(text)
                .when(self.project.is_some() && !observe, |d| {
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
        self.sync_cursor(window, cx);
        self.sync_browser();
        let lead = match self.layout {
            Layout::Sidebars => Some(self.aside(cx)),
            Layout::Compact => Some(self.nav(cx)),
            Layout::Focus => None,
        };
        let column = (self.layout == Layout::Sidebars).then(|| self.column_view(cx));
        let panel = (self.layout == Layout::Compact && self.panel).then(|| self.panel_view(cx));
        let page = self.main_view(window, cx);
        let overlay = self.overlay_view(window, cx);
        div()
            .relative()
            .size_full()
            .flex()
            .bg(WINDOW)
            .font_family(SANS)
            .line_height(relative(1.2))
            .text_color(TEXT)
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
            .on_action(cx.listener(Self::next_needs_you))
            .on_action(cx.listener(Self::go_to_up_next))
            .on_action(cx.listener(Self::jump_to))
            .on_action(cx.listener(Self::next_session))
            .on_action(cx.listener(Self::prev_session))
            .on_modifiers_changed(cx.listener(Self::on_modifiers))
            .on_action(cx.listener(Self::toggle_rail))
            .on_action(cx.listener(Self::toggle_focus))
            .on_action(cx.listener(Self::new_tab))
            .on_action(cx.listener(Self::new_browser))
            .on_action(cx.listener(Self::close_active_tab))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::quit))
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
