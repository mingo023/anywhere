pub(crate) mod alerts;
pub(crate) mod chrome;
pub(crate) mod geometry;
pub(crate) mod dock;
pub(crate) mod jump;
pub(crate) mod project;
pub(crate) mod sounds;
pub(crate) mod toast;

use crate::add_to_chat::ChatComposer;
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
use crate::git_ui::commit::Commits;
use crate::git_ui::diff::DiffState;
use crate::git_ui::graph::GraphState;
use crate::git_ui::pull_requests::PullRequests;
use crate::inbox::InboxState;
use crate::modals::pair_phone::PairPhone;
use crate::modals::{add_project, new_session};
use crate::palette::PaletteState;
use crate::panels::Panels;
use crate::removal::Removals;
use crate::settings::SettingsState;
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
use workspace::tree::PaneId;
use workspace::{Doc, Tab, Workspace};

/// The pane a worktree starts with.
pub(crate) const MAIN: PaneId = 0;

pub struct Desktop {
    pub(crate) daemon: Daemon,
    pub(crate) outbox: Outbox,
    pub(crate) alerts: Alerts,
    pub(crate) chime: Chime,
    pub(crate) badge: Badge,
    pub(crate) terminals: Terminals,
    pub(crate) creates: Creates,
    pub(crate) removals: Removals,
    pub(crate) agents: Agents,
    pub(crate) store: Store,
    pub(crate) project: Option<String>,
    pub(crate) screen: Screen,
    pub(crate) side: Side,
    pub(crate) layout: Layout,
    /// Indexed by `Column`; `None` keeps the design's width.
    pub(crate) widths: [Option<f32>; 2],
    pub(crate) workspaces: HashMap<String, Workspace>,
    pub(crate) repos: HashMap<String, Repo>,
    pub(crate) diff: DiffState,
    pub(crate) chat: ChatComposer,
    pub(crate) changes: ChangesState,
    pub(crate) prs: PullRequests,
    pub(crate) graph: GraphState,
    pub(crate) commit: Commits,
    pub(crate) terminal: TerminalViewState,
    pub(crate) panels: Panels,
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
    pub(crate) settings: SettingsState,
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
        let (chat, chat_subs) = ChatComposer::new(window, cx);
        let (changes, changes_subs) = ChangesState::new(window, cx);
        let (new_form, new_subs) = new_session::NewForm::new(window, cx);
        let (repo_form, repo_subs) = add_project::RepoForm::new(window, cx);
        let (geometry, geometry_subs) = Geometry::new(window, cx);
        let browsers = Browsers::new(window, cx);
        let root = cx.focus_handle();
        window.focus(&root, cx);
        let this = cx.weak_entity();
        window.on_window_should_close(cx, move |_, cx| this.update(cx, |d, cx| d.may_quit(cx)).unwrap_or(true));
        let mut _subs = vec![
            // The setting changes in System Settings, so the window coming back is when it may have.
            cx.observe_window_activation(window, |this, _, cx| follow_reduce_motion(this.store.appearance.reduce_motion, cx)),
            cx.observe_window_appearance(window, |this, window, cx| this.set_appearance(window.appearance(), window, cx)),
            // Unfocused, GPUI dispatches keys from the window's root node, above this view's action handlers.
            cx.on_focus_lost(window, |this, window, cx| window.focus(&this.root, cx)),
        ];
        _subs.extend(palette_subs);
        _subs.extend(sidebar_subs);
        _subs.extend(chips_subs);
        _subs.extend(chat_subs);
        _subs.extend(changes_subs);
        _subs.extend(new_subs);
        _subs.extend(repo_subs);
        _subs.extend(geometry_subs);
        let (layout, widths) = (store.layout, [store.widths.projects, store.widths.sessions]);
        Self {
            daemon,
            outbox,
            alerts: Alerts::new(),
            chime: Chime::new(),
            badge: Badge::default(),
            terminals: Terminals::new(),
            creates: Creates::default(),
            removals: Removals::default(),
            agents: Agents::default(),
            project: store.projects.first().cloned(),
            store,
            screen: Screen::Sessions,
            side: Side::Sessions,
            layout,
            widths,
            workspaces: HashMap::new(),
            repos: HashMap::new(),
            diff: DiffState::default(),
            chat,
            changes,
            prs: PullRequests::default(),
            graph: GraphState::default(),
            commit: Commits::default(),
            initials: String::new(),
            explorer: ExplorerState::new(),
            git_run: 0,
            git_done: 0,
            error: None,
            root,
            terminal: TerminalViewState::new(cx),
            panels: Panels::default(),
            browsers,
            inbox: InboxState::new(cx),
            palette,
            sidebar,
            settings: SettingsState::default(),
            chips,
            overlay: None,
            worktrees: HashMap::new(),
            worktree: None,
            confirm: None,
            row_menu: None,
            preview: PreviewState::default(),
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
        let w = self.workspace(&tree);
        if let Some((pane, i)) = w.tab_of(&term) {
            w.tree.select(pane, i);
        }
        self.load_active(cx);
        self.focus_pane(term, window, cx);
        self.save_soon(cx);
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
        self.worktree = None;
        for v in self.diff.panes.values_mut() {
            v.file = None;
        }
        for p in self.preview.panes.values_mut() {
            p.file = None;
        }
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
        cx.notify();
    }

