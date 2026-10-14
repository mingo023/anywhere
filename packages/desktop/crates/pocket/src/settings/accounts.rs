use super::catalog::Setting;
use super::{action, field_box, group};
use crate::accounts::{self, BUILT_IN, Host, Key, Tone};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use crate::util::now_ms;
use chrono::Local;
use daemon::Account;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::LaunchPick;
use theme::*;

pub(super) const ROWS: &[Setting] = &[];

fn tag(label: String, tone: Tone) -> Div {
    let (bg, ink) = match tone {
        Tone::Plain => (FILL_3, TEXT_2),
        Tone::Accent => (ACCENT_BG, ACCENT),
        Tone::Warn => (WAITING_BG, WAITING_TEXT),
        Tone::Bad => (FAILED_BG, FAILED_TEXT),
    };
    div().h(px(18.)).px(px(6.)).flex().flex_none().items_center().rounded(px(5.)).bg(bg).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(ink).whitespace_nowrap().child(label)
}

fn meter(label: &'static str, w: &daemon::Window, stale: bool, now: i64) -> Div {
    let (percent, tone, reset) = accounts::meter(w, now, &Local);
    let (fill, ink) = match tone {
        _ if stale => (TEXT_5, TEXT_4),
        Tone::Bad => (FAILED, FAILED_TEXT),
        Tone::Warn => (WAITING, WAITING_TEXT),
        _ => (TEXT_3, TEXT_2),
    };
    let track = div().flex_1().h(px(5.)).rounded(px(3.)).bg(FILL_4).overflow_hidden().child(div().h_full().w(relative(percent as f32 / 100.)).rounded(px(3.)).bg(fill));
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
    let windows = usage.into_iter().flat_map(|u| [("5h", &u.session), ("Week", &u.week)]).filter_map(|(label, w)| w.as_ref().map(|w| meter(label, w, stale, now)));
    div().w(px(228.)).flex_none().flex().flex_col().gap(px(5.)).children(windows.collect::<Vec<_>>())
}

fn menu_row(id: impl Into<ElementId>, icon: &str, label: &str, enabled: bool) -> Stateful<Div> {
    ui::menu_row(id, icon, label, None).when(!enabled, |d| d.opacity(0.45).cursor_default())
}

