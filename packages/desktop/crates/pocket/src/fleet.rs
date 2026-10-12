//! SSH hosts: a link to each one's pocketd, and the Settings form that adds one.

use crate::desktop::Desktop;
use futures::StreamExt;
use futures::channel::mpsc::{self, UnboundedSender};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use hosts::Host;
use hosts::link::{self, Link, Paths, Supervisor};
use hosts::ssh::Failure;
use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

/// Each host's link as last reported; a host is listed only while it's in Settings.
#[derive(Default)]
pub(crate) struct Links(pub(crate) HashMap<String, Link>);

impl Links {
    pub fn get(&self, id: &str) -> Option<&Link> {
        self.0.get(id)
    }

    fn start(&mut self, id: &str) {
        self.0.insert(id.into(), Link::Connecting);
    }

    fn remove(&mut self, id: &str) {
        self.0.remove(id);
    }

    /// Records `link`; true when it changed something on screen. A removed host's last words are dropped.
    fn apply(&mut self, id: &str, link: Link) -> bool {
        match self.0.get_mut(id) {
            Some(shown) if *shown != link => {
                *shown = link;
                true
            }
            _ => false,
        }
    }
}

/// `target` as a new host, unless one already goes there.
pub(crate) fn new_host(hosts: &[Host], target: hosts::Target) -> Result<Host, String> {
    match hosts.iter().find(|h| h.target == target) {
        Some(h) => Err(format!("{} is already added", h.name)),
        None => Ok(Host::new(target, hosts)),
    }
}

/// The add form's fields; open while it shows.
pub(crate) struct AddForm {
    pub open: bool,
    pub address: Entity<InputState>,
    pub port: Entity<InputState>,
    pub user: Entity<InputState>,
    /// Tried only if the host refuses the user's keys.
    pub password: Entity<InputState>,
    pub key_file: Option<String>,
    pub error: Option<String>,
    /// The hosts `~/.ssh/config` names, read when the form opens.
    pub suggestions: Vec<String>,
    /// The suggestion that filled the form; ssh's config brings the rest, so only the host shows.
    pub picked: Option<String>,
}

impl AddForm {
    /// Empties the form, then puts `picked` in the address.
    fn fill(&mut self, picked: Option<String>, window: &mut Window, cx: &mut App) {
        (self.key_file, self.error) = (None, None);
        let address = picked.clone().unwrap_or_default();
        self.address.update(cx, |f, cx| f.set_value(address, window, cx));
        for field in [&self.port, &self.user, &self.password] {
            field.update(cx, |f, cx| f.set_value("", window, cx));
        }
        self.picked = picked;
    }
}

/// A password typed in the add form, kept until its host says whether it takes the user's keys.
#[derive(Default)]
pub(crate) struct Pending(Option<(String, String)>);

impl Pending {
    fn hold(&mut self, id: String, password: String) {
        self.0 = Some((id, password));
    }

    fn forget(&mut self, id: &str) {
        self.0.take_if(|(pending, _)| pending == id);
    }

    /// The password to sign in with, once `id`'s keys are refused; dropped once the link settles any other way.
    fn take_for(&mut self, id: &str, link: &Link) -> Option<String> {
        if self.0.as_ref().is_none_or(|(pending, _)| pending != id) || matches!(link, Link::Connecting | Link::Retrying(_)) {
            return None;
        }
        self.0.take().filter(|_| *link == Link::Failed(Failure::Auth)).map(|(_, password)| password)
    }
}

/// Wrong passwords in a row: each spends sshd's penalty budget, which past a few locks out the user's own Terminal too.
#[derive(Default)]
pub(crate) struct Tries {
    wrong: u32,
    locked_at: Option<Instant>,
}

impl Tries {
    const WAIT: Duration = Duration::from_secs(20);

    /// Counts a wrong password; true once it's the second in a row, after which sign-ins wait out `WAIT`.
    fn wrong(&mut self, now: Instant) -> bool {
        self.wrong += 1;
        if self.wrong >= 2 {
            self.locked_at = Some(now);
        }
        self.locked_at.is_some()
    }

