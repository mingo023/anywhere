pub(crate) mod context;
pub(crate) mod cursor;
pub(crate) mod pane;
pub(crate) mod scroll;
pub(crate) mod surface;
pub(crate) mod tab_menu;
pub(crate) mod tabs;

use crate::actions::{CopySelection, NewTab, Paste, SelectAll};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, drag_area, observe_banner};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use scroll::{Wheel, WheelRows};
use std::ops::Range;
use std::time::Duration;
use term::{Autoscroll, Pointer, Scroll};
use theme::*;
use workspace::{Doc, Tab};

pub struct TerminalViewState {
    pub(crate) focus: FocusHandle,
    pub(crate) focused: Option<String>,
    /// The IME's uncommitted text, drawn at the cursor until it commits.
    pub(crate) marked: Option<String>,
    pub(crate) tab_scroll: ScrollHandle,
    /// The worktree and tab last scrolled into view, so a tab is revealed once when it becomes active rather than every frame.
    pub(crate) tab_revealed: Option<(String, usize)>,
    pub(crate) tab_menu: bool,
    pub(crate) selection: Option<Drag>,
    pub(crate) wheel: WheelRows,
    pub(crate) autoscroll: Option<Task<()>>,
    blink: Option<Task<()>>,
    pub(crate) blink_on: bool,
    /// Typing reaches the terminal: it holds keyboard focus and no overlay is open.
    pub(crate) keyboard: bool,
    /// Set by the focused pane as it draws, so the next frame knows whether its cursor should blink.
    pub(crate) cursor_blinks: bool,
    /// The focused pane's cursor cell on screen, where the IME puts its candidate window.
    pub(crate) caret: Option<Bounds<Pixels>>,
}

impl TerminalViewState {
    pub fn new(cx: &mut Context<Desktop>) -> Self {
        Self { focus: cx.focus_handle(), focused: None, marked: None, tab_scroll: ScrollHandle::new(), tab_revealed: None, tab_menu: false, selection: None, wheel: WheelRows::default(), autoscroll: None, blink: None, blink_on: true, keyboard: false, cursor_blinks: false, caret: None }
    }
}

/// A mouse selection in one pane while its button is down; the selection itself lives in the pane's `Term`.
pub struct Drag {
    pub pane: String,
    at: Pointer,
    line: f32,
    /// Shift-clicked onto an existing selection, so moves drag its far end.
    extend: bool,
}

impl Drag {
    /// Records where the mouse went, and says whether it moved the drag begun in `pane`.
    pub fn follow(&mut self, pane: &str, at: Pointer) -> bool {
        let moved = self.pane == pane && self.at != at;
        if moved {
            self.at = at;
        }
        moved
    }
}

/// Rows per autoscroll tick: one, two or three as the mouse gets further past the edge.
pub fn tick_rows(at: &Pointer, line: f32) -> usize {
    let past = if at.y < 0. { -at.y } else { at.y - at.height as f64 };
    (past / line as f64).ceil().clamp(1., 3.) as usize
}

impl Desktop {
    pub fn focus_pane(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal.focused = Some(id);
        window.focus(&self.terminal.focus, cx);
        cx.notify();
    }

    pub(crate) fn select_start(&mut self, pane: &str, at: Pointer, clicks: usize, shift: bool, line: f32, cx: &mut Context<Self>) {
        let Some(t) = self.terminals.sessions.term(pane) else { return };
        let extend = shift && t.extend(at);
        if !extend {
            t.press(at, clicks);
        }
        self.terminal.selection = Some(Drag { pane: pane.to_string(), at, line, extend });
        cx.notify();
    }

