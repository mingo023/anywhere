pub(crate) mod context;
pub(crate) mod cursor;
pub(crate) mod link;
pub(crate) mod menu;
pub(crate) mod mouse;
pub(crate) mod pane;
pub(crate) mod pointer;
pub(crate) mod scroll;
pub(crate) mod surface;

use crate::actions::{CopySelection, NewTab, Paste, SelectAll};
use crate::desktop::Desktop;
use crate::desktop::chrome::Confirm;
use gpui_kit::*;
use std::collections::HashMap;
use term::{Pointer, Scroll};
use workspace::Place;

/// A pane and what keeps its events coming.
pub(crate) struct Slot {
    pub(crate) view: Entity<pane::TerminalPane>,
    pub(crate) _sub: Subscription,
}

/// The window-level part of the terminals: the one focus handle every focused pane takes, which pane keys go to, and the one open menu.
pub struct TerminalViewState {
    pub(crate) focus: FocusHandle,
    pub(crate) focused: Option<String>,
    /// The pane whose right-click menu is open, and where the click was.
    pub(crate) menu: Option<(String, Point<Pixels>)>,
    pub(crate) dismissed: pane::DismissedBanners,
    /// Each terminal's pane, by terminal id.
    panes: HashMap<String, Slot>,
}

impl TerminalViewState {
    pub fn new(cx: &mut Context<Desktop>) -> Self {
        Self { focus: cx.focus_handle(), focused: None, menu: None, dismissed: pane::DismissedBanners::default(), panes: HashMap::new() }
    }

    /// Drops the panes of terminals that are gone.
    pub(crate) fn forget_panes(&mut self, live: impl Fn(&str) -> bool) {
        self.panes.retain(|id, _| live(id));
    }

    pub(crate) fn pane(&self, id: &str) -> Option<&Entity<pane::TerminalPane>> {
        self.panes.get(id).map(|s| &s.view)
    }

    pub(crate) fn forget_selections(&self, cx: &mut App) {
        for slot in self.panes.values() {
            slot.view.update(cx, |p, _| p.forget_selection());
        }
    }
}

/// A mouse selection in one pane while its button is down; the selection itself lives in the pane's `Term`.
pub struct Drag {
    pub pane: String,
    pub(super) at: Pointer,
    pub(super) line: f32,
    /// Shift-clicked onto an existing selection, so moves drag its far end.
    pub(super) extend: bool,
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

/// What letting go of a selecting press does besides ending the selection.
#[derive(Debug, PartialEq)]
pub enum Release {
    Copy(String),
    /// A click that selected nothing: opens the link under it, if any.
    OpenLink,
    Nothing,
}

/// The settings that decide what a click or letting go does.
#[derive(Clone, Copy, Default)]
pub struct Clicks {
    pub copy_on_select: bool,
    pub click_opens_links: bool,
}

impl From<&store::prefs::Terminal> for Clicks {
    fn from(t: &store::prefs::Terminal) -> Self {
        Self { copy_on_select: t.copy_on_select, click_opens_links: t.click_opens_links }
    }
}

pub fn release(selected: Option<String>, t: &Clicks) -> Release {
    match selected {
        Some(text) if t.copy_on_select => Release::Copy(text),
        None if t.click_opens_links => Release::OpenLink,
        _ => Release::Nothing,
    }
}

/// Whether a left click opens the link under it: ⌘ (Ctrl off macOS) always does; a plain click only when asked, and only if a program tracking the mouse doesn't take it.
pub fn click_opens_link(cmd: bool, shift: bool, tracking: bool, t: &Clicks) -> bool {
    cmd || (t.click_opens_links && !(tracking && !shift))
}

/// Text that could run commands asks first, unless the program brackets pastes or the user turned the question off.
pub fn asks_before_paste(confirm: bool, bracketed: bool, text: &str) -> bool {
    confirm && !bracketed && !term::paste_is_safe(text)
}

/// Rows per autoscroll tick: one, two or three as the mouse gets further past the edge.
pub fn tick_rows(at: &Pointer, line: f32) -> usize {
    let past = if at.y < 0. { -at.y } else { at.y - at.height as f64 };
    (past / line as f64).ceil().clamp(1., 3.) as usize
}

/// What ⌘V puts in a terminal.
#[derive(Debug, PartialEq)]
pub enum Pasted {
    Text(String),
    /// An image with no text: agents (Claude Code, Codex) read it off the clipboard themselves on ^V, as iTerm sends.
    CtrlV,
}

pub fn pasted(item: &ClipboardItem) -> Option<Pasted> {
    match item.text().filter(|t| !t.is_empty()) {
        Some(text) => Some(Pasted::Text(text)),
        None => item.entries().iter().any(|e| matches!(e, ClipboardEntry::Image(_))).then_some(Pasted::CtrlV),
    }
}

impl Desktop {
    pub fn focus_pane(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.terminal.focused = Some(self.delegated_term(&id));
        window.focus(&self.terminal.focus, cx);
        cx.notify();
    }

