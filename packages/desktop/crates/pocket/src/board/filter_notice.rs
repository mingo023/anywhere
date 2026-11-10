use super::parts::bar_button;
use crate::desktop::Desktop;
use crate::team::plural;
use gpui_kit::*;
use theme::*;
use ui::Variant;

impl Desktop {
    pub(super) fn filter_notice(&self, cx: &mut Context<Self>) -> Option<Div> {
        let n = self.board.hidden;
        (n > 0).then(|| {
            let clear = div()
                .id("board-clear-filters")
                .flex()
                .items_center()
                .gap(px(4.))
                .cursor_pointer()
                .font_weight(FontWeight::MEDIUM)
                .text_color(TEXT)
                .child("Clear filters")
                .child(icon("x", 11., TEXT_3))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.clear_board_filters(window, cx)));
            let bar = ui::pop(div()).h(px(32.)).px(px(12.)).flex().items_center().gap(px(12.)).rounded(px(8.)).text_size(px(12.5)).text_color(TEXT_2).child(format!("{} hidden by filters", plural(n, "ticket", "tickets"))).child(clear);
            div().absolute().bottom(px(8.)).left_0().right_0().flex().justify_center().child(bar)
        })
    }

    /// The list when the filters and the search took everything.
    pub(super) fn filtered_out(&self, cx: &mut Context<Self>) -> Div {
        let clear = bar_button("board-filtered-clear", Variant::Ghost, None, "Clear filters").child(icon("x", 11., TEXT_3)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.clear_board_filters(window, cx)));
        let drawing = svg().path("icons/stacked-tickets.svg").w(px(88.)).h(px(72.)).text_color(TEXT_4);
        let heading = div().mt(px(18.)).text_size(px(15.)).line_height(px(20.)).font_weight(FontWeight::SEMIBOLD).child("No tickets match these filters");
        let count = div().child(format!("{} hidden by filters", plural(self.board.hidden, "ticket", "tickets")));
        let hidden = div().mt(px(16.)).h(px(40.)).pl(px(14.)).pr(px(6.)).flex().items_center().gap(px(20.)).rounded(px(9.)).border_1().border_color(SEPARATOR).text_size(px(12.5)).text_color(TEXT_3).child(count).child(clear);
        div().flex_1().min_h_0().flex().flex_col().items_center().justify_center().child(drawing).child(heading).child(hidden)
    }
}
