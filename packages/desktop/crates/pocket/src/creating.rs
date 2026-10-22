use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::util::basename;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::{Value, json};
use std::cmp::Ordering;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use theme::*;

/// A create's steps in the order pocketd runs them.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Step {
    Prepare,
    Verify,
    Fetch,
    Worktree,
    Copy,
    Setup,
    Agent,
}

impl Step {
    const ALL: [Step; 7] = [Step::Prepare, Step::Verify, Step::Fetch, Step::Worktree, Step::Copy, Step::Setup, Step::Agent];

    fn wire(self) -> &'static str {
        match self {
            Step::Prepare => "prepare",
            Step::Verify => "verify",
            Step::Fetch => "fetch",
            Step::Worktree => "worktree",
            Step::Copy => "copy",
            Step::Setup => "setup",
            Step::Agent => "agent",
        }
    }

    pub(crate) fn label(self) -> &'static str {
        match self {
            Step::Prepare => "Preparing",
            Step::Verify => "Checking branch name",
            Step::Fetch => "Fetching latest changes",
            Step::Worktree => "Creating git worktree",
            Step::Copy => "Copying configuration",
            Step::Setup => "Setting up",
            Step::Agent => "Starting agent",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Mark {
    Done,
    Active,
    Failed,
    Pending,
}

struct Line {
    step: Step,
    took: Option<Duration>,
    note: String,
}

/// A worktree this window asked pocketd to make, from Send until its agent starts.
pub(crate) struct Create {
    pub(crate) request: String,
    /// Where the worktree goes, until pocketd says where it went.
    pub(crate) path: String,
    spec: Value,
    lines: Vec<Line>,
    at: usize,
    started: Instant,
    step_started: Instant,
    ended: Option<Instant>,
    /// The agent pocketd started, once the create is done.
    agent: Option<String>,
    pub(crate) failed: Option<(String, String)>,
    /// The worktree's terminal is on screen instead.
    pub(crate) output: bool,
}

impl Create {
    pub(crate) fn new(request: String, path: String, spec: Value, now: Instant) -> Self {
        let new = &spec["checkout"]["new"];
        let steps = if new.is_object() {
            let off = |k: &str| new[k].as_bool() == Some(false);
            Step::ALL.into_iter().filter(|s| !(*s == Step::Copy && off("copy") || *s == Step::Setup && off("setup"))).collect()
        } else {
            vec![Step::Prepare, Step::Agent]
        };
        let lines = steps.into_iter().map(|step| Line { step, took: None, note: String::new() }).collect();
        Self { request, path, spec, lines, at: 0, started: now, step_started: now, ended: None, agent: None, failed: None, output: false }
    }

    pub(crate) fn project(&self) -> &str {
        self.spec["project"].as_str().unwrap_or_default()
    }

    pub(crate) fn name(&self) -> String {
        basename(&self.path)
    }

    fn worktree(&self) -> git::Worktree {
        git::Worktree { path: self.path.clone(), branch: self.name(), main: false }
    }

    /// The spec's `checkout.new`: what the sheet asked for.
    pub(crate) fn asked(&self) -> &Value {
        &self.spec["checkout"]["new"]
    }

    pub(crate) fn base(&self) -> &str {
        self.spec["checkout"]["new"]["base"].as_str().unwrap_or_default()
    }

    pub(crate) fn prompt(&self) -> &str {
        self.spec["prompt"].as_str().unwrap_or_default()
    }

    /// pocketd sends each step it runs as it starts, so the ones before `step` it didn't send were skipped.
    pub(crate) fn reach(&mut self, step: &str, note: &str, now: Instant) {
        let Some(step) = Step::ALL.into_iter().find(|s| s.wire() == step) else { return };
        let mut i = match self.lines.iter().position(|l| l.step == step) {
            Some(i) => i,
            None => {
                let i = self.lines.iter().position(|l| l.step > step).unwrap_or(self.lines.len());
                self.lines.insert(i, Line { step, took: None, note: String::new() });
                i
            }
        };
        if i > self.at {
            self.lines[self.at].took = Some(now - self.step_started);
            self.lines.drain(self.at + 1..i);
            (self.at, self.step_started) = (self.at + 1, now);
            i = self.at;
        }
        if !note.is_empty() {
            self.lines[i].note = note.to_string();
        }
    }

    pub(crate) fn finish(&mut self, agent: String, now: Instant) {
        if let Some(l) = self.lines.get_mut(self.at) {
            l.took = Some(now - self.step_started);
        }
        (self.at, self.ended, self.agent) = (self.lines.len(), Some(now), Some(agent));
    }

    /// How far the page has gone, once a finished create holds a beat and leaves.
    fn leaving(&self, now: Instant, reduce_motion: bool) -> Option<f32> {
        let since = now - self.ended.filter(|_| self.agent.is_some())?;
        let t = since.saturating_sub(HOLD).as_secs_f32() / LEAVE.as_secs_f32();
        Some(if reduce_motion && since >= HOLD { 1. } else { t.min(1.) })
    }

    pub(crate) fn fail(&mut self, message: String, detail: String, now: Instant) {
        (self.failed, self.ended) = (Some((message, detail)), Some(now));
    }

    pub(crate) fn steps(&self) -> impl Iterator<Item = (Step, Mark, Option<Duration>, &str)> {
        self.lines.iter().enumerate().map(|(i, l)| {
            let mark = match i.cmp(&self.at) {
                Ordering::Less => Mark::Done,
                Ordering::Equal if self.failed.is_some() => Mark::Failed,
                Ordering::Equal => Mark::Active,
                Ordering::Greater => Mark::Pending,
            };
            (l.step, mark, l.took, l.note.as_str())
        })
    }

    pub(crate) fn elapsed(&self, now: Instant) -> Duration {
        self.ended.unwrap_or(now) - self.started
    }

    /// Whether git has made the worktree.
    pub(crate) fn made(&self) -> bool {
        self.lines.iter().position(|l| l.step == Step::Worktree).is_none_or(|w| self.at > w)
    }

    /// The spec a retry sends: once the worktree exists, asking for it again would fail as taken.
    pub(crate) fn again(&self) -> Value {
        let mut spec = self.spec.clone();
        if self.made() {
            spec["checkout"] = json!({"worktree": self.path});
        }
        spec
    }
}

pub(crate) fn clock(d: Duration) -> String {
    let s = d.as_secs();
    format!("{}:{:02}", s / 60, s % 60)
}

fn took(d: Duration) -> String {
    if d.as_secs() < 60 { format!("{:.1}s", d.as_secs_f32()) } else { clock(d) }
}

const RISE: Duration = Duration::from_millis(240);
/// A finished create's page stays this long, so its last check reads, before it leaves.
const HOLD: Duration = Duration::from_millis(260);
const LEAVE: Duration = Duration::from_millis(150);
/// How long a finished create waits for pocketd to list its agent before its page goes anyway.
const UNLISTED: Duration = Duration::from_secs(2);
/// A handed-off prompt shows as sending this long; by then the transcript holds it.
const FRESH: Duration = Duration::from_secs(10);

/// When a part starts to rise, in ms, and from how far below.
pub(crate) type Part = (f32, f32);
pub(crate) const BAR: Part = (0., 4.);
pub(crate) const BUBBLE: Part = (40., 10.);
pub(crate) const DOCK: Part = (60., 14.);
pub(crate) const THINKING: Part = (140., 8.);
const TITLE: Part = (0., 8.);
const BRANCH: Part = (40., 8.);
const STEPS: Part = (80., 8.);
/// The first step row; each next one rises `STAGGER` ms after the one above it.
const STEP: Part = (120., 8.);
const STAGGER: f32 = 30.;

/// A part's opacity and how far it still sits below its place.
#[derive(Debug, PartialEq)]
pub(crate) struct Rise {
    opacity: f32,
    offset: f32,
}

impl Rise {
    pub(crate) fn on(&self, el: Div) -> Div {
        el.relative().top(px(self.offset)).opacity(self.opacity)
    }
}

/// `part` `since` its page opened, while it still moves; reduced motion fades it in, all at once.
pub(crate) fn rise(since: Duration, (delay, by): Part, reduce_motion: bool) -> Option<Rise> {
    let ms = since.as_secs_f32() * 1000.;
    if reduce_motion {
        return (ms < 150.).then(|| Rise { opacity: ms / 150., offset: 0. });
    }
    let t = ((ms - delay) / RISE.as_secs_f32() / 1000.).clamp(0., 1.);
    let t = (ms < delay + RISE.as_secs_f32() * 1000.).then(|| ease_out_quint()(t))?;
    Some(Rise { opacity: t, offset: by * (1. - t) })
}

/// A prompt sent with a new session, as its chat opens.
struct Arrival {
    at: Instant,
    prompt: String,
}

#[derive(Default)]
pub(crate) struct Creates {
    pub(crate) list: Vec<Create>,
    /// A timer redraws the clock while a running create is on screen.
    ticking: bool,
    /// The chats finished creates handed off to, by agent.
    arrivals: HashMap<String, Arrival>,
    /// Prompts of sessions started in place, which have no create page, by request until pocketd makes their terminal.
    asked: HashMap<String, String>,
    /// Those prompts once pocketd made their terminal, by terminal: the agent is listed before it has the prompt.
    opening: HashMap<String, Arrival>,
}

impl Creates {
    pub(crate) fn get(&mut self, request: &str) -> Option<&mut Create> {
        self.list.iter_mut().find(|c| c.request == request)
    }

    pub(crate) fn remove(&mut self, request: &str) -> Option<Create> {
        let i = self.list.iter().position(|c| c.request == request)?;
        Some(self.list.remove(i))
    }

    /// The create whose progress shows for worktree `tree`.
    pub(crate) fn shown(&self, tree: &str) -> Option<&Create> {
        self.list.iter().find(|c| c.path == tree && !c.output)
    }

    pub(crate) fn show_progress(&mut self, tree: &str) {
        for c in self.list.iter_mut().filter(|c| c.path == tree) {
            c.output = false;
        }
    }

    /// pocketd made this window's create in `cwd`; returns where the create expected it. `cwd` joins its project's
    /// `worktrees` now: the create's terminal arrives before git lists it, and would be taken for a project of its own.
    pub(crate) fn started(&mut self, request: &str, cwd: String, worktrees: &mut HashMap<String, Vec<git::Worktree>>) -> Option<String> {
        let c = self.get(request)?;
        let expected = std::mem::replace(&mut c.path, cwd);
        let listed = worktrees.entry(c.project().to_string()).or_default();
        if !listed.iter().any(|w| w.path == c.path) {
            listed.push(c.worktree());
        }
        Some(expected)
    }

    /// `listed`, then the worktrees of `project` being made that git doesn't list yet.
    pub(crate) fn trees(&self, project: &str, listed: &[git::Worktree]) -> Vec<git::Worktree> {
        let making = self.list.iter().filter(|c| c.project() == project && !listed.iter().any(|w| w.path == c.path));
        listed.iter().cloned().chain(making.map(Create::worktree)).collect()
    }

    /// `setups`, plus each running create keyed by its request.
    pub(crate) fn setups(&self, setups: &HashMap<String, String>) -> HashMap<String, String> {
        let running = self.list.iter().filter(|c| c.failed.is_none()).map(|c| (c.request.clone(), c.path.clone()));
        setups.clone().into_iter().chain(running).collect()
    }

    /// Hands each finished create to its chat once its page has left and pocketd lists its agent; says whether any went.
    fn hand_off(&mut self, listed: impl Fn(&str) -> bool, now: Instant) -> bool {
        let done = |c: &Create| {
            let since = c.ended.map_or(Duration::ZERO, |e| now - e);
            c.agent.as_deref().is_some_and(|a| since >= HOLD + LEAVE && (listed(a) || since > UNLISTED))
        };
        let (done, going): (Vec<_>, Vec<_>) = std::mem::take(&mut self.list).into_iter().partition(done);
        self.list = going;
        self.arrivals.retain(|_, a| now - a.at < FRESH);
        for c in &done {
            self.arrivals.insert(c.agent.clone().unwrap_or_default(), Arrival { at: now, prompt: c.prompt().to_string() });
        }
        !done.is_empty()
    }

    /// A session started in place sent `prompt`; its chat shows it as sending from when pocketd makes its terminal.
    pub(crate) fn sent_in_place(&mut self, request: String, prompt: &str) {
        if !prompt.is_empty() {
            self.asked.insert(request, prompt.to_string());
        }
    }

    pub(crate) fn made_terminal(&mut self, request: &str, terminal: String, now: Instant) {
        self.opening.retain(|_, a| now - a.at < FRESH);
        if let Some(prompt) = self.asked.remove(request) {
            self.opening.insert(terminal, Arrival { at: now, prompt });
        }
    }

    pub(crate) fn failed_in_place(&mut self, request: &str) {
        self.asked.remove(request);
    }

    /// The prompt to show as sending when `agent`'s chat in `terminal` first opens; not once the transcript should hold it, or it would never settle.
    pub(crate) fn unsent(&self, agent: &str, terminal: &str, now: Instant) -> Option<&str> {
        let a = self.arrivals.get(agent).or_else(|| self.opening.get(terminal))?;
        (!a.prompt.is_empty() && now - a.at < FRESH).then_some(a.prompt.as_str())
    }

    /// How far `part` of `agent`'s chat has risen, while it still moves.
    pub(crate) fn arriving(&self, agent: &str, part: Part, now: Instant, reduce_motion: bool) -> Option<Rise> {
        rise(now - self.arrivals.get(agent)?.at, part, reduce_motion)
    }

    pub(crate) fn failed(&self, tree: &str) -> bool {
        self.list.iter().any(|c| c.path == tree && c.failed.is_some())
    }

    /// Forgets a create; its worktree stays `selected` only if git made it.
    pub(crate) fn drop(&mut self, request: &str, selected: &mut Option<String>) -> Option<Create> {
        let c = self.remove(request)?;
        if !c.made() && selected.as_ref() == Some(&c.path) {
            *selected = None;
        }
        Some(c)
    }
}

impl Desktop {
    /// The create on screen, unless its worktree's terminal is.
    pub(crate) fn shown_create(&self) -> Option<&Create> {
        if self.terminals.link.is_down() || self.screen != Screen::Sessions {
            return None;
        }
        self.creates.shown(&self.place()?)
    }

    /// Hands finished creates to their chats; says whether any went.
    pub(crate) fn hand_off_creates(&mut self) -> bool {
        let agents = &self.agents;
        self.creates.hand_off(|a| agents.get(a).is_some(), Instant::now())
    }

    /// `el`, rising into `agent`'s chat just opened from its create.
    pub(crate) fn arriving(&self, agent: &str, part: Part, el: Div, window: &mut Window, cx: &App) -> Div {
        match self.creates.arriving(agent, part, Instant::now(), cx.reduce_motion()) {
            Some(r) => {
                window.request_animation_frame();
                r.on(el)
            }
            None => el,
        }
    }

    fn tick_create(&mut self, cx: &mut Context<Self>) {
        if self.creates.ticking || !self.shown_create().is_some_and(|c| c.failed.is_none()) {
            return;
        }
        self.creates.ticking = true;
        cx.spawn(async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let going = this.update(cx, |d, cx| {
                    let handed = d.hand_off_creates();
                    d.creates.ticking = d.shown_create().is_some_and(|c| c.failed.is_none());
                    if d.creates.ticking || handed {
                        cx.notify();
                    }
                    d.creates.ticking
                });
                if !going.unwrap_or(false) {
                    break;
                }
            }
        })
        .detach();
    }

    pub(crate) fn retry_create(&mut self, request: &str, cx: &mut Context<Self>) {
        let Some(spec) = self.creates.get(request).map(|c| c.again()) else { return };
        let sent = self.send_create(spec.clone());
        if let (Some(sent), Some(c)) = (sent, self.creates.get(request)) {
            *c = Create::new(sent, c.path.clone(), spec, Instant::now());
        }
        cx.notify();
    }

    pub(crate) fn edit_create(&mut self, request: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = self.creates.drop(request, &mut self.worktree) {
            self.reopen_new_worktree(&c, window, cx);
        }
    }

    pub(crate) fn creating_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if self.hand_off_creates() {
            window.request_animation_frame();
        }
        self.tick_create(cx);
        let Some(c) = self.shown_create() else { return div() };
        let now = Instant::now();
        let reduce = cx.reduce_motion();
        let mut moving = false;
        let mut rising = |el: Div, part: Part| match rise(now - c.started, part, reduce) {
            Some(r) => {
                moving = true;
                r.on(el)
            }
            None => el,
        };
        let bar = self.page_bar(vec![self.repo_name(c.project()), c.name()], Vec::new(), div(), cx);
        let mono = |s: String| div().font_family(MONO).text_color(TEXT_2).child(s);
        let title = div()
            .flex()
            .items_center()
            .child(div().text_size(px(17.)).font_weight(FontWeight::BOLD).child(match (&c.failed, c.made()) {
                (None, _) => "Creating worktree",
                (Some(_), false) => "Couldn't create worktree",
                (Some(_), true) => "Couldn't start the session",
            }))
            .child(div().ml_auto().font_family(MONO).text_size(px(13.)).text_color(TEXT_3).child(clock(c.elapsed(now))));
        let branch = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(13.))
            .text_color(TEXT_3)
            .child(icon("branch", 13., TEXT_3))
            .child(mono(c.name()))
            .when(!c.base().is_empty(), |d| d.child("from").child(mono(c.base().to_string())));
        let (title, branch) = (rising(title, TITLE), rising(branch, BRANCH));
        let rows: Vec<_> = c.steps().enumerate().map(|(i, (step, mark, spent, note))| {
            let lead = match mark {
                Mark::Done => icon("check", 14., SUCCESS).into_any_element(),
                Mark::Active => spinner(("create-step", step as usize), 14., TEXT_3).into_any_element(),
                Mark::Failed => icon("x-bold", 12., FAILED).into_any_element(),
                Mark::Pending => div().size(px(6.)).rounded_full().bg(TEXT_5).into_any_element(),
            };
            let row = div()
                .flex()
                .gap(px(10.))
                .py(px(7.))
                .child(div().size(px(16.)).flex().flex_none().items_center().justify_center().child(lead))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(3.))
                        .child(div().text_size(px(13.5)).text_color(if mark == Mark::Pending { TEXT_4 } else { TEXT }).child(step.label()))
                        .when(!note.is_empty(), |d| d.child(div().text_size(px(12.)).text_color(TEXT_3).child(note.to_string()))),
                )
                .children(spent.map(|t| div().font_family(MONO).text_size(px(12.)).text_color(TEXT_4).child(took(t))));
            rising(row, (STEP.0 + STAGGER * i as f32, STEP.1))
        }).collect();
        let steps = div()
            .flex()
            .flex_col()
            .px(px(14.))
            .py(px(6.))
            .rounded(px(14.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(rgba(0x1111130a), 1., 2.)])
            .children(rows);
        let steps = rising(steps, STEPS);
        let error = c.failed.clone().map(|(message, detail)| {
            div()
                .flex()
                .flex_col()
                .gap(px(4.))
                .px(px(12.))
                .py(px(10.))
                .rounded(px(10.))
                .bg(FAILED_BG)
                .text_size(px(13.))
                .text_color(FAILED_TEXT)
                .child(message)
                .when(!detail.is_empty(), |d| d.child(div().font_family(MONO).text_size(px(12.)).text_color(TEXT_2).child(detail)))
        });
        let r = c.request.clone();
        let output = self.workspaces.get(&c.path).is_some_and(|w| w.tree.panes().iter().any(|p| !p.tabs.is_empty())).then(|| {
            let r = r.clone();
            ui::link("create-output", "View output").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if let Some(c) = this.creates.get(&r) {
                    c.output = true;
                }
                cx.notify();
            }))
        });
        let actions = c.failed.is_some().then(|| {
            let (retry, edit, dismiss) = (r.clone(), r.clone(), r.clone());
            div()
                .ml_auto()
                .flex()
                .gap(px(8.))
                .child(ui::button("create-dismiss", ui::Variant::Ghost, None, "Dismiss").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.creates.drop(&dismiss, &mut this.worktree);
                    cx.notify();
                })))
                .when(!c.made(), |d| {
                    d.child(ui::button("create-edit", ui::Variant::Secondary, None, "Edit").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.edit_create(&edit, window, cx))))
                })
                .child(ui::button("create-retry", ui::Variant::Primary, None, "Try again").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.retry_create(&retry, cx))))
        });
        let footer = div().flex().items_center().min_h(px(32.)).children(output).children(actions);
        let gone = c.leaving(now, reduce).map(|t| ease_out_quint()(t));
        if moving || gone.is_some_and(|t| t < 1.) {
            window.request_animation_frame();
        }
        let page = div().w(px(440.)).flex().flex_col().gap(px(12.)).child(title).child(branch).child(steps).children(error).child(footer);
        let page = match gone {
            Some(t) => Rise { opacity: 1. - t, offset: -6. * t }.on(page),
            None => page,
        };
        div().flex_1().flex().flex_col().child(bar).child(div().flex_1().flex().flex_col().items_center().justify_center().child(page))
    }
}

