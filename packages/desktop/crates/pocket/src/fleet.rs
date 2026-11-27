//! SSH hosts: the ones this Mac's pocketd links, shown and served here, and the Settings form that adds one.

use crate::desktop::Desktop;
use crate::spaces::{Space, flash_key};
use agents::hosts::{Entry, Failure, Link, Progress};
use daemon::{Daemon, Endpoint};
use futures::StreamExt;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use hosts::Host;
use hosts::checklist::{Checklist, Outcome};
use hosts::remote::{Client, HostInfo};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};

const STATUS_RETRY: Duration = Duration::from_secs(2);

/// Each host's link as last reported; a host is listed only while it's in Settings.
#[derive(Default, Clone)]
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

/// Whether `list`, which was still running if `was_open`, just ended in an install: the host is new to the sidebar.
fn just_installed(was_open: bool, list: &Checklist) -> bool {
    was_open && !list.update && list.outcome == Some(Outcome::Installed)
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

/// The hosts pocketd links and how each link is doing, as its events last said.
#[derive(Default)]
pub(crate) struct Hosts {
    pub list: Vec<Host>,
    pub links: Links,
}

#[derive(Debug, PartialEq)]
pub(crate) enum Turn {
    Online(String),
    Offline(String),
    Gone(String),
}

impl Hosts {
    fn replace(&mut self, entries: Vec<Entry>) -> Vec<Turn> {
        let gone: Vec<String> = self.list.iter().filter(|h| !entries.iter().any(|e| e.id == h.id)).map(|h| h.id.clone()).collect();
        for id in &gone {
            self.forget(id);
        }
        gone.into_iter().map(Turn::Gone).chain(entries.into_iter().filter_map(|e| self.update(e))).collect()
    }

    /// Takes one host added or changed.
    fn update(&mut self, entry: Entry) -> Option<Turn> {
        let host = Host::from(&entry);
        match self.list.iter_mut().find(|h| h.id == host.id) {
            Some(shown) => *shown = host,
            None => {
                self.links.start(&host.id);
                self.list.push(host);
            }
        }
        let was = self.links.online(&entry.id);
        self.links.apply(&entry.id, entry.link);
        match (was, self.links.online(&entry.id)) {
            (false, true) => Some(Turn::Online(entry.id)),
            (true, false) => Some(Turn::Offline(entry.id)),
            _ => None,
        }
    }

    fn forget(&mut self, id: &str) {
        self.list.retain(|h| h.id != id);
        self.links.remove(id);
    }
}

/// What a host message sent to pocketd was for, until it answers.
enum Asked {
    /// With the password to sign in with if the host refuses the user's keys.
    Add {
        password: String,
    },
    Install(String),
    Uninstall(String),
    SignIn(String),
    Edit(String),
}

#[derive(Debug, PartialEq)]
pub(crate) enum Removing {
    Uninstall,
    Host,
    Forget,
    Project(String),
}

#[derive(Clone, Copy)]
pub(crate) enum Leave {
    Keep,
    Uninstall,
    Uninstalled,
}

/// What removing host `id` does, in order. The host is let go before its projects leave the list, so their
/// terminals keep running there; uninstalling comes first and the rest waits for it. pocketd lets the host
/// go as it uninstalls, so nothing more is sent for it then.
pub(crate) fn removal(id: &str, leave: Leave, projects: &[String]) -> Vec<Removing> {
    let on_host = projects.iter().filter(|p| hosts::split(p).0 == Some(id)).map(|p| Removing::Project(p.clone()));
    let uninstall = matches!(leave, Leave::Uninstall).then_some(Removing::Uninstall);
    let host = if matches!(leave, Leave::Keep) { Removing::Host } else { Removing::Forget };
    uninstall.into_iter().chain([host]).chain(on_host).collect()
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
    /// The suggestion that filled the form; ssh's config brings what its port and user leave blank.
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
        let [port, user] = placeholders(picked.is_some());
        self.port.update(cx, |f, cx| f.set_placeholder(port, window, cx));
        self.user.update(cx, |f, cx| f.set_placeholder(user, window, cx));
        self.picked = picked;
    }
}

/// What a blank port and username mean: an alias's `~/.ssh/config`, else ssh's port 22 and this Mac's user.
fn placeholders(alias: bool) -> [String; 2] {
    match alias {
        true => ["Default".into(), "From ~/.ssh/config".into()],
        false => ["22".into(), std::env::var("USER").unwrap_or_default()],
    }
}