    /// Whether a sign-in may start now; once the wait is over the count starts again.
    fn allowed(&mut self, now: Instant) -> bool {
        match self.locked_at {
            Some(at) if now < at + Self::WAIT => false,
            Some(_) => {
                *self = Self::default();
                true
            }
            None => true,
        }
    }
}

/// What the password form says after a sign-in failed; `locked` after too many wrong passwords.
pub(crate) fn sign_in_error(failure: Failure, name: &str, locked: bool) -> String {
    match failure {
        Failure::Auth if locked => format!("Wrong password twice. Wait {} seconds, then try again.", Tries::WAIT.as_secs()),
        Failure::Auth => "Wrong password.".into(),
        // Its usual message promises a retry, which a sign-in never makes.
        Failure::Throttled => format!("{name} is limiting sign-ins after failed attempts. Wait {} seconds, then try again.", Tries::WAIT.as_secs()),
        f => f.message(name),
    }
}

/// The password field under a host whose keys it refused.
pub(crate) struct PasswordForm {
    /// The host it's open for.
    pub host: Option<String>,
    pub input: Entity<InputState>,
    pub error: Option<String>,
    pub signing_in: bool,
    tries: Tries,
}

pub(crate) struct Fleet {
    pub links: Links,
    supervisors: HashMap<String, Supervisor>,
    /// Set once links start; captures never start them.
    events: Option<UnboundedSender<(String, Link)>>,
    pub form: AddForm,
    pub password: PasswordForm,
    pending: Pending,
}

impl Fleet {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let address = cx.new(|cx| InputState::new(window, cx).placeholder("10.0.4.12 or devbox.example.com"));
        let port = cx.new(|cx| InputState::new(window, cx).placeholder("22"));
        let user = cx.new(|cx| InputState::new(window, cx).placeholder(std::env::var("USER").unwrap_or_default()));
        let add_password = cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("Only if it refuses your keys"));
        let password = cx.new(|cx| InputState::new(window, cx).masked(true).placeholder("Password"));
        let mut subs: Vec<_> = [&address, &port, &user, &add_password]
            .into_iter()
            .map(|field| {
                cx.subscribe_in(field, window, |this, _, ev: &InputEvent, window, cx| match ev {
                    InputEvent::PressEnter { .. } => this.add_host(window, cx),
                    InputEvent::Change if this.fleet.form.error.take().is_some() => cx.notify(),
                    _ => {}
                })
            })
            .collect();
        subs.push(cx.subscribe_in(&password, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { .. } => this.sign_in_with_password(window, cx),
            InputEvent::Change if this.fleet.password.error.take().is_some() => cx.notify(),
            _ => {}
        }));
        let form = AddForm { open: false, address, port, user, password: add_password, key_file: None, error: None, suggestions: Vec::new(), picked: None };
        let password = PasswordForm { host: None, input: password, error: None, signing_in: false, tries: Tries::default() };
        (Self { links: Links::default(), supervisors: HashMap::new(), events: None, form, password, pending: Pending::default() }, subs)
    }

    fn connect(&mut self, host: Host, home: &Path) {
        let Some(events) = &self.events else { return };
        self.links.start(&host.id);
        let paths = Paths { home: home.into(), remote_dir: channel::home_dir_name().into() };
        self.supervisors.insert(host.id.clone(), link::supervise(host, paths, events.clone()));
    }

    pub fn retry(&self, id: &str) {
        if let Some(s) = self.supervisors.get(id) {
            s.retry();
        }
    }

    /// Dropping the supervisor closes the tunnel off the UI thread.
    fn remove(&mut self, id: &str) {
        self.supervisors.remove(id);
        self.links.remove(id);
        if self.password.host.as_deref() == Some(id) {
            self.password.host = None;
        }
        self.pending.forget(id);
    }
}