impl Desktop {
    pub(crate) fn accounts_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let now = now_ms();
        let checking = !self.accounts.checking.is_empty();
        let reload = if checking {
            div().size(px(26.)).flex().items_center().justify_center().child(spinner("accounts-checking", 14., TEXT_3)).into_any_element()
        } else {
            ui::icon_button_sized("accounts-reload", "reload", 26., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.refresh_accounts(cx))).into_any_element()
        };
        let toolbar = div().flex().items_center().gap(px(6.)).text_size(px(12.)).text_color(TEXT_3).child(accounts::checked(checking, self.accounts.checked_at, now)).child(reload);
        let machines: Vec<Host> = std::iter::once(None).chain(self.store.hosts.iter().map(|h| Some(h.id.clone()))).filter(|h| self.accounts.lists.contains_key(h)).collect();
        let groups = machines.into_iter().map(|host| {
            let title = host.as_deref().map_or_else(|| "This Mac".into(), |id| self.host_name(id));
            group(&title, self.machine_rows(&host, now, cx))
        });
        let caption = div()
            .text_size(px(12.))
            .line_height(px(16.))
            .text_color(TEXT_3)
            .child("Agents start on the selected account, and move to it when idle. Usage is checked every 5 minutes while Anywhere is open.");
        div().flex().flex_col().gap(px(22.)).child(toolbar).children(groups.collect::<Vec<_>>()).child(caption)
    }

    /// Each provider's caption, then its accounts.
    fn machine_rows(&mut self, host: &Host, now: i64, cx: &mut Context<Self>) -> Vec<Div> {
        let list = self.accounts.lists.get(host).cloned().unwrap_or_default();
        let mut rows = Vec::new();
        for provider in LaunchPick::PROVIDERS {
            let of: Vec<&Account> = list.iter().filter(|a| a.provider == provider).collect();
            if of.is_empty() {
                continue;
            }
            let key = Key { host: host.clone(), provider: provider.into(), id: String::new() };
            let signing = self.accounts.signins.iter().position(|s| s.on(host, provider));
            let add = action(SharedString::from(format!("account-add-{}-{provider}", host.as_deref().unwrap_or_default())), "Add account").when(signing.is_some(), |d| d.opacity(0.45).cursor_default()).when(signing.is_none(), |d| {
                d.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.sign_in_account(key.clone(), window, cx)))
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
                    .child(add),
            );
            let shared = accounts::shared(&list, provider);
            for a in of {
                match a.pending {
                    // The panel below stands for a sign-in still running here.
                    true if signing.is_some() && a.error.is_empty() => {}
                    true => rows.push(self.pending_row(host, a, cx)),
                    false => rows.push(self.account_row(host, a, shared, now, cx)),
                }
            }
            rows.extend(signing.map(|i| self.sign_in_panel(i, cx)));
        }
        rows
    }

    fn account_row(&mut self, host: &Host, a: &Account, shared: bool, now: i64, cx: &mut Context<Self>) -> Div {
        let key = Key::of(host, a);
        let id = SharedString::from(format!("account-{}-{}-{}", host.as_deref().unwrap_or_default(), a.provider, a.id));
        let name = accounts::name(a).to_string();
        let renaming = self.accounts.rename.as_ref().filter(|r| r.key == key).map(|r| r.input.clone());
        let title = match renaming {
            Some(input) => div().flex_1().min_w_0().child(Input::new(&input).appearance(false).p_0().text_size(px(13.))),
            None => div().truncate().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(name.clone()),
        };
        let tags = accounts::tags(a, shared, now, &Local).into_iter().map(|(label, tone)| tag(label, tone));
        let line = div().flex().items_center().gap(px(6.)).min_w_0().child(title).children(tags.collect::<Vec<_>>());
        let sub = div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(accounts::subline(a, provider_name(&a.provider), &self.machine_name(host)));
        let who = div().flex_1().min_w_0().flex().flex_col().gap(px(1.)).child(line).child(sub);
        let side = if a.signed_in {
            meters(a, now)
        } else {
            let key = key.clone();
            div().w(px(228.)).flex_none().flex().justify_end().child(action(SharedString::from(format!("{id}-sign-in")), "Sign in\u{2026}").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.sign_in_account(key.clone(), window, cx))))
        };
        let open = self.accounts.menu.as_ref() == Some(&key);
        let toggle = key.clone();
        let more = ui::icon_button_sized(SharedString::from(format!("{id}-more")), "more", 26., TEXT_3).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            this.accounts.menu = (!open).then(|| toggle.clone());
            cx.notify();
        }));
        let menu = open.then(|| self.account_menu(&id, key.clone(), a, name, cx));
        let can_default = !a.default && a.signed_in;
        let radio = div().id(SharedString::from(format!("{id}-default"))).flex_none().child(ui::radio(a.default)).when(!a.signed_in, |d| d.opacity(0.35)).when(can_default, |d| {
            d.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.account_op(key.host.clone(), key.op("accounts.default"), cx)))
        });
        div().px(px(14.)).py(px(10.)).flex().items_center().gap(px(16.)).child(radio).child(who).child(side).child(div().relative().flex_none().child(more).children(menu.map(|m| ui::dropdown_right(30., m))))
    }

    fn account_menu(&mut self, id: &SharedString, key: Key, a: &Account, name: String, cx: &mut Context<Self>) -> impl IntoElement + use<> {
        let rename = (a.id != BUILT_IN).then(|| {
            let (key, name) = (key.clone(), name.clone());
            ui::menu_row(SharedString::from(format!("{id}-rename")), "pencil", "Rename", None)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_account_rename(key.clone(), name.clone(), window, cx)))
        });
        let sign = if a.signed_in {
            let (key, name) = (key.clone(), name.clone());
            ui::menu_row(SharedString::from(format!("{id}-sign-out")), "x", "Sign out", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.accounts.menu = None;
                (this.confirm, this.overlay) = (Some(Confirm::SignOutAccount { key: key.clone(), name: name.clone() }), Some(Overlay::Confirm));
                cx.notify();
            }))
        } else {
            let key = key.clone();
            ui::menu_row(SharedString::from(format!("{id}-sign-in-menu")), "arrow-right", "Sign in\u{2026}", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.sign_in_account(key.clone(), window, cx)))
        };
        let why = match a.running {
            _ if a.id == BUILT_IN => Some("The built-in login stays. Sign out instead.".to_string()),
            0 => None,
            1 => Some("Close its session first.".into()),
            n => Some(format!("Close its {n} sessions first.")),
        };
        let remove = match why {
            Some(_) => menu_row(SharedString::from(format!("{id}-remove")), "trash", "Remove\u{2026}", false),
            None => ui::danger_row(SharedString::from(format!("{id}-remove")), "trash", "Remove\u{2026}").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.accounts.menu = None;
                (this.confirm, this.overlay) = (Some(Confirm::RemoveAccount { key: key.clone(), name: name.clone() }), Some(Overlay::Confirm));
                cx.notify();
            })),
        };
        let why = why.map(|w| div().px(px(10.)).pb(px(6.)).max_w(px(220.)).text_size(px(11.5)).line_height(px(15.)).text_color(TEXT_4).child(w));
        let rows = div().children(rename).when(a.id != BUILT_IN, |d| d.child(ui::menu_divider())).child(sign).child(remove).children(why);
        let menu = ui::pop(div()).min_w(px(220.)).p(px(4.)).flex().flex_col().occlude().on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.accounts.menu = None;
            cx.notify();
        }));
        ui::menu_in(SharedString::from(format!("{id}-menu-in")), menu.child(rows))
    }

    /// A sign-in still running in its terminal, or the reason it failed.
    fn pending_row(&mut self, host: &Host, a: &Account, cx: &mut Context<Self>) -> Div {
        let row = div().px(px(14.)).py(px(10.)).flex().items_center().gap(px(10.)).text_size(px(12.5));
        if a.error.is_empty() {
            return row.child(spinner(SharedString::from(format!("account-pending-{}", a.id)), 13., TEXT_3)).child(div().text_color(TEXT_2).child("Signing in\u{2026}"));
        }
        let key = Key::of(host, a);
        let again = Key { id: String::new(), ..key.clone() };
        let retry = action(SharedString::from(format!("account-retry-{}", a.id)), "Try again").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.sign_in_account(again.clone(), window, cx)));
        let dismiss = action(SharedString::from(format!("account-dismiss-{}", a.id)), "Dismiss").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.account_op(key.host.clone(), key.op("accounts.remove"), cx)));
        row.child(icon("warning", 13., FAILED)).child(div().flex_1().min_w_0().text_color(FAILED_TEXT).child(a.error.clone())).child(retry).child(dismiss)
    }

    /// A sign-in running out of sight: what it waits for, the link and code it printed, a field for a code to type back, and its terminal's lines on demand.
    fn sign_in_panel(&mut self, i: usize, cx: &mut Context<Self>) -> Div {
        let s = &self.accounts.signins[i];
        let (host, provider) = (s.key.host.clone(), s.key.provider.clone());
        let (code_input, output_open) = (s.code.clone(), s.output_open);
        let lines = s.terminal.clone().and_then(|id| self.terminals.sessions.term(&id).map(|t| {
            let (f, cells) = t.frame();
            accounts::screen_lines(cells, f.cols)
        })).unwrap_or_default();
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
            .child(div().flex_none().font_weight(FontWeight::NORMAL).text_size(px(12.)).text_color(TEXT_3).child(self.machine_name(&host)));
        let device = device.map(|code| {
            let copy = code.clone();
            div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(div().font_family(MONO).text_size(px(20.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(code))
                .child(action(id("copy-code"), "Copy code").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())))))
        });
        let links = link.map(|link| {
            let copy = link.clone();
            [
                action(id("open"), "Open again").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_url(&link))),
                action(id("copy"), "Copy link").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())))),
            ]
        });
        // Codex signs in with a code typed into the browser, so only Claude can be handed one back.
        let paste = (provider == "claude").then(|| field_box().w(px(230.)).child(div().flex_1().min_w_0().child(Input::new(&code_input).appearance(false).p_0().font_family(MONO).text_size(px(12.)))));
        let cancel = {
            let (host, provider) = (host.clone(), provider.clone());
            action(id("cancel"), "Cancel").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.cancel_sign_in(&host, &provider, cx)))
        };
        let actions = div().flex().items_center().gap(px(6.)).children(links.into_iter().flatten()).children(paste).child(div().flex_1()).child(cancel);
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
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if let Some(s) = this.accounts.signins.get_mut(i) {
                    s.output_open = !s.output_open;
                }
                cx.notify();
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
        div().pl(px(46.)).pr(px(14.)).py(px(12.)).flex().flex_col().gap(px(10.)).child(status).children(device).child(actions).child(div().flex().child(toggle)).children(output)
    }
}
