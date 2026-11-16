use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

/// An agent asks for a host this Mac doesn't link yet: why, and a button that opens the add form with the host it named.
pub(super) fn card(id: &str, hint: &str, reason: &str, cx: &mut Context<Desktop>) -> Div {
    let tile = div().size(px(28.)).flex().flex_none().items_center().justify_center().rounded(px(7.)).bg(FILL_2).child(icon("server", 14., TEXT_2));
    let title = match hint {
        "" => "Add a host".to_string(),
        name => format!("Add {name} as a host"),
    };
    let text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(1.))
        .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(title))
        .children((!reason.is_empty()).then(|| div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(reason.to_string())));
    let hint = hint.to_string();
    let add = ui::button(SharedString::from(format!("host-link-{id}")), ui::Variant::Secondary, None, "Add host\u{2026}")
        .h(px(26.))
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.link_host(hint.clone(), window, cx)));
    div().w_full().px(px(12.)).py(px(10.)).flex().items_center().gap(px(12.)).rounded(px(10.)).border_1().border_color(SEPARATOR).child(tile).child(text).child(add)
}
