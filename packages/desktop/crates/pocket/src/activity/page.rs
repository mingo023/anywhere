use super::logic::COLS;
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(crate) fn activity_page(&mut self, cx: &mut Context<Self>) -> Div {
        let columns: Vec<_> = COLS.into_iter().map(|col| self.activity_column(col, cx)).collect();
        let body = div().id("activity-columns").flex_1().min_h_0().overflow_x_scroll().px(px(12.)).pb(px(8.)).flex().gap(px(8.)).children(columns);
        div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().bg(CANVAS).child(self.activity_bar(cx)).child(body).children(self.activity_panel(cx))
    }

    fn activity_bar(&self, cx: &mut Context<Self>) -> Div {
        let title = div().ml(px(2.)).flex_1().min_w_0().flex().items_center().gap(px(7.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(icon("activity", 14., TEXT_3)).child("Activity");
        let filters = self.activity_filters(cx);
        self.detail_bar(title, filters, cx).h(px(44.))
    }
}
