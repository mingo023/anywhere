use super::{Menu, RepoForm};
use crate::accounts;
use daemon::Account;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::LaunchPick;
use theme::*;

fn option(id: impl Into<ElementId>, on: bool, label: String, aside: String, enabled: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(5.))
        .text_size(px(13.))
        .text_color(TEXT)
        .child(div().w(px(14.)).flex_none().when(on, |d| d.child(icon("check", 13., TEXT))))
        .child(div().flex_1().min_w_0().truncate().child(label))
        .child(div().flex_none().pl(px(14.)).text_size(px(12.)).text_color(TEXT_3).child(aside))
        .when(enabled, |d| d.cursor_pointer().hover(|d| d.bg(FILL_2)))
        .when(!enabled, |d| d.opacity(0.45))
}

impl RepoForm {
    /// One row per agent with accounts to pick between on the project's machine; none when no agent has.
    pub(super) fn project_account_rows(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let list = &self.side.accounts;
        LaunchPick::PROVIDERS.into_iter().filter(|p| accounts::shared(list, p)).map(|p| self.project_account_picker(list, p, cx).into_any_element()).collect()
    }

    fn project_account_picker(&self, list: &[Account], provider: &'static str, cx: &mut Context<Self>) -> Div {
        let draft = &self.draft;
        let chosen = draft.accounts.get(provider).and_then(|id| accounts::picked(list, provider, id));
        let default = accounts::default_of(list, provider).map(|a| accounts::name(a).to_string()).unwrap_or_default();
        let shown = match chosen {
            Some(a) => div().text_color(TEXT).child(accounts::name(a).to_string()),
            None => div().flex().gap(px(4.)).child(div().text_color(TEXT).child("Default")).child(div().text_color(TEXT_3).child(format!("\u{b7} {default}"))),
        };
        let open = self.menu == Some(Menu::Account(provider));
        let button = ui::popover_trigger(
            div()
                .id(SharedString::from(format!("repo-account-{provider}")))
                .h(px(28.))
                .pl(px(10.))
                .pr(px(7.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(7.))
                .bg(FILL_3)
                .hover(|d| d.bg(FILL_4))
                .cursor_pointer()
                .text_size(px(12.5))
                .child(shown)
                .child(icon("chevron-down", 11., TEXT_3)),
            open,
            cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                this.menu = Some(Menu::Account(provider));
                cx.notify();
            }),
        );
        let menu = open.then(|| {
            let pick = |id: Option<String>| {
                cx.listener(move |this: &mut Self, _: &ClickEvent, _, cx| {
                    match &id {
                        Some(id) => _ = this.draft.accounts.insert(provider.into(), id.clone()),
                        None => _ = this.draft.accounts.remove(provider),
                    }
                    this.menu = None;
                    cx.notify();
                })
            };
            let first =
                option(SharedString::from(format!("repo-account-{provider}-default")), chosen.is_none(), "Default".into(), format!("{default} now"), true)
                    .on_click(pick(None));
            let keep = [chosen.map_or("", |c| c.id.as_str())];
            let rows = accounts::choices(list, provider, &keep).enumerate().map(|(i, a)| {
                let aside = if a.signed_in { a.email.clone() } else { "Signed out".into() };
                let row = option(
                    (SharedString::from(format!("repo-account-{provider}-option")), i),
                    chosen.is_some_and(|c| c.id == a.id),
                    accounts::name(a).to_string(),
                    aside,
                    a.signed_in,
                );
                if a.signed_in { row.on_click(pick(Some(a.id.clone()))) } else { row }
            });
            let close = cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.menu = None;
                cx.notify();
            });
            let menu = ui::pop(div())
                .min_w(px(260.))
                .p(px(4.))
                .flex()
                .flex_col()
                .child(first)
                .child(ui::menu_divider())
                .children(rows.collect::<Vec<_>>())
                .occlude()
                .on_mouse_down_out(close);
            ui::dropdown_right(34., ui::menu_in(SharedString::from(format!("repo-account-{provider}-menu-in")), menu))
        });
        super::agent_row(provider, self.agent_name(provider)).child(div().relative().flex_none().child(button).children(menu))
    }
}