#[cfg(test)]
mod tests {
    use super::{BUBBLE, Create, Creates, Mark, Rise, Step, clock, rise};
    use serde_json::json;
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    fn spec(copy: bool, setup: bool) -> serde_json::Value {
        json!({"project": "/p", "checkout": {"new": {"name": "fix", "base": "main", "copy": copy, "setup": setup}}, "provider": "claude", "access": "ask", "prompt": "Fix CI"})
    }

    fn create(copy: bool, setup: bool) -> (Create, Instant) {
        let t0 = Instant::now();
        (Create::new("r1".into(), "/w/fix".into(), spec(copy, setup), t0), t0)
    }

    fn marks(c: &Create) -> Vec<(Step, Mark)> {
        c.steps().map(|(s, m, _, _)| (s, m)).collect()
    }

    #[test]
    fn a_new_worktree_lists_only_the_steps_it_asked_for() {
        let steps = |c: Create| c.steps().map(|(s, ..)| s).collect::<Vec<_>>();
        assert_eq!(steps(create(false, true).0), [Step::Prepare, Step::Verify, Step::Fetch, Step::Worktree, Step::Setup, Step::Agent]);
        let existing = json!({"project": "/p", "checkout": {"worktree": "/w/fix"}, "provider": "claude", "access": "ask"});
        assert_eq!(steps(Create::new("r1".into(), "/w/fix".into(), existing, Instant::now())), [Step::Prepare, Step::Agent]);
    }