    pub(crate) fn select_extend(&mut self, pane: &str, at: Pointer, cx: &mut Context<Self>) {
        let Some(d) = self.terminal.selection.as_mut() else { return };
        if !d.follow(pane, at) {
            return;
        }
        let extend = d.extend;
        let Some(t) = self.terminals.sessions.term(pane) else { return };
        let edge = if extend {
            t.extend(at);
            Autoscroll::None
        } else {
            t.drag(at)
        };
        if edge != Autoscroll::None && self.terminal.autoscroll.is_none() {
            self.terminal.autoscroll = Some(cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(24)).await;
                    if !this.update(cx, |d, cx| d.autoscroll_tick(cx)).unwrap_or(false) {
                        break;
                    }
                }
            }));
        }
        cx.notify();
    }

    /// Moves one tick toward the edge the drag is held past; false once the drag is back inside the grid or over.
    fn autoscroll_tick(&mut self, cx: &mut Context<Self>) -> bool {
        let going = match &self.terminal.selection {
            Some(d) => {
                let (at, n) = (d.at, tick_rows(&d.at, d.line));
                self.terminals.sessions.term(&d.pane).is_some_and(|t| (0..n).all(|_| t.autoscroll(at) != Autoscroll::None))
            }
            None => false,
        };
        if !going {
            self.terminal.autoscroll = None;
        }
        cx.notify();
        going
    }

    pub(crate) fn select_release(&mut self) {
        self.terminal.autoscroll = None;
        if let Some(d) = self.terminal.selection.take()
            && let Some(t) = self.terminals.sessions.term(&d.pane)
        {
            t.release();
        }
    }

    pub(crate) fn copy_selection(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = &self.terminal.focused else { return };
        let Some(text) = self.terminals.sessions.get(id).and_then(|s| s.term.as_ref()).and_then(|t| t.selection_text()) else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    pub(crate) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.terminal.focused.clone() else { return };
        if let Some(t) = self.terminals.sessions.term(&id) {
            t.select_all();
            cx.notify();
        }
    }

    /// Pastes at once when the pane brackets pastes or the text can't run anything; otherwise asks first.
    pub(crate) fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.terminal.focused.clone().filter(|_| self.overlay.is_none()) else { return };
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()).filter(|t| !t.is_empty()) else { return };
        let Some(t) = self.terminals.sessions.term(&pane) else { return };
        if t.mode(2004) || term::paste_is_safe(&text) {
            self.paste_into(&pane, &text, cx);
        } else {
            self.confirm = Some(Confirm::Paste { pane, text });
            self.overlay = Some(Overlay::Confirm);
            cx.notify();
        }
    }

    /// Re-reads bracketed mode, since the program may have changed it while the confirm was open.
    pub(crate) fn paste_into(&mut self, pane: &str, text: &str, cx: &mut Context<Self>) {
        let Some(bracketed) = self.terminals.sessions.term(pane).map(|t| t.mode(2004)) else { return };
        self.send_input(pane, &term::paste_bytes(text, bracketed), cx);
    }

    pub(crate) fn wheel(&mut self, pane: &str, e: &ScrollWheelEvent, line: f32, cx: &mut Context<Self>) {
        let rows = self.terminal.wheel.rows(pane, e.delta, e.touch_phase, line);
        let Some(t) = self.terminals.sessions.term(pane).filter(|_| rows != 0) else { return };
        match scroll::route(rows, e.modifiers.shift, t.alt_screen(), t.mode(1007)) {
            Wheel::Viewport(d) => t.scroll(Scroll::Delta(d)),
            Wheel::Arrows { up, n } => {
                let bytes = scroll::arrows(up, n, t.app_cursor());
                self.send_input(pane, &bytes, cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn scroll_to_bottom(&mut self, pane: &str, cx: &mut Context<Self>) {
        if let Some(t) = self.terminals.sessions.term(pane) {
            t.scroll(Scroll::Bottom);
            cx.notify();
        }
    }

    /// Notes whether typing reaches the terminal, and keeps one blink timer running while the focused pane's cursor should blink.
    pub(crate) fn sync_cursor(&mut self, window: &Window, cx: &mut Context<Self>) {
        self.terminal.keyboard = self.terminal.focus.is_focused(window) && self.overlay.is_none();
        let frame_blink = std::mem::take(&mut self.terminal.cursor_blinks) && self.terminal.keyboard;
        let on = cursor::blinks(frame_blink, cx.reduce_motion(), window.is_window_active());
        if on == self.terminal.blink.is_some() {
            return;
        }
        self.terminal.blink_on = true;
        self.terminal.blink = on.then(|| {
            cx.spawn(async move |this, cx| {
                loop {
                    cx.background_executor().timer(Duration::from_millis(530)).await;
                    let toggled = this.update(cx, |d, cx| {
                        d.terminal.blink_on = !d.terminal.blink_on;
                        cx.notify();
                    });
                    if toggled.is_err() {
                        break;
                    }
                }
            })
        });
    }

    /// The one way input reaches a pane, so the view always returns to the prompt.
    pub(crate) fn send_input(&mut self, pane: &str, bytes: &[u8], cx: &mut Context<Self>) {
        if self.agents.observe_only() {
            return;
        }
        self.terminal.blink_on = true;
        self.terminal.blink = None;
        if let Some(t) = self.terminals.sessions.term(pane) {
            t.select_none();
        }
        self.scroll_to_bottom(pane, cx);
        self.daemon.input(pane, bytes);
    }

    pub fn new_tab(&mut self, _: &NewTab, _: &mut Window, cx: &mut Context<Self>) {
        self.terminal.tab_menu = false;
        self.new_shell(None, cx);
    }

    pub(crate) fn on_term_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.terminal.focused.clone().filter(|_| self.overlay.is_none()) else { return };
        let Some(s) = self.terminals.sessions.get(&id) else { return };
        let app_cursor = s.term.as_ref().is_some_and(|t| t.app_cursor());
        if let Some(bytes) = keys::key_bytes(&ev.keystroke, app_cursor) {
            self.send_input(&id, &bytes, cx);
            cx.stop_propagation();
        }
    }

    pub(crate) fn session_page(&mut self, tree: &str, cx: &mut Context<Self>) -> Div {
        let (added, removed) = self.repo().map(|r| r.totals()).unwrap_or_default();
        let diff = (added + removed > 0).then(|| {
            div()
                .id("bar-diff")
                .flex_none()
                .mr(px(4.))
                .cursor_pointer()
                .child(ui::meta_diff(added, removed, 12.))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_changes(None, cx)))
        });
        let error = self.error.clone().map(|e| div().min_w_0().truncate().mr(px(6.)).text_size(px(12.5)).text_color(FAILED_TEXT).child(e));
        let ring = self.terminal.focused.as_deref().and_then(|t| self.summary(t)).and_then(|a| a.context()).map(|(used, window)| context::ring(used, window));
        let status = div().ml_auto().pl(px(8.)).min_w_0().flex().items_center().children(error).children(ring);
        let observe = self.agents.observe_only();
        let right = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .when(!observe, |d| {
                d.child(ui::icon_group([
                    ui::group_button("split-right", "split-right").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(false), cx))),
                    ui::group_button("split-down", "split-down").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(true), cx))),
                ]))
            })
            .child(ui::icon_group([
                ui::group_button("session-more", "more").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::More, window, cx))),
            ]));
        let (pad, toggle) = self.bar_start(cx);
        let bar = drag_area(div())
            .h(px(42.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(pad))
            .pr(px(10.))
            .children(toggle)
            .child(self.term_tabs(tree, cx))
            .child(status)
            .children(diff)
            .child(right);
        let body = match self.workspace(tree).active().cloned() {
            Some(Tab::Term(rows)) => self.panes(rows, cx),
            Some(Tab::Doc(Doc::File(p))) if self.preview.file.as_ref() == Some(&p) => self.file_view(cx),
            Some(Tab::Doc(Doc::Diff(p))) if self.diff.file.as_ref() == Some(&p) => self.diff_view(cx),
            Some(Tab::Doc(_)) | None => div().flex_1(),
        };
        div().flex_1().min_h_0().flex().flex_col().bg(SURFACE_SUNKEN).child(bar).when(observe, |d| d.child(observe_banner())).child(body)
    }
}

