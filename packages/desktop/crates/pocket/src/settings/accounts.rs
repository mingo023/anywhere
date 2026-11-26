use super::catalog::Setting;
use super::side::{MachineSide, SignInSide};
use super::{AccountsAct, Settings, SettingsEvent, action, field_box, group, row};
use crate::accounts::{self, BUILT_IN, FLASH_FOR, Key, Tone};
use crate::desktop::chrome::Confirm;
use crate::util::now_ms;
use chrono::Local;
use daemon::Account;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::LaunchPick;
use theme::*;

pub(super) const ROWS: &[Setting] = &[];

pub(crate) fn tag(label: String, tone: Tone) -> Div {
    let (bg, ink) = match tone {
        Tone::Plain => (FILL_3, TEXT_2),
        Tone::Accent => (ACCENT_BG, ACCENT),
        Tone::Warn => (WAITING_BG, WAITING_TEXT),
        Tone::Bad => (FAILED_BG, FAILED_TEXT),
    };
    div()
        .h(px(18.))
        .px(px(6.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(5.))
        .bg(bg)
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(ink)
        .whitespace_nowrap()
        .child(label)
}

fn meter(label: &'static str, w: &daemon::Window, stale: bool, now: i64) -> Div {
    let (percent, tone, reset) = accounts::meter(w, now, &Local);
    let (fill, ink) = match tone {
        _ if stale => (TEXT_5, TEXT_4),
        Tone::Bad => (FAILED, FAILED_TEXT),
        Tone::Warn => (WAITING, WAITING_TEXT),
        _ => (TEXT_3, TEXT_2),
    };
    let track =
        div().flex_1().h(px(5.)).rounded(px(3.)).bg(FILL_4).overflow_hidden().child(div().h_full().w(relative(percent as f32 / 100.)).rounded(px(3.)).bg(fill));
    div()
        .flex()
        .items_center()
        .gap(px(7.))
        .text_size(px(11.5))
        .line_height(px(14.))
        .text_color(TEXT_3)
        .child(div().w(px(34.)).flex_none().child(label))
        .child(track)
        .child(div().w(px(30.)).flex_none().flex().justify_end().font_weight(FontWeight::SEMIBOLD).text_color(ink).child(format!("{percent}%")))
        .child(div().w(px(52.)).flex_none().whitespace_nowrap().text_color(TEXT_4).child(reset))
}

fn meters(a: &Account, now: i64) -> Div {
    let usage = a.usage.as_ref();
    let stale = usage.is_some_and(|u| u.stale);
    let windows =
        usage.into_iter().flat_map(|u| [("5h", &u.session), ("Week", &u.week)]).filter_map(|(label, w)| w.as_ref().map(|w| meter(label, w, stale, now)));
    div().w(px(228.)).flex_none().flex().flex_col().gap(px(5.)).children(windows.collect::<Vec<_>>())
}

fn adds_accounts(provider: &str) -> bool {
    agents::providers::get(provider).is_some_and(|b| b.adds_accounts)
}

/// Why an account can't be removed, which its menu says: the built-in login stays, and an account with sessions running on it
/// stays until they close.
fn remove_blocked(a: &Account) -> Option<String> {
    match a.running {
        _ if a.id == BUILT_IN => Some("The built-in login stays. Sign out instead.".into()),
        0 => None,
        1 => Some("Close its session first.".into()),
        n => Some(format!("Close its {n} sessions first.")),
    }
}

/// Whether "Use for new agents" applies: a signed-in account that new agents don't already start on.
fn can_use_for_new(a: &Account) -> bool {
    a.signed_in && !a.default
}

/// Whether the code field shows its Enter button: once it holds something to send.
fn enter_visible(code: &str) -> bool {
    !code.trim().is_empty()
}

/// How the flash fades: held for a third of its time, then out.
fn flash_fade(t: f32) -> f32 {
    if t < 0.35 { 1. } else { 1. - (t - 0.35) / 0.65 }
}

fn menu_row(id: impl Into<ElementId>, icon: &str, label: &str, enabled: bool) -> Stateful<Div> {
    ui::menu_row(id, icon, label, None).when(!enabled, |d| d.opacity(0.45).cursor_default())
}

impl Settings {
    pub(crate) fn accounts_settings(&self, cx: &mut Context<Self>) -> Div {
        let now = now_ms();
        let checking = self.side.accounts.checking;
        let reload = if checking {
            div().size(px(26.)).flex().items_center().justify_center().child(spinner("accounts-checking", 14., TEXT_3)).into_any_element()
        } else {
            ui::icon_button_sized("accounts-reload", "reload", 26., TEXT_3)
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Accounts(AccountsAct::Reload))))
                .into_any_element()
        };
        let toolbar = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(TEXT_3)
            .child(accounts::checked(checking, self.side.accounts.checked_at, now))
            .child(reload);
        let groups = self.side.accounts.machines.iter().map(|m| {
            let auto = m.auto_switch.is_some().then(|| self.auto_switch_row(m, cx));
            group(&m.title, auto.into_iter().chain(self.machine_rows(m, now, cx)).collect())
        });
        let caption = div()
            .text_size(px(12.))
            .line_height(px(16.))
            .text_color(TEXT_3)
            .child("New sessions start on the selected account, unless their project picks one in Settings \u{203a} Projects. Usage is checked every 5 minutes while Anywhere is open.");
        div().flex().flex_col().gap(px(22.)).child(toolbar).children(groups.collect::<Vec<_>>()).child(caption)
    }

    /// Whether the machine moves a session at its account's limit to another; off while no agent has a second account there.
    fn auto_switch_row(&self, m: &MachineSide, cx: &mut Context<Self>) -> Div {
        let (host, list) = (&m.host, m.list.as_slice());
        let many = LaunchPick::PROVIDERS.into_iter().any(|p| accounts::shared(list, p));
        let on = many && m.auto_switch == Some(true);
        let hint = if many {
            "A session that hits its limit resumes on the account with the most left"
        } else {
            "Add a second account of the same agent to switch between them"
        };
        let id = SharedString::from(format!("accounts-auto-{}", host.as_deref().unwrap_or_default()));
        let host = host.clone();
        let toggle = div().id(id).flex_none().child(ui::toggle("switch", on)).when(!many, |d| d.opacity(0.45)).when(many, |d| {
            d.cursor_pointer()
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Accounts(AccountsAct::SetAuto { host: host.clone(), on: !on }))))
        });
        row("Switch accounts at a limit", Some(hint), toggle)
    }

    /// Each provider's caption, then its accounts.
    fn machine_rows(&self, m: &MachineSide, now: i64, cx: &mut Context<Self>) -> Vec<Div> {
        let (host, list) = (&m.host, &m.list);
        let mut rows = Vec::new();
        for provider in LaunchPick::PROVIDERS {
            let of: Vec<&Account> = list.iter().filter(|a| a.provider == provider && accounts::on_page(a)).collect();
            if of.is_empty() {
                continue;
            }
            let key = Key { host: host.clone(), provider: provider.into(), id: String::new() };
            let signing = m.signins.iter().find(|s| s.provider == provider);
            let add = adds_accounts(provider).then(|| {
                let id = SharedString::from(format!("account-add-{}-{provider}", host.as_deref().unwrap_or_default()));
                match signing {
                    Some(_) => action(id, div().flex().items_center().gap(px(6.)).child(spinner("account-add-spin", 12., TEXT_3)).child("Signing in\u{2026}"))
                        .opacity(0.45)
                        .cursor_default(),
                    None => action(id, "Add account")
                        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Accounts(AccountsAct::SignIn(key.clone()))))),
                }
            });
            rows.push(
                div()
                    .px(px(14.))
                    .py(px(7.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .bg(FILL_1)
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(TEXT_2)
                    .child(provider_icon(provider, 13., TEXT_2))
                    .child(div().flex_1().child(provider_name(provider)))
                    .children(add),
            );
            let shared = accounts::shared(list, provider);
            for a in of {
                match a.pending {
                    // The panel below stands for a sign-in still running here.
                    true if signing.is_some() && a.error.is_empty() => {}
                    true => rows.push(self.pending_row(host, a, cx)),
                    false => rows.push(self.account_row(m, a, shared, now, cx)),
                }
            }
            rows.extend(signing.map(|s| self.sign_in_panel(m, s, cx)));
        }
        rows
    }

    fn account_row(&self, m: &MachineSide, a: &Account, shared: bool, now: i64, cx: &mut Context<Self>) -> Div {
        let host = &m.host;
        let key = Key::of(host, a);
        let key_of_row = key.clone();
        let id = SharedString::from(format!("account-{}-{}-{}", host.as_deref().unwrap_or_default(), a.provider, a.id));
        let name = accounts::name(a).to_string();
        let renaming = self.side.accounts.rename.as_ref().filter(|(k, _)| *k == key).map(|(_, input)| input.clone());
        let title = match renaming {
            Some(input) => div().flex_1().min_w_0().child(Input::new(&input).appearance(false).p_0().text_size(px(13.))),
            None => div().truncate().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(name.clone()),
        };
        let tags = accounts::tags(a, shared, now, &Local).into_iter().map(|(label, tone)| tag(label, tone));
        let line = div().flex().items_center().gap(px(6.)).min_w_0().child(title).children(tags.collect::<Vec<_>>());
        let sub = div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(accounts::subline(a, provider_name(&a.provider), &m.name));
        let who = div().flex_1().min_w_0().flex().flex_col().gap(px(1.)).child(line).child(sub);
        let side = if a.signed_in {
            meters(a, now)
        } else {
            let key = key.clone();
            div().w(px(228.)).flex_none().flex().justify_end().child(
                action(SharedString::from(format!("{id}-sign-in")), "Sign in\u{2026}")
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Accounts(AccountsAct::SignIn(key.clone()))))),
            )
        };
        // Sign out would do nothing for a login pocketd keeps no others of, and Rename only names those others. Its
        // column stays, empty, so the meters line up with the other rows'.
        let mut more = div().relative().flex_none().w(px(26.));
        if adds_accounts(&a.provider) {
            let open = self.side.accounts.menu.as_ref() == Some(&key);
            let toggle = key.clone();
            let trigger = ui::popover_trigger(
                ui::icon_button_sized(SharedString::from(format!("{id}-more")), "more", 26., TEXT_3),
                open,
                cx.listener(move |_, _: &MouseDownEvent, _, cx| cx.emit(SettingsEvent::Accounts(AccountsAct::Menu(Some(toggle.clone()))))),
            );
            let menu = open.then(|| account_menu(&id, key.clone(), a, name, cx));
            more = more.child(trigger).children(menu.map(|m| ui::dropdown_right(30., m)));
        }
        let can_default = !a.default && a.signed_in;
        let radio = div().id(SharedString::from(format!("{id}-default"))).flex_none().child(ui::radio(a.default)).when(!a.signed_in, |d| d.opacity(0.35)).when(
            can_default,
            |d| {
                d.cursor_pointer().on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                    cx.emit(SettingsEvent::Accounts(AccountsAct::Op { host: key.host.clone(), msg: key.op("accounts.default") }))
                }))
            },
        );
        let row = div().px(px(14.)).py(px(10.)).flex().items_center().gap(px(16.)).child(radio).child(who).child(side).child(more);
        if self.side.accounts.flash.as_ref() != Some(&key_of_row) {
            return row;
        }
        let tint = |t: f32| Hsla::from(WAITING_BG).opacity(flash_fade(t));
        div().child(row.with_animation(SharedString::from(format!("{id}-flash")), Animation::new(FLASH_FOR), move |d, t| d.bg(tint(t))))
    }

    /// A sign-in still running in its terminal, or the reason it failed.
    fn pending_row(&self, host: &crate::accounts::Host, a: &Account, cx: &mut Context<Self>) -> Div {
        let row = div().px(px(14.)).py(px(10.)).flex().items_center().gap(px(10.)).text_size(px(12.5));
        if a.error.is_empty() {
            return row
                .child(spinner(SharedString::from(format!("account-pending-{}", a.id)), 13., TEXT_3))
                .child(div().text_color(TEXT_2).child("Signing in\u{2026}"));
        }
        let key = Key::of(host, a);
        let again = Key { id: String::new(), ..key.clone() };
        let retry = action(SharedString::from(format!("account-retry-{}", a.id)), "Try again")
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Accounts(AccountsAct::SignIn(again.clone())))));
        let dismiss = action(SharedString::from(format!("account-dismiss-{}", a.id)), "Dismiss").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.emit(SettingsEvent::Accounts(AccountsAct::Op { host: key.host.clone(), msg: key.op("accounts.remove") }))
        }));
        row.child(icon("warning", 13., FAILED)).child(div().flex_1().min_w_0().text_color(FAILED_TEXT).child(a.error.clone())).child(retry).child(dismiss)
    }

    /// A sign-in running out of sight: what it waits for, the link and code it printed, a field for a code to type back, and its terminal's lines on demand.
    fn sign_in_panel(&self, m: &MachineSide, s: &SignInSide, cx: &mut Context<Self>) -> Div {
        let (host, provider) = (m.host.clone(), s.provider.clone());
        let (code_input, output_open, lines) = (s.code.clone(), s.output_open, s.lines.clone());
        let (link, device) = (accounts::sign_in_link(&lines), accounts::device_code(&lines));
        let id = |part: &str| SharedString::from(format!("signin-{}-{provider}-{part}", host.as_deref().unwrap_or_default()));
        let waiting = match &device {
            Some(_) => format!("Enter this code in your browser to sign in to {}\u{2026}", provider_name(&provider)),
            None => format!("Finish signing in to {} in your browser\u{2026}", provider_name(&provider)),
        };
        let status = div()
            .flex()
            .items_center()
            .gap(px(9.))
            .text_size(px(13.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT)
            .child(spinner(id("spin"), 13., TEXT_3))
            .child(div().min_w_0().truncate().child(waiting))
            .child(div().flex_none().font_weight(FontWeight::NORMAL).text_size(px(12.)).text_color(TEXT_3).child(m.name.clone()));
        let device = device.map(|code| {
            let copy = code.clone();
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(div().font_family(MONO).text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(code))
                .child(
                    action(id("copy-code"), "Copy code")
                        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())))),
                )
        });
        let links = link.map(|link| {
            let copy = link.clone();
            [
                action(id("open"), "Open again").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&link))),
                action(id("copy"), "Copy link")
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())))),
            ]
        });
        // Codex signs in with a code typed into the browser, so only Claude can be handed one back.
        let paste = agents::providers::get(&provider).is_some_and(|b| b.pastes_code).then(|| {
            let enter = enter_visible(code_input.read(cx).value().as_ref()).then(|| {
                let (host, provider) = (host.clone(), provider.clone());
                div()
                    .id(id("send"))
                    .h(px(20.))
                    .px(px(7.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(4.))
                    .rounded(px(5.))
                    .bg(TEXT)
                    .cursor_pointer()
                    .tab_index(0)
                    .text_size(px(11.5))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(PAGE)
                    .whitespace_nowrap()
                    .child("\u{21a9} Enter")
                    .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new("Send the code (\u{21a9})").build(window, cx))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                        cx.emit(SettingsEvent::Accounts(AccountsAct::SendCode { host: host.clone(), provider: provider.clone() }))
                    }))
            });
            let field = div().flex_1().min_w_0().child(Input::new(&code_input).appearance(false).p_0().font_family(MONO).text_size(px(12.)));
            field_box().w(px(260.)).pr(px(3.)).child(field).children(enter.map(super::focus_ring))
        });
        let cancel = {
            let (host, provider) = (host.clone(), provider.clone());
            action(id("cancel"), "Cancel").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.emit(SettingsEvent::Accounts(AccountsAct::Cancel { host: host.clone(), provider: provider.clone() }))
            }))
        };
        let actions = div().flex().items_center().gap(px(6.)).children(links.into_iter().flatten()).children(paste).child(div().flex_1()).child(cancel);
        let (open_host, open_provider) = (host.clone(), provider.clone());
        let toggle = div()
            .id(id("output"))
            .flex()
            .items_center()
            .gap(px(5.))
            .text_size(px(12.))
            .text_color(TEXT_3)
            .cursor_pointer()
            .hover(|d| d.text_color(TEXT))
            .child(icon(if output_open { "chevron-down" } else { "chevron-right" }, 11., TEXT_4))
            .child("Terminal output")
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                cx.emit(SettingsEvent::Accounts(AccountsAct::ToggleOutput { host: open_host.clone(), provider: open_provider.clone() }))
            }));
        let output = output_open.then(|| {
            div()
                .p(px(10.))
                .rounded(px(8.))
                .bg(FILL_2)
                .font_family(MONO)
                .text_size(px(12.))
                .line_height(px(18.))
                .text_color(TEXT_BODY)
                .children(lines.into_iter().map(|l| div().min_h(px(18.)).child(l)))
        });
        div()
            .pl(px(46.))
            .pr(px(14.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(status)
            .children(device)
            .child(actions)
            .child(div().flex().child(toggle))
            .children(output)
    }
}