    #[test]
    fn reaching_a_step_finishes_the_ones_before_it_and_times_them() {
        let (mut c, t0) = create(true, true);
        c.reach("prepare", "", t0);
        c.reach("verify", "", t0 + Duration::from_millis(200));
        c.reach("fetch", "", t0 + Duration::from_millis(500));
        let took: Vec<_> = c.steps().take(3).map(|(_, m, t, _)| (m, t)).collect();
        assert_eq!(took, [(Mark::Done, Some(Duration::from_millis(200))), (Mark::Done, Some(Duration::from_millis(300))), (Mark::Active, None)]);
        assert_eq!(marks(&c)[3], (Step::Worktree, Mark::Pending));
    }

    #[test]
    fn a_note_lands_on_its_step_even_once_past_it() {
        let (mut c, t0) = create(false, false);
        c.reach("verify", "", t0);
        c.reach("fetch", "", t0);
        c.reach("worktree", "", t0);
        c.reach("fetch", "Couldn't fetch, using local main", t0);
        let notes: Vec<_> = c.steps().map(|(s, _, _, n)| (s, n.to_string())).filter(|(_, n)| !n.is_empty()).collect();
        assert_eq!(notes, [(Step::Fetch, "Couldn't fetch, using local main".to_string())]);
        assert_eq!(marks(&c)[3], (Step::Worktree, Mark::Active));
    }

