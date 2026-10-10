mod divider;
mod drop;
mod drop_marks;
mod layouts;
mod shortcuts;
mod strip;
mod tab_actions;
mod tab_menu;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, drag_area, id, observe_banner};
use crate::terminal_view::context;
use drop::Aim;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use theme::*;
use workspace::tree::{Axis, Divider, Edge, Node, Pane, PaneId, Rect, Target, UNIT};
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
    /// The worktree whose panes were last synced; pane ids repeat across worktrees, so a held tab can't outlive a switch.
    tree: Option<String>,
    /// Whether the panels saved last launch were brought back; until then `desktop.json` keeps them as they were.
    pub(crate) restored: bool,
}

/// Whether `pane` touches the top of the main area, where its bar also moves the window, and its left, where the bar starts with the sidebar toggle.
pub(crate) fn edges(layout: &[(PaneId, Rect)], pane: PaneId) -> (bool, bool) {
    layout.iter().find(|(p, _)| *p == pane).map_or((true, true), |(_, r)| (r.y < 1e-4, r.x < 1e-4))
}

pub(crate) fn rect(b: Bounds<Pixels>) -> Rect {
    Rect { x: b.origin.x.into(), y: b.origin.y.into(), w: b.size.width.into(), h: b.size.height.into() }
}