fn account_menu(id: &SharedString, key: Key, a: &Account, name: String, cx: &mut Context<Settings>) -> impl IntoElement + use<> {
    let can_use = can_use_for_new(a);
    let use_for_new = menu_row(SharedString::from(format!("{id}-use")), "check", "Use for new agents", can_use).when(can_use, |d| {
        let key = key.clone();
        d.on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.emit(SettingsEvent::Accounts(AccountsAct::Menu(None)));
            cx.emit(SettingsEvent::Accounts(AccountsAct::Op { host: key.host.clone(), msg: key.op("accounts.default") }));
        }))
    });
    let rename = (a.id != BUILT_IN).then(|| {
        let (key, name) = (key.clone(), name.clone());
        ui::menu_row(SharedString::from(format!("{id}-rename")), "pencil", "Rename", None).on_click(
            cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Accounts(AccountsAct::Rename { key: key.clone(), name: name.clone() }))),
        )
    });
    let sign = if a.signed_in {
        let (key, name) = (key.clone(), name.clone());
        ui::menu_row(SharedString::from(format!("{id}-sign-out")), "x", "Sign out", None).on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.emit(SettingsEvent::Accounts(AccountsAct::Menu(None)));
            cx.emit(SettingsEvent::Ask(Confirm::SignOutAccount { key: key.clone(), name: name.clone() }));
        }))
    } else {
        let key = key.clone();
        ui::menu_row(SharedString::from(format!("{id}-sign-in-menu")), "arrow-right", "Sign in\u{2026}", None)
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Accounts(AccountsAct::SignIn(key.clone())))))
    };
    let why = remove_blocked(a);
    let remove = match why {
        Some(_) => menu_row(SharedString::from(format!("{id}-remove")), "trash", "Remove\u{2026}", false),
        None => ui::danger_row(SharedString::from(format!("{id}-remove")), "trash", "Remove\u{2026}").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
            cx.emit(SettingsEvent::Accounts(AccountsAct::Menu(None)));
            cx.emit(SettingsEvent::Ask(Confirm::RemoveAccount { key: key.clone(), name: name.clone() }));
        })),
    };
    let why = why.map(|w| div().px(px(10.)).pb(px(6.)).max_w(px(220.)).text_size(px(11.5)).line_height(px(15.)).text_color(TEXT_4).child(w));
    let rows = div().child(use_for_new).children(rename).child(ui::menu_divider()).child(sign).child(remove).children(why);
    let menu = ui::pop(div())
        .min_w(px(220.))
        .p(px(4.))
        .flex()
        .flex_col()
        .occlude()
        .on_mouse_down_out(cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(SettingsEvent::Accounts(AccountsAct::Menu(None)))));
    ui::menu_in(SharedString::from(format!("{id}-menu-in")), menu.child(rows))
}

