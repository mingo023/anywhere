use super::card::DragTicket;
use super::logic::{Col, Slot, busy, set_away};
use super::parts::col_heading;
use ui::empty_hint;
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

const COL_RADIUS: f32 = 10.;

impl Desktop {
    pub(super) fn add_ticket_button(&self, id: &'static str, cx: &mut Context<Self>) -> Stateful<Div> {
        ui::icon_button_sized(id, "plus", 22., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.focus_board_composer(window, cx)))
    }

    fn column_head(&self, col: Col, cx: &mut Context<Self>) -> Div {
        let count = match col {
            Col::Working => format!("{}/{}", busy(&self.agents.board.tickets), self.agents.board.limit),
            _ => self.board.columns[col as usize].len().to_string(),
        };
        let hide = ui::icon_button_sized(("board-hide", col as usize), "minus", 22., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            set_away(col, true, &mut this.board.tucked, &mut this.board.show_closed);
            this.board_changed();
            cx.notify();
        }));
        let add = (col == Col::Todo).then(|| self.add_ticket_button("board-todo-add", cx));
        div().h(px(38.)).flex_none().pl(px(12.)).pr(px(8.)).flex().items_center().child(col_heading(col, count)).child(div().flex_1()).child(hide).children(add)
    }

    pub(super) fn board_column(&mut self, col: Col, cx: &mut Context<Self>) -> Stateful<Div> {
        let empty = self.board.columns[col as usize].is_empty().then(|| empty_hint(col.hint()));
        let cards = list(self.board.lists[col as usize].clone(), cx.processor(move |this, ix, _, cx| {
            let index = this.board.columns[col as usize].get(ix).copied();
            index.map_or_else(|| div().into_any_element(), |index| this.ticket_card(Slot { col, pos: ix, index }, cx))
        }))
        .flex_1()
        .w_full();
        div()
            .id(("board-col", col as usize))
            .w(px(288.))
            .h_full()
            .flex()
            .flex_none()
            .flex_col()
            .rounded(px(COL_RADIUS))
            .bg(FILL_1)
            .drag_over::<DragTicket>(move |s, d, _, _| if col.takes(d.from) { s.bg(FILL_3) } else { s })
            .on_drop(cx.listener(move |this, d: &DragTicket, _, cx| this.drop_ticket(&d.id, d.from, col, None, cx)))
            .child(self.column_head(col, cx))
            .children(empty)
            .child(cards)
    }
}
