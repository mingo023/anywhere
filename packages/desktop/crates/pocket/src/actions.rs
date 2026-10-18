use crate::explorer::menu::{self, Arrow, Item};
use crate::{browser, inbox, settings};
use gpui_kit::*;
use std::collections::BTreeMap;
use std::rc::Rc;
use workspace::tree::Edge;

actions!(desktop, [OpenPalette, GoToFile, OpenSession, StartSession, NextNeedsYou, GoToUpNext, NextSession, PrevSession, ToggleRail, ToggleSidebar, ToggleFocus, NewWorktree, ProjectSettings, OpenSettings, FocusSearch, OpenAutomations, CheckForUpdates, NewTab, CopySelection, SelectAll, Paste, CloseTab, Save, Quit, NewBrowser, FocusAddress, Reload, Back, Forward, SplitRight, SplitDown, PrevTab, NextTab, ZoomPane, EqualizePanes, AddToChat, ZoomIn, ZoomOut, ResetZoom]);

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

#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct TreeMenuItem(pub Item);

#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct TreeMenuArrow(pub Arrow);

/// Chooses the tree menu's highlighted chat or item.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct TreeMenuChoose;

/// Focuses the panel touching the focused one at this edge.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct FocusPane(pub Edge);

/// `keys`, written for macOS, as this OS presses them.
pub fn native(keys: &str, context: Option<&str>) -> String {
    if cfg!(target_os = "macos") { keys.to_string() } else { off_mac(keys, context) }
}

/// ⌘ becomes Ctrl, with Shift in a terminal, whose Ctrl keys are the shell's. Mac's own ⌃ becomes Alt, so ⌘1 and ⌃1 stay apart, except ⌃Tab, which is Ctrl-Tab everywhere.
fn off_mac(keys: &str, context: Option<&str>) -> String {
    let k = Keystroke::parse(keys).unwrap();
    let m = k.modifiers;
    let tab = k.key == "tab";
    let modifiers = Modifiers { control: m.platform || (m.control && tab), alt: m.alt || (m.control && !tab), shift: m.shift || (m.platform && context == Some(keys::CONTEXT)), platform: false, ..m };
    Keystroke { modifiers, ..k }.unparse()
}

fn bind<A: Action>(keys: &str, action: A, context: Option<&str>) -> KeyBinding {
    KeyBinding::new(&native(keys, context), action, context)
}

