pub(crate) mod alerts;
pub(crate) mod chrome;
pub(crate) mod close;
pub(crate) mod dock;
pub(crate) mod geometry;
pub(crate) mod jump;
pub(crate) mod locals;
pub(crate) mod pages;
pub(crate) mod project;
pub(crate) mod quit;
pub(crate) mod sounds;
pub(crate) mod toast;

use crate::accounts::Accounts;
use crate::activity::Activity;
use crate::add_to_chat::ChatComposer;
use crate::automations::Automations;
use crate::board::Board;
use crate::browser::Browsers;
use crate::chat::ChatState;
use crate::creating::Creates;
use crate::desktop::alerts::Alerts;
use crate::desktop::chrome::{Confirm, Layout, Overlay, RowMenu, Screen, Side};
use crate::desktop::dock::Badge;
use crate::desktop::geometry::Geometry;
use crate::desktop::jump::Chips;
use crate::desktop::sounds::Chime;
use crate::explorer::Explorer;
use crate::explorer::preview::Previews;
use crate::fleet::Fleet;
use crate::git_ui::changes::Changes;
use crate::git_ui::commit::Commits;
use crate::git_ui::diff::{At, DiffState};
use crate::git_ui::gh_accounts::GhAccounts;
use crate::git_ui::graph::Graph;
use crate::git_ui::pull_request::PullRequest;
use crate::git_ui::pull_requests::PullRequests;
use crate::host_agents::HostAgents;
use crate::inbox::Inbox;
use crate::modals::pair_phone::PairPhone;
use crate::modals::{add_project, clone_project, new_session};
use crate::models::Models;
use crate::notch::Notch;
use crate::open_in::OpenIn;
use crate::palette::Palette;
use crate::panels::Panels;
use crate::removal::Removals;
use crate::renames::Renames;
use crate::settings::Settings;
use crate::sidebar::Sidebar;
use crate::spaces::Spaces;
use crate::star::Star;
use crate::terminal_view::TerminalViewState;
use crate::terminals::Terminals;
use crate::terminals::link::{Hint, Update};
use crate::themes::Themes;
use crate::updates::Updates;
use crate::welcome::Welcome;
use agents::Agents;
use git::Repo;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use hosts::session::{Daemons, Outboxes};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use store::Store;
use theme::*;
use workspace::tree::PaneId;
use workspace::{Doc, Tab, Workspaces};

/// The pane a worktree starts with.
pub(crate) const MAIN: PaneId = 0;

pub struct Desktop {
    pub(crate) daemon: Daemons,
    pub(crate) outbox: Outboxes,
    pub(crate) alerts: Alerts,
    pub(crate) notch: Notch,
    pub(crate) chime: Chime,
    pub(crate) badge: Badge,
    pub(crate) terminals: Entity<Terminals>,
    pub(crate) creates: Entity<Creates>,
    pub(crate) removals: Removals,
    pub(crate) renames: Renames,
    pub(crate) agents: Agents,
    pub(crate) models: Models,
    pub(crate) store: Store,
    pub(crate) themes: Themes,
    pub(crate) project: Option<String>,
    pub(crate) screen: Screen,
    pub(crate) side: Side,
    pub(crate) layout: Layout,
    /// Indexed by `Column`; `None` keeps the design's width.
    pub(crate) widths: [Option<f32>; 2],
    pub(crate) workspaces: Entity<Workspaces>,
    pub(crate) repos: HashMap<String, Arc<Repo>>,
    pub(crate) diff: DiffState,
    pub(crate) chat: Entity<ChatComposer>,
    pub(crate) chats: ChatState,
    pub(crate) changes: Entity<Changes>,
    pub(crate) prs: PullRequests,
    pub(crate) gh_accounts: GhAccounts,
    pub(crate) pr: PullRequest,
    pub(crate) graph: Entity<Graph>,
    pub(crate) commit: Commits,
    pub(crate) terminal: TerminalViewState,
    pub(crate) panels: Entity<Panels>,
    pub(crate) open_in: Entity<OpenIn>,
    pub(crate) browsers: Browsers,
    pub(crate) explorer: Entity<Explorer>,
    pub(crate) git_run: u64,
    pub(crate) git_done: u64,
    pub(crate) inbox: Entity<Inbox>,
    pub(crate) activity: Entity<Activity>,
    pub(crate) handoff: Entity<crate::handoff::HandoffState>,
    pub(crate) automations: Entity<Automations>,
    pub(crate) board: Entity<Board>,
    pub(crate) error: Option<String>,
    pub(crate) root: FocusHandle,
    pub(crate) palette: Entity<Palette>,
    pub(crate) sidebar: Entity<Sidebar>,
    pub(crate) spaces: Spaces,
    pub(crate) settings: Entity<Settings>,
    pub(crate) fleet: Fleet,
    pub(crate) host_agents: HostAgents,
    pub(crate) registry: crate::registry::Registry,
    pub(crate) accounts: Accounts,
    pub(crate) updates: Updates,
    pub(crate) star: Entity<Star>,
    pub(crate) welcome: Option<Entity<Welcome>>,
    pub(crate) chips: Chips,
    pub(crate) delegated: crate::delegated::DelegatedState,
    pub(crate) overlay: Option<Overlay>,
    pub(crate) worktrees: HashMap<String, Vec<git::Worktree>>,
    pub(crate) tree_lookups: ::project::Lookups,
    pub(crate) worktree: Option<String>,
    pub(crate) confirm: Entity<crate::modals::confirm::ConfirmSheet>,
    pub(crate) row_menu: Option<RowMenu>,
    pub(crate) preview: Entity<Previews>,
    pub(crate) new_form: Entity<new_session::NewForm>,
    pub(crate) repo_form: Entity<add_project::RepoForm>,
    pub(crate) host_finder: Entity<crate::modals::host_finder::HostFinder>,
    pub(crate) clone_form: Entity<clone_project::CloneForm>,
    pub(crate) pair: Entity<PairPhone>,
    pub(crate) capturing: bool,
    pub(crate) geometry: Geometry,
    pub(crate) _subs: Vec<Subscription>,
}

