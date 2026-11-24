use super::tab_face::Lead;
use super::top_bar::Face;
use super::{Head, Panels, PanelsEvent, head};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, observe_banner};
use crate::explorer::preview::relative;
use crate::open_in::OpenIn;
use gpui_kit::*;
use std::collections::HashMap;
use workspace::tree::{Pane, PaneId};
use workspace::{Doc, Place, Tab};

/// What `Desktop` hands the panels each frame for the worktree on screen: what they draw that they can't read for themselves.
pub(crate) struct Frame {
    pub(super) tree: String,
    pub(super) bar: BarFrame,
    pub(super) banner: Option<Div>,
    /// The held tab, drawn under the pointer while it is off its strip.
    pub(super) ghost: Option<Lead>,
    pub(super) draw: Drawing,
}

/// What the panes and their menus draw.
pub(crate) struct Drawing {
    pub(super) panes: HashMap<PaneId, Dress>,
    /// Whether the seams between panes may be dragged: not under an overlay, which they would steal clicks from.
    pub(super) seams: bool,
    pub(super) menus: Menus,
}

/// The bar over the panes: its tabs and what stands beside them.
pub(crate) struct BarFrame {
    pub(super) at: usize,
    pub(super) closable: bool,
    pub(super) tabs: Vec<Face>,
    pub(super) pad: f32,
    pub(super) toggle: Option<Stateful<Div>>,
    pub(super) open_in: Option<Entity<OpenIn>>,
}

/// What focuses a pane on a press.
pub(crate) type Press = Box<dyn Fn(&MouseDownEvent, &mut Window, &mut App)>;

/// A drawn pane's body, and what its head says.
pub(crate) struct Dress {
    pub(super) body: Body,
    pub(super) tabs: Vec<TabFace>,
    pub(super) title: String,
    /// The agent whose chat the pane's head is rising into.
    pub(super) arriving: Option<String>,
    /// Focuses the pane before the press reaches what is under it.
    pub(super) press: Press,
}

pub(crate) enum Body {
    Page(AnyElement),
    Empty(Empty),
}

