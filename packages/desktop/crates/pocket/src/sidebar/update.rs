use crate::desktop::Desktop;
use crate::updates::{Update, release_url};
use gpui_kit::*;
use theme::*;

impl Desktop {
    /// "Restart to update" once an update is ready; "What's new" after a relaunch on a new version.
    pub(super) fn update_card(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        match &self.updates.update {
            Update::Idle => None,
            Update::Ready { version } => Some(
                ui::button("aside-restart-update", ui::Variant::Accent, Some("reload"), format!("Restart to update · v{version}"))
                    .mt(px(8.))
                    .mx(px(2.))
                    .h(px(34.))
                    .rounded(px(17.))
                    .justify_center()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.restart_to_update(cx)))
                    .into_any_element(),
            ),
            Update::WhatsNew { version } => {
                let url = release_url(version);
                let dismiss = div()
                    .id("aside-whats-new-dismiss")
                    .size(px(20.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .justify_center()
                    .rounded(px(5.))
                    .hover(|s| s.bg(FILL_3))
                    .child(icon("x", 12., TEXT_3))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.dismiss_whats_new(cx);
                    }));
                Some(
                    div()
                        .id("aside-whats-new")
                        .mt(px(8.))
                        .mx(px(2.))
                        .py(px(8.))
                        .pl(px(10.))
                        .pr(px(6.))
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .rounded(px(12.))
                        .bg(Token::new(0xffffff8c, 0xebebeb0d))
                        .shadow(vec![ui::ring(FILL_3, 0.5)])
                        .text_size(px(12.))
                        .text_color(TEXT)
                        .cursor_pointer()
                        .child(icon("sparkle", 12., TEXT_2))
                        .child(div().flex_1().min_w_0().truncate().child(format!("What's new in {version}")))
                        .child(dismiss)
                        .on_click(move |_: &ClickEvent, _: &mut Window, cx: &mut App| cx.open_url(&url))
                        .into_any_element(),
                )
            }
        }
    }
}
