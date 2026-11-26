use super::catalog::Setting;
use super::dropdown;
use super::fields::{filter_field, filter_matches};
use super::projects::FILTER_ABOVE;
use super::host::{service_line, uptime};
use super::side::HostRow;
use super::{HostsAct, Settings, SettingsEvent, field_box, row};
use crate::desktop::chrome::Confirm;
use crate::fleet::needs_update;
use crate::spaces::Space;
use crate::spaces::header::tile;
use crate::util::LOCAL_ICON;
use agents::hosts::{Failure, Link};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use hosts::checklist::{self, Checklist, Mark, Outcome};
use hosts::{Host, failure_message};
use theme::*;

pub(super) const ROWS: &[Setting] = &[
    Setting::custom("hosts", "Hosts", "Hosts", |d, _, cx| d.host_rows(cx)),
    Setting::custom("add-host", "Hosts", "Add a host", |d, s, cx| d.add_host_row(s, cx)).hint("A Mac, Linux or Windows machine you can reach over SSH"),
];

/// The menu that holds a host's actions.
pub(super) const HOST_MENU: &str = "host-actions";

/// What a host's row can do, most relevant first: the first is the row's button, all of them are in its menu.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum HostAction {
    Update,
    Open,
    UsePassword,
    Install,
    Reconnect,
}

impl HostAction {
    fn label(self) -> &'static str {
        match self {
            Self::Update => "Update\u{2026}",
            Self::Open => "Open",
            Self::UsePassword => "Use password",
            Self::Install => "Install",
            Self::Reconnect => "Reconnect",
        }
    }
}

/// What decides a host's actions.
pub(super) struct Facts<'a> {
    pub link: Option<&'a Link>,
    /// An install or update's steps are showing.
    pub installing: bool,
    pub needs_update: bool,
    /// The password form is open for this host.
    pub typing_password: bool,
    pub signing_in: bool,
}

/// The actions a host offers now, most relevant first; Remove is always in the menu and isn't listed.
pub(super) fn actions(f: &Facts) -> Vec<HostAction> {
    let not_running = matches!(f.link, Some(Link::Retrying(Failure::NotRunning) | Link::Failed(Failure::NotRunning)));
    // A reconnect while a password sign-in runs would spend sshd's penalty budget twice.
    let reconnect = f.link.is_some_and(|l| !matches!(l, Link::Connecting)) && !f.signing_in;
    [
        (f.needs_update && !f.installing, HostAction::Update),
        (matches!(f.link, Some(Link::Online { .. })), HostAction::Open),
        (f.link == Some(&Link::Failed(Failure::Auth)) && !f.typing_password, HostAction::UsePassword),
        (not_running && !f.installing, HostAction::Install),
        (reconnect, HostAction::Reconnect),
    ]
    .into_iter()
    .filter_map(|(on, a)| on.then_some(a))
    .collect()
}

/// The host an action is for.
#[derive(Clone)]
pub(super) struct Target {
    id: String,
    name: String,
    windows: bool,
}

/// What a host's row says while an older pocketd, which doesn't link hosts, runs here.
const UPDATE_SERVICE: &str = "Update Anywhere\u{2019}s background service to manage hosts";

/// What a host's row says about its link.
fn link_line(link: Option<&Link>, name: &str) -> String {
    match link {
        None => "Not connected".into(),
        Some(Link::Connecting) => "Connecting\u{2026}".into(),
        // pocketd answered without its status.
        Some(Link::Online { version, .. }) if version.is_empty() => "Connected".into(),
        Some(Link::Online { version, uptime: up }) => format!("Connected \u{b7} version {version} \u{b7} up {}", uptime(*up)),
        // Its message already says it tries again.
        Some(Link::Retrying(Failure::Throttled)) => failure_message(Failure::Throttled, name),
        Some(Link::Retrying(f)) => format!("{} Trying again\u{2026}", failure_message(*f, name)),
        Some(Link::Failed(f)) => failure_message(*f, name),
        Some(Link::Unknown) => "Status unknown".into(),
    }
}

/// What the install's panel is headed: for a finished install, that the host is ready.
fn checklist_title(list: &Checklist, name: &str) -> String {
    match (&list.outcome, list.update) {
        (None, false) => "Installing pocketd".into(),
        (None, true) => "Updating pocketd".into(),
        (Some(Outcome::Failed(_)), false) => "Couldn't install pocketd".into(),
        (Some(Outcome::Failed(_)), true) => "Couldn't update pocketd".into(),
        (Some(_), false) => format!("{name} is ready."),
        (Some(_), true) => "pocketd is updated".into(),
    }
}

