use crate::desktop::Desktop;
use daemon::Account;
use gpui_kit::*;
use theme::*;

/// Stands in for every email alike, so its length gives nothing away.
const MASK: &str = "\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}\u{2022}";

impl Desktop {
    pub(super) fn chat_account_email(&self, a: &Account, size: f32, ink: impl Into<Hsla>, cx: &mut Context<Self>) -> Stateful<Div> {
        let shown = self.chats.composer.read(cx).draft.reveals(&a.id);
        let id = a.id.clone();
        div()
            .id(SharedString::from(format!("chat-account-email-{}", a.id)))
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(5.))
            .px(px(4.))
            .mx(px(-4.))
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|d| d.bg(FILL_2))
            .text_size(px(size))
            .text_color(ink.into())
            .child(div().min_w_0().truncate().child(if shown { a.email.clone() } else { MASK.into() }))
            .child(icon(if shown { "eye-off" } else { "eye" }, size, TEXT_4))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                // A Switch account row switches on click; showing its email must not.
                cx.stop_propagation();
                this.chats.composer.update(cx, |c, _| c.draft.toggle_email(&id));
                cx.notify();
            }))
    }
}
