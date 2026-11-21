use crate::desktop::Desktop;
use gpui_kit::*;
use workspace::Place;

impl Desktop {
    /// A top tab's right-click menu, while it is open.
    pub(super) fn top_actions(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (agent, at) = self.panels.top_actions.clone()?;
        let s = self.agents.get(&agent)?;
        let fork = (self.chat_shown(&s.terminal_id) && self.can_fork(s)).then(|| {
            let agent = agent.clone();
            ui::menu_row("top-open-terminal", "terminal", "Open in Terminal chat", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.panels.top_actions = None;
                this.fork_session(&agent, Some(Place::Top), cx);
                cx.notify();
            }))
        });
        let handoff = self.can_hand_off(s).then(|| {
            ui::menu_row("top-handoff", "server", crate::handoff::menu_label(self.handoff.moving(&agent)), None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.panels.top_actions = None;
                this.open_handoff(&agent, window, cx);
            }))
        });
        if fork.is_none() && handoff.is_none() {
            return None;
        }
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
            .children(fork)
            .children(handoff);
        Some(deferred(anchored().position(at).snap_to_window_with_margin(px(8.)).child(ui::menu_in("top-actions-in", menu))).with_priority(1))
    }
}
