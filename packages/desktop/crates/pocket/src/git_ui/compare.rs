use crate::desktop::Desktop;
use crate::modals::new_session::listed_branches;
use git::FileStat;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use store::scope::{Against, Scope};
use workspace::Doc;

/// A branch the Compare menu offers: the worktree's base, which a pick keeps following, or another.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    Base(String),
    Branch(String),
}

impl Choice {
    pub fn against(&self) -> Against {
        match self {
            Choice::Base(_) => Against::Base,
            Choice::Branch(b) => Against::Ref(b.clone()),
        }
    }
}

/// The branch `against` names in a worktree whose base is `base`; `None` for a base it doesn't have.
pub fn branch_named(against: &Against, base: Option<&str>) -> Option<String> {
    match against {
        Against::Base => base.map(str::to_string),
        Against::Ref(b) => Some(b.clone()),
    }
}

/// The branches holding `search`: the base, then the others as the new session's branch menu lists them, less `current`.
pub fn choices(base: Option<&str>, current: &str, branches: &[(String, Option<i64>)], remote: &[String], search: &str) -> Vec<Choice> {
    let needle = search.trim().to_lowercase();
    let mut out: Vec<Choice> = base.filter(|b| b.to_lowercase().contains(&needle)).map(|b| Choice::Base(b.to_string())).into_iter().collect();
    let listed = listed_branches(branches, remote, search).into_iter().map(|(local, b)| if local.is_some() { b.to_string() } else { format!("origin/{b}") });
    out.extend(listed.filter(|b| Some(b.as_str()) != base && b != current).map(Choice::Branch));
    out
}

/// What the scope switch says: `Uncommitted`, or the ref compared against.
pub fn switch_label(scope: &Scope, against: Option<&str>) -> String {
    if scope.compare { format!("vs {}", against.unwrap_or("base")) } else { "Uncommitted".into() }
}

/// A Compare read for a worktree: the ref it was against, and the files it found; `None` when the ref named no commit.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub cwd: String,
    pub against: String,
    pub files: Option<Vec<FileStat>>,
}

#[derive(Debug, PartialEq)]
pub enum Landed {
    Same,
    Changed,
    /// The scope fell back to the base, which is to be read now.
    Again,
}

/// The Compare on screen and why a worktree's scope last fell back to the base; plain data, so it tests without GPUI.
#[derive(Debug, Default, PartialEq)]
pub struct CompareView {
    pub(crate) shown: Option<Shown>,
    /// The worktree whose scope fell back, and what to say there until its scope changes.
    notice: Option<(String, String)>,
    /// The read last started and not shown, so a redraw doesn't start another; kept when git failed to answer, leaving the retry to the next refresh.
    pub(crate) pending: Option<(String, String)>,
}

impl CompareView {
    /// The Compare on screen, if it was read for `cwd` against `against`.
    pub fn get(&self, cwd: &str, against: &str) -> Option<&Shown> {
        self.shown.as_ref().filter(|s| s.cwd == cwd && s.against == against)
    }

    /// Whether `cwd`'s Compare against `against` is neither on screen nor being read.
    pub fn needs_read(&self, cwd: &str, against: &str) -> bool {
        self.get(cwd, against).is_none() && self.pending.as_ref().is_none_or(|(c, a)| c != cwd || a != against)
    }

    /// The ref the Compare on screen is against, which its rows open.
    pub fn against(&self) -> Option<&str> {
        self.shown.as_ref().map(|s| s.against.as_str())
    }

    /// Why `cwd`'s scope fell back to the base, if it did.
    pub fn notice(&self, cwd: &str) -> Option<&str> {
        self.notice.as_ref().filter(|(c, _)| c == cwd).map(|(_, n)| n.as_str())
    }

    /// Forgets why `cwd`'s scope fell back, as it changed since.
    pub fn forget_notice(&mut self, cwd: &str) {
        if self.notice(cwd).is_some() {
            self.notice = None;
        }
    }