    /// Shows tab `i` of `pane` and focuses the pane.
    pub fn select_tab(&mut self, pane: PaneId, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        self.workspace(&tree).tree.select(pane, i);
        match self.pane_tab(pane) {
            Some(Tab::Term(id)) => self.focus_pane(id, window, cx),
            Some(Tab::Doc(doc)) => self.show_doc(pane, doc, cx),
            Some(Tab::Web(_)) => window.focus(&self.browsers.focus, cx),
            None => {}
        }
        self.save_soon(cx);
        cx.notify();
    }

    /// The pane keys and actions go to.
    pub(crate) fn focused_pane(&self) -> PaneId {
        self.cwd().and_then(|t| self.workspaces.get(&t)).map_or(MAIN, |w| w.tree.focused)
    }

    pub(crate) fn pane_ids(&self) -> Vec<PaneId> {
        self.cwd().and_then(|t| self.workspaces.get(&t)).map_or(vec![MAIN], |w| w.tree.panes().iter().map(|p| p.id).collect())
    }

    /// The tab `pane` shows in the worktree on screen.
    pub(crate) fn pane_tab(&self, pane: PaneId) -> Option<Tab> {
        self.workspaces.get(&self.cwd()?)?.tree.pane(pane)?.active().cloned()
    }

    /// The file or changes the focused pane shows.
    pub fn active_doc(&self) -> Option<Doc> {
        match self.pane_tab(self.focused_pane())? {
            Tab::Doc(doc) => Some(doc),
            Tab::Term(_) | Tab::Web(_) => None,
        }
    }

    /// Opens `doc` in its tab of the worktree on screen; unless `pin`, in the preview tab the next open takes over.
    /// With no pane holding docs it opens in a new pane right of the focused one, or in the focused one when that has no room to split.
    pub fn open_doc(&mut self, doc: Doc, pin: bool, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        self.screen = Screen::Sessions;
        let bounds = self.panels.bounds;
        let w = self.workspace(&tree);
        let pane = w.doc_pane(bounds);
        let (pane, _) = w.open_doc(doc.clone(), pin, pane);
        self.show_doc(pane, doc, cx);
        self.save_soon(cx);
    }

    pub(crate) fn pin_doc(&mut self, doc: &Doc, cx: &mut Context<Self>) {
        if let Some(tree) = self.cwd() {
            self.workspace(&tree).pin(doc);
            self.save_soon(cx);
        }
    }

    pub(crate) fn close_doc(&mut self, doc: &Doc, cx: &mut Context<Self>) {
        if let Some((pane, i)) = self.cwd().and_then(|t| self.workspaces.get(&t)?.doc_tab(doc)) {
            self.close_tab(pane, i, cx);
        }
    }

    fn show_doc(&mut self, pane: PaneId, doc: Doc, cx: &mut Context<Self>) {
        match doc {
            Doc::File(path) => {
                self.preview.open(pane, path);
                self.load_file(pane, cx);
            }
            Doc::Diff(path) => {
                self.diff.select(pane, path, None);
                self.load_diff(pane, cx);
            }
            Doc::CommitFile { sha, path } => {
                self.diff.select(pane, path, Some(sha));
                self.load_diff(pane, cx);
            }
            Doc::Commit(sha) => self.show_commit(pane, sha, cx),
        }
        cx.notify();
    }

    pub fn loaded(&self, pane: PaneId, doc: &Doc) -> bool {
        let diff = self.diff.view(pane);
        match doc {
            Doc::File(p) => self.preview.file(pane) == Some(p.as_str()),
            Doc::Diff(p) => diff.is_some_and(|v| v.shows(p, None)),
            Doc::CommitFile { sha, path } => diff.is_some_and(|v| v.shows(path, Some(sha.as_str()))),
            Doc::Commit(sha) => self.commit.get(pane).is_some_and(|c| c.sha.as_ref() == Some(sha)),
        }
    }

