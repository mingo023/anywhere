use super::frame::TopActions;
use super::{Panels, PanelsEvent};
use gpui_kit::*;

impl Panels {
    /// A top tab's right-click menu, while it is open.
    pub(super) fn top_actions(&self, rows: Option<&TopActions>, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (_, at) = self.top_actions.clone()?;
        let rows = rows?;
        let fork = rows.fork.then(|| {
            let agent = rows.agent.clone();
            ui::menu_row("top-open-terminal", "terminal", "Open in Terminal chat", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.top_actions = None;
                cx.emit(PanelsEvent::Fork(agent.clone()));
            }))
        });
        let handoff = rows.handoff.map(|label| {
            let agent = rows.agent.clone();
            ui::menu_row("top-handoff", "server", label, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.top_actions = None;
                cx.emit(PanelsEvent::Handoff(agent.clone()));
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
                this.top_actions = None;
                cx.notify();
            }))
            .children(fork)
            .children(handoff);
        Some(deferred(anchored().position(at).snap_to_window_with_margin(px(8.)).child(ui::menu_in("top-actions-in", menu))).with_priority(1))
    }
}
