use super::parts::action_button;
use crate::desktop::Desktop;
use gpui_kit::component::input::Input;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn board_bar(&self, cx: &mut Context<Self>) -> Div {
        let search = div().w(px(180.)).h(px(26.)).px(px(8.)).flex().items_center().gap(px(6.)).rounded(px(6.)).border_1().border_color(SEPARATOR).child(icon("search", 12., TEXT_3)).child(div().flex_1().min_w_0().text_size(px(12.5)).child(Input::new(&self.board.search).appearance(false).p_0()));
        let new = action_button("board-new", "New ticket").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.edit_ticket(None, window, cx)));
        let right = div().flex().items_center().gap(px(8.)).child(search).child(new);
        let title = div().flex_1().min_w_0().flex().items_center().gap(px(7.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(icon("blocks", 14., TEXT_3)).child("Board");
        self.detail_bar(title, right, cx)
    }
}
