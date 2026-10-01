use std::ffi::c_char;

unsafe extern "C" {
    fn ghostty_paste_is_safe(data: *const c_char, len: usize) -> bool;
    fn ghostty_paste_encode(data: *mut c_char, len: usize, bracketed: bool, buf: *mut c_char, buf_len: usize, out_written: *mut usize) -> i32;
}

/// Whether `text` can't run anything by itself: no newline, and no marker that ends a bracketed paste.
pub fn paste_is_safe(text: &str) -> bool {
    unsafe { ghostty_paste_is_safe(text.as_ptr().cast(), text.len()) }
}

/// What a paste sends: control bytes blanked, then either wrapped in bracketed-paste markers or with newlines as returns.
pub fn paste_bytes(text: &str, bracketed: bool) -> Vec<u8> {
    let mut data = text.as_bytes().to_vec();
    // The two 6-byte bracket markers are all encoding ever adds.
    let mut out = vec![0u8; data.len() + 12];
    let mut n = 0;
    let r = unsafe { ghostty_paste_encode(data.as_mut_ptr().cast(), data.len(), bracketed, out.as_mut_ptr().cast(), out.len(), &mut n) };
    assert_eq!(r, 0, "libghostty-vt: paste_encode needed {n} bytes");
    out.truncate(n);
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bracketed_paste_is_wrapped() {
        assert_eq!(paste_bytes("ls\n", true), b"\x1b[200~ls\n\x1b[201~");
    }

    #[test]
    fn unbracketed_newlines_become_returns() {
        assert_eq!(paste_bytes("a\nb", false), b"a\rb");
    }

    #[test]
    fn control_bytes_are_stripped_from_pastes() {
        assert_eq!(paste_bytes("a\x1b[201~b", true), b"\x1b[200~a [201~b\x1b[201~");
    }

    #[test]
    fn a_multi_line_paste_is_not_safe() {
        assert!(paste_is_safe("ls -la"));
        assert!(!paste_is_safe("ls\nrm -rf ~"));
        assert!(!paste_is_safe("a\x1b[201~b"));
    }
}