impl Desktop {
    pub(crate) fn new(daemon: Daemons, outbox: Outboxes, mut store: Store, themes: Themes, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let palette = cx.new(|cx| Palette::new(window, cx));
        let palette_subs = cx.subscribe_in(&palette, window, Self::on_palette_event);
        let graph = cx.new(|_| Graph::new(store.graph_share, &store.diff));
        let graph_subs = cx.subscribe_in(&graph, window, Self::on_graph_event);
        let sidebar = cx.new(|cx| Sidebar::new(store.general.panel_open, window, cx));
        let sidebar_subs = Self::sidebar_subs(&sidebar, window, cx);
        let spaces = Spaces::new(store.sidebar.space.as_deref());
        let settings = cx.new(|cx| Settings::new(&store, window, cx));
        let settings_subs = [cx.subscribe_in(&settings, window, Self::on_settings_event), cx.observe(&settings, |_, _, cx| cx.notify())];
        let (fleet, fleet_subs) = Fleet::new(window, cx);
        let (chips, chips_subs) = Chips::new(window, cx);
        let chat = cx.new(|cx| ChatComposer::new(window, cx));
        let chat_subs = [cx.subscribe_in(&chat, window, Self::on_chat_event), cx.observe(&chat, |_, _, cx| cx.notify())];
        let (chats, chats_subs) = ChatState::new(window, cx);
        let workspaces = cx.new(|_| Workspaces::default());
        let creates = cx.new(|_| Creates::default());
        let panels = cx.new(|cx| Panels::new(workspaces.clone(), creates.clone(), cx));
        let panels_subs = [cx.subscribe_in(&panels, window, Self::on_panels_event), cx.observe(&panels, |_, _, cx| cx.notify())];
        let changes = cx.new(|cx| Changes::new(graph.clone(), window, cx));
        let changes_subs = cx.subscribe_in(&changes, window, Self::on_changes_event);
        let explorer = cx.new(|_| Explorer::new());
        let preview = cx.new(|_| Previews::new());
        let preview_subs = cx.subscribe(&preview, Self::on_preview_event);
        let explorer_subs = cx.subscribe_in(&explorer, window, Self::on_explorer_event);
        let (pr, pr_subs) = PullRequest::new(window, cx);
        let new_form = cx.new(|cx| new_session::NewForm::new(window, cx));
        let new_subs = Self::watch_new_form(&new_form, window, cx);
        let repo_form = cx.new(|cx| add_project::RepoForm::new(window, cx));
        let repo_subs = cx.subscribe_in(&repo_form, window, Self::on_repo_event);
        let host_finder = cx.new(|cx| crate::modals::host_finder::HostFinder::new(cx));
        let host_finder_subs = cx.subscribe_in(&host_finder, window, Self::on_finder_event);
        let confirm = cx.new(|_| crate::modals::confirm::ConfirmSheet::default());
        let confirm_subs = cx.subscribe_in(&confirm, window, Self::on_confirm_event);
        let clone_form = cx.new(|cx| clone_project::CloneForm::new(window, cx));
        let clone_subs = cx.subscribe_in(&clone_form, window, Self::on_clone_event);
        let (geometry, geometry_subs) = Geometry::new(window, cx);
        let automations = cx.new(|cx| Automations::new(window, cx));
        let automations_subs = Self::watch_automations(&automations, window, cx);
        let board_start = Self::board_start_hook(cx);
        let board = cx.new(|cx| Board::new(window, cx));
        let board_subs = cx.subscribe_in(&board, window, Self::on_board_event);
        let activity = cx.new(|cx| Activity::new(window, cx));
        let activity_subs = cx.subscribe_in(&activity, window, Self::on_activity_event);
        let handoff = cx.new(|cx| crate::handoff::HandoffState::new(window, cx));
        let handoff_subs = cx.subscribe_in(&handoff, window, Self::on_handoff_event);
        let pair = cx.new(|_| PairPhone::default());
        let pair_subs = cx.subscribe_in(&pair, window, Self::on_pair_event);
        let notch = Notch::new(window, cx);
        let browsers = Browsers::new(window, cx);
        // Read before updates and the star record this launch.
        let (welcome, welcome_subs) = match crate::welcome::greets(&store).then(|| Self::greet(window, cx)) {
            Some((welcome, subs)) => (Some(welcome), Some(subs)),
            None => (None, None),
        };
        let updates = Updates::new(&mut store, cx);
        let star = cx.new(|_| Star::new(&mut store, &updates.update, crate::star::unix_now()));
        let star_subs = cx.subscribe_in(&star, window, Self::on_star_event);
        let home = store.home().to_path_buf();
        let open_in = cx.new(|cx| OpenIn::new(&home, cx));
        let open_in_subs = cx.subscribe_in(&open_in, window, Self::on_open_in_event);
        let creates_subs = cx.subscribe_in(&creates, window, Self::on_creates_event);
        let inbox = cx.new(Inbox::new);
        let inbox_subs = cx.subscribe_in(&inbox, window, Self::on_inbox_event);
        let terminals = cx.new(|_| Terminals::new(crate::terminals::sessions::config(&store.terminal)));
        let terminals_subs = [
            cx.subscribe(&terminals, |this, _, ev: &crate::terminals::TerminalsEvent, _| this.on_terminals_event(ev)),
            cx.observe(&terminals, |_, _, cx| cx.notify()),
        ];
        let root = cx.focus_handle();
        window.focus(&root, cx);
        crate::desktop::chrome::apply_zoom(store.appearance.zoom_factor(), window);
        let this = cx.weak_entity();
        // The window stays open either way: quitting closes it.
        window.on_window_should_close(cx, move |window, cx| this.update(cx, |d, cx| d.close_window(window, cx)).is_err());
        let mut _subs = vec![
            // The setting changes in System Settings, so the window coming back is when it may have.
            cx.observe_window_activation(window, |this, window, cx| {
                follow_reduce_motion(this.store.appearance.reduce_motion, cx);
                follow_reduce_transparency(cx);
                // Render tells alerts which panes are seen, but a hidden window renders no frame.
                this.sync_view(window, cx);
                if this.screen == Screen::Settings {
                    this.settings.update(cx, |s, cx| s.load_banner_permission(cx));
                }
                if window.is_window_active() {
                    this.reload_themes(cx);
                }
            }),
            cx.observe_window_appearance(window, |this, window, cx| this.set_appearance(window.appearance(), window, cx)),
            // Unfocused, GPUI dispatches keys from the window's root node, above this view's action handlers.
            cx.on_focus_lost(window, |this, window, cx| window.focus(&this.root, cx)),
        ];
        _subs.push(Self::confirm_on_enter(cx));
        _subs.extend(welcome_subs);
        _subs.push(palette_subs);
        _subs.push(graph_subs);
        _subs.push(star_subs);
        _subs.push(open_in_subs);
        _subs.push(inbox_subs);
        _subs.push(creates_subs);
        _subs.extend(sidebar_subs);
        _subs.extend(settings_subs);
        _subs.extend(fleet_subs);
        _subs.extend(chips_subs);
        _subs.extend(chat_subs);
        _subs.extend(chats_subs);
        _subs.extend(panels_subs);
        _subs.extend(terminals_subs);
        _subs.push(changes_subs);
        _subs.push(explorer_subs);
        _subs.push(preview_subs);
        _subs.extend(pr_subs);
        _subs.extend(new_subs);
        _subs.push(repo_subs);
        _subs.push(confirm_subs);
        _subs.push(host_finder_subs);
        _subs.push(clone_subs);
        _subs.extend(geometry_subs);
        _subs.extend(automations_subs);
        _subs.push(board_start);
        _subs.push(board_subs);
        _subs.push(activity_subs);
        _subs.push(handoff_subs);
        _subs.push(pair_subs);
        let (layout, widths) = (store.general.launch_layout.unwrap_or(store.layout), [store.widths.projects, store.widths.sessions]);
        let diff = DiffState::new(&store.diff);
        let registry = crate::registry::Registry::new(&store);
        Self {
            daemon,
            outbox,
            alerts: Alerts::new(),
            notch,
            chime: Chime::new(),
            badge: Badge::default(),
            terminals,
            themes,
            creates,
            removals: Removals::default(),
            renames: Renames::default(),
            agents: Agents::default(),
            models: Models::default(),
            project: store.projects.first().cloned(),
            store,
            screen: Screen::Sessions,
            side: Side::Sessions,
            layout,
            widths,
            workspaces,
            repos: HashMap::new(),
            diff,
            chat,
            chats,
            changes,
            prs: PullRequests::default(),
            gh_accounts: GhAccounts::default(),
            pr,
            graph,
            commit: Commits::default(),
            explorer,
            git_run: 0,
            git_done: 0,
            error: None,
            root,
            terminal: TerminalViewState::new(cx),
            panels,
            open_in,
            browsers,
            inbox,
            activity,
            handoff,
            automations,
            board,
            palette,
            sidebar,
            spaces,
            settings,
            fleet,
            host_agents: HostAgents::default(),
            registry,
            accounts: Accounts::default(),
            updates,
            star,
            welcome,
            chips,
            delegated: crate::delegated::DelegatedState::new(),
            overlay: None,
            worktrees: HashMap::new(),
            tree_lookups: ::project::Lookups::default(),
            worktree: None,
            confirm,
            row_menu: None,
            preview,
            new_form,
            repo_form,
            host_finder,
            clone_form,
            pair,
            capturing: false,
            geometry,
            _subs,
        }
    }

