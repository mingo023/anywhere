use crate::actions::{CloseTab, Quit};
use crate::desktop::Desktop;
use gpui_kit::*;
use workspace::tree::PaneId;

#[derive(Debug, PartialEq)]
enum CmdW {
    Nothing,
    Tab(PaneId, usize),
    Hide,
}

/// An open overlay keeps ⌘W. With no tab to close, ⌘W hides the window if closing it keeps Anywhere running; it never quits.
fn cmd_w(overlay: bool, tab: Option<(PaneId, usize)>, hides: bool) -> CmdW {
    match tab {
        _ if overlay => CmdW::Nothing,
        Some((pane, i)) => CmdW::Tab(pane, i),
        None if hides => CmdW::Hide,
        None => CmdW::Nothing,
    }
}

/// Whether closing the window keeps Anywhere running; only macOS has a Dock to bring it back from.
fn hides(close_quits: bool) -> bool {
    cfg!(target_os = "macos") && !close_quits
}

/// Whether the window is hidden, by closing it or ⌘H.
pub(crate) fn hidden() -> bool {
    #[cfg(target_os = "macos")]
    {
        objc2::MainThreadMarker::new().is_some_and(|mtm| objc2_app_kit::NSApplication::sharedApplication(mtm).isHidden())
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}

/// Activates Anywhere, unhiding the window if closing it hid it.
pub(crate) fn show(cx: &App) {
    #[cfg(target_os = "macos")]
    {
        if let Some(mtm) = objc2::MainThreadMarker::new() {
            objc2_app_kit::NSApplication::sharedApplication(mtm).unhide(None);
        }
    }
    cx.activate(true);
}

impl Desktop {
    pub fn close_active_tab(&mut self, _: &CloseTab, _: &mut Window, cx: &mut Context<Self>) {
        let tab = self.session_tree().and_then(|tree| {
            let p = self.workspace(&tree).tree.focused();
            p.active().map(|_| (p.id, p.active))
        });
        match cmd_w(self.overlay.is_some(), tab, hides(self.store.general.close_quits)) {
            CmdW::Tab(pane, i) => self.close_tab(pane, i, cx),
            CmdW::Hide => cx.hide(),
            CmdW::Nothing => {}
        }
    }

    /// Hiding keeps the panes, scroll positions and browser tabs in memory, so the window comes back as it was.
    pub(crate) fn close_window(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if hides(self.store.general.close_quits) {
            cx.hide();
        } else {
            self.quit(&Quit, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CmdW, cmd_w};

    #[test]
    fn cmd_w_closes_the_focused_tab_and_with_none_hides_the_window() {
        assert_eq!(cmd_w(false, Some((1, 2)), true), CmdW::Tab(1, 2));
        assert_eq!(cmd_w(false, None, true), CmdW::Hide);
    }

    #[test]
    fn an_open_overlay_keeps_cmd_w_from_closing_anything() {
        assert_eq!(cmd_w(true, Some((1, 0)), true), CmdW::Nothing);
        assert_eq!(cmd_w(true, None, true), CmdW::Nothing);
    }

    #[test]
    fn cmd_w_never_quits_when_closing_the_window_would() {
        assert_eq!(cmd_w(false, None, false), CmdW::Nothing);
        assert_eq!(cmd_w(false, Some((0, 0)), false), CmdW::Tab(0, 0));
    }
}
