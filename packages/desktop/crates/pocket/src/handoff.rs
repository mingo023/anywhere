//! Handing a session on this Mac off to a connected host: the code, and the conversation where it can move.

mod cheer;
mod view;

use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::spaces::Space;
use crate::util::basename;
use futures::StreamExt;
use futures::channel::mpsc;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use handoff::Step;
use handoff::checks::{Probe, Problem, Wanted};
use handoff::pack::Pack;
use handoff::place::Placed;
use serde_json::json;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use store::LaunchPick;

/// How long the agent's turn may take to stop.
const STOP: Duration = Duration::from_secs(20);
/// How long the host's agent may take to show up, then to name its conversation.
const START: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(500);
/// How many polls a connected host's pocketd gets to answer before it reads as not ready.
const CONNECT_TRIES: usize = 20;
const FORK_HISTORY_CAP: &str = "fork.history.v1";
const APPEND_CAP: &str = "fs.append.v1";

/// The session handed off, as it was when the sheet opened.
#[derive(Clone, Debug, Default)]
pub(crate) struct Source {
    pub agent: String,
    pub cwd: String,
    pub title: String,
    pub provider: String,
    pub agent_name: String,
    /// Its agent's own session id; `None` when Anywhere didn't start it.
    pub session: Option<String>,
    pub model: String,
    pub effort: String,
    pub working: bool,
    /// The project's folder name on this Mac.
    pub repo: String,
    pub branch: String,
    pub files: usize,
    pub added: usize,
    pub removed: usize,
    pub commits: usize,
    pub messages: usize,
    /// Why its conversation can't move; `None` when it can.
    pub why_not: Option<String>,
}

impl Source {
    pub fn conversation(&self) -> bool {
        self.why_not.is_none()
    }

    /// Whether the host's session is checked for this one's messages. Only a pocketd with `caps` naming fork history
    /// shows it before the first prompt, and Codex forks in its TUI.
    pub fn awaits_history(&self, caps: &[String]) -> bool {
        caps.iter().any(|c| c == FORK_HISTORY_CAP) && self.provider == "claude" && self.messages > 0
    }
}

/// What a host row says of that host.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Tag {
    Checking,
    Ready,
    NotReady,
    SummaryOnly,
}

pub(crate) fn tag(probe: Option<&Probe>, conversation: bool) -> Tag {
    match probe {
        None => Tag::Checking,
        Some(p) if !p.ready() => Tag::NotReady,
        Some(_) if conversation => Tag::Ready,
        Some(_) => Tag::SummaryOnly,
    }
}

/// The pick sheet's main button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Primary {
    HandOff,
    CheckAgain,
    ReviewSummary,
    WriteSummary,
    /// The host is still being checked.
    Waiting,
}

/// `drafted`: this Mac had a conversation to draft the summary from.
pub(crate) fn primary(tag: Tag, drafted: bool) -> Primary {
    match tag {
        Tag::Checking => Primary::Waiting,
        Tag::NotReady => Primary::CheckAgain,
        Tag::Ready => Primary::HandOff,
        Tag::SummaryOnly if drafted => Primary::ReviewSummary,
        Tag::SummaryOnly => Primary::WriteSummary,
    }
}

/// A step's result.
#[derive(Debug)]
pub(crate) enum Landed {
    Stopped,
    Packed(Pack),
    Sent(String),
    Placed(Placed),
    Carried,
    /// The host's agent, by id.
    Started(String),
    Checked,
}

/// A handoff underway, or stopped at a failed step.
#[derive(Debug)]
pub(crate) struct Run {
    pub steps: Vec<Step>,
    pub at: usize,
    pub failed: Option<String>,
    pub pack: Option<Pack>,
    pub bundle: Option<String>,
    pub placed: Option<Placed>,
    pub started: Option<String>,
    /// Bytes of the conversation on the host, of its whole.
    pub sent: Option<(usize, usize)>,
}

impl Run {
    pub fn new(working: bool, conversation: bool) -> Self {
        Self { steps: handoff::steps(working, conversation), at: 0, failed: None, pack: None, bundle: None, placed: None, started: None, sent: None }
    }

    pub fn step(&self) -> Option<Step> {
        self.steps.get(self.at).copied()
    }

    pub fn land(&mut self, l: Landed) {
        match l {
            Landed::Packed(p) => self.pack = Some(p),
            Landed::Sent(b) => self.bundle = Some(b),
            Landed::Placed(p) => self.placed = Some(p),
            Landed::Started(id) => self.started = Some(id),
            Landed::Stopped | Landed::Carried | Landed::Checked => {}
        }
        self.at += 1;
    }

    /// A retry starts again at the failed step; making the worktree uses up the bundle, so it is sent again first.
    pub fn retry(&mut self) {
        self.failed = None;
        self.sent = None;
        if self.step() == Some(Step::Worktree) {
            self.at -= 1;
        }
    }

