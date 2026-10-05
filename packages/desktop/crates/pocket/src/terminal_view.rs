pub(crate) mod context;
pub(crate) mod cursor;
pub(crate) mod link;
pub(crate) mod mouse;
pub(crate) mod pane;
pub(crate) mod scroll;
pub(crate) mod surface;

use crate::actions::{CloseTab, CopySelection, NewTab, Paste, SelectAll};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use gpui_kit::*;
use mouse::{Move, Reporting};
use scroll::{Wheel, WheelRows};
use std::ops::Range;
use std::time::Duration;
use term::{Autoscroll, Button, MouseAction, Pointer, Scroll};
use workspace::Place;

pub struct TerminalViewState {
    pub(crate) focus: FocusHandle,
    pub(crate) focused: Option<String>,
    /// The IME's uncommitted text, drawn at the cursor until it commits.
    pub(crate) marked: Option<String>,
    pub(crate) selection: Option<Drag>,
    pub(crate) reporting: Reporting,
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
        Self { focus: cx.focus_handle(), focused: None, marked: None, selection: None, reporting: Reporting::default(), wheel: WheelRows::default(), autoscroll: None, blink: None, blink_on: true, keyboard: false, cursor_blinks: false, caret: None }
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

    /// Hands the press to a program tracking the mouse; false leaves it for selecting text.
    pub(crate) fn mouse_press(&mut self, pane: &str, at: Pointer, button: Button, mods: Modifiers, cx: &mut Context<Self>) -> bool {
        let Some(t) = self.terminals.sessions.term(pane) else { return false };
        let tracking = t.mouse_tracking() && !self.agents.observe_only();
        if !self.terminal.reporting.press(pane, button, tracking, mods.shift) {
            return false;
        }
        t.select_none();
        self.send_mouse(pane, at, MouseAction::Press, Some(button), mods);
        cx.notify();
        true
    }

    pub(crate) fn mouse_motion(&mut self, pane: &str, at: Pointer, hovered: bool, pressed: bool, mods: Modifiers) {
        let button = match self.terminal.reporting.motion(pane, hovered, pressed) {
            Move::Drag(b) => Some(b),
            Move::Hover => None,
            Move::Skip => return,
        };
        self.send_mouse(pane, at, MouseAction::Motion, button, mods);
    }

    pub(crate) fn mouse_release(&mut self, pane: &str, at: Pointer, button: Button, mods: Modifiers) {
        if self.terminal.reporting.release(pane, button) {
            self.send_mouse(pane, at, MouseAction::Release, Some(button), mods);
        }
    }

    /// Unlike `send_input`, leaves the selection and scroll alone: hovering a program that tracks the mouse mustn't clear them.
    fn send_mouse(&mut self, pane: &str, at: Pointer, action: MouseAction, button: Option<Button>, mods: Modifiers) {
        if self.agents.observe_only() {
            return;
        }
        let Some(t) = self.terminals.sessions.term(pane) else { return };
        let bytes = t.mouse_report(at, action, button, mods.control, mods.alt);
        if !bytes.is_empty() {
            self.daemon.input(pane, &bytes);
        }
    }

    /// Opens the link under `at` in a browser tab; false if there is none.
    pub(crate) fn open_link(&mut self, pane: &str, at: Pointer, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(t) = self.terminals.sessions.term(pane) else { return false };
        let (f, cells) = t.frame();
        let Some(url) = link::link_at(cells, f.cols, at.row, at.col) else { return false };
        self.open_browser(Some(url), window, cx);
        true
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

    pub(crate) fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.terminal.focused.clone().filter(|_| self.overlay.is_none()) else { return };
        let Some(text) = cx.read_from_clipboard().and_then(|c| c.text()).filter(|t| !t.is_empty()) else { return };
        self.offer_paste(pane, text, cx);
    }

    pub(crate) fn drop_paths(&mut self, pane: String, paths: &ExternalPaths, window: &mut Window, cx: &mut Context<Self>) {
        if self.overlay.is_some() {
            return;
        }
        self.focus_pane(pane.clone(), window, cx);
        self.offer_paste(pane, term::dropped_paths(paths.paths()), cx);
    }

    /// Pastes at once when the pane brackets pastes or the text can't run anything; otherwise asks first.
    fn offer_paste(&mut self, pane: String, text: String, cx: &mut Context<Self>) {
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

    pub(crate) fn wheel(&mut self, pane: &str, e: &ScrollWheelEvent, at: Pointer, line: f32, cx: &mut Context<Self>) {
        let rows = self.terminal.wheel.rows(pane, e.delta, e.touch_phase, line);
        let Some(t) = self.terminals.sessions.term(pane).filter(|_| rows != 0) else { return };
        match scroll::route(rows, e.modifiers.shift, t.mouse_tracking(), t.alt_screen(), t.mode(1007)) {
            Wheel::Viewport(d) => t.scroll(Scroll::Delta(d)),
            Wheel::Arrows { up, n } => {
                let bytes = scroll::arrows(up, n, t.app_cursor());
                self.send_input(pane, &bytes, cx);
            }
            Wheel::Report { up, n } => {
                let bytes = t.wheel_report(at, up, e.modifiers.control, e.modifiers.alt).repeat(n);
                if !bytes.is_empty() {
                    self.send_input(pane, &bytes, cx);
                }
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
        self.panels.menu = None;
        self.new_shell(Place::Pane(None), cx);
    }

    pub fn close_active_tab(&mut self, _: &CloseTab, _: &mut Window, cx: &mut Context<Self>) {
        if self.overlay.is_some() {
            return;
        }
        let Some(tree) = self.session_tree() else { return };
        let p = self.workspace(&tree).tree.focused();
        let (pane, i) = (p.id, p.active);
        self.close_tab(pane, i, cx);
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
