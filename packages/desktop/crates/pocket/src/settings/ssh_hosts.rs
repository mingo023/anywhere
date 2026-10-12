use super::catalog::Setting;
use super::host::{service_line, uptime};
use super::{field_box, row};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use hosts::Host;
use hosts::link::Link;
use hosts::ssh::Failure;
use theme::*;

pub(super) const ROWS: &[Setting] = &[
    Setting::custom("hosts", "Hosts", "Hosts", |d, _, cx| d.host_rows(cx)),
    Setting::custom("add-host", "Hosts", "Add a host", |d, s, cx| d.add_host_row(s, cx)).hint("A machine with Anywhere installed that you can reach over SSH"),
];

/// What a host's row says about its link.
fn link_line(link: Option<&Link>, name: &str) -> String {
    match link {
        None => "Not connected".into(),
        Some(Link::Connecting) => "Connecting\u{2026}".into(),
        // pocketd answered without its status.
        Some(Link::Online { version, .. }) if version.is_empty() => "Connected".into(),
        Some(Link::Online { version, uptime: up }) => format!("Connected \u{b7} version {version} \u{b7} up {}", uptime(*up)),
        // Its message already says it tries again.
        Some(Link::Retrying(Failure::Throttled)) => Failure::Throttled.message(name),
        Some(Link::Retrying(f)) => format!("{} Trying again\u{2026}", f.message(name)),
        Some(Link::Failed(f)) => f.message(name),
    }
}

/// A host row's second line: where it goes, unless its name says so, and how its link is doing.
fn host_line(host: &Host, link: Option<&Link>) -> String {
    let address = host.address();
    if address == host.name { link_line(link, &host.name) } else { format!("{address} \u{b7} {}", link_line(link, &host.name)) }
}

fn link_color(link: Option<&Link>) -> Token {
    match link {
        Some(Link::Online { .. }) => SUCCESS,
        Some(Link::Failed(_)) => FAILED,
        Some(Link::Connecting | Link::Retrying(_)) => WAITING_DOT,
        None => TEXT_4,
    }
}

fn host_row(i: usize, tile: &'static str, name: String, line: String, dot: Token) -> Div {
    let tile = div().size(px(28.)).flex().flex_none().items_center().justify_center().rounded(px(7.)).bg(FILL_2).child(icon(tile, 14., TEXT_2));
    let line = div().flex().items_start().gap(px(6.)).text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(div().pt(px(4.5)).child(ui::dot(7., dot))).child(div().flex_1().min_w_0().child(line));
    let text = div().flex_1().min_w_0().flex().flex_col().gap(px(1.)).child(div().truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(name)).child(line);
    div().px(px(14.)).py(px(9.)).flex().items_center().gap(px(12.)).when(i > 0, |r| r.border_t(px(0.5)).border_color(SEPARATOR)).child(tile).child(text)
}

fn field(label: &'static str, input: &Entity<InputState>) -> Div {
    let input = field_box().w_full().child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(input).appearance(false).p_0().text_size(px(12.))));
    div().flex().flex_col().gap(px(5.)).child(div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(label)).child(input)
}