    /// Drops what closed panes showed, and loads the docs that tabs revealed by closing or switching worktrees show, unless they are loaded already.
    pub(crate) fn load_active(&mut self, cx: &mut Context<Self>) {
        let panes = self.pane_ids();
        self.diff.retain_panes(&panes);
        self.commit.retain_panes(&panes);
        self.preview.retain_panes(&panes);
        self.browsers.retain_panes(&panes);
        for pane in panes {
            if let Some(Tab::Doc(doc)) = self.pane_tab(pane)
                && !self.loaded(pane, &doc)
            {
                self.show_doc(pane, doc, cx);
            }
        }
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.panels.menu.is_some() || self.panels.actions.is_some() || self.row_menu.is_some() || self.changes.commit_menu || self.changes.menu || self.sidebar.picker.open
    }

    /// Returns whether a menu was open.
    pub(crate) fn close_menus(&mut self) -> bool {
        let open = self.menu_open();
        (self.panels.menu, self.panels.actions, self.row_menu, self.changes.commit_menu, self.changes.menu) = (None, None, None, false, false);
        self.sidebar.menu_at = None;
        self.sidebar.picker.open = false;
        open
    }

    /// The worktree whose panes the main view shows.
    pub(crate) fn session_tree(&self) -> Option<String> {
        if self.terminals.link.is_down() || self.screen != Screen::Sessions {
            return None;
        }
        self.cwd()
    }

    fn main_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let body = match self.session_tree() {
            _ if self.shown_create().is_some() => self.creating_page(cx),
            Some(tree) => self.panels_view(&tree, window, cx),
            None if self.terminals.link.is_down() => self.link_page(cx),
            None if matches!(self.screen, Screen::Inbox) => self.inbox_detail(cx),
            None => self.blank_page(cx),
        };
        ui::page(div()).relative().flex_1().min_w_0().h_full().flex().flex_col().overflow_hidden().child(body).children(self.error_toast(cx)).children(self.deleting_toast()).children(self.added_toast())
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
        let (pad, toggle) = self.bar_start(12., cx);
        let bar = chrome::drag_area(ui::page_bar())
            .pl(px(pad))
            .children(toggle)
            .child(ui::breadcrumb(vec!["Sessions".into()]));
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
        self.sync_panels();
        self.sync_code(window, cx);
        self.sync_view(window, cx);
        self.sync_cursor(window, cx);
        self.sync_browser(window);
        let settings = self.screen == Screen::Settings;
        let lead = match self.layout {
            _ if settings => None,
            Layout::Sidebars => Some(self.aside(cx)),
            Layout::Compact => Some(self.nav(cx)),
            Layout::Focus => None,
        };
        let column = if settings { Some(self.settings_nav(cx)) } else { crate::sidebar::column_shown(self.layout, self.screen, self.sidebar.column_hidden).then(|| self.column_view(cx)) };
        let page = if settings { self.settings_page(cx) } else { self.main_view(window, cx) };
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
                } else if this.overlay.is_none() && this.screen == Screen::Settings {
                    this.close_settings(window, cx);
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
            .on_action(cx.listener(Self::open_settings))
            .on_action(cx.listener(Self::next_needs_you))
            .on_action(cx.listener(Self::go_to_up_next))
            .on_action(cx.listener(Self::jump_to))
            .on_action(cx.listener(Self::next_session))
            .on_action(cx.listener(Self::prev_session))
            .on_modifiers_changed(cx.listener(Self::on_modifiers))
            .on_action(cx.listener(Self::toggle_rail))
            .on_action(cx.listener(Self::toggle_sidebar))
            .on_action(cx.listener(Self::toggle_focus))
            .on_action(cx.listener(Self::new_tab))
            .on_action(cx.listener(Self::new_browser))
            .on_action(cx.listener(Self::close_active_tab))
            .on_action(cx.listener(Self::split_right))
            .on_action(cx.listener(Self::split_down))
            .on_action(cx.listener(Self::focus_toward))
            .on_action(cx.listener(Self::prev_tab))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::zoom_pane))
            .on_action(cx.listener(Self::equalize_panes))
            .on_action(cx.listener(Self::save))
            .on_action(cx.listener(Self::quit))
            .on_action(cx.listener(Self::open_selected))
            .on_action(cx.listener(Self::add_to_chat))
            .children(lead)
            .children(column)
            .child(page)
            .children(overlay)
            .children(self.chat_pill(cx))
            .children(self.chat_popover(cx))
    }
}

/// GPUI leaves `reduce_motion` to the app; this sets the user's choice, or mirrors the macOS setting when `pref` is `None`.
pub(crate) fn follow_reduce_motion(pref: Option<bool>, cx: &mut App) {
    #[cfg(target_os = "macos")]
    cx.set_reduce_motion(crate::settings::reduce_motion(pref, || objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()));
}
