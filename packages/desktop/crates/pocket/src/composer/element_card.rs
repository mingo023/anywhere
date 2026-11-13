use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use theme::*;

/// An element picked on a page, as tall as a file card beside it: its number and host over what it says.
pub(crate) fn element_card(id: impl Into<ElementId>, number: &str, host: &str, title: &str, tip: Option<web::Element>) -> Stateful<Div> {
    let glyph = div().size(px(34.)).flex_none().flex().items_center().justify_center().rounded(px(8.)).bg(FILL_2).child(icon("globe", 16., TEXT_2));
    let top = div()
        .flex()
        .gap(px(5.))
        .text_size(px(13.))
        .line_height(px(17.))
        .font_weight(FontWeight::MEDIUM)
        .child(div().flex_none().text_color(MENTION).child(number.to_string()))
        .child(div().min_w_0().truncate().text_color(TEXT).child(host.to_string()));
    let words = div().min_w_0().flex().flex_col().gap(px(1.)).child(top).child(div().truncate().text_size(px(11.5)).line_height(px(15.)).text_color(TEXT_3).child(title.to_string()));
    let card = div()
        .id(id)
        .h(px(56.))
        .max_w(px(240.))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(10.))
        .pl(px(10.))
        .pr(px(14.))
        .rounded(px(10.))
        .bg(FILL_1)
        .border_1()
        .border_color(hairline(0.10))
        .child(glyph)
        .child(words);
    match tip {
        Some(e) => card.tooltip(move |window, cx| {
            let e = e.clone();
            Tooltip::element(move |_, _| preview(&e)).build(window, cx)
        }),
        None => card,
    }
}

/// A chip's hover: the element's name, selector and size over the start of its HTML, each on one line.
fn preview(e: &web::Element) -> Div {
    let html: String = e.html.split_whitespace().collect::<Vec<_>>().join(" ").chars().take(200).collect();
    let head = div()
        .flex()
        .items_center()
        .gap(px(6.))
        .child(div().flex_none().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(e.name.clone()))
        .child(div().flex_1().min_w_0().truncate().text_color(TEXT_3).child(e.selector.clone()))
        .child(div().flex_none().pl(px(6.)).text_color(TEXT_3).child(format!("{} × {}", e.width, e.height)));
    div()
        .w(px(360.))
        .py(px(4.))
        .flex()
        .flex_col()
        .gap(px(6.))
        .text_size(px(12.))
        .child(head)
        .child(div().truncate().font_family(MONO).text_size(px(11.5)).text_color(TEXT_2).child(html))
}
