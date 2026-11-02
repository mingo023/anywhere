use gpui::{Bounds, Pixels, Point, point};

use crate::global_state::confine_to_cell;

/// [`confine_to_cell`], and for a drag that didn't start in a cell, its ends
/// pinned to the cell painting now (see [`clamp_to_cell`]): a drag into the
/// table from above selects in reading order, not the band on screen.
pub(super) fn select_in_cell(
    anchor_cell: Option<(Point<Pixels>, Bounds<Pixels>)>,
    painting: Option<Bounds<Pixels>>,
    anchor: Point<Pixels>,
    cursor: Point<Pixels>,
) -> Option<(Point<Pixels>, Point<Pixels>)> {
    let in_anchor_cell = anchor_cell.is_some_and(|(cell_anchor, _)| cell_anchor == anchor);
    let (anchor, cursor) = confine_to_cell(anchor_cell, painting, anchor, cursor)?;
    Some(match painting {
        Some(cell) if !in_anchor_cell => (clamp_to_cell(anchor, cell), clamp_to_cell(cursor, cell)),
        _ => (anchor, cursor),
    })
}

/// Moves a selection endpoint into `cell` in reading order: one in an earlier
/// row, or left of the cell in its row, goes to the cell's start; one in a
/// later row, or right of it, to its end. Cells stretch to their row's height,
/// so the cell's top and bottom are the row's.
fn clamp_to_cell(p: Point<Pixels>, cell: Bounds<Pixels>) -> Point<Pixels> {
    let start = cell.origin;
    let end = point(cell.right(), cell.bottom());
    if p.y < cell.top() || (p.y < cell.bottom() && p.x < cell.left()) {
        start
    } else if p.y >= cell.bottom() || p.x >= cell.right() {
        end
    } else {
        p
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::text::inline::point_in_text_selection;
    use gpui::{px, size};

    fn cell(x: f32, y: f32, w: f32, h: f32) -> Bounds<Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(w), px(h)))
    }

    fn selects(
        char_at: Point<Pixels>,
        anchor: Point<Pixels>,
        cursor: Point<Pixels>,
        anchor_cell: Option<Bounds<Pixels>>,
        cell: Bounds<Pixels>,
    ) -> bool {
        select_in_cell(anchor_cell.map(|c| (anchor, c)), Some(cell), anchor, cursor).is_some_and(
            |(start, end)| point_in_text_selection(char_at, px(10.), start, end, px(24.)),
        )
    }

    #[test]
    fn a_drag_inside_one_cell_leaves_the_cell_beside_it_unselected() {
        let left = cell(0., 0., 200., 80.);
        let right = cell(200., 0., 400., 80.);
        let (anchor, cursor) = (point(px(12.), px(20.)), point(px(100.), px(44.)));

        assert!(selects(
            point(px(50.), px(8.)),
            anchor,
            cursor,
            Some(left),
            left
        ));
        for char_at in [point(px(212.), px(8.)), point(px(212.), px(32.))] {
            assert!(!selects(char_at, anchor, cursor, Some(left), right));
        }
    }

    #[test]
    fn a_drag_from_above_the_table_selects_up_to_the_cursor_in_reading_order() {
        let above = point(px(0.), px(-40.));
        let cursor = point(px(100.), px(100.));
        let earlier_row = cell(200., 0., 400., 80.);
        let cursor_cell = cell(0., 80., 200., 80.);
        let right_of_cursor = cell(200., 80., 400., 80.);

        assert!(selects(
            point(px(350.), px(32.)),
            above,
            cursor,
            None,
            earlier_row
        ));
        assert!(selects(
            point(px(50.), px(88.)),
            above,
            cursor,
            None,
            cursor_cell
        ));
        assert!(!selects(
            point(px(212.), px(88.)),
            above,
            cursor,
            None,
            right_of_cursor
        ));
    }
}
