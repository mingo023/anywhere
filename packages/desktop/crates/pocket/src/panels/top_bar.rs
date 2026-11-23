use super::rect;
use super::strip::{DragTop, GAP, TabDrag, slots};
use crate::desktop::chrome::drag_area;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Instant;
use theme::*;
use workspace::tree::Rect;

/// The bar over a worktree's panes: its top tabs, held, dragged and scrolled here, over what `Desktop` hands it each frame.
#[derive(Default)]
pub(crate) struct TopBar {
    scroll: ScrollHandle,
    /// The worktree and tab last scrolled into view, so it is revealed once per switch rather than every frame.
    revealed: Option<(String, usize)>,
    /// The top tab held on the bar.
    drag: Option<TabDrag>,
    /// The bar as last drawn, in window pixels.
    pub(super) bounds: Rect,
    /// The top tab shown, as last drawn.
    shown: usize,
}

/// What `TopBar` asks `Desktop` for.
pub(crate) enum TopEvent {
    Switch(usize),
    Close(usize),
    /// A tab was picked up, so pages are pictured while it moves.
    Grabbed,
    /// The held tab is below the bar, at the pointer.
    Aim {
        from: usize,
        at: (f32, f32),
    },
    /// The held tab is on the bar, so no pane is aimed at.
    Level,
    /// The held tab was let go, having moved to a place on the bar if it did.
    Released(Option<(usize, usize)>),
    /// A tab of an agent was right-clicked.
    Menu(String, Point<Pixels>),
}

impl EventEmitter<TopEvent> for TopBar {}

/// One top tab, as `Desktop` reads it from the panes it holds.
pub(super) struct Face {
    pub(super) lead: AnyElement,
    pub(super) title: String,
    pub(super) meta: String,
    pub(super) italic: bool,
    /// The agent whose menu a right-click opens.
    pub(super) menu: Option<String>,
}

/// What the bar draws besides what it holds.
pub(super) struct Bar {
    pub(super) tree: String,
    pub(super) at: usize,
    pub(super) closable: bool,
    pub(super) tabs: Vec<Face>,
    pub(super) pad: f32,
    pub(super) toggle: Option<Stateful<Div>>,
    pub(super) plus: Div,
    pub(super) trailing: Vec<AnyElement>,
}

impl TopBar {
    /// Whether a tab is held on the bar.
    pub(super) fn holding(&self) -> bool {
        self.drag.is_some()
    }

    /// Lets go of a held tab without moving it; true if one was held.
    pub(super) fn cancel(&mut self) -> bool {
        self.drag.take_if(|d| !d.released).is_some()
    }

    /// Each of the `n` tabs' left edge and width in the window.
    pub(super) fn slots(&self, n: usize) -> Vec<(f32, f32)> {
        slots(&self.scroll, n)
    }

    /// The shown tab is already in view, so the bar needn't scroll to it.
    pub(super) fn revealed(&mut self, tree: String, at: usize) {
        self.revealed = Some((tree, at));
    }