/// A host row's second line: where it goes, unless the `shown` name says so, and how its link is doing.
fn host_line(host: &Host, shown: &str, link: Option<&Link>) -> String {
    let address = host.address();
    if address == shown { link_line(link, &host.name) } else { format!("{address} \u{b7} {}", link_line(link, &host.name)) }
}

pub(crate) fn link_color(link: Option<&Link>) -> Token {
    match link {
        Some(Link::Online { .. }) => SUCCESS,
        Some(Link::Failed(_)) => FAILED,
        Some(Link::Connecting | Link::Retrying(_)) => WAITING_DOT,
        Some(Link::Unknown) | None => TEXT_4,
    }
}

fn host_row(i: usize, tile: Div, name: String, line: String, dot: Token) -> Div {
    let line = div()
        .flex()
        .items_start()
        .gap(px(6.))
        .text_size(px(12.))
        .line_height(px(16.))
        .text_color(TEXT_3)
        .child(div().pt(px(4.5)).child(ui::dot(7., dot)))
        .child(div().flex_1().min_w_0().child(line));
    let text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(1.))
        .child(div().truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(name))
        .child(line);
    div().px(px(14.)).py(px(9.)).flex().items_center().gap(px(12.)).when(i > 0, |r| r.border_t(px(0.5)).border_color(SEPARATOR)).child(tile).child(text)
}

fn field(label: &'static str, input: &Entity<InputState>) -> Div {
    let input = field_box().w_full().child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(input).appearance(false).p_0().text_size(px(12.))));
    div().flex().flex_col().gap(px(5.)).child(div().text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(label)).child(input)
}

