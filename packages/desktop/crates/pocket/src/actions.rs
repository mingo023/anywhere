use gpui_kit::*;

actions!(desktop, [OpenPalette, GoToFile, OpenSession, StartSession, NextWaiting, ToggleRail, ToggleFocus, NewWorktree, ProjectSettings, NewTab]);

pub fn bindings() -> Vec<KeyBinding> {
    vec![
        KeyBinding::new("cmd-k", OpenPalette, None),
        KeyBinding::new("cmd-p", GoToFile, None),
        KeyBinding::new("cmd-n", StartSession, None),
        KeyBinding::new("cmd-j", NextWaiting, None),
        KeyBinding::new("cmd-\\", ToggleRail, None),
        KeyBinding::new("cmd-.", ToggleFocus, None),
        KeyBinding::new("cmd-t", NewTab, None),
        KeyBinding::new("cmd-shift-n", NewWorktree, None),
        KeyBinding::new("cmd-,", ProjectSettings, None),
        KeyBinding::new("cmd-enter", OpenSession, None),
    ]
}
