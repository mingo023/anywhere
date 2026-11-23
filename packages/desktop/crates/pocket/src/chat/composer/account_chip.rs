mod email;
mod switch;
mod usage;

use super::ComposerState;
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
    page.with_animation(id, Animation::new(Duration::from_millis(240)).with_easing(ease_out_quint()), move |e, t| {
        e.relative().left(px(from * (1. - t))).opacity(t)
    })
    .into_any_element()
}

/// What the account chip and its pages draw: the agent's account, the others on its machine, and what pocketd last said of them.
pub(crate) struct Chip {
    agent: Summary,
    host: Host,
    account: Account,
    /// Whether the agent can move to another account of its provider on its machine.
    switch: bool,
    checking: bool,
    machine: String,
    /// How many other signed-in accounts it could switch to.
    more: usize,
    /// Every account its Switch page lists, built only while a page is open.
    rows: Vec<Account>,
}

/// What a page of the account chip asks of `Desktop`, which checks usage, switches accounts and opens Settings.
pub(crate) enum Ask {
    Check(Host),
    Switch { host: Host, provider: String, id: String, session: String },
    Manage,
}

impl Desktop {
    /// The chat agent's account with its fuller limit, opening its usage and, while its machine has others, the accounts it can switch to.
    pub(super) fn chat_account_chip(&self, agent: &Summary, open: bool) -> Option<Chip> {
        if !agent.chat_only {
            return None;
        }
        let host: Host = hosts::session::untag(&agent.id).0.map(str::to_string);
        let (a, switch) = self.accounts.chip(&host, &agent.provider, agent.shown_account())?;
        let list = self.accounts.list(&host);
        let more = accounts::choices(list, &agent.provider, &[]).filter(|x| x.id != a.id && x.signed_in).count();
        // The account it runs on stays listed while it switches away, as picking it calls the switch off.
        let running = if agent.account.is_empty() { accounts::BUILT_IN } else { &agent.account };
        let rows = if open { accounts::choices(list, &agent.provider, &[&a.id, running]).cloned().collect() } else { Vec::new() };
        Some(Chip {
            agent: agent.clone(),
            checking: self.accounts.checking.contains(&host),
            machine: self.machine_name(&host),
            account: a.clone(),
            host,
            switch,
            more,
            rows,
        })
    }

    pub(super) fn on_account_ask(&mut self, ask: &Ask, window: &mut Window, cx: &mut Context<Self>) {
        match ask {
            Ask::Check(host) => self.check_accounts(host.clone(), true, cx),
            Ask::Switch { host, provider, id, session } => {
                self.account_op(host.clone(), serde_json::json!({"op": "accounts.switch", "key": provider, "id": id, "text": session}), cx)
            }
            Ask::Manage => {
                self.open_settings(&crate::actions::OpenSettings, window, cx);
                self.show_section(crate::settings::Section::Accounts, cx);
            }
        }
        cx.notify();
    }
}

impl ComposerState {
    pub(super) fn account_chip(&self, chip: &Chip, cx: &mut Context<Self>) -> Div {
        let (agent, a) = (&chip.agent, &chip.account);
        let meter = accounts::peak(a).map(|p| {
            let pct = div().text_color(ink(accounts::tone(p))).font_weight(FontWeight::MEDIUM).child(format!("{p}%"));
            div().flex().items_center().gap(px(5.)).child(bar(p, 22., 4.)).child(pct)
        });
        let open = matches!(self.draft.menu, Some(Menu::Usage { .. } | Menu::Switch));
        let trigger = ui::popover_trigger(
            div()
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
                .when(!a.label.is_empty(), |d| d.child(div().max_w(px(140.)).truncate().text_color(TEXT_2).child(a.label.clone())))
                .children(meter),
            open,
            cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.draft.toggle_account();
                cx.notify();
            }),
        );
        let menu = open.then(|| self.account_menu(chip, cx));
        div().relative().flex_none().child(trigger).children(menu)
    }

    fn account_menu(&self, chip: &Chip, cx: &mut Context<Self>) -> Div {
        let page = match self.draft.menu {
            Some(Menu::Switch) if chip.switch => slide("chat-account-switch-in", SLIDE, self.account_switch(chip, cx)),
            Some(Menu::Usage { back: true }) => slide("chat-account-usage-back", -SLIDE, self.account_usage(chip, cx)),
            _ => self.account_usage(chip, cx).into_any_element(),
        };
        let pop = ui::pop(div().id("chat-account-menu")).w(px(WIDTH)).overflow_hidden().occlude().child(page).on_mouse_down_out(cx.listener(
            |this, _: &MouseDownEvent, _, cx| {
                this.close_menu(cx);
            },
        ));
        // It opens upward, as the composer sits at the bottom.
        div().absolute().top(px(-6.)).left(px(-6.)).child(
            deferred(anchored().anchor(Anchor::BottomLeft).snap_to_window_with_margin(px(8.)).child(ui::menu_in("chat-account-menu-in", pop))).with_priority(1),
        )
    }
}

impl Desktop {
    pub(crate) fn escape_composer_menu(&mut self, cx: &mut App) -> bool {
        self.chats.composer.update(cx, |c, _| c.draft.escape_account() || c.draft.escape_model())
    }
}
