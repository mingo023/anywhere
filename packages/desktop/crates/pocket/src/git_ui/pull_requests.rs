use crate::desktop::Desktop;
use git::github::{self, GhProblem, GhStatus, Pr, PrState};
use gpui_kit::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use theme::*;

const OTHERS_EVERY: Duration = Duration::from_secs(5 * 60);
const GH_TURN: Duration = Duration::from_secs(60);

/// Each worktree's PR as `gh` last reported it, and when it was asked.
#[derive(Default)]
pub(crate) struct PullRequests {
    by_tree: HashMap<String, (Instant, Option<Pr>)>,
    /// The tree `gh` is asked about, and since when.
    running: Option<(String, Instant)>,
    problem: Option<(GhProblem, Instant)>,
    /// What the last Settings check of `gh` found; `Some(None)` while it runs.
    pub(crate) gh: Option<Option<GhStatus>>,
}

/// What the Changes menu offers for the tree on screen.
#[derive(Debug, PartialEq)]
pub(crate) enum PrItem<'a> {
    Open(&'a str),
    Create,
    Hint(&'static str),
}

impl PullRequests {
    /// The tree to ask `gh` about next: the one on screen `every` so often, the others every OTHERS_EVERY, one at a time.
    fn due(&self, selected: Option<&str>, every: Duration, trees: &[String], now: Instant) -> Option<String> {
        // A gh that hangs gives up its turn after a minute.
        if self.running.as_ref().is_some_and(|(_, at)| now.duration_since(*at) < GH_TURN) || self.problem.is_some_and(|(_, at)| now.duration_since(at) < OTHERS_EVERY) {
            return None;
        }
        let stale = |t: &str, every: Duration| self.by_tree.get(t).is_none_or(|(at, _)| now.duration_since(*at) >= every);
        selected
            .filter(|s| trees.iter().any(|t| t == s) && stale(s, every))
            .map(str::to_string)
            .or_else(|| trees.iter().find(|t| stale(t, OTHERS_EVERY)).cloned())
    }

    /// Records a Settings check of `gh`; a ready one lets PR polling resume at once.
    pub(crate) fn checked(&mut self, status: GhStatus) {
        if matches!(status, GhStatus::Ready { .. }) {
            self.problem = None;
        }
        self.gh = Some(Some(status));
    }

    /// Marks `gh` as asked about `tree` since `started`, the run [`Self::apply`] expects an answer from.
    pub(crate) fn start(&mut self, tree: &str, started: Instant) {
        self.running = Some((tree.to_string(), started));
    }

    /// Records what the `gh` run for `tree` begun at `started` said, unless that run was forgotten or superseded. True when it changes what's shown.
    pub(crate) fn apply(&mut self, tree: String, started: Instant, result: Result<Option<Pr>, GhProblem>, now: Instant) -> bool {
        if !self.running.as_ref().is_some_and(|(t, at)| *t == tree && *at == started) {
            return false;
        }
        self.running = None;
        match result {
            Err(p) => {
                let changed = self.problem.map(|(old, _)| old) != Some(p);
                self.problem = Some((p, now));
                changed
            }
            Ok(pr) => {
                let changed = self.problem.take().is_some() || self.by_tree.get(&tree).is_none_or(|(_, old)| *old != pr);
                self.by_tree.insert(tree, (now, pr));
                changed
            }
        }
    }

    pub(crate) fn get(&self, tree: &str) -> Option<&Pr> {
        self.by_tree.get(tree)?.1.as_ref()
    }

    /// Create is offered only once `gh` has answered for `tree` with no PR, and `branch` isn't `base`.
    pub(crate) fn item(&self, tree: &str, branch: &str, base: Option<&str>) -> Option<PrItem<'_>> {
        if let Some(pr) = self.get(tree) {
            return Some(PrItem::Open(&pr.url));
        }
        match self.problem.map(|(p, _)| p) {
            Some(GhProblem::Missing) => Some(PrItem::Hint("Install gh to see PRs")),
            Some(GhProblem::LoggedOut) => Some(PrItem::Hint("Run gh auth login to see PRs")),
            None => (self.by_tree.contains_key(tree) && base.is_some_and(|b| github::base_branch(b) != branch)).then_some(PrItem::Create),
        }
    }

    /// Forgets `tree`'s PR, and drops the answer of any run in flight for it, so the next poll asks `gh` again.
    pub(crate) fn forget(&mut self, tree: &str) {
        self.by_tree.remove(tree);
        self.drop_run(tree);
    }

    /// Shows `pr` for `tree` at once, as when it was just opened, until the next poll asks `gh`.
    pub(crate) fn put(&mut self, tree: &str, pr: Pr, now: Instant) {
        self.by_tree.insert(tree.to_string(), (now, Some(pr)));
        self.expire(tree, now);
    }

    /// Changes what's shown for `tree`'s PR after acting on it, so an answer asked for before can't undo it.
    pub(crate) fn edit(&mut self, tree: &str, f: impl FnOnce(&mut Pr)) {
        if let Some((_, Some(pr))) = self.by_tree.get_mut(tree) {
            f(pr);
        }
        self.drop_run(tree);
    }

    /// Keeps showing `tree`'s PR but asks `gh` about it at the next poll.
    pub(crate) fn expire(&mut self, tree: &str, now: Instant) {
        if let Some((at, _)) = self.by_tree.get_mut(tree) {
            *at = now.checked_sub(OTHERS_EVERY).unwrap_or(*at);
        }
        self.drop_run(tree);
    }

    fn drop_run(&mut self, tree: &str) {
        if self.running.as_ref().is_some_and(|(t, _)| t == tree) {
            self.running = None;
        }
    }
}

