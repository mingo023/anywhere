use super::drop::Aim;
use super::frame::{Drawing, TabFace};
use super::{Panels, PanelsEvent};
use crate::slide::Slide;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Instant;
use theme::*;
use workspace::tree::{Pane, PaneId, Target};
use workspace::{Place, Tab};

pub(super) const GAP: f32 = 2.;

/// What a tab drag carries. The held tab draws itself on the strip, so the drag has no preview.
pub(super) struct DragTab;
pub(super) struct DragTop;

/// Each of a strip's `n` tabs' left edge and width on screen, in the window; fewer before the strip is first drawn.
pub(super) fn slots(scroll: &ScrollHandle, n: usize) -> Vec<(f32, f32)> {
    let scrolled = f32::from(scroll.offset().x);
    (0..n).map_while(|i| scroll.bounds_for_item(i)).map(|b| (f32::from(b.left()) + scrolled, f32::from(b.size.width))).collect()
}

impl Panels {
    /// Moves tab `from` of `pane` to `to` in the same strip.
    pub(super) fn move_tab(&mut self, pane: PaneId, from: usize, to: usize, cx: &mut Context<Self>) {
        self.edit(cx, |w| w.tree.move_tab(pane, from, Target::Into { pane, index: to }));
        cx.emit(PanelsEvent::Changed);
    }

    fn release_tab(&mut self, cx: &mut Context<Self>) {
        let aim = self.aim.take();
        self.at = None;
        let Some((pane, d)) = self.drag.as_mut() else { return };
        let (pane, from) = (*pane, d.from);
        match (d.release(), aim) {
            (Some((from, to)), _) => self.move_tab(pane, from, to, cx),
            (None, Some(Aim::To(to))) => self.move_tab_to(pane, from, to, cx),
            (None, Some(Aim::Top(i))) => {
                self.edit(cx, |w| w.detach_tab(pane, from, i));
                cx.emit(PanelsEvent::Shown);
            }
            _ => {}
        }
    }

    /// A + whose menu opens a new tab at `place`.
    pub(super) fn new_tab_controls(&self, place: Place, ink: Token, draw: &Drawing, cx: &mut Context<Self>) -> Div {
        let open = self.menu == Some(place);
        let plus = ui::control(div().id("new-tab"))
            .size(px(28.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .cursor_pointer()
            .when(open, |d| d.bg(FILL_3))
            .hover(|s| s.bg(FILL_3))
            .child(icon("plus", 15., ink));
        let plus = ui::popover_trigger(
            plus,
            open,
            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                this.menu = Some(place);
                this.actions = None;
                this.top_actions = None;
                cx.emit(PanelsEvent::NewTabMenu);
                cx.notify();
            }),
        );
        let menu = open.then(|| ui::dropdown(29., ui::menu_in("tab-menu-in", self.tab_menu_view(place, draw.menus.new_tab.as_ref(), cx))));
        div().relative().flex().flex_none().items_center().child(plus).children(menu)
    }

