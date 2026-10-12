//! SSH hosts: a link to each one's pocketd, and the Settings form that adds one.

use crate::desktop::Desktop;
use futures::StreamExt;
use futures::channel::mpsc::{self, UnboundedSender};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use hosts::Host;
use hosts::link::{self, Link, Paths, Supervisor};
use std::collections::HashMap;
use std::path::Path;

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
        for field in [&self.port, &self.user] {
            field.update(cx, |f, cx| f.set_value("", window, cx));
        }
        self.picked = picked;
    }
}

pub(crate) struct Fleet {
    pub links: Links,
    supervisors: HashMap<String, Supervisor>,
    /// Set once links start; captures never start them.
    events: Option<UnboundedSender<(String, Link)>>,
    pub form: AddForm,
}

impl Fleet {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let address = cx.new(|cx| InputState::new(window, cx).placeholder("10.0.4.12 or devbox.example.com"));
        let port = cx.new(|cx| InputState::new(window, cx).placeholder("22"));
        let user = cx.new(|cx| InputState::new(window, cx).placeholder(std::env::var("USER").unwrap_or_default()));
        let subs = [&address, &port, &user]
            .into_iter()
            .map(|field| {
                cx.subscribe_in(field, window, |this, _, ev: &InputEvent, window, cx| match ev {
                    InputEvent::PressEnter { .. } => this.add_host(window, cx),
                    InputEvent::Change if this.fleet.form.error.take().is_some() => cx.notify(),
                    _ => {}
                })
            })
            .collect();
        let form = AddForm { open: false, address, port, user, key_file: None, error: None, suggestions: Vec::new(), picked: None };
        (Self { links: Links::default(), supervisors: HashMap::new(), events: None, form }, subs)
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
                    if d.fleet.links.apply(&id, link) {
                        cx.notify();
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
        let added = hosts::target(&address, &port, &user, form.key_file.as_deref(), &form.suggestions)
            .map_err(String::from)
            .and_then(|target| new_host(&self.store.hosts, target));
        match added {
            Ok(host) => {
                self.store.hosts.push(host.clone());
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

    pub(crate) fn remove_host(&mut self, id: &str, cx: &mut Context<Self>) {
        self.fleet.remove(id);
        if let Some(at) = self.store.hosts.iter().position(|h| h.id == id) {
            let known_hosts = self.store.hosts.remove(at).known_hosts(self.store.home());
            cx.background_executor().spawn(async move { std::fs::remove_file(known_hosts) }).detach();
        }
        self.save_soon(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Link, Links, new_host};
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
    fn a_host_already_added_isnt_added_twice() {
        let devbox = Target::Alias { name: "devbox".into() };
        let first = new_host(&[], devbox.clone()).unwrap();
        assert_eq!(new_host(&[first], devbox), Err("devbox is already added".into()));
    }
}
