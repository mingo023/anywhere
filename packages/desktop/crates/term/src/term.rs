mod paste;
mod selection;

pub use paste::{paste_bytes, paste_is_safe};
pub use selection::{Autoscroll, Pointer};
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
    pub selected: u8,
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
    /// 0 while the user has scrolled up into history.
    pub at_bottom: u8,
    /// One of `CURSOR_BAR`, `CURSOR_BLOCK`, `CURSOR_UNDERLINE`, as the program last set it with DECSCUSR.
    pub cursor_style: u8,
    pub cursor_blink: u8,
}

impl Cell {
    pub fn ch(&self) -> char {
        char::from_u32(self.cp).filter(|_| self.cp != 0).unwrap_or(' ')
    }
}

pub const WIDE_SPACER_TAIL: u8 = 2;

pub const CURSOR_BAR: u8 = 0;
pub const CURSOR_BLOCK: u8 = 1;
pub const CURSOR_UNDERLINE: u8 = 2;

/// Most lines of history kept above the screen. libghostty prunes a whole page at a time, so past the cap it keeps a few hundred fewer.
pub const SCROLLBACK: usize = 10_000;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Scroll {
    Bottom,
    /// Rows; negative moves toward older output.
    Delta(isize),
}

unsafe extern "C" {
    fn pt_new(cols: u16, rows: u16, scrollback: usize) -> *mut c_void;
    fn pt_write(p: *mut c_void, data: *const u8, len: usize);
    fn pt_mode(p: *mut c_void, dec: u16) -> u8;
    fn pt_alt_screen(p: *mut c_void) -> u8;
    fn pt_scroll(p: *mut c_void, tag: i32, delta: isize);
    fn pt_scrollback_rows(p: *mut c_void) -> usize;
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
        let ptr = unsafe { pt_new(cols, rows, SCROLLBACK) };
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
        self.mode(1)
    }

    /// Whether DEC private mode `dec` (`CSI ? dec h`) is set.
    pub fn mode(&self, dec: u16) -> bool {
        unsafe { pt_mode(self.ptr, dec) == 1 }
    }

    pub fn alt_screen(&self) -> bool {
        unsafe { pt_alt_screen(self.ptr) == 1 }
    }

    pub fn scroll(&mut self, s: Scroll) {
        let (tag, delta) = match s {
            Scroll::Bottom => (1, 0),
            Scroll::Delta(d) => (2, d),
        };
        unsafe { pt_scroll(self.ptr, tag, delta) }
    }

    pub fn scrollback_rows(&self) -> usize {
        unsafe { pt_scrollback_rows(self.ptr) }
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
    fn the_cursor_starts_as_a_blinking_block_and_follows_decscusr() {
        let mut t = Term::new(10, 2);
        let cursor = |t: &mut Term| {
            let (f, _) = t.frame();
            (f.cursor_style, f.cursor_blink)
        };
        assert_eq!(cursor(&mut t), (CURSOR_BLOCK, 1));
        t.write(b"\x1b[6 q");
        assert_eq!(cursor(&mut t), (CURSOR_BAR, 0));
        t.write(b"\x1b[3 q");
        assert_eq!(cursor(&mut t), (CURSOR_UNDERLINE, 1));
        t.write(b"\x1b[0 q");
        assert_eq!(cursor(&mut t), (CURSOR_BLOCK, 1));
    }

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

    fn row(t: &mut Term, y: usize) -> String {
        let (f, cells) = t.frame();
        cells[y * f.cols as usize..][..f.cols as usize].iter().map(Cell::ch).collect::<String>().trim_end().to_string()
    }

    fn lines(t: &mut Term, range: std::ops::Range<usize>) {
        for i in range {
            t.write(format!("{i}\r\n").as_bytes());
        }
    }

    #[test]
    fn keeps_ten_thousand_lines_of_scrollback() {
        let mut t = Term::new(80, 5);
        lines(&mut t, 0..30_000);
        assert!((9_000..=SCROLLBACK).contains(&t.scrollback_rows()), "{}", t.scrollback_rows());
    }

    #[test]
    fn scrolling_up_leaves_the_bottom_and_back_returns_to_it() {
        let mut t = Term::new(20, 5);
        lines(&mut t, 0..50);
        assert_eq!((t.frame().0.at_bottom, row(&mut t, 0).as_str()), (1, "46"));
        t.scroll(Scroll::Delta(-3));
        assert_eq!((t.frame().0.at_bottom, row(&mut t, 0).as_str()), (0, "43"));
        t.scroll(Scroll::Bottom);
        assert_eq!((t.frame().0.at_bottom, row(&mut t, 0).as_str()), (1, "46"));
    }

    #[test]
    fn output_while_scrolled_up_keeps_the_viewport() {
        let mut t = Term::new(20, 5);
        lines(&mut t, 0..50);
        t.scroll(Scroll::Delta(-10));
        lines(&mut t, 50..60);
        assert_eq!((t.frame().0.at_bottom, row(&mut t, 0).as_str()), (0, "36"));
    }

    #[test]
    fn reports_the_alternate_screen_and_its_scroll_mode() {
        let mut t = Term::new(10, 2);
        assert!(!t.alt_screen() && t.mode(1007));
        t.write(b"\x1b[?1049h\x1b[?1007l");
        assert!(t.alt_screen() && !t.mode(1007));
    }

    #[test]
    fn reports_bracketed_paste_mode() {
        let mut t = Term::new(10, 2);
        assert!(!t.mode(2004));
        t.write(b"\x1b[?2004h");
        assert!(t.mode(2004));
    }

    #[test]
    #[should_panic(expected = "libghostty-vt")]
    fn new_panics_when_the_terminal_cannot_be_created() {
        Term::new(0, 0);
    }
}
