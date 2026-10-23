use super::bar;
use crate::accounts::{self, BUILT_IN, Host, Tone};
use crate::composer::Menu;
use crate::desktop::Desktop;
use crate::settings::Section;
use crate::settings::accounts::tag;
use crate::util::now_ms;
use agents::Summary;
use chrono::Local;
use daemon::Account;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::json;
use theme::*;

impl Desktop {
    /// Every account of the agent's provider on its machine; picking one moves the agent there, at once or when its turn ends.
    pub(super) fn chat_account_switch(&self, agent: &Summary, host: &Host, current: &Account, cx: &mut Context<Self>) -> Div {
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
        // The account it runs on stays listed while it switches away, as picking it calls the switch off.
        let running = if agent.account.is_empty() { BUILT_IN } else { &agent.account };
        let rows = accounts::choices(self.accounts.list(host), &agent.provider, &[&current.id, running]).map(|x| self.chat_account_row(agent, host, x, x.id == current.id, cx)).collect::<Vec<_>>();
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
        let why = if a.signed_in { accounts::back_at(a, now_ms(), &Local).map(|b| tag(b, Tone::Bad)) } else { Some(tag("Signed out".into(), Tone::Plain)) };
        let plan = (!a.plan.is_empty()).then(|| tag(a.plan.clone(), Tone::Plain));
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
            let pct = |w: &Option<daemon::Window>| w.as_ref().map_or("\u{2013}".into(), |w| format!("{}%", accounts::percent(w)));
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
}
