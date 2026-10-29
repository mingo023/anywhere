mod divider;
mod drop;
mod drop_marks;
mod layouts;
mod pane_drag;
mod shortcuts;
mod strip;
mod tab_actions;
mod tab_menu;
mod top_tabs;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, id, observe_banner};
use drop::Aim;
use pane_drag::{DragPane, Source};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use std::collections::HashMap;
use theme::*;
use workspace::tree::{Axis, Divider, Node, Pane, PaneId, Rect, Target, Tree};
use workspace::{Doc, Place, Tab};

#[derive(Default)]
pub(crate) struct Strip {
    scroll: ScrollHandle,
    /// The worktree and tab last scrolled into view, so a tab is revealed once when it becomes active rather than every frame.
    revealed: Option<(String, usize)>,
}

/// The main area's panes: their strips, menus and the tab held on one.
#[derive(Default)]
pub(crate) struct Panels {
    strips: HashMap<PaneId, Strip>,
    /// The pane whose + menu is open.
    pub(crate) menu: Option<PaneId>,
    /// The tab whose right-click menu is open, its pane, and where the click was.
    pub(crate) actions: Option<(PaneId, Tab, Point<Pixels>)>,
    /// The tab held on a strip, and the strip's pane.
    pub(crate) drag: Option<(PaneId, strip::TabDrag)>,
    /// Where the click that opened the More menu was.
    pub(crate) more_at: Option<Point<Pixels>>,
    /// The main area as last drawn, in window pixels; which panes have room to split depends on it.
    pub(crate) bounds: Rect,
    /// Where the held tab would drop, once it has left its strip.
    pub(crate) aim: Option<Aim>,
    /// The pointer while a tab is held, in the window.
    at: Option<(f32, f32)>,
    /// Each strip's tabs on screen while a tab is held.
    slots: drop::Slots,
    /// The worktree and top tab whose panes were last synced; pane ids differ between them, so a held tab can't outlive a switch.
    tree: Option<(String, usize)>,
    /// Whether the panels saved last launch were brought back; until then `desktop.json` keeps them as they were.
    pub(crate) restored: bool,
    /// Panels saved last launch for worktrees on hosts, until each host first lists its terminals.
    pub(crate) waiting: std::collections::BTreeMap<String, workspace::Workspace>,
    top_scroll: ScrollHandle,
    /// The worktree and top tab last scrolled into view, so it is revealed once per switch rather than every frame.
    top_revealed: Option<(String, usize)>,
    /// The top tab held on the bar.
    top_drag: Option<strip::TabDrag>,
    /// The pane held by its head, and where it would drop.
    pane_drag: Option<pane_drag::Held>,
    /// The top bar as last drawn, in window pixels.
    bar: Rect,
}

impl Panels {
    /// Whether a held tab, pane or top tab may drop over a page, whose native view would hide the marks.
    pub(crate) fn parks_pages(&self) -> bool {
        self.drag.as_ref().is_some_and(|(_, d)| d.away) || self.pane_drag.is_some()
    }

    /// Whether a tab, pane or top tab is held, so the pictures of pages taken for it are still wanted.
    pub(crate) fn holding(&self) -> bool {
        self.drag.is_some() || self.pane_drag.is_some() || self.top_drag.is_some()
    }
}

pub(crate) fn rect(b: Bounds<Pixels>) -> Rect {
    Rect { x: b.origin.x.into(), y: b.origin.y.into(), w: b.size.width.into(), h: b.size.height.into() }
}

/// What sits atop a pane: nothing over a lone pane whose one tab is a terminal, since its top tab names it; a title over such a pane in a split, to drag it by; else its tabs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Head {
    Bare,
    Title,
    Tabs,
}

pub(crate) fn head(t: &Tree<Tab>, pane: PaneId) -> Head {
    match (t.pane(pane).map(|p| p.tabs.as_slice()), t.panes().len()) {
        (Some([Tab::Term(_)]), 1) => Head::Bare,
        (Some([Tab::Term(_)]), _) => Head::Title,
        _ => Head::Tabs,
    }
}

