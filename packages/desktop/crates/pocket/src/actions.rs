use crate::browser;
use gpui_kit::*;
use workspace::tree::Edge;

actions!(desktop, [OpenPalette, GoToFile, OpenSession, StartSession, NextNeedsYou, GoToUpNext, NextSession, PrevSession, ToggleRail, ToggleFocus, NewWorktree, ProjectSettings, NewTab, CopySelection, SelectAll, Paste, CloseTab, Save, Quit, NewBrowser, FocusAddress, Reload, Back, Forward, SplitRight, SplitDown, PrevTab, NextTab, ZoomPane, EqualizePanes, AddToChat]);

/// The nth session in the visible list, 1-based.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct JumpTo(pub usize);

#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct PageEdit(pub web::Edit);

/// Focuses the panel touching the focused one at this edge.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct FocusPane(pub Edge);

pub fn bindings() -> Vec<KeyBinding> {
    let mut out = vec![
        KeyBinding::new("cmd-k", OpenPalette, None),
        KeyBinding::new("cmd-l", AddToChat, None),
        KeyBinding::new("cmd-p", GoToFile, None),
        KeyBinding::new("cmd-n", StartSession, None),
        KeyBinding::new("cmd-j", NextNeedsYou, None),
        KeyBinding::new("cmd-shift-j", GoToUpNext, None),
        KeyBinding::new("ctrl-tab", NextSession, None),
        KeyBinding::new("ctrl-shift-tab", PrevSession, None),
        KeyBinding::new("cmd-\\", ToggleRail, None),
        KeyBinding::new("cmd-.", ToggleFocus, None),
        KeyBinding::new("cmd-t", NewTab, None),
        KeyBinding::new("cmd-w", CloseTab, None),
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-shift-n", NewWorktree, None),
        KeyBinding::new("cmd-,", ProjectSettings, None),
        KeyBinding::new("cmd-enter", OpenSession, None),
        KeyBinding::new("cmd-c", CopySelection, Some(keys::CONTEXT)),
        KeyBinding::new("cmd-a", SelectAll, Some(keys::CONTEXT)),
        KeyBinding::new("cmd-v", Paste, Some(keys::CONTEXT)),
        KeyBinding::new("cmd-shift-b", NewBrowser, None),
        KeyBinding::new("cmd-d", SplitRight, None),
        KeyBinding::new("cmd-shift-d", SplitDown, None),
        KeyBinding::new("cmd-alt-left", FocusPane(Edge::Left), None),
        KeyBinding::new("cmd-alt-right", FocusPane(Edge::Right), None),
        KeyBinding::new("cmd-alt-up", FocusPane(Edge::Top), None),
        KeyBinding::new("cmd-alt-down", FocusPane(Edge::Bottom), None),
        // gpui-base's `Input` binds these to add cursors; at equal depth the later binding wins, so panels keep them in the code editor too.
        KeyBinding::new("cmd-alt-up", FocusPane(Edge::Top), Some("Input")),
        KeyBinding::new("cmd-alt-down", FocusPane(Edge::Bottom), Some("Input")),
        // macOS reports ⌘⇧[ as `{` with shift already applied (gpui-pre-macos parse_keystroke), so `cmd-shift-[` would never match.
        KeyBinding::new("cmd-{", PrevTab, None),
        KeyBinding::new("cmd-}", NextTab, None),
        KeyBinding::new("cmd-shift-enter", ZoomPane, None),
        KeyBinding::new("cmd-ctrl-=", EqualizePanes, None),
        KeyBinding::new("cmd-l", FocusAddress, Some(browser::CONTEXT)),
        KeyBinding::new("cmd-r", Reload, Some(browser::CONTEXT)),
        KeyBinding::new("cmd-[", Back, Some(browser::CONTEXT)),
        KeyBinding::new("cmd-]", Forward, Some(browser::CONTEXT)),
        KeyBinding::new("cmd-c", PageEdit(web::Edit::Copy), Some(browser::CONTEXT)),
        KeyBinding::new("cmd-x", PageEdit(web::Edit::Cut), Some(browser::CONTEXT)),
        KeyBinding::new("cmd-v", PageEdit(web::Edit::Paste), Some(browser::CONTEXT)),
        KeyBinding::new("cmd-a", PageEdit(web::Edit::SelectAll), Some(browser::CONTEXT)),
        KeyBinding::new("cmd-z", PageEdit(web::Edit::Undo), Some(browser::CONTEXT)),
        KeyBinding::new("cmd-shift-z", PageEdit(web::Edit::Redo), Some(browser::CONTEXT)),
    ];
    out.extend((1..=9).map(|n| KeyBinding::new(&format!("cmd-{n}"), JumpTo(n), None)));
    out
}

#[cfg(test)]
mod tests {
    use super::bindings;
    use gpui_kit::Keystroke;
    use std::collections::HashSet;

    #[test]
    fn no_two_bindings_share_a_keystroke_and_context() {
        let mut seen = HashSet::new();
        for b in bindings().into_iter().chain(keys::bindings()) {
            let keys: Vec<String> = b.keystrokes().iter().map(|k| k.unparse()).collect();
            let context = b.predicate().map(|p| p.to_string());
            assert!(seen.insert((keys.clone(), context.clone())), "{keys:?} in {context:?} is bound twice");
        }
    }

    /// The actions bound to `keys` outside any context.
    fn bound(keys: &str) -> Vec<&'static str> {
        let want = vec![Keystroke::parse(keys).unwrap().unparse()];
        bindings().iter().filter(|b| b.predicate().is_none() && b.keystrokes().iter().map(|k| k.unparse()).collect::<Vec<_>>() == want).map(|b| b.action().name()).collect()
    }

    #[test]
    fn the_panel_shortcuts_work_everywhere() {
        let panel = [("cmd-d", "desktop::SplitRight"), ("cmd-shift-d", "desktop::SplitDown"), ("cmd-alt-left", "desktop::FocusPane"), ("cmd-alt-right", "desktop::FocusPane"), ("cmd-alt-up", "desktop::FocusPane"), ("cmd-alt-down", "desktop::FocusPane"), ("cmd-{", "desktop::PrevTab"), ("cmd-}", "desktop::NextTab"), ("cmd-shift-enter", "desktop::ZoomPane"), ("cmd-ctrl-=", "desktop::EqualizePanes"), ("cmd-w", "desktop::CloseTab")];
        for (keys, action) in panel {
            assert_eq!(bound(keys), [action], "{keys}");
        }
    }
}
