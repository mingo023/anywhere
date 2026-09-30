use crate::{Cell, WIDE_SPACER_TAIL};

/// A boundary between cells: `col` 0 is before a row's first cell, `cols` after its last.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Pos {
    pub row: u16,
    pub col: u16,
}

/// The cells between where a drag began and where it is now, in reading order.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Selection {
    pub anchor: Pos,
    pub head: Pos,
}

impl Selection {
    pub fn at(p: Pos) -> Self {
        Self { anchor: p, head: p }
    }

    pub fn is_empty(&self) -> bool {
        self.anchor == self.head
    }

    pub fn contains(&self, col: u16, row: u16) -> bool {
        let p = Pos { row, col };
        self.anchor.min(self.head) <= p && p < self.anchor.max(self.head)
    }

    /// One line per row, without the blanks that pad a row to the screen's width.
    pub fn text(&self, cells: &[Cell], cols: u16) -> String {
        let (start, end) = (self.anchor.min(self.head), self.anchor.max(self.head));
        let rows = cells.chunks(cols.max(1) as usize).enumerate().skip(start.row as usize).take((end.row - start.row) as usize + 1);
        let lines: Vec<String> = rows
            .map(|(y, row)| {
                let line: String = row.iter().enumerate().filter(|(x, c)| c.wide != WIDE_SPACER_TAIL && self.contains(*x as u16, y as u16)).map(|(_, c)| c.ch()).collect();
                line.trim_end().to_string()
            })
            .collect();
        lines.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(rows: &[&str]) -> Vec<Cell> {
        rows.iter().flat_map(|r| r.chars().map(|c| Cell { cp: if c == '.' { 0 } else { c as u32 }, ..Default::default() })).collect()
    }

    fn sel(from: (u16, u16), to: (u16, u16)) -> Selection {
        Selection { anchor: Pos { row: from.0, col: from.1 }, head: Pos { row: to.0, col: to.1 } }
    }

    #[test]
    fn a_click_without_a_drag_selects_nothing() {
        let s = Selection::at(Pos { row: 0, col: 2 });
        assert!(s.is_empty());
        assert!(!s.contains(2, 0));
        assert_eq!(s.text(&grid(&["abcd"]), 4), "");
    }

    #[test]
    fn covers_cells_between_the_two_boundaries_whichever_way_it_was_dragged() {
        let cells = grid(&["abcd", "efgh"]);
        assert_eq!(sel((0, 1), (0, 3)).text(&cells, 4), "bc");
        assert_eq!(sel((0, 3), (0, 1)).text(&cells, 4), "bc");
        assert_eq!(sel((1, 2), (0, 2)).text(&cells, 4), "cd\nef");
    }

    #[test]
    fn a_drag_across_rows_takes_whole_middle_rows() {
        let s = sel((0, 3), (2, 1));
        assert!(!s.contains(2, 0) && s.contains(3, 0));
        assert!(s.contains(0, 1) && s.contains(3, 1));
        assert!(s.contains(0, 2) && !s.contains(1, 2));
        assert_eq!(s.text(&grid(&["abcd", "efgh", "ijkl"]), 4), "d\nefgh\ni");
    }

    #[test]
    fn copied_rows_drop_trailing_blanks_but_keep_inner_ones() {
        assert_eq!(sel((0, 0), (1, 4)).text(&grid(&["a b.", "...."]), 4), "a b\n");
    }

    #[test]
    fn a_wide_character_is_copied_once() {
        let mut cells = grid(&["a..b"]);
        cells[1] = Cell { cp: '世' as u32, wide: 1, ..Default::default() };
        cells[2].wide = WIDE_SPACER_TAIL;
        assert_eq!(sel((0, 0), (0, 4)).text(&cells, 4), "a世b");
    }
}
