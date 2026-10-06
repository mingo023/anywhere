use crate::Term;
use std::ffi::c_void;

unsafe extern "C" {
    fn pt_kitty_keyboard(p: *mut c_void) -> u8;
    fn pt_key(p: *mut c_void, name: *const u8, name_len: usize, mods: Mods, text: *const u8, text_len: usize, unshifted: u32, out: *mut u8, cap: usize) -> usize;
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Mods {
    pub shift: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub cmd: bool,
}

impl Term {
    /// Whether the program turned on the Kitty keyboard protocol.
    pub fn kitty_keyboard(&self) -> bool {
        unsafe { pt_kitty_keyboard(self.ptr) == 1 }
    }

    /// A key press, encoded in the format the program asked for; empty when it has none.
    /// `key` is a name like `enter` or the key's character, `text` what the key types.
    pub fn key_report(&mut self, key: &str, mods: Mods, text: &str) -> Vec<u8> {
        let mut buf = [0u8; 64];
        let mut chars = key.chars();
        let unshifted = chars.next().filter(|_| chars.next().is_none()).map_or(0, u32::from);
        let n = unsafe { pt_key(self.ptr, key.as_ptr(), key.len(), mods, text.as_ptr(), text.len(), unshifted, buf.as_mut_ptr(), buf.len()) };
        buf[..n].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHIFT: Mods = Mods { shift: true, ctrl: false, alt: false, cmd: false };
    const CMD: Mods = Mods { shift: false, ctrl: false, alt: false, cmd: true };

    #[test]
    fn modified_enter_follows_the_kitty_keyboard_protocol_once_the_program_asks() {
        let mut t = Term::new(80, 24);
        assert!(!t.kitty_keyboard());
        t.write(b"\x1b[>5u");
        assert!(t.kitty_keyboard());
        assert_eq!(t.key_report("enter", CMD, ""), b"\x1b[13;9u");
        assert_eq!(t.key_report("enter", SHIFT, ""), b"\x1b[13;2u");
        assert_eq!(t.key_report("enter", Mods::default(), ""), b"\r");
    }

    #[test]
    fn characters_are_encoded_by_their_text() {
        let mut t = Term::new(80, 24);
        assert_eq!(t.key_report("c", Mods { ctrl: true, ..Mods::default() }, "c"), [3]);
        assert_eq!(t.key_report("b", Mods { alt: true, ..Mods::default() }, "b"), b"\x1bb");
    }

    #[test]
    fn unknown_keys_without_text_encode_to_nothing() {
        let mut t = Term::new(80, 24);
        assert!(t.key_report("shift", SHIFT, "").is_empty());
    }
}
