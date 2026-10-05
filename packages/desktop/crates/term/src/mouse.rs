use crate::{Pointer, Term};
use std::ffi::c_void;

unsafe extern "C" {
    fn pt_mouse_tracking(p: *mut c_void) -> u8;
    fn pt_mouse(p: *mut c_void, at: *const Pointer, action: u8, button: u8, ctrl: u8, alt: u8, out: *mut u8, cap: usize) -> usize;
}

/// Values are libghostty's `GhosttyMouseAction`.
#[repr(u8)]
#[derive(Clone, Copy)]
pub enum MouseAction {
    Press = 0,
    Release = 1,
    Motion = 2,
}

/// Values are libghostty's `GhosttyMouseButton`.
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Button {
    Left = 1,
    Right = 2,
    Middle = 3,
    WheelUp = 4,
    WheelDown = 5,
}

impl Term {
    pub fn mouse_tracking(&self) -> bool {
        unsafe { pt_mouse_tracking(self.ptr) == 1 }
    }

    /// One wheel notch at `at`, encoded in the format the program asked for; empty when its tracking mode leaves the wheel out.
    pub fn wheel_report(&mut self, at: Pointer, up: bool, ctrl: bool, alt: bool) -> Vec<u8> {
        self.mouse_report(at, MouseAction::Press, Some(if up { Button::WheelUp } else { Button::WheelDown }), ctrl, alt)
    }

    /// A mouse event at `at`, `button` being the one pressed, released or held; empty when the program's tracking mode leaves it out.
    pub fn mouse_report(&mut self, at: Pointer, action: MouseAction, button: Option<Button>, ctrl: bool, alt: bool) -> Vec<u8> {
        let mut buf = [0u8; 64];
        let button = button.map_or(0, |b| b as u8);
        let n = unsafe { pt_mouse(self.ptr, &at, action as u8, button, ctrl as u8, alt as u8, buf.as_mut_ptr(), buf.len()) };
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
    fn clicks_are_reported_as_press_and_release() {
        let mut t = Term::new(80, 24);
        t.write(b"\x1b[?1000h\x1b[?1006h");
        assert_eq!(t.mouse_report(at(2, 4), MouseAction::Press, Some(Button::Left), false, false), b"\x1b[<0;5;3M");
        assert_eq!(t.mouse_report(at(2, 4), MouseAction::Release, Some(Button::Left), false, false), b"\x1b[<0;5;3m");
        assert_eq!(t.mouse_report(at(2, 4), MouseAction::Press, Some(Button::Right), false, false), b"\x1b[<2;5;3M");
    }

    #[test]
    fn drags_are_reported_only_in_button_event_mode() {
        let mut t = Term::new(80, 24);
        t.write(b"\x1b[?1000h\x1b[?1006h");
        assert!(t.mouse_report(at(2, 4), MouseAction::Motion, Some(Button::Left), false, false).is_empty());
        t.write(b"\x1b[?1002h");
        assert_eq!(t.mouse_report(at(2, 5), MouseAction::Motion, Some(Button::Left), false, false), b"\x1b[<32;6;3M");
    }

    #[test]
    fn bare_motion_is_reported_only_in_any_event_mode() {
        let mut t = Term::new(80, 24);
        t.write(b"\x1b[?1002h\x1b[?1006h");
        assert!(t.mouse_report(at(2, 4), MouseAction::Motion, None, false, false).is_empty());
        t.write(b"\x1b[?1003h");
        assert_eq!(t.mouse_report(at(2, 5), MouseAction::Motion, None, false, false), b"\x1b[<35;6;3M");
    }

    #[test]
    fn motion_within_one_cell_is_reported_once() {
        let mut t = Term::new(80, 24);
        t.write(b"\x1b[?1003h\x1b[?1006h");
        let inside = Pointer::at(4. * 8.4 + 6., 2. * 17.6 + 9., 8.4, 17.6, 80, 24);
        assert_eq!(t.mouse_report(at(2, 4), MouseAction::Motion, None, false, false), b"\x1b[<35;5;3M");
        assert!(t.mouse_report(inside, MouseAction::Motion, None, false, false).is_empty());
        assert_eq!(t.mouse_report(at(2, 5), MouseAction::Motion, None, false, false), b"\x1b[<35;6;3M");
    }

    #[test]
    fn a_restarted_program_hears_the_first_move_even_in_the_same_cell() {
        let mut t = Term::new(80, 24);
        t.write(b"\x1b[?1003h\x1b[?1006h");
        t.mouse_report(at(2, 4), MouseAction::Motion, None, false, false);
        t.write(b"\x1b[?1003l");
        assert!(t.mouse_report(at(2, 4), MouseAction::Motion, None, false, false).is_empty());
        t.write(b"\x1b[?1003h");
        assert_eq!(t.mouse_report(at(2, 4), MouseAction::Motion, None, false, false), b"\x1b[<35;5;3M");
    }

    #[test]
    fn clicks_are_not_reported_without_tracking() {
        let mut t = Term::new(80, 24);
        t.write(b"\x1b[?1006h");
        assert!(t.mouse_report(at(2, 4), MouseAction::Press, Some(Button::Left), false, false).is_empty());
        assert!(t.mouse_report(at(2, 4), MouseAction::Release, Some(Button::Left), false, false).is_empty());
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
