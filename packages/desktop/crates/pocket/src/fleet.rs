//! SSH hosts: a link to each one's pocketd, and the Settings form that adds one.

use crate::desktop::Desktop;
use futures::StreamExt;
use futures::channel::mpsc::{self, UnboundedSender};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use daemon::{Daemon, Endpoint};
use hosts::Host;
use hosts::checklist::{Checklist, Outcome};
use hosts::install::{self, Platform, Progress, Step};
use hosts::link::{self, Link, Paths, Supervisor};
use hosts::remote::{Client, HostInfo};
use hosts::ssh::Failure;
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use std::time::{Duration, Instant};

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

/// Whether the host's pocketd is older than the one this app ships; one that didn't say its version is left alone.
/// A version that isn't numbers, as a dev build's, only has to differ.
pub(crate) fn needs_update(link: Option<&Link>, app: &str) -> bool {
    let Some(Link::Online { version, .. }) = link else { return false };
    let numbers = |v: &str| -> Option<Vec<u64>> { v.split('-').next()?.split('.').map(|n| n.parse().ok()).collect() };
    match (numbers(version), numbers(app)) {
        _ if version.is_empty() || version == app => false,
        // A pre-release comes before its release.
        (Some(host), Some(ours)) => host < ours || (host == ours && version.contains('-')),
        _ => true,
    }
}

/// What a host's pocketd answering `status` asks of this Mac.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Status {
    /// Serve its folders and terminals here: its link is still up and it can.
    pub serve: bool,
    /// Its owner token isn't the one its open session and agents channel send, as after a re-pair.
    pub reopen: bool,
}

/// What `info` from a host whose link is `online` asks; `opened_with` is the token its session was opened with, if one is.
pub(crate) fn on_status(info: &HostInfo, online: bool, opened_with: Option<&Option<String>>, token: &Option<String>) -> Status {
    let serve = online && info.serves_projects();
    Status { serve, reopen: serve && opened_with.is_some_and(|was| was != token) }
}

/// What an install off the UI thread reports.
enum Report {
    Windows(bool),
    Progress(Progress),
}