    /// Whether the run already stopped the agent's turn on this Mac.
    pub fn stopped_turn(&self) -> bool {
        self.steps.first() == Some(&Step::Stop) && self.at > 0
    }

    /// Cancelling is offered until the host's agent starts.
    pub fn cancellable(&self) -> bool {
        self.failed.is_none() && !matches!(self.step(), Some(Step::Start | Step::Check) | None)
    }
}

#[derive(Debug, PartialEq)]
pub(crate) enum Stage {
    Pick,
    Summary,
    Moving,
    Done,
}

fn next_generation() -> u64 {
    static LAST: AtomicU64 = AtomicU64::new(0);
    LAST.fetch_add(1, Ordering::Relaxed) + 1
}

/// One handoff, from the sheet opening to its last step.
pub(crate) struct Handoff {
    pub source: Source,
    pub host: Option<String>,
    pub probes: HashMap<String, Probe>,
    pub stage: Stage,
    pub run: Option<Run>,
    /// Set anew by each check and step, so a result for an older one, of this handoff or another, is dropped.
    pub generation: u64,
    /// Each click on the mascot cheers again.
    pub cheers: usize,
    /// What each probed host's pocketd can do.
    pub caps: HashMap<String, Vec<String>>,
}

impl Handoff {
    pub fn new(source: Source) -> Self {
        Self { source, host: None, probes: HashMap::new(), stage: Stage::Pick, run: None, generation: 0, cheers: 0, caps: HashMap::new() }
    }

    pub fn probe(&self) -> Option<&Probe> {
        self.probes.get(self.host.as_deref()?)
    }

    /// Whether the run stopped while handing off to `host`, so it goes on once that host is back.
    pub fn stopped_on(&self, host: &str) -> bool {
        self.host.as_deref() == Some(host) && self.run.as_ref().is_some_and(|r| r.failed.is_some())
    }

    /// Picks the first host in `online` unless the picked one still is.
    pub fn keep_host(&mut self, online: &[String]) {
        if !self.host.as_ref().is_some_and(|h| online.contains(h)) {
            self.host = online.first().cloned();
        }
    }
}

/// The menu row that opens the sheet, or brings back the handoff `underway`.
pub(crate) fn menu_label(underway: bool) -> &'static str {
    if underway { "Show handoff" } else { "Hand off to a host…" }
}

/// How a session of this Mac reads while it's handed off, and after.
#[derive(Clone, Debug, PartialEq)]
pub(crate) enum Mark {
    /// Underway to the host, by id.
    Moving(String),
    /// Stopped at a failed step; the session is still here.
    Stopped,
}

impl Mark {
    pub fn host(&self) -> Option<&str> {
        match self {
            Mark::Moving(h) => Some(h),
            Mark::Stopped => None,
        }
    }

    /// The line under its card's title, the host called `host`.
    pub fn notice(&self, host: &str) -> ui::Notice {
        let (icon, text, warn) = match self {
            Mark::Moving(_) => ("server", format!("Moving to {host}…"), false),
            Mark::Stopped => ("warning", "Handoff stopped · still here".into(), true),
        };
        ui::Notice { icon: Some(icon), text: text.into(), warn }
    }

    /// What its tab says under the title, in place of `agent`, its agent's name.
    pub fn tab(&self, agent: &str, host: &str) -> String {
        match self {
            Mark::Moving(_) => format!("{agent} · moving to {host}"),
            Mark::Stopped => agent.to_string(),
        }
    }

    /// What its locked composer says. A stopped handoff keeps it locked, as a retry sends the code packed before.
    pub fn composer(&self) -> &'static str {
        match self {
            Mark::Stopped => "Prompts wait until you retry or close the handoff",
            _ => "Prompts wait until the handoff finishes",
        }
    }
}

/// How session `agent` reads, given the handoff open. Once the handoff is done it reads as before: its conversation
/// goes on here too, apart from the host's copy.
pub(crate) fn mark(open: Option<&Handoff>, agent: &str) -> Option<Mark> {
    let h = open.filter(|h| h.source.agent == agent && h.stage == Stage::Moving)?;
    Some(if h.run.as_ref()?.failed.is_some() { Mark::Stopped } else { Mark::Moving(h.host.clone()?) })
}

pub(crate) enum HandoffEvent {
    Close,
    AddHost,
    Check,
    Start,
    OpenShell(String),
    Cancel,
    Retry,
    OpenHandedOff,
}

pub(crate) struct HostRow {
    pub id: String,
    pub name: String,
    pub address: String,
    pub glyph: &'static str,
    pub colour: Option<u32>,
    pub online: bool,
}

pub(crate) struct HandoffState {
    pub open: Option<Handoff>,
    pub summary: Entity<TextareaState>,
    hosts: Vec<HostRow>,
    names: HashMap<String, String>,
    capturing: bool,
    _subs: Vec<Subscription>,
}

impl EventEmitter<HandoffEvent> for HandoffState {}

