use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(crate) fn added_toast(&self) -> Option<impl IntoElement> {
        let (provider, _) = self.chat.added.as_ref()?;
        let toast = ui::pop(div())
            .absolute()
            .right(px(16.))
            .bottom(px(16.))
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
                    .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(format!("{}'s", provider_name(provider))))
                    .child("input · ⏎ sends"),
            );
        Some(toast.with_animation("added-toast-in", Animation::new(MENU_IN).with_easing(ease_out_quint()), |d, t| d.opacity(t)))
    }
}
