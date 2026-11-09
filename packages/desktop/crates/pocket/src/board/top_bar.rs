use super::Layout;
use super::logic::shown_cols;
use crate::desktop::Desktop;
use crate::desktop::chrome::pill;
use gpui_kit::component::input::Input;
use gpui_kit::*;
use theme::*;
use ui::Variant;

impl Desktop {
    fn board_bar(&self, cx: &mut Context<Self>) -> Div {
        let search = div().w(px(200.)).h(px(28.)).px(px(8.)).flex().items_center().gap(px(6.)).rounded(px(8.)).bg(FILL_2).child(icon("search", 13., TEXT_3)).child(div().flex_1().min_w_0().text_size(px(13.)).child(Input::new(&self.board.search).appearance(false).p_0()));
        let new = pill("board-new", Variant::Primary, Some("plus"), "New ticket").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.edit_ticket(None, window, cx)));
        let right = div().flex().items_center().gap(px(8.)).child(search).child(new);
        self.detail_bar(div().flex_1().min_w_0().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child("Board"), right, cx)
    }

    pub(crate) fn board_page(&mut self, cx: &mut Context<Self>) -> Div {
        let body = if self.board.layout == Layout::Board {
            let columns: Vec<_> = shown_cols(self.board.show_closed).map(|col| self.board_column(col, cx)).collect();
            div().id("board-columns").flex_1().min_h_0().overflow_x_scroll().px(px(24.)).pt(px(4.)).pb(px(16.)).flex().gap(px(12.)).children(columns).into_any_element()
        } else {
            self.board_list(cx).into_any_element()
        };
        div().relative().flex_1().min_w_0().min_h_0().flex().flex_col().child(self.board_bar(cx)).child(self.filter_bar(cx)).child(body).children(self.card_menu(cx)).children(self.board_toast(cx))
    }
}
