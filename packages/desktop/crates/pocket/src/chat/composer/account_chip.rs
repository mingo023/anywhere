use crate::accounts::{self, BUILT_IN, Host, Tone};
use crate::composer::Menu;
use crate::desktop::Desktop;
use crate::settings::Section;
use crate::util::now_ms;
use agents::Summary;
use chrono::Local;
use daemon::Account;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::json;
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

fn bar(percent: u32, w: f32, h: f32) -> Div {
    let fill = div().h_full().w(relative(percent as f32 / 100.)).rounded(px(h / 2.)).bg(ink(accounts::tone(percent)));
    div().w(px(w)).h(px(h)).flex_none().rounded(px(h / 2.)).bg(FILL_4).overflow_hidden().child(fill)
}

fn tag(label: impl Into<SharedString>, bad: bool) -> Div {
    let (bg, ink) = if bad { (FAILED_BG, FAILED_TEXT) } else { (FILL_3, TEXT_2) };
    div().h(px(17.)).px(px(5.)).flex().flex_none().items_center().rounded(px(5.)).bg(bg).text_size(px(10.5)).font_weight(FontWeight::SEMIBOLD).text_color(ink).child(label.into())
}

/// A usage window: how much is used, the bar, what's left and when it resets.
fn block(title: &'static str, w: &daemon::Window, now: i64) -> Div {
    let percent = w.percent.round().clamp(0., 100.) as u32;
    let resets = (w.resets_at > 0).then(|| format!("Resets in {}", accounts::until(w.resets_at, now)));
    div()
        .px(px(12.))
        .py(px(11.))
        .flex()
        .flex_col()
        .gap(px(7.))
        .rounded(px(10.))
        .bg(FILL_2)
        .child(div().flex().justify_between().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title).child(format!("{percent}% used")))
        .child(bar(percent, WIDTH - 48., 5.).w_full())
        .child(div().flex().justify_between().text_size(px(11.5)).text_color(TEXT_3).child(format!("{}% remaining", 100 - percent)).children(resets))
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
        if !accounts::shared(list, &agent.provider) {
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

    /// The account in use and its two limits, then the way to the others.
    fn chat_account_usage(&self, agent: &Summary, host: &Host, a: &Account, cx: &mut Context<Self>) -> Div {
        let now = now_ms();
        let checking = self.accounts.checking.contains(host);
        let reload = if checking {
            div().size(px(26.)).flex().items_center().justify_center().child(spinner("chat-account-checking", 13., TEXT_3)).into_any_element()
        } else {
            let host = host.clone();
            ui::icon_button_sized("chat-account-reload", "reload", 26., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.check_accounts(host.clone(), true, cx))).into_any_element()
        };
        let checked = accounts::checked(checking, a.usage.as_ref().map_or(0, |u| u.at), now);
        let head = div()
            .px(px(14.))
            .pt(px(14.))
            .pb(px(10.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().size(px(30.)).flex().flex_none().items_center().justify_center().rounded(px(8.)).bg(FILL_2).child(provider_icon(&agent.provider, 16., TEXT_2)))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(div().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(format!("{} usage", provider_name(&agent.provider))))
                    .child(div().text_size(px(12.)).text_color(TEXT_3).child(checked)),
            )
            .child(reload);
        let who = [a.plan.clone(), a.email.clone()].into_iter().filter(|s| !s.is_empty() && s != accounts::name(a));
        let in_use = div()
            .truncate()
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .child(format!("In use: {}", std::iter::once(accounts::name(a).to_string()).chain(who).collect::<Vec<_>>().join(" \u{b7} ")));
        let windows = a.usage.as_ref().into_iter().flat_map(|u| [("5-hour limit", &u.session), ("Weekly limit", &u.week)]).filter_map(|(t, w)| w.as_ref().map(|w| block(t, w, now)));
        let list = self.accounts.list(host);
        let more = accounts::listed(list, &agent.provider).filter(|x| x.id != a.id && x.signed_in).count();
        let next = div()
            .id("chat-account-more")
            .mx(px(12.))
            .mt(px(8.))
            .mb(px(12.))
            .h(px(38.))
            .pl(px(12.))
            .pr(px(10.))
            .flex()
            .items_center()
            .gap(px(9.))
            .rounded(px(10.))
            .bg(FILL_2)
            .cursor_pointer()
            .hover(|d| d.bg(FILL_4))
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(TEXT)
            .child("Switch account")
            .child(div().flex_1())
            .child(div().font_weight(FontWeight::NORMAL).text_size(px(12.)).text_color(TEXT_3).child(format!("{more} more")))
            .child(icon("chevron-right", 12., TEXT_4))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.chats.composer.draft.menu = Some(Menu::Switch);
                cx.notify();
            }));
        let hero = div().px(px(12.)).flex().flex_col().gap(px(8.)).child(div().px(px(2.)).child(in_use)).children(windows.collect::<Vec<_>>());
        div().flex().flex_col().child(head).child(hero).child(next)
    }

    /// Every account of the agent's provider on its machine; picking one moves the agent there, at once or when its turn ends.
    fn chat_account_switch(&self, agent: &Summary, host: &Host, current: &Account, cx: &mut Context<Self>) -> Div {
        let back = div()
            .id("chat-account-back")
            .h(px(28.))
            .pl(px(4.))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(2.))
            .rounded(px(7.))
            .cursor_pointer()
            .hover(|d| d.bg(FILL_3))
            .text_size(px(13.))
            .text_color(TEXT_2)
            .child(icon("back", 13., TEXT_2))
            .child("Usage")
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.chats.composer.draft.menu = Some(Menu::Usage { back: true });
                cx.notify();
            }));
        let title = div().absolute().left_0().right_0().flex().justify_center().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("Switch account");
        let head = div().relative().h(px(46.)).px(px(8.)).flex().items_center().child(title).child(back);
        let rows = accounts::listed(self.accounts.list(host), &agent.provider).map(|x| self.chat_account_row(agent, host, x, x.id == current.id, cx)).collect::<Vec<_>>();
        let manage = div()
            .id("chat-account-manage")
            .h(px(24.))
            .px(px(8.))
            .flex()
            .items_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|d| d.bg(FILL_3))
            .text_color(TEXT_2)
            .child("Manage\u{2026}")
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.chats.composer.draft.menu = None;
                this.open_settings(&crate::actions::OpenSettings, window, cx);
                this.show_section(Section::Accounts, cx);
            }));
        let only = format!("Only {} accounts on {}", provider_name(&agent.provider), self.machine_name(host));
        let foot = div().pl(px(16.)).pr(px(10.)).pt(px(8.)).pb(px(10.)).flex().items_center().justify_between().border_t(px(0.5)).border_color(SEPARATOR).text_size(px(12.)).text_color(TEXT_3).child(only).child(manage);
        div().flex().flex_col().child(head).child(div().px(px(6.)).pt(px(2.)).pb(px(4.)).flex().flex_col().children(rows)).child(foot)
    }

    fn chat_account_row(&self, agent: &Summary, host: &Host, a: &Account, current: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let why = if a.signed_in { accounts::back_at(a, now_ms(), &Local).map(|b| tag(b, true)) } else { Some(tag("Signed out", false)) };
        let plan = (!a.plan.is_empty()).then(|| tag(a.plan.clone(), false));
        let name = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .min_w_0()
            .text_size(px(13.))
            .font_weight(if current { FontWeight::SEMIBOLD } else { FontWeight::MEDIUM })
            .text_color(TEXT)
            .child(div().min_w_0().truncate().child(accounts::name(a).to_string()))
            .children(plan)
            .children(why);
        let email = (!a.label.is_empty() || a.id == BUILT_IN).then(|| a.email.clone()).filter(|e| !e.is_empty() && e != accounts::name(a));
        let who = div().flex_1().min_w_0().flex().flex_col().child(name).children(email.map(|e| div().truncate().text_size(px(11.5)).text_color(TEXT_3).child(e)));
        let usage = a.usage.as_ref().filter(|_| a.signed_in).map(|u| {
            let pct = |w: &Option<daemon::Window>| w.as_ref().map_or("\u{2013}".into(), |w| format!("{}%", w.percent.round().clamp(0., 100.)));
            div()
                .flex()
                .flex_none()
                .flex_col()
                .items_end()
                .gap(px(4.))
                .child(div().text_size(px(11.5)).text_color(TEXT_3).child(format!("5h {} \u{b7} wk {}", pct(&u.session), pct(&u.week))))
                .child(bar(accounts::peak(a).unwrap_or(0), 64., 4.))
        });
        let check = div().w(px(14.)).flex_none().when(current, |d| d.child(icon("check", 13., TEXT)));
        let row = div().id(SharedString::from(format!("chat-account-row-{}", a.id))).px(px(10.)).py(px(7.)).flex().items_center().gap(px(10.)).rounded(px(8.)).child(check).child(who).children(usage);
        if !a.signed_in {
            return row.opacity(0.5);
        }
        if current {
            return row;
        }
        let (id, provider, theirs) = (a.id.clone(), agent.provider.clone(), hosts::session::untag(&agent.id).1.to_string());
        let host = host.clone();
        row.cursor_pointer().hover(|d| d.bg(FILL_3)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.chats.composer.draft.menu = None;
            this.account_op(host.clone(), json!({"op": "accounts.switch", "key": provider, "id": id, "text": theirs}), cx);
            cx.notify();
        }))
    }

    /// Esc on the accounts goes back to the usage; on the usage it closes. False while neither shows.
    pub(crate) fn escape_account_menu(&mut self) -> bool {
        let menu = &mut self.chats.composer.draft.menu;
        match menu {
            Some(Menu::Switch) => *menu = Some(Menu::Usage { back: true }),
            Some(Menu::Usage { .. }) => *menu = None,
            _ => return false,
        }
        true
    }
}