    /// `pane`'s tabs in worktree `tree`.
    pub(super) fn strip_tabs(&mut self, draw: &Drawing, tree: &str, pane: &Pane<Tab>, faces: Vec<TabFace>, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let (p, active, tabs) = (pane.id, pane.active, &pane.tabs);
        let reduce_motion = cx.reduce_motion();
        let strip = self.strips.entry(p).or_default();
        let scroll = strip.scroll.clone();
        let shown = Some((tree.to_string(), active));
        if strip.revealed != shown {
            scroll.scroll_to_item(active);
            strip.revealed = shown;
        }
        let slots = self.drag.is_some().then(|| slots(&scroll, tabs.len()));
        if let Some(slots) = &slots {
            self.slots.insert(p, slots.clone());
        }
        if let (Some((_, d)), Some(slots)) = (self.drag.as_mut().filter(|(at, _)| *at == p), slots) {
            // gpui ends a drag on any mouse up, even one the strip never sees; a hold outliving it would keep its tab lifted.
            if !d.lay(slots) || !(d.released || cx.has_active_drag()) {
                self.drag = None;
                window.request_animation_frame();
            } else if d.ease(Instant::now(), reduce_motion) {
                window.request_animation_frame();
            } else if d.released {
                self.drag = None;
                window.request_animation_frame();
            }
        }
        let drag = self.drag.as_ref().filter(|(at, _)| *at == p).map(|(_, d)| d);
        let held = drag.and_then(Slide::held);
        let offsets = drag.map(|d| d.offsets.clone()).unwrap_or_default();
        let items: Vec<_> = tabs
            .iter()
            .zip(faces)
            .enumerate()
            .map(|(i, (tab, face))| {
                let lifted = held == Some(i);
                let selected = i == active;
                let TabFace { lead, closable } = face;
                let close = ui::control(div().id(("close-tab", i)))
                    .absolute()
                    .right(px(4.))
                    .top(relative(0.5))
                    .mt(px(-10.))
                    .size(px(20.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(4.))
                    .cursor_pointer()
                    .when(!selected, |d| d.opacity(0.).group_hover("term-tab", |s| s.opacity(1.)))
                    .hover(|s| s.bg(FILL_3))
                    .child(icon("x", 12., TEXT_3))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        cx.emit(PanelsEvent::Close(p, i));
                    }));
                let doc = match tab {
                    Tab::Doc(d) => Some(d.clone()),
                    Tab::Term(_) | Tab::Web(_) => None,
                };
                let menu_tab = tab.clone();
                let tab = ui::control(div().id(("tab", i)))
                    .h(px(30.))
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .pl(px(8.))
                    .pr(px(28.))
                    .rounded(px(6.))
                    .cursor_pointer()
                    .when(selected, |d| d.bg(FILL_3).text_color(TEXT))
                    .when(!selected, |d| d.text_color(TEXT_3).hover(|s| s.bg(FILL_1).text_color(TEXT)))
                    .child(lead.view().flex_1().min_w_0())
                    .on_click(cx.listener(move |_, ev: &ClickEvent, _, cx| {
                        let pin = doc.clone().filter(|_| ev.click_count() > 1);
                        cx.emit(PanelsEvent::Select { pane: p, tab: i, pin });
                    }))
                    .on_drag(DragTab, {
                        let this = cx.weak_entity();
                        move |_, grab, window, cx| {
                            let pointer = f32::from(window.mouse_position().x);
                            this.update(cx, |this, cx| {
                                this.drag = Some((p, Slide::new(i, grab.x.into(), pointer, GAP)));
                                cx.emit(PanelsEvent::Grab(p, i));
                            })
                            .ok();
                            cx.new(|_| EmptyView)
                        }
                    })
                    .on_mouse_down(
                        MouseButton::Right,
                        cx.listener(move |_, e: &MouseDownEvent, _, cx| cx.emit(PanelsEvent::TabMenu(p, menu_tab.clone(), e.position))),
                    );
                let visual = div()
                    .group("term-tab")
                    .relative()
                    .left(px(offsets.get(i).copied().unwrap_or(0.)))
                    .size_full()
                    .flex()
                    .items_center()
                    .text_size(px(13.))
                    .whitespace_nowrap()
                    // Else the bar's drag_area moves the window, which takes the mouse before the tab's drag can start.
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_down(MouseButton::Middle, cx.listener(move |_, _: &MouseDownEvent, _, cx| cx.emit(PanelsEvent::Close(p, i))))
                    .child(tab)
                    .when(closable, |d| d.child(close));
                // The slot holds the tab's place while it slides, so the strip can measure where tabs sit; deferred, the held tab draws over the rest.
                div().w(px(224.)).min_w(px(112.)).flex_shrink(1.).h_full().child(if lifted {
                    deferred(visual).into_any_element()
                } else {
                    visual.into_any_element()
                })
            })
            .collect();
        // As f32: Pixels orders -0 below 0, so a strip that can't scroll would show its right fade.
        let (offset, max) = (f32::from(scroll.offset().x), f32::from(scroll.max_offset().x));
        let fade = |left: bool| {
            let solid: Hsla = SURFACE_SUNKEN.into();
            let (solid, clear) = (solid, solid.opacity(0.));
            let (from, to) = if left { (solid, clear) } else { (clear, solid) };
            div().absolute().top_0().bottom_0().w(px(24.)).when(left, |d| d.left_0()).when(!left, |d| d.right_0()).bg(linear_gradient(
                90.,
                linear_color_stop(from, 0.),
                linear_color_stop(to, 1.),
            ))
        };
        let strip = div()
            .id("tab-strip")
            .track_scroll(&scroll)
            .overflow_x_scroll()
            .h_full()
            .flex()
            .min_w_0()
            .items_center()
            .gap(px(GAP))
            .on_drag_move(cx.listener(move |this, e: &DragMoveEvent<DragTab>, _, cx| {
                let pointer = f32::from(e.event.position.x);
                // A drag move comes every frame, moved or not; notifying on each would redraw forever.
                if let Some((_, d)) = this.drag.as_mut().filter(|(at, d)| *at == p && d.pointer != pointer) {
                    d.pointer = pointer;
                    cx.notify();
                }
            }))
            .capture_any_mouse_up(cx.listener(|this, e: &MouseUpEvent, _, cx| {
                if e.button == MouseButton::Left {
                    this.release_tab(cx);
                }
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| this.release_tab(cx)))
            .children(items);
        div()
            .relative()
            .flex()
            .flex_initial()
            .min_w_0()
            .h_full()
            .child(strip)
            .when(offset < 0., |d| d.child(fade(true)))
            .when(offset > -max, |d| d.child(fade(false)))
            .children(self.tab_actions(tree, p, tabs, draw.menus.actions.as_ref(), cx))
    }
}