pub fn bindings() -> Vec<KeyBinding> {
    let mut out = vec![
        bind("cmd-k", OpenPalette, None),
        bind("cmd-l", AddToChat, None),
        bind("cmd-p", GoToFile, None),
        bind("cmd-n", StartSession, None),
        bind("cmd-j", NextNeedsYou, None),
        bind("cmd-shift-j", GoToUpNext, None),
        bind("ctrl-tab", NextSession, None),
        bind("ctrl-shift-tab", PrevSession, None),
        bind("cmd-b", ToggleSidebar, None),
        bind("cmd-\\", ToggleRail, None),
        bind("cmd-.", ToggleFocus, None),
        bind("cmd-t", NewTab, None),
        bind("cmd-w", CloseTab, None),
        bind("cmd-s", Save, None),
        bind("cmd-q", Quit, None),
        bind("cmd-shift-n", NewWorktree, None),
        bind("cmd-,", OpenSettings, None),
        bind("cmd-shift-a", OpenAutomations, None),
        bind("cmd-=", ZoomIn, None),
        bind("cmd--", ZoomOut, None),
        bind("cmd-0", ResetZoom, None),
        // gpui-base's `Input` binds ⌘F to its own search, so only the Settings page claims it.
        bind("cmd-f", FocusSearch, Some(settings::CONTEXT)),
        // Unscoped, it ties with a text field's own ⌘↵ in depth and, bound later, wins.
        bind("cmd-enter", OpenSession, Some(inbox::CONTEXT)),
        bind("cmd-c", CopySelection, Some(keys::CONTEXT)),
        bind("cmd-a", SelectAll, Some(keys::CONTEXT)),
        bind("cmd-v", Paste, Some(keys::CONTEXT)),
        bind("cmd-shift-b", NewBrowser, None),
        bind("cmd-d", SplitRight, None),
        bind("cmd-shift-d", SplitDown, None),
        bind("cmd-alt-left", FocusPane(Edge::Left), None),
        bind("cmd-alt-right", FocusPane(Edge::Right), None),
        bind("cmd-alt-up", FocusPane(Edge::Top), None),
        bind("cmd-alt-down", FocusPane(Edge::Bottom), None),
        // gpui-base's `Input` binds these to add cursors; at equal depth the later binding wins, so panels keep them in the code editor too.
        bind("cmd-alt-up", FocusPane(Edge::Top), Some("Input")),
        bind("cmd-alt-down", FocusPane(Edge::Bottom), Some("Input")),
        // macOS reports ⌘⇧[ as `{` with shift already applied (gpui-pre-macos parse_keystroke), so `cmd-shift-[` would never match.
        bind("cmd-{", PrevTab, None),
        bind("cmd-}", NextTab, None),
        bind("cmd-shift-enter", ZoomPane, None),
        bind("cmd-ctrl-=", EqualizePanes, None),
        bind("cmd-l", FocusAddress, Some(browser::CONTEXT)),
        bind("cmd-r", Reload, Some(browser::CONTEXT)),
        bind("cmd-[", Back, Some(browser::CONTEXT)),
        bind("cmd-]", Forward, Some(browser::CONTEXT)),
        bind("cmd-c", PageEdit(web::Edit::Copy), Some(browser::CONTEXT)),
        bind("cmd-x", PageEdit(web::Edit::Cut), Some(browser::CONTEXT)),
        bind("cmd-v", PageEdit(web::Edit::Paste), Some(browser::CONTEXT)),
        bind("cmd-a", PageEdit(web::Edit::SelectAll), Some(browser::CONTEXT)),
        bind("cmd-z", PageEdit(web::Edit::Undo), Some(browser::CONTEXT)),
        bind("cmd-shift-z", PageEdit(web::Edit::Redo), Some(browser::CONTEXT)),
        bind("cmd-l", TreeMenuItem(Item::Mention), Some(menu::CONTEXT)),
        bind("cmd-alt-c", TreeMenuItem(Item::Copy), Some(menu::CONTEXT)),
        bind("cmd-alt-shift-c", TreeMenuItem(Item::CopyRelative), Some(menu::CONTEXT)),
        bind("enter", TreeMenuChoose, Some(menu::CONTEXT)),
        bind("down", TreeMenuArrow(Arrow::Down), Some(menu::CONTEXT)),
        bind("up", TreeMenuArrow(Arrow::Up), Some(menu::CONTEXT)),
        bind("left", TreeMenuArrow(Arrow::Left), Some(menu::CONTEXT)),
        bind("right", TreeMenuArrow(Arrow::Right), Some(menu::CONTEXT)),
        bind("cmd-backspace", TreeMenuItem(Item::Trash), Some(menu::CONTEXT)),
    ];
    out.extend((1..=9).map(|n| bind(&format!("cmd-{n}"), JumpToTab(n), None)));
    // Not ⌘⇧n: macOS takes ⌘⇧3–5 for screenshots before the app sees them.
    out.extend((1..=9).map(|n| bind(&format!("ctrl-{n}"), JumpTo(n), None)));
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
    let mut out: Vec<KeyBinding> = bindings()
        .into_iter()
        .filter_map(|b| match custom.get(&binding_id(&b)) {
            None => Some(b),
            Some(keys) if keys.is_empty() => None,
            Some(keys) => Some(KeyBinding::load(keys, b.action().boxed_clone(), b.predicate(), false, None, &DummyKeyboardMapper).unwrap_or(b)),
        })
        .collect();
    if !cfg!(target_os = "macos") {
        out.extend(terminal_aliases(&out));
    }
    out
}