    /// Points `cwd`'s `scope` back at the base after `gone`, what it compared against, went away, and says so; false when it was on the base already.
    pub fn fall_back(&mut self, cwd: &str, scope: &mut Scope, gone: &str) -> bool {
        if scope.against == Against::Base {
            return false;
        }
        scope.against = Against::Base;
        self.notice = Some((cwd.to_string(), format!("Can't find {gone}, so this compares with the base.")));
        true
    }

    /// Shows `read`, the Compare of `cwd` against `against`; a ref that named no commit points `scope` back at the base, to read again, and git failing to answer keeps what's shown.
    pub fn land(&mut self, cwd: &str, against: String, read: Result<Option<Vec<FileStat>>, String>, scope: &mut Scope) -> Landed {
        let Ok(read) = read else { return Landed::Same };
        if self.pending.as_ref().is_some_and(|(c, a)| c == cwd && *a == against) {
            self.pending = None;
        }
        if read.is_none() && self.fall_back(cwd, scope, &against) {
            return Landed::Again;
        }
        let shown = Shown { cwd: cwd.to_string(), against, files: read };
        if self.shown.as_ref() == Some(&shown) {
            return Landed::Same;
        }
        self.shown = Some(shown);
        Landed::Changed
    }
}

pub struct CompareState {
    pub(crate) view: CompareView,
    pub(crate) menu: bool,
    pub(crate) search: Entity<InputState>,
    /// The local branches, with their last commit's time, and origin's, of the worktree the menu last opened in.
    pub(crate) branches: Vec<(String, Option<i64>)>,
    pub(crate) remote: Vec<String>,
}

impl CompareState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search branches…"));
        let subs = vec![cx.subscribe_in(&search, window, |this, _, ev: &InputEvent, _, cx| match ev {
            InputEvent::PressEnter { .. } => {
                if let Some(choice) = this.compare_choices(cx).into_iter().next() {
                    this.pick_compare(choice, cx);
                }
            }
            InputEvent::Change => cx.notify(),
            _ => {}
        })];
        (Self { view: CompareView::default(), menu: false, search, branches: Vec::new(), remote: Vec::new() }, subs)
    }
}

impl Desktop {
    pub(crate) fn scope(&self, cwd: &str) -> Scope {
        self.store.scopes.get(cwd).cloned().unwrap_or_default()
    }

    fn set_scope(&mut self, cwd: String, scope: Scope, cx: &mut Context<Self>) {
        if scope == Scope::default() {
            self.store.scopes.remove(&cwd);
        } else {
            self.store.scopes.insert(cwd, scope);
        }
        self.save_soon(cx);
    }

    /// The branch `cwd`'s Compare is against; `None` in Uncommitted, or against a base it doesn't have.
    pub(crate) fn compare_branch(&self, cwd: &str) -> Option<String> {
        let scope = self.scope(cwd);
        scope.compare.then(|| branch_named(&scope.against, self.repos.get(cwd).and_then(|r| r.base.as_deref()))).flatten()
    }

    /// Reads the shown worktree's Compare off the UI thread.
    pub(crate) fn load_compare(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        let Some(against) = self.compare_branch(&cwd) else { return };
        self.compare.view.pending = Some((cwd.clone(), against.clone()));
        let task = cx.background_executor().spawn({
            let (cwd, against) = (cwd.clone(), against.clone());
            async move { git::compare::read(&hosts::runner_for(&cwd), &cwd, &against) }
        });
        cx.spawn(async move |this, cx| {
            let read = task.await;
            this.update(cx, |d, cx| {
                if d.cwd().as_ref() != Some(&cwd) || d.compare_branch(&cwd).as_ref() != Some(&against) {
                    return;
                }
                let mut scope = d.scope(&cwd);
                match d.compare.view.land(&cwd, against, read, &mut scope) {
                    Landed::Same => {}
                    Landed::Changed => cx.notify(),
                    Landed::Again => {
                        d.set_scope(cwd, scope, cx);
                        cx.notify();
                        d.load_compare(cx);
                    }
                }
            })
            .ok();
        })
        .detach();
    }

