use crate::Term;
use std::ffi::c_void;

/// Where the mouse is over the grid: the cell under it, kept inside the grid, and its offset in pixels from the grid's top-left, which may fall outside it while dragging.
#[repr(C)]
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pointer {
    pub col: u16,
    pub row: u16,
    pub x: f64,
    pub y: f64,
    pub cell_width: u32,
    pub height: u32,
}

impl Pointer {
    pub fn at(x: f32, y: f32, cell: f32, line: f32, cols: u16, rows: u16) -> Self {
        let col = (x / cell).floor().clamp(0., cols.saturating_sub(1) as f32) as u16;
        let row = (y / line).floor().clamp(0., rows.saturating_sub(1) as f32) as u16;
        Self { col, row, x: x as f64, y: y as f64, cell_width: cell.round().max(1.) as u32, height: (rows as f32 * line).round() as u32 }
    }
}

/// Which way a drag held past the grid's edge wants the viewport to move.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Autoscroll {
    None,
    Up,
    Down,
}

impl Autoscroll {
    fn from(v: u8) -> Self {
        match v {
            1 => Self::Up,
            2 => Self::Down,
            _ => Self::None,
        }
    }
}

unsafe extern "C" {
    fn pt_press(p: *mut c_void, at: *const Pointer, clicks: u8);
    fn pt_drag(p: *mut c_void, at: *const Pointer) -> u8;
    fn pt_tick(p: *mut c_void, at: *const Pointer) -> u8;
    fn pt_release(p: *mut c_void);
    fn pt_extend(p: *mut c_void, at: *const Pointer) -> u8;
    fn pt_select_all(p: *mut c_void);
    fn pt_select_none(p: *mut c_void);
    fn pt_selection_text(p: *mut c_void, len: *mut usize) -> *mut u8;
    fn pt_text_free(buf: *mut u8, len: usize);
}

/// Selection lives in libghostty, pinned to the text rather than the screen, so it survives scrolling and new output.
impl Term {
    /// Starts a selection; a double click takes the word, a triple click the line.
    pub fn press(&mut self, at: Pointer, clicks: usize) {
        unsafe { pt_press(self.ptr, &at, clicks.min(3) as u8) }
    }

    pub fn drag(&mut self, at: Pointer) -> Autoscroll {
        Autoscroll::from(unsafe { pt_drag(self.ptr, &at) })
    }

    /// Scrolls one row toward the edge the drag is held past and extends the selection there.
    pub fn autoscroll(&mut self, at: Pointer) -> Autoscroll {
        Autoscroll::from(unsafe { pt_tick(self.ptr, &at) })
    }

    pub fn release(&mut self) {
        unsafe { pt_release(self.ptr) }
    }

    /// Moves the selection's far end to `at`, and says whether there was one to move.
    pub fn extend(&mut self, at: Pointer) -> bool {
        unsafe { pt_extend(self.ptr, &at) == 1 }
    }

    pub fn select_all(&mut self) {
        unsafe { pt_select_all(self.ptr) }
    }

    pub fn select_none(&mut self) {
        unsafe { pt_select_none(self.ptr) }
    }