    #[test]
    fn a_step_pocketd_skips_drops_off_the_list_and_one_it_adds_joins_it() {
        let (mut c, t0) = create(false, true);
        c.reach("worktree", "", t0);
        c.reach("agent", "", t0);
        assert!(!marks(&c).iter().any(|(s, _)| *s == Step::Setup));
        let (mut c, t0) = create(false, false);
        for step in ["verify", "fetch", "worktree", "setup"] {
            c.reach(step, "", t0);
        }
        assert_eq!(marks(&c)[4..], [(Step::Setup, Mark::Active), (Step::Agent, Mark::Pending)]);
        c.reach("bogus", "", t0);
        assert_eq!(marks(&c).len(), 6);
    }

    #[test]
    fn a_failure_marks_its_step_and_stops_the_clock() {
        let (mut c, t0) = create(false, true);
        c.reach("setup", "", t0 + Duration::from_secs(2));
        c.fail("Setup exited 1".into(), "npm ERR!".into(), t0 + Duration::from_secs(65));
        assert_eq!(marks(&c).last(), Some(&(Step::Agent, Mark::Pending)));
        assert!(marks(&c).contains(&(Step::Setup, Mark::Failed)));
        assert_eq!(clock(c.elapsed(t0 + Duration::from_secs(300))), "1:05");
    }

