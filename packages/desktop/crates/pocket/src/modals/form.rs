use gpui_kit::component::input::InputState;
use gpui_kit::*;
use theme::*;

pub(crate) fn default_base<'a>(branches: impl Iterator<Item = &'a str> + Clone, preferred: &str, current: &str) -> usize {
    let at = |name: &str| branches.clone().position(|b| b == name);
    at(preferred).or_else(|| at("main")).or_else(|| at("master")).or_else(|| at(current)).unwrap_or(0)
}

pub(crate) fn home() -> String {
    std::env::var("HOME").unwrap_or_default()
}

pub(crate) fn typed_or(input: &Entity<InputState>, fallback: impl FnOnce() -> String, cx: &App) -> String {
    let v = input.read(cx).value().trim().to_string();
    if v.is_empty() { fallback() } else { v }
}

pub fn footer(note: impl IntoElement, cancel: Stateful<Div>, submit: Stateful<Div>) -> Div {
    div().pt(px(4.)).flex().items_center().gap(px(8.)).child(div().flex_1().min_w_0().text_size(px(13.)).text_color(rgba(TEXT_4)).child(note)).child(cancel).child(submit)
}