    /// Changes the shown worktree's scope and reads its Compare again.
    pub(crate) fn change_scope(&mut self, change: impl FnOnce(&mut Scope), cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        let mut scope = self.scope(&cwd);
        change(&mut scope);
        self.compare.view.forget_notice(&cwd);
        self.set_scope(cwd, scope, cx);
        self.load_compare(cx);
        cx.notify();
    }

    pub(crate) fn pick_compare(&mut self, choice: Choice, cx: &mut Context<Self>) {
        self.compare.menu = false;
        self.change_scope(
            |s| {
                s.compare = true;
                s.against = choice.against();
            },
            cx,
        );
    }

    /// Opens the branch menu emptied and focused, listing the worktree's branches once git has read them.
    pub(crate) fn toggle_compare_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.compare.menu = !self.compare.menu;
        (self.changes.menu, self.changes.commit_menu) = (false, false);
        cx.notify();
        let Some(cwd) = self.cwd().filter(|_| self.compare.menu) else { return };
        self.compare.search.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        let task = cx.background_executor().spawn({
            let cwd = cwd.clone();
            async move {
                let runner = hosts::runner_for(&cwd);
                (git::dated_branches(&runner, &cwd), git::origin_branches(&runner, &cwd))
            }
        });
        cx.spawn(async move |this, cx| {
            let (branches, remote) = task.await;
            this.update(cx, |d, cx| {
                if d.cwd().as_ref() == Some(&cwd) {
                    (d.compare.branches, d.compare.remote) = (branches, remote);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// The branches the menu offers for what its search holds.
    pub(crate) fn compare_choices(&self, cx: &App) -> Vec<Choice> {
        let Some(repo) = self.repo() else { return Vec::new() };
        choices(repo.base.as_deref(), &repo.branch, &self.compare.branches, &self.compare.remote, &self.compare.search.read(cx).value())
    }

    /// Opens `path` as the Compare on screen shows it.
    pub(crate) fn open_compare_file(&mut self, path: String, pin: bool, cx: &mut Context<Self>) {
        let Some(against) = self.compare.view.against().map(str::to_string) else { return };
        self.open_doc(Doc::CompareFile { against, path }, pin, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, CompareView, Landed, branch_named, choices, switch_label};
    use git::FileStat;
    use store::scope::{Against, Scope};

    fn file(path: &str) -> FileStat {
        FileStat { path: path.into(), added: 1, removed: 0, staged: false, unstaged: false, status: 'M', old_path: None }
    }

    #[test]
    fn a_compare_is_against_the_base_when_there_is_one_or_the_branch_picked() {
        assert_eq!(branch_named(&Against::Base, Some("origin/main")), Some("origin/main".into()));
        assert_eq!(branch_named(&Against::Base, None), None);
        assert_eq!(branch_named(&Against::Ref("feat/x".into()), None), Some("feat/x".into()));
    }

    #[test]
    fn a_read_shows_once_and_again_only_when_it_changed() {
        let mut view = CompareView { pending: Some(("/w/fix".into(), "main".into())), ..CompareView::default() };
        let mut scope = Scope { compare: true, ..Scope::default() };
        let read = Ok(Some(vec![file("a.rs")]));
        assert_eq!(view.land("/w/fix", "main".into(), read.clone(), &mut scope), Landed::Changed);
        assert_eq!(view.pending, None);
        assert_eq!(view.land("/w/fix", "main".into(), read, &mut scope), Landed::Same);
        assert!(view.get("/w/fix", "main").is_some());
        assert!(view.get("/w/other", "main").is_none());
        assert!(view.get("/w/fix", "dev").is_none());
    }

    #[test]
    fn a_compare_needs_reading_unless_it_is_shown_or_being_read() {
        let mut view = CompareView::default();
        assert!(view.needs_read("/w/fix", "main"));
        view.pending = Some(("/w/fix".into(), "main".into()));
        assert!(!view.needs_read("/w/fix", "main"));
        assert!(view.needs_read("/w/fix", "dev"));
        view.land("/w/fix", "main".into(), Ok(Some(Vec::new())), &mut Scope { compare: true, ..Scope::default() });
        assert!(!view.needs_read("/w/fix", "main"));
    }

    #[test]
    fn git_failing_to_answer_keeps_what_is_shown_and_the_scope_and_waits_for_the_next_refresh() {
        let mut view = CompareView::default();
        let mut scope = Scope { compare: true, against: Against::Ref("feat".into()) };
        assert_eq!(view.land("/w/fix", "feat".into(), Ok(Some(Vec::new())), &mut scope), Landed::Changed);
        view.pending = Some(("/w/fix".into(), "feat".into()));
        assert_eq!(view.land("/w/fix", "feat".into(), Err("not connected".into()), &mut scope), Landed::Same);
        assert_eq!(scope.against, Against::Ref("feat".into()));
        assert!(view.get("/w/fix", "feat").is_some());
        assert_eq!(view.pending, Some(("/w/fix".into(), "feat".into())), "a redraw doesn't ask again");
    }

    #[test]
    fn a_ref_that_went_away_falls_back_to_the_base_and_says_so_in_that_worktree_until_its_scope_changes() {
        let mut view = CompareView::default();
        let mut scope = Scope { compare: true, against: Against::Ref("old-branch".into()) };
        assert_eq!(view.land("/w/fix", "old-branch".into(), Ok(None), &mut scope), Landed::Again);
        assert_eq!(scope, Scope { compare: true, against: Against::Base });
        assert_eq!(view.notice("/w/fix"), Some("Can't find old-branch, so this compares with the base."));
        assert_eq!(view.notice("/w/other"), None);
        view.forget_notice("/w/other");
        assert!(view.notice("/w/fix").is_some());
        view.forget_notice("/w/fix");
        assert_eq!(view.notice("/w/fix"), None);
    }

    #[test]
    fn a_base_that_names_no_commit_shows_as_missing_instead_of_falling_back() {
        let mut view = CompareView::default();
        let mut scope = Scope { compare: true, ..Scope::default() };
        assert_eq!(view.land("/w/fix", "origin/main".into(), Ok(None), &mut scope), Landed::Changed);
        assert_eq!(view.get("/w/fix", "origin/main").map(|s| s.files.is_none()), Some(true));
        assert_eq!(view.notice("/w/fix"), None);
    }

    fn labels(choices: &[Choice]) -> Vec<String> {
        choices
            .iter()
            .map(|c| match c {
                Choice::Base(b) => format!("base {b}"),
                Choice::Branch(b) => b.clone(),
            })
            .collect()
    }

    #[test]
    fn the_menu_offers_the_base_then_the_other_branches_but_not_the_one_checked_out() {
        let branches = [("main".to_string(), None), ("mine".to_string(), None), ("old".to_string(), None)];
        assert_eq!(labels(&choices(Some("origin/main"), "mine", &branches, &["remote-only".into()], "")), ["base origin/main", "main", "old"]);
    }

    #[test]
    fn a_search_filters_the_base_and_the_local_and_remote_branches() {
        let branches = [("feat/y".to_string(), None), ("main".to_string(), None)];
        let remote = ["feat/z".to_string(), "main".to_string()];
        assert_eq!(labels(&choices(Some("origin/main"), "mine", &branches, &remote, "feat")), ["feat/y", "origin/feat/z"]);
        assert_eq!(labels(&choices(Some("origin/main"), "mine", &branches, &remote, "main")), ["base origin/main", "main"]);
        assert!(choices(Some("origin/main"), "mine", &branches, &remote, "v1.2").is_empty());
    }

    #[test]
    fn picking_the_base_follows_it_and_another_branch_names_it() {
        assert_eq!(Choice::Base("origin/main".into()).against(), Against::Base);
        assert_eq!(Choice::Branch("origin/feat/z".into()).against(), Against::Ref("origin/feat/z".into()));
    }

    #[test]
    fn the_switch_says_uncommitted_or_the_ref() {
        let compare = Scope { compare: true, ..Scope::default() };
        assert_eq!(switch_label(&Scope::default(), Some("origin/main")), "Uncommitted");
        assert_eq!(switch_label(&compare, Some("origin/main")), "vs origin/main");
        assert_eq!(switch_label(&compare, None), "vs base");
    }
}
