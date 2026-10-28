use gpui::{
    AnyElement, App, Bounds, Element, ElementId, GlobalElementId, InspectorElementId,
    IntoElement, LayoutId, Pixels, Point, Window, point,
};

use crate::GlobalState;

/// A table cell that the selection reads on its own: while it paints, the
/// view's selection endpoints are pinned to it (see [`clamp_to_cell`]), so a
/// drag inside one cell doesn't select the cells beside it.
pub(super) struct SelectionCell {
    child: AnyElement,
}

pub(super) fn selection_cell(child: impl IntoElement) -> SelectionCell {
    SelectionCell {
        child: child.into_any_element(),
    }
}

/// Moves a selection endpoint into `cell` in reading order: one in an earlier
/// row, or left of the cell in its row, goes to the cell's start; one in a
/// later row, or right of it, to its end. Cells stretch to their row's height,
/// so the cell's top and bottom are the row's.
pub(super) fn clamp_to_cell(p: Point<Pixels>, cell: Bounds<Pixels>) -> Point<Pixels> {
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

impl IntoElement for SelectionCell {
    type Element = Self;

    fn into_element(self) -> Self {
        self
    }
}

impl Element for SelectionCell {
    type RequestLayoutState = ();
    type PrepaintState = ();

    fn id(&self) -> Option<ElementId> {
        None
    }

    fn source_location(&self) -> Option<&'static std::panic::Location<'static>> {
        None
    }

    fn request_layout(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        window: &mut Window,
        cx: &mut App,
    ) -> (LayoutId, ()) {
        (self.child.request_layout(window, cx), ())
    }

    fn prepaint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        _: Bounds<Pixels>,
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        self.child.prepaint(window, cx);
    }

    fn paint(
        &mut self,
        _: Option<&GlobalElementId>,
        _: Option<&InspectorElementId>,
        bounds: Bounds<Pixels>,
        _: &mut (),
        _: &mut (),
        window: &mut Window,
        cx: &mut App,
    ) {
        let Some(view) = GlobalState::global(cx).text_view_state().cloned() else {
            self.child.paint(window, cx);
            return;
        };
        let outer = view.update(cx, |state, _| state.selection_cell.replace(bounds));
        self.child.paint(window, cx);
        view.update(cx, |state, _| state.selection_cell = outer);
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
        cell: Bounds<Pixels>,
    ) -> bool {
        point_in_text_selection(
            char_at,
            px(10.),
            clamp_to_cell(anchor, cell),
            clamp_to_cell(cursor, cell),
            px(24.),
        )
    }

    #[test]
    fn a_drag_inside_one_cell_leaves_the_cell_beside_it_unselected() {
        let left = cell(0., 0., 200., 80.);
        let right = cell(200., 0., 400., 80.);
        let (anchor, cursor) = (point(px(12.), px(20.)), point(px(100.), px(44.)));

        assert!(selects(point(px(50.), px(8.)), anchor, cursor, left));
        for char_at in [point(px(212.), px(8.)), point(px(212.), px(32.))] {
            assert!(!selects(char_at, anchor, cursor, right));
        }
    }

    #[test]
    fn a_drag_from_one_cell_into_the_next_selects_in_reading_order() {
        let left = cell(0., 0., 200., 80.);
        let right = cell(200., 0., 400., 80.);
        let (anchor, cursor) = (point(px(100.), px(20.)), point(px(300.), px(20.)));

        // The left cell from the anchor to its end, the right from its start to the cursor.
        assert!(!selects(point(px(12.), px(8.)), anchor, cursor, left));
        assert!(selects(point(px(12.), px(32.)), anchor, cursor, left));
        assert!(selects(point(px(212.), px(8.)), anchor, cursor, right));
        assert!(!selects(point(px(350.), px(8.)), anchor, cursor, right));
        assert!(!selects(point(px(212.), px(32.)), anchor, cursor, right));
    }

    #[test]
    fn a_drag_from_above_the_table_selects_up_to_the_cursor_in_reading_order() {
        let above = point(px(0.), px(-40.));
        let cursor = point(px(100.), px(100.));
        let earlier_row = cell(200., 0., 400., 80.);
        let cursor_cell = cell(0., 80., 200., 80.);
        let right_of_cursor = cell(200., 80., 400., 80.);

        assert!(selects(point(px(350.), px(32.)), above, cursor, earlier_row));
        assert!(selects(point(px(50.), px(88.)), above, cursor, cursor_cell));
        assert!(!selects(point(px(212.), px(88.)), above, cursor, right_of_cursor));
    }
}
