use crate::{Pointer, Term};
use std::ffi::c_void;

unsafe extern "C" {
    fn pt_mouse_tracking(p: *mut c_void) -> u8;
    fn pt_wheel(p: *mut c_void, at: *const Pointer, up: u8, ctrl: u8, alt: u8, out: *mut u8, cap: usize) -> usize;
}

impl Term {
    pub fn mouse_tracking(&self) -> bool {
        unsafe { pt_mouse_tracking(self.ptr) == 1 }
    }

    /// One wheel notch at `at`, encoded in the format the program asked for; empty when its tracking mode leaves the wheel out.
    pub fn wheel_report(&mut self, at: Pointer, up: bool, ctrl: bool, alt: bool) -> Vec<u8> {
        let mut buf = [0u8; 64];
        let n = unsafe { pt_wheel(self.ptr, &at, up as u8, ctrl as u8, alt as u8, buf.as_mut_ptr(), buf.len()) };
        buf[..n].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(row: u16, col: u16) -> Pointer {
        Pointer::at(col as f32 * 8.4 + 1., row as f32 * 17.6 + 1., 8.4, 17.6, 80, 24)
    }

    #[test]
    fn mouse_tracking_follows_the_program() {
        let mut t = Term::new(80, 24);
        assert!(!t.mouse_tracking());
        t.write(b"\x1b[?1000h");
        assert!(t.mouse_tracking());
        t.write(b"\x1b[?1000l");
        assert!(!t.mouse_tracking());
    }

    #[test]
    fn wheel_reports_the_notch_and_cell_in_the_format_asked_for() {
        let mut t = Term::new(80, 24);
        t.write(b"\x1b[?1049h\x1b[?1000h\x1b[?1006h");
        assert_eq!(t.wheel_report(at(0, 0), true, false, false), b"\x1b[<64;1;1M");
        assert_eq!(t.wheel_report(at(23, 79), false, false, false), b"\x1b[<65;80;24M");
        t.write(b"\x1b[?1006l");
        assert_eq!(t.wheel_report(at(2, 4), true, false, false), b"\x1b[M`%#");
    }

    #[test]
    fn wheel_reports_carry_ctrl_and_alt() {
        let mut t = Term::new(80, 24);
        t.write(b"\x1b[?1000h\x1b[?1006h");
        assert_eq!(t.wheel_report(at(0, 0), true, true, false), b"\x1b[<80;1;1M");
        assert_eq!(t.wheel_report(at(0, 0), true, false, true), b"\x1b[<72;1;1M");
    }

    #[test]
    fn nothing_is_reported_without_tracking_or_in_x10_mode() {
        let mut t = Term::new(80, 24);
        assert!(t.wheel_report(at(0, 0), true, false, false).is_empty());
        t.write(b"\x1b[?9h");
        assert!(t.wheel_report(at(0, 0), true, false, false).is_empty());
    }
}