pub(crate) fn tone(pr: &Pr) -> Token {
    let c = pr.checks;
    match pr.state {
        PrState::Merged => MERGED,
        PrState::Closed => TEXT_4,
        PrState::Open if pr.draft => TEXT_4,
        PrState::Open if c.failed > 0 => FAILED_TEXT,
        PrState::Open if c.pending > 0 => WAITING_TEXT,
        PrState::Open if c.passed > 0 => SUCCESS_TEXT,
        PrState::Open => TEXT_3,
    }
}

pub(crate) fn detail(pr: &Pr) -> String {
    let state = match pr.state {
        PrState::Merged => Some("Merged"),
        PrState::Closed => Some("Closed"),
        PrState::Open => pr.draft.then_some("Draft"),
    };
    let c = pr.checks;
    let counts: Vec<String> = [(c.failed, "failed"), (c.pending, "pending"), (c.passed, "passed")].into_iter().filter(|(n, _)| *n > 0).map(|(n, w)| format!("{n} {w}")).collect();
    let checks = if counts.is_empty() { "No checks".to_string() } else { counts.join(" · ") };
    match state {
        Some(s) => format!("{s} · {checks}"),
        None => checks,
    }
}

struct PrCard {
    title: String,
    line: String,
}

impl Render for PrCard {
    fn render(&mut self, _: &mut Window, _: &mut Context<Self>) -> impl IntoElement {
        ui::pop(div())
            .p(px(10.))
            .max_w(px(320.))
            .flex()
            .flex_col()
            .gap(px(2.))
            .text_size(px(12.))
            .text_color(TEXT_2)
            .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(self.title.clone()))
            .child(self.line.clone())
    }
}

/// `#123`, coloured by the PR's state and checks, with its title and checks on hover.
pub(crate) fn chip(id: impl Into<ElementId>, pr: &Pr) -> Stateful<Div> {
    let (title, line) = (format!("#{} {}", pr.number, pr.title), detail(pr));
    div()
        .id(id)
        .flex_none()
        .font_family(MONO)
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(tone(pr))
        .child(format!("#{}", pr.number))
        .tooltip(move |_, cx| cx.new(|_| PrCard { title: title.clone(), line: line.clone() }).into())
        .tooltip_show_delay(Duration::from_millis(350))
}