/// Typed text arrives here rather than as key-downs so IMEs (Telex, dead keys, CJK) can compose it.
impl EntityInputHandler for Desktop {
    fn text_for_range(&mut self, _: Range<usize>, _: &mut Option<Range<usize>>, _: &mut Window, _: &mut Context<Self>) -> Option<String> {
        None
    }

    fn selected_text_range(&mut self, _: bool, _: &mut Window, _: &mut Context<Self>) -> Option<UTF16Selection> {
        Some(UTF16Selection { range: 0..0, reversed: false })
    }

    fn marked_text_range(&self, _: &mut Window, _: &mut Context<Self>) -> Option<Range<usize>> {
        self.terminal.marked.as_ref().map(|text| 0..text.encode_utf16().count())
    }

    fn unmark_text(&mut self, _: &mut Window, _: &mut Context<Self>) {
        self.terminal.marked = None;
    }

    fn replace_text_in_range(&mut self, _: Option<Range<usize>>, text: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.terminal.marked = None;
        if let Some(id) = self.terminal.focused.clone().filter(|_| self.overlay.is_none()) {
            self.send_input(&id, text.as_bytes(), cx);
        }
    }

    fn replace_and_mark_text_in_range(
        &mut self,
        _: Option<Range<usize>>,
        text: &str,
        _: Option<Range<usize>>,
        _: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.terminal.marked = (!text.is_empty() && self.overlay.is_none()).then(|| text.to_string());
        cx.notify();
    }

    fn bounds_for_range(&mut self, _: Range<usize>, _: Bounds<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<Bounds<Pixels>> {
        self.terminal.caret
    }

    fn character_index_for_point(&mut self, _: Point<Pixels>, _: &mut Window, _: &mut Context<Self>) -> Option<usize> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::{Drag, tick_rows};
    use term::Pointer;

    fn at(row: u16, col: u16) -> Pointer {
        Pointer::at(col as f32 * 8., row as f32 * 20. + 1., 8., 20., 10, 5)
    }

    #[test]
    fn a_drag_follows_only_the_pane_it_started_in() {
        let mut d = Drag { pane: "a".into(), at: at(0, 1), line: 20., extend: false };
        assert!(d.follow("a", at(0, 4)));
        assert!(!d.follow("a", at(0, 4)));
        assert!(!d.follow("b", at(1, 0)));
        assert_eq!(d.at, at(0, 4));
    }

    #[test]
    fn autoscroll_speeds_up_with_distance_past_the_edge() {
        let past = |y: f64| tick_rows(&Pointer { y, ..at(0, 0) }, 20.);
        assert_eq!((past(0.5), past(-15.), past(-35.), past(-500.)), (1, 1, 2, 3));
        assert_eq!((past(100. + 10.), past(100. + 45.)), (1, 3));
    }
}
