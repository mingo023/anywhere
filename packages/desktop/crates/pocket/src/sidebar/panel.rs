use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, HEADER, SEAM, Side, drag_area};
use crate::sidebar::column::{changes_dot, has_changes};
use crate::sidebar::rail::rail_button;
use crate::status::Status;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use workspace::tree::Axis;

const MIN: f32 = 280.;
const MAX: f32 = 600.;
/// The design's width, until the user drags the edge.
const OPEN: f32 = 334.;
const RAIL_WIDTH: f32 = 44.;

/// The payload of a drag on the panel's edge; its own type, so the left column's resize doesn't follow it.
struct PanelEdge;

/// `clicked`'s rail icon, with the panel `open` on `side`: the side to show, and whether the panel stays open.
pub(crate) fn rail_pick(side: Side, open: bool, clicked: Side) -> (Side, bool) {
    (clicked, !(open && side == clicked))
}

/// The panel's width with its edge dragged `wide` from the rail, or `None` once that's narrow enough to collapse it.
fn edge_width(wide: f32) -> Option<f32> {
    (wide >= MIN * 0.55).then(|| wide.clamp(MIN, MAX))
}

impl Desktop {
    fn pick_side(&mut self, side: Side, cx: &mut Context<Self>) {
        (self.side, self.sidebar.panel_open) = rail_pick(self.side, self.sidebar.panel_open, side);
        self.refresh_graph(cx);
        cx.notify();
    }

    fn drag_panel_edge(&mut self, wide: f32, cx: &mut Context<Self>) {
        let width = edge_width(wide);
        let col = Column::Sessions as usize;
        // A drag move comes every frame, moved or not; notifying on each would redraw forever.
        if self.sidebar.panel_open == width.is_some() && width.is_none_or(|w| self.widths[col] == Some(w)) {
            return;
        }
        self.sidebar.panel_open = width.is_some();
        if width.is_some() {
            self.widths[col] = width;
            self.save_soon(cx);
        }
        cx.notify();
    }

    /// The right-hand panel, empty while collapsed; a drag that collapses it can still bring it back.
    pub(crate) fn workspace_panel(&mut self, cx: &mut Context<Self>) -> Div {
        let open = self.sidebar.panel_open;
        let reset = |this: &mut Self, cx: &mut Context<Self>| this.reset_width(Column::Sessions, cx);
        let seam = self.seam("panel-seam", Axis::Row, reset, cx).filter(|_| open).map(|handle| {
            handle.left(px(-SEAM / 2.)).on_drag(PanelEdge, |_, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| EmptyView)
            })
        });
        let width = self.width(Column::Sessions, OPEN);
        let body = open.then(|| self.workspace_column(cx).w(px(width)).border_l(px(0.5)).border_color(SEPARATOR));
        div()
            .relative()
            .flex_none()
            .h_full()
            .children(body)
            // Deferred so the handle paints over the page it straddles.
            .children(seam.map(deferred))
            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<PanelEdge>, _, cx| {
                this.drag_panel_edge(f32::from(e.bounds.right() - e.event.position.x), cx);
            }))
    }

    /// The rail at the window's right edge: an icon per side, each opening the panel on it or closing it when shown.
    pub(crate) fn workspace_rail(&self, cx: &mut Context<Self>) -> Div {
        let open = self.sidebar.panel_open;
        let waiting = self.sessions_matching("").iter().any(|c| c.status == Status::NeedsYou);
        let dirty = has_changes(self.repo());
        let sides = [(Side::Sessions, "comment"), (Side::Explorer, "file"), (Side::Changes, "branch")].into_iter().enumerate().map(|(i, (side, name))| {
            rail_button(("panel-side", i), name, open && self.side == side)
                .when(side == Side::Sessions && waiting, |d| d.child(ui::dot(6., WAITING_DOT).absolute().top(px(5.)).right(px(5.))))
                .when(side == Side::Changes && dirty, |d| d.child(changes_dot()))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_side(side, cx)))
        });
        drag_area(div().bg(SIDE))
            .w(px(RAIL_WIDTH))
            .flex_none()
            .h_full()
            .pt(px((HEADER - 32.) / 2.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .border_l(px(0.5))
            .border_color(SEPARATOR)
            .children(sides)
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX, MIN, edge_width, rail_pick};
    use crate::desktop::chrome::Side;

    #[test]
    fn the_dragged_edge_holds_between_the_narrowest_and_widest_and_collapses_the_panel_well_short_of_the_narrowest() {
        assert_eq!([100., 160., 384., 900.].map(edge_width), [None, Some(MIN), Some(384.), Some(MAX)]);
    }

    #[test]
    fn a_rail_icon_opens_its_side_and_the_shown_sides_icon_closes_the_panel() {
        let got = [rail_pick(Side::Sessions, true, Side::Explorer), rail_pick(Side::Explorer, true, Side::Explorer), rail_pick(Side::Explorer, false, Side::Explorer)];
        assert_eq!(got, [(Side::Explorer, true), (Side::Explorer, false), (Side::Explorer, true)]);
    }
}
