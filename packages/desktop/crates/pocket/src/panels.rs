mod divider;
mod drop;
mod drop_marks;
mod frame;
mod layouts;
mod pane_drag;
mod shortcuts;
mod strip;
mod tab_actions;
mod tab_face;
mod tab_menu;
mod top_actions;
mod top_bar;
mod top_tabs;

use crate::creating::{self, Creates};
use crate::desktop::chrome::id;
use crate::glide::Glide;
use crate::modals::more::MoreMenu;
use drop::Aim;
use frame::{Drawing, Frame};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use pane_drag::{DragPane, Source};
use std::collections::HashMap;
use std::time::Instant;
use theme::*;
use top_bar::TopBar;
use workspace::tree::{Axis, Divider, Node, Pane, PaneId, Rect, Tree};
use workspace::{Doc, Place, Tab, Workspaces};

#[derive(Default)]
pub(crate) struct Strip {
    scroll: ScrollHandle,
    /// The worktree and tab last scrolled into view, so a tab is revealed once when it becomes active rather than every frame.
    revealed: Option<(String, usize)>,
}

/// What the panels ask `Desktop` for.
pub(crate) enum PanelsEvent {
    /// A tab was clicked, and with a second click its doc is to be pinned.
    Select {
        pane: PaneId,
        tab: usize,
        pin: Option<Doc>,
    },
    Close(PaneId, usize),
    CloseOthers(PaneId, usize),
    /// A tab was picked up: show it, and picture the pages while it moves.
    Grab(PaneId, usize),
    /// A pane was picked up by its grip.
    Freeze,
    /// A tab was right-clicked.
    TabMenu(PaneId, Tab, Point<Pixels>),
    /// A top tab was right-clicked.
    TopMenu(String, Point<Pixels>),
    /// The + was pressed, which shuts the other menus it shares the screen with.
    NewTabMenu,
    NewShell(Place),
    NewChat(Place),
    NewAgent(&'static str, Place),
    OpenBrowser(Place),
    NewBrowser,
    GoToFile,
    Ask(String),
    Fork(String),
    Handoff(String),
    TopSwitch(usize),
    TopClose(usize),
    /// Panes moved: show what the focused one shows, and save.
    Moved,
    /// A tab moved: show what the focused pane shows.
    Shown,
    /// The panes changed shape or order.
    Changed,
}

impl EventEmitter<PanelsEvent> for Panels {}

/// The main area's panes: their strips, menus and the tab held on one.
pub(crate) struct Panels {
    workspaces: Entity<Workspaces>,
    creates: Entity<Creates>,
    strips: HashMap<PaneId, Strip>,
    /// Where the open + menu's new tabs go: its pane, or a new top tab.
    menu: Option<Place>,
    /// The tab whose right-click menu is open, its pane, and where the click was.
    actions: Option<(PaneId, Tab, Point<Pixels>)>,
    /// The agent whose top tab's right-click menu is open, and where the click was.
    top_actions: Option<(String, Point<Pixels>)>,
    /// The tab held on a strip, and the strip's pane.
    drag: Option<(PaneId, strip::TabDrag)>,
    /// The More menu over a file tab.
    pub(crate) more: Entity<MoreMenu>,
    /// The main area as last drawn, in window pixels; which panes have room to split depends on it.
    pub(crate) bounds: Rect,
    /// Where the held tab would drop, once it has left its strip.
    aim: Option<Aim>,
    /// The pointer while a tab is held, in the window.
    at: Option<(f32, f32)>,
    /// Each strip's tabs on screen while a tab is held.
    slots: drop::Slots,
    /// The worktree and top tab whose panes were last synced; pane ids differ between them, so a held tab can't outlive a switch.
    tree: Option<(String, usize)>,
    /// The bar over the panes: the top tabs.
    pub(crate) top: Entity<TopBar>,
    /// The pane held by its head, and where it would drop.
    pane_drag: Option<pane_drag::Held>,
    /// The panes last synced, to tell when a split makes one.
    panes: Vec<PaneId>,
    /// The pane a split just made, and how far it has grown to its share.
    born: Option<(PaneId, Glide)>,
    _subs: Vec<Subscription>,
}

impl Panels {
    pub(crate) fn new(workspaces: Entity<Workspaces>, creates: Entity<Creates>, cx: &mut Context<Self>) -> Self {
        let top = cx.new(|_| TopBar::default());
        let more = cx.new(|_| MoreMenu::default());
        let _subs = vec![cx.subscribe(&top, Self::on_top_event), cx.observe(&top, |_, _, cx| cx.notify())];
        Self {
            workspaces,
            creates,
            strips: HashMap::new(),
            menu: None,
            actions: None,
            top_actions: None,
            drag: None,
            more,
            bounds: Rect::default(),
            aim: None,
            at: None,
            slots: Default::default(),
            tree: None,
            top,
            pane_drag: None,
            panes: Vec::new(),
            born: None,
            _subs,
        }
    }