    pub(super) fn view(&mut self, bar: Bar, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let Bar { tree, at, closable, tabs, pad, toggle, plus, trailing } = bar;
        let shown = Some((tree, at));
        if self.revealed != shown {
            self.scroll.scroll_to_item(at);
            self.revealed = shown;
        }
        self.shown = at;
        let reduce_motion = cx.reduce_motion();
        if let Some(d) = self.drag.as_mut() {
            // gpui ends a drag on any mouse up, even one the bar never sees; a hold outliving it would keep its tab lifted.
            if !d.lay(slots(&self.scroll, tabs.len())) || !(d.released || cx.has_active_drag()) {
                self.drag = None;
                window.request_animation_frame();
            } else if d.ease(Instant::now(), reduce_motion) {
                window.request_animation_frame();
            } else if d.released {
                self.drag = None;
                window.request_animation_frame();
            }
        }
        let held = self.drag.as_ref().and_then(TabDrag::held);
        let offsets = self.drag.as_ref().map(|d| d.offsets.clone()).unwrap_or_default();
        let tabs: Vec<_> = tabs
            .into_iter()
            .enumerate()
            .map(|(i, face)| {
                let tab = self.tab(i, i == at, closable, face, cx).left(px(offsets.get(i).copied().unwrap_or(0.)));
                // The slot holds the tab's place while it slides, so the bar can measure where tabs sit; deferred, the held tab draws over the rest.
                div().w(px(224.)).min_w(px(112.)).flex_shrink(1.).child(if held == Some(i) { deferred(tab).into_any_element() } else { tab.into_any_element() })
            })
            .collect();
        let strip = div()
            .id("top-strip")
            .track_scroll(&self.scroll)
            .overflow_x_scroll()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .gap(px(GAP))
            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<DragTop>, _, cx| this.hold((e.event.position.x.into(), e.event.position.y.into()), cx)))
            .capture_any_mouse_up(cx.listener(|this, e: &MouseUpEvent, _, cx| {
                if e.button == MouseButton::Left {
                    this.release(cx);
                }
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, _, cx| this.release(cx)))
            .children(tabs)
            .child(plus);
        let this = cx.weak_entity();
        let measure = canvas(
            move |b, _, cx| {
                this.update(cx, |this, _| this.bounds = rect(b)).ok();
            },
            |_, _, _, _| {},
        )
        .absolute()
        .inset_0();
        drag_area(div())
            .h(px(40.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(pad))
            .pr(px(6.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .children(toggle)
            .child(strip)
            .children(trailing)
            .child(measure)
    }

    fn tab(&self, i: usize, selected: bool, closable: bool, face: Face, cx: &mut Context<Self>) -> Stateful<Div> {
        let Face { lead, title, meta, italic, menu } = face;
        let ink = if selected { TEXT } else { TEXT_3 };
        let close = ui::control(div().id(("close-top", i)))
            .absolute()
            .right(px(4.))
            .top(px(5.))
            .size(px(20.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .cursor_pointer()
            .opacity(0.)
            .group_hover("top-tab", |s| s.opacity(1.))
            .hover(|s| s.bg(FILL_3))
            .child(icon("x", 12., TEXT_3))
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                cx.emit(TopEvent::Close(i));
            }));
        let text = div()
            .min_w_0()
            .flex()
            .flex_col()
            .justify_center()
            .line_height(relative(1.2))
            .child(div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).truncate().when(italic, |d| d.italic()).child(title))
            .child(div().text_size(px(11.)).text_color(TEXT_3).truncate().child(meta));
        div()
            .id(("top-tab", i))
            .group("top-tab")
            .relative()
            .w_full()
            .h(px(30.))
            .flex()
            .items_center()
            .gap(px(7.))
            .pl(px(8.))
            .pr(px(28.))
            .rounded(px(6.))
            .cursor_pointer()
            .text_color(ink)
            .when(selected, |d| d.bg(FILL_3))
            .when(!selected, |d| d.hover(|s| s.bg(FILL_1).text_color(TEXT)))
            .child(div().flex_none().flex().when(!selected, |d| d.opacity(0.55)).child(lead))
            .child(text)
            .when(closable && self.drag.is_none(), |d| d.child(close))
            // Else the bar's drag_area takes the press and moves the window.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(TopEvent::Switch(i))))
            .on_drag(DragTop, {
                let this = cx.weak_entity();
                move |_, grab, window, cx| {
                    let pointer = f32::from(window.mouse_position().x);
                    this.update(cx, |this, cx| {
                        this.drag = Some(TabDrag::new(i, grab.x.into(), pointer));
                        cx.emit(TopEvent::Grabbed);
                    })
                    .ok();
                    cx.new(|_| EmptyView)
                }
            })
            .when(closable, |d| d.on_mouse_up(MouseButton::Middle, cx.listener(move |_, _: &MouseUpEvent, _, cx| cx.emit(TopEvent::Close(i)))))
            .when_some(menu, |d, agent| {
                d.on_mouse_down(MouseButton::Right, cx.listener(move |_, e: &MouseDownEvent, _, cx| cx.emit(TopEvent::Menu(agent.clone(), e.position))))
            })
    }

    /// Follows a held tab: along the bar it makes room among the others; below it, one not shown leaves the bar to join the shown one beside a pane.
    fn hold(&mut self, at: (f32, f32), cx: &mut Context<Self>) {
        let (shown, b) = (self.shown, self.bounds);
        let Some(d) = self.drag.as_mut() else { return };
        let away = at.1 >= b.y + b.h && d.from != shown;
        // A drag move comes every frame, moved or not; notifying on each would redraw forever.
        if d.away != away || (!away && d.pointer != at.0) {
            cx.notify();
        }
        (d.pointer, d.away) = (at.0, away);
        cx.emit(if away { TopEvent::Aim { from: d.from, at } } else { TopEvent::Level });
    }

    fn release(&mut self, cx: &mut Context<Self>) {
        let Some(d) = self.drag.as_mut() else { return };
        cx.emit(TopEvent::Released(d.release()));
    }
}