impl Desktop {
    /// Connects to every saved host, and keeps their links up to date on screen.
    pub(crate) fn connect_hosts(&mut self, cx: &mut Context<Self>) {
        let (tx, mut rx) = mpsc::unbounded();
        self.fleet.events = Some(tx);
        for host in self.store.hosts.clone() {
            self.fleet.connect(host, self.store.home());
        }
        cx.spawn(async move |this, cx| {
            while let Some((id, link)) = rx.next().await {
                let shown = |d: &mut Desktop, cx: &mut Context<Desktop>| {
                    let password = d.fleet.pending.take_for(&id, &link);
                    if d.fleet.links.apply(&id, link) {
                        cx.notify();
                    }
                    if let Some(password) = password {
                        d.sign_in(id, password, cx);
                    }
                };
                if this.update(cx, shown).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn open_add_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fleet.form.open = true;
        self.fleet.form.address.update(cx, |f, cx| f.focus(window, cx));
        cx.spawn(async |this, cx| {
            let home = std::env::var_os("HOME").map(std::path::PathBuf::from).unwrap_or_default();
            let found = cx.background_executor().spawn(async move { hosts::ssh_config::aliases(&home) }).await;
            let _ = this.update(cx, |d, cx| {
                d.fleet.form.suggestions = found;
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn close_add_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.fleet.form.open = false;
        self.fleet.form.fill(None, window, cx);
        cx.notify();
    }

    /// Fills the form with a host from `~/.ssh/config`, which brings its own user, port and key.
    pub(crate) fn suggest_host(&mut self, alias: String, window: &mut Window, cx: &mut Context<Self>) {
        self.fleet.form.fill(Some(alias), window, cx);
        cx.notify();
    }

    pub(crate) fn pick_key_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_path(false, window, cx, |d, paths, _, _| {
            if let Some(path) = paths.into_iter().next() {
                d.fleet.form.key_file = Some(crate::util::tilde(&path.to_string_lossy()));
            }
        });
    }

    pub(crate) fn add_host(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = &self.fleet.form;
        let [address, port, user] = [&form.address, &form.port, &form.user].map(|f| f.read(cx).value().to_string());
        let password = form.password.read(cx).value().to_string();
        let added = hosts::target(&address, &port, &user, form.key_file.as_deref(), &form.suggestions)
            .map_err(String::from)
            .and_then(|target| new_host(&self.store.hosts, target));
        match added {
            Ok(host) => {
                self.store.hosts.push(host.clone());
                if !password.is_empty() {
                    self.fleet.pending.hold(host.id.clone(), password);
                }
                self.fleet.connect(host, self.store.home());
                self.save_soon(cx);
                self.close_add_host(window, cx);
            }
            Err(e) => {
                self.fleet.form.error = Some(e);
                cx.notify();
            }
        }
    }

    pub(crate) fn open_password(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        let form = &mut self.fleet.password;
        (form.host, form.error, form.tries) = (Some(id), None, Tries::default());
        form.input.update(cx, |f, cx| {
            f.set_value("", window, cx);
            f.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn close_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = &mut self.fleet.password;
        (form.host, form.error) = (None, None);
        form.input.update(cx, |f, cx| f.set_value("", window, cx));
        cx.notify();
    }

    pub(crate) fn sign_in_with_password(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let form = &self.fleet.password;
        let password = form.input.read(cx).value().to_string();
        let Some(id) = form.host.clone().filter(|_| !password.is_empty()) else { return };
        form.input.update(cx, |f, cx| f.set_value("", window, cx));
        self.sign_in(id, password, cx);
    }

    /// Puts the app's own key on the host with `password`, then reconnects with that key.
    fn sign_in(&mut self, id: String, password: String, cx: &mut Context<Self>) {
        let Some(host) = self.store.hosts.iter().find(|h| h.id == id).cloned() else { return };
        let form = &mut self.fleet.password;
        if form.host.as_ref() != Some(&id) {
            form.tries = Tries::default();
        }
        // One at a time, and none while sshd's penalty for the last wrong passwords lasts.
        if form.signing_in || !form.tries.allowed(Instant::now()) {
            return;
        }
        (form.host, form.signing_in, form.error) = (Some(id), true, None);
        let (id, name, home) = (host.id.clone(), host.name.clone(), self.store.home().to_path_buf());
        cx.spawn(async move |this, cx| {
            let signed_in = cx.background_executor().spawn(async move { hosts::install::with_password(&host, &home, &password) }).await;
            let _ = this.update(cx, |d, cx| {
                if signed_in.is_ok() {
                    d.fleet.retry(&id);
                }
                let form = &mut d.fleet.password;
                form.signing_in = false;
                // The form may have moved to another host meanwhile.
                if form.host.as_ref() == Some(&id) {
                    match signed_in {
                        Ok(()) => form.host = None,
                        Err(f) => {
                            let locked = f == Failure::Auth && form.tries.wrong(Instant::now());
                            form.error = Some(sign_in_error(f, &name, locked));
                        }
                    }
                }
                cx.notify();
            });
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn remove_host(&mut self, id: &str, cx: &mut Context<Self>) {
        self.fleet.remove(id);
        if let Some(at) = self.store.hosts.iter().position(|h| h.id == id) {
            let host = self.store.hosts.remove(at);
            let home = self.store.home().to_path_buf();
            cx.background_executor()
                .spawn(async move {
                    let _ = std::fs::remove_file(host.known_hosts(&home));
                    hosts::install::remove_key(&host, &home);
                })
                .detach();
        }
        self.save_soon(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Link, Links, Pending, Tries, new_host, sign_in_error};
    use std::time::{Duration, Instant};
    use hosts::Target;
    use hosts::ssh::Failure;

    #[test]
    fn a_link_shows_only_what_changed_for_a_host_still_listed() {
        let mut links = Links::default();
        links.start("devbox");
        assert!(!links.apply("devbox", Link::Connecting));
        assert!(links.apply("devbox", Link::Retrying(Failure::Refused)));
        links.remove("devbox");
        assert!(!links.apply("devbox", Link::Connecting), "a removed host's last report is dropped");
        assert_eq!(links.get("devbox"), None);
    }

    #[test]
    fn a_typed_password_is_used_only_once_its_hosts_keys_are_refused() {
        let pending = |id: &str| Pending(Some((id.into(), "hunter2".into())));
        let mut waiting = pending("devbox");
        assert_eq!(waiting.take_for("devbox", &Link::Retrying(Failure::Throttled)), None);
        assert_eq!(waiting.take_for("gpu", &Link::Failed(Failure::Auth)), None, "another host's refusal");
        assert_eq!(waiting.take_for("devbox", &Link::Failed(Failure::Auth)), Some("hunter2".into()));
        assert_eq!(waiting.take_for("devbox", &Link::Failed(Failure::Auth)), None, "used once");
        let mut keys_worked = pending("devbox");
        assert_eq!(keys_worked.take_for("devbox", &Link::Online { version: "1.4.2".into(), uptime: 0 }), None);
        assert_eq!(keys_worked.0, None, "dropped once the keys work");
    }

    #[test]
    fn a_second_wrong_password_waits_out_sshds_penalty() {
        let (mut tries, now) = (Tries::default(), Instant::now());
        assert!(tries.allowed(now));
        assert!(!tries.wrong(now));
        assert!(tries.allowed(now), "one wrong password leaves another try");
        assert!(tries.wrong(now));
        assert!(!tries.allowed(now + Duration::from_secs(19)));
        assert!(tries.allowed(now + Tries::WAIT));
        assert!(!tries.wrong(now + Tries::WAIT), "the count starts again after the wait");
    }

    #[test]
    fn a_failed_sign_in_never_promises_a_retry_it_wont_make() {
        assert_eq!(sign_in_error(Failure::Auth, "devbox", false), "Wrong password.");
        assert_eq!(sign_in_error(Failure::Auth, "devbox", true), "Wrong password twice. Wait 20 seconds, then try again.");
        assert!(!sign_in_error(Failure::Throttled, "devbox", false).contains("Trying again"));
    }

    #[test]
    fn a_host_already_added_isnt_added_twice() {
        let devbox = Target::Alias { name: "devbox".into() };
        let first = new_host(&[], devbox.clone()).unwrap();
        assert_eq!(new_host(&[first], devbox), Err("devbox is already added".into()));
    }
}