    /// Whether a held tab, pane or top tab may drop over a page, whose native view would hide the marks.
    pub(crate) fn parks_pages(&self) -> bool {
        self.drag.as_ref().is_some_and(|(_, d)| d.away) || self.pane_drag.is_some()
    }

    /// Steps the pane a split just made toward its share; true while it still grows.
    pub(crate) fn tick(&mut self, now: Instant, reduce_motion: bool) -> bool {
        let growing = self.born.as_mut().is_some_and(|(_, glide)| glide.tick(1., now, reduce_motion));
        if !growing {
            self.born = None;
        }
        growing
    }

    /// Whether a tab, pane or top tab is held, so the pictures of pages taken for it are still wanted.
    pub(crate) fn holding(&self, cx: &App) -> bool {
        self.drag.is_some() || self.pane_drag.is_some() || self.top.read(cx).holding()
    }

    /// Whether a menu of the panels is open.
    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_some() || self.actions.is_some() || self.top_actions.is_some()
    }

    pub(crate) fn close_menus(&mut self, cx: &mut Context<Self>) {
        if self.menu.is_some() || self.actions.is_some() || self.top_actions.is_some() {
            (self.menu, self.actions, self.top_actions) = (None, None, None);
            cx.notify();
        }
    }

    /// Shuts the menu of the + button and a tab's, leaving a top tab's.
    pub(crate) fn close_tab_menus(&mut self, cx: &mut Context<Self>) {
        if self.menu.is_some() || self.actions.is_some() {
            (self.menu, self.actions) = (None, None);
            cx.notify();
        }
    }

    pub(crate) fn set_menu(&mut self, menu: Option<Place>, cx: &mut Context<Self>) {
        if self.menu != menu {
            self.menu = menu;
            cx.notify();
        }
    }

    pub(crate) fn set_top_actions(&mut self, actions: Option<(String, Point<Pixels>)>, cx: &mut Context<Self>) {
        if self.top_actions != actions {
            self.top_actions = actions;
            cx.notify();
        }
    }

    /// Which of the menus are open: the + button's, a tab's, a top tab's.
    pub(crate) fn menus_open(&self) -> [bool; 3] {
        [self.menu.is_some(), self.actions.is_some(), self.top_actions.is_some()]
    }

    /// Lets go of a held pane, top tab or tab without moving it; true if one was held.
    pub(crate) fn cancel_drag(&mut self, cx: &mut App) -> bool {
        let tab = self.drag.take_if(|(_, d)| !d.released).is_some();
        let top = self.top.update(cx, |b, _| b.cancel());
        self.pane_drag.take().is_some() | tab | top
    }

    /// The worktree whose panes were last drawn.
    fn tree(&self) -> Option<&str> {
        self.tree.as_ref().map(|(tree, _)| tree.as_str())
    }

    /// Runs `f` on the panes drawn, without waiting on `Desktop` to sync them first.
    fn edit<R>(&self, cx: &mut Context<Self>, f: impl FnOnce(&mut workspace::Workspace) -> R) -> Option<R> {
        let tree = self.tree()?;
        self.workspaces.update(cx, |all, _| all.get_mut(tree).map(f))
    }