impl Desktop {
    /// Points keys at the focused pane's terminal, and forgets what closed panes held.
    pub(crate) fn sync_panels(&mut self) {
        let Some(tree) = self.session_tree() else {
            self.panels.drag = None;
            return;
        };
        if self.panels.tree.as_ref() != Some(&tree) {
            self.panels.drag = None;
            self.panels.tree = Some(tree.clone());
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
        let t = &self.workspace(tree).tree;
        let (root, zoomed) = (t.root.clone(), t.zoomed.and_then(|z| t.pane(z)).cloned());
        let layout = t.drawn_layout(UNIT);
        let body = match zoomed {
            Some(p) => self.pane_view(tree, &p, &layout, window, cx).into_any_element(),
            None => self.node_view(tree, &root, Vec::new(), &layout, window, cx),
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
        let observe = self.agents.observe_only();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(SURFACE_SUNKEN)
            .child(
                div()
                    .relative()
                    .flex_1()
                    .min_h_0()
                    .flex()
                    .child(measure)
                    .child(body)
                    .children(self.drop_marks(tree))
                    .on_drag_move(cx.listener(|this, e: &DragMoveEvent<strip::DragTab>, _, cx| this.aim_tab((e.event.position.x.into(), e.event.position.y.into()), cx))),
            )
            .when(observe, |d| d.child(observe_banner()))
    }

    fn node_view(&mut self, tree: &str, node: &Node<Tab>, path: Vec<usize>, layout: &[(PaneId, Rect)], window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (axis, sizes, children) = match node {
            Node::Pane(p) => return self.pane_view(tree, p, layout, window, cx).into_any_element(),
            Node::Split { axis, sizes, children } => (*axis, sizes, children),
        };
        let mut cells = Vec::new();
        for (j, (child, &size)) in children.iter().zip(sizes).enumerate() {
            let inner = self.node_view(tree, child, [path.as_slice(), &[j]].concat(), layout, window, cx);
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

    fn pane_view(&mut self, tree: &str, pane: &Pane<Tab>, layout: &[(PaneId, Rect)], window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = pane.id;
        let (top, left) = edges(layout, p);
        let (pad, toggle) = if top && left { self.bar_start(10., cx) } else { (10., None) };
        let ring = match pane.active() {
            Some(Tab::Term(t)) => self.summary(t).and_then(|a| a.context()).map(|(used, window)| context::ring(used, window, agents::level(used, window, self.store.sidebar.warn_at, self.store.sidebar.critical_at))),
            _ => None,
        };
        let status = div().ml_auto().pl(px(8.)).min_w_0().flex().items_center().children(ring);
        let observe = self.agents.observe_only();
        let t = &self.workspaces[tree].tree;
        let focused = t.focused == p;
        let can = [Edge::Right, Edge::Bottom].map(|edge| t.can_split(p, edge, self.panels.bounds));
        let split = |name: &'static str, edge: Edge, can: bool, cx: &mut Context<Self>| {
            let b = ui::group_button(name, name);
            if can { b.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.new_shell(Place::Split(p, edge), cx))) } else { b.opacity(0.35) }
        };
        let right = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .when(!observe, |d| d.child(ui::icon_group([split("split-right", Edge::Right, can[0], cx), split("split-down", Edge::Bottom, can[1], cx)])))
            .child(ui::icon_group([ui::group_button("session-more", "more").on_click(cx.listener(|this, e: &ClickEvent, window, cx| this.open_more(e.position(), window, cx)))]));
        let bar = div()
            .h(px(drop::STRIP_H))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(pad))
            .pr(px(10.))
            .children(toggle)
            .child(self.strip_tabs(tree, pane, window, cx))
            .child(status)
            .child(right);
        let body = self.pane_body(p, focused, pane.active().cloned(), window, cx);
        div()
            .id(id(format!("pane:{p}")))
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| this.focus_panel(p, window, cx)))
            .child(if top { drag_area(bar) } else { bar })
            .child(body)
    }

    fn empty_pane(&self, p: PaneId, cx: &mut Context<Self>) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .child(div().text_size(px(13.)).text_color(TEXT_2).child("No open tabs"))
            .child(div().text_size(px(12.5)).text_color(TEXT_3).child("Use + to start a shell or an agent"))
            .when(!self.agents.observe_only(), |d| {
                d.child(
                    ui::button(id(format!("pane-new-terminal:{p}")), ui::Variant::Secondary, Some("terminal"), "New terminal")
                        .mt(px(8.))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.new_shell(Place::Pane(Some(p)), cx))),
                )
            })
    }

    fn pane_body(&mut self, p: PaneId, focused: bool, tab: Option<Tab>, window: &mut Window, cx: &mut Context<Self>) -> Div {
        match tab {
            Some(Tab::Term(id)) => self.term_body(&id, focused, cx),
            Some(Tab::Doc(doc @ Doc::File(_))) if self.loaded(p, &doc) => self.file_view(p, cx),
            Some(Tab::Doc(doc @ (Doc::Diff(_) | Doc::CommitFile { .. } | Doc::PrFile { .. }))) if self.loaded(p, &doc) => self.diff_view(p, cx),
            Some(Tab::Doc(doc @ Doc::Commit(_))) if self.loaded(p, &doc) => self.commit_view(p, cx),
            Some(Tab::Web(id)) => self.browser_view(p, id, window, cx),
            Some(Tab::Doc(_)) => div().flex_1(),
            None if focused && self.offers_empty_prompt() => self.empty_composer(window, cx),
            None => self.empty_pane(p, cx),
        }
    }

    pub(crate) fn open_more(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.panels.more_at = Some(at);
        self.open(Overlay::More, window, cx);
    }

    /// Moves the focus to pane `p`, and the keys with it.
    fn focus_panel(&mut self, p: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        let t = &mut self.workspace(&tree).tree;
        if t.focused == p {
            return;
        }
        self.release_page();
        let t = &mut self.workspace(&tree).tree;
        t.focus(p);
        match t.focused().active().cloned() {
            Some(Tab::Term(id)) => self.focus_pane(id, window, cx),
            Some(Tab::Web(_)) => window.focus(&self.browsers.focus, cx),
            None if self.offers_empty_prompt() => self.focus_empty_prompt(window, cx),
            Some(Tab::Doc(_)) | None => window.focus(&self.root, cx),
        }
        self.save_soon(cx);
        cx.notify();
    }

    /// Follows a held tab over the panes; off its own strip, its siblings slide home and the drop lights up.
    fn aim_tab(&mut self, at: (f32, f32), cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        if self.panels.at == Some(at) {
            return;
        }
        let Some((from, d)) = self.panels.drag.as_mut() else { return };
        let Some(w) = self.workspaces.get(&tree) else { return };
        let t = &w.tree;
        let layout = t.drawn_layout(self.panels.bounds);
        let tabs = t.pane(*from).map_or(0, |p| p.tabs.len());
        // `mid` reads the pointer, so it must be this move's.
        d.pointer = at.0;
        // Before its strip is laid out the tab has no middle; aiming then would read as away and park every page for a frame.
        let Some(mid) = d.mid() else { return };
        let aim = drop::aim(&layout, at, *from, tabs, mid, &self.panels.slots);
        d.away = aim != Some(Aim::Home);
        (self.panels.aim, self.panels.at) = (aim, Some(at));
        cx.notify();
    }

    /// Moves tab `i` of `pane` to `to`, then shows what the focused pane shows and loads what the moved tab needs.
    pub(crate) fn move_tab_to(&mut self, pane: PaneId, i: usize, to: Target, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        let t = &mut self.workspace(&tree).tree;
        t.move_tab(pane, i, to);
        let (focused, active) = (t.focused, t.focused().active);
        self.select_tab(focused, active, window, cx);
        self.load_active(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::edges;
    use workspace::tree::{Edge, Tree, UNIT};

    #[test]
    fn only_panes_on_the_top_edge_take_the_window_bar_and_only_the_top_left_one_its_controls() {
        let mut t = Tree::default();
        t.push(0, 1);
        let b = t.split(0, Edge::Right, 2).unwrap();
        let c = t.split(b, Edge::Bottom, 3).unwrap();
        let layout = t.layout(UNIT);
        assert_eq!([0, b, c].map(|p| edges(&layout, p)), [(true, true), (true, false), (false, false)]);
    }
}
