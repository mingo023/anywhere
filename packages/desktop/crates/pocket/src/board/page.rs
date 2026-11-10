use super::Layout;
use super::logic::{Body, body};
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(crate) fn board_page(&mut self, cx: &mut Context<Self>) -> Div {
        if self.board.page.is_some() {
            return self.ticket_page(cx);
        }
        let shown = body(self.board.layout == Layout::List, self.agents.board.tickets.len(), self.board.rows.len(), self.board.hidden);
        let main = match shown {
            Body::Hero => self.board_hero(cx).into_any_element(),
            Body::Columns => {
                let columns: Vec<_> = self.board.shown.clone().into_iter().map(|col| self.board_column(col, cx)).collect();
                div().id("board-columns").flex_1().min_h_0().overflow_x_scroll().track_scroll(&self.board.columns_scroll).px(px(12.)).pb(px(8.)).flex().gap(px(8.)).children(columns).children(self.hidden_columns(cx)).into_any_element()
            }
            Body::List => self.board_list(cx).into_any_element(),
            Body::Filtered => self.filtered_out(cx).into_any_element(),
        };
        let dock = (shown != Body::Hero).then(|| self.board_dock(cx));
        let notice = (shown != Body::Filtered).then(|| self.filter_notice(cx)).flatten();
        let area = div().relative().flex_1().min_h_0().flex().flex_col().child(main).children(notice).children(self.board_toast(cx)).children(self.bulk_bar(cx));
        div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().bg(CANVAS).child(self.board_bar(cx)).child(self.board_tabs(cx)).children(self.active_filters(cx)).child(area).children(dock).children(self.card_menu(cx))
    }
}