/// A terminal hands Ctrl and a letter to the shell (`keys::bindings`), so there a shortcut on them answers to Ctrl-Shift and the letter, when nothing else has those.
fn terminal_aliases(bindings: &[KeyBinding]) -> Vec<KeyBinding> {
    let on = |b: &KeyBinding, keys: &str| b.keystrokes().iter().map(|k| k.inner().unparse()).eq([keys.to_string()]);
    let reaches_terminal = |b: &KeyBinding| b.predicate().is_none_or(|p| p.to_string() == keys::CONTEXT);
    bindings
        .iter()
        .filter_map(|b| {
            let [k] = b.keystrokes() else { return None };
            let k = k.inner();
            let ctrl_letter = k.modifiers == Modifiers::control() && k.key.len() == 1 && k.key.chars().all(|c| c.is_ascii_lowercase());
            let alias = format!("ctrl-shift-{}", k.key);
            let free = !bindings.iter().any(|o| reaches_terminal(o) && on(o, &alias));
            (b.predicate().is_none() && ctrl_letter && free).then(|| KeyBinding::load(&alias, b.action().boxed_clone(), KeyBindingContextPredicate::parse(keys::CONTEXT).ok().map(Rc::new), false, None, &DummyKeyboardMapper).unwrap())
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
    use super::{CopySelection, NewTab, SplitDown, SplitRight, binding_id, bindings, keymap, native, off_mac, terminal_aliases};
    use std::collections::BTreeMap;
    use crate::inbox;
    use gpui_kit::{KeyBinding, KeyContext, Keymap, Keystroke, actions};
    use std::collections::HashSet;

    #[test]
    fn no_two_bindings_share_a_keystroke_and_context() {
        let mut seen = HashSet::new();
        for b in keymap(&BTreeMap::new()).into_iter().chain(keys::bindings()) {
            let keys: Vec<String> = b.keystrokes().iter().map(|k| k.unparse()).collect();
            let context = b.predicate().map(|p| p.to_string());
            assert!(seen.insert((keys.clone(), context.clone())), "{keys:?} in {context:?} is bound twice");
        }
    }

    actions!(test, [Submit]);

    /// The actions bound to `keys`, as macOS writes them, outside any context.
    fn bound(keys: &str) -> Vec<&'static str> {
        let want = vec![Keystroke::parse(&native(keys, None)).unwrap().unparse()];
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
            keymap.bindings_for_input(&[Keystroke::parse("secondary-enter").unwrap()], &stack).0.first().map(|b| b.action().name())
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
        let unparsed = |k: &str| Keystroke::parse(k).unwrap().unparse();
        assert_eq!(keys("desktop::OpenPalette"), [unparsed("cmd-shift-p")]);
        assert!(keys("desktop::Reload in Browser").is_empty());
        assert_eq!(keys("desktop::GoToFile"), [unparsed(&native("cmd-p", None))]);
    }

    #[test]
    fn off_macos_cmd_becomes_ctrl_and_ctrl_becomes_alt_except_on_tab() {
        let off = |k: &str| off_mac(k, None);
        assert_eq!([off("cmd-k"), off("cmd-shift-j"), off("ctrl-1"), off("cmd-ctrl-="), off("ctrl-shift-tab"), off("cmd-alt-left")], ["ctrl-k", "ctrl-shift-j", "alt-1", "ctrl-alt-=", "ctrl-shift-tab", "ctrl-alt-left"]);
        assert_eq!(off_mac("cmd-c", Some(keys::CONTEXT)), "ctrl-shift-c");
    }

    #[test]
    fn a_ctrl_letter_shortcut_answers_to_ctrl_shift_in_a_terminal_unless_those_keys_are_taken() {
        let bindings = [KeyBinding::new("ctrl-t", NewTab, None), KeyBinding::new("ctrl-d", SplitRight, None), KeyBinding::new("ctrl-shift-d", SplitDown, None), KeyBinding::new("ctrl-c", CopySelection, Some(keys::CONTEXT))];
        let aliases: Vec<_> = terminal_aliases(&bindings).iter().map(|b| (b.keystrokes()[0].inner().unparse(), b.action().name(), b.predicate().map(|p| p.to_string()))).collect();
        assert_eq!(aliases, [("ctrl-shift-t".to_string(), "desktop::NewTab", Some(keys::CONTEXT.to_string()))]);
    }
}