impl Desktop {
    /// Checks whether `gh` is installed and signed in, off the UI thread.
    pub(crate) fn check_gh(&mut self, cx: &mut Context<Self>) {
        if self.prs.gh == Some(None) {
            return;
        }
        self.prs.gh = Some(None);
        let task = cx.background_executor().spawn(async { github::status(daemon::run_login(github::VERSION, "/", ""), daemon::run_login(github::AUTH, "/", "")) });
        cx.spawn(async move |this, cx| {
            let status = task.await;
            this.update(cx, |d, cx| {
                d.prs.checked(status);
                d.poll_prs(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Asks `gh` about the next due worktree, off the UI thread.
    pub(crate) fn poll_prs(&mut self, cx: &mut Context<Self>) {
        let mut trees: Vec<String> = self.project.as_ref().and_then(|p| self.listed_trees(p)).into_iter().flatten().filter(|w| !w.main && w.branch != "detached").map(|w| w.path).collect();
        trees.sort();
        let Some(tree) = self.prs.due(self.cwd().as_deref(), self.store.general.prs_every(), &trees, Instant::now()) else { return };
        let started = Instant::now();
        self.prs.start(&tree, started);
        let dir = tree.clone();
        let task = cx.background_executor().spawn(async move {
            let mut result = github::view_result(daemon::run_login(github::VIEW, &dir, ""));
            let mut lost = false;
            if let Ok(Some(pr)) = &mut result
                && pr.state == PrState::Open
            {
                let argv = github::threads_argv(pr.number);
                let threads = super::pull_request::run(&argv, &dir, "").ok().and_then(|json| github::threads_result(&json));
                lost = threads.is_none();
                pr.threads = threads.unwrap_or_default();
            }
            (result, lost)
        });
        cx.spawn(async move |this, cx| {
            let (mut result, lost) = task.await;
            this.update(cx, |d, cx| {
                // A failed threads query keeps the threads already shown rather than emptying the list.
                if let (true, Ok(Some(pr))) = (lost, &mut result) {
                    pr.threads = d.prs.get(&tree).map(|p| p.threads.clone()).unwrap_or_default();
                }
                if d.prs.apply(tree, started, result, Instant::now()) {
                    d.refresh_pr_diffs(cx);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{Duration, GhProblem, GhStatus, Instant, Pr, PrItem, PullRequests, detail, tone};
    use git::github::{Checks, PrState};
    use theme::{FAILED_TEXT, MERGED, SUCCESS_TEXT, TEXT_3, TEXT_4, WAITING_TEXT};

    fn pr(n: u32) -> Pr {
        Pr { number: n, title: format!("PR {n}"), url: format!("https://github.com/acme/app/pull/{n}"), ..Pr::default() }
    }

    /// Starts a `gh` run for `tree` at `now` and applies its answer.
    fn answer(prs: &mut PullRequests, tree: &str, result: Result<Option<Pr>, GhProblem>, now: Instant) -> bool {
        prs.start(tree, now);
        prs.apply(tree.into(), now, result, now)
    }

    const EVERY: Duration = Duration::from_secs(30);

    fn trees() -> Vec<String> {
        vec!["/a".into(), "/b".into(), "/c".into()]
    }

    #[test]
    fn the_tree_on_screen_is_asked_about_first_then_the_rest_in_order() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        assert_eq!(prs.due(Some("/b"), EVERY, &trees(), now).as_deref(), Some("/b"));
        answer(&mut prs, "/b", Ok(None), now);
        assert_eq!(prs.due(Some("/b"), EVERY, &trees(), now).as_deref(), Some("/a"));
        answer(&mut prs, "/a", Ok(Some(pr(1))), now);
        assert_eq!(prs.due(Some("/b"), EVERY, &trees(), now).as_deref(), Some("/c"));
        answer(&mut prs, "/c", Ok(None), now);
        assert_eq!(prs.due(Some("/b"), EVERY, &trees(), now), None);
        assert_eq!(prs.due(Some("/main"), EVERY, &trees(), now), None);
    }

    #[test]
    fn gh_runs_one_at_a_time_a_hung_one_gives_up_its_turn_and_a_missing_or_logged_out_gh_is_asked_again_after_5_minutes() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        prs.running = Some(("/a".into(), now));
        assert_eq!(prs.due(None, EVERY, &trees(), now), None);
        assert_eq!(prs.due(None, EVERY, &trees(), now + Duration::from_secs(60)).as_deref(), Some("/a"));
        prs.apply("/a".into(), now, Err(GhProblem::LoggedOut), now);
        assert!(prs.running.is_none());
        assert_eq!(prs.due(None, EVERY, &trees(), now + Duration::from_secs(299)), None);
        let later = now + Duration::from_secs(300);
        assert_eq!(prs.due(None, EVERY, &trees(), later).as_deref(), Some("/a"));
        assert!(answer(&mut prs, "/a", Ok(None), later));
        assert_eq!(prs.item("/a", "fix", Some("main")), Some(PrItem::Create));
    }

    #[test]
    fn the_tree_on_screen_is_asked_again_after_30s_and_the_others_after_5_minutes() {
        let (mut prs, t0) = (PullRequests::default(), Instant::now());
        for t in trees() {
            answer(&mut prs, &t, Ok(None), t0);
        }
        let at = |s| t0 + Duration::from_secs(s);
        assert_eq!(prs.due(Some("/b"), EVERY, &trees(), at(29)), None);
        assert_eq!(prs.due(Some("/b"), EVERY, &trees(), at(30)).as_deref(), Some("/b"));
        answer(&mut prs, "/b", Ok(None), at(290));
        assert_eq!(prs.due(Some("/b"), EVERY, &trees(), at(299)), None);
        assert_eq!(prs.due(Some("/b"), EVERY, &trees(), at(300)).as_deref(), Some("/a"));
    }

    #[test]
    fn the_tree_on_screen_is_asked_again_as_often_as_chosen() {
        let (mut prs, t0) = (PullRequests::default(), Instant::now());
        for t in trees() {
            answer(&mut prs, &t, Ok(None), t0);
        }
        let every = Duration::from_secs(15);
        assert_eq!(prs.due(Some("/b"), every, &trees(), t0 + Duration::from_secs(14)), None);
        assert_eq!(prs.due(Some("/b"), every, &trees(), t0 + every).as_deref(), Some("/b"));
    }

    #[test]
    fn applying_the_same_pr_again_changes_nothing() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        assert!(answer(&mut prs, "/a", Ok(None), now));
        assert!(!answer(&mut prs, "/a", Ok(None), now));
        assert!(answer(&mut prs, "/a", Ok(Some(pr(1))), now));
        assert!(!answer(&mut prs, "/a", Ok(Some(pr(1))), now));
        let failing = Pr { checks: Checks { failed: 1, ..Checks::default() }, ..pr(1) };
        assert!(answer(&mut prs, "/a", Ok(Some(failing)), now));
        assert!(answer(&mut prs, "/a", Err(GhProblem::Missing), now));
        assert!(!answer(&mut prs, "/a", Err(GhProblem::Missing), now));
    }

    #[test]
    fn a_chip_is_toned_by_state_then_by_its_worst_check() {
        let with = |state, draft, failed, pending, passed| Pr { state, draft, checks: Checks { passed, failed, pending }, ..pr(1) };
        let tones = [
            with(PrState::Merged, false, 1, 0, 0),
            with(PrState::Closed, false, 0, 0, 1),
            with(PrState::Open, true, 1, 0, 0),
            with(PrState::Open, false, 1, 1, 1),
            with(PrState::Open, false, 0, 1, 1),
            with(PrState::Open, false, 0, 0, 1),
            with(PrState::Open, false, 0, 0, 0),
        ]
        .map(|p| tone(&p));
        assert_eq!(tones, [MERGED, TEXT_4, TEXT_4, FAILED_TEXT, WAITING_TEXT, SUCCESS_TEXT, TEXT_3]);
    }

    #[test]
    fn a_chips_tooltip_names_the_state_and_counts_the_checks() {
        let open = Pr { checks: Checks { passed: 3, failed: 3, pending: 2 }, ..pr(1) };
        let merged = Pr { state: PrState::Merged, ..pr(2) };
        let draft = Pr { draft: true, checks: Checks { pending: 1, ..Checks::default() }, ..pr(3) };
        assert_eq!([detail(&open), detail(&merged), detail(&draft)], ["3 failed · 2 pending · 3 passed", "Merged · No checks", "Draft · 1 pending"]);
    }

    #[test]
    fn a_ready_gh_found_in_settings_lets_pr_polling_resume_at_once() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        answer(&mut prs, "/a", Err(GhProblem::LoggedOut), now);
        prs.checked(GhStatus::SignedOut { version: "2.81.0".into() });
        assert_eq!(prs.due(None, EVERY, &trees(), now), None);
        prs.checked(GhStatus::Ready { version: "2.81.0".into(), login: "me".into() });
        assert_eq!(prs.due(None, EVERY, &trees(), now), Some("/a".into()));
    }

    #[test]
    fn the_changes_menu_opens_the_pr_or_says_what_gh_needs() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        assert_eq!(prs.item("/a", "fix", Some("main")), None);
        answer(&mut prs, "/a", Ok(Some(pr(7))), now);
        assert_eq!(prs.item("/a", "fix", Some("main")), Some(PrItem::Open("https://github.com/acme/app/pull/7")));
        answer(&mut prs, "/b", Err(GhProblem::Missing), now);
        assert_eq!(prs.item("/b", "fix", Some("main")), Some(PrItem::Hint("Install gh to see PRs")));
        assert_eq!(prs.item("/a", "fix", Some("main")), Some(PrItem::Open("https://github.com/acme/app/pull/7")));
        let mut logged_out = PullRequests::default();
        answer(&mut logged_out, "/a", Err(GhProblem::LoggedOut), now);
        assert_eq!(logged_out.item("/a", "fix", Some("main")), Some(PrItem::Hint("Run gh auth login to see PRs")));
    }

    #[test]
    fn the_changes_menu_offers_create_once_gh_found_no_pr_for_a_branch_off_its_base() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        assert_eq!(prs.item("/a", "fix", Some("main")), None);
        answer(&mut prs, "/a", Ok(None), now);
        assert_eq!(prs.item("/a", "fix", Some("main")), Some(PrItem::Create));
        assert_eq!(prs.item("/a", "fix", None), None);
        assert_eq!(prs.item("/a", "main", Some("origin/main")), None);
        answer(&mut prs, "/a", Ok(Some(pr(3))), now);
        assert_eq!(prs.item("/a", "fix", Some("main")), Some(PrItem::Open("https://github.com/acme/app/pull/3")));
    }

    #[test]
    fn a_forgotten_tree_is_due_at_once() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        let trees = vec!["/a".to_string()];
        answer(&mut prs, "/a", Ok(None), now);
        assert_eq!(prs.due(Some("/a"), EVERY, &trees, now), None);
        prs.forget("/a");
        assert_eq!(prs.due(Some("/a"), EVERY, &trees, now).as_deref(), Some("/a"));
    }

    #[test]
    fn an_answer_from_a_forgotten_or_superseded_run_does_not_land() {
        let (mut prs, t0) = (PullRequests::default(), Instant::now());
        let t1 = t0 + Duration::from_secs(60);
        prs.running = Some(("/a".into(), t0));
        prs.forget("/a");
        assert!(!prs.apply("/a".into(), t0, Ok(Some(pr(1))), t0));
        assert_eq!(prs.get("/a"), None);
        prs.running = Some(("/a".into(), t1));
        assert!(!prs.apply("/a".into(), t0, Ok(Some(pr(1))), t1));
        assert!(prs.apply("/a".into(), t1, Ok(None), t1));
        assert_eq!(prs.item("/a", "fix", Some("main")), Some(PrItem::Create));
    }

    #[test]
    fn a_pr_just_opened_shows_at_once_and_is_asked_about_at_the_next_poll() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        prs.put("/a", pr(7), now);
        assert_eq!(prs.get("/a").map(|p| p.number), Some(7));
        assert_eq!(prs.due(Some("/a"), EVERY, &trees(), now).as_deref(), Some("/a"));
    }

    #[test]
    fn an_edit_outlives_the_answer_of_a_run_asked_for_before_it() {
        let (mut prs, now) = (PullRequests::default(), Instant::now());
        answer(&mut prs, "/a", Ok(Some(pr(1))), now);
        prs.start("/a", now);
        prs.edit("/a", |p| p.state = PrState::Merged);
        assert!(!prs.apply("/a".into(), now, Ok(Some(pr(1))), now));
        assert_eq!(prs.get("/a").map(|p| p.state), Some(PrState::Merged));
    }

    #[test]
    fn an_expired_pr_stays_shown_until_gh_answers_again() {
        let (mut prs, now) = (PullRequests::default(), Instant::now() + Duration::from_secs(600));
        answer(&mut prs, "/a", Ok(Some(pr(1))), now);
        assert_eq!(prs.due(Some("/a"), EVERY, &trees(), now).as_deref(), Some("/b"));
        prs.expire("/a", now);
        assert_eq!(prs.get("/a").map(|p| p.number), Some(1));
        assert_eq!(prs.due(Some("/a"), EVERY, &trees(), now).as_deref(), Some("/a"));
    }
}
