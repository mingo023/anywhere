use super::card::col_glyph;
use super::logic::Col;
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn activity_column(&mut self, col: Col, cx: &mut Context<Self>) -> Stateful<Div> {
        let count = self.activity.columns[col as usize].len();
        let head = div().h(px(38.)).flex_none().pl(px(12.)).pr(px(8.)).flex().items_center().gap(px(8.)).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(col_glyph(col)).child(col.name()).child(div().font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(count.to_string()));
        let empty = (count == 0).then(|| div().mx(px(8.)).min_h(px(72.)).flex().flex_none().items_center().justify_center().px(px(18.)).py(px(12.)).rounded(px(9.)).border_1().border_dashed().border_color(SEPARATOR).text_center().text_size(px(12.)).line_height(px(18.)).text_color(TEXT_4).child(col.hint()));
        let cards = list(self.activity.lists[col as usize].clone(), cx.processor(move |this, ix, _, cx| this.activity_card(col, ix, cx))).flex_1().w_full();
        div().id(("activity-col", col as usize)).flex_1().min_w(px(200.)).max_w(px(340.)).h_full().flex().flex_col().rounded(px(10.)).bg(FILL_1).child(head).children(empty).child(cards)
    }
}
