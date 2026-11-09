use super::card::DragTicket;
use super::logic::{Col, Slot, busy};
use super::parts::status_glyph;
use crate::desktop::Desktop;
use agents::tickets::Status;
use gpui_kit::*;
use theme::*;
use ui::{self, Variant};

impl Desktop {
    fn column_head(&self, col: Col, cx: &mut Context<Self>) -> Div {
        let count = match col {
            Col::Working => format!("{}/{}", busy(&self.agents.board.tickets), self.agents.board.limit),
            _ => self.board.columns[col as usize].len().to_string(),
        };
        let add = (col == Col::Todo).then(|| ui::icon_button_sized("board-todo-add", "plus", 20., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.edit_ticket(None, window, cx))));
        div()
            .h(px(36.))
            .flex_none()
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(13.))
            .child(div().size(px(16.)).flex().flex_none().items_center().justify_center().child(status_glyph(("board-col-glyph", col as usize), col, Status::None)))
            .child(div().font_weight(FontWeight::MEDIUM).child(col.name()))
            .child(div().text_color(TEXT_4).child(count))
            .child(div().flex_1())
            .children(add)
    }

    fn empty_hint(&self, cx: &mut Context<Self>) -> Div {
        let new = ui::button("board-empty-new", Variant::Ghost, Some("plus"), "New ticket").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.edit_ticket(None, window, cx)));
        div()
            .px(px(12.))
            .py(px(16.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .text_center()
            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child("Nothing to do yet"))
            .child(div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child("Write what you want done, then start it when you're ready."))
            .child(new)
    }

    pub(super) fn board_column(&mut self, col: Col, cx: &mut Context<Self>) -> Stateful<Div> {
        let cards = list(self.board.lists[col as usize].clone(), cx.processor(move |this, ix, _, cx| {
            let index = this.board.columns[col as usize].get(ix).copied();
            index.map_or_else(|| div().into_any_element(), |index| this.ticket_card(Slot { col, pos: ix, index }, cx))
        }))
        .flex_1()
        .w_full();
        let empty = (col == Col::Todo && self.agents.board.tickets.is_empty()).then(|| self.empty_hint(cx));
        div()
            .id(("board-col", col as usize))
            .w(px(280.))
            .h_full()
            .flex()
            .flex_none()
            .flex_col()
            .rounded(px(10.))
            .bg(FILL_1)
            .drag_over::<DragTicket>(move |s, d, _, _| if col.takes(d.from) { s.bg(FILL_3) } else { s })
            .on_drop(cx.listener(move |this, d: &DragTicket, _, cx| this.drop_ticket(&d.id, d.from, col, None, cx)))
            .child(self.column_head(col, cx))
            .children(empty)
            .child(cards)
    }
}