    #[test]
    fn a_retry_makes_the_worktree_again_only_if_git_never_made_it() {
        let (mut c, t0) = create(true, true);
        c.reach("worktree", "", t0);
        assert!(!c.made());
        assert_eq!(c.again(), spec(true, true));
        c.reach("setup", "", t0);
        let again = c.again();
        assert!(c.made());
        assert_eq!((&again["checkout"], &again["prompt"]), (&json!({"worktree": "/w/fix"}), &json!("Fix CI")));
    }

    #[test]
    fn a_worktree_being_made_joins_its_project_until_git_lists_it() {
        let mut creates = Creates::default();
        creates.list.push(create(false, false).0);
        let main = git::Worktree { path: "/p".into(), branch: "main".into(), main: true };
        let paths = |t: Vec<git::Worktree>| t.into_iter().map(|w| (w.path, w.main)).collect::<Vec<_>>();
        assert_eq!(paths(creates.trees("/p", std::slice::from_ref(&main))), [("/p".to_string(), true), ("/w/fix".to_string(), false)]);
        assert_eq!(paths(creates.trees("/q", &[])), []);
        assert_eq!(creates.started("r1", "/private/w/fix".into(), &mut HashMap::new()), Some("/w/fix".to_string()));
        let listed = [main, git::Worktree { path: "/private/w/fix".into(), branch: "fix".into(), main: false }];
        assert_eq!(creates.trees("/p", &listed).len(), 2);
    }

