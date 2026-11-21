//! Handing a session on this Mac off to a connected host: the code, and the conversation where it can move.

mod cheer;
mod view;

use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::util::basename;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use handoff::Step;
use handoff::checks::{Probe, Problem, Wanted};
use handoff::pack::Pack;
use handoff::place::Placed;
use serde_json::json;
use std::collections::{BTreeMap, HashMap};
use std::time::{Duration, Instant};
use store::LaunchPick;

/// How long the agent's turn may take to stop.
const STOP: Duration = Duration::from_secs(20);
/// How long the host's agent may take to show up, then to name its conversation.
const START: Duration = Duration::from_secs(60);
const POLL: Duration = Duration::from_millis(500);
/// How many polls a connected host's pocketd gets to answer before it reads as not ready.
const CONNECT_TRIES: usize = 20;

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
}

impl Run {
    pub fn new(working: bool, conversation: bool) -> Self {
        Self { steps: handoff::steps(working, conversation), at: 0, failed: None, pack: None, bundle: None, placed: None, started: None }
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

/// One handoff, from the sheet opening to its last step.
pub(crate) struct Handoff {
    pub source: Source,
    pub host: Option<String>,
    pub probes: HashMap<String, Probe>,
    pub stage: Stage,
    pub run: Option<Run>,
    /// Bumped by each check, step, retry and cancel, so a result for an older one is dropped.
    pub generation: u64,
    /// Each click on the mascot cheers again.
    pub cheers: usize,
}

impl Handoff {
    pub fn new(source: Source) -> Self {
        Self { source, host: None, probes: HashMap::new(), stage: Stage::Pick, run: None, generation: 0, cheers: 0 }
    }

    pub fn probe(&self) -> Option<&Probe> {
        self.probes.get(self.host.as_deref()?)
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
    /// Its conversation went on at the host, by id.
    Moved(String),
}

impl Mark {
    pub fn host(&self) -> Option<&str> {
        match self {
            Mark::Moving(h) | Mark::Moved(h) => Some(h),
            Mark::Stopped => None,
        }
    }

    /// The line under its card's title, the host called `host`.
    pub fn notice(&self, host: &str) -> ui::Notice {
        let (icon, text, warn) = match self {
            Mark::Moving(_) => ("server", format!("Moving to {host}…"), false),
            Mark::Stopped => ("warning", "Handoff stopped · still here".into(), true),
            Mark::Moved(_) => ("server", format!("Moved to {host}"), false),
        };
        ui::Notice { icon: Some(icon), text: text.into(), warn }
    }

    /// What its tab says under the title, in place of `agent`, its agent's name.
    pub fn tab(&self, agent: &str, host: &str) -> String {
        match self {
            Mark::Moving(_) => format!("{agent} · moving to {host}"),
            Mark::Stopped => agent.to_string(),
            Mark::Moved(_) => format!("Moved to {host}"),
        }
    }
}

/// How session `agent` reads, given the handoff open and the sessions that moved.
pub(crate) fn mark(open: Option<&Handoff>, moved: &BTreeMap<String, store::Moved>, agent: &str) -> Option<Mark> {
    if let Some(m) = moved.get(agent) {
        return Some(Mark::Moved(m.host.clone()));
    }
    let h = open.filter(|h| h.source.agent == agent && h.stage == Stage::Moving)?;
    Some(if h.run.as_ref()?.failed.is_some() { Mark::Stopped } else { Mark::Moving(h.host.clone()?) })
}

pub(crate) struct HandoffState {
    pub open: Option<Handoff>,
    pub summary: Entity<TextareaState>,
}

impl HandoffState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let summary = cx.new(|cx| TextareaState::new(window, cx).placeholder("What were you doing, what's done, what's next?").auto_grow(7, 14));
        let subs = vec![cx.observe(&summary, |_, _, cx| cx.notify())];
        (Self { open: None, summary }, subs)
    }

    /// Whether a handoff of agent `id` is underway.
    pub fn moving(&self, id: &str) -> bool {
        self.open.as_ref().is_some_and(|h| h.source.agent == id && h.stage == Stage::Moving)
    }
}

/// A host's own path for this Mac's `path` in a moved transcript: the path as given, and as it resolves.
fn mac_paths(cwd: &str) -> Vec<String> {
    let mut list = vec![cwd.to_string()];
    if let Some(real) = std::fs::canonicalize(cwd).ok().and_then(|p| p.to_str().map(String::from)).filter(|p| p != cwd) {
        list.push(real);
    }
    list
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

    /// Opens the sheet for agent `id`, or brings back the handoff of it underway.
    pub(crate) fn open_handoff(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.handoff.open.as_ref().is_some_and(|h| h.source.agent == id && h.run.is_some()) {
            return self.open(Overlay::Handoff, window, cx);
        }
        let Some(s) = self.agents.get(id).filter(|s| self.can_hand_off(s)) else { return };
        let agent_name = self.agent_name(&s.provider);
        let repo = self.repos.get(&s.cwd);
        let projects = self.projects();
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
            repo: basename(self.project_of(&s.cwd, &projects).map_or(&s.cwd, |p| p)),
            branch: repo.map(|r| r.branch.clone()).unwrap_or_default(),
            files: repo.map_or(0, |r| r.files.len()),
            added,
            removed,
            commits: repo.map_or(0, |r| r.unpushed.unwrap_or(r.ahead)),
            messages: timeline.iter().filter(|i| i.kind == "user" || i.kind == "assistant").count(),
        };
        let recap = crate::activity::recap::recap(timeline);
        let known = handoff::summary::Known { title: &source.title, branch: &source.branch, files: source.files, asked: Some(recap.asked.as_str()).filter(|a| !a.is_empty()), replied: Some(recap.said.as_str()) };
        let draft = handoff::summary::draft(&known);
        self.handoff.summary.update(cx, |t, cx| t.set_value(draft, window, cx));
        self.handoff.open = Some(Handoff::new(source));
        self.open(Overlay::Handoff, window, cx);
        self.check_hosts(cx);
    }

