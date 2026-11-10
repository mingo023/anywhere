use super::logic::{Col, offers_hidden_columns};
use super::parts::{CARD_RADIUS, col_heading};
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn hidden_columns(&self, cx: &mut Context<Self>) -> Option<Div> {
        offers_hidden_columns(self.board.tab, self.board.show_closed).then(|| {
            let closed = self.board.columns[Col::Closed as usize].len();
            let row = div().id("board-hidden-closed").h(px(34.)).px(px(10.)).flex().items_center().rounded(px(CARD_RADIUS)).bg(SURFACE).shadow(ui::row_shadow()).cursor_pointer().child(col_heading(Col::Closed, closed.to_string())).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.board.show_closed = true;
                this.board_changed();
                cx.notify();
            }));
            div().w(px(200.)).flex().flex_none().flex_col().gap(px(6.)).child(div().h(px(36.)).px(px(4.)).flex().items_center().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(TEXT_3).child("Hidden columns")).child(row)
        })
    }
}
