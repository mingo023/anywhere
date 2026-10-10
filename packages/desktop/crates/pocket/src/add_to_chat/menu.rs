use super::Choice;
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn chat_target_picker(&self, chosen: Option<&Choice>, choices: &[Choice], cx: &mut Context<Self>) -> Div {
        let pill = div()
            .id("chat-target")
            .h(px(32.))
            .px(px(12.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(7.))
            .rounded(px(16.))
            .bg(FILL_3)
            .cursor_pointer()
            .hover(|s| s.bg(FILL_4))
            .text_size(px(13.))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.chat.menu = !this.chat.menu;
                cx.notify();
            }));
        let pill = match chosen {
            Some(c) => pill
                .child(provider_icon(&c.provider, 13., TEXT))
                .child(div().font_weight(FontWeight::SEMIBOLD).child(self.agent_name(&c.provider)))
                .child(div().text_color(TEXT_6).child("·"))
                .child(div().max_w(px(160.)).truncate().text_color(TEXT_2).child(c.title.clone())),
            None => pill.text_color(TEXT_2).child("No agent"),
        };
        let menu = self.chat.menu.then(|| {
            let picked = chosen.map(|c| c.id.clone());
            let items = choices.iter().enumerate().map(|(i, c)| {
                let id = c.id.clone();
                let sub = if c.note.is_empty() { self.agent_name(&c.provider).to_string() } else { format!("{} · {}", self.agent_name(&c.provider), c.note) };
                div()
                    .id(("chat-choice", i))
                    .px(px(10.))
                    .py(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(10.))
                    .rounded(px(10.))
                    .text_size(px(13.))
                    .child(provider_icon(&c.provider, 15., TEXT))
                    .child(
                        div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .flex_col()
                            .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(c.title.clone()))
                            .child(div().truncate().text_size(px(11.5)).text_color(TEXT_3).child(sub)),
                    )
                    .child(div().size(px(14.)).when(picked.as_ref() == Some(&c.id), |d| d.child(icon("check", 14., TEXT))))
                    .when(!c.ready, |d| d.opacity(0.5))
                    .when(c.ready, |d| {
                        d.cursor_pointer().hover(|s| s.bg(FILL_3)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.chat.target = Some(id.clone());
                            this.chat.menu = false;
                            cx.notify();
                        }))
                    })
            });
            let new = div()
                .id("chat-new-session")
                .h(px(34.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(10.))
                .cursor_pointer()
                .hover(|s| s.bg(FILL_3))
                .text_size(px(13.))
                .child(icon("plus", 15., TEXT_2))
                .child("New session…")
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.chat_in_new_session(window, cx)));
            deferred(
                anchored().anchor(Anchor::BottomLeft).offset(point(px(0.), px(-6.))).snap_to_window_with_margin(px(8.)).child(ui::menu_in(
                    "chat-menu-in",
                    ui::pop(div().id("chat-menu"))
                        .w(px(340.))
                        .p(px(6.))
                        .flex()
                        .flex_col()
                        .child(div().px(px(10.)).pt(px(4.)).pb(px(6.)).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child("Add to"))
                        .children(items)
                        .child(div().my(px(4.)).h(px(0.5)).bg(SEPARATOR))
                        .child(new)
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.chat.menu = false;
                            cx.notify();
                        })),
                )),
            )
            .with_priority(1)
        });
        div().relative().child(pill.child(icon("chevron-down", 12., TEXT_3))).children(menu)
    }
}
