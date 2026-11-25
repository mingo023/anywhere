use super::mouse::Move;
use super::pane::{PaneEvent, TerminalPane};
use super::scroll::{self, Wheel};
use super::{Drag, Release, click_opens_link, link, release, tick_rows};
use crate::terminals::sessions::Grid;
use gpui_kit::*;
use std::time::Duration;
use term::{Autoscroll, Button, MouseAction, Pointer, Scroll};

impl TerminalPane {
    fn grid(&self) -> Option<Grid> {
        self.look.as_ref()?.grid.clone()
    }

    pub(super) fn select_start(&mut self, at: Pointer, clicks: usize, shift: bool, line: f32, cx: &mut Context<Self>) {
        let Some(grid) = self.grid() else { return };
        let extend = {
            let mut t = grid.get();
            let extend = shift && t.extend(at);
            if !extend {
                t.press(at, clicks);
            }
            extend
        };
        self.selection = Some(Drag { pane: self.id.clone(), at, line, extend });
        cx.notify();
    }

    /// Hands the press to a program tracking the mouse; false leaves it for selecting text.
    pub(super) fn mouse_press(&mut self, at: Pointer, button: Button, mods: Modifiers, cx: &mut Context<Self>) -> bool {
        let (Some(grid), Some(observe_only)) = (self.grid(), self.look.as_ref().map(|l| l.observe_only)) else { return false };
        let tracking = grid.get().mouse_tracking() && !observe_only;
        if !self.reporting.press(&self.id, button, tracking, mods.shift) {
            return false;
        }
        grid.get().select_none();
        self.send_mouse(at, MouseAction::Press, Some(button), mods, cx);
        cx.notify();
        true
    }

    pub(super) fn mouse_motion(&mut self, at: Pointer, hovered: bool, pressed: bool, mods: Modifiers, cx: &mut Context<Self>) {
        let button = match self.reporting.motion(&self.id, hovered, pressed) {
            Move::Drag(b) => Some(b),
            Move::Hover => None,
            Move::Skip => return,
        };
        self.send_mouse(at, MouseAction::Motion, button, mods, cx);
    }

    pub(super) fn mouse_release(&mut self, at: Pointer, button: Button, mods: Modifiers, cx: &mut Context<Self>) {
        if self.reporting.release(&self.id, button) {
            self.send_mouse(at, MouseAction::Release, Some(button), mods, cx);
        }
    }

    /// Unlike typed input, leaves the selection and scroll alone: hovering a program that tracks the mouse mustn't clear them.
    fn send_mouse(&mut self, at: Pointer, action: MouseAction, button: Option<Button>, mods: Modifiers, cx: &mut Context<Self>) {
        if !self.look.as_ref().is_some_and(|l| l.may_send) {
            return;
        }
        let Some(grid) = self.grid() else { return };
        let bytes = grid.get().mouse_report(at, action, button, mods.control, mods.alt);
        if !bytes.is_empty() {
            cx.emit(PaneEvent::Report(bytes));
        }
    }

    pub(super) fn open_link(&mut self, at: Pointer, cx: &mut Context<Self>) -> bool {
        let Some(grid) = self.grid() else { return false };
        let url = {
            let mut t = grid.get();
            let (f, cells) = t.frame();
            link::link_at(cells, f.cols, at.row, at.col)
        };
        let Some(url) = url else { return false };
        cx.emit(PaneEvent::OpenLink(url));
        true
    }

    pub(super) fn hover_link(&mut self, at: Option<Pointer>, mods: Modifiers, cx: &mut Context<Self>) {
        let over = at.is_some_and(|at| {
            let (Some(grid), Some(look)) = (self.grid(), self.look.as_ref()) else { return false };
            let mut t = grid.get();
            if !click_opens_link(mods.secondary(cx), mods.shift, t.mouse_tracking() && !look.observe_only, &look.clicks) {
                return false;
            }
            let (f, cells) = t.frame();
            link::link_at(cells, f.cols, at.row, at.col).is_some()
        });
        if over != self.link_hover {
            self.link_hover = over;
            cx.notify();
        }
    }

    pub(super) fn select_extend(&mut self, at: Pointer, cx: &mut Context<Self>) {
        let Some(d) = self.selection.as_mut() else { return };
        if !d.follow(&self.id, at) {
            return;
        }
        let extend = d.extend;
        let Some(grid) = self.grid() else { return };
        let edge = {
            let mut t = grid.get();
            if extend {
                t.extend(at);
                Autoscroll::None
            } else {
                t.drag(at)
            }
        };
        if edge != Autoscroll::None && self.autoscroll.is_none() {
            self.autoscroll = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(24)).await;
                    if !this.update(cx, |p, cx| p.autoscroll_tick(cx)).unwrap_or(false) {
                        break;
                    }
                }
            }));
        }
        cx.notify();
    }

    /// Moves one tick toward the edge the drag is held past; false once the drag is back inside the grid or over.
    fn autoscroll_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let going = match (&self.selection, self.grid()) {
            (Some(d), Some(grid)) => {
                let (at, n) = (d.at, tick_rows(&d.at, d.line));
                let mut t = grid.get();
                (0..n).all(|_| t.autoscroll(at) != Autoscroll::None)
            }
            _ => false,
        };
        if !going {
            self.autoscroll = None;
        }
        cx.notify();
        going
    }

    pub(super) fn select_release(&mut self, cx: &mut Context<Self>) {
        self.autoscroll = None;
        let Some(d) = self.selection.take() else { return };
        let (Some(grid), Some(clicks)) = (self.grid(), self.look.as_ref().map(|l| l.clicks)) else { return };
        let selected = {
            let mut t = grid.get();
            t.release();
            t.selection_text()
        };
        match release(selected, &clicks) {
            Release::Copy(text) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            Release::OpenLink => _ = self.open_link(d.at, cx),
            Release::Nothing => {}
        }
    }

    pub(super) fn wheel(&mut self, e: &ScrollEvent, at: Pointer, line: f32, cx: &mut Context<Self>) {
        let rows = self.wheel.rows(&self.id, e.delta, e.touch_phase, line);
        let Some(grid) = self.grid().filter(|_| rows != 0) else { return };
        let input = {
            let mut t = grid.get();
            match scroll::route(rows, e.modifiers.shift, t.mouse_tracking(), t.alt_screen(), t.mode(1007)) {
                Wheel::Viewport(d) => {
                    t.scroll(Scroll::Delta(d));
                    None
                }
                Wheel::Arrows { up, n } => Some(scroll::arrows(up, n, t.app_cursor())),
                Wheel::Report { up, n } => Some(t.wheel_report(at, up, e.modifiers.control, e.modifiers.alt).repeat(n)).filter(|b| !b.is_empty()),
            }
        };
        if let Some(bytes) = input {
            cx.emit(PaneEvent::Input(bytes));
        }
        cx.notify();
    }
}
