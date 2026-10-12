//! SSH hosts: a link to each one's pocketd, and the Settings form that adds one.

use crate::desktop::Desktop;
use futures::StreamExt;
use futures::channel::mpsc::{self, UnboundedSender};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use daemon::Daemon;
use hosts::Host;
use hosts::checklist::{Checklist, Outcome};
use hosts::install::{self, Progress, Step};
use hosts::link::{self, Link, Paths, Supervisor};
use hosts::remote::Client;
use std::collections::{HashMap, HashSet};
use std::path::Path;
use std::sync::Arc;

/// Each host's link as last reported; a host is listed only while it's in Settings.
#[derive(Default)]
pub(crate) struct Links(pub(crate) HashMap<String, Link>);

impl Links {
    pub fn get(&self, id: &str) -> Option<&Link> {
        self.0.get(id)
    }

    pub fn online(&self, id: &str) -> bool {
        matches!(self.get(id), Some(Link::Online { .. }))
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

/// Whether the host's pocketd isn't the one this app ships; one that didn't say its version is left alone.
pub(crate) fn needs_update(link: Option<&Link>, app: &str) -> bool {
    matches!(link, Some(Link::Online { version, .. }) if !version.is_empty() && version != app)
}

#[derive(Debug, PartialEq)]
pub(crate) enum Removing {
    Uninstall,
    Host,
    Project(String),
}

/// What removing host `id` does, in order. The host is let go before its projects leave the list, so their
/// terminals keep running there; uninstalling comes first and the rest waits for it.
pub(crate) fn removal(id: &str, uninstall: bool, projects: &[String]) -> Vec<Removing> {
    let on_host = projects.iter().filter(|p| hosts::split(p).0 == Some(id)).map(|p| Removing::Project(p.clone()));
    uninstall.then_some(Removing::Uninstall).into_iter().chain([Removing::Host]).chain(on_host).collect()
}

fn paths(home: &Path) -> Paths {
    Paths { home: home.into(), remote_dir: channel::home_dir_name().into() }
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
    /// Each host's pocketd messages flowing into the window; dropping one lets its connection go.
    sessions: HashMap<String, Task<()>>,
    pub form: AddForm,
    /// Installs and updates under way, or ended and not yet dismissed.
    pub installs: HashMap<String, Checklist>,
    /// Hosts whose pocketd runs on Windows, where an update closes terminals.
    pub windows: HashSet<String>,
    /// What a host's row says instead of its link, as while its service is uninstalled.
    pub notices: HashMap<String, String>,
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
        let fleet = Self {
            links: Links::default(),
            supervisors: HashMap::new(),
            events: None,
            sessions: HashMap::new(),
            form,
            installs: HashMap::new(),
            windows: HashSet::new(),
            notices: HashMap::new(),
        };
        (fleet, subs)
    }

    fn connect(&mut self, host: Host, home: &Path) {
        let Some(events) = &self.events else { return };
        self.links.start(&host.id);
        self.supervisors.insert(host.id.clone(), link::supervise(host, paths(home), events.clone()));
    }

    pub fn retry(&self, id: &str) {
        if let Some(s) = self.supervisors.get(id) {
            s.retry();
        }
    }

    /// Records how the install on `id` goes. Once pocketd runs, the link reconnects rather than wait out its
    /// backoff; a pocketd that was already running needs no checklist.
    fn install_progress(&mut self, id: &str, progress: Progress) {
        let Some(list) = self.installs.get_mut(id) else { return };
        let ended = list.outcome.is_some();
        list.apply(progress);
        if ended || !list.succeeded() {
            return;
        }
        if list.outcome == Some(Outcome::Reused) {
            self.installs.remove(id);
        }
        self.retry(id);
    }

    /// Dropping the supervisor closes the tunnel off the UI thread.
    fn remove(&mut self, id: &str) {
        self.supervisors.remove(id);
        self.sessions.remove(id);
        self.links.remove(id);
        self.installs.remove(id);
        self.windows.remove(id);
        self.notices.remove(id);
    }
}

impl Desktop {
    /// Connects to every saved host, and keeps their links up to date on screen.
    pub(crate) fn connect_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (tx, mut rx) = mpsc::unbounded();
        self.fleet.events = Some(tx);
        for host in self.store.hosts.clone() {
            self.fleet.connect(host, self.store.home());
        }
        cx.spawn_in(window, async move |this, cx| {
            while let Some((id, link)) = rx.next().await {
                let shown = |d: &mut Desktop, window: &mut Window, cx: &mut Context<Desktop>| {
                    let was = d.fleet.links.online(&id);
                    if d.fleet.links.apply(&id, link) {
                        match (was, d.fleet.links.online(&id)) {
                            (false, true) => d.host_online(&id, window, cx),
                            (true, false) => hosts::remote::forget(&id),
                            _ => {}
                        }
                        cx.notify();
                    }
                };
                if this.update_in(cx, shown).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Serves the host's folders and terminals here, once its pocketd says it can.
    fn host_online(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.store.hosts.iter().find(|h| h.id == id) else { return };
        let sock = host.socket(self.store.home());
        let client = Arc::new(Client::new(id, sock.clone()));
        let id = id.to_string();
        cx.spawn_in(window, async move |this, cx| {
            let probe = client.clone();
            let info = cx.background_executor().spawn(async move { probe.status() }).await;
            this.update_in(cx, |d, window, cx| {
                let Ok(info) = info else { return };
                if info.os == "windows" {
                    d.fleet.windows.insert(id.clone());
                }
                if !d.fleet.links.online(&id) || !info.serves_projects() {
                    return;
                }
                hosts::remote::register(client);
                d.open_host_agents(&id, &sock, info.user_home, window, cx);
                if !d.daemon.has(&id) {
                    d.open_host_session(id, &sock, window, cx);
                }
                cx.notify();
            })
        })
        .detach();
    }

    /// Talks to the host's pocketd for as long as the host stays added; its own reconnects ride out the link's.
    fn open_host_session(&mut self, id: String, sock: &Path, window: &mut Window, cx: &mut Context<Self>) {
        let (daemon, mut rx) = Daemon::spawn(sock);
        self.daemon.add(&id, daemon);
        let host = id.clone();
        let task = cx.spawn_in(window, async move |this, cx| {
            while let Some(mut m) = rx.next().await {
                hosts::session::tag_msg(&host, &mut m);
                if this.update_in(cx, |d, window, cx| d.on_msg_from(Some(&host), m, window, cx)).is_err() {
                    break;
                }
            }
        });
        self.fleet.sessions.insert(id, task);
    }

    pub(crate) fn host_name(&self, id: &str) -> String {
        self.store.hosts.iter().find(|h| h.id == id).map_or_else(|| id.to_string(), |h| h.name.clone())
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
                self.fleet.connect(host.clone(), self.store.home());
                self.install_host(&host.id, false, cx);
                self.save_soon(cx);
                self.close_add_host(window, cx);
            }
            Err(e) => {
                self.fleet.form.error = Some(e);
                cx.notify();
            }
        }
    }

    /// Puts pocketd on the host, or with `update` replaces it, off the UI thread; its row shows each step.
    pub(crate) fn install_host(&mut self, id: &str, update: bool, cx: &mut Context<Self>) {
        let Some(host) = self.store.hosts.iter().find(|h| h.id == id).cloned() else { return };
        if self.fleet.installs.get(id).is_some_and(|l| l.outcome.is_none()) {
            return;
        }
        self.fleet.installs.insert(host.id.clone(), Checklist::new(update));
        let paths = paths(self.store.home());
        let (tx, mut rx) = mpsc::unbounded();
        // Not the background executor: an install can take minutes.
        std::thread::spawn(move || {
            let send = |p| {
                let _ = tx.unbounded_send(p);
            };
            // Finding the platform and uploading the archive come before bootstrap.sh says it checks the host.
            send(Progress::Step(Step::Detect));
            let ran = install::probe(&host, &paths).and_then(|platform| {
                let source = install::source(platform, channel::version(), install::dist_dir().as_deref()).map_err(install::Error::Failed)?;
                if update { install::update(&host, &paths, &source, send) } else { install::install(&host, &paths, &source, send) }
            });
            if let Err(e) = ran {
                send(Progress::Error(e.message(&host.name)));
            }
        });
        let id = id.to_string();
        cx.spawn(async move |this, cx| {
            while let Some(progress) = rx.next().await {
                let shown = this.update(cx, |d, cx| {
                    d.fleet.install_progress(&id, progress);
                    cx.notify();
                });
                if shown.is_err() {
                    break;
                }
            }
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn remove_host(&mut self, id: &str, uninstall: bool, cx: &mut Context<Self>) {
        for step in removal(id, uninstall, &self.projects()) {
            match step {
                Removing::Uninstall => return self.uninstall_host(id, cx),
                Removing::Host => self.forget_host(id, cx),
                Removing::Project(p) => self.remove_project(&p, cx),
            }
        }
    }

    /// Removes pocketd's service from the host, then the host; a host that can't be reached stays, saying why.
    fn uninstall_host(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(host) = self.store.hosts.iter().find(|h| h.id == id).cloned() else { return };
        let paths = paths(self.store.home());
        self.fleet.notices.insert(host.id.clone(), "Uninstalling pocketd\u{2026}".into());
        cx.spawn(async move |this, cx| {
            let (id, name) = (host.id.clone(), host.name.clone());
            let done = cx.background_executor().spawn(async move { install::uninstall(&host, &paths) }).await;
            let _ = this.update(cx, |d, cx| match done {
                Ok(()) => d.remove_host(&id, false, cx),
                Err(e) => {
                    d.fleet.notices.insert(id, e.message(&name));
                    cx.notify();
                }
            });
        })
        .detach();
        cx.notify();
    }

    fn forget_host(&mut self, id: &str, cx: &mut Context<Self>) {
        self.fleet.remove(id);
        self.daemon.remove(id);
        self.close_host_agents(id);
        hosts::remote::forget(id);
        let left = self.terminals.lists.forget(id);
        self.terminals.listed(left);
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
    use super::{Link, Links, Removing, needs_update, new_host, removal};
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

    #[test]
    fn an_update_is_offered_only_when_the_hosts_pocketd_is_another_version() {
        let online = |version: &str| Link::Online { version: version.into(), uptime: 0 };
        assert!(needs_update(Some(&online("1.4.2")), "1.5.0"));
        assert!(!needs_update(Some(&online("1.5.0")), "1.5.0"));
        assert!(!needs_update(Some(&online("")), "1.5.0"), "an old pocketd that didn't say");
        assert!(!needs_update(Some(&Link::Retrying(Failure::NotRunning)), "1.5.0"));
        assert!(!needs_update(None, "1.5.0"));
    }

    #[test]
    fn removing_from_this_mac_lets_the_host_go_then_its_projects() {
        let projects = ["//devbox/home/me/app".to_string(), "/Users/me/site".into(), "//gpu/home/me/ml".into()];
        assert_eq!(removal("devbox", false, &projects), [Removing::Host, Removing::Project("//devbox/home/me/app".into())]);
        assert_eq!(removal("nas", false, &projects), [Removing::Host]);
    }

    #[test]
    fn uninstalling_removes_the_service_before_anything_else() {
        let projects = ["//devbox/home/me/app".to_string()];
        assert_eq!(removal("devbox", true, &projects), [Removing::Uninstall, Removing::Host, Removing::Project("//devbox/home/me/app".into())]);
    }
}