/// The host to install on once `platform` is known, and where its pocketd comes from.
fn install_plan(host: &Host, platform: Platform, version: &str, dist: Option<&Path>) -> Result<(Host, install::Source), String> {
    let source = install::source(platform, version, dist)?;
    Ok((Host { windows: matches!(platform, Platform::Windows { .. }), ..host.clone() }, source))
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
    /// Each host's pocketd messages flowing into the window; dropping one lets its connection go.
    sessions: HashMap<String, Task<()>>,
    pub form: AddForm,
    /// Installs and updates under way, or ended and not yet dismissed.
    pub installs: HashMap<String, Checklist>,
    /// The owner token each host's session and agents channel were opened with.
    tokens: HashMap<String, Option<String>>,
    /// What a host's row says instead of its link, as while its service is uninstalled.
    pub notices: HashMap<String, String>,
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
        let fleet = Self {
            links: Links::default(),
            supervisors: HashMap::new(),
            events: None,
            sessions: HashMap::new(),
            form,
            installs: HashMap::new(),
            tokens: HashMap::new(),
            notices: HashMap::new(),
            password,
            pending: Pending::default(),
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
        self.tokens.remove(id);
        self.notices.remove(id);
        if self.password.host.as_deref() == Some(id) {
            self.password.host = None;
        }
        self.pending.forget(id);
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
                    let password = d.fleet.pending.take_for(&id, &link);
                    if d.fleet.links.apply(&id, link) {
                        match (was, d.fleet.links.online(&id)) {
                            (false, true) => d.host_online(&id, window, cx),
                            (true, false) => hosts::remote::forget(&id),
                            _ => {}
                        }
                        cx.notify();
                    }
                    if let Some(password) = password {
                        d.sign_in(id, password, cx);
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
        let Some(host) = self.store.hosts.iter().find(|h| h.id == id).cloned() else { return };
        let home = self.store.home().to_path_buf();
        let id = id.to_string();
        cx.spawn_in(window, async move |this, cx| {
            let (client, endpoint, info) = cx
                .background_executor()
                .spawn(async move {
                    let endpoint = host.endpoint(&home);
                    let client = Arc::new(Client::to(&host.id, endpoint.clone()));
                    let info = client.status();
                    (client, endpoint, info)
                })
                .await;
            this.update_in(cx, |d, window, cx| {
                let Ok(info) = info else { return };
                if info.os == "windows" {
                    d.host_is_windows(&id, true, cx);
                }
                let status = on_status(&info, d.fleet.links.online(&id), d.fleet.tokens.get(&id), &endpoint.token);
                if !status.serve {
                    return;
                }
                if status.reopen {
                    d.daemon.remove(&id);
                    d.close_host_agents(&id);
                }
                d.fleet.tokens.insert(id.clone(), endpoint.token.clone());
                hosts::remote::register(client);
                d.open_host_agents(&id, &endpoint, info.user_home, window, cx);
                d.check_accounts(Some(id.clone()), true, cx);
                if !d.daemon.has(&id) {
                    d.open_host_session(id, endpoint, window, cx);
                }
                cx.notify();
            })
        })
        .detach();
    }

    /// Talks to the host's pocketd for as long as the host stays added; its own reconnects ride out the link's.
    fn open_host_session(&mut self, id: String, endpoint: Endpoint, window: &mut Window, cx: &mut Context<Self>) {
        let (daemon, mut rx) = Daemon::spawn_to(endpoint);
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

    /// Saves what the host runs, once known from its pocketd or an install's probe.
    fn host_is_windows(&mut self, id: &str, windows: bool, cx: &mut Context<Self>) {
        if let Some(host) = self.store.hosts.iter_mut().find(|h| h.id == id && h.windows != windows) {
            host.windows = windows;
            self.save_soon(cx);
        }
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
        let password = form.password.read(cx).value().to_string();
        let added = hosts::target(&address, &port, &user, form.key_file.as_deref(), &form.suggestions)
            .map_err(String::from)
            .and_then(|target| new_host(&self.store.hosts, target));
        match added {
            Ok(host) => {
                self.store.hosts.push(host.clone());
                self.fleet.connect(host.clone(), self.store.home());
                // A password signs in first; until then ssh can't reach the host to install.
                match password.is_empty() {
                    true => self.install_host(&host.id, false, cx),
                    false => self.fleet.pending.hold(host.id.clone(), password),
                }
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
                let _ = tx.unbounded_send(Report::Progress(p));
            };
            // Finding the platform and uploading the archive come before bootstrap.sh says it checks the host.
            send(Progress::Step(Step::Detect));
            let ran = install::probe(&host, &paths).and_then(|platform| {
                let (host, source) = install_plan(&host, platform, channel::version(), install::dist_dir().as_deref()).map_err(install::Error::Failed)?;
                let _ = tx.unbounded_send(Report::Windows(host.windows));
                if update { install::update(&host, &paths, &source, send) } else { install::install(&host, &paths, &source, send) }
            });
            if let Err(e) = ran {
                send(Progress::Error(e.message(&host.name)));
            }
        });
        let id = id.to_string();
        cx.spawn(async move |this, cx| {
            while let Some(report) = rx.next().await {
                let shown = this.update(cx, |d, cx| {
                    match report {
                        Report::Windows(windows) => d.host_is_windows(&id, windows, cx),
                        Report::Progress(progress) => d.fleet.install_progress(&id, progress),
                    }
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
            let signed_in = cx.background_executor().spawn(async move { hosts::sign_in::with_password(&host, &home, &password) }).await;
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

    fn forget_host(&mut self, id: &str, cx: &mut Context<Self>) {
        self.fleet.remove(id);
        self.daemon.remove(id);
        self.close_host_agents(id);
        hosts::remote::forget(id);
        let left = self.terminals.lists.forget(id);
        self.terminals.listed(left);
        if let Some(at) = self.store.hosts.iter().position(|h| h.id == id) {
            let host = self.store.hosts.remove(at);
            let home = self.store.home().to_path_buf();
            cx.background_executor()
                .spawn(async move {
                    let _ = std::fs::remove_file(host.known_hosts(&home));
                    let _ = std::fs::remove_file(host.owner_file(&home));
                    hosts::sign_in::remove_key(&host, &home);
                })
                .detach();
        }
        self.save_soon(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Link, Links, Pending, Platform, Removing, Status, Tries, install_plan, needs_update, new_host, on_status, removal, sign_in_error};
    use hosts::remote::HostInfo;
    use std::time::{Duration, Instant};
    use hosts::ssh::Failure;
    use hosts::{Host, Target};

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

    #[test]
    fn an_update_is_offered_only_when_the_hosts_pocketd_is_older() {
        let online = |version: &str| Link::Online { version: version.into(), uptime: 0 };
        assert!(needs_update(Some(&online("1.4.2")), "1.5.0"));
        assert!(needs_update(Some(&online("1.4.10")), "1.5.0"));
        assert!(!needs_update(Some(&online("1.10.0")), "1.9.0"), "a newer host is never downgraded");
        assert!(needs_update(Some(&online("1.5.0-rc.1")), "1.5.0"));
        assert!(needs_update(Some(&online("0.0.0-e2e")), "dev"), "a dev build only has to differ");
        assert!(!needs_update(Some(&online("1.5.0")), "1.5.0"));
        assert!(!needs_update(Some(&online("")), "1.5.0"), "an old pocketd that didn't say");
        assert!(!needs_update(Some(&Link::Retrying(Failure::NotRunning)), "1.5.0"));
        assert!(!needs_update(None, "1.5.0"));
    }

    #[test]
    fn a_host_is_served_only_while_its_link_is_up_and_its_pocketd_holds_projects() {
        let info = HostInfo { caps: vec!["exec.v1".into(), "fs.v1".into()], ..HostInfo::default() };
        assert_eq!(on_status(&info, true, None, &None), Status { serve: true, reopen: false });
        assert_eq!(on_status(&info, false, None, &None), Status::default(), "the link dropped while status was asked");
        assert_eq!(on_status(&HostInfo::default(), true, None, &None), Status::default(), "an old pocketd only runs terminals");
    }

    #[test]
    fn a_re_paired_hosts_session_reopens_with_its_new_token() {
        let info = HostInfo { caps: vec!["exec.v1".into(), "fs.v1".into()], ..HostInfo::default() };
        let (old, new) = (Some("t1".to_string()), Some("t2".to_string()));
        assert!(on_status(&info, true, Some(&old), &new).reopen);
        assert!(!on_status(&info, true, Some(&old), &old).reopen);
    }

    #[test]
    fn an_install_runs_as_the_platform_the_probe_found() {
        let host = Host::new(Target::Alias { name: "pc".into() }, &[]);
        let dist = std::env::temp_dir().join(format!("fleet-plan-{}", std::process::id()));
        std::fs::create_dir_all(&dist).unwrap();
        std::fs::write(dist.join("pocketd-windows-amd64.zip"), b"").unwrap();
        let (on, source) = install_plan(&host, Platform::Windows { arch: "amd64" }, "dev", Some(&dist)).unwrap();
        assert!(on.windows && source == hosts::install::Source::Archive(dist.join("pocketd-windows-amd64.zip")));
        let (on, _) = install_plan(&Host { windows: true, ..host }, Platform::Unix { os: "linux", arch: "arm64" }, "1.2.0", None).unwrap();
        assert!(!on.windows);
        std::fs::remove_dir_all(dist).unwrap();
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
