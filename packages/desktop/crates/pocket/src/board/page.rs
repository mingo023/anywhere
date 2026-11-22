use super::Board;
use super::Layout;
use super::logic::{Body, body};
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Render for Board {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let page = div().relative().flex_1().min_w_0().min_h_0().flex().flex_col();
        if self.page.is_some() {
            return page.child(self.ticket_page(cx));
        }
        let shown = body(self.layout == Layout::List, self.tickets.tickets.len(), self.rows.len(), self.hidden);
        let main = match shown {
            Body::Hero => self.board_hero(cx).into_any_element(),
            Body::Columns => {
                let columns: Vec<_> = self.shown.clone().into_iter().map(|col| self.board_column(col, cx)).collect();
                div()
                    .id("board-columns")
                    .flex_1()
                    .min_h_0()
                    .overflow_x_scroll()
                    .track_scroll(&self.columns_scroll)
                    .px(px(12.))
                    .pb(px(8.))
                    .flex()
                    .gap(px(8.))
                    .children(columns)
                    .children(self.hidden_columns(cx))
                    .into_any_element()
            }
            Body::List => self.board_list(cx).into_any_element(),
            Body::Filtered => self.filtered_out(cx).into_any_element(),
        };
        let dock = (shown != Body::Hero).then(|| self.board_dock(cx));
        let notice = (shown != Body::Filtered).then(|| self.filter_notice(cx)).flatten();
        let area =
            div().relative().flex_1().min_h_0().flex().flex_col().child(main).children(notice).children(self.board_toast(cx)).children(self.bulk_bar(cx));
        page.child(self.board_tabs(cx)).children(self.active_filters(cx)).child(area).children(dock).children(self.card_menu(cx))
    }
}

impl Board {
    /// The bar's crumb, and what sits at its right: the search, or the ticket page's steps.
    pub(crate) fn bar(&self, cx: &mut Context<Self>) -> (Div, Div) {
        if self.page.is_some() { self.ticket_page_bar(cx) } else { self.board_bar(cx) }
    }
}

impl Desktop {
    /// The page: its bar over the Board or the ticket's page.
    pub(crate) fn board_page(&mut self, cx: &mut Context<Self>) -> Div {
        let side = self.board_side(cx);
        let (crumb, right) = self.board.update(cx, |b, cx| {
            b.set(side);
            b.bar(cx)
        });
        let bar = self.detail_bar(crumb, right, cx).h(px(44.));
        div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().bg(CANVAS).child(bar).child(self.board.clone())
    }
}