impl Desktop {
    /// Points keys at the focused pane's terminal, and forgets what closed panes held.
    pub(crate) fn sync_panels(&mut self) {
        let Some(tree) = self.session_tree() else {
            self.panels.drag = None;
            return;
        };
        let at = self.workspace(&tree).at;
        if self.panels.tree.as_ref() != Some(&(tree.clone(), at)) {
            self.panels.drag = None;
            self.panels.tree = Some((tree.clone(), at));
        }
        let w = self.workspace(&tree);
        let focused = match w.tree.focused().active() {
            Some(Tab::Term(id)) => Some(id.clone()),
            _ => None,
        };
        let live: Vec<PaneId> = w.tree.panes().iter().map(|p| p.id).collect();
        let drawn: Vec<PaneId> = w.tree.drawn().iter().map(|p| p.id).collect();
        self.terminal.focused = focused;
        self.panels.strips.retain(|p, _| live.contains(p));
        if self.panels.menu.is_some_and(|p| !live.contains(&p)) {
            self.panels.menu = None;
        }
        // Only the held tab's strip ends a hold, so one whose strip isn't drawn would never end.
        if self.shown_create().is_some() || self.panels.drag.as_ref().is_some_and(|(p, _)| !drawn.contains(p)) {
            self.panels.drag = None;
        }
        if self.panels.drag.is_none() {
            (self.panels.aim, self.panels.at) = (None, None);
            self.panels.slots.clear();
        }
    }