#[cfg(test)]
mod tests {
    use super::{can_use_for_new, enter_visible, flash_fade, remove_blocked};
    use crate::accounts::BUILT_IN;
    use daemon::Account;

    fn account(id: &str) -> Account {
        Account { id: id.into(), provider: "claude".into(), signed_in: true, ..Default::default() }
    }

    #[test]
    fn the_built_in_login_cannot_be_removed_and_the_menu_says_to_sign_out() {
        assert_eq!(remove_blocked(&account(BUILT_IN)).as_deref(), Some("The built-in login stays. Sign out instead."));
        assert_eq!(remove_blocked(&Account { running: 2, ..account(BUILT_IN) }).as_deref(), Some("The built-in login stays. Sign out instead."));
    }

    #[test]
    fn an_account_with_sessions_cannot_be_removed_until_they_close() {
        assert_eq!(remove_blocked(&Account { running: 1, ..account("work") }).as_deref(), Some("Close its session first."));
        assert_eq!(remove_blocked(&Account { running: 3, ..account("work") }).as_deref(), Some("Close its 3 sessions first."));
        assert_eq!(remove_blocked(&account("work")), None);
    }

    #[test]
    fn only_a_signed_in_account_new_agents_do_not_already_use_can_be_picked() {
        assert!(can_use_for_new(&account("work")));
        assert!(!can_use_for_new(&Account { default: true, ..account("work") }));
        assert!(!can_use_for_new(&Account { signed_in: false, ..account("work") }));
    }

    #[test]
    fn the_enter_button_shows_once_the_code_field_has_text_and_hides_when_cleared() {
        assert!(!enter_visible(""));
        assert!(!enter_visible("  \n"));
        assert!(enter_visible("a8Kq2-Lm9x"));
    }

    #[test]
    fn the_flash_holds_then_fades_out() {
        assert_eq!((flash_fade(0.), flash_fade(0.35), flash_fade(1.)), (1., 1., 0.));
        assert!(flash_fade(0.6) < 1. && flash_fade(0.6) > 0.);
    }
}
