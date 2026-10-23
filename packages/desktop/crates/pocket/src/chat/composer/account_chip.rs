mod switch;
mod usage;

use crate::accounts::{self, Host, Tone};
use crate::composer::Menu;
use crate::desktop::Desktop;
use agents::Summary;
use daemon::Account;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;
use theme::*;

const WIDTH: f32 = 340.;
/// How far a page slides in from beside the one it replaces.
const SLIDE: f32 = 48.;

fn ink(tone: Tone) -> Hsla {
    match tone {
        Tone::Bad => FAILED.into(),
        Tone::Warn => WAITING.into(),
        _ => TEXT_3.into(),
    }
}

pub(super) fn bar(percent: u32, w: f32, h: f32) -> Div {
    let fill = div().h_full().w(relative(percent as f32 / 100.)).rounded(px(h / 2.)).bg(ink(accounts::tone(percent)));
    div().w(px(w)).h(px(h)).flex_none().rounded(px(h / 2.)).bg(FILL_4).overflow_hidden().child(fill)
}

/// Slides a page in from `from` px beside it.
fn slide(id: &'static str, from: f32, page: Div) -> AnyElement {
    page.with_animation(id, Animation::new(Duration::from_millis(240)).with_easing(ease_out_quint()), move |e, t| e.relative().left(px(from * (1. - t))).opacity(t)).into_any_element()
}

impl Desktop {
    /// The chat agent's account with its fuller limit, opening its usage and the accounts it can switch to; only while its agent has others on its machine.
    pub(super) fn chat_account_chip(&self, agent: &Summary, cx: &mut Context<Self>) -> Option<Div> {
        if !agent.chat_only {
            return None;
        }
        let host: Host = hosts::session::untag(&agent.id).0.map(str::to_string);
        let list = self.accounts.list(&host);
        // A pocketd that names no switching at a limit is too old to switch by hand either.
        if !accounts::shared(list, &agent.provider) || !self.accounts.auto_switch.contains_key(&host) {
            return None;
        }
        let a = accounts::in_use(list, &agent.provider, agent.shown_account())?;
        let meter = accounts::peak(a).map(|p| {
            let pct = div().text_color(ink(accounts::tone(p))).font_weight(FontWeight::MEDIUM).child(format!("{p}%"));
            div().flex().items_center().gap(px(5.)).child(bar(p, 22., 4.)).child(pct)
        });
        let open = matches!(self.chats.composer.draft.menu, Some(Menu::Usage { .. } | Menu::Switch));
        let chip = div()
            .id("chat-account")
            .h(px(22.))
            .px(px(6.))
            .flex()
            .items_center()
            .gap(px(5.))
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|d| d.bg(FILL_2))
            .when(open, |d| d.bg(FILL_2))
            .child(provider_icon(&agent.provider, 12., TEXT_2))
            .child(div().max_w(px(140.)).truncate().text_color(TEXT_2).child(accounts::name(a).to_string()))
            .children(meter)
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.chats.composer.draft.menu = (!open).then_some(Menu::Usage { back: false });
                cx.notify();
            }));
        let menu = open.then(|| self.chat_account_menu(agent, &host, a, cx));
        Some(div().relative().flex_none().child(chip).children(menu))
    }

    fn chat_account_menu(&self, agent: &Summary, host: &Host, a: &Account, cx: &mut Context<Self>) -> Div {
        let page = match self.chats.composer.draft.menu {
            Some(Menu::Switch) => slide("chat-account-switch-in", SLIDE, self.chat_account_switch(agent, host, a, cx)),
            Some(Menu::Usage { back: true }) => slide("chat-account-usage-back", -SLIDE, self.chat_account_usage(agent, host, a, cx)),
            _ => self.chat_account_usage(agent, host, a, cx).into_any_element(),
        };
        let pop = ui::pop(div().id("chat-account-menu")).w(px(WIDTH)).overflow_hidden().occlude().child(page).on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.chats.composer.draft.menu = None;
            cx.notify();
        }));
        // It opens upward, as the composer sits at the bottom.
        div().absolute().top(px(-6.)).left(px(-6.)).child(deferred(anchored().anchor(Anchor::BottomLeft).snap_to_window_with_margin(px(8.)).child(ui::menu_in("chat-account-menu-in", pop))).with_priority(1))
    }

    pub(crate) fn escape_account_menu(&mut self) -> bool {
        self.chats.composer.draft.escape_account()
    }
}
