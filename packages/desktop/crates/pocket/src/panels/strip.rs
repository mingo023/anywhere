use super::drop::Aim;
use crate::browser::Browser;
use crate::desktop::Desktop;
use crate::desktop::chrome::{id, state};
use crate::status::Status;
use crate::util::basename;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Instant;
use theme::*;
use ui::{self, dot};
use workspace::tree::{Pane, PaneId, Target};
use workspace::{Doc, Tab};

pub(super) const GAP: f32 = 2.;
/// How quickly a sliding tab closes on where it's going: about 90% of the way in 80ms.
const SLIDE: f32 = 0.035;

/// What a tab drag carries. The held tab draws itself on the strip, so the drag has no preview.
pub(super) struct DragTab;

/// A tab held on the strip and the tabs sliding out of its way; once released, every tab sliding home.
pub(crate) struct TabDrag {
    pub(super) from: usize,
    /// Where the pointer holds the tab, from its left edge.
    pub(super) grab: f32,
    pub(super) pointer: f32,
    /// Each tab's left edge and width as laid out, before any slide.
    slots: Vec<(f32, f32)>,
    /// How far each tab is drawn from its slot.
    offsets: Vec<f32>,
    /// The last frame, while any tab is on its way.
    last_frame: Option<Instant>,
    released: bool,
    /// Held off its strip: the other tabs slide home and a release here moves nothing.
    pub(crate) away: bool,
}

/// Each of a strip's `n` tabs' left edge and width on screen, in the window; fewer before the strip is first drawn.
pub(super) fn slots(scroll: &ScrollHandle, n: usize) -> Vec<(f32, f32)> {
    let scrolled = f32::from(scroll.offset().x);
    (0..n).map_while(|i| scroll.bounds_for_item(i)).map(|b| (f32::from(b.left()) + scrolled, f32::from(b.size.width))).collect()
}

impl TabDrag {
    fn new(from: usize, grab: f32, pointer: f32) -> Self {
        Self { from, grab, pointer, slots: Vec::new(), offsets: Vec::new(), last_frame: None, released: false, away: false }
    }

    /// Takes where the strip laid its tabs out; false once it has gained or lost one.
    fn lay(&mut self, slots: Vec<(f32, f32)>) -> bool {
        if self.offsets.is_empty() {
            self.offsets = vec![0.; slots.len()];
        } else if slots.len() != self.offsets.len() {
            return false;
        }
        self.slots = slots;
        true
    }

    /// The held tab's left edge, under the pointer but within the strip, and where it would land: past every tab whose middle its leading edge has crossed.
    fn aim(&self) -> Option<(f32, usize)> {
        let (n, from) = (self.slots.len(), self.from);
        let &(_, w) = self.slots.get(from)?;
        let (first, last) = (self.slots[0], self.slots[n - 1]);
        let left = (self.pointer - self.grab).min(last.0 + last.1 - w).max(first.0);
        let crossed = |j: usize| {
            let mid = self.slots[j].0 + self.slots[j].1 / 2.;
            if j > from { left + w > mid } else { left < mid }
        };
        Some((left, from + (from + 1..n).filter(|&j| crossed(j)).count() - (0..from).filter(|&j| crossed(j)).count()))
    }

    /// How far each tab should be drawn from its slot: the held one under the pointer, the ones it passed moved over to fill its place.
    fn targets(&self) -> Vec<f32> {
        let n = self.slots.len();
        let Some((left, to)) = self.aim().filter(|_| !self.released && !self.away) else { return vec![0.; n] };
        let step = self.slots[self.from].1 + GAP;
        (0..n)
            .map(|i| match i {
                i if i == self.from => left - self.slots[i].0,
                i if self.from < i && i <= to => -step,
                i if to <= i && i < self.from => step,
                _ => 0.,
            })
            .collect()
    }

    fn held(&self) -> Option<usize> {
        (!self.released && !self.away).then_some(self.from)
    }

