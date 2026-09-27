use gpui_kit::{KeyBinding, Keystroke, NoAction};

pub const CONTEXT: &str = "Terminal";

/// gpui-component's Root binds tab and shift-tab to focus cycling; in the terminal they belong to the CLI.
pub fn bindings() -> [KeyBinding; 2] {
    [KeyBinding::new("tab", NoAction {}, Some(CONTEXT)), KeyBinding::new("shift-tab", NoAction {}, Some(CONTEXT))]
}

pub fn key_bytes(k: &Keystroke, app_cursor: bool) -> Option<Vec<u8>> {
    let m = &k.modifiers;
    let cursor = |c: &str| format!("\x1b{}{c}", if app_cursor { 'O' } else { '[' });
    let seq = match k.key.as_str() {
        "enter" => "\r".into(),
        "backspace" => "\x7f".into(),
        "escape" => "\x1b".into(),
        "tab" if m.shift => "\x1b[Z".into(),
        "tab" => "\t".into(),
        "up" => cursor("A"),
        "down" => cursor("B"),
        "right" => cursor("C"),
        "left" => cursor("D"),
        "home" => cursor("H"),
        "end" => cursor("F"),
        "delete" => "\x1b[3~".into(),
        "pageup" => "\x1b[5~".into(),
        "pagedown" => "\x1b[6~".into(),
        "f1" => "\x1bOP".into(),
        "f2" => "\x1bOQ".into(),
        "f3" => "\x1bOR".into(),
        "f4" => "\x1bOS".into(),
        "f5" => "\x1b[15~".into(),
        "f6" => "\x1b[17~".into(),
        "f7" => "\x1b[18~".into(),
        "f8" => "\x1b[19~".into(),
        "f9" => "\x1b[20~".into(),
        "f10" => "\x1b[21~".into(),
        "f11" => "\x1b[23~".into(),
        "f12" => "\x1b[24~".into(),
        "space" | "2" if m.control => "\0".into(),
        "/" if m.control => "\x1f".into(),
        "space" => " ".into(),
        key if m.control && key.len() == 1 => char::from(key.as_bytes()[0] & 0x1f).into(),
        key if m.alt && key.len() == 1 => if m.shift { key.to_ascii_uppercase() } else { key.to_string() },
        _ => return None,
    };
    Some(if m.alt { format!("\x1b{seq}") } else { seq }.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{KeyContext, Keymap, actions};

    fn bytes(s: &str) -> Option<Vec<u8>> {
        key_bytes(&Keystroke::parse(s).unwrap(), false)
    }

    #[test]
    fn maps_named_keys_to_terminal_sequences() {
        assert_eq!(bytes("enter"), Some(b"\r".to_vec()));
        assert_eq!(bytes("escape"), Some(b"\x1b".to_vec()));
        assert_eq!(bytes("shift-tab"), Some(b"\x1b[Z".to_vec()));
        assert_eq!(bytes("up"), Some(b"\x1b[A".to_vec()));
    }

    #[test]
    fn maps_control_letters_to_c0_codes() {
        assert_eq!(bytes("ctrl-c"), Some(vec![3]));
        assert_eq!(bytes("ctrl-space"), Some(vec![0]));
        assert_eq!(bytes("ctrl-2"), Some(vec![0]));
        assert_eq!(bytes("ctrl-/"), Some(vec![0x1f]));
    }

    #[test]
    fn alt_prefixes_escape() {
        let mut k = Keystroke::parse("alt-b").unwrap();
        k.key_char = Some("∫".into());
        assert_eq!(key_bytes(&k, false), Some(b"\x1bb".to_vec()));
        assert_eq!(bytes("alt-shift-b"), Some(b"\x1bB".to_vec()));
        assert_eq!(bytes("alt-backspace"), Some(b"\x1b\x7f".to_vec()));
        assert_eq!(bytes("alt-ctrl-c"), Some(b"\x1b\x03".to_vec()));
    }

    #[test]
    fn maps_editing_and_function_keys() {
        assert_eq!(bytes("delete"), Some(b"\x1b[3~".to_vec()));
        assert_eq!(bytes("home"), Some(b"\x1b[H".to_vec()));
        assert_eq!(bytes("end"), Some(b"\x1b[F".to_vec()));
        assert_eq!(bytes("pageup"), Some(b"\x1b[5~".to_vec()));
        assert_eq!(bytes("pagedown"), Some(b"\x1b[6~".to_vec()));
        assert_eq!(bytes("f1"), Some(b"\x1bOP".to_vec()));
        assert_eq!(bytes("f4"), Some(b"\x1bOS".to_vec()));
        assert_eq!(bytes("f5"), Some(b"\x1b[15~".to_vec()));
        assert_eq!(bytes("f11"), Some(b"\x1b[23~".to_vec()));
        assert_eq!(bytes("f12"), Some(b"\x1b[24~".to_vec()));
    }

    #[test]
    fn arrows_follow_application_cursor_mode() {
        let app = |s| key_bytes(&Keystroke::parse(s).unwrap(), true);
        assert_eq!(app("up"), Some(b"\x1bOA".to_vec()));
        assert_eq!(app("left"), Some(b"\x1bOD".to_vec()));
        assert_eq!(app("home"), Some(b"\x1bOH".to_vec()));
        assert_eq!(bytes("left"), Some(b"\x1b[D".to_vec()));
    }

    #[test]
    fn leaves_typed_characters_to_text_input() {
        let mut k = Keystroke::parse("e").unwrap();
        k.key_char = Some("é".into());
        assert_eq!(key_bytes(&k, false), None);
        assert_eq!(bytes("cmd-c"), None);
        assert_eq!(bytes("shift"), None);
    }

    actions!(test, [CycleFocus]);

    #[test]
    fn tab_reaches_the_terminal_past_root_focus_cycling() {
        let mut keymap = Keymap::default();
        keymap.add_bindings([KeyBinding::new("tab", CycleFocus, Some("Root")), KeyBinding::new("shift-tab", CycleFocus, Some("Root"))]);
        keymap.add_bindings(bindings());
        let stack = [KeyContext::parse("Root").unwrap(), KeyContext::parse(CONTEXT).unwrap()];
        for key in ["tab", "shift-tab"] {
            assert!(keymap.bindings_for_input(&[Keystroke::parse(key).unwrap()], &stack).0.is_empty(), "{key}");
        }
    }
}