    /// Takes in which panes are alive and drawn in worktree `tree`, whose shown top tab is `at`; forgets what closed panes held.
    pub(crate) fn sync(&mut self, tree: String, at: usize, live: Vec<PaneId>, drawn: &[PaneId], creating: bool) {
        if self.tree.as_ref() != Some(&(tree.clone(), at)) {
            self.drag = None;
            self.tree = Some((tree, at));
            // Pane ids repeat between trees, so one new here was not born of a split.
            (self.panes, self.born) = (Vec::new(), None);
        }
        if let Some(pane) = born(&self.panes, &live) {
            self.born = Some((pane, Glide::from(0.)));
        }
        self.strips.retain(|p, _| live.contains(p));
        if matches!(self.menu, Some(Place::Pane(Some(p))) if !live.contains(&p)) {
            self.menu = None;
        }
        self.panes = live;
        // Only the held tab's strip ends a hold, so one whose strip isn't drawn would never end.
        if creating || self.drag.as_ref().is_some_and(|(p, _)| !drawn.contains(p)) {
            self.drag = None;
        }
        if self.drag.is_none() {
            (self.aim, self.at) = (None, None);
            self.slots.clear();
        }
    }

    /// Nothing is drawn: a tab can't be held on a strip that isn't.
    pub(crate) fn hide(&mut self) {
        self.drag = None;
    }

    /// The tab held off its strip, to draw under the pointer.
    pub(crate) fn ghost_tab(&self, cx: &App) -> Option<Tab> {
        let ((from, d), _) = (self.drag.as_ref()?, self.at?);
        if !d.away {
            return None;
        }
        self.workspaces.read(cx).get(self.tree()?)?.tree.pane(*from)?.tabs.get(d.from).cloned()
    }