    #[test]
    fn a_started_create_lists_its_worktree_in_its_project_once() {
        let mut creates = Creates::default();
        creates.list.push(create(false, false).0);
        let main = git::Worktree { path: "/p".into(), branch: "main".into(), main: true };
        let mut worktrees = HashMap::from([("/p".to_string(), vec![main])]);
        creates.started("r1", "/private/w/fix".into(), &mut worktrees);
        creates.started("r1", "/private/w/fix".into(), &mut worktrees);
        let paths: Vec<_> = worktrees["/p"].iter().map(|w| (w.path.as_str(), w.branch.as_str(), w.main)).collect();
        assert_eq!(paths, [("/p", "main", true), ("/private/w/fix", "fix", false)]);
    }

    #[test]
    fn a_running_create_sets_up_its_worktree_and_a_failed_one_is_marked_failed() {
        let mut creates = Creates::default();
        creates.list.push(create(false, false).0);
        let setups: HashMap<String, String> = [("t1".to_string(), "/w/other".to_string())].into();
        assert_eq!(creates.setups(&setups).len(), 2);
        assert!(!creates.failed("/w/fix"));
        creates.get("r1").unwrap().fail("No".into(), String::new(), Instant::now());
        assert_eq!(creates.setups(&setups), setups);
        assert!(creates.failed("/w/fix"));
    }