impl Desktop {
    fn host_rows(&mut self, cx: &mut Context<Self>) -> Div {
        let connected = !self.terminals.link.is_down();
        let here = host_row(0, "laptop", "This Mac".into(), service_line(connected, self.settings.host.status.as_ref()), if connected { SUCCESS } else { TEXT_4 })
            .child(div().flex_none().text_size(px(12.5)).text_color(TEXT_3).child("Here"));
        let remotes = self.store.hosts.clone().into_iter().enumerate().map(|(i, h)| {
            let link = self.fleet.links.get(&h.id);
            let line = host_line(&h, link);
            let typing = self.fleet.password.host.as_ref() == Some(&h.id);
            // A reconnect while a password sign-in runs would spend sshd's penalty budget twice.
            let reconnect = link.filter(|l| !matches!(l, Link::Connecting) && !(typing && self.fleet.password.signing_in)).map(|_| {
                let id = h.id.clone();
                super::action(("host-reconnect", i), "Reconnect").on_click(cx.listener(move |this, _: &ClickEvent, _, _| this.fleet.retry(&id)))
            });
            let password = (link == Some(&Link::Failed(Failure::Auth)) && !typing).then(|| {
                let id = h.id.clone();
                super::action(("host-password", i), "Use password").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_password(id.clone(), window, cx)))
            });
            let (id, name) = (h.id.clone(), h.name.clone());
            let remove = ui::button(("host-remove", i), ui::Variant::Danger, None, "Remove").h(px(26.)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                (this.confirm, this.overlay) = (Some(Confirm::RemoveHost { id: id.clone(), name: name.clone() }), Some(Overlay::Confirm));
                cx.notify();
            }));
            let row = host_row(i + 1, "server", h.name.clone(), line, link_color(link)).children(password).children(reconnect).child(remove);
            div().child(row).when(typing, |d| d.child(self.password_form(&h.name, cx)))
        });
        div().flex().flex_col().child(here).children(remotes.collect::<Vec<_>>())
    }

    /// Signs in once with a password to put the app's own key on a host that refused the user's keys.
    fn password_form(&self, name: &str, cx: &mut Context<Self>) -> Div {
        let form = &self.fleet.password;
        let input = field_box().child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(&form.input).appearance(false).p_0().text_size(px(12.))));
        let cancel = ui::button("host-password-cancel", ui::Variant::Secondary, None, "Cancel").h(px(26.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_password(window, cx)));
        let sign_in = match form.signing_in {
            true => ui::button("host-sign-in", ui::Variant::Primary, None, "Signing in\u{2026}").h(px(26.)).opacity(0.6),
            false => ui::button("host-sign-in", ui::Variant::Primary, None, "Sign in").h(px(26.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.sign_in_with_password(window, cx))),
        };
        let note = format!("Used once to add a key for this app to {name}\u{2019}s ~/.ssh/authorized_keys. The password isn\u{2019}t saved.");
        let error = form.error.clone().map(|e| div().text_size(px(12.)).text_color(FAILED_TEXT).child(e));
        // Lines up under the host's name, past its 28px tile and the 12px gap.
        div()
            .pl(px(54.))
            .pr(px(14.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().flex().items_center().gap(px(8.)).child(input).child(div().flex_1()).child(cancel).child(sign_in))
            .children(error)
            .child(div().text_size(px(12.)).text_color(TEXT_3).child(note))
    }

    fn add_host_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let form = &self.fleet.form;
        if !form.open {
            let add = ui::button("host-add", ui::Variant::Primary, Some("plus"), "Add host").h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_add_host(window, cx)));
            return row(setting.label, setting.hint, add);
        }
        let chips = form.suggestions.iter().enumerate().map(|(i, alias)| {
            let on = form.picked.as_ref() == Some(alias);
            let picked = alias.clone();
            div()
                .id(("host-suggest", i))
                .h(px(24.))
                .px(px(10.))
                .flex()
                .items_center()
                .rounded(px(12.))
                .cursor_pointer()
                .text_size(px(12.))
                .font_weight(FontWeight::MEDIUM)
                .when(on, |d| d.bg(ACCENT).text_color(ON_TEXT))
                .when(!on, |d| d.bg(FILL_2).text_color(TEXT_BODY).hover(|d| d.bg(FILL_4)))
                .child(alias.clone())
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.suggest_host(picked.clone(), window, cx)))
        });
        let suggested = (!form.suggestions.is_empty()).then(|| {
            div().flex().flex_wrap().items_center().gap(px(6.)).child(div().mr(px(4.)).text_size(px(12.)).text_color(TEXT_3).child("Suggested")).children(chips.collect::<Vec<_>>())
        });
        let key = match &form.key_file {
            Some(path) => div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .text_color(TEXT_2)
                .child("Signs in with")
                .child(div().font_family(MONO).text_color(TEXT).child(path.clone()))
                .child(ui::icon_button_sized("host-key-clear", "x", 20., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.fleet.form.key_file = None;
                    cx.notify();
                }))),
            None => div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .text_color(TEXT_3)
                .child("Signs in with your SSH keys and ~/.ssh/config.")
                .child(ui::link("host-key-file", "Use a key file\u{2026}").text_size(px(12.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.pick_key_file(window, cx)))),
        };
        let error = form.error.clone().map(|e| div().text_size(px(12.)).text_color(FAILED_TEXT).child(e));
        let cancel = ui::button("host-cancel", ui::Variant::Secondary, None, "Cancel").h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_add_host(window, cx)));
        let connect = ui::button("host-connect", ui::Variant::Primary, None, "Connect").h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.add_host(window, cx)));
        let foot = div().pt(px(2.)).flex().items_center().gap(px(8.)).child(div().flex_1().text_size(px(12.)).text_color(TEXT_3).child("Nothing is changed in your ~/.ssh")).child(cancel).child(connect);
        div()
            .px(px(14.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(setting.label))
            .children(suggested)
            .map(|d| match form.picked {
                Some(_) => d.child(div().flex().gap(px(10.)).child(field("Host", &form.address).flex_1()).child(field("Password", &form.password).flex_1())).child(
                    div().text_size(px(12.)).text_color(TEXT_3).child("From your ~/.ssh/config: its user, port, key and jump host are used as they are."),
                ),
                None => d
                    .child(div().flex().gap(px(10.)).child(field("Address", &form.address).flex_1()).child(field("Port", &form.port).w(px(80.))))
                    .child(div().flex().gap(px(10.)).child(field("Username", &form.user).flex_1()).child(field("Password", &form.password).flex_1()))
                    .child(key),
            })
            .children(error)
            .child(foot)
    }
}

#[cfg(test)]
mod tests {
    use super::{Failure, Host, Link, host_line, link_line};
    use hosts::Target;

    #[test]
    fn a_host_row_says_how_its_link_is_doing() {
        assert_eq!(link_line(Some(&Link::Online { version: "1.4.2".into(), uptime: 3 * 86_400 }), "devbox"), "Connected \u{b7} version 1.4.2 \u{b7} up 3d");
        assert_eq!(link_line(Some(&Link::Retrying(Failure::Dropped)), "devbox"), "Lost the connection to devbox. Trying again\u{2026}");
        assert_eq!(link_line(Some(&Link::Failed(Failure::Auth)), "devbox"), Failure::Auth.message("devbox"));
        assert_eq!(link_line(None, "devbox"), "Not connected");
    }

    #[test]
    fn a_host_row_names_the_address_only_when_its_name_doesnt() {
        let alias = Host::new(Target::Alias { name: "devbox".into() }, &[]);
        assert_eq!(host_line(&alias, None), "Not connected");
        let ip = Host::new(Target::Address { addr: "10.0.4.12".into(), port: 2222, user: "mingo".into(), key_file: None }, &[]);
        assert_eq!(host_line(&ip, None), "mingo@10.0.4.12:2222 \u{b7} Not connected");
    }
}
