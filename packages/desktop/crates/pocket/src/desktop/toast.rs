use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    /// The last error, floated over the main area's bottom-right corner until dismissed or replaced.
    pub(crate) fn error_toast(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let error = self.error.clone()?;
        let mark = div().mt(px(1.)).size(px(18.)).flex().flex_none().items_center().justify_center().rounded(px(9.)).bg(FAILED).child(icon("x-bold", 9., WHITE));
        let dismiss = div()
            .id("error-toast-dismiss")
            .size(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_3))
            .child(icon("x", 12., TEXT_3))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.error = None;
                cx.notify();
            }));
        let toast = ui::pop(div())
            .id("error-toast")
            .absolute()
            .right(px(16.))
            .bottom(px(16.))
            .w(px(300.))
            .pl(px(12.))
            .pr(px(10.))
            .py(px(11.))
            .flex()
            .items_start()
            .gap(px(10.))
            .rounded(px(14.))
            .occlude()
            .child(mark)
            .child(div().flex_1().min_w_0().text_size(px(13.)).text_color(TEXT).child(error))
            .child(dismiss);
        Some(toast.with_animation("error-toast-in", Animation::new(MENU_IN).with_easing(ease_out_quint()), |d, t| d.opacity(t)))
    }
}
