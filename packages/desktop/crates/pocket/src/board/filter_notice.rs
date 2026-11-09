use crate::desktop::Desktop;
use crate::team::plural;
use gpui_kit::*;
use theme::*;

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
                .text_color(TEXT)
                .font_weight(FontWeight::MEDIUM)
                .child("Clear filters")
                .child(icon("x", 11., TEXT_3))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.clear_board_filters(window, cx)));
            let bar = ui::pop(div()).h(px(32.)).px(px(12.)).flex().items_center().gap(px(12.)).rounded(px(8.)).text_size(px(12.5)).text_color(TEXT_2).child(format!("{} hidden by filters", plural(n, "ticket", "tickets"))).child(clear);
            div().absolute().bottom(px(14.)).left_0().right_0().flex().justify_center().child(bar)
        })
    }
}
