mod selection;

pub use selection::{Pos, Selection};
use std::ffi::c_void;

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Cell {
    pub cp: u32,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
    pub has_fg: u8,
    pub has_bg: u8,
    pub bold: u8,
    pub italic: u8,
    pub underline: u8,
    pub inverse: u8,
    pub wide: u8,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
pub struct Frame {
    pub cols: u16,
    pub rows: u16,
    pub cursor_x: u16,
    pub cursor_y: u16,
    pub cursor_visible: u8,
    pub fg: [u8; 3],
    pub bg: [u8; 3],
}

impl Cell {
    pub fn ch(&self) -> char {
        char::from_u32(self.cp).filter(|_| self.cp != 0).unwrap_or(' ')
    }
}

pub const WIDE_SPACER_TAIL: u8 = 2;

unsafe extern "C" {
    fn pt_new(cols: u16, rows: u16) -> *mut c_void;
    fn pt_write(p: *mut c_void, data: *const u8, len: usize);
    fn pt_app_cursor(p: *mut c_void) -> u8;
    fn pt_resize(p: *mut c_void, cols: u16, rows: u16);
    fn pt_frame(p: *mut c_void, f: *mut Frame, out: *mut Cell, cap: usize) -> usize;
    fn pt_free(p: *mut c_void);
}

pub struct Term {
    ptr: *mut c_void,
    cells: Vec<Cell>,
}

impl Term {
    pub fn new(cols: u16, rows: u16) -> Self {
        let ptr = unsafe { pt_new(cols, rows) };
        assert!(!ptr.is_null(), "libghostty-vt: cannot create a {cols}x{rows} terminal");
        Self { ptr, cells: Vec::new() }
    }

    pub fn write(&mut self, data: &[u8]) {
        unsafe { pt_write(self.ptr, data.as_ptr(), data.len()) }
    }

    pub fn resize(&mut self, cols: u16, rows: u16) {
        unsafe { pt_resize(self.ptr, cols, rows) }
    }

    pub fn app_cursor(&self) -> bool {
        unsafe { pt_app_cursor(self.ptr) == 1 }
    }

    pub fn frame(&mut self) -> (Frame, &[Cell]) {
        let mut f = Frame::default();
        unsafe { pt_frame(self.ptr, &mut f, std::ptr::null_mut(), 0) };
        self.cells.resize(f.cols as usize * f.rows as usize, Cell::default());
        let n = unsafe { pt_frame(self.ptr, &mut f, self.cells.as_mut_ptr(), self.cells.len()) };
        (f, &self.cells[..n])
    }
}

impl Drop for Term {
    fn drop(&mut self) {
        unsafe { pt_free(self.ptr) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renders_written_text_style_and_cursor() {
        let mut t = Term::new(10, 2);
        t.write(b"ab\x1b[1mc");
        let (f, cells) = t.frame();
        assert_eq!((f.cols, f.rows, cells.len()), (10, 2, 20));
        let text: String = cells[..3].iter().map(|c| char::from_u32(c.cp).unwrap()).collect();
        assert_eq!(text, "abc");
        assert_eq!((cells[1].bold, cells[2].bold), (0, 1));
        assert_eq!((f.cursor_x, f.cursor_y), (3, 0));
    }

    #[test]
    fn survives_long_combining_sequence() {
        let mut t = Term::new(10, 2);
        t.write(format!("a{}", "\u{301}".repeat(40)).as_bytes());
        let (_, cells) = t.frame();
        assert_eq!(cells[0].cp, 0xFFFD);
    }

    #[test]
    fn resize_changes_the_grid() {
        let mut t = Term::new(10, 2);
        t.resize(4, 3);
        let (f, cells) = t.frame();
        assert_eq!((f.cols, f.rows, cells.len()), (4, 3, 12));
    }

    #[test]
    fn reports_application_cursor_mode() {
        let mut t = Term::new(10, 2);
        assert!(!t.app_cursor());
        t.write(b"\x1b[?1h");
        assert!(t.app_cursor());
        t.write(b"\x1b[?1l");
        assert!(!t.app_cursor());
    }

    #[test]
    #[should_panic(expected = "libghostty-vt")]
    fn new_panics_when_the_terminal_cannot_be_created() {
        Term::new(0, 0);
    }
}
