use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

/// The Team chip: the next send asks the agent for a team plan. `remove` is its ×.
pub(crate) fn team_chip(id: impl Into<ElementId>, remove: impl Fn(&mut Desktop, &mut Window, &mut Context<Desktop>) + 'static, cx: &mut Context<Desktop>) -> Div {
    let ink = Hsla::from(COMMAND);
    let x = div()
        .id(id)
        .size(px(18.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(9.))
        .cursor_pointer()
        .hover(move |s| s.bg(ink.opacity(0.15)))
        .child(icon("x", 10., ink.opacity(0.8)))
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            remove(this, window, cx);
            cx.notify();
        }));
    div()
        .h(px(26.))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(5.))
        .pl(px(8.))
        .pr(px(4.))
        .rounded(px(8.))
        .bg(ink.opacity(0.1))
        .border_1()
        .border_color(ink.opacity(0.25))
        .text_size(px(12.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(ink)
        .child(icon("team", 13., ink))
        .child("Team")
        .child(x)
}
