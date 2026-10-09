use crate::{browser, inbox, settings};
use gpui_kit::*;
use std::collections::BTreeMap;
use workspace::tree::Edge;

actions!(desktop, [OpenPalette, GoToFile, OpenSession, StartSession, NextNeedsYou, GoToUpNext, NextSession, PrevSession, ToggleRail, ToggleSidebar, ToggleFocus, NewWorktree, ProjectSettings, OpenSettings, FocusSearch, OpenAutomations, CheckForUpdates, NewTab, CopySelection, SelectAll, Paste, CloseTab, Save, Quit, NewBrowser, FocusAddress, Reload, Back, Forward, SplitRight, SplitDown, PrevTab, NextTab, ZoomPane, EqualizePanes, AddToChat]);

/// The nth session in the visible list, 1-based.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct JumpTo(pub usize);

/// The nth tab in the focused pane, 1-based.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct JumpToTab(pub usize);

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
        KeyBinding::new("cmd-b", ToggleSidebar, None),
        KeyBinding::new("cmd-\\", ToggleRail, None),
        KeyBinding::new("cmd-.", ToggleFocus, None),
        KeyBinding::new("cmd-t", NewTab, None),
        KeyBinding::new("cmd-w", CloseTab, None),
        KeyBinding::new("cmd-s", Save, None),
        KeyBinding::new("cmd-q", Quit, None),
        KeyBinding::new("cmd-shift-n", NewWorktree, None),
        KeyBinding::new("cmd-,", OpenSettings, None),
        KeyBinding::new("cmd-shift-a", OpenAutomations, None),
        // gpui-base's `Input` binds ⌘F to its own search, so only the Settings page claims it.
        KeyBinding::new("cmd-f", FocusSearch, Some(settings::CONTEXT)),
        // Unscoped, it ties with a text field's own ⌘↵ in depth and, bound later, wins.
        KeyBinding::new("cmd-enter", OpenSession, Some(inbox::CONTEXT)),
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
    out.extend((1..=9).map(|n| KeyBinding::new(&format!("cmd-{n}"), JumpToTab(n), None)));
    // Not ⌘⇧n: macOS takes ⌘⇧3–5 for screenshots before the app sees them.
    out.extend((1..=9).map(|n| KeyBinding::new(&format!("ctrl-{n}"), JumpTo(n), None)));
    out
}

/// What the Keyboard page and `Store.keys` call `b`: its action, and the context it's bound in.
pub fn binding_id(b: &KeyBinding) -> String {
    match b.predicate() {
        Some(p) => format!("{} in {p}", b.action().name()),
        None => b.action().name().to_string(),
    }
}

/// `bindings`, with each one `custom` names on new keys, or gone when they're empty.
pub fn keymap(custom: &BTreeMap<String, String>) -> Vec<KeyBinding> {
    bindings()
        .into_iter()
        .filter_map(|b| match custom.get(&binding_id(&b)) {
            None => Some(b),
            Some(keys) if keys.is_empty() => None,
            Some(keys) => Some(KeyBinding::load(keys, b.action().boxed_clone(), b.predicate(), false, None, &DummyKeyboardMapper).unwrap_or(b)),
        })
        .collect()
}

/// Swaps this app's bindings for `keymap(custom)` in place, keeping the ones gpui-base and the terminal bound.
pub fn rebind(custom: &BTreeMap<String, String>, cx: &mut App) {
    let ours = |b: &KeyBinding| b.action().name().starts_with("desktop::");
    let all: Vec<KeyBinding> = cx.key_bindings().borrow().bindings().cloned().collect();
    // Later bindings win ties, so ours go back where they were: after gpui-base's, before the terminal's.
    let at = all.iter().position(ours).unwrap_or(all.len());
    let (before, after): (Vec<_>, Vec<_>) = (all[..at].to_vec(), all[at..].iter().filter(|b| !ours(b)).cloned().collect());
    cx.clear_key_bindings();
    cx.bind_keys(before);
    cx.bind_keys(keymap(custom));
    cx.bind_keys(after);
}

#[cfg(test)]
mod tests {
    use super::{binding_id, bindings, keymap};
    use std::collections::BTreeMap;
    use crate::inbox;
    use gpui_kit::{KeyBinding, KeyContext, Keymap, Keystroke, actions};
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

    actions!(test, [Submit]);

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

    #[test]
    fn cmd_n_jumps_to_a_tab_and_ctrl_n_to_a_session() {
        assert_eq!([bound("cmd-1"), bound("cmd-9")], [["desktop::JumpToTab"], ["desktop::JumpToTab"]]);
        assert_eq!([bound("ctrl-1"), bound("ctrl-9")], [["desktop::JumpTo"], ["desktop::JumpTo"]]);
    }

    #[test]
    fn cmd_enter_opens_a_session_only_from_the_inbox() {
        let mut keymap = Keymap::default();
        keymap.add_bindings([KeyBinding::new("secondary-enter", Submit, Some("Input"))]);
        keymap.add_bindings(bindings());
        let action = |stack: &[&str]| {
            let stack: Vec<KeyContext> = stack.iter().map(|c| KeyContext::parse(c).unwrap()).collect();
            keymap.bindings_for_input(&[Keystroke::parse("cmd-enter").unwrap()], &stack).0.first().map(|b| b.action().name())
        };
        assert_eq!(action(&["Root", inbox::CONTEXT, "Input"]), Some("test::Submit"));
        assert_eq!(action(&["Root", inbox::CONTEXT]), Some("desktop::OpenSession"));
    }

    #[test]
    fn cmd_comma_opens_settings() {
        assert_eq!(bound("cmd-,"), ["desktop::OpenSettings"]);
    }

    #[test]
    fn cmd_shift_a_opens_automations() {
        assert_eq!(bound("cmd-shift-a"), ["desktop::OpenAutomations"]);
    }

    #[test]
    fn a_changed_shortcut_moves_to_its_new_keys_and_an_emptied_one_is_gone() {
        let custom = BTreeMap::from([("desktop::OpenPalette".to_string(), "cmd-shift-p".to_string()), ("desktop::Reload in Browser".to_string(), String::new())]);
        let keys = |id: &str| keymap(&custom).iter().filter(|b| binding_id(b) == id).map(|b| b.keystrokes()[0].inner().unparse()).collect::<Vec<_>>();
        assert_eq!(keys("desktop::OpenPalette"), ["cmd-shift-p"]);
        assert!(keys("desktop::Reload in Browser").is_empty());
        assert_eq!(keys("desktop::GoToFile"), ["cmd-p"]);
    }
}