impl Settings {
    fn host_rows(&self, cx: &mut Context<Self>) -> Div {
        let connected = !self.side.link_down;
        let here = host_row(
            0,
            tile(LOCAL_ICON, None, 28.),
            "This Mac".into(),
            service_line(connected, self.host.status.as_ref()),
            if connected { SUCCESS } else { TEXT_4 },
        )
        .child(div().flex_none().text_size(px(12.5)).text_color(TEXT_3).child("Here"));
        if self.side.hosts.read_only {
            // The hosts the desktop linked itself before pocketd did, named as it saved them.
            let saved = self
                .side
                .hosts
                .saved
                .iter()
                .enumerate()
                .map(|(i, name)| host_row(i + 1, tile("server", None, 28.), name.clone(), UPDATE_SERVICE.into(), TEXT_4));
            return div().flex().flex_col().child(here).children(saved.collect::<Vec<_>>());
        }
        let query = self.projects.hosts_filter.read(cx).value().to_string();
        let filtering = self.side.hosts.rows.len() + 1 > FILTER_ABOVE;
        let query = if filtering { query } else { String::new() };
        let show_here = filter_matches(&query, &["This Mac"]);
        let remotes = self.side.hosts.rows.iter().enumerate().filter(|(_, r)| filter_matches(&query, &[r.shown.as_str(), r.host.address().as_str()])).enumerate().map(|(n, (i, HostRow { host: h, link, shown, glyph, colour, install, notice }))| {
            let link = link.as_ref();
            let list = install.as_ref();
            let line = notice.clone().unwrap_or_else(|| host_line(h, shown, link));
            let password_form = self.side.hosts.password.as_ref();
            let typing = password_form.is_some_and(|p| p.host.as_ref() == Some(&h.id));
            let facts = Facts {
                link,
                installing: list.is_some(),
                needs_update: needs_update(link, channel::version()),
                typing_password: typing,
                signing_in: typing && password_form.is_some_and(|p| p.signing_in),
            };
            let todo = actions(&facts);
            let target = Target { id: h.id.clone(), name: shown.clone(), windows: h.windows };
            let button = todo.first().map(|a| {
                let (a, target) = (*a, target.clone());
                super::action(("host-action", i), a.label()).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.run_host_action(a, &target, cx)))
            });
            let more = self.host_menu(i, &h.id, &todo, &target, cx);
            let shown_name = shown.clone();
            let row = host_row(n + usize::from(show_here), tile(glyph, *colour, 28.), shown.clone(), line, link_color(link)).children(button).child(more);
            let panel = list.cloned().map(|list| self.checklist_panel(i, &h.id, &shown_name, &list, cx));
            div().flex().flex_col().child(row).when(typing, |d| d.child(self.password_form(&h.name, cx))).children(panel)
        });
        let filter = filtering.then(|| filter_field(&self.projects.hosts_filter));
        let here = show_here.then_some(here);
        div().flex().flex_col().children(filter).children(here).children(remotes.collect::<Vec<_>>())
    }

    fn run_host_action(&mut self, action: HostAction, t: &Target, cx: &mut Context<Self>) {
        self.menu = None;
        let id = t.id.clone();
        cx.emit(match action {
            HostAction::Update => SettingsEvent::Ask(Confirm::UpdateHost { id, name: t.name.clone(), windows: t.windows }),
            HostAction::Open => SettingsEvent::Hosts(HostsAct::Open(Space::Host(id))),
            HostAction::UsePassword => SettingsEvent::Hosts(HostsAct::OpenPassword(id)),
            HostAction::Install => SettingsEvent::Hosts(HostsAct::Install { id, update: false }),
            HostAction::Reconnect => SettingsEvent::Hosts(HostsAct::Retry(id)),
        });
        cx.notify();
    }

    /// The ⋯ button, and under it the host's every action, then Remove.
    fn host_menu(&self, i: usize, id: &str, todo: &[HostAction], t: &Target, cx: &mut Context<Self>) -> Div {
        let open = self.menu == Some(HOST_MENU) && self.projects.host_menu.as_deref() == Some(id);
        let owner = id.to_string();
        let button = ui::icon_button_sized(("host-more", i), "more", 26., TEXT_3).tab_index(0);
        let button = dropdown::trigger_with(button, open, move |this| (this.menu, this.projects.host_menu) = (Some(HOST_MENU), Some(owner.clone())), cx);
        let menu = open.then(|| {
            let items = todo.iter().enumerate().map(|(n, a)| {
                let (a, t) = (*a, t.clone());
                dropdown::pick_row(("host-action-item", i * 8 + n), false)
                    .child(a.label())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.run_host_action(a, &t, cx)))
            });
            let remove = {
                let (id, name) = (t.id.clone(), t.name.clone());
                dropdown::pick_row(("host-remove", i), false).text_color(FAILED_TEXT).child("Remove\u{2026}").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.menu = None;
                    cx.emit(SettingsEvent::Ask(Confirm::RemoveHost { id: id.clone(), name: name.clone() }));
                    cx.notify();
                }))
            };
            let sep = (!todo.is_empty()).then(|| div().h(px(0.5)).my(px(4.)).mx(px(4.)).bg(SEPARATOR));
            let items: Vec<_> = items.collect();
            ui::menu_in(
                SharedString::from(format!("host-menu-in-{i}")),
                dropdown::shuts(ui::pop(div()), HOST_MENU, cx).min_w(px(160.)).p(px(4.)).flex().flex_col().children(items).children(sep).child(remove),
            )
        });
        div().relative().flex_none().child(button).children(menu.map(|m| ui::dropdown_right(30., m)))
    }

    /// The install's steps under the host's row, with Retry once it failed and Done once pocketd runs.
    fn checklist_panel(&self, i: usize, id: &str, name: &str, list: &Checklist, cx: &mut Context<Self>) -> Div {
        let name = name.to_string();
        let title = checklist_title(list, &name);
        let rows = list.rows().map(|(step, mark)| {
            let lead = match mark {
                Mark::Done => icon("check", 13., SUCCESS).into_any_element(),
                Mark::Active => spinner(("host-step", i * 8 + step as usize), 13., TEXT_3).into_any_element(),
                Mark::Failed => icon("x-bold", 11., FAILED).into_any_element(),
                Mark::Pending | Mark::Skipped => div().size(px(5.)).rounded_full().bg(TEXT_5).into_any_element(),
            };
            div()
                .flex()
                .items_center()
                .gap(px(8.))
                .h(px(22.))
                .child(div().size(px(14.)).flex().flex_none().items_center().justify_center().child(lead))
                .child(div().text_size(px(12.5)).text_color(if mark == Mark::Pending { TEXT_4 } else { TEXT }).child(checklist::label(step)))
        });
        let warnings = list.warnings.iter().map(|w| div().text_size(px(12.)).text_color(TEXT_3).child(w.clone()));
        let error = match &list.outcome {
            Some(Outcome::Failed(e)) => Some(div().text_size(px(12.)).text_color(FAILED_TEXT).child(e.clone())),
            _ => None,
        };
        let (id, update) = (id.to_string(), list.update);
        let (open, button) = match &list.outcome {
            Some(Outcome::Failed(_)) => (
                None,
                Some(
                    super::action(("host-install-retry", i), "Retry")
                        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Hosts(HostsAct::Install { id: id.clone(), update })))),
                ),
            ),
            Some(_) => {
                let (space, done_id) = (Space::Host(id.clone()), id.clone());
                let open = (!update).then(|| {
                    ui::button(("host-install-open", i), ui::Variant::Primary, None, format!("Open {name}")).h(px(26.)).on_click(cx.listener(
                        move |_, _: &ClickEvent, _, cx| {
                            cx.emit(SettingsEvent::Hosts(HostsAct::DismissInstall(id.clone())));
                            cx.emit(SettingsEvent::Hosts(HostsAct::Open(space.clone())));
                        },
                    ))
                });
                let done = super::action(("host-install-done", i), "Done")
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Hosts(HostsAct::DismissInstall(done_id.clone())))));
                (open, Some(done))
            }
            None => (None, None),
        };
        let head = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(div().flex_1().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(title))
            .children(open)
            .children(button);
        div()
            .ml(px(54.))
            .mr(px(14.))
            .mb(px(10.))
            .px(px(12.))
            .py(px(8.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .rounded(px(8.))
            .bg(FILL_2)
            .child(head)
            .children(rows.collect::<Vec<_>>())
            .children(warnings)
            .children(error)
    }

    /// Signs in once with a password to put the app's own key on a host that refused the user's keys.
    fn password_form(&self, name: &str, cx: &mut Context<Self>) -> Div {
        let Some(form) = &self.side.hosts.password else { return div() };
        let input = field_box().child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(&form.input).appearance(false).p_0().text_size(px(12.))));
        let cancel = ui::button("host-password-cancel", ui::Variant::Secondary, None, "Cancel")
            .h(px(26.))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Hosts(HostsAct::ClosePassword))));
        let sign_in = match form.signing_in {
            true => ui::button("host-sign-in", ui::Variant::Primary, None, "Signing in\u{2026}").h(px(26.)).opacity(0.6),
            false => ui::button("host-sign-in", ui::Variant::Primary, None, "Sign in")
                .h(px(26.))
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Hosts(HostsAct::SignInWithPassword)))),
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
        let Some(form) = &self.side.hosts.form else { return div() };
        if self.side.hosts.read_only {
            return row(setting.label, Some(UPDATE_SERVICE), div());
        }
        if !form.open {
            let add = ui::button("host-add", ui::Variant::Primary, Some("plus"), "Add host")
                .h(px(28.))
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Hosts(HostsAct::OpenAdd))));
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
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Hosts(HostsAct::Suggest(picked.clone())))))
        });
        let suggested = (!form.suggestions.is_empty()).then(|| {
            div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.))
                .child(div().mr(px(4.)).text_size(px(12.)).text_color(TEXT_3).child("Suggested"))
                .children(chips.collect::<Vec<_>>())
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
                .child(
                    ui::icon_button_sized("host-key-clear", "x", 20., TEXT_3)
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Hosts(HostsAct::ClearKeyFile)))),
                ),
            None => div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .text_color(TEXT_3)
                .child("Signs in with your SSH keys and ~/.ssh/config.")
                .child(
                    ui::link("host-key-file", "Use a key file\u{2026}")
                        .text_size(px(12.))
                        .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Hosts(HostsAct::PickKeyFile)))),
                ),
        };
        let error = form.error.clone().map(|e| div().text_size(px(12.)).text_color(FAILED_TEXT).child(e));
        let cancel = ui::button("host-cancel", ui::Variant::Secondary, None, "Cancel")
            .h(px(28.))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Hosts(HostsAct::CloseAdd))));
        let connect = ui::button("host-connect", ui::Variant::Primary, None, "Connect")
            .h(px(28.))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Hosts(HostsAct::Add))));
        let foot = div()
            .pt(px(2.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().flex_1().text_size(px(12.)).text_color(TEXT_3).child("Nothing is changed in your ~/.ssh"))
            .child(cancel)
            .child(connect);
        div()
            .px(px(14.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(setting.label))
            .children(suggested)
            .map(|d| match form.picked {
                Some(_) => {
                    d.child(div().flex().gap(px(10.)).child(field("Host", &form.address).flex_1()).child(field("Password", &form.password).flex_1())).child(
                        div().text_size(px(12.)).text_color(TEXT_3).child("From your ~/.ssh/config: its user, port, key and jump host are used as they are."),
                    )
                }
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
    use super::{Facts, Failure, Host, HostAction, Link, actions, checklist_title, failure_message, host_line, link_line};
    use agents::hosts::Progress;
    use hosts::Target;
    use hosts::checklist::Checklist;

    #[test]
    fn a_finished_install_says_the_host_is_ready_and_an_update_does_not() {
        let done = |update| {
            let mut list = Checklist::new(update);
            list.apply(Progress::Done);
            list
        };
        assert_eq!(checklist_title(&done(false), "devbox"), "devbox is ready.");
        assert_eq!(checklist_title(&done(true), "devbox"), "pocketd is updated");
        assert_eq!(checklist_title(&Checklist::new(false), "devbox"), "Installing pocketd");
        let mut failed = Checklist::new(false);
        failed.apply(Progress::Error("no sshd".into()));
        assert_eq!(checklist_title(&failed, "devbox"), "Couldn't install pocketd");
    }

    #[test]
    fn a_host_row_says_how_its_link_is_doing() {
        assert_eq!(link_line(Some(&Link::Online { version: "1.4.2".into(), uptime: 3 * 86_400 }), "devbox"), "Connected \u{b7} version 1.4.2 \u{b7} up 3d");
        assert_eq!(link_line(Some(&Link::Retrying(Failure::Dropped)), "devbox"), "Lost the connection to devbox. Trying again\u{2026}");
        assert_eq!(link_line(Some(&Link::Failed(Failure::Auth)), "devbox"), failure_message(Failure::Auth, "devbox"));
        assert_eq!(link_line(None, "devbox"), "Not connected");
        assert_eq!(link_line(Some(&Link::Unknown), "devbox"), "Status unknown");
    }

    #[test]
    fn a_host_row_names_the_address_only_when_its_name_doesnt() {
        let host = |name: &str, target| Host { id: "h".into(), name: name.into(), target, windows: false, socket: Default::default() };
        let alias = host("devbox", Target::Alias { name: "devbox".into() });
        assert_eq!(host_line(&alias, "devbox", None), "Not connected");
        assert_eq!(host_line(&alias, "GPU box", None), "devbox \u{b7} Not connected");
        let ip = host("10.0.4.12", Target::Address { addr: "10.0.4.12".into(), port: 2222, user: "dev".into(), key_file: None });
        assert_eq!(host_line(&ip, "10.0.4.12", None), "dev@10.0.4.12:2222 \u{b7} Not connected");
    }

    fn online() -> Link {
        Link::Online { version: "1.4.2".into(), uptime: 5 }
    }

    fn facts(link: Option<&Link>) -> Facts<'_> {
        Facts { link, installing: false, needs_update: false, typing_password: false, signing_in: false }
    }

    #[test]
    fn a_connected_host_opens_first_and_can_reconnect() {
        assert_eq!(actions(&facts(Some(&online()))), [HostAction::Open, HostAction::Reconnect]);
    }

    #[test]
    fn an_outdated_host_offers_the_update_before_open() {
        assert_eq!(actions(&Facts { needs_update: true, ..facts(Some(&online())) }), [HostAction::Update, HostAction::Open, HostAction::Reconnect]);
    }

    #[test]
    fn a_host_that_refused_the_keys_offers_the_password_first() {
        let failed = Link::Failed(Failure::Auth);
        assert_eq!(actions(&facts(Some(&failed))), [HostAction::UsePassword, HostAction::Reconnect]);
        assert_eq!(actions(&Facts { typing_password: true, ..facts(Some(&failed)) }), [HostAction::Reconnect]);
        assert_eq!(actions(&Facts { typing_password: true, signing_in: true, ..facts(Some(&failed)) }), []);
    }

    #[test]
    fn a_host_without_pocketd_offers_the_install_first_until_its_steps_show() {
        let stopped = Link::Retrying(Failure::NotRunning);
        assert_eq!(actions(&facts(Some(&stopped))), [HostAction::Install, HostAction::Reconnect]);
        assert_eq!(actions(&Facts { installing: true, ..facts(Some(&stopped)) }), [HostAction::Reconnect]);
    }

    #[test]
    fn a_host_still_connecting_or_never_linked_offers_nothing_but_remove() {
        assert_eq!(actions(&facts(Some(&Link::Connecting))), []);
        assert_eq!(actions(&facts(None)), []);
    }

    #[test]
    fn an_update_in_progress_hides_the_update() {
        assert_eq!(actions(&Facts { needs_update: true, installing: true, ..facts(Some(&online())) }), [HostAction::Open, HostAction::Reconnect]);
    }
}