    /// Worktree `tree`'s panes: all of them, or only the zoomed one.
    pub(crate) fn panels_view(&mut self, tree: &str, window: &mut Window, cx: &mut Context<Self>) -> Div {
        // gpui ends a drag on any mouse up, and one outside the panels never reaches `drop_pane`.
        if !cx.has_active_drag() {
            self.panels.pane_drag = None;
        }
        let t = &self.workspace(tree).tree;
        let (root, zoomed) = (t.root.clone(), t.zoomed.and_then(|z| t.pane(z)).cloned());
        let body = match zoomed {
            Some(p) => self.pane_view(tree, &p, window, cx).into_any_element(),
            None => self.node_view(tree, &root, Vec::new(), window, cx),
        };
        let this = cx.weak_entity();
        let measure = canvas(
            move |b, _, cx| {
                this.update(cx, |this, _| this.panels.bounds = rect(b)).ok();
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let this = cx.weak_entity();
        let measure_bar = canvas(
            move |b, _, cx| {
                this.update(cx, |this, _| this.panels.bar = rect(b)).ok();
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let observe = self.agents.observe_only();
        let drag_tree = tree.to_string();
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(SURFACE_SUNKEN)
            .child(self.top_bar(tree, window, cx).child(measure_bar))
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(measure)
                    .child(body)
                    .children(self.drop_marks(tree)),
            )
            .when(observe, |d| d.child(observe_banner()))
            .children(self.pane_marks(tree))
            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<strip::DragTab>, _, cx| this.aim_tab((e.event.position.x.into(), e.event.position.y.into()), cx)))
            .on_drag_move(cx.listener(move |this, e: &DragMoveEvent<DragPane>, _, cx| this.aim_pane(&drag_tree, (e.event.position.x.into(), e.event.position.y.into()), cx)))
            .on_drop(cx.listener(|this, d: &DragPane, window, cx| this.drop_pane(d.0, window, cx)))
    }

    fn node_view(&mut self, tree: &str, node: &Node<Tab>, path: Vec<usize>, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (axis, sizes, children) = match node {
            Node::Pane(p) => return self.pane_view(tree, p, window, cx).into_any_element(),
            Node::Split { axis, sizes, children } => (*axis, sizes, children),
        };
        let mut cells = Vec::new();
        for (j, (child, &size)) in children.iter().zip(sizes).enumerate() {
            let inner = self.node_view(tree, child, [path.as_slice(), &[j]].concat(), window, cx);
            let cell = div().relative().flex().flex_none().flex_basis(relative(size)).min_w_0().min_h_0().child(inner);
            cells.push(match (j, axis) {
                (0, _) => cell,
                (_, Axis::Row) => cell.border_l(px(0.5)).border_color(SEPARATOR).children(self.divider(&path, j, axis, cx)),
                (_, Axis::Column) => cell.border_t(px(0.5)).border_color(SEPARATOR).children(self.divider(&path, j, axis, cx)),
            });
        }
        div()
            .size_full()
            .flex()
            .when(axis == Axis::Column, |d| d.flex_col())
            .children(cells)
            .on_drag_move(cx.listener(move |this, e: &DragMoveEvent<Divider>, _, cx| {
                let d = e.drag(cx).clone();
                if d.path != path {
                    return;
                }
                let (at, len) = match axis {
                    Axis::Row => (e.event.position.x - e.bounds.left(), e.bounds.size.width),
                    Axis::Column => (e.event.position.y - e.bounds.top(), e.bounds.size.height),
                };
                this.resize_split(&d, at.into(), len.into(), cx);
            }))
            .into_any_element()
    }

    fn pane_view(&mut self, tree: &str, pane: &Pane<Tab>, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = pane.id;
        let t = &self.workspaces[tree].tree;
        let focused = t.focused == p;
        let (shape, split) = (head(t, p), t.panes().len() > 1);
        let bar = match shape {
            Head::Bare => None,
            Head::Title => Some(self.title_head(pane, focused, cx)),
            Head::Tabs => Some(self.tabs_head(tree, pane, split, window, cx)),
        };
        let bar = match (bar, pane.active()) {
            (Some(bar), Some(Tab::Term(t))) => match self.summary(t).map(|a| a.id.clone()) {
                Some(agent) => Some(self.arriving(&agent, crate::creating::BAR, bar, window, cx)),
                None => Some(bar),
            },
            (bar, _) => bar,
        };
        let body = self.pane_body(p, focused, pane.active().cloned(), window, cx);
        let lifted = self.panels.pane_drag.as_ref().is_some_and(|h| h.from == Source::Pane(p));
        div()
            .id(id(format!("pane:{p}")))
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| this.focus_panel(p, window, cx)))
            // An unfocused pane tracks no focus, so the press would bubble to the root, whose `track_focus` retakes focus from `focus_panel`.
            .when(!focused, |d| d.on_any_mouse_down(|_, window, _| window.prevent_default()))
            .when(lifted, |d| d.opacity(0.4))
            .children(bar)
            .child(body)
    }

    /// A pane's tabs, with a grip to drag it by when it shares the tree, and its + menu.
    fn tabs_head(&mut self, tree: &str, pane: &Pane<Tab>, split: bool, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let p = pane.id;
        let grip = div().id("pane-grip").w(px(20.)).h(px(30.)).flex_none().flex().items_center().justify_center().rounded(px(6.)).cursor_grab().hover(|s| s.bg(FILL_1)).child(icon("grip", 14., TEXT_4));
        let filler = div().id("pane-filler").flex_1().min_w(px(16.)).self_stretch();
        let filler = if split { self.pane_handle(filler, p, cx) } else { filler };
        div()
            .h(px(drop::STRIP_H))
            .flex_none()
            .flex()
            .items_center()
            .pl(px(6.))
            .pr(px(10.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .when(split, |d| d.child(self.pane_handle(grip, p, cx)))
            .child(self.strip_tabs(tree, pane, window, cx))
            .child(filler)
            .child(self.new_tab_controls(p, cx))
    }

    /// A lone terminal's head in a split: a grip, a dot while focused, its title and ×.
    fn title_head(&self, pane: &Pane<Tab>, focused: bool, cx: &mut Context<Self>) -> Div {
        let p = pane.id;
        let title = pane.active().map(|tab| self.tab_text(tab)).unwrap_or_default();
        let close = ui::control(div().id(("close-pane", p)))
            .size(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_3))
            .child(icon("x", 12., TEXT_3))
            .tooltip(|window, cx| Tooltip::new("Close Pane (⌘W)").build(window, cx))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.close_tab(p, 0, cx);
            }));
        let head = div()
            .id("pane-title")
            .h(px(drop::STRIP_H))
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(6.))
            .px(px(8.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(icon("grip", 14., TEXT_4))
            .child(if focused { ui::dot(8., ACCENT) } else { div().size(px(8.)).flex_none() })
            .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).child(title))
            .child(close);
        // A plain div around it, as a pane's head may rise in with its agent.
        div().flex().flex_none().child(self.pane_handle(head, p, cx))
    }

    /// The pane is focused before a click lands, so the browser and file search open in it.
    fn empty_pane(&self, p: PaneId, cx: &mut Context<Self>) -> Div {
        let title = div().pb(px(6.)).text_size(px(13.)).text_color(TEXT_3).child("No open tabs");
        let pane = div().flex_1().flex().flex_col().items_center().justify_center();
        if self.agents.observe_only() {
            return pane.child(title);
        }
        let provider = self.store.agents.default_agent();
        let agent = ui::menu_pick(id(format!("pane-new-agent:{p}")), provider_icon(provider, 14., TEXT_2), &self.agent_name(provider), false)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.new_agent_tab(provider, Place::Pane(Some(p)), cx)));
        let terminal = ui::menu_row(id(format!("pane-new-terminal:{p}")), "prompt", "Terminal", Some("⌘T"))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.new_shell(Place::Pane(Some(p)), cx)));
        let browser = ui::menu_row(id(format!("pane-new-browser:{p}")), "globe", "Browser", Some("⌘⇧B"))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.new_browser(&crate::actions::NewBrowser, window, cx)));
        let file = ui::menu_row(id(format!("pane-go-to-file:{p}")), "search", "Go to file", Some("⌘P"))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.go_to_file(&crate::actions::GoToFile, window, cx)));
        pane.child(div().w(px(260.)).flex().flex_col().gap(px(2.)).child(title).child(agent).child(terminal).child(browser).child(file))
    }

    fn pane_body(&mut self, p: PaneId, focused: bool, tab: Option<Tab>, window: &mut Window, cx: &mut Context<Self>) -> Div {
        match tab {
            Some(Tab::Term(id)) if self.chat_shown(&id) => self.chat_view(&id, window, cx),
            Some(Tab::Term(id)) => self.term_body(&id, focused, cx),
            Some(Tab::Doc(doc @ Doc::File(_))) if self.loaded(p, &doc) => self.file_view(p, cx),
            Some(Tab::Doc(doc @ (Doc::Diff(_) | Doc::CommitFile { .. } | Doc::PrFile { .. }))) if self.loaded(p, &doc) => self.diff_view(p, cx),
            Some(Tab::Doc(doc @ Doc::Commit(_))) if self.loaded(p, &doc) => self.commit_view(p, cx),
            Some(Tab::Web(id)) => self.browser_view(p, id, window, cx),
            Some(Tab::Doc(_)) => div().flex_1(),
            None => self.empty_pane(p, cx),
        }
    }

    pub(crate) fn open_more(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.panels.more_at = Some(at);
        self.open(Overlay::More, window, cx);
    }

    /// Moves the focus to pane `p`, and the keys with it.
    fn focus_panel(&mut self, p: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.place() else { return };
        let t = &mut self.workspace(&tree).tree;
        if t.focused == p {
            return;
        }
        self.release_page();
        let t = &mut self.workspace(&tree).tree;
        t.focus(p);
        match t.focused().active().cloned() {
            Some(Tab::Term(id)) => self.focus_chat_or_terminal(id, window, cx),
            Some(Tab::Web(_)) => window.focus(&self.browsers.focus, cx),
            Some(Tab::Doc(_)) | None => window.focus(&self.root, cx),
        }
        self.save_soon(cx);
        cx.notify();
    }

    /// Follows a held tab over the bar and the panes; off its own strip, its siblings slide home and the drop lights up.
    fn aim_tab(&mut self, at: (f32, f32), cx: &mut Context<Self>) {
        let Some(tree) = self.place() else { return };
        if self.panels.at == Some(at) {
            return;
        }
        let top = self.bar_index(&tree, at);
        let Some((from, d)) = self.panels.drag.as_mut() else { return };
        let Some(w) = self.workspaces.get(&tree) else { return };
        let t = &w.tree;
        let layout = t.drawn_layout(self.panels.bounds);
        let tabs = t.pane(*from).map_or(0, |p| p.tabs.len());
        // `mid` reads the pointer, so it must be this move's.
        d.pointer = at.0;
        // Before its strip is laid out the tab has no middle; aiming then would read as away and park every page for a frame.
        let Some(mid) = d.mid() else { return };
        let aim = match top {
            Some(i) => Some(Aim::Top(i)),
            None => drop::aim(&layout, at, *from, tabs, mid, &self.panels.slots),
        };
        d.away = aim != Some(Aim::Home);
        (self.panels.aim, self.panels.at) = (aim, Some(at));
        cx.notify();
    }

    /// Moves tab `i` of `pane` to `to`, then shows what the focused pane shows and loads what the moved tab needs.
    pub(crate) fn move_tab_to(&mut self, pane: PaneId, i: usize, to: Target, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.place() else { return };
        let t = &mut self.workspace(&tree).tree;
        t.move_tab(pane, i, to);
        self.show_focused(&tree, window, cx);
    }

    /// Shows what worktree `tree`'s focused pane shows, once its panes have moved.
    fn show_focused(&mut self, tree: &str, window: &mut Window, cx: &mut Context<Self>) {
        let t = &self.workspace(tree).tree;
        let (focused, active) = (t.focused, t.focused().active);
        self.select_tab(focused, active, window, cx);
        self.load_active(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{Head, head};
    use workspace::tree::{Edge, Tree};
    use workspace::{Doc, Tab};

    #[test]
    fn a_lone_terminal_has_no_head_and_a_title_in_a_split() {
        let mut t = Tree::lone(0, Tab::Term("a".into()));
        assert_eq!(head(&t, 0), Head::Bare);
        let b = t.split(0, Edge::Right, Tab::Term("b".into())).unwrap();
        assert_eq!((head(&t, 0), head(&t, b)), (Head::Title, Head::Title));
    }

    #[test]
    fn a_pane_with_docs_or_several_tabs_shows_its_tabs() {
        let mut t = Tree::lone(0, Tab::Term("a".into()));
        t.push(0, Tab::Term("b".into()));
        assert_eq!(head(&t, 0), Head::Tabs);
        let d = t.split(0, Edge::Right, Tab::Doc(Doc::File("/x".into()))).unwrap();
        assert_eq!(head(&t, d), Head::Tabs);
        assert_eq!(head(&Tree::default(), 0), Head::Tabs);
    }
}
