use super::{Body, Quote};
use crate::desktop::Desktop;
use gpui_kit::component::input::{Escape, Textarea};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

impl Desktop {
    pub(crate) fn chat_card(&self, quote: &Quote, quoted: bool, cx: &mut Context<Self>) -> Div {
        let choices = self.chat_choices();
        let target = self.chat_target(&choices);
        let chosen = target.as_deref().and_then(|id| choices.iter().find(|c| c.id == id));
        let head = div()
            .px(px(16.))
            .pt(px(12.))
            .flex()
            .items_center()
            .gap(px(12.))
            .text_size(px(12.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(6.))
                    .child(icon(quote.icon(), 13., ACCENT))
                    .child(div().min_w_0().truncate().font_family(MONO).font_weight(FontWeight::SEMIBOLD).text_color(ACCENT).child(quote.label())),
            )
            .child(div().flex_none().text_color(TEXT_3).child("esc to dismiss"));
        let prose = matches!(quote.body, Body::Prose { .. });
        let excerpt = quoted.then(|| {
            div()
                .mx(px(16.))
                .mt(px(10.))
                .px(px(10.))
                .py(px(6.))
                .max_h(px(61.))
                .overflow_hidden()
                .rounded(px(8.))
                .bg(PAGE)
                .border_1()
                .border_color(SEPARATOR)
                .text_color(TEXT_2)
                .map(|d| if prose { d.text_size(px(12.5)).line_height(px(18.)) } else { d.font_family(MONO).text_size(px(11.5)).line_height(px(16.)).whitespace_nowrap() })
                .child(quote.excerpt())
        });
        let field = div().px(px(16.)).py(px(10.)).text_size(px(14.5)).line_height(px(21.75)).child(Textarea::new(&self.chat.input).appearance(false));
        let ready = chosen.is_some();
        let add = ui::button("chat-add", Variant::Accent, None, "Add to input")
            .child(ui::button_kbd("⌘↵"))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.add_to_input(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let cancel = ui::button("chat-cancel", Variant::Ghost, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.cancel_chat(window, cx)));
        let foot = div()
            .px(px(12.))
            .pt(px(10.))
            .pb(px(12.))
            .flex()
            .flex_wrap()
            .items_center()
            .gap(px(8.))
            .border_t_1()
            .border_color(HAIRLINE)
            .child(self.chat_target_picker(chosen, &choices, cx))
            .child(div().ml_auto().flex().items_center().gap(px(8.)).child(cancel).child(add));
        div()
            .flex()
            .flex_col()
            .rounded(px(16.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(ACCENT_RING, 1.), ui::shadow(rgba(0x1111131a), 8., 24.), ui::shadow(rgba(0x1111130f), 1., 2.)])
            .font_family(SANS)
            .whitespace_normal()
            .on_action(cx.listener(|this, _: &Escape, window, cx| {
                if this.chat.menu {
                    this.chat.menu = false;
                    cx.notify();
                } else {
                    this.cancel_chat(window, cx);
                }
            }))
            .child(head)
            .children(excerpt)
            .child(field)
            .child(foot)
    }
}