    #[test]
    fn viewing_the_output_hides_the_progress_until_its_row_is_picked() {
        let mut creates = Creates::default();
        creates.list.push(create(false, false).0);
        creates.get("r1").unwrap().output = true;
        assert!(creates.shown("/w/fix").is_none());
        creates.show_progress("/w/fix");
        assert!(creates.shown("/w/fix").is_some());
    }

    #[test]
    fn dropping_a_create_deselects_its_worktree_only_if_git_never_made_it() {
        let mut creates = Creates::default();
        let mut selected = Some("/w/fix".to_string());
        creates.list.push(create(false, false).0);
        creates.drop("r1", &mut selected);
        assert_eq!(selected, None);
        let (mut made, t0) = create(false, false);
        for step in ["prepare", "verify", "fetch", "worktree", "agent"] {
            made.reach(step, "", t0);
        }
        creates.list.push(made);
        selected = Some("/w/fix".to_string());
        creates.drop("r1", &mut selected);
        assert_eq!(selected.as_deref(), Some("/w/fix"));
        assert!(creates.list.is_empty());
    }

    #[test]
    fn finishing_marks_every_step_done_and_stops_the_clock() {
        let (mut c, t0) = create(false, false);
        c.reach("verify", "", t0 + Duration::from_secs(1));
        c.finish("a1".into(), t0 + Duration::from_secs(3));
        assert!(marks(&c).iter().all(|(_, m)| *m == Mark::Done));
        assert_eq!(c.elapsed(t0 + Duration::from_secs(9)), Duration::from_secs(3));
    }