impl HandoffState {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let summary = cx.new(|cx| TextareaState::new(window, cx).placeholder("What were you doing, what's done, what's next?").auto_grow(7, 14));
        let subs = vec![cx.observe(&summary, |_, _, cx| cx.notify())];
        Self { open: None, summary, hosts: Vec::new(), names: HashMap::new(), capturing: false, _subs: subs }
    }

    pub fn show(&mut self, hosts: Vec<HostRow>, names: HashMap<String, String>, capturing: bool) {
        (self.hosts, self.names, self.capturing) = (hosts, names, capturing);
    }

    /// Whether a handoff of agent `id` is underway.
    pub fn moving(&self, id: &str) -> bool {
        self.open.as_ref().is_some_and(|h| h.source.agent == id && h.stage == Stage::Moving)
    }

    fn name(&self, id: &str) -> String {
        self.hosts.iter().find(|h| h.id == id).map(|h| h.name.clone()).or_else(|| self.names.get(id).cloned()).unwrap_or_else(|| id.to_string())
    }

    fn online(&self, id: &str) -> bool {
        self.hosts.iter().any(|h| h.id == id && h.online)
    }
}

/// What `next_step` reads of the handoff before running its step, so the step runs with the sheet's `Entity` free.
struct Now {
    step: Option<Step>,
    generation: u64,
    source: Source,
    caps: Vec<String>,
    pack: Option<Pack>,
    bundle: Option<String>,
    placed: Option<Placed>,
    started: Option<String>,
    project: Option<String>,
    host: String,
}

/// A host's own path for this Mac's `path` in a moved transcript: the path as given, and as it resolves.
fn mac_paths(cwd: &str) -> Vec<String> {
    let mut list = vec![cwd.to_string()];
    if let Some(real) = std::fs::canonicalize(cwd).ok().and_then(|p| p.to_str().map(String::from)).filter(|p| p != cwd) {
        list.push(real);
    }
    list
}

/// Whether the host's session shows the conversation: open under its id, and holding its messages when they're awaited.
fn shows_conversation(opened: bool, awaits: bool, items: usize) -> bool {
    opened && (!awaits || items > 0)
}

impl Desktop {
    /// Whether agent `s` can be handed off: a session on this Mac, on a desktop that may start sessions.
    pub(crate) fn can_hand_off(&self, s: &agents::Summary) -> bool {
        !self.agents.observe_only() && hosts::split(&s.cwd).0.is_none() && s.cwd.starts_with('/')
    }

    /// The connected hosts, in the switcher's order.
    fn online_hosts(&self) -> Vec<String> {
        self.fleet.hosts.list.iter().filter(|h| self.fleet.hosts.links.online(&h.id)).map(|h| h.id.clone()).collect()
    }

    pub(crate) fn handoff_view(&mut self, cx: &mut Context<Self>) -> Div {
        let order = self.space_order();
        let hosts = self
            .fleet
            .hosts
            .list
            .iter()
            .map(|h| {
                let space = Space::on(Some(&h.id));
                HostRow {
                    id: h.id.clone(),
                    name: self.host_name(&h.id),
                    address: h.address(),
                    glyph: self.space_glyph(&space),
                    colour: self.space_colour(&space, &order),
                    online: self.fleet.hosts.links.online(&h.id),
                }
            })
            .collect();
        let (names, capturing) = (self.store.sidebar.space_names.clone(), self.capturing);
        self.handoff.update(cx, |h, _| h.show(hosts, names, capturing));
        div().absolute().inset_0().child(self.handoff.clone())
    }

