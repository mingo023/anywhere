use crate::desktop::Desktop;
use gpui_kit::*;
use workspace::Place;

impl Desktop {
    /// A top tab's right-click menu, while it is open.
    pub(super) fn top_actions(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (agent, at) = self.panels.top_actions.clone()?;
        self.agents.get(&agent).filter(|a| self.can_fork(a))?;
        let open = ui::menu_row("top-open-terminal", "terminal", "Open in Terminal chat", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.panels.top_actions = None;
            this.fork_session(&agent, Some(Place::Top), cx);
            cx.notify();
        }));
        let menu = ui::pop(div().id("top-actions"))
            .w(px(220.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.panels.top_actions = None;
                cx.notify();
            }))
            .child(open);
        Some(deferred(anchored().position(at).snap_to_window_with_margin(px(8.)).child(ui::menu_in("top-actions-in", menu))).with_priority(1))
    }
}
