use super::HostFinder;
use gpui_kit::*;
use theme::*;

impl HostFinder {
    pub(super) fn finder_note(&self, glyph: &str, title: String, detail: impl Into<SharedString>) -> Div {
        div()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .child(icon(glyph, 22., TEXT_4))
            .child(div().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).child(title))
            .child(div().text_size(px(12.5)).text_color(TEXT_3).child(detail.into()))
    }
}
