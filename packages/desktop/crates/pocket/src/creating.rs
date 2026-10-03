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
        Self { request, path, spec, lines, at: 0, started: now, step_started: now, ended: None, failed: None, output: false }
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

#[derive(Default)]
pub(crate) struct Creates {
    pub(crate) list: Vec<Create>,
    /// A timer redraws the clock while a running create is on screen.
    ticking: bool,
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
        self.creates.shown(&self.cwd()?)
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
                    d.creates.ticking = d.shown_create().is_some_and(|c| c.failed.is_none());
                    if d.creates.ticking {
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
        let Some(c) = self.creates.get(request) else { return };
        let spec = c.again();
        let request = self.outbox.create(spec.clone());
        *c = Create::new(request, c.path.clone(), spec, Instant::now());
        cx.notify();
    }

    pub(crate) fn edit_create(&mut self, request: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(c) = self.creates.drop(request, &mut self.worktree) {
            self.reopen_new_worktree(&c, window, cx);
        }
    }

    pub(crate) fn creating_page(&mut self, cx: &mut Context<Self>) -> Div {
        self.tick_create(cx);
        let Some(c) = self.shown_create() else { return div() };
        let now = Instant::now();
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
        let rows = c.steps().map(|(step, mark, spent, note)| {
            let lead = match mark {
                Mark::Done => icon("check", 14., SUCCESS).into_any_element(),
                Mark::Active => spinner(("create-step", step as usize), 14., TEXT_3).into_any_element(),
                Mark::Failed => icon("x-bold", 12., FAILED).into_any_element(),
                Mark::Pending => div().size(px(6.)).rounded_full().bg(TEXT_5).into_any_element(),
            };
            div()
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
                .children(spent.map(|t| div().font_family(MONO).text_size(px(12.)).text_color(TEXT_4).child(took(t))))
        });
        let steps = div()
            .flex()
            .flex_col()
            .px(px(14.))
            .py(px(6.))
            .rounded(px(14.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(rgba(0x1111130a), 1., 2.)])
            .children(rows);
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
        div().flex_1().flex().flex_col().child(bar).child(
            div().flex_1().flex().flex_col().items_center().justify_center().child(div().w(px(440.)).flex().flex_col().gap(px(12.)).child(title).child(branch).child(steps).children(error).child(footer)),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Create, Creates, Mark, Step, clock};
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
}