    pub(crate) fn on_terminal_pane(&mut self, id: &str, ev: &pane::PaneEvent, window: &mut Window, cx: &mut Context<Self>) {
        use pane::PaneEvent::*;
        match ev {
            Fit(cols, rows) => self.fit(id, *cols, *rows, cx),
            Focus => self.focus_pane(id.to_string(), window, cx),
            Dropped(text) => {
                if self.overlay.is_none() {
                    self.focus_pane(id.to_string(), window, cx);
                    self.offer_paste(id.to_string(), text.clone(), cx);
                }
            }
            Input(bytes) => self.send_input(id, bytes, cx),
            Report(bytes) => {
                if self.terminals.read(cx).may_send(id, self.agents.observe_only()) {
                    self.daemon.input(id, bytes);
                }
            }
            OpenLink(url) => {
                if self.store.terminal.system_browser {
                    cx.open_url(url);
                } else {
                    self.open_browser(Some(url.clone()), window, cx);
                }
            }
            OpenMenu(at) => {
                self.close_menus(cx);
                self.terminal.menu = Some((id.to_string(), *at));
                cx.notify();
            }
            CloseMenu => {
                self.terminal.menu = None;
                cx.notify();
            }
            Pick(item) => {
                self.terminal.menu = None;
                match item {
                    menu::Item::Copy => self.copy_selection(&CopySelection, window, cx),
                    menu::Item::Paste => self.paste(&Paste, window, cx),
                    menu::Item::Clear => self.clear_pane(id, cx),
                    menu::Item::Close => self.close_terminal(id, cx),
                    menu::Item::Kill => self.kill_pane(id, cx),
                }
                cx.notify();
            }
            Banner(agent, text) => {
                self.terminal.dismissed.dismiss(agent, text);
                cx.notify();
            }
        }
    }

    pub(crate) fn copy_selection(&mut self, _: &CopySelection, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = &self.terminal.focused else { return };
        let Some(text) = self.terminals.read(cx).sessions.term(id).and_then(|t| t.selection_text()) else { return };
        cx.write_to_clipboard(ClipboardItem::new_string(text));
    }

    pub(crate) fn select_all(&mut self, _: &SelectAll, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.terminal.focused.clone() else { return };
        if let Some(grid) = self.terminals.read(cx).sessions.grid(&id).cloned() {
            grid.get().select_all();
            cx.notify();
        }
    }

    pub(crate) fn paste(&mut self, _: &Paste, _: &mut Window, cx: &mut Context<Self>) {
        let Some(pane) = self.terminal.focused.clone().filter(|_| self.overlay.is_none()) else { return };
        match cx.read_from_clipboard().as_ref().and_then(pasted) {
            Some(Pasted::Text(text)) => self.offer_paste(pane, text, cx),
            Some(Pasted::CtrlV) => self.send_input(&pane, b"\x16", cx),
            None => {}
        }
    }

    fn offer_paste(&mut self, pane: String, text: String, cx: &mut Context<Self>) {
        let Some(bracketed) = self.terminals.read(cx).sessions.term(&pane).map(|t| t.mode(2004)) else { return };
        if !asks_before_paste(self.store.terminal.confirm_paste, bracketed, &text) {
            self.paste_into(&pane, &text, cx);
        } else {
            self.ask(Confirm::Paste { pane, text }, cx);
            cx.notify();
        }
    }

    /// Re-reads bracketed mode, since the program may have changed it while the confirm was open.
    pub(crate) fn paste_into(&mut self, pane: &str, text: &str, cx: &mut Context<Self>) {
        let Some(bracketed) = self.terminals.read(cx).sessions.term(pane).map(|t| t.mode(2004)) else { return };
        self.send_input(pane, &term::paste_bytes(text, bracketed), cx);
    }

    /// Wipes the screen and scrollback, as ⌘K does in Ghostty, and has an idle shell redraw its prompt at the top.
    pub(crate) fn clear_pane(&mut self, pane: &str, cx: &mut Context<Self>) {
        let idle = self.terminals.read(cx).sessions.get(pane).is_some_and(|s| s.busy().is_none());
        let Some(mut t) = self.terminals.read(cx).sessions.term(pane) else { return };
        // Only this window's copy: pocketd keeps the scrollback, so a reattach brings it back.
        t.write(b"\x1b[H\x1b[2J\x1b[3J");
        drop(t);
        if idle {
            self.send_input(pane, b"\x0c", cx);
        }
        cx.notify();
    }

