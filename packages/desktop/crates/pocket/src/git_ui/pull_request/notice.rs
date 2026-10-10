use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    /// What a PR action just did, with Undo when it can be taken back.
    pub(crate) fn pr_notice(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let notice = self.pr.notice.as_ref().filter(|_| self.error.is_none())?;
        let raised = self.removals.label().is_some() || self.chat.added.is_some();
        let undo = notice.undo.is_some().then(|| {
            ui::link("pr-undo", "Undo").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.undo_pr(cx)))
        });
        let toast = ui::pop(div())
            .absolute()
            .right(px(16.))
            .bottom(px(if raised { 56. } else { 16. }))
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
            .children(undo);
        Some(toast.with_animation("pr-notice-in", Animation::new(MENU_IN).with_easing(ease_out_quint()), |d, t| d.opacity(t)))
    }
}