    #[test]
    fn a_finished_page_holds_a_beat_then_leaves() {
        let (mut c, t0) = create(false, false);
        assert_eq!(c.leaving(t0, false), None);
        c.finish("a1".into(), t0);
        let at = |ms| c.leaving(t0 + Duration::from_millis(ms), false);
        assert_eq!((at(200), at(335), at(500)), (Some(0.), Some(0.5), Some(1.)));
        assert_eq!(c.leaving(t0 + Duration::from_millis(260), true), Some(1.));
    }

    #[test]
    fn a_finished_create_hands_off_once_its_page_left_and_its_agent_is_listed_or_pocketd_is_slow_to_list_it() {
        let (mut c, t0) = create(false, false);
        c.finish("a1".into(), t0);
        let mut creates = Creates { list: vec![c], ..Default::default() };
        assert!(!creates.hand_off(|_| true, t0 + Duration::from_millis(300)));
        assert!(!creates.hand_off(|_| false, t0 + Duration::from_secs(1)));
        assert!(creates.hand_off(|a| a == "a1", t0 + Duration::from_secs(1)));
        assert!(creates.list.is_empty() && creates.unsent("a1", "t1", t0 + Duration::from_secs(1)) == Some("Fix CI"));

        let (mut c, t0) = create(false, false);
        c.finish("a2".into(), t0);
        creates.list.push(c);
        assert!(creates.hand_off(|_| false, t0 + Duration::from_secs(3)));
    }

    #[test]
    fn a_create_still_going_never_hands_off() {
        let (c, t0) = create(false, false);
        let mut creates = Creates { list: vec![c], ..Default::default() };
        assert!(!creates.hand_off(|_| true, t0 + Duration::from_secs(60)));
        assert_eq!(creates.list.len(), 1);
    }

    #[test]
    fn a_handed_off_prompt_shows_as_sending_only_while_fresh() {
        let (mut c, t0) = create(false, false);
        c.finish("a1".into(), t0);
        let mut creates = Creates { list: vec![c], ..Default::default() };
        creates.hand_off(|_| true, t0 + Duration::from_secs(1));
        assert_eq!(creates.unsent("a1", "t1", t0 + Duration::from_secs(2)), Some("Fix CI"));
        assert_eq!(creates.unsent("a1", "t1", t0 + Duration::from_secs(30)), None);
        assert_eq!(creates.unsent("other", "t1", t0), None);
    }

    #[test]
    fn a_prompt_sent_in_place_shows_as_sending_from_when_its_terminal_is_made() {
        let (mut creates, t0) = (Creates::default(), Instant::now());
        creates.sent_in_place("r1".into(), "Fix CI");
        assert_eq!(creates.unsent("a1", "t1", t0), None);
        creates.made_terminal("r1", "t1".into(), t0);
        assert_eq!(creates.unsent("a1", "t1", t0 + Duration::from_secs(2)), Some("Fix CI"));
        assert_eq!(creates.unsent("a1", "t1", t0 + Duration::from_secs(30)), None);
        assert_eq!(creates.unsent("a1", "t2", t0), None);
    }

    #[test]
    fn a_session_that_failed_to_start_in_place_forgets_its_prompt() {
        let (mut creates, t0) = (Creates::default(), Instant::now());
        creates.sent_in_place("r1".into(), "Fix CI");
        creates.failed_in_place("r1");
        creates.made_terminal("r1", "t1".into(), t0);
        assert_eq!(creates.unsent("a1", "t1", t0), None);
    }

    #[test]
    fn a_part_waits_its_turn_then_rises_into_place() {
        let at = |ms| rise(Duration::from_millis(ms), BUBBLE, false);
        assert_eq!(at(20), Some(Rise { opacity: 0., offset: 10. }));
        assert!(at(160).is_some_and(|r| r.opacity > 0.5 && r.offset < 5.));
        assert_eq!(at(280), None);
    }

    #[test]
    fn reduced_motion_fades_every_part_in_at_once() {
        assert_eq!(rise(Duration::from_millis(75), BUBBLE, true), Some(Rise { opacity: 0.5, offset: 0. }));
        assert_eq!(rise(Duration::from_millis(150), BUBBLE, true), None);
    }
}