/// The fields under a host's row that change how ssh reaches it; open while `host` is set.
pub(crate) struct EditForm {
    pub host: Option<String>,
    pub address: Entity<InputState>,
    pub port: Entity<InputState>,
    pub user: Entity<InputState>,
    pub error: Option<String>,
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
        f => hosts::failure_message(f, name),
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
    pub hosts: Hosts,
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
    pub edit: EditForm,
    /// Hosts whose update can't run from here, as one whose pocketd the Anywhere app there owns.
    pub stuck: HashSet<String>,
    pending: Pending,
    /// Host messages sent and not yet answered, by request id.
    asked: HashMap<String, Asked>,
    /// Each host's next `status` ask after one failed; a newer one replaces it, so one runs per host.
    status_retries: HashMap<String, Task<()>>,
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
                cx.subscribe_in(field, window, |this, _, ev: &InputEvent, _, cx| match ev {
                    InputEvent::PressEnter { .. } => this.add_host(cx),
                    InputEvent::Change if this.fleet.form.error.take().is_some() => cx.notify(),
                    _ => {}
                })
            })
            .collect();
        let [edit_address, edit_port, edit_user] = [(); 3].map(|_| cx.new(|cx| InputState::new(window, cx)));
        for field in [&edit_address, &edit_port, &edit_user] {
            subs.push(cx.subscribe_in(field, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { .. } => this.save_edit_host(window, cx),
                InputEvent::Change if this.fleet.edit.error.take().is_some() => cx.notify(),
                _ => {}
            }));
        }
        subs.push(cx.subscribe_in(&password, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { .. } => this.sign_in_with_password(window, cx),
            InputEvent::Change if this.fleet.password.error.take().is_some() => cx.notify(),
            _ => {}
        }));
        let form = AddForm { open: false, address, port, user, password: add_password, key_file: None, error: None, suggestions: Vec::new(), picked: None };
        let password = PasswordForm { host: None, input: password, error: None, signing_in: false, tries: Tries::default() };
        let fleet = Self {
            hosts: Hosts::default(),
            sessions: HashMap::new(),
            form,
            installs: HashMap::new(),
            tokens: HashMap::new(),
            notices: HashMap::new(),
            password,
            edit: EditForm { host: None, address: edit_address, port: edit_port, user: edit_user, error: None },
            stuck: HashSet::new(),
            pending: Pending::default(),
            asked: HashMap::new(),
            status_retries: HashMap::new(),
        };
        (fleet, subs)
    }

    /// Records how the install on `id` goes, as one started elsewhere too; a pocketd that was already running
    /// needs no checklist. True when this progress finished an install, as its host's space is new then.
    fn install_progress(&mut self, id: &str, update: bool, progress: Progress) -> bool {
        let list = self.installs.entry(id.into()).or_insert_with(|| Checklist::new(update));
        let was_open = list.outcome.is_none();
        list.apply(progress);
        let installed = just_installed(was_open, list);
        if list.update && list.stuck {
            self.stuck.insert(id.into());
        }
        if list.outcome == Some(Outcome::Reused) {
            self.installs.remove(id);
        }
        installed
    }

    fn remove(&mut self, id: &str) {
        self.hosts.forget(id);
        self.sessions.remove(id);
        self.installs.remove(id);
        self.tokens.remove(id);
        self.notices.remove(id);
        self.status_retries.remove(id);
        if self.password.host.as_deref() == Some(id) {
            self.password.host = None;
        }
        if self.edit.host.as_deref() == Some(id) {
            self.edit.host = None;
        }
        self.stuck.remove(id);
        self.pending.forget(id);
    }
}