/// What a pane with no tabs offers.
pub(crate) enum Empty {
    /// Pocketd is managed elsewhere: it offers nothing.
    Observing,
    /// The agent that opens by default, by provider and name, if any is installed.
    Offers(Option<(&'static str, String)>),
}

pub(crate) struct TabFace {
    pub(super) lead: Lead,
    pub(super) closable: bool,
}

/// The open menus' rows, for the menus that are open.
#[derive(Default)]
pub(crate) struct Menus {
    pub(super) new_tab: Option<NewTab>,
    pub(super) actions: Option<TabFile>,
    pub(super) top: Option<TopActions>,
}

pub(crate) struct NewTab {
    pub(super) branch: String,
    pub(super) agents: Vec<AgentRow>,
}

pub(crate) struct AgentRow {
    pub(super) provider: &'static str,
    pub(super) name: String,
    pub(super) hint: Option<String>,
}

/// The file a right-clicked tab shows: its path and its path from the explorer's root.
#[derive(Default)]
pub(crate) struct TabFile(pub(super) Option<(String, String)>);

pub(crate) struct TopActions {
    pub(super) agent: String,
    pub(super) fork: bool,
    pub(super) handoff: Option<&'static str>,
}

impl Desktop {
    /// Worktree `tree`'s panes: all of them, or only the zoomed one.
    pub(crate) fn panels_view(&mut self, tree: &str, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let shown: Vec<Pane<Tab>> = {
            let t = &self.workspaces.read(cx)[tree].tree;
            match t.zoomed.and_then(|z| t.pane(z)) {
                Some(p) => vec![p.clone()],
                None => t.panes().into_iter().cloned().collect(),
            }
        };
        let panes = shown.iter().map(|pane| (pane.id, self.dress(tree, pane, window, cx))).collect();
        let (pad, toggle) = self.bar_start(6., cx);
        let bar = self.bar_frame(tree, pad, toggle, cx);
        let ghost = self.panels.read(cx).ghost_tab(cx).map(|tab| self.tab_lead(&tab, false, true, cx));
        let frame = Frame {
            tree: tree.to_string(),
            bar,
            banner: self.agents.observe_only().then(observe_banner),
            ghost,
            draw: Drawing { panes, seams: self.overlay.is_none(), menus: self.panel_menus(cx) },
        };
        self.panels.update(cx, |p, cx| p.view(frame, window, cx))
    }

    fn dress(&mut self, tree: &str, pane: &Pane<Tab>, window: &mut Window, cx: &mut Context<Self>) -> Dress {
        let p = pane.id;
        let (focused, shape) = {
            let t = &self.workspaces.read(cx)[tree].tree;
            (t.focused == p, head(t, p))
        };
        let (tabs, title) = match shape {
            Head::Bare => (Vec::new(), String::new()),
            Head::Title => (Vec::new(), pane.active().map(|tab| self.tab_text(tab, cx)).unwrap_or_default()),
            Head::Tabs => (self.tab_faces(tree, pane, cx), String::new()),
        };
        let arriving = match pane.active() {
            Some(Tab::Term(t)) => self.summary(t).map(|a| a.id.clone()),
            _ => None,
        };
        let body = self.pane_body(p, focused, pane.active().cloned(), window, cx);
        Dress { body, tabs, title, arriving, press: Box::new(cx.listener(move |this, _: &MouseDownEvent, window, cx| this.focus_panel(p, window, cx))) }
    }

    fn tab_faces(&self, tree: &str, pane: &Pane<Tab>, cx: &App) -> Vec<TabFace> {
        let preview = self.workspaces.read(cx)[tree].preview.clone();
        let observe = self.agents.observe_only();
        pane.tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let doc = match tab {
                    Tab::Doc(d) => Some(d),
                    Tab::Term(_) | Tab::Web(_) => None,
                };
                let closable = match tab {
                    Tab::Term(id) => self.terminals.read(cx).may_close(id, observe),
                    Tab::Doc(_) | Tab::Web(_) => true,
                };
                TabFace { lead: self.tab_lead(tab, doc.is_some() && doc == preview.as_ref(), i == pane.active, cx), closable }
            })
            .collect()
    }

    fn pane_body(&mut self, p: PaneId, focused: bool, tab: Option<Tab>, window: &mut Window, cx: &mut Context<Self>) -> Body {
        Body::Page(
            match tab {
                Some(Tab::Term(id)) => {
                    let id = self.delegated_term(&id);
                    if self.chat_shown(&id) { self.chat_view(&id, window, cx) } else { self.term_body(&id, focused, window, cx) }
                }
                Some(Tab::Doc(doc @ Doc::File(_))) if self.loaded(p, &doc, cx) => self.file_view(p, cx),
                Some(Tab::Doc(doc @ (Doc::Diff(_) | Doc::CommitFile { .. } | Doc::PrFile { .. } | Doc::CompareFile { .. }))) if self.loaded(p, &doc, cx) => {
                    self.diff_view(p, cx)
                }
                Some(Tab::Doc(doc @ Doc::Commit(_))) if self.loaded(p, &doc, cx) => self.commit_view(p, cx),
                Some(Tab::Web(id)) => self.browser_view(p, id, window, cx),
                Some(Tab::Doc(_)) => div().flex_1(),
                None => return Body::Empty(self.empty()),
            }
            .into_any_element(),
        )
    }

    fn empty(&self) -> Empty {
        if self.agents.observe_only() {
            return Empty::Observing;
        }
        let provider = (!self.offered().is_empty()).then(|| self.store.agents.default_agent(&self.agents));
        Empty::Offers(provider.map(|provider| (provider, self.agent_name(provider))))
    }

