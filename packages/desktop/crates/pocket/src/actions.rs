use gpui_kit::*;

actions!(desktop, [OpenPalette, GoToFile, OpenSession, StartSession, NextNeedsYou, GoToUpNext, NextSession, PrevSession, ToggleRail, ToggleFocus, NewWorktree, ProjectSettings, NewTab, CopySelection, SelectAll, Paste, CloseTab, Save, Quit]);

/// The nth session in the visible list, 1-based.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct JumpTo(pub usize);

pub fn bindings() -> Vec<KeyBinding> {
    let mut out = vec![
        KeyBinding::new("cmd-k", OpenPalette, None),
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
    ];
    out.extend((1..=9).map(|n| KeyBinding::new(&format!("cmd-{n}"), JumpTo(n), None)));
    out
}

#[cfg(test)]
mod tests {
    use super::bindings;
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
}
