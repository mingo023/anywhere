use super::{WIDTH, bar};
use crate::accounts::{self, Host};
use crate::composer::Menu;
use crate::desktop::Desktop;
use crate::util::now_ms;
use agents::Summary;
use daemon::Account;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// A usage window: how much is used, the bar, what's left and when it resets.
fn block(title: &'static str, w: &daemon::Window, now: i64) -> Div {
    let percent = accounts::percent(w);
    let resets = (w.resets_at > 0).then(|| format!("Resets in {}", accounts::until(w.resets_at, now)));
    div()
        .px(px(12.))
        .py(px(11.))
        .flex()
        .flex_col()
        .gap(px(7.))
        .rounded(px(10.))
        .bg(FILL_2)
        .child(
            div().flex().justify_between().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title).child(format!("{percent}% used")),
        )
        .child(bar(percent, WIDTH - 48., 5.).w_full())
        .child(div().flex().justify_between().text_size(px(11.5)).text_color(TEXT_3).child(format!("{}% remaining", 100 - percent)).children(resets))
}

impl Desktop {
    /// The account in use and its two limits, then the way to the others while there are any.
    pub(super) fn chat_account_usage(&self, agent: &Summary, host: &Host, a: &Account, switch: bool, cx: &mut Context<Self>) -> Div {
        let now = now_ms();
        let checking = self.accounts.checking.contains(host);
        let reload = if checking {
            div().size(px(26.)).flex().items_center().justify_center().child(spinner("chat-account-checking", 13., TEXT_3)).into_any_element()
        } else {
            let host = host.clone();
            ui::icon_button_sized("chat-account-reload", "reload", 26., TEXT_3)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.check_accounts(host.clone(), true, cx)))
                .into_any_element()
        };
        let checked = accounts::checked(checking, a.usage.as_ref().map_or(0, |u| u.at), now);
        let head = div()
            .px(px(14.))
            .pt(px(14.))
            .pb(px(10.))
            .flex()
            .items_center()
            .gap(px(10.))
            .child(div().size(px(30.)).flex().flex_none().items_center().justify_center().rounded(px(8.)).bg(FILL_2).child(provider_icon(
                &agent.provider,
                16.,
                TEXT_2,
            )))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(format!("{} usage", provider_name(&agent.provider))),
                    )
                    .child(div().text_size(px(12.)).text_color(TEXT_3).child(checked)),
            )
            .child(reload);
        let label = match (accounts::alias(a), a.email.is_empty()) {
            (None, _) => "In use:".to_string(),
            (Some(who), true) => format!("In use: {who}"),
            (Some(who), false) => format!("In use: {who} \u{b7}"),
        };
        let in_use = div()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(4.))
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .child(div().flex_none().child(label))
            .when(!a.email.is_empty(), |d| d.child(self.chat_account_email(a, 12.5, TEXT_2, cx)));
        let windows = a
            .usage
            .as_ref()
            .into_iter()
            .flat_map(|u| [("5-hour limit", &u.session), ("Weekly limit", &u.week)])
            .filter_map(|(t, w)| w.as_ref().map(|w| block(t, w, now)));
        let hero = div().px(px(12.)).flex().flex_col().gap(px(8.)).child(div().px(px(2.)).child(in_use)).children(windows.collect::<Vec<_>>());
        let page = div().flex().flex_col().child(head);
        if !switch {
            return page.child(hero.pb(px(12.)));
        }
        let list = self.accounts.list(host);
        let more = accounts::choices(list, &agent.provider, &[]).filter(|x| x.id != a.id && x.signed_in).count();
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
                this.chats.composer.update(cx, |c, _| c.draft.menu = Some(Menu::Switch));
                cx.notify();
            }));
        page.child(hero).child(next)
    }
}