    /// The rows of each of the panels' menus that is open.
    fn panel_menus(&self, cx: &App) -> Menus {
        let (menu, actions, top) = {
            let p = self.panels.read(cx);
            (p.menu, p.actions.clone(), p.top_actions.clone())
        };
        Menus {
            new_tab: menu.map(|_| self.new_tab_rows(cx)),
            actions: actions.map(|(_, tab, _)| self.tab_file(&tab, cx)),
            top: top.and_then(|(agent, _)| self.top_actions_rows(agent, cx)),
        }
    }

    fn new_tab_rows(&self, cx: &App) -> NewTab {
        NewTab {
            branch: self.repo(cx).map(|r| r.branch.clone()).unwrap_or_default(),
            agents: self
                .offered()
                .into_iter()
                .map(|provider| AgentRow { provider, name: self.agent_name(provider), hint: self.model_hint(provider) })
                .collect(),
        }
    }

    fn tab_file(&self, tab: &Tab, cx: &App) -> TabFile {
        let path = match tab {
            Tab::Doc(Doc::File(p)) => Some(p.clone()),
            Tab::Doc(Doc::Diff(p) | Doc::CommitFile { path: p, .. } | Doc::PrFile { path: p, .. } | Doc::CompareFile { path: p, .. }) => {
                self.cwd(cx).map(|cwd| format!("{cwd}/{p}"))
            }
            _ => None,
        };
        TabFile(path.map(|path| {
            let rel = relative(&path, &self.explore_root(cx).unwrap_or_default());
            (path, rel)
        }))
    }

    fn top_actions_rows(&self, agent: String, cx: &App) -> Option<TopActions> {
        let s = self.agents.get(&agent)?;
        let fork = self.chat_shown(&s.terminal_id) && self.can_fork(s);
        let handoff = self.can_hand_off(s).then(|| crate::handoff::menu_label(self.handoff.read(cx).moving(&agent)));
        Some(TopActions { agent, fork, handoff })
    }

    /// Closes every tab of `pane` but tab `keep`, highest index first so the rest keep theirs, stopping at the first that asks to be confirmed.
    fn close_others(&mut self, pane: PaneId, keep: usize, cx: &mut Context<Self>) {
        let count =
            |this: &Self, cx: &App| this.place(cx).and_then(|t| this.workspaces.read(cx).get(&t)?.tree.pane(pane).map(|p| p.tabs.len())).unwrap_or_default();
        for i in (0..count(self, cx)).rev().filter(|i| *i != keep) {
            let before = count(self, cx);
            self.close_tab(pane, i, cx);
            if count(self, cx) == before {
                return;
            }
        }
    }

