use super::{NUM, SIGN};
use crate::desktop::Desktop;
use crate::util::{ago_long, now_ms};
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn comment_card(&self, i: usize, cx: &mut Context<Self>) -> Div {
        let Some(c) = self.diff.comments.get(i) else { return div() };
        let head = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .child(div().font_family(MONO).font_weight(FontWeight::SEMIBOLD).text_color(WAITING_TEXT).child(c.label.clone()))
            .child(div().text_color(TEXT_4).child(format!("· {}", ago_long(c.at, now_ms()))))
            .child(div().flex_1())
            .child(div().flex().items_center().gap(px(4.)).font_weight(FontWeight::SEMIBOLD).text_color(SUCCESS_TEXT).child(icon("check", 12., SUCCESS_TEXT)).child("Sent"));
        let resolve = div()
            .id(("resolve", i))
            .h(px(24.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(5.))
            .rounded(px(12.))
            .bg(FILL_3)
            .hover(|s| s.bg(FILL_4))
            .cursor_pointer()
            .text_size(px(12.5))
            .font_weight(FontWeight::MEDIUM)
            .child(icon("check", 12., TEXT_2))
            .child("Resolve")
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.resolve_comment(i, cx)));
        div()
            .mt(px(6.))
            .mb(px(10.))
            .mr(px(20.))
            .ml(px(if self.diff.split { NUM + SIGN } else { 2. * NUM + SIGN }))
            .px(px(16.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(12.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(SEPARATOR, 0.5), ui::shadow(rgba(0x1111130f), 1., 3.)])
            .font_family(SANS)
            .whitespace_normal()
            .child(head)
            .child(div().text_size(px(14.5)).line_height(px(21.)).text_color(TEXT).child(c.text.clone()))
            .child(div().flex().child(resolve))
    }
}