    /// The selection as plain text, soft wraps joined and trailing blanks trimmed.
    pub fn selection_text(&self) -> Option<String> {
        let mut len = 0;
        let buf = unsafe { pt_selection_text(self.ptr, &mut len) };
        if buf.is_null() {
            return None;
        }
        let text = String::from_utf8_lossy(unsafe { std::slice::from_raw_parts(buf, len) }).into_owned();
        unsafe { pt_text_free(buf, len) };
        Some(text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const CELL: f32 = 10.;
    const LINE: f32 = 20.;

    fn term(rows: &[&str]) -> Term {
        let mut t = Term::new(20, rows.len() as u16);
        t.write(rows.join("\r\n").as_bytes());
        t
    }

    fn at(row: u16, col: u16) -> Pointer {
        Pointer::at(col as f32 * CELL, row as f32 * LINE + 1., CELL, LINE, 20, 3)
    }

    fn drag(t: &mut Term, from: (u16, u16), to: (u16, u16)) -> Option<String> {
        t.press(at(from.0, from.1), 1);
        t.drag(at(to.0, to.1));
        t.release();
        t.selection_text()
    }

    fn selected(t: &mut Term) -> Vec<usize> {
        t.frame().1.iter().enumerate().filter(|(_, c)| c.selected == 1).map(|(i, _)| i).collect()
    }

    #[test]
    fn a_click_without_a_drag_selects_nothing() {
        let mut t = term(&["abcd"]);
        assert_eq!(drag(&mut t, (0, 2), (0, 2)), None);
        assert!(selected(&mut t).is_empty());
    }

    #[test]
    fn covers_cells_between_the_two_boundaries_whichever_way_it_was_dragged() {
        let mut t = term(&["abcd", "efgh"]);
        assert_eq!(drag(&mut t, (0, 1), (0, 3)).as_deref(), Some("bc"));
        assert_eq!(drag(&mut t, (0, 3), (0, 1)).as_deref(), Some("bc"));
        assert_eq!(drag(&mut t, (1, 2), (0, 2)).as_deref(), Some("cd\nef"));
    }

    #[test]
    fn a_drag_across_rows_takes_whole_middle_rows() {
        let mut t = term(&["abcd", "efgh", "ijkl"]);
        assert_eq!(drag(&mut t, (0, 3), (2, 1)).as_deref(), Some("d\nefgh\ni"));
        assert_eq!(selected(&mut t), (3..=40).collect::<Vec<_>>());
    }

    #[test]
    fn copied_rows_drop_trailing_blanks_but_keep_inner_ones() {
        let mut t = term(&["a b", ""]);
        assert_eq!(drag(&mut t, (0, 0), (1, 4)).as_deref(), Some("a b"));
    }

    #[test]
    fn a_wide_character_is_copied_once() {
        let mut t = term(&["a世b"]);
        assert_eq!(drag(&mut t, (0, 0), (0, 4)).as_deref(), Some("a世b"));
    }

    #[test]
    fn a_double_click_takes_the_word_and_a_triple_click_the_line() {
        let mut t = Term::new(20, 2);
        t.write(b"hello world");
        t.press(at(0, 7), 1);
        t.press(at(0, 7), 2);
        assert_eq!(t.selection_text().as_deref(), Some("world"));
        t.press(at(0, 7), 3);
        assert_eq!(t.selection_text().as_deref(), Some("hello world"));
    }

    #[test]
    fn a_new_click_clears_the_selection() {
        let mut t = term(&["abcd"]);
        drag(&mut t, (0, 0), (0, 3));
        t.press(at(0, 1), 1);
        assert_eq!(t.selection_text(), None);
    }

    #[test]
    fn shift_click_moves_the_far_end() {
        let mut t = term(&["abcd", "efgh"]);
        assert!(!t.extend(at(1, 1)));
        drag(&mut t, (0, 1), (0, 3));
        assert!(t.extend(at(1, 1)));
        assert_eq!(t.selection_text().as_deref(), Some("bcd\nef"));
    }

    #[test]
    fn the_selection_stays_on_its_text_through_output_and_scrolling() {
        let mut t = Term::new(10, 3);
        t.write(b"one\r\ntwo\r\nthree");
        t.press(at(1, 0), 1);
        t.drag(at(1, 3));
        t.release();
        t.write(b"\r\nfour\r\nfive");
        assert_eq!(t.selection_text().as_deref(), Some("two"));
        t.scroll(crate::Scroll::Delta(-2));
        assert_eq!(selected(&mut t), vec![10, 11, 12]);
    }

    #[test]
    fn dragging_past_the_top_scrolls_into_history_and_keeps_selecting() {
        let mut t = Term::new(10, 3);
        t.write(b"1\r\n2\r\n3\r\n4\r\n5");
        t.press(at(2, 1), 1);
        let above = Pointer { y: -5., ..at(0, 0) };
        assert_eq!(t.drag(above), Autoscroll::Up);
        assert_eq!(t.autoscroll(above), Autoscroll::Up);
        t.autoscroll(above);
        t.release();
        assert_eq!(t.frame().0.at_bottom, 0);
        assert_eq!(t.selection_text().as_deref(), Some("1\n2\n3\n4\n5"));
    }

    #[test]
    fn select_all_takes_the_history_too() {
        let mut t = Term::new(10, 2);
        t.write(b"1\r\n2\r\n3\r\n4");
        t.select_all();
        assert_eq!(t.selection_text().as_deref(), Some("1\n2\n3\n4"));
        t.select_none();
        assert_eq!(t.selection_text(), None);
    }

    #[test]
    fn a_point_off_the_grid_keeps_to_its_edge() {
        let cell = |x, y| {
            let p = Pointer::at(x, y, 8., 20., 10, 5);
            (p.col, p.row)
        };
        assert_eq!(cell(-5., -3.), (0, 0));
        assert_eq!(cell(500., 500.), (9, 4));
    }
}
