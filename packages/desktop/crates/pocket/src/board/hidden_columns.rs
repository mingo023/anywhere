use super::card::DragTicket;
use super::logic::{Col, set_away};
use super::parts::status_glyph;
use agents::tickets::Status;
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    fn hidden_row(&self, col: Col, cx: &mut Context<Self>) -> Stateful<Div> {
        let n = self.board.columns[col as usize].len();
        let unfolds = col == Col::Closed || self.board.tucked[col as usize];
        div()
            .id(("board-hidden", col as usize))
            .h(px(30.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(7.))
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .when(unfolds, |d| {
                d.cursor_pointer().hover(|s| s.bg(FILL_2)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    set_away(col, false, &mut this.board.tucked, &mut this.board.show_closed);
                    this.board_changed();
                    cx.notify();
                }))
            })
            .drag_over::<DragTicket>(move |s, d, _, _| if col.takes(d.from) { s.bg(FILL_3) } else { s })
            .on_drop(cx.listener(move |this, d: &DragTicket, _, cx| this.drop_ticket(&d.id, d.from, col, None, cx)))
            .child(status_glyph(col, Status::None))
            .child(div().flex_1().min_w_0().truncate().child(col.name()))
            .child(div().text_size(px(11.5)).text_color(TEXT_4).child(n.to_string()))
    }

    pub(super) fn hidden_columns(&self, cx: &mut Context<Self>) -> Option<Div> {
        let away = &self.board.away;
        (!away.is_empty()).then(|| {
            let rows: Vec<_> = away.iter().map(|c| self.hidden_row(*c, cx)).collect();
            div().w(px(200.)).flex().flex_none().flex_col().px(px(4.)).pt(px(2.)).child(div().h(px(30.)).px(px(8.)).flex().items_center().text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(TEXT_3).child("Hidden columns")).children(rows)
        })
    }
}