    /// Shows an agent's session: its worktree, the tab holding its terminal, and that pane focused. A delegated session has no tab, so it shows in its chat's.
    pub fn focus_agent(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let id = &self.delegated_show(id);
        let Some(term) = self.agents.get(id).map(|a| a.terminal_id.clone()) else { return };
        let Some(tree) = self.terminals.read(cx).sessions.get(&term).and_then(|s| self.place_of(&s.info, cx)) else { return };
        self.show_tree(&tree, cx);
        self.side = Side::Sessions;
        self.workspace(&tree, cx, |w| {
            if let Some((pane, i)) = w.show_term(&term) {
                w.tree.select(pane, i);
            }
        });
        self.load_active(cx);
        self.focus_pane(term, window, cx);
        self.save_soon(cx);
        self.refresh_git(cx);
        self.load_shown_repo(cx);
        self.load_explorer_root(cx);
    }

    pub(crate) fn set_appearance(&mut self, appearance: WindowAppearance, window: &mut Window, cx: &mut Context<Self>) {
        self.paint_theme(theme::dark(appearance), cx);
        window.set_background_appearance(theme::window_background());
    }

    pub(crate) fn show_tree(&mut self, tree: &str, cx: &mut Context<Self>) {
        if let Some(p) = self.folder_of(tree, cx).and_then(|cwd| self.project_of(&cwd, &self.projects(cx)).cloned()) {
            self.follow_space(&p, cx);
            self.project = Some(p);
        }
        self.screen = Screen::Sessions;
        self.new_form.update(cx, |f, _| f.shown = false);
        self.worktree = Some(tree.to_string());
    }