    pub(crate) fn on_handoff_event(&mut self, _: &Entity<HandoffState>, ev: &HandoffEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            HandoffEvent::Close => self.close_overlay(window, cx),
            HandoffEvent::AddHost => {
                self.close_overlay(window, cx);
                self.show_hosts(window, cx);
            }
            HandoffEvent::Check => self.check_hosts(cx),
            HandoffEvent::Start => self.start_handoff(cx),
            HandoffEvent::OpenShell(id) => self.open_host_shell(id, window, cx),
            HandoffEvent::Cancel => self.cancel_handoff(window, cx),
            HandoffEvent::Retry => self.retry_handoff(cx),
            HandoffEvent::OpenHandedOff => self.open_handed_off(window, cx),
        }
    }

    /// Opens the sheet for agent `id`, or brings back the handoff of it underway.
    pub(crate) fn open_handoff(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        // One handoff runs at a time: a session's menu brings back the one underway rather than dropping it.
        if self.handoff.read(cx).open.as_ref().is_some_and(|h| h.run.is_some() && (h.source.agent == id || h.stage == Stage::Moving)) {
            return self.open(Overlay::Handoff, window, cx);
        }
        let Some(s) = self.agents.get(id).filter(|s| self.can_hand_off(s)) else { return };
        let agent_name = self.agent_name(&s.provider);
        let repo = self.repos.get(&s.cwd);
        let projects = self.projects(cx);
        let (added, removed) = repo.map(|r| r.totals()).unwrap_or_default();
        let timeline = self.agents.timelines.get(id).map(Vec::as_slice).unwrap_or_default();
        let source = Source {
            agent: s.id.clone(),
            cwd: s.cwd.clone(),
            title: s.title.clone(),
            provider: s.provider.clone(),
            why_not: handoff::why_not(&s.provider, &agent_name, s.provider_session_id.as_deref()),
            agent_name,
            session: s.provider_session_id.clone(),
            model: s.model.clone().unwrap_or_default(),
            effort: s.effort.clone().unwrap_or_default(),
            working: s.status == "working",
            repo: basename(self.agent_project(s, &s.cwd, &projects).map_or(&s.cwd, |p| p)),
            branch: repo.map(|r| r.branch.clone()).unwrap_or_default(),
            files: repo.map_or(0, |r| r.files.len()),
            added,
            removed,
            commits: repo.map_or(0, |r| r.unpushed.unwrap_or(r.ahead)),
            messages: timeline.iter().filter(|i| i.kind == "user" || i.kind == "assistant").count(),
        };
        let recap = crate::activity::recap::recap(timeline);
        let known = handoff::summary::Known {
            title: &source.title,
            branch: &source.branch,
            files: source.files,
            asked: Some(recap.asked.as_str()).filter(|a| !a.is_empty()),
            replied: Some(recap.said.as_str()),
        };
        let draft = handoff::summary::draft(&known);
        self.handoff.update(cx, |h, cx| {
            h.summary.update(cx, |t, cx| t.set_value(draft, window, cx));
            h.open = Some(Handoff::new(source));
        });
        self.open(Overlay::Handoff, window, cx);
        self.check_hosts(cx);
    }

    /// Checks every connected host for what the handoff needs.
    pub(crate) fn check_hosts(&mut self, cx: &mut Context<Self>) {
        let online = self.online_hosts();
        let projects = self.projects(cx);
        let checking = self.handoff.update(cx, |h, _| {
            let h = h.open.as_mut()?;
            h.keep_host(&online);
            h.probes.clear();
            h.generation = next_generation();
            Some((h.generation, h.source.clone()))
        });
        let Some((generation, source)) = checking else { return };
        for id in online {
            let on_host: Vec<String> = projects.iter().filter(|p| hosts::split(p).0 == Some(id.as_str())).map(|p| hosts::split(p).1.to_string()).collect();
            let s = source.clone();
            let host = id.clone();
            let executor = cx.background_executor().clone();
            let task = cx.background_executor().spawn(async move {
                // A host's link turns online before its pocketd is asked for status and registered.
                let mut client = hosts::remote::connected(&host);
                for _ in 0..CONNECT_TRIES {
                    if client.is_some() {
                        break;
                    }
                    executor.timer(POLL).await;
                    client = hosts::remote::connected(&host);
                }
                let Some(client) = client else { return (Probe { project: None, problems: vec![Problem::Offline] }, vec![]) };
                let origin = handoff::pack::origin(&s.cwd);
                let w = Wanted { origin: origin.as_deref(), repo: &s.repo, branch: &s.branch, agent: Some(&s.provider) };
                let caps = client.status().map(|i| i.caps).unwrap_or_default();
                (handoff::checks::probe(&*client, &on_host, &w, &|oid| handoff::pack::holds(&s.cwd, oid)), caps)
            });
            cx.spawn(async move |this, cx| {
                let (probe, caps) = task.await;
                this.update(cx, |d, cx| {
                    d.handoff.update(cx, |h, cx| {
                        if let Some(h) = h.open.as_mut().filter(|h| h.generation == generation) {
                            h.caps.insert(id.clone(), caps);
                            h.probes.insert(id, probe);
                            cx.notify();
                        }
                    });
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    pub(crate) fn start_handoff(&mut self, cx: &mut Context<Self>) {
        let started = self.handoff.update(cx, |h, _| {
            let h = h.open.as_mut()?;
            h.run = Some(Run::new(h.source.working, h.source.conversation()));
            h.stage = Stage::Moving;
            Some(())
        });
        if started.is_some() {
            self.next_step(cx);
        }
    }

    /// Runs the step the handoff is at, or finishes it.
    fn next_step(&mut self, cx: &mut Context<Self>) {
        let now = self.handoff.update(cx, |s, _| {
            let h = s.open.as_mut()?;
            let host = h.host.clone()?;
            h.run.as_ref()?;
            h.generation = next_generation();
            let run = h.run.as_ref()?;
            let step = run.step();
            let needs_pack = matches!(step, Some(Step::Send | Step::Worktree));
            let now = Now {
                step,
                generation: h.generation,
                source: h.source.clone(),
                caps: h.caps.get(&host).cloned().unwrap_or_default(),
                pack: run.pack.as_ref().filter(|_| needs_pack).cloned(),
                bundle: run.bundle.clone(),
                placed: run.placed.clone(),
                started: run.started.clone(),
                project: h.probe().and_then(|p| p.project.clone()),
                host,
            };
            if step.is_none() {
                h.stage = Stage::Done;
            }
            Some(now)
        });
        let Some(Now { step, generation, source, caps, pack, bundle, placed, started, project, host }) = now else { return };
        let Some(step) = step else {
            if self.overlay.is_none() {
                self.overlay = Some(Overlay::Handoff);
            }
            return cx.notify();
        };
        let id = format!("{}-{}", basename(&source.cwd), crate::util::now_ms());
        let Some(client) = hosts::remote::connected(&host) else {
            let name = self.host_name(&host);
            return self.step_failed(generation, format!("{name} went offline"), cx);
        };
        match step {
            Step::Stop => {
                self.outbox.interrupt(&source.agent);
                let agent = source.agent.clone();
                self.wait_for(
                    generation,
                    STOP,
                    "The turn didn’t stop",
                    move |d| d.agents.get(&agent).is_none_or(|s| s.status != "working").then_some(Landed::Stopped),
                    cx,
                );
            }
            Step::Pack => self.in_background(generation, move || handoff::pack::pack(&source.cwd, &id).map(Landed::Packed), cx),
            Step::Send => {
                let Some(pack) = pack else { return };
                self.in_background(generation, move || handoff::place::send(&*client, &pack, &id).map(Landed::Sent), cx);
            }
            Step::Worktree => {
                let (Some(pack), Some(bundle), Some(project)) = (pack, bundle, project) else { return };
                let name = basename(&source.cwd);
                let given = self.tree_of(&source.cwd, cx).and_then(|t| self.agents.given_name(&t).map(String::from));
                let task = cx.background_executor().spawn({
                    let client = client.clone();
                    async move { handoff::place::worktree(&*client, &project, &source.repo, &name, &bundle, &pack) }
                });
                cx.spawn(async move |this, cx| {
                    let placed = task.await;
                    this.update(cx, |d, cx| match placed {
                        Ok(p) if d.handoff.read(cx).open.as_ref().is_some_and(|h| h.generation == generation) => {
                            if let Some(given) = &given {
                                let tree = hosts::place(&host, &p.path);
                                d.outbox.rename(&tree, given);
                                d.agents.set_name(&tree, given);
                            }
                            d.step_landed(generation, Landed::Placed(p), cx)
                        }
                        // Cancelled while the worktree was being made: take it back.
                        Ok(p) => cx.background_executor().spawn(async move { handoff::place::undo(&*client, &p) }).detach(),
                        Err(e) => d.step_failed(generation, e, cx),
                    })
                    .ok();
                })
                .detach();
            }
            Step::Conversation => {
                let (Some(placed), Some(session)) = (placed, source.session.clone()) else { return };
                let home = crate::modals::form::home();
                let framed = caps.iter().any(|c| c == APPEND_CAP);
                let (tx, mut rx) = mpsc::unbounded();
                cx.spawn(async move |this, cx| {
                    while let Some(sent) = rx.next().await {
                        if this.update(cx, |d, cx| d.conversation_sent(generation, sent, cx)).is_err() {
                            break;
                        }
                    }
                })
                .detach();
                self.in_background(
                    generation,
                    move || {
                        let t = handoff::transcript::find(&source.provider, &session, std::path::Path::new(&home))
                            .ok_or_else(|| "This Mac has no copy of the conversation".to_string())?;
                        let from = mac_paths(&source.cwd);
                        let mut sent = |done, of| _ = tx.unbounded_send((done, of));
                        handoff::transcript::carry(&*client, &t, &from.iter().map(String::as_str).collect::<Vec<_>>(), &placed.path, framed, &mut sent)
                            .map(|_| Landed::Carried)
                    },
                    cx,
                );
            }
            Step::Start => {
                let Some(placed) = placed else { return };
                let cwd = hosts::place(&host, &placed.path);
                // A retry takes up the agent a slow first try started rather than starting a second.
                if let Some(found) = self.agents.list.iter().find(|s| s.cwd == cwd).map(|s| s.id.clone()) {
                    return self.step_landed(generation, Landed::Started(found), cx);
                }
                let prompt = if source.conversation() { String::new() } else { self.handoff.read(cx).summary.read(cx).value().trim().to_string() };
                let pick = LaunchPick {
                    provider: source.provider.clone(),
                    model: source.model.clone(),
                    effort: source.effort.clone(),
                    account: crate::accounts::BUILT_IN.into(),
                    ..LaunchPick::default()
                };
                let mut spec = pick.spec(&hosts::place(&host, &placed.project), json!({"worktree": hosts::place(&host, &placed.path)}), &prompt);
                self.store.agents.launch(&mut spec, &self.agents, &self.efforts_on(&source.provider, &source.model));
                if let Some(session) = source.session.clone().filter(|_| source.conversation()) {
                    spec["fork"] = session.into();
                    // A moved conversation carries on rather than plans anew; pocketd refuses a Codex fork that plans first.
                    spec["plan"] = false.into();
                }
                // pocketd starts sessions only in projects it was told of; a clone made by following a fix isn't yet.
                let project = hosts::place(&host, &placed.project);
                if !self.store.projects.contains(&project) {
                    self.store.projects.push(project);
                    self.store.save();
                    self.publish_registry();
                }
                if self.send_create(spec).is_none() {
                    let name = self.host_name(&host);
                    return self.step_failed(generation, format!("{name} went offline"), cx);
                }
                let failure = format!("{} didn’t start on {}", source.agent_name, self.host_name(&host));
                self.wait_for(generation, START, &failure, move |d| d.agents.list.iter().find(|s| s.cwd == cwd).map(|s| Landed::Started(s.id.clone())), cx);
            }
            Step::Check => {
                let Some(started) = started else { return };
                let awaits = source.awaits_history(&caps);
                let failure = format!("The conversation didn’t open on {}", self.host_name(&host));
                self.wait_for(
                    generation,
                    START,
                    &failure,
                    move |d| {
                        let opened = d.agents.get(&started).is_some_and(|s| s.provider_session_id.is_some());
                        let items = d.agents.timelines.get(&started).map_or(0, Vec::len);
                        shows_conversation(opened, awaits, items).then_some(Landed::Checked)
                    },
                    cx,
                );
            }
        }
        cx.notify();
    }

    fn in_background(&mut self, generation: u64, work: impl FnOnce() -> Result<Landed, String> + Send + 'static, cx: &mut Context<Self>) {
        let task = cx.background_executor().spawn(async move { work() });
        cx.spawn(async move |this, cx| {
            let result = task.await;
            this.update(cx, |d, cx| match result {
                Ok(l) => d.step_landed(generation, l, cx),
                Err(e) => d.step_failed(generation, e, cx),
            })
            .ok();
        })
        .detach();
    }

    /// Lands the step once `found` sees its result in the app, or fails it with `failure` after `limit`.
    fn wait_for(&mut self, generation: u64, limit: Duration, failure: &str, found: impl Fn(&Desktop) -> Option<Landed> + 'static, cx: &mut Context<Self>) {
        let failure = failure.to_string();
        let until = Instant::now() + limit;
        cx.spawn(async move |this, cx| {
            loop {
                let waiting = this.update(cx, |d, cx| {
                    if d.handoff.read(cx).open.as_ref().is_none_or(|h| h.generation != generation) {
                        return false;
                    }
                    match found(d) {
                        Some(l) => d.step_landed(generation, l, cx),
                        None if Instant::now() >= until => d.step_failed(generation, failure.clone(), cx),
                        None => return true,
                    }
                    false
                });
                if !matches!(waiting, Ok(true)) {
                    break;
                }
                cx.background_executor().timer(POLL).await;
            }
        })
        .detach();
    }

    fn conversation_sent(&mut self, generation: u64, sent: (usize, usize), cx: &mut Context<Self>) {
        self.handoff.update(cx, |s, cx| {
            if let Some(run) = s.open.as_mut().filter(|h| h.generation == generation).and_then(|h| h.run.as_mut()) {
                run.sent = Some(sent);
                cx.notify();
            }
        });
    }

    fn step_landed(&mut self, generation: u64, l: Landed, cx: &mut Context<Self>) {
        let landed = self.handoff.update(cx, |s, _| {
            let run = s.open.as_mut().filter(|h| h.generation == generation).and_then(|h| h.run.as_mut())?;
            run.land(l);
            Some(())
        });
        if landed.is_some() {
            self.next_step(cx);
        }
    }

    fn step_failed(&mut self, generation: u64, message: String, cx: &mut Context<Self>) {
        let failed = self.handoff.update(cx, |s, _| {
            let run = s.open.as_mut().filter(|h| h.generation == generation).and_then(|h| h.run.as_mut())?;
            run.failed = Some(message);
            Some(())
        });
        if failed.is_none() {
            return;
        }
        if self.overlay.is_none() {
            self.overlay = Some(Overlay::Handoff);
        }
        cx.notify();
    }

    pub(crate) fn retry_handoff(&mut self, cx: &mut Context<Self>) {
        let retried = self.handoff.update(cx, |s, _| {
            s.open.as_mut().and_then(|h| h.run.as_mut())?.retry();
            Some(())
        });
        if retried.is_some() {
            self.next_step(cx);
        }
    }

    pub(crate) fn handoff_host_back(&mut self, host: &str, cx: &mut Context<Self>) {
        if self.handoff.read(cx).open.as_ref().is_some_and(|h| h.stopped_on(host)) {
            self.retry_handoff(cx);
        }
    }

    /// Stops the handoff and takes back the worktree it made on the host, unless the host's agent started in it.
    pub(crate) fn cancel_handoff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(h) = self.handoff.update(cx, |s, _| s.open.take()) {
            self.undo_handoff(h, cx);
        }
        self.close_overlay(window, cx);
    }

    fn undo_handoff(&mut self, h: Handoff, cx: &mut Context<Self>) {
        let (Some(host), Some(run)) = (h.host, h.run) else { return };
        // An agent that started too late to be seen may be working in the worktree.
        if run.failed.is_some() && run.step() == Some(Step::Start) {
            return;
        }
        let Some(placed) = run.placed else { return };
        if let Some(agent) = &run.started {
            // A started agent is taken back only when its check failed: it opened without the history.
            if run.failed.is_none() {
                return;
            }
            if let Some(term) = self.agents.get(agent).map(|s| s.terminal_id.clone()) {
                self.close_pane(&term, cx);
            }
        }
        let Some(client) = hosts::remote::connected(&host) else { return };
        cx.background_executor().spawn(async move { handoff::place::undo(&*client, &placed) }).detach();
    }

    /// The sheet going away: a handoff underway keeps going, out of sight; any other is over.
    pub(crate) fn handoff_closed(&mut self, cx: &mut Context<Self>) {
        let over = self.handoff.update(cx, |s, _| {
            let underway = s.open.as_ref().is_some_and(|h| h.stage == Stage::Moving && h.run.as_ref().is_some_and(|r| r.failed.is_none()));
            if underway { None } else { s.open.take() }
        });
        if let Some(h) = over {
            let failed = h.run.as_ref().is_some_and(|r| r.failed.is_some());
            if failed {
                self.undo_handoff(h, cx);
            }
        }
    }

    /// Goes to the session that took over on the host.
    pub(crate) fn open_handed_off(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let started = self.handoff.update(cx, |s, _| s.open.take()).and_then(|h| h.run).and_then(|r| r.started);
        self.close_overlay(window, cx);
        if let Some(id) = started {
            self.focus_agent(&id, window, cx);
        }
    }

    /// What session `agent` shows of a handoff of it.
    pub(crate) fn handoff_mark(&self, agent: &str, cx: &App) -> Option<Mark> {
        mark(self.handoff.read(cx).open.as_ref(), agent)
    }

    /// What session `agent`'s card says of a handoff of it.
    pub(crate) fn handoff_notice(&self, agent: &str, cx: &App) -> Option<ui::Notice> {
        self.handoff_mark(agent, cx).map(|m| m.notice(&self.mark_host(&m)))
    }

    /// What session `agent`'s tab says under its title, `name` being its agent's.
    pub(crate) fn handoff_tab(&self, agent: &str, name: &str, cx: &App) -> Option<String> {
        self.handoff_mark(agent, cx).map(|m| m.tab(name, &self.mark_host(&m)))
    }

    fn mark_host(&self, m: &Mark) -> String {
        m.host().map(|h| self.host_name(h)).unwrap_or_default()
    }

    /// A shell on host `id` for fixing what the check found, in its checkout of the project, else any project there.
    pub(crate) fn open_host_shell(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let found = self.handoff.read(cx).open.as_ref().and_then(|h| h.probes.get(id)).and_then(|p| p.project.clone()).map(|p| hosts::place(id, &p));
        let Some(project) = found.or_else(|| self.projects(cx).into_iter().find(|p| hosts::split(p).0 == Some(id))) else { return };
        self.close_overlay(window, cx);
        self.select_tree(project, None, cx);
        self.new_shell(workspace::Place::Pane(None), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{FORK_HISTORY_CAP, Handoff, Landed, Mark, Primary, Run, Source, Stage, Tag, mark, primary, shows_conversation, tag};
    use handoff::Step;
    use handoff::checks::{Probe, Problem};

    fn ready() -> Probe {
        Probe { project: Some("/home/dev/code/app".into()), problems: vec![] }
    }

    #[test]
    fn a_host_reads_ready_only_when_it_has_everything_and_the_conversation_can_move() {
        assert_eq!(tag(None, true), Tag::Checking);
        assert_eq!(tag(Some(&ready()), true), Tag::Ready);
        assert_eq!(tag(Some(&ready()), false), Tag::SummaryOnly);
        let missing = Probe { project: None, problems: vec![Problem::NoProject { repo: "app".into(), origin: None }] };
        assert_eq!(tag(Some(&missing), true), Tag::NotReady);
    }

    #[test]
    fn the_button_checks_again_until_the_host_is_ready_then_hands_off_or_asks_for_a_summary() {
        assert_eq!(primary(Tag::Checking, true), Primary::Waiting);
        assert_eq!(primary(Tag::NotReady, true), Primary::CheckAgain);
        assert_eq!(primary(Tag::Ready, false), Primary::HandOff);
        assert_eq!(primary(Tag::SummaryOnly, true), Primary::ReviewSummary);
        assert_eq!(primary(Tag::SummaryOnly, false), Primary::WriteSummary);
    }

    #[test]
    fn each_step_keeps_its_result_for_the_next() {
        let mut run = Run::new(false, true);
        assert_eq!(run.step(), Some(Step::Pack));
        run.at = 1;
        run.land(Landed::Sent("/b".into()));
        assert_eq!((run.step(), run.bundle.as_deref()), (Some(Step::Worktree), Some("/b")));
        run.at = 4;
        run.land(Landed::Started("devbox/a1".into()));
        assert_eq!(run.step(), Some(Step::Check));
        run.land(Landed::Checked);
        assert_eq!(run.started.as_deref(), Some("devbox/a1"));
        assert_eq!(run.step(), None);
    }

    #[test]
    fn a_failed_worktree_retries_from_sending_the_bundle_again() {
        let mut run = Run::new(true, true);
        run.at = 3;
        assert_eq!(run.step(), Some(Step::Worktree));
        run.failed = Some("no space".into());
        assert!(!run.cancellable());
        run.retry();
        assert_eq!((run.step(), run.failed.as_deref()), (Some(Step::Send), None));
        run.sent = Some((4, 9));
        run.retry();
        assert_eq!(run.sent, None, "a retry sends the conversation again from its start");
        run.at = 5;
        run.retry();
        assert_eq!(run.step(), Some(Step::Start));
    }

    #[test]
    fn cancelling_stops_being_offered_once_the_host_agent_starts() {
        let mut run = Run::new(false, false);
        assert!(run.cancellable());
        run.at = 3;
        assert_eq!(run.step(), Some(Step::Start));
        assert!(!run.cancellable());
    }

    #[test]
    fn the_picked_host_stays_while_connected_else_the_first_one_is_picked() {
        let mut h = Handoff::new(Source::default());
        let online = vec!["devbox".to_string(), "mini".to_string()];
        h.keep_host(&online);
        assert_eq!(h.host.as_deref(), Some("devbox"));
        h.host = Some("mini".into());
        h.keep_host(&online);
        assert_eq!(h.host.as_deref(), Some("mini"));
        h.keep_host(&online[..1]);
        assert_eq!(h.host.as_deref(), Some("devbox"));
        h.keep_host(&[]);
        assert_eq!(h.host, None);
    }

    #[test]
    fn a_session_reads_moving_then_stopped_on_a_failure_and_as_before_once_done() {
        let mut h = Handoff::new(Source { agent: "a1".into(), ..Source::default() });
        h.host = Some("devbox".into());
        assert_eq!(mark(Some(&h), "a1"), None, "picking a host moves nothing yet");
        h.stage = Stage::Moving;
        h.run = Some(Run::new(false, true));
        assert_eq!(mark(Some(&h), "a1"), Some(Mark::Moving("devbox".into())));
        assert_eq!(mark(Some(&h), "a2"), None);
        h.run.as_mut().unwrap().failed = Some("offline".into());
        assert_eq!(mark(Some(&h), "a1"), Some(Mark::Stopped));
        assert_eq!(Mark::Stopped.composer(), "Prompts wait until you retry or close the handoff");
        h.run.as_mut().unwrap().failed = None;
        h.stage = Stage::Done;
        assert_eq!(mark(Some(&h), "a1"), None, "its conversation goes on here once the host has its copy");
    }

    #[test]
    fn a_stopped_run_goes_on_only_when_its_own_host_is_back() {
        let mut h = Handoff::new(Source::default());
        h.host = Some("devbox".into());
        h.run = Some(Run::new(false, true));
        assert!(!h.stopped_on("devbox"), "a run still going needs no retry");
        h.run.as_mut().unwrap().failed = Some("devbox went offline".into());
        assert!(h.stopped_on("devbox"));
        assert!(!h.stopped_on("mac-mini"));
    }

    #[test]
    fn only_a_claude_conversation_with_messages_on_a_host_that_shows_fork_history_is_checked_for_it() {
        let claude = Source { provider: "claude".into(), messages: 3, ..Source::default() };
        let new = [FORK_HISTORY_CAP.to_string()];
        assert!(claude.awaits_history(&new));
        assert!(!claude.awaits_history(&[]), "an older host shows the history only after the first prompt");
        assert!(!Source { provider: "codex".into(), ..claude.clone() }.awaits_history(&new));
        assert!(!Source { messages: 0, ..claude }.awaits_history(&new));
    }

    #[test]
    fn the_host_shows_the_conversation_once_it_opened_and_has_messages_if_they_are_awaited() {
        assert!(!shows_conversation(false, false, 5));
        assert!(shows_conversation(true, false, 0));
        assert!(!shows_conversation(true, true, 0));
        assert!(shows_conversation(true, true, 1));
    }

    #[test]
    fn a_failure_says_the_turn_stopped_only_once_the_run_stopped_it() {
        let mut run = Run::new(true, true);
        assert!(!run.stopped_turn());
        run.land(Landed::Stopped);
        assert!(run.stopped_turn());
        assert!(!Run { at: 2, ..Run::new(false, true) }.stopped_turn());
    }
}
