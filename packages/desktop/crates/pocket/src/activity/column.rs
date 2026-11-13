use super::card::col_glyph;
use super::logic::Col;
use crate::board::parts::{empty_hint, heading};
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn activity_column(&mut self, col: Col, cx: &mut Context<Self>) -> Stateful<Div> {
        let count = self.activity.columns[col as usize].len();
        let head = div().h(px(38.)).flex_none().pl(px(12.)).pr(px(8.)).flex().items_center().child(heading(col_glyph(col), col.name(), count.to_string()));
        let empty = (count == 0).then(|| empty_hint(col.hint()));
        let cards = list(self.activity.lists[col as usize].clone(), cx.processor(move |this, ix, _, cx| this.activity_card(col, ix, cx))).flex_1().w_full();
        div().id(("activity-col", col as usize)).flex_1().min_w(px(200.)).max_w(px(340.)).h_full().flex().flex_col().rounded(px(10.)).bg(FILL_1).child(head).children(empty).child(cards)
    }
}