    pub fn select_project(&mut self, p: String, cx: &mut Context<Self>) {
        let tree = self.worktree.clone().filter(|_| self.project.as_ref() == Some(&p));
        self.select_tree(p, tree, cx);
    }

    fn set_project(&mut self, p: Option<String>, cx: &mut Context<Self>) {
        if let Some(p) = &p {
            self.follow_space(p, cx);
        }
        self.project = p;
        self.worktree = None;
        for s in self.diff.panes.values() {
            s.view.update(cx, |p, _| p.view.file = None);
        }
        self.preview.update(cx, |p, _| p.state.panes.values_mut().for_each(|f| f.file = None));
        self.explorer.update(cx, |e, _| e.clear_tree());
    }

    /// Shows worktree `tree` of project `p`; `None` is the project's main worktree.
    pub fn select_tree(&mut self, p: String, tree: Option<String>, cx: &mut Context<Self>) {
        let moved = self.project.as_ref() != Some(&p);
        // After `set_project`, whose space switch reads whether the new-session page was up.
        if moved {
            self.set_project(Some(p), cx);
        }
        self.screen = Screen::Sessions;
        self.new_form.update(cx, |f, _| f.shown = false);
        if moved || self.worktree != tree {
            self.worktree = tree;
            self.refresh_git(cx);
            self.load_shown_repo(cx);
            self.load_explorer_root(cx);
            self.load_active(cx);
        }
        cx.notify();
    }

    /// Shows tab `i` of `pane` and focuses the pane.
    pub fn select_tab(&mut self, pane: PaneId, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.place(cx) else { return };
        self.workspace(&tree, cx, |w| w.tree.select(pane, i));
        match self.pane_tab(pane, cx) {
            Some(Tab::Term(id)) => {
                if let Some(agent) = self.summary(&id).map(|a| a.id.clone()) {
                    self.delegated.leave(&agent);
                }
                self.focus_pane(id, window, cx)
            }
            Some(Tab::Doc(doc)) => self.show_doc(pane, doc, cx),
            Some(Tab::Web(_)) => window.focus(&self.browsers.focus, cx),
            None => {}
        }
        self.save_soon(cx);
        cx.notify();
    }

    /// The pane keys and actions go to.
    pub(crate) fn focused_pane(&self, cx: &App) -> PaneId {
        self.place(cx).and_then(|t| self.workspaces.read(cx).get(&t)).map_or(MAIN, |w| w.tree.focused)
    }

    pub(crate) fn pane_ids(&self, cx: &App) -> Vec<PaneId> {
        self.place(cx).and_then(|t| self.workspaces.read(cx).get(&t)).map_or(vec![MAIN], |w| w.tops().iter().flat_map(|t| t.panes()).map(|p| p.id).collect())
    }

    /// The tab `pane` shows in the worktree on screen.
    pub(crate) fn pane_tab(&self, pane: PaneId, cx: &App) -> Option<Tab> {
        self.workspaces.read(cx).get(&self.place(cx)?)?.tree.pane(pane)?.active().cloned()
    }

    /// The file or changes the focused pane shows.
    pub fn active_doc(&self, cx: &App) -> Option<Doc> {
        match self.pane_tab(self.focused_pane(cx), cx)? {
            Tab::Doc(doc) => Some(doc),
            Tab::Term(_) | Tab::Web(_) => None,
        }
    }

    /// Opens `doc` in its tab of the worktree on screen; unless `pin`, in the preview tab the next open takes over.
    /// With no pane holding docs it opens in a new pane right of the focused one, or in the focused one when that has no room to split.
    pub fn open_doc(&mut self, doc: Doc, pin: bool, cx: &mut Context<Self>) {
        let Some(tree) = self.place(cx) else { return };
        self.screen = Screen::Sessions;
        self.new_form.update(cx, |f, _| f.shown = false);
        let bounds = self.panels.read(cx).bounds;
        let files = &self.store.files;
        let (pin, next) = (pin || !files.preview_tabs, files.new_tabs == store::prefs::files::NewTabs::AfterCurrent);
        let (pane, _) = self.workspace(&tree, cx, |w| {
            let pane = w.doc_pane(bounds);
            w.place_doc(doc.clone(), pin, pane, next)
        });
        self.show_doc(pane, doc, cx);
        self.save_soon(cx);
    }

