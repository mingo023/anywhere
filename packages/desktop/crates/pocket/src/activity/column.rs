use super::card::col_glyph;
use super::logic::Col;
use ui::column_heading;
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn activity_column(&mut self, col: Col, cx: &mut Context<Self>) -> Stateful<Div> {
        let count = self.activity.grouped.columns[col as usize].len();
        let head = div().h(px(36.)).flex_none().pl(px(12.)).pr(px(8.)).flex().items_center().child(column_heading(col_glyph(col, count > 0), col.name(), count.to_string())).children(col.why().map(|why| div().ml_auto().text_size(px(11.)).text_color(TEXT_4).child(why)));
        let empty = (count == 0).then(|| div().px(px(14.)).pt(px(6.)).pb(px(12.)).text_size(px(12.)).line_height(px(18.)).text_color(TEXT_4).child(col.hint()));
        let cards = list(self.activity.lists[col as usize].clone(), cx.processor(move |this, ix, _, cx| this.activity_card(col, ix, cx))).flex_1().w_full();
        div().id(("activity-col", col as usize)).flex_1().min_w(px(208.)).max_w(px(320.)).h_full().flex().flex_col().rounded(px(10.)).bg(FILL_1).child(head).children(empty).child(cards)
    }
}