    /// Moves each tab toward its target as of `now`, the held one straight there; true while any is still on its way.
    fn ease(&mut self, now: Instant, reduce_motion: bool) -> bool {
        let dt = self.last_frame.map_or(0., |t| now.duration_since(t).as_secs_f32());
        let k = if reduce_motion { 1. } else { 1. - (-dt / SLIDE).exp() };
        let (targets, held) = (self.targets(), self.held());
        for (i, (o, &t)) in self.offsets.iter_mut().zip(&targets).enumerate() {
            *o = if Some(i) == held || (t - *o).abs() < 0.5 { t } else { *o + (t - *o) * k };
        }
        let moving = self.offsets != targets;
        self.last_frame = moving.then_some(now);
        moving
    }

    /// The held tab's middle under the pointer, unclamped, so it follows the pointer past the strip.
    pub(super) fn mid(&self) -> Option<f32> {
        let &(_, w) = self.slots.get(self.from)?;
        Some(self.pointer - self.grab + w / 2.)
    }

    /// Lets the held tab go, once, and returns where it moves from and to; each tab then slides home from where it's drawn, in the new order.
    fn release(&mut self) -> Option<(usize, usize)> {
        if self.released {
            return None;
        }
        let targets = self.targets();
        self.released = true;
        let (_, to) = self.aim().filter(|_| !self.away)?;
        let (from, w) = (self.from, self.slots[self.from].1);
        let (l, lw) = self.slots[to];
        let landing = if to > from { l + lw - w } else { l };
        let mut offsets: Vec<f32> = self.offsets.iter().zip(targets).map(|(o, t)| o - t).collect();
        offsets.remove(from);
        offsets.insert(to, self.slots[from].0 + self.offsets[from] - landing);
        self.offsets = offsets;
        Some((from, to))
    }
}

impl Desktop {
    pub(super) fn tab_lead(&self, tab: &Tab, preview: bool, ink: Token) -> Div {
        let row = div().flex().items_center().gap(px(7.));
        let label = |text: String| div().max_w(px(150.)).truncate().child(text);
        let term = match tab {
            Tab::Term(term) => term,
            Tab::Web(id) => {
                let b = self.browsers.tabs.get(id);
                let lead = match b {
                    Some(b) if b.loading => spinner(format!("tab-loading:{id}"), 13., TEXT_3).into_any_element(),
                    _ => icon("globe", 13., TEXT_3).into_any_element(),
                };
                let text = b.map(Browser::label).unwrap_or_default();
                return row.child(lead).child(label(text.to_string()));
            }
            Tab::Doc(doc) => {
                let (path, text) = match doc {
                    Doc::File(p) | Doc::Diff(p) => (p, basename(p)),
                    Doc::CommitFile { sha, path } => (path, format!("{} ({})", basename(path), git::short_sha(sha))),
                    Doc::Commit(sha) => return row.child(icon("diff-multiple", 14., ink)).child(label(git::short_sha(sha).to_string()).when(preview, |d| d.italic())),
                };
                let totals = match doc {
                    Doc::Diff(p) => self.repo().and_then(|r| r.files.iter().find(|f| f.path == *p)).map(|f| ui::meta_diff(f.added, f.removed, 11.)),
                    _ => None,
                };
                let unsaved = matches!(doc, Doc::File(p) if self.preview.dirty(p)).then(|| dot(6., TEXT_2));
                return row.child(file_icon(path, false, false, 14.)).child(label(text).when(preview, |d| d.italic())).children(totals).children(unsaved);
            }
        };
        if let Some(a) = self.summary(term) {
            let mark = ui::indicator(id(format!("tab-mark:{}", a.id)), Status::of(a).map(|s| state(s, 0, 0)));
            return row.child(provider_icon(&a.provider, 13., ink)).child(label(provider_name(&a.provider).into())).children(mark);
        }
        let s = self.terminals.sessions.get(term);
        let busy = s.and_then(|s| s.busy());
        let mark = match s {
            Some(s) if s.failed() => icon("x", 12., FAILED).into_any_element(),
            _ if busy.is_some() => dot(6., ACCENT).into_any_element(),
            _ => dot(6., TEXT_5).into_any_element(),
        };
        let text = busy.map_or_else(|| self.pane_label(term), str::to_string);
        row.child(icon("prompt", 13., TEXT_3)).child(label(text)).child(mark)
    }

    /// Moves tab `from` of `pane` to `to` in the same strip.
    pub(crate) fn move_tab(&mut self, pane: PaneId, from: usize, to: usize, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        self.workspace(&tree).tree.move_tab(pane, from, Target::Into { pane, index: to });
        self.save_soon(cx);
        cx.notify();
    }

