use gpui_kit::*;
use theme::*;

pub(crate) fn added(agent: &str) -> Div {
    ui::pop(div())
        .h(px(32.))
        .px(px(12.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(10.))
        .text_size(px(12.5))
        .text_color(TEXT_2)
        .whitespace_nowrap()
        .child(icon("check", 14., SUCCESS))
        .child(
            div()
                .flex()
                .gap(px(4.))
                .child("Added to")
                .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(format!("{agent}'s")))
                .child("input · ⏎ sends"),
        )
}