    /// The one way input reaches a pane, so the view always returns to the prompt.
    pub(crate) fn send_input(&mut self, pane: &str, bytes: &[u8], cx: &mut Context<Self>) {
        if !self.terminals.read(cx).may_send(pane, self.agents.observe_only()) {
            return;
        }
        if let Some(view) = self.terminal.pane(pane) {
            view.update(cx, |p, _| p.typed());
        }
        if let Some(grid) = self.terminals.read(cx).sessions.grid(pane).cloned() {
            let mut t = grid.get();
            t.select_none();
            t.scroll(Scroll::Bottom);
            drop(t);
            cx.notify();
        }
        self.daemon.input(pane, bytes);
    }

    pub fn new_tab(&mut self, _: &NewTab, _: &mut Window, cx: &mut Context<Self>) {
        self.panels.update(cx, |p, _| p.menu = None);
        self.open_tab(Place::Pane(None), cx);
    }

    pub(crate) fn on_term_key(&mut self, ev: &KeyDownEvent, _: &mut Window, cx: &mut Context<Self>) {
        let Some(id) = self.terminal.focused.clone().filter(|_| self.overlay.is_none()) else { return };
        if keys::option_types(&ev.keystroke) && !self.store.terminal.option_as_meta.takes(option_held(true), option_held(false)) {
            return;
        }
        let bytes = {
            let Some(mut t) = self.terminals.read(cx).sessions.term(&id) else { return };
            keys::key_bytes(&ev.keystroke, &mut t)
        };
        if let Some(bytes) = bytes {
            // Through the pane's event, as IME text is, so keys and composed text reach the terminal in the order they came.
            match self.terminal.pane(&id) {
                Some(view) => view.update(cx, |_, cx| cx.emit(pane::PaneEvent::Input(bytes))),
                None => self.send_input(&id, &bytes, cx),
            }
            cx.stop_propagation();
        }
    }
}

/// Whether the left or right Option key is down in the event being handled; GPUI's modifiers don't say which.
fn option_held(left: bool) -> bool {
    #[cfg(target_os = "macos")]
    {
        // NX_DEVICELALTKEYMASK and NX_DEVICERALTKEYMASK, from IOKit's IOLLEvent.h.
        let mask = if left { 0x20 } else { 0x40 };
        let Some(mtm) = objc2::MainThreadMarker::new() else { return false };
        objc2_app_kit::NSApplication::sharedApplication(mtm).currentEvent().is_some_and(|e| e.modifierFlags().0 & mask != 0)
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = left;
        false
    }
}

#[cfg(test)]
mod tests {
    use super::{Clicks, Drag, Pasted, Release, asks_before_paste, click_opens_link, pasted, release, tick_rows};
    use gpui_kit::{ClipboardItem, Image, ImageFormat};
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

    #[test]
    fn an_image_without_text_pastes_as_ctrl_v() {
        let image = ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1]));
        assert_eq!(pasted(&image), Some(Pasted::CtrlV));
    }

    #[test]
    fn text_pastes_as_text_and_an_empty_clipboard_pastes_nothing() {
        assert_eq!(pasted(&ClipboardItem::new_string("ls".into())), Some(Pasted::Text("ls".into())));
        assert_eq!(pasted(&ClipboardItem::new_string(String::new())), None);
    }

    #[test]
    fn letting_go_of_a_selection_copies_it_only_when_asked() {
        let copying = Clicks { copy_on_select: true, ..Clicks::default() };
        assert_eq!(release(Some("ls".into()), &copying), Release::Copy("ls".into()));
        assert_eq!(release(Some("ls".into()), &Clicks::default()), Release::Nothing);
    }

    #[test]
    fn a_plain_click_opens_a_link_only_when_asked() {
        let clicking = Clicks { click_opens_links: true, ..Clicks::default() };
        assert_eq!(release(None, &clicking), Release::OpenLink);
        assert_eq!(release(Some("x".into()), &clicking), Release::Nothing);
        assert_eq!(release(None, &Clicks::default()), Release::Nothing);
    }

    #[test]
    fn a_link_is_clickable_with_cmd_or_a_plain_click_a_tracking_program_does_not_take() {
        let clicking = Clicks { click_opens_links: true, ..Clicks::default() };
        assert!(click_opens_link(true, false, true, &Clicks::default()));
        assert!(!click_opens_link(false, false, false, &Clicks::default()));
        assert!(click_opens_link(false, false, false, &clicking));
        assert!(!click_opens_link(false, false, true, &clicking));
        assert!(click_opens_link(false, true, true, &clicking));
    }

    #[test]
    fn a_paste_that_could_run_commands_asks_unless_turned_off_or_bracketed() {
        let lines = "rm -rf build\nmake\n";
        assert!(asks_before_paste(true, false, lines));
        assert!(!asks_before_paste(false, false, lines));
        assert!(!asks_before_paste(true, true, lines));
        assert!(!asks_before_paste(true, false, "hello"));
    }
}
