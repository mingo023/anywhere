use super::{ChatBoxEvent, ComposerState, chip_menu};
use crate::composer::launch::{self, Pick};
use crate::composer::{self, Menu};
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

pub(super) enum Shown {
    Full(String),
    Plain(String),
    Pick(String),
}

impl Desktop {
    /// The agent's access, once pocketd names it; a picker on a chat agent, as only those switch it. An agent with one
    /// access to run with shows that, whatever pocketd names.
    pub(super) fn chat_access(&self, agent: &Summary, view_only: bool) -> Option<Shown> {
        if self.agents.provider_info(&agent.provider).never_asks() {
            return Some(Shown::Full(self.agent_name(&agent.provider)));
        }
        let access = agent.access.as_deref().filter(|a| !a.is_empty())?.to_string();
        if view_only || !agent.chat_only || !self.agents.access_offered() {
            return Some(Shown::Plain(access));
        }
        Some(Shown::Pick(access))
    }
}

impl ComposerState {
    pub(super) fn access_pill(&self, agent: &str, shown: Shown, cx: &mut Context<Self>) -> Div {
        let access = match shown {
            Shown::Full(name) => return div().child(launch::full_access_chip("chat-access", name)),
            Shown::Plain(access) => return access_pill(&access),
            Shown::Pick(access) => access,
        };
        let open = self.draft.menu == Some(Menu::Access);
        let chip = ui::popover_trigger(
            composer::picker(access_pill(&access), "chat-access", open),
            open,
            cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.draft.toggle(Menu::Access);
                cx.notify();
            }),
        );
        let menu = open.then(|| {
            let id = agent.to_string();
            let pick: Pick<Self> = Rc::new(move |this, access, cx| {
                cx.emit(ChatBoxEvent::Access { agent: id.clone(), access });
                this.close_menu(cx);
            });
            let rows = launch::access_rows("chat-access-row", launch::switch_choices(), &access, pick, cx);
            chip_menu("chat-access-menu", "Access", 300., rows, Self::close_menu, cx)
        });
        div().relative().child(chip).children(menu)
    }
}
