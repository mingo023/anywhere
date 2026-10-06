use gpui_kit::{KeyBinding, Keystroke, NoAction};
use term::{Mods, Term};

pub const CONTEXT: &str = "Terminal";

/// gpui-component's Root binds tab and shift-tab to focus cycling, and Pocket binds cmd-enter to open the inbox's session; in the terminal they belong to the CLI.
pub fn bindings() -> [KeyBinding; 3] {
    [KeyBinding::new("tab", NoAction {}, Some(CONTEXT)), KeyBinding::new("shift-tab", NoAction {}, Some(CONTEXT)), KeyBinding::new("cmd-enter", NoAction {}, Some(CONTEXT))]
}

/// What a key down sends to the pty, in the encoding the program asked for; `None` for text, which arrives through the input handler, and for Pocket's own ⌘ keys.
pub fn key_bytes(k: &Keystroke, term: &mut Term) -> Option<Vec<u8>> {
    let m = &k.modifiers;
    let single_char = k.key.chars().count() == 1;
    let newline = !m.control && !m.alt && m.platform != m.shift;
    match k.key.as_str() {
        "left" if m.alt => return Some(b"\x1bb".to_vec()),
        "right" if m.alt => return Some(b"\x1bf".to_vec()),
        "left" if m.platform => return Some(vec![0x01]),
        "right" if m.platform => return Some(vec![0x05]),
        "backspace" if m.platform => return Some(vec![0x15]),
        // The newline Claude Code's /terminal-setup installs; other agent CLIs read it as alt-enter, whatever keyboard protocol they're on.
        "enter" if newline => return Some(b"\x1b\r".to_vec()),
        // Shells would print a ⌘ key's escape sequence; only programs on the Kitty protocol expect one.
        _ if m.platform && !term.kitty_keyboard() => return None,
        _ if single_char && !m.control && !m.alt && !m.platform => return None,
        _ => {}
    }
    let text = match k.key.as_str() {
        "space" => " ".into(),
        key if single_char && m.shift => key.to_uppercase(),
        key if single_char => key.into(),
        _ => String::new(),
    };
    let mods = Mods { shift: m.shift, ctrl: m.control, alt: m.alt, cmd: m.platform };
    let bytes = term.key_report(&k.key, mods, &text);
    (!bytes.is_empty()).then_some(bytes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::{KeyContext, Keymap, actions};

    fn bytes(s: &str) -> Option<Vec<u8>> {
        key_bytes(&Keystroke::parse(s).unwrap(), &mut Term::new(80, 24))
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
        assert_eq!(key_bytes(&k, &mut Term::new(80, 24)), Some(b"\x1bb".to_vec()));
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
        let mut t = Term::new(80, 24);
        t.write(b"\x1b[?1h");
        let mut app = |s| key_bytes(&Keystroke::parse(s).unwrap(), &mut t);
        assert_eq!(app("up"), Some(b"\x1bOA".to_vec()));
        assert_eq!(app("left"), Some(b"\x1bOD".to_vec()));
        assert_eq!(app("home"), Some(b"\x1bOH".to_vec()));
        assert_eq!(bytes("left"), Some(b"\x1b[D".to_vec()));
    }

    #[test]
    fn leaves_typed_characters_to_text_input() {
        let mut k = Keystroke::parse("e").unwrap();
        k.key_char = Some("é".into());
        assert_eq!(key_bytes(&k, &mut Term::new(80, 24)), None);
        assert_eq!(bytes("cmd-c"), None);
        assert_eq!(bytes("shift"), None);
        let mut kitty = Term::new(80, 24);
        kitty.write(b"\x1b[>5u");
        assert_eq!(key_bytes(&Keystroke::parse("a").unwrap(), &mut kitty), None);
    }

    #[test]
    fn option_arrows_move_by_word() {
        assert_eq!(bytes("alt-left"), Some(b"\x1bb".to_vec()));
        assert_eq!(bytes("alt-right"), Some(b"\x1bf".to_vec()));
    }

    #[test]
    fn command_arrows_go_to_line_start_and_end() {
        assert_eq!(bytes("cmd-left"), Some(vec![0x01]));
        assert_eq!(bytes("cmd-right"), Some(vec![0x05]));
    }

    #[test]
    fn command_backspace_kills_the_line() {
        assert_eq!(bytes("cmd-backspace"), Some(vec![0x15]));
    }

    #[test]
    fn command_keys_never_reach_a_shell() {
        for key in ["cmd-v", "cmd-a", "cmd-k", "cmd-up", "cmd-shift-t"] {
            assert_eq!(bytes(key), None, "{key}");
        }
    }

    #[test]
    fn modified_enter_reaches_programs_on_the_kitty_protocol() {
        let mut t = Term::new(80, 24);
        t.write(b"\x1b[>5u");
        let mut kitty = |s| key_bytes(&Keystroke::parse(s).unwrap(), &mut t);
        assert_eq!(kitty("ctrl-enter"), Some(b"\x1b[13;5u".to_vec()));
        assert_eq!(kitty("enter"), Some(b"\r".to_vec()));
    }

    #[test]
    fn command_and_shift_enter_insert_a_newline_in_any_keyboard_protocol() {
        for setup in [&b""[..], b"\x1b[>5u"] {
            let mut t = Term::new(80, 24);
            t.write(setup);
            for key in ["cmd-enter", "shift-enter"] {
                assert_eq!(key_bytes(&Keystroke::parse(key).unwrap(), &mut t), Some(b"\x1b\r".to_vec()), "{key}");
            }
        }
    }

    actions!(test, [CycleFocus, OpenSession]);

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

    #[test]
    fn cmd_enter_reaches_the_terminal_past_open_session() {
        let mut keymap = Keymap::default();
        keymap.add_bindings([KeyBinding::new("cmd-enter", OpenSession, None)]);
        keymap.add_bindings(bindings());
        let stack = [KeyContext::parse("Root").unwrap(), KeyContext::parse(CONTEXT).unwrap()];
        assert!(keymap.bindings_for_input(&[Keystroke::parse("cmd-enter").unwrap()], &stack).0.is_empty());
    }
}