    pub(crate) fn pin_doc(&mut self, doc: &Doc, cx: &mut Context<Self>) {
        if let Some(tree) = self.place(cx) {
            self.workspace(&tree, cx, |w| w.pin(doc));
            self.save_soon(cx);
        }
    }

    pub(crate) fn close_doc(&mut self, doc: &Doc, cx: &mut Context<Self>) {
        if let Some((pane, i)) = self.place(cx).and_then(|t| self.workspaces.read(cx).get(&t)?.doc_tab(doc)) {
            self.close_tab(pane, i, cx);
        }
    }

    fn show_doc(&mut self, pane: PaneId, doc: Doc, cx: &mut Context<Self>) {
        match doc {
            Doc::File(path) => {
                let source = self.store.files.markdown_source;
                self.preview.update(cx, |p, _| p.state.open(pane, path, source));
                self.load_file(pane, cx);
            }
            Doc::Diff(path) => {
                self.select_diff(pane, path, At::Working, cx);
                self.load_diff(pane, cx);
            }
            Doc::CommitFile { sha, path } => {
                self.select_diff(pane, path, At::Commit(sha), cx);
                self.load_diff(pane, cx);
            }
            Doc::PrFile { base, path } => {
                self.select_diff(pane, path, At::Base(base), cx);
                self.pin_threads(cx);
                self.load_diff(pane, cx);
            }
            Doc::CompareFile { against, path } => {
                self.select_diff(pane, path, At::Compare(against), cx);
                self.pin_threads(cx);
                self.load_diff(pane, cx);
            }
            Doc::Commit(sha) => self.show_commit(pane, sha, cx),
        }
        cx.notify();
    }

    pub fn loaded(&self, pane: PaneId, doc: &Doc, cx: &App) -> bool {
        let diff = self.diff.view(pane, cx);
        match doc {
            Doc::File(p) => self.preview.read(cx).file(pane) == Some(p.as_str()),
            Doc::Diff(p) => diff.is_some_and(|v| v.shows(p, &At::Working)),
            Doc::CommitFile { sha, path } => diff.is_some_and(|v| v.shows(path, &At::Commit(sha.clone()))),
            Doc::PrFile { base, path } => diff.is_some_and(|v| v.shows(path, &At::Base(base.clone()))),
            Doc::CompareFile { against, path } => diff.is_some_and(|v| v.shows(path, &At::Compare(against.clone()))),
            Doc::Commit(sha) => self.commit.shows(pane, sha, cx),
        }
    }

    /// Drops what closed panes showed, and loads the docs that tabs revealed by closing or switching worktrees show, unless they are loaded already.
    pub(crate) fn load_active(&mut self, cx: &mut Context<Self>) {
        let panes = self.pane_ids(cx);
        self.diff.retain_panes(&panes);
        self.commit.retain_panes(&panes);
        self.preview.update(cx, |p, _| p.retain_panes(&panes));
        self.browsers.retain_panes(&panes);
        for pane in panes {
            if let Some(Tab::Doc(doc)) = self.pane_tab(pane, cx)
                && !self.loaded(pane, &doc, cx)
            {
                self.show_doc(pane, doc, cx);
            }
        }
    }

    pub(crate) fn menu_open(&self, cx: &App) -> bool {
        self.board.read(cx).menu_open()
            || self.panels.read(cx).menu_open()
            || self.row_menu.is_some()
            || self.changes.read(cx).menu_open()
            || self.pr.ship_menu
            || self.pr.merge_menu
            || self.pr.create_menu
            || self.sidebar.read(cx).menu_open(cx)
            || self.automations.read(cx).menu.is_some()
            || self.explorer.read(cx).menu_open()
            || self.terminal.menu.is_some()
            || self.accounts.menu.is_some()
            || self.clone_form.read(cx).machine_menu
            || self.chats.files.is_some()
            || self.chats.select.read(cx).menu_open()
            || self.open_in.read(cx).menu.is_some()
            || self.activity.read(cx).menu.is_some()
    }

    /// Returns whether a menu was open.
    pub(crate) fn close_menus(&mut self, cx: &mut App) -> bool {
        let open = self.menu_open(cx);
        self.panels.update(cx, |p, _| p.close_menus());
        self.row_menu = None;
        self.changes.update(cx, |c, _| c.shut_menus());
        (self.pr.ship_menu, self.pr.merge_menu, self.pr.create_menu) = (false, false, false);
        self.sidebar.update(cx, |s, cx| s.shut_menus(cx));
        self.automations.update(cx, |a, _| a.menu = None);
        self.board.update(cx, |b, _| b.close_menu());
        self.activity.update(cx, |a, _| a.menu = None);
        self.accounts.menu = None;
        self.clone_form.update(cx, |f, _| f.close_menus());
        self.explorer.update(cx, |e, _| _ = e.close_menu());
        self.terminal.menu = None;
        self.chats.files = None;
        self.chats.select.update(cx, |s, _| s.close_menu());
        self.open_in.update(cx, |o, _| o.menu = None);
        open
    }