    fn release_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let aim = self.panels.aim.take();
        self.panels.at = None;
        let Some((pane, d)) = self.panels.drag.as_mut() else { return };
        let (pane, from) = (*pane, d.from);
        match (d.release(), aim) {
            (Some((from, to)), _) => self.move_tab(pane, from, to, cx),
            (None, Some(Aim::To(to))) => self.move_tab_to(pane, from, to, window, cx),
            _ => {}
        }
    }

    fn new_tab_controls(&self, pane: PaneId, cx: &mut Context<Self>) -> Div {
        let open = self.panels.menu == Some(pane);
        let plus = div()
            .id("new-tab")
            .size(px(28.))
            .ml(px(2.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .cursor_pointer()
            .when(open, |d| d.bg(FILL_3))
            .hover(|s| s.bg(FILL_3))
            .child(icon("plus", 15., TEXT_2))
            // Runs before the open menu's click-outside handler, which would otherwise close it only for this click to reopen it.
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.panels.menu = if open { None } else { Some(pane) };
                this.panels.actions = None;
                this.row_menu = None;
                cx.notify();
            }));
        let menu = open.then(|| ui::dropdown(29., ui::menu_in("tab-menu-in", self.tab_menu_view(pane, cx))));
        div().relative().flex().flex_none().items_center().child(plus).children(menu)
    }

    /// `pane`'s tabs in worktree `tree`, with its + menu.
    pub(super) fn strip_tabs(&mut self, tree: &str, pane: &Pane<Tab>, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let (p, active, tabs) = (pane.id, pane.active, &pane.tabs);
        let w = &self.workspaces[tree];
        let focused = w.tree.focused == p;
        let preview = w.preview.clone();
        let observe = self.agents.observe_only();
        let reduce_motion = cx.reduce_motion();
        let strip = self.panels.strips.entry(p).or_default();
        let scroll = strip.scroll.clone();
        let shown = Some((tree.to_string(), active));
        if strip.revealed != shown {
            scroll.scroll_to_item(active);
            strip.revealed = shown;
        }
        let slots = self.panels.drag.is_some().then(|| slots(&scroll, tabs.len()));
        if let Some(slots) = &slots {
            self.panels.slots.insert(p, slots.clone());
        }
        if let (Some((_, d)), Some(slots)) = (self.panels.drag.as_mut().filter(|(at, _)| *at == p), slots) {
            // gpui ends a drag on any mouse up, even one the strip never sees; a hold outliving it would keep its tab lifted.
            if !d.lay(slots) || !(d.released || cx.has_active_drag()) {
                self.panels.drag = None;
                window.request_animation_frame();
            } else if d.ease(Instant::now(), reduce_motion) {
                window.request_animation_frame();
            } else if d.released {
                self.panels.drag = None;
                window.request_animation_frame();
            }
        }
        let drag = self.panels.drag.as_ref().filter(|(at, _)| *at == p).map(|(_, d)| d);
        let held = drag.and_then(TabDrag::held);
        let offsets = drag.map(|d| d.offsets.clone()).unwrap_or_default();
        let items: Vec<_> = tabs
            .iter()
            .enumerate()
            .map(|(i, tab)| {
                let lifted = held == Some(i);
                let selected = i == active;
                let closable = match tab {
                    Tab::Term(id) => self.terminals.may_close(id, observe),
                    Tab::Doc(_) | Tab::Web(_) => true,
                };
                let close = div()
                    .id(("close-tab", i))
                    .size(px(20.))
                    .mr(px(4.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .cursor_pointer()
                    .opacity(0.)
                    .group_hover("term-tab", |s| s.opacity(1.))
                    .hover(|s| s.bg(FILL_3))
                    .child(icon("x", 11., TEXT_4))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.close_tab(p, i, cx);
                    }));
                let doc = match tab {
                    Tab::Doc(d) => Some(d.clone()),
                    Tab::Term(_) | Tab::Web(_) => None,
                };
                let tab = div()
                    .id(("tab", i))
                    .h(px(28.))
                    .pl(px(10.))
                    .pr(px(6.))
                    .flex()
                    .items_center()
                    .cursor_pointer()
                    .child(self.tab_lead(tab, doc.is_some() && doc == preview, if selected { TEXT } else { TEXT_2 }))
                    .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
                        this.select_tab(p, i, window, cx);
                        if ev.click_count() > 1
                            && let Some(doc) = &doc
                        {
                            this.pin_doc(doc, cx);
                        }
                    }))
                    .on_drag(DragTab, {
                        let this = cx.weak_entity();
                        move |_, grab, window, cx| {
                            let pointer = f32::from(window.mouse_position().x);
                            this.update(cx, |this, cx| {
                                this.select_tab(p, i, window, cx);
                                this.panels.drag = Some((p, TabDrag::new(i, grab.x.into(), pointer)));
                                this.freeze_pages(window);
                            })
                            .ok();
                            cx.new(|_| EmptyView)
                        }
                    })
                    .when(tabs.len() > 1, |d| {
                        let tab = tab.clone();
                        d.on_mouse_down(
                            MouseButton::Right,
                            cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                                this.close_menus();
                                this.panels.actions = Some((p, tab.clone(), e.position));
                                cx.notify();
                            }),
                        )
                    });
                let visual = div()
                    .group("term-tab")
                    .relative()
                    .left(px(offsets.get(i).copied().unwrap_or(0.)))
                    .h(px(28.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .rounded(px(7.))
                    .text_size(px(12.5))
                    .whitespace_nowrap()
                    .when(selected && focused, |d| d.bg(SURFACE).shadow(ui::row_shadow()).font_weight(FontWeight::SEMIBOLD).text_color(TEXT))
                    .when(selected && !focused, |d| d.bg(FILL_2).font_weight(FontWeight::MEDIUM).text_color(TEXT))
                    .when(!selected, |d| d.font_weight(FontWeight::MEDIUM).text_color(TEXT_2).hover(|s| s.bg(FILL_2)))
                    // Else the bar's drag_area moves the window, which takes the mouse before the tab's drag can start.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .child(tab)
                    .when(closable, |d| d.child(close));
                // The slot holds the tab's place while it slides, so the strip can measure where tabs sit; deferred, the held tab draws over the rest.
                div().flex_none().child(if lifted { deferred(visual).into_any_element() } else { visual.into_any_element() })
            })
            .collect();
        // As f32: Pixels orders -0 below 0, so a strip that can't scroll would show its right fade.
        let (offset, max) = (f32::from(scroll.offset().x), f32::from(scroll.max_offset().x));
        let fade = |left: bool| {
            let solid: Hsla = SURFACE_SUNKEN.into();
            let (solid, clear) = (solid, solid.opacity(0.));
            let (from, to) = if left { (solid, clear) } else { (clear, solid) };
            div().absolute().top_0().bottom_0().w(px(24.)).when(left, |d| d.left_0()).when(!left, |d| d.right_0()).bg(linear_gradient(90., linear_color_stop(from, 0.), linear_color_stop(to, 1.)))
        };
        let strip = div()
            .id("tab-strip")
            .track_scroll(&scroll)
            .overflow_x_scroll()
            .flex()
            .min_w_0()
            .items_center()
            .gap(px(GAP))
            .on_drag_move(cx.listener(move |this, e: &DragMoveEvent<DragTab>, _, cx| {
                let pointer = f32::from(e.event.position.x);
                // A drag move comes every frame, moved or not; notifying on each would redraw forever.
                if let Some((_, d)) = this.panels.drag.as_mut().filter(|(at, d)| *at == p && d.pointer != pointer) {
                    d.pointer = pointer;
                    cx.notify();
                }
            }))
            .capture_any_mouse_up(cx.listener(|this, e: &MouseUpEvent, window, cx| {
                if e.button == MouseButton::Left {
                    this.release_tab(window, cx);
                }
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, window, cx| this.release_tab(window, cx)))
            // The padding keeps the selected tab's shadow inside the clip, else only its corners show; the margin undoes the shift.
            .p(px(3.))
            .m(px(-3.))
            .children(items);
        div()
            .flex()
            .flex_initial()
            .min_w_0()
            .h(px(40.))
            .items_center()
            .gap(px(2.))
            .child(div().relative().flex().min_w_0().child(strip).when(offset < 0., |d| d.child(fade(true))).when(offset > -max, |d| d.child(fade(false))))
            .child(self.new_tab_controls(p, cx))
            .children(self.tab_actions(tree, p, tabs, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::TabDrag;
    use std::time::{Duration, Instant};

    /// Three 100px tabs 2px apart, tab `from` held 10px from its left edge.
    fn holding(from: usize, pointer: f32) -> TabDrag {
        let mut d = TabDrag::new(from, 10., pointer);
        d.lay(vec![(0., 100.), (102., 100.), (204., 100.)]);
        d
    }

    #[test]
    fn a_held_tab_lands_past_the_tabs_whose_middle_its_leading_edge_crossed() {
        assert_eq!(holding(0, 10.).release(), Some((0, 0)));
        assert_eq!(holding(0, 160.).release(), Some((0, 1)));
        assert_eq!(holding(2, 100.).release(), Some((2, 1)));
        assert_eq!(holding(2, 10.).release(), Some((2, 0)));
    }

    #[test]
    fn a_held_tab_stays_within_the_strip_yet_reaches_either_end() {
        let mut d = holding(0, 1000.);
        d.ease(Instant::now(), true);
        assert_eq!(d.offsets[0], 204.);
        assert_eq!(d.release(), Some((0, 2)));
        let mut d = holding(2, -500.);
        d.ease(Instant::now(), true);
        assert_eq!(d.offsets[2], -204.);
        assert_eq!(d.release(), Some((2, 0)));
    }

    #[test]
    fn the_tabs_a_held_tab_passes_move_over_to_fill_its_place() {
        let mut d = holding(0, 1000.);
        d.ease(Instant::now(), true);
        assert_eq!(d.offsets, [204., -102., -102.]);
        let mut d = holding(2, -500.);
        d.ease(Instant::now(), true);
        assert_eq!(d.offsets, [102., 102., -204.]);
    }

    #[test]
    fn passed_tabs_slide_while_the_held_one_stays_under_the_pointer() {
        let (mut d, t) = (holding(0, 1000.), Instant::now());
        assert!(d.ease(t, false));
        assert_eq!(d.offsets, [204., 0., 0.]);
        assert!(d.ease(t + Duration::from_millis(16), false));
        assert!(-102. < d.offsets[1] && d.offsets[1] < 0.);
        assert!(!d.ease(t + Duration::from_secs(1), false));
        assert_eq!(d.offsets, [204., -102., -102.]);
    }

    #[test]
    fn with_reduced_motion_tabs_jump_straight_to_their_place() {
        let mut d = holding(0, 1000.);
        assert!(!d.ease(Instant::now(), true));
        assert_eq!(d.offsets, [204., -102., -102.]);
    }

    #[test]
    fn a_released_tab_slides_home_from_where_it_was_let_go() {
        let mut d = holding(0, 160.);
        d.ease(Instant::now(), true);
        assert_eq!(d.release(), Some((0, 1)));
        assert_eq!(d.offsets, [0., 48., 0.]);
        assert_eq!(d.held(), None);
        assert!(!d.ease(Instant::now(), true));
        assert_eq!(d.offsets, [0., 0., 0.]);
    }

    #[test]
    fn a_tab_is_let_go_only_once() {
        let mut d = holding(0, 160.);
        assert_eq!(d.release(), Some((0, 1)));
        assert_eq!(d.release(), None);
    }

    #[test]
    fn a_release_before_the_strip_is_measured_moves_nothing() {
        assert_eq!(TabDrag::new(0, 10., 500.).release(), None);
    }

    #[test]
    fn a_hold_ends_once_the_strip_gains_or_loses_a_tab() {
        let mut d = holding(0, 160.);
        assert!(d.lay(vec![(0., 100.), (102., 100.), (204., 100.)]));
        assert!(!d.lay(vec![(0., 100.), (102., 100.)]));
    }

    #[test]
    fn a_tab_held_off_its_strip_lets_the_others_slide_home_and_drops_nowhere() {
        let mut d = holding(0, 160.);
        d.away = true;
        d.ease(Instant::now(), true);
        assert_eq!((d.offsets.clone(), d.held()), (vec![0.; 3], None));
        assert_eq!(d.release(), None);
    }

    #[test]
    fn the_held_tab_s_middle_follows_the_pointer_past_the_strip() {
        assert_eq!(holding(1, 500.).mid(), Some(540.));
    }
}