impl Desktop {
    /// Takes what this Mac's pocketd says of its hosts and of the host messages sent it.
    pub(crate) fn on_hosts(&mut self, ev: agents::Event, window: &mut Window, cx: &mut Context<Self>) {
        use agents::Event;
        let turns = match ev {
            Event::Hosts(entries) => self.fleet.hosts.replace(entries),
            Event::HostUpdate { request, host } => {
                let id = host.id.clone();
                if self.fleet.hosts.list.iter().any(|h| h.id == id && h.target != host.target) {
                    self.fleet.stuck.remove(&id);
                    self.fleet.installs.remove(&id);
                }
                let turns = self.fleet.hosts.update(host).into_iter().collect();
                match self.fleet.asked.remove(&request) {
                    Some(Asked::Add { password }) => {
                        self.close_add_host(window, cx);
                        // A password signs in first; until then ssh can't reach the host to install.
                        match password.is_empty() {
                            true => self.install_host(&id, false, cx),
                            false => self.fleet.pending.hold(id, password),
                        }
                    }
                    Some(Asked::Edit(edited)) if self.fleet.edit.host.as_ref() == Some(&edited) => self.close_edit_host(cx),
                    _ => {}
                }
                turns
            }
            Event::HostProgress { host, update, progress } => {
                if self.fleet.install_progress(&host, update, progress) {
                    self.flash(flash_key(&Space::Host(host)), cx);
                }
                Vec::new()
            }
            Event::HostError { request, detail, message, .. } => {
                match self.fleet.asked.remove(&request) {
                    Some(Asked::Add { .. }) => self.fleet.form.error = Some(message),
                    Some(Asked::Install(id)) => self.fleet.installs.get_mut(&id).into_iter().for_each(|l| l.fail(message.clone())),
                    Some(Asked::Uninstall(id)) => _ = self.fleet.notices.insert(id, message),
                    Some(Asked::SignIn(id)) => self.signed_in(&id, Some((detail, message))),
                    Some(Asked::Edit(id)) if self.fleet.edit.host.as_ref() == Some(&id) => self.fleet.edit.error = Some(message),
                    Some(Asked::Edit(_)) | None => {}
                }
                Vec::new()
            }
            Event::HostDone(request) => {
                match self.fleet.asked.remove(&request) {
                    Some(Asked::Uninstall(id)) => self.leave_host(&id, Leave::Uninstalled, cx),
                    Some(Asked::SignIn(id)) => self.signed_in(&id, None),
                    _ => {}
                }
                Vec::new()
            }
            _ => return,
        };
        for turn in turns {
            match turn {
                Turn::Online(id) => self.host_online(&id, window, cx),
                Turn::Offline(id) => {
                    hosts::remote::forget(&id);
                    self.removals.forget(Some(&id));
                }
                Turn::Gone(id) => self.forget_host(&id, cx),
            }
        }
        let refused: Vec<(String, String)> = self
            .fleet
            .hosts
            .list
            .iter()
            .filter_map(|h| Some((h.id.clone(), self.fleet.pending.take_for(&h.id, self.fleet.hosts.links.get(&h.id)?)?)))
            .collect();
        for (id, password) in refused {
            self.sign_in(id, password, cx);
        }
        cx.notify();
    }