    /// Whether the main view shows panes with no overlay, menu or creating page over them.
    pub(crate) fn panes_on_screen(&self, cx: &App) -> bool {
        self.session_tree(cx).is_some() && self.shown_create(cx).is_none() && self.overlay.is_none() && !self.menu_open(cx)
    }

    /// The worktree whose panes the main view shows.
    pub(crate) fn session_tree(&self, cx: &App) -> Option<String> {
        if self.terminals.read(cx).link.is_down()
            || self.terminals.read(cx).link.update(Instant::now()).is_some()
            || self.screen != Screen::Sessions
            || self.new_form.read(cx).shown
        {
            return None;
        }
        self.place(cx)
    }

    /// A git project's main checkout on screen with nothing open in it, once the saved layouts are back: the new session page stands in for its empty panes.
    pub(crate) fn idle_main(&self, cx: &App) -> bool {
        let Some(p) = self.project.as_deref() else { return false };
        let Some(main) = self.tree_of(p, cx) else { return false };
        self.workspaces.read(cx).is_restored()
            && new_session::may_start(&self.agents)
            && self.worktrees.get(p).is_some_and(|w| !w.is_empty())
            && self.place(cx).as_ref() == Some(&main)
            && !self.in_use(&main, cx)
    }

    /// The provider's Label from Settings, else its own name.
    pub(crate) fn agent_name(&self, provider: &str) -> String {
        self.store.agents.label(provider).unwrap_or_else(|| theme::provider_name(provider).into())
    }

    fn main_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let now = Instant::now();
        // The hand-back ends on time even when another page covers it, since it keeps the panes hidden.
        if matches!(self.terminals.read(cx).link.update(now), Some(Update::Back(_))) {
            window.request_animation_frame();
        }
        let body = match self.session_tree(cx) {
            _ if self.shown_create(cx).is_some() => self.creating_page(window, cx),
            Some(_) if self.idle_main(cx) => {
                // Every way onto the main checkout ends here, and opening the page needs the window; re-checked since the frame may be stale by then.
                cx.defer_in(window, |this, window, cx| {
                    if let Some(p) = this.project.clone().filter(|_| this.idle_main(cx) && !this.new_form.read(cx).shown && this.overlay.is_none()) {
                        this.new_place(p, window, cx);
                    }
                });
                div()
            }
            Some(tree) => self.panels_view(&tree, window, cx),
            None if self.terminals.read(cx).link.is_down() || self.terminals.read(cx).link.update(now).is_some() => self.link_page(now, window, cx),
            None if self.new_form.read(cx).shown && self.screen == Screen::Sessions => self.new_session_page(cx),
            None => match self.screen_view(window, cx) {
                Some(page) => page,
                None => self.blank_page(cx),
            },
        };
        ui::page(div())
            .relative()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(body)
            .children(self.error_toast(cx))
            .children(self.deleting_toast(cx))
            .children(self.added_toast(cx))
            .children(self.tree_toast(cx))
            .children(self.pr_notice(cx))
    }

    fn link_page(&self, now: Instant, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let (hint, update, stale) = {
            let link = &self.terminals.read(cx).link;
            (link.hint(now), link.update(now), link.stale().unwrap_or_default().to_string())
        };
        let tip = match hint {
            None | Some(Hint::Stale) => Vec::new(),
            Some(Hint::Install) => {
                vec![div().child("Not starting? In Terminal:").into_any_element(), div().font_family(MONO).child("pocketd daemon install").into_any_element()]
            }
            Some(Hint::LoginItems) => vec![
                div().child("Anywhere's background service is off.").into_any_element(),
                ui::button("open-login-items", ui::Variant::Primary, None, "Open Login Items")
                    .mt(px(6.))
                    .on_click(|_, _, _| daemon::service::open_login_items())
                    .into_any_element(),
            ],
            Some(Hint::Reopen) => vec![div().child("Not starting? Quit and reopen Anywhere.").into_any_element()],
        };
        let body = match update {
            Some(update) => self.updating_page(update, &stale, tip, window, cx),
            None => div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .text_size(px(14.))
                .text_color(TEXT_3)
                .when(hint != Some(Hint::LoginItems), |d| d.child("Starting Anywhere's terminal service…"))
                .children(tip),
        };
        div().flex_1().flex().flex_col().child(self.page_bar(vec!["Sessions".into()], Vec::new(), div(), cx)).child(body)
    }

    pub(crate) fn relaunch_if_replaced(&mut self, cx: &mut Context<Self>) {
        let replaced = replaced_on_disk(cx);
        cx.spawn(async move |this, cx| {
            if replaced.await {
                this.update(cx, |d, cx| d.relaunch(cx)).ok();
            }
        })
        .detach();
    }

    pub(crate) fn relaunch_or_restart_service(&mut self, cx: &mut Context<Self>) {
        let replaced = replaced_on_disk(cx);
        cx.spawn(async move |this, cx| {
            if replaced.await {
                this.update(cx, |d, cx| {
                    if !d.relaunch(cx) {
                        let unsaved = d.preview.read(cx).drafts.len();
                        d.ask(Confirm::Relaunch(unsaved), cx);
                        cx.notify();
                    }
                })
                .ok();
            } else {
                this.update(cx, |d, cx| d.restart_service(cx)).ok();
            }
        })
        .detach();
    }

    /// Unsaved edits live only in memory, so a relaunch waits for them as quitting does.
    fn relaunch(&mut self, cx: &mut Context<Self>) -> bool {
        let clean = self.preview.read(cx).drafts.is_empty();
        if clean {
            cx.restart();
        }
        clean
    }

    pub(crate) fn restart_service(&mut self, cx: &mut Context<Self>) {
        cx.spawn(async |this, cx| {
            let result = cx.background_executor().spawn(async { daemon::service::restart() }).await;
            this.update(cx, |d, cx| {
                if let Err(e) = result {
                    d.error = Some(format!("Couldn't restart the service: {e}"));
                    cx.notify();
                }
            })
        })
        .detach();
    }

    fn blank_page(&self, cx: &mut Context<Self>) -> Div {
        let text = if self.project.is_none() { "Add a project to begin." } else { "Pick a session, or start a new one." };
        let observe = self.agents.observe_only();
        let (pad, toggle) = self.bar_start(12., cx);
        let bar = chrome::drag_area(ui::page_bar()).pl(px(pad)).children(toggle).child(ui::breadcrumb(vec!["Sessions".into()]));
        div().flex_1().flex().flex_col().child(bar).when(observe, |d| d.child(chrome::observe_banner())).child(
            div().flex_1().flex().flex_col().items_center().justify_center().gap(px(14.)).text_size(px(14.)).text_color(TEXT_3).child(text).when(
                self.project.is_some() && !observe,
                |d| {
                    d.child(
                        ui::button("blank-new", ui::Variant::Primary, Some("sparkle"), "New session")
                            .child(ui::button_kbd("⌘N"))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.new_session(None, false, window, cx))),
                    )
                },
            ),
        )
    }
}

