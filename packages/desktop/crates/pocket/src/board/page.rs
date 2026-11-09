use super::Layout;
use super::logic::shown_cols;
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(crate) fn board_page(&mut self, cx: &mut Context<Self>) -> Div {
        let body = if self.board.layout == Layout::Board {
            let columns: Vec<_> = shown_cols(self.board.tab, self.board.show_closed).map(|col| self.board_column(col, cx)).collect();
            div().id("board-columns").flex_1().min_h_0().overflow_x_scroll().px(px(16.)).pb(px(16.)).flex().gap(px(8.)).children(columns).children(self.hidden_columns(cx)).into_any_element()
        } else {
            self.board_list(cx).into_any_element()
        };
        div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().bg(CANVAS).child(self.board_bar(cx)).child(self.board_tabs(cx)).children(self.active_filters(cx)).child(body).children(self.filter_notice(cx)).children(self.card_menu(cx)).children(self.board_toast(cx))
    }
}