    /// Worktree `tree`'s panes: all of them, or only the zoomed one.
    pub(crate) fn view(&mut self, frame: Frame, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let Frame { tree, bar, banner, ghost, mut draw } = frame;
        // gpui ends a drag on any mouse up, and one outside the panels never reaches `drop_pane`.
        if !cx.has_active_drag() {
            self.pane_drag = None;
        }
        let (root, zoomed) = {
            let t = &self.workspaces.read(cx)[tree.as_str()].tree;
            (t.root.clone(), t.zoomed.and_then(|z| t.pane(z)).cloned())
        };
        let body = match zoomed {
            Some(p) => self.pane_view(&mut draw, &tree, &p, window, cx).into_any_element(),
            None => self.node_view(&mut draw, &tree, &root, Vec::new(), window, cx),
        };
        let this = cx.weak_entity();
        let measure = canvas(
            move |b, _, cx| {
                this.update(cx, |this, _| this.bounds = rect(b)).ok();
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let drag_tree = tree.clone();
        let bar = self.top_bar(&tree, bar, &draw, window, cx);
        let marks = self.drop_marks(&tree, ghost, cx);
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(SURFACE_SUNKEN)
            .child(bar)
            .child(div().relative().flex_1().min_h_0().flex().child(measure).child(body).children(marks))
            .when_some(banner, |d, banner| d.child(banner))
            .children(self.pane_marks(&tree, cx))
            .on_drag_move(
                cx.listener(|this, e: &DragMoveEvent<strip::DragTab>, _, cx| this.aim_tab((e.event.position.x.into(), e.event.position.y.into()), cx)),
            )
            .on_drag_move(cx.listener(move |this, e: &DragMoveEvent<DragPane>, _, cx| {
                this.aim_pane(&drag_tree, (e.event.position.x.into(), e.event.position.y.into()), cx)
            }))
            .on_drop(cx.listener(|this, d: &DragPane, _, cx| this.drop_pane(d.0, cx)))
    }

    fn node_view(&mut self, draw: &mut Drawing, tree: &str, node: &Node<Tab>, path: Vec<usize>, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (axis, sizes, children) = match node {
            Node::Pane(p) => return self.pane_view(draw, tree, p, window, cx).into_any_element(),
            Node::Split { axis, sizes, children } => (*axis, sizes, children),
        };
        let born = self.born.as_ref().and_then(|(pane, glide)| {
            let j = children.iter().position(|c| matches!(c, Node::Pane(p) if p.id == *pane))?;
            let (_, at) = self.workspaces.read(cx).get(tree)?.tree.layout(self.bounds).into_iter().find(|(p, _)| p == pane)?;
            Some((j, glide.at(), at))
        });
        let sizes = born.map_or_else(|| sizes.clone(), |(j, t, _)| growing(sizes, j, t));
        let mut cells = Vec::new();
        for (j, (child, size)) in children.iter().zip(sizes).enumerate() {
            let inner = self.node_view(draw, tree, child, [path.as_slice(), &[j]].concat(), window, cx);
            let cell = div().relative().flex().flex_none().flex_basis(relative(size)).min_w_0().min_h_0();
            let cell = match born.filter(|(b, ..)| *b == j) {
                // At the size it grows to, so it slides in whole and a terminal in it starts at the right width.
                Some((_, _, at)) => {
                    let whole = match axis {
                        Axis::Row => div().flex_none().h_full().w(px(at.w)),
                        Axis::Column => div().flex_none().w_full().h(px(at.h)),
                    };
                    cell.overflow_hidden().when(axis == Axis::Column, |d| d.flex_col()).when(j == 0, |d| d.justify_end()).child(whole.child(inner))
                }
                None => cell.child(inner),
            };
            cells.push(match (j, axis) {
                (0, _) => cell,
                (_, Axis::Row) => cell.border_l(px(0.5)).border_color(SEPARATOR).children(self.divider(&path, j, axis, draw.seams, cx)),
                (_, Axis::Column) => cell.border_t(px(0.5)).border_color(SEPARATOR).children(self.divider(&path, j, axis, draw.seams, cx)),
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

    fn pane_view(&mut self, draw: &mut Drawing, tree: &str, pane: &Pane<Tab>, window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = pane.id;
        let Some(dress) = draw.panes.remove(&p) else { return div().id(id(format!("pane:{p}"))) };
        let (focused, shape, split) = {
            let t = &self.workspaces.read(cx)[tree].tree;
            (t.focused == p, head(t, p), t.panes().len() > 1)
        };
        let bar = match shape {
            Head::Bare => None,
            Head::Title => Some(self.title_head(pane, dress.title, focused, cx)),
            Head::Tabs => Some(self.tabs_head(draw, tree, pane, dress.tabs, split, window, cx)),
        };
        let bar = match (bar, dress.arriving) {
            (Some(bar), Some(agent)) => Some(creating::arriving(&self.creates, &agent, creating::BAR, bar, window, cx)),
            (bar, _) => bar,
        };
        let body = match dress.body {
            frame::Body::Page(body) => body,
            frame::Body::Empty(empty) => self.empty_pane(p, empty, cx).into_any_element(),
        };
        let lifted = self.pane_drag.as_ref().is_some_and(|h| h.from == Source::Pane(p));
        div()
            .id(id(format!("pane:{p}")))
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .capture_any_mouse_down(dress.press)
            // An unfocused pane tracks no focus, so the press would bubble to the root, whose `track_focus` retakes focus from `focus_panel`.
            .when(!focused, |d| d.on_any_mouse_down(|_, window, _| window.prevent_default()))
            .when(lifted, |d| d.opacity(0.4))
            .children(bar)
            .child(body)
    }

    /// A pane's tabs, with a grip to drag it by when it shares the tree, and its + menu.
    #[expect(clippy::too_many_arguments, reason = "what a pane's head draws comes from the frame, the pane and the window")]
    fn tabs_head(
        &mut self,
        draw: &Drawing,
        tree: &str,
        pane: &Pane<Tab>,
        faces: Vec<frame::TabFace>,
        split: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let p = pane.id;
        let grip = div()
            .id("pane-grip")
            .w(px(20.))
            .h(px(30.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(6.))
            .cursor_grab()
            .hover(|s| s.bg(FILL_1))
            .child(icon("grip", 14., TEXT_4));
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
            .child(self.strip_tabs(draw, tree, pane, faces, window, cx))
            .child(filler)
            .child(self.new_tab_controls(Place::Pane(Some(p)), TEXT_2, draw, cx).ml(px(2.)))
    }

    /// A lone terminal's head in a split: a grip, a dot while focused, its title and ×.
    fn title_head(&self, pane: &Pane<Tab>, title: String, focused: bool, cx: &mut Context<Self>) -> Div {
        let p = pane.id;
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
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(PanelsEvent::Close(p, 0));
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
    fn empty_pane(&self, p: PaneId, empty: frame::Empty, cx: &mut Context<Self>) -> Div {
        let title = div().pb(px(6.)).text_size(px(13.)).text_color(TEXT_3).child("No open tabs");
        let pane = div().flex_1().flex().flex_col().items_center().justify_center();
        let frame::Empty::Offers(agent) = empty else {
            return pane.child(title);
        };
        let agent = agent.map(|(provider, name)| {
            ui::menu_pick(id(format!("pane-new-agent:{p}")), provider_icon(provider, 14., TEXT_2), &name, false)
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(PanelsEvent::NewAgent(provider, Place::Pane(Some(p))))))
        });
        let terminal = ui::menu_row(id(format!("pane-new-terminal:{p}")), "prompt", "Terminal", Some("⌘T"))
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(PanelsEvent::NewShell(Place::Pane(Some(p))))));
        let browser = ui::menu_row(id(format!("pane-new-browser:{p}")), "globe", "Browser", Some("⌘⇧B"))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PanelsEvent::NewBrowser)));
        let file = ui::menu_row(id(format!("pane-go-to-file:{p}")), "search", "Go to file", Some("⌘P"))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PanelsEvent::GoToFile)));
        pane.child(div().w(px(260.)).flex().flex_col().gap(px(2.)).child(title).children(agent).child(terminal).child(browser).child(file))
    }

    /// Follows a held tab over the bar and the panes; off its own strip, its siblings slide home and the drop lights up.
    fn aim_tab(&mut self, at: (f32, f32), cx: &mut Context<Self>) {
        let Some(tree) = self.tree().map(str::to_string) else { return };
        if self.at == Some(at) {
            return;
        }
        let top = self.bar_index(&tree, at, cx);
        let Some((from, d)) = self.drag.as_mut() else { return };
        let Some(w) = self.workspaces.read(cx).get(&tree) else { return };
        let t = &w.tree;
        let layout = t.drawn_layout(self.bounds);
        let tabs = t.pane(*from).map_or(0, |p| p.tabs.len());
        // `mid` reads the pointer, so it must be this move's.
        d.pointer = at.0;
        // Before its strip is laid out the tab has no middle; aiming then would read as away and park every page for a frame.
        let Some(mid) = d.mid() else { return };
        let aim = match top {
            Some(i) => Some(Aim::Top(i)),
            None => drop::aim(&layout, at, *from, tabs, mid, &self.slots),
        };
        d.away = aim != Some(Aim::Home);
        (self.aim, self.at) = (aim, Some(at));
        cx.notify();
    }

    /// Moves tab `i` of `pane` to `to`; `Desktop` then shows what the focused pane shows and loads what the moved tab needs.
    fn move_tab_to(&mut self, pane: PaneId, i: usize, to: workspace::tree::Target, cx: &mut Context<Self>) {
        self.edit(cx, |w| w.tree.move_tab(pane, i, to));
        cx.emit(PanelsEvent::Shown);
    }
}