    /// Checks every connected host for what the handoff needs.
    pub(crate) fn check_hosts(&mut self, cx: &mut Context<Self>) {
        let online = self.online_hosts();
        let projects = self.projects();
        let Some(h) = self.handoff.open.as_mut() else { return };
        h.keep_host(&online);
        h.probes.clear();
        h.generation += 1;
        let generation = h.generation;
        let source = h.source.clone();
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
                let Some(client) = client else { return Probe { project: None, problems: vec![Problem::Offline] } };
                let origin = handoff::pack::origin(&s.cwd);
                let w = Wanted { origin: origin.as_deref(), repo: &s.repo, branch: &s.branch, agent: Some(&s.provider) };
                handoff::checks::probe(&*client, &on_host, &w, &|oid| handoff::pack::holds(&s.cwd, oid))
            });
            cx.spawn(async move |this, cx| {
                let probe = task.await;
                this.update(cx, |d, cx| {
                    if let Some(h) = d.handoff.open.as_mut().filter(|h| h.generation == generation) {
                        h.probes.insert(id, probe);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
        cx.notify();
    }

    pub(crate) fn start_handoff(&mut self, cx: &mut Context<Self>) {
        let Some(h) = self.handoff.open.as_mut() else { return };
        h.run = Some(Run::new(h.source.working, h.source.conversation()));
        h.stage = Stage::Moving;
        self.next_step(cx);
    }

    /// Runs the step the handoff is at, or finishes it.
    fn next_step(&mut self, cx: &mut Context<Self>) {
        let Some(h) = self.handoff.open.as_mut() else { return };
        let (Some(host), Some(run)) = (h.host.clone(), h.run.as_ref()) else { return };
        h.generation += 1;
        let generation = h.generation;
        let Some(step) = run.step() else {
            h.stage = Stage::Done;
            if let Some(agent) = run.started.clone().filter(|_| h.source.conversation()) {
                self.store.moved.insert(h.source.agent.clone(), store::Moved { host, agent });
                self.store.save();
            }
            if self.overlay.is_none() {
                self.overlay = Some(Overlay::Handoff);
            }
            return cx.notify();
        };
        let source = h.source.clone();
        let id = format!("{}-{}", basename(&source.cwd), crate::util::now_ms());
        let Some(client) = hosts::remote::connected(&host) else {
            let name = self.host_name(&host);
            return self.step_failed(generation, format!("{name} went offline"), cx);
        };
        match step {
            Step::Stop => {
                self.outbox.interrupt(&source.agent);
                let agent = source.agent.clone();
                self.wait_for(generation, STOP, "The turn didn’t stop", move |d| d.agents.get(&agent).is_none_or(|s| s.status != "working").then_some(Landed::Stopped), cx);
            }
            Step::Pack => self.in_background(generation, move || handoff::pack::pack(&source.cwd, &id).map(Landed::Packed), cx),
            Step::Send => {
                let Some(pack) = run.pack.clone() else { return };
                self.in_background(generation, move || handoff::place::send(&*client, &pack, &id).map(Landed::Sent), cx);
            }
            Step::Worktree => {
                let (Some(pack), Some(bundle), Some(project)) = (run.pack.clone(), run.bundle.clone(), h.probe().and_then(|p| p.project.clone())) else { return };
                let name = basename(&source.cwd);
                self.in_background(generation, move || handoff::place::worktree(&*client, &project, &source.repo, &name, &bundle, &pack).map(Landed::Placed), cx);
            }
            Step::Conversation => {
                let (Some(placed), Some(session)) = (run.placed.clone(), source.session.clone()) else { return };
                let home = crate::modals::form::home();
                self.in_background(
                    generation,
                    move || {
                        let t = handoff::transcript::find(&source.provider, &session, std::path::Path::new(&home)).ok_or_else(|| "This Mac has no copy of the conversation".to_string())?;
                        let from = mac_paths(&source.cwd);
                        handoff::transcript::carry(&*client, &t, &from.iter().map(String::as_str).collect::<Vec<_>>(), &placed.path).map(|_| Landed::Carried)
                    },
                    cx,
                );
            }
            Step::Start => {
                let Some(placed) = run.placed.clone() else { return };
                let cwd = hosts::place(&host, &placed.path);
                // A retry takes up the agent a slow first try started rather than starting a second.
                if let Some(found) = self.agents.list.iter().find(|s| s.cwd == cwd).map(|s| s.id.clone()) {
                    return self.step_landed(generation, Landed::Started(found), cx);
                }
                let prompt = if source.conversation() { String::new() } else { self.handoff.summary.read(cx).value().trim().to_string() };
                let pick = LaunchPick { provider: source.provider.clone(), model: source.model.clone(), effort: source.effort.clone(), account: crate::accounts::BUILT_IN.into(), ..LaunchPick::default() };
                let mut spec = pick.spec(&hosts::place(&host, &placed.project), json!({"worktree": hosts::place(&host, &placed.path)}), &prompt);
                if let Some(session) = source.session.clone().filter(|_| source.conversation()) {
                    spec["fork"] = session.into();
                }
                self.store.agents.launch(&mut spec, &self.agents, &self.efforts_on(&source.provider, &source.model));
                if self.send_create(spec).is_none() {
                    let name = self.host_name(&host);
                    return self.step_failed(generation, format!("{name} went offline"), cx);
                }
                let failure = format!("{} didn’t start on {}", source.agent_name, self.host_name(&host));
                self.wait_for(generation, START, &failure, move |d| d.agents.list.iter().find(|s| s.cwd == cwd).map(|s| Landed::Started(s.id.clone())), cx);
            }
            Step::Check => {
                let Some(started) = run.started.clone() else { return };
                let failure = format!("The conversation didn’t open on {}", self.host_name(&host));
                self.wait_for(generation, START, &failure, move |d| d.agents.get(&started).and_then(|s| s.provider_session_id.as_ref()).map(|_| Landed::Checked), cx);
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
                    if d.handoff.open.as_ref().is_none_or(|h| h.generation != generation) {
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

    fn step_landed(&mut self, generation: u64, l: Landed, cx: &mut Context<Self>) {
        let Some(run) = self.handoff.open.as_mut().filter(|h| h.generation == generation).and_then(|h| h.run.as_mut()) else { return };
        run.land(l);
        self.next_step(cx);
    }

    fn step_failed(&mut self, generation: u64, message: String, cx: &mut Context<Self>) {
        let Some(run) = self.handoff.open.as_mut().filter(|h| h.generation == generation).and_then(|h| h.run.as_mut()) else { return };
        run.failed = Some(message);
        if self.overlay.is_none() {
            self.overlay = Some(Overlay::Handoff);
        }
        cx.notify();
    }

    pub(crate) fn retry_handoff(&mut self, cx: &mut Context<Self>) {
        if let Some(run) = self.handoff.open.as_mut().and_then(|h| h.run.as_mut()) {
            run.retry();
            self.next_step(cx);
        }
    }

    /// Stops the handoff and takes back the worktree it made on the host, unless the host's agent started in it.
    pub(crate) fn cancel_handoff(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(h) = self.handoff.open.take() {
            self.undo_handoff(h, cx);
        }
        self.close_overlay(window, cx);
    }

    fn undo_handoff(&mut self, h: Handoff, cx: &mut Context<Self>) {
        let (Some(host), Some(run)) = (h.host, h.run) else { return };
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
        let underway = self.handoff.open.as_ref().is_some_and(|h| h.stage == Stage::Moving && h.run.as_ref().is_some_and(|r| r.failed.is_none()));
        if !underway && let Some(h) = self.handoff.open.take() {
            let failed = h.run.as_ref().is_some_and(|r| r.failed.is_some());
            if failed {
                self.undo_handoff(h, cx);
            }
        }
    }

    /// Goes to the session that took over on the host.
    pub(crate) fn open_handed_off(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let started = self.handoff.open.take().and_then(|h| h.run).and_then(|r| r.started);
        self.close_overlay(window, cx);
        if let Some(id) = started {
            self.focus_agent(&id, window, cx);
        }
    }

    /// What session `agent` shows of a handoff of it.
    pub(crate) fn handoff_mark(&self, agent: &str) -> Option<Mark> {
        mark(self.handoff.open.as_ref(), &self.store.moved, agent)
    }

    /// What session `agent`'s card says of a handoff of it.
    pub(crate) fn handoff_notice(&self, agent: &str) -> Option<ui::Notice> {
        self.handoff_mark(agent).map(|m| m.notice(&self.mark_host(&m)))
    }

    /// What session `agent`'s tab says under its title, `name` being its agent's.
    pub(crate) fn handoff_tab(&self, agent: &str, name: &str) -> Option<String> {
        self.handoff_mark(agent).map(|m| m.tab(name, &self.mark_host(&m)))
    }

    fn mark_host(&self, m: &Mark) -> String {
        m.host().map(|h| self.host_name(h)).unwrap_or_default()
    }

    /// Goes to the session that took over from `agent` on its host.
    pub(crate) fn open_moved(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(m) = self.store.moved.get(agent).cloned() {
            self.focus_agent(&m.agent, window, cx);
        }
    }

    /// A shell on host `id` for fixing what the check found, in its checkout of the project, else any project there.
    pub(crate) fn open_host_shell(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let found = self.handoff.open.as_ref().and_then(|h| h.probes.get(id)).and_then(|p| p.project.clone()).map(|p| hosts::place(id, &p));
        let Some(project) = found.or_else(|| self.projects().into_iter().find(|p| hosts::split(p).0 == Some(id))) else { return };
        self.close_overlay(window, cx);
        self.select_tree(project, None, cx);
        self.new_shell(workspace::Place::Pane(None), cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{Handoff, Landed, Mark, Primary, Run, Source, Stage, Tag, mark, primary, tag};
    use std::collections::BTreeMap;
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
    fn a_session_reads_moving_then_stopped_on_a_failure_and_moved_once_recorded() {
        let mut h = Handoff::new(Source { agent: "a1".into(), ..Source::default() });
        h.host = Some("devbox".into());
        let none = BTreeMap::new();
        assert_eq!(mark(Some(&h), &none, "a1"), None, "picking a host moves nothing yet");
        h.stage = Stage::Moving;
        h.run = Some(Run::new(false, true));
        assert_eq!(mark(Some(&h), &none, "a1"), Some(Mark::Moving("devbox".into())));
        assert_eq!(mark(Some(&h), &none, "a2"), None);
        h.run.as_mut().unwrap().failed = Some("offline".into());
        assert_eq!(mark(Some(&h), &none, "a1"), Some(Mark::Stopped));
        let moved = BTreeMap::from([("a1".to_string(), store::Moved { host: "devbox".into(), agent: "devbox/b1".into() })]);
        assert_eq!(mark(None, &moved, "a1"), Some(Mark::Moved("devbox".into())));
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