    pub(crate) fn on_panels_event(&mut self, _: &Entity<Panels>, ev: &PanelsEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            PanelsEvent::Select { pane, tab, pin } => {
                self.select_tab(*pane, *tab, window, cx);
                if let Some(doc) = pin {
                    self.pin_doc(doc, cx);
                }
            }
            PanelsEvent::Close(pane, i) => self.close_tab(*pane, *i, cx),
            PanelsEvent::CloseOthers(pane, i) => {
                self.close_others(*pane, *i, cx);
                cx.notify();
            }
            PanelsEvent::Grab(pane, i) => {
                self.select_tab(*pane, *i, window, cx);
                self.freeze_pages(window, cx);
            }
            PanelsEvent::Freeze => self.freeze_pages(window, cx),
            PanelsEvent::TabMenu(pane, tab, at) => {
                self.close_menus(cx);
                self.panels.update(cx, |p, _| p.actions = Some((*pane, tab.clone(), *at)));
                cx.notify();
            }
            PanelsEvent::TopMenu(agent, at) => {
                self.close_menus(cx);
                self.panels.update(cx, |p, _| p.top_actions = Some((agent.clone(), *at)));
                cx.notify();
            }
            PanelsEvent::NewTabMenu => self.row_menu = None,
            PanelsEvent::NewShell(place) => self.new_shell(*place, cx),
            PanelsEvent::NewChat(place) => self.new_chat_tab(*place, cx),
            PanelsEvent::NewAgent(provider, place) => self.new_agent_tab(provider, *place, cx),
            PanelsEvent::OpenBrowser(place) => {
                self.close_menus(cx);
                self.open_browser_at(*place, None, window, cx);
            }
            PanelsEvent::NewBrowser => self.new_browser(&crate::actions::NewBrowser, window, cx),
            PanelsEvent::GoToFile => self.go_to_file(&crate::actions::GoToFile, window, cx),
            PanelsEvent::Ask(prompt) => self.new_session(Some(prompt.clone()), false, window, cx),
            PanelsEvent::Fork(agent) => {
                self.fork_session(agent, Some(Place::Top), cx);
                cx.notify();
            }
            PanelsEvent::Handoff(agent) => self.open_handoff(agent, window, cx),
            PanelsEvent::TopSwitch(i) => self.switch_top(*i, window, cx),
            PanelsEvent::TopClose(i) => self.close_top(*i, cx),
            PanelsEvent::Moved => {
                self.show_placed(window, cx);
                self.save_soon(cx);
                cx.notify();
            }
            PanelsEvent::Shown => self.show_placed(window, cx),
            PanelsEvent::Changed => {
                self.save_soon(cx);
                cx.notify();
            }
        }
    }

    /// Points keys at the focused pane's terminal, and forgets what closed panes held.
    pub(crate) fn sync_panels(&mut self, cx: &mut App) {
        self.terminal.forget_panes(|id| self.terminals.read(cx).sessions.get(id).is_some());
        let Some(tree) = self.session_tree(cx) else {
            self.panels.update(cx, |p, _| p.hide());
            return;
        };
        let (at, focused, live, drawn) = self.workspace(&tree, cx, |w| {
            let focused = match w.tree.focused().active() {
                Some(Tab::Term(id)) => Some(id.clone()),
                _ => None,
            };
            let live: Vec<PaneId> = w.tree.panes().iter().map(|p| p.id).collect();
            let drawn: Vec<PaneId> = w.tree.drawn().iter().map(|p| p.id).collect();
            (w.at, focused, live, drawn)
        });
        self.terminal.focused = focused.map(|id| self.delegated_term(&id));
        let creating = self.shown_create(cx).is_some();
        self.panels.update(cx, |p, _| p.sync(tree, at, live, &drawn, creating));
    }

    /// Moves the focus to pane `p`, and the keys with it.
    pub(super) fn focus_panel(&mut self, p: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.place(cx) else { return };
        if self.workspace(&tree, cx, |w| w.tree.focused == p) {
            return;
        }
        self.release_page(cx);
        let active = self.workspace(&tree, cx, |w| {
            w.tree.focus(p);
            w.tree.focused().active().cloned()
        });
        match active {
            Some(Tab::Term(id)) => self.focus_chat_or_terminal(id, window, cx),
            Some(Tab::Web(_)) => window.focus(&self.browsers.focus, cx),
            Some(Tab::Doc(_)) | None => window.focus(&self.root, cx),
        }
        self.save_soon(cx);
        cx.notify();
    }

    pub(crate) fn open_more(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.panels.update(cx, |p, _| p.more_at = Some(at));
        self.open(Overlay::More, window, cx);
    }

    /// Shows what worktree `tree`'s focused pane shows, once its panes have moved.
    fn show_focused(&mut self, tree: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (focused, active) = self.workspace(tree, cx, |w| (w.tree.focused, w.tree.focused().active));
        self.select_tab(focused, active, window, cx);
        self.load_active(cx);
    }

    fn show_placed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tree) = self.place(cx) {
            self.show_focused(&tree, window, cx);
        }
    }
}