/// The pane a split just made: the one `now` has that `was` lacked, the rest unchanged.
fn born(was: &[PaneId], now: &[PaneId]) -> Option<PaneId> {
    let mut new = now.iter().filter(|p| !was.contains(p));
    let pane = new.next().copied().filter(|_| !was.is_empty() && now.len() == was.len() + 1);
    pane.filter(|_| new.next().is_none())
}

/// `sizes` with the cell at `born` grown `t` of the way from nothing, the rest sharing what it has yet to take.
fn growing(sizes: &[f32], born: usize, t: f32) -> Vec<f32> {
    let spare = sizes[born] * (1. - t);
    let rest = sizes.iter().sum::<f32>() - sizes[born];
    sizes.iter().enumerate().map(|(j, &s)| if j == born { s - spare } else { s + spare * s / rest }).collect()
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

#[cfg(test)]
mod tests {
    use super::{Head, born, growing, head};
    use workspace::tree::{Edge, Tree};
    use workspace::{Doc, Tab};

    #[test]
    fn a_split_bears_the_one_pane_it_adds_and_nothing_else_does() {
        assert_eq!(born(&[0, 1], &[0, 2, 1]), Some(2));
        assert_eq!([born(&[], &[0]), born(&[0, 1], &[0, 1]), born(&[0, 1], &[0]), born(&[0, 1], &[0, 2]), born(&[0], &[0, 1, 2])], [None; 5]);
    }

    #[test]
    fn a_born_cell_grows_from_nothing_while_the_rest_give_way_in_proportion() {
        assert_eq!(growing(&[0.25, 0.25, 0.5], 2, 0.), [0.5, 0.5, 0.]);
        assert_eq!(growing(&[0.25, 0.25, 0.5], 2, 0.5), [0.375, 0.375, 0.25]);
        assert_eq!(growing(&[0.25, 0.25, 0.5], 2, 1.), [0.25, 0.25, 0.5]);
    }

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
