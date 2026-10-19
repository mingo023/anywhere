use crate::composer::{self, Menu};
use crate::composer::launch::{self, Pick};
use crate::desktop::Desktop;
use agents::Summary;
use gpui_kit::*;
use std::rc::Rc;
use theme::*;

/// The agent's access, as read where it can't be switched.
fn access_pill(access: &str) -> Div {
    composer::pill()
        .child(icon("shield", 14., TEXT_2))
        .child(div().whitespace_nowrap().font_weight(FontWeight::MEDIUM).text_color(launch::access_color(access)).child(launch::access_shown(access, "")))
}

fn close(this: &mut Desktop, cx: &mut Context<Desktop>) {
    this.chats.composer.draft.menu = None;
    cx.notify();
}

impl Desktop {
    /// The agent's access, once pocketd names it; a picker on a chat agent, as only those switch it.
    pub(super) fn chat_access_pill(&self, agent: &Summary, view_only: bool, cx: &mut Context<Self>) -> Option<Div> {
        let access = agent.access.as_deref().filter(|a| !a.is_empty())?;
        if view_only || !agent.chat_only || !self.agents.access_offered() {
            return Some(access_pill(access));
        }
        let open = self.chats.composer.draft.menu == Some(Menu::Access);
        let chip = composer::picker(access_pill(access), "chat-access", open)
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.chats.composer.draft.toggle(Menu::Access);
                cx.notify();
            }));
        let menu = open.then(|| {
            let id = agent.id.clone();
            let pick: Pick = Rc::new(move |this, access, cx| {
                this.outbox.access(&id, &access);
                close(this, cx);
            });
            let rows = launch::access_rows("chat-access-row", launch::switch_choices(), access, pick, cx);
            let menu = ui::pop(div().id("chat-access-menu"))
                .w(px(300.))
                .p(px(5.))
                .flex()
                .flex_col()
                .gap(px(2.))
                // Menus hang over the transcript; without this a click on one also lands on what it covers.
                .occlude()
                .children(rows)
                .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| close(this, cx)));
            div().absolute().top(px(-6.)).left_0().child(deferred(anchored().anchor(Anchor::BottomLeft).snap_to_window_with_margin(px(8.)).child(ui::menu_in("chat-access-menu-in", menu))).with_priority(1))
        });
        Some(div().relative().child(chip).children(menu))
    }
}