/// Whether an update swapped another build of the app in on disk under this one, as a host update does.
fn replaced_on_disk(cx: &App) -> Task<bool> {
    cx.background_executor().spawn(async { crate::terminals::link::replaced(channel::version(), daemon::service::bundle_version().as_deref()) })
}

impl Render for Desktop {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.sync_panels(cx);
        // A capture draws one frame a step, which would catch a glide midway.
        let (now, capturing, reduce_motion) = (Instant::now(), self.capturing, cx.reduce_motion() || self.capturing);
        // Not `||`: each steps every frame.
        if self.sidebar.update(cx, |s, _| s.tick(self.layout, now, reduce_motion))
            | self.panels.update(cx, |p, _| p.tick(now, reduce_motion))
            | self.star.update(cx, |s, _| s.tick(now, capturing))
            | self.chats.dock.update(cx, |d, _| d.tick(now, reduce_motion))
        {
            window.request_animation_frame();
        }
        self.sync_view(window, cx);
        self.sync_browser(window, cx);
        let welcome = self.welcome_view(now, reduce_motion, window, cx);
        // The welcome hides the app until it fades, so the app isn't built, and what animates in it asks for no frames.
        let shown = !self.welcome.as_ref().is_some_and(|w| w.read(cx).covers());
        let settings = self.screen == Screen::Settings;
        if settings {
            self.sync_settings(cx);
        }
        let lead = (shown && !settings).then(|| self.lead(cx));
        let column = if !shown {
            None
        } else if settings {
            let width = self.width(crate::desktop::chrome::Column::Projects, 272.);
            Some(self.settings.update(cx, |s, cx| s.nav(width, cx)))
        } else {
            crate::sidebar::column_shown(self.layout, self.screen).then(|| self.column_view(cx))
        };
        let page = shown.then(|| if settings { self.settings.clone().into_any_element() } else { self.main_view(window, cx).into_any_element() });
        let workspace = (shown && self.screen == Screen::Sessions && self.sidebar.read(cx).unfocused() > 0.).then(|| self.workspace_side(cx));
        // The finder opened from the new-session page has no overlay under it.
        let overlay = if shown { self.overlay_view(cx).or_else(|| self.host_finder_view(cx)) } else { None };
        div()
            .relative()
            .size_full()
            .flex()
            .bg(theme::window_fill())
            .font_family(ui_font())
            .line_height(relative(1.2))
            .text_color(TEXT)
            .when(self.screen == Screen::Inbox, |d| d.key_context(crate::inbox::CONTEXT))
            .when(self.screen == Screen::Settings, |d| d.key_context(crate::settings::CONTEXT))
            .when(self.explorer.read(cx).menu_open(), |d| d.key_context(crate::explorer::menu::CONTEXT))
            .track_focus(&self.root)
            .capture_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                this.log_focus_miss(&ev.keystroke, window, cx); // [DEBUG-focus]
                this.hide_chips(cx);
                if crate::explorer::toast::is_undo(&ev.keystroke, || window.context_stack()) && this.undo_trash(cx) {
                    return cx.stop_propagation();
                }
                if this.type_tree_menu(&ev.keystroke, cx) {
                    return cx.stop_propagation();
                }
                if ev.keystroke.key != "escape" {
                    return;
                }
                if this.close_host_finder(cx)
                    || this.panels.update(cx, |p, cx| p.cancel_drag(cx))
                    || this.explorer.update(cx, |e, _| e.cancel_edit())
                    || this.sidebar.update(cx, |s, cx| s.cancel_rename(cx))
                    || this.new_form.update(cx, |f, cx| f.close_picker(cx))
                    || this.escape_composer_menu(cx)
                    || this.close_menus(cx)
                    || this.chat_escape_dock(cx)
                    || this.delegated_escape(window, cx)
                {
                    cx.notify();
                } else if this.overlay.is_some_and(|o| o != Overlay::Palette) {
                    this.close_overlay(window, cx);
                } else if this.overlay.is_none() && this.new_form.read(cx).shown && this.screen == Screen::Sessions && !this.idle_main(cx) {
                    this.leave_new_session(window, cx);
                } else if this.overlay.is_some() || !this.escape_screen(window, cx) {
                    return;
                }
                cx.stop_propagation();
            }))
            .on_action(cx.listener(Self::quit))
            // Bound shortcuts run before `capture_key_down`, so under the welcome the app's own have no handler.
            .when(shown, |root| {
                root.on_key_down(cx.listener(|this, ev: &KeyDownEvent, window, cx| {
                    if this.screen_key(&ev.keystroke, window, cx) || this.type_into_chat(&ev.keystroke, window, cx) {
                        cx.stop_propagation();
                    }
                }))
                .on_action(cx.listener(|this, _: &crate::actions::OpenPalette, window, cx| this.open(Overlay::Palette, window, cx)))
                .on_action(cx.listener(|this, _: &crate::actions::StartSession, window, cx| this.new_session(None, false, window, cx)))
                .on_action(cx.listener(|this, _: &crate::actions::PrevPrompt, _, cx| this.chat_step_prompt(false, cx)))
                .on_action(cx.listener(|this, _: &crate::actions::NextPrompt, _, cx| this.chat_step_prompt(true, cx)))
                .on_action(cx.listener(Self::go_to_file))
                .on_action(cx.listener(Self::new_worktree))
                .on_action(cx.listener(Self::project_settings))
                .on_action(cx.listener(Self::open_settings))
                .on_action(cx.listener(Self::focus_settings_search))
                .map(|root| pages::on_actions(root, cx))
                .on_action(cx.listener(Self::check_for_updates))
                .on_action(cx.listener(Self::next_needs_you))
                .on_action(cx.listener(Self::go_to_up_next))
                .on_action(cx.listener(|this, _: &crate::actions::BackToParent, window, cx| {
                    if !this.delegated_back(window, cx) {
                        cx.propagate();
                    }
                }))
                .on_action(cx.listener(Self::jump_to))
                .on_action(cx.listener(Self::switch_to_nth_space))
                .on_action(cx.listener(Self::next_session))
                .on_action(cx.listener(Self::prev_session))
                .on_modifiers_changed(cx.listener(Self::on_modifiers))
                .on_action(cx.listener(Self::toggle_rail))
                .on_action(cx.listener(Self::toggle_sidebar))
                .on_action(cx.listener(Self::toggle_focus))
                .on_action(cx.listener(Self::zoom_in))
                .on_action(cx.listener(Self::zoom_out))
                .on_action(cx.listener(Self::reset_zoom))
                .on_action(cx.listener(Self::new_tab))
                .on_action(cx.listener(Self::new_browser))
                .on_action(cx.listener(Self::close_active_tab))
                .on_action(cx.listener(Self::split_right))
                .on_action(cx.listener(Self::split_down))
                .on_action(cx.listener(Self::focus_toward))
                .on_action(cx.listener(Self::prev_tab))
                .on_action(cx.listener(Self::next_tab))
                .on_action(cx.listener(Self::jump_to_tab))
                .on_action(cx.listener(Self::zoom_pane))
                .on_action(cx.listener(Self::equalize_panes))
                .on_action(cx.listener(Self::save))
                .on_action(cx.listener(Self::open_selected))
                .on_action(cx.listener(Self::add_to_chat))
                .on_action(cx.listener(Self::tree_menu_item))
                .on_action(cx.listener(Self::tree_menu_arrow))
                .on_action(cx.listener(Self::tree_menu_choose))
                .on_action(cx.listener(Self::copy_worktree_path))
            })
            .children(lead)
            .children(column)
            .children(page)
            .children(workspace)
            .children(overlay)
            .children(shown.then(|| self.chat_pill(cx)).flatten())
            .children(shown.then(|| self.chat_popover(cx)).flatten())
            .children(shown.then(|| self.chat_select_offer(cx)).flatten())
            .children(welcome)
    }
}

/// Whether macOS is set to reduce transparency, which floating glass answers with a solid fill.
pub(crate) struct ReduceTransparency(pub(crate) bool);

impl Global for ReduceTransparency {}

pub(crate) fn follow_reduce_transparency(cx: &mut App) {
    #[cfg(target_os = "macos")]
    cx.set_global(ReduceTransparency(objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()));
    #[cfg(not(target_os = "macos"))]
    cx.set_global(ReduceTransparency(false));
}

/// GPUI leaves `reduce_motion` to the app; this sets the user's choice, or mirrors the macOS setting when `pref` is `None`.
pub(crate) fn follow_reduce_motion(pref: Option<bool>, cx: &mut App) {
    #[cfg(target_os = "macos")]
    cx.set_reduce_motion(crate::settings::reduce_motion(pref, || objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()));
    #[cfg(not(target_os = "macos"))]
    cx.set_reduce_motion(crate::settings::reduce_motion(pref, || false));
}