    /// Serves the host's folders and terminals here, once its pocketd says it can.
    fn host_online(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.fleet.hosts.list.iter().find(|h| h.id == id).cloned() else { return };
        let id = id.to_string();
        cx.spawn_in(window, async move |this, cx| {
            let (client, endpoint, info) = cx
                .background_executor()
                .spawn(async move {
                    let endpoint = host.endpoint();
                    let client = Arc::new(Client::to(&host.id, endpoint.clone()));
                    let info = client.status();
                    (client, endpoint, info)
                })
                .await;
            this.update_in(cx, |d, window, cx| {
                let Ok(info) = info else {
                    // The link only turns online once, so a pocketd that didn't answer (still starting, say) is asked again here.
                    let retry = cx.spawn_in(window, {
                        let id = id.clone();
                        async move |this, cx| {
                            cx.background_executor().timer(STATUS_RETRY).await;
                            this.update_in(cx, |d, window, cx| {
                                if d.fleet.hosts.links.online(&id) && hosts::remote::connected(&id).is_none() {
                                    d.host_online(&id, window, cx);
                                }
                            })
                            .ok();
                        }
                    });
                    d.fleet.status_retries.insert(id.clone(), retry);
                    return;
                };
                let status = on_status(&info, d.fleet.hosts.links.online(&id), d.fleet.tokens.get(&id), &endpoint.token);
                if !status.serve {
                    return;
                }
                if status.reopen {
                    d.daemon.remove(&id);
                    d.close_host_agents(&id);
                }
                d.fleet.tokens.insert(id.clone(), endpoint.token.clone());
                hosts::remote::register(client);
                d.host_finder.update(cx, |f, cx| f.host_back(&id, window, cx));
                d.handoff_host_back(&id, cx);
                d.clone_form.update(cx, |f, cx| f.fetch_home(window, cx));
                let shared = hosts::remote::shared_dir(&info);
                d.open_host_agents(&id, &endpoint, info.user_home, shared, window, cx);
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

    /// Opens Settings › Hosts with the add form filled with `hint`, as an agent's card asks.
    pub(crate) fn link_host(&mut self, hint: String, window: &mut Window, cx: &mut Context<Self>) {
        self.show_hosts(window, cx);
        self.open_add_host(Some(hint), window, cx);
    }

    pub(crate) fn show_hosts(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.open_settings(&crate::actions::OpenSettings, window, cx);
        self.show_section(crate::settings::Section::Hosts, cx);
    }

    /// An older pocketd, without hosts.v1, answered: it links no hosts, so the saved ones only show.
    pub(crate) fn hosts_read_only(&self) -> bool {
        !self.agents.caps.is_empty() && !self.agents.hosts_offered()
    }

    /// The host's name as the user sees it: the one they renamed it to, else the one it was added with.
    pub(crate) fn host_name(&self, id: &str) -> String {
        self.store.sidebar.space_names.get(id).cloned().unwrap_or_else(|| self.added_name(id))
    }

    /// The name pocketd lists the host under, which failure messages name as the one to `ssh`.
    pub(crate) fn added_name(&self, id: &str) -> String {
        self.fleet.hosts.list.iter().find(|h| h.id == id).map_or_else(|| id.to_string(), |h| h.name.clone())
    }

    /// Opens the add form, its address filled with `hint` when an agent named the host.
    pub(crate) fn open_add_host(&mut self, hint: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.close_edit_host(cx);
        self.fleet.form.open = true;
        self.fleet.form.address.update(cx, |f, cx| {
            if let Some(hint) = hint {
                f.set_value(hint, window, cx);
            }
            f.focus(window, cx);
        });
        cx.spawn(async |this, cx| {
            let home = std::env::home_dir().unwrap_or_default();
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

    pub(crate) fn add_host(&mut self, cx: &mut Context<Self>) {
        let form = &self.fleet.form;
        let [address, port, user] = [&form.address, &form.port, &form.user].map(|f| f.read(cx).value().to_string());
        let password = form.password.read(cx).value().to_string();
        let target = match &form.picked {
            Some(alias) if alias == address.trim() => hosts::alias(&address, &port, &user),
            _ => hosts::target(&address, &port, &user, form.key_file.as_deref(), &form.suggestions),
        };
        match target {
            Ok(target) => {
                let request = self.outbox.local.host_add(&target);
                self.fleet.asked.insert(request, Asked::Add { password });
            }
            Err(e) => self.fleet.form.error = Some(e.into()),
        }
        cx.notify();
    }

    /// Opens the edit form under host `id`'s row, filled with how ssh reaches it now; one form shows at a time.
    pub(crate) fn open_edit_host(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(host) = self.fleet.hosts.list.iter().find(|h| h.id == id) else { return };
        let [port, user] = placeholders(matches!(host.target, agents::hosts::Target::Alias { .. }));
        let form = &self.fleet.edit;
        for ((field, value), placeholder) in
            [&form.address, &form.port, &form.user].into_iter().zip(hosts::fields(&host.target)).zip([String::new(), port, user])
        {
            field.update(cx, |f, cx| {
                f.set_placeholder(placeholder, window, cx);
                f.set_value(value, window, cx);
            });
        }
        form.user.update(cx, |f, cx| f.focus(window, cx));
        (self.fleet.edit.host, self.fleet.edit.error) = (Some(id.into()), None);
        if self.fleet.form.open {
            self.close_add_host(window, cx);
        }
        if self.fleet.password.host.as_deref() == Some(id) {
            self.close_password(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn close_edit_host(&mut self, cx: &mut Context<Self>) {
        (self.fleet.edit.host, self.fleet.edit.error) = (None, None);
        cx.notify();
    }

    /// Sends what the edit form says; a target as it was only shuts the form.
    pub(crate) fn save_edit_host(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let form = &self.fleet.edit;
        let Some(host) = form.host.as_ref().and_then(|id| self.fleet.hosts.list.iter().find(|h| &h.id == id)) else { return };
        let [address, port, user] = [&form.address, &form.port, &form.user].map(|f| f.read(cx).value().to_string());
        match hosts::edited(&host.target, &address, &port, &user) {
            Ok(target) if target == host.target => return self.close_edit_host(cx),
            Ok(target) => {
                let id = host.id.clone();
                let request = self.outbox.local.host_edit(&id, &target);
                self.fleet.asked.insert(request, Asked::Edit(id));
            }
            Err(e) => self.fleet.edit.error = Some(e.into()),
        }
        cx.notify();
    }

    pub(crate) fn install_host(&mut self, id: &str, update: bool, cx: &mut Context<Self>) {
        if self.fleet.installs.get(id).is_some_and(|l| l.outcome.is_none()) {
            return;
        }
        if self.fleet.edit.host.as_deref() == Some(id) {
            self.close_edit_host(cx);
        }
        self.fleet.installs.insert(id.into(), Checklist::new(update));
        let archives = hosts::dist_dir().map(|d| d.display().to_string());
        let request = self.outbox.local.host_install(id, update, archives.as_deref());
        self.fleet.asked.insert(request, Asked::Install(id.into()));
        cx.notify();
    }

    pub(crate) fn retry_host(&mut self, id: &str) {
        self.outbox.local.host_retry(id);
    }

    pub(crate) fn remove_host(&mut self, id: &str, uninstall: bool, cx: &mut Context<Self>) {
        self.leave_host(id, if uninstall { Leave::Uninstall } else { Leave::Keep }, cx);
    }

    fn leave_host(&mut self, id: &str, leave: Leave, cx: &mut Context<Self>) {
        for step in removal(id, leave, &self.projects(cx)) {
            match step {
                Removing::Uninstall => return self.uninstall_host(id, cx),
                Removing::Host => {
                    self.outbox.local.host_remove(id, false);
                    self.forget_host(id, cx);
                }
                Removing::Forget => self.forget_host(id, cx),
                Removing::Project(p) => self.remove_project(&p, cx),
            }
        }
    }

    /// Asks pocketd to remove its service from the host, then the host; a host that can't be reached stays, saying why.
    fn uninstall_host(&mut self, id: &str, cx: &mut Context<Self>) {
        let request = self.outbox.local.host_remove(id, true);
        self.fleet.asked.insert(request, Asked::Uninstall(id.into()));
        self.fleet.notices.insert(id.into(), "Uninstalling pocketd\u{2026}".into());
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

    pub(crate) fn sign_in(&mut self, id: String, password: String, cx: &mut Context<Self>) {
        if !self.fleet.hosts.list.iter().any(|h| h.id == id) {
            return;
        }
        let form = &mut self.fleet.password;
        if form.host.as_ref() != Some(&id) {
            form.tries = Tries::default();
        }
        // One at a time, and none while sshd's penalty for the last wrong passwords lasts.
        if form.signing_in || !form.tries.allowed(Instant::now()) {
            return;
        }
        (form.host, form.signing_in, form.error) = (Some(id.clone()), true, None);
        let request = self.outbox.local.host_sign_in(&id, &password);
        self.fleet.asked.insert(request, Asked::SignIn(id));
        cx.notify();
    }

    /// How the sign-in on `id` ended: `refused` with why ssh failed, if it did, and pocketd's words.
    fn signed_in(&mut self, id: &str, refused: Option<(Option<Failure>, String)>) {
        let name = self.added_name(id);
        let form = &mut self.fleet.password;
        form.signing_in = false;
        // The form may have moved to another host meanwhile.
        if form.host.as_deref() != Some(id) {
            return;
        }
        match refused {
            None => form.host = None,
            Some((Some(f), _)) => {
                let locked = f == Failure::Auth && form.tries.wrong(Instant::now());
                form.error = Some(sign_in_error(f, &name, locked));
            }
            Some((None, message)) => form.error = Some(message),
        }
    }

    fn forget_host(&mut self, id: &str, cx: &mut Context<Self>) {
        self.fleet.remove(id);
        self.daemon.remove(id);
        self.close_host_agents(id);
        self.registry.forget(id);
        hosts::remote::forget(id);
        self.terminals.update(cx, |t, _| {
            let left = t.lists.forget(id);
            t.listed(left);
        });
        self.activity_changed(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Failure, Hosts, Leave, Link, Links, Pending, Removing, Status, Tries, Turn, just_installed, needs_update, on_status, removal, sign_in_error};
    use agents::hosts::{Entry, Target};
    use hosts::remote::HostInfo;
    use std::time::{Duration, Instant};

    fn entry(id: &str, link: Link) -> Entry {
        Entry { id: id.into(), name: id.into(), target: Target::alias(id), windows: false, socket: format!("/h/{id}.sock"), link }
    }

    fn online() -> Link {
        Link::Online { version: "1.4.2".into(), uptime: 0 }
    }

    #[test]
    fn only_an_install_that_just_finished_makes_a_new_space() {
        let list = |update, progress: &[agents::hosts::Progress]| {
            let mut list = hosts::checklist::Checklist::new(update);
            progress.iter().cloned().for_each(|p| list.apply(p));
            list
        };
        let done = list(false, &[agents::hosts::Progress::Done]);
        assert!(just_installed(true, &done));
        assert!(!just_installed(false, &done), "a late message after the end");
        assert!(!just_installed(true, &list(true, &[agents::hosts::Progress::Done])), "an update");
        assert!(!just_installed(true, &list(false, &[agents::hosts::Progress::Running])), "pocketd already ran");
        assert!(!just_installed(true, &list(false, &[agents::hosts::Progress::Error("no sshd".into())])));
    }

    #[test]
    fn a_host_list_from_pocketd_replaces_the_hosts_shown() {
        let mut hosts = Hosts::default();
        hosts.replace(vec![entry("devbox", online()), entry("gpu", Link::Connecting)]);
        let turns = hosts.replace(vec![entry("gpu", Link::Retrying(Failure::NotRunning)), entry("nas", Link::Connecting)]);
        assert_eq!(turns, [Turn::Gone("devbox".into())]);
        assert_eq!(hosts.list.iter().map(|h| h.id.as_str()).collect::<Vec<_>>(), ["gpu", "nas"]);
        assert_eq!((hosts.links.get("devbox"), hosts.links.get("gpu")), (None, Some(&Link::Retrying(Failure::NotRunning))));
    }

    #[test]
    fn a_host_update_changes_only_that_host() {
        let mut hosts = Hosts::default();
        hosts.replace(vec![entry("devbox", online()), entry("gpu", Link::Connecting)]);
        let renamed = Entry { name: "GPU box".into(), ..entry("gpu", Link::Failed(Failure::Auth)) };
        assert_eq!(hosts.update(renamed), None);
        assert_eq!(hosts.list.iter().map(|h| h.name.as_str()).collect::<Vec<_>>(), ["devbox", "GPU box"]);
        assert_eq!((hosts.links.get("devbox"), hosts.links.get("gpu")), (Some(&online()), Some(&Link::Failed(Failure::Auth))));
        hosts.update(entry("nas", Link::Connecting));
        assert_eq!(hosts.list.len(), 3, "an added host joins the list");
    }

    #[test]
    fn a_host_turning_online_is_served_once() {
        let mut hosts = Hosts::default();
        assert_eq!(hosts.replace(vec![entry("devbox", online())]), [Turn::Online("devbox".into())], "already online when first listed");
        assert_eq!(hosts.update(entry("devbox", Link::Online { version: "1.4.2".into(), uptime: 10 })), None);
        assert_eq!(hosts.update(entry("devbox", Link::Retrying(Failure::Dropped))), Some(Turn::Offline("devbox".into())));
        assert_eq!(hosts.update(entry("devbox", online())), Some(Turn::Online("devbox".into())));
    }

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
    fn removing_from_this_mac_lets_the_host_go_then_its_projects() {
        let projects = ["//devbox/home/me/app".to_string(), "/Users/me/site".into(), "//gpu/home/me/ml".into()];
        assert_eq!(removal("devbox", Leave::Keep, &projects), [Removing::Host, Removing::Project("//devbox/home/me/app".into())]);
        assert_eq!(removal("nas", Leave::Keep, &projects), [Removing::Host]);
    }

    #[test]
    fn uninstalling_removes_the_service_before_anything_else() {
        let projects = ["//devbox/home/me/app".to_string()];
        assert_eq!(removal("devbox", Leave::Uninstall, &projects), [Removing::Uninstall, Removing::Forget, Removing::Project("//devbox/home/me/app".into())]);
    }

    #[test]
    fn a_finished_uninstall_does_not_ask_pocketd_to_remove_the_host_again() {
        let projects = ["//devbox/home/me/app".to_string()];
        assert_eq!(removal("devbox", Leave::Uninstalled, &projects), [Removing::Forget, Removing::Project("//devbox/home/me/app".into())]);
    }
}
