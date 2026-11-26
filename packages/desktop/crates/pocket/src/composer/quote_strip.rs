use crate::add_to_chat::{Body, Quote};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

pub(crate) fn quote_strip(quote: &Quote, remove: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Div {
    let prose = matches!(quote.body, Body::Prose { .. });
    let first = quote.excerpt().lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_string();
    div()
        .mx(px(10.))
        .mt(px(10.))
        .pl(px(10.))
        .pr(px(6.))
        .py(px(7.))
        .flex()
        .items_center()
        .gap(px(10.))
        .rounded(px(10.))
        .bg(WELL)
        .text_size(px(12.))
        .text_color(TEXT_3)
        .child(div().w(px(2.)).h(px(30.)).flex_none().rounded(px(1.)).bg(TEXT_4))
        .child(icon(quote.icon(), 13., TEXT_3))
        .child(
            div()
                .min_w_0()
                .flex_1()
                .flex()
                .flex_col()
                .child(
                    div()
                        .flex()
                        .gap(px(6.))
                        .child("Quoting")
                        .child(div().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(quote.label())),
                )
                .child(div().mt(px(1.)).truncate().text_color(TEXT_2).when(!prose, |d| d.font_family(code_font()).text_size(px(11.5))).child(first)),
        )
        .child(
            ui::icon_button_sized("composer-quote-remove", "x", 24., TEXT_3)
                .tooltip(|window, cx| Tooltip::new("Remove quote").build(window, cx))
                .on_click(remove),
        )
}
