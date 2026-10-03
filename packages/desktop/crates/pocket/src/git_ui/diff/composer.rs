use super::{NUM, SIGN};
use crate::desktop::Desktop;
use gpui_kit::component::input::{Escape, Textarea};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

impl Desktop {
    pub(super) fn composer(&self, cx: &mut Context<Self>) -> Div {
        let lines = self.diff.draft().and_then(|v| v.pick.label(&v.lines)).unwrap_or_default();
        let target = self.comment_target();
        let ready = target.is_some() && !self.diff.input.read(cx).value().trim().is_empty();
        let head = div()
            .px(px(16.))
            .pt(px(12.))
            .flex()
            .items_center()
            .justify_between()
            .text_size(px(12.))
            .child(div().font_family(MONO).font_weight(FontWeight::SEMIBOLD).text_color(ACCENT).child(lines))
            .child(div().text_color(TEXT_3).child("esc to dismiss"));
        let field = div().px(px(16.)).py(px(10.)).text_size(px(14.5)).line_height(px(21.75)).child(Textarea::new(&self.diff.input).appearance(false));
        let submit = ui::button("comment-submit", Variant::Accent, None, "Comment")
            .child(ui::button_kbd("⌘↵"))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.submit_comment(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let cancel =
            ui::button("comment-cancel", Variant::Ghost, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.cancel_comment(window, cx)));
        let foot = div()
            .px(px(12.))
            .pt(px(10.))
            .pb(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_t_1()
            .border_color(HAIRLINE)
            .child(self.target_picker(target, cx))
            .child(div().flex_1())
            .child(cancel)
            .child(submit);
        div()
            .mt(px(6.))
            .mb(px(10.))
            .mr(px(20.))
            .ml(px(if self.diff.split { NUM + SIGN } else { 2. * NUM + SIGN }))
            .flex()
            .flex_col()
            .rounded(px(16.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(ACCENT_RING, 1.), ui::shadow(rgba(0x1111131a), 8., 24.), ui::shadow(rgba(0x1111130f), 1., 2.)])
            .font_family(SANS)
            .whitespace_normal()
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.cancel_comment(window, cx)))
            .child(head)
            .child(field)
            .child(foot)
    }
}
