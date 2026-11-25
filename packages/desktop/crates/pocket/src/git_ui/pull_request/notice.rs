use super::{PrEvent, PrSide};
use gpui_kit::*;
use theme::*;

impl PrSide {
    pub(crate) fn toast(&self, cx: &mut Context<Self>) -> Option<Div> {
        let notice = self.notice.as_ref()?;
        let undo = notice.undo.is_some().then(|| ui::link("pr-undo", "Undo").on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PrEvent::Undo))));
        Some(
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
                .child(notice.text.clone())
                .children(undo),
        )
    }
}
