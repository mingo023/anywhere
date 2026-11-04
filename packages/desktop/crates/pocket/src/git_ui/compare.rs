use crate::desktop::Desktop;
use crate::modals::new_session::listed_branches;
use crate::sidebar::tree_place;
use git::compare::{Comparison, Range};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use std::collections::HashSet;
use store::scope::{Against, Scope};
use workspace::Doc;

/// Another worktree of the project the Compare menu offers: its name as the sidebar shows it, and its branch.
#[derive(Clone, Debug, PartialEq)]
pub struct Sibling {
    pub path: String,
    pub name: String,
    pub branch: String,
}

/// A ref the Compare menu offers.
#[derive(Clone, Debug, PartialEq)]
pub enum Choice {
    Base(String),
    Worktree(Sibling),
    Branch(String),
    /// A tag, commit or any other ref typed into the search.
    Typed(String),
}

impl Choice {
    pub fn against(&self) -> Against {
        match self {
            Choice::Base(_) => Against::Base,
            Choice::Worktree(t) => Against::Worktree(t.path.clone()),
            Choice::Branch(r) | Choice::Typed(r) => Against::Ref(r.clone()),
        }
    }
}

/// A worktree's branch; a detached one names none.
fn branch_of(w: &git::Worktree) -> Option<&str> {
    Some(w.branch.as_str()).filter(|b| !b.is_empty() && *b != "detached")
}

/// What a Compare is against right now.
#[derive(Debug, PartialEq)]
pub enum Target {
    Ref(String),
    /// The worktree has no base to compare with.
    NoBase,
    /// The project's worktrees aren't listed yet, so one compared against can't be told from one that's gone.
    Unlisted,
    /// The worktree compared against is no longer listed or on no branch; its folder's name.
    Gone(String),
}

impl Target {
    /// The range `scope` reads once this names a ref.
    pub fn range(&self, scope: &Scope) -> Option<Range> {
        match self {
            Target::Ref(against) => Some(Range { against: against.clone(), direct: scope.direct, working: scope.working }),
            Target::NoBase | Target::Unlisted | Target::Gone(_) => None,
        }
    }
}

/// What `against` names in a worktree whose base is `base`, among the project's `worktrees` once listed.
pub fn target(against: &Against, base: Option<&str>, worktrees: Option<&[git::Worktree]>) -> Target {
    match against {
        Against::Base => base.map_or(Target::NoBase, |b| Target::Ref(b.to_string())),
        Against::Ref(r) => Target::Ref(r.clone()),
        Against::Worktree(path) => match worktrees {
            None => Target::Unlisted,
            Some(ws) => match ws.iter().find(|w| &w.path == path).and_then(branch_of) {
                Some(b) => Target::Ref(b.to_string()),
                None => Target::Gone(crate::util::basename(path)),
            },
        },
    }
}

/// The project's worktrees besides `cwd` that the menu offers: those on a branch, called what `name` says.
pub fn siblings(worktrees: &[git::Worktree], cwd: &str, name: impl Fn(&git::Worktree) -> String) -> Vec<Sibling> {
    worktrees
        .iter()
        .filter(|w| w.path != cwd)
        .filter_map(|w| Some(Sibling { name: name(w), branch: branch_of(w)?.to_string(), path: w.path.clone() }))
        .collect()
}

/// The menu's refs holding `search`: the base, the project's other worktrees, then branches as the new session's branch menu lists them, less `current` and those a worktree is on; then what was typed, unless a ref listed is named that.
pub fn choices(base: Option<&str>, trees: &[Sibling], current: &str, branches: &[(String, Option<i64>)], remote: &[String], search: &str) -> Vec<Choice> {
    let needle = search.trim().to_lowercase();
    let holds = |s: &str| s.to_lowercase().contains(&needle);
    let mut out: Vec<Choice> = base.filter(|b| holds(b)).map(|b| Choice::Base(b.to_string())).into_iter().collect();
    out.extend(trees.iter().filter(|t| holds(&t.name) || holds(&t.branch)).cloned().map(Choice::Worktree));
    let taken: HashSet<&str> = trees.iter().map(|t| t.branch.as_str()).chain(base).chain([current]).collect();
    let listed = listed_branches(branches, remote, search).into_iter().map(|(local, b)| if local.is_some() { b.to_string() } else { format!("origin/{b}") });
    out.extend(listed.filter(|b| !taken.contains(b.as_str())).map(Choice::Branch));
    let typed = search.trim();
    let named = |c: &Choice| match c {
        Choice::Base(r) | Choice::Branch(r) | Choice::Typed(r) => r == typed,
        Choice::Worktree(t) => t.branch == typed,
    };
    if !typed.is_empty() && !out.iter().any(named) {
        out.push(Choice::Typed(typed.to_string()));
    }
    out
}

/// What the scope switch says: `Uncommitted`, or the ref compared against with how far each side has moved once it's read.
pub fn switch_label(scope: &Scope, against: Option<&str>, counts: Option<(usize, usize)>) -> String {
    if !scope.compare {
        return "Uncommitted".into();
    }
    let against = against.unwrap_or("base");
    match counts {
        Some((ahead, behind)) => format!("vs {against} · {ahead} ahead · {behind} behind"),
        None => format!("vs {against}"),
    }
}

/// A Compare read for a worktree: the range it was for, and what it found; `None` when the ref named no commit.
#[derive(Clone, Debug, PartialEq)]
pub struct Shown {
    pub cwd: String,
    pub range: Range,
    pub comparison: Option<Comparison>,
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
    pub(crate) pending: Option<(String, Range)>,
}

impl CompareView {
    /// The Compare on screen, if it was read for `cwd` and `range`.
    pub fn get(&self, cwd: &str, range: &Range) -> Option<&Shown> {
        self.shown.as_ref().filter(|s| s.cwd == cwd && s.range == *range)
    }

    /// The range of the Compare on screen, which its rows open.
    pub fn range(&self) -> Option<&Range> {
        self.shown.as_ref().map(|s| &s.range)
    }

    /// Why `cwd`'s scope fell back to the base, if it did.
    pub fn notice(&self, cwd: &str) -> Option<&str> {
        self.notice.as_ref().filter(|(c, _)| c == cwd).map(|(_, n)| n.as_str())
    }

    /// Forgets why `cwd`'s scope fell back, as it changed since.
    pub fn changed(&mut self, cwd: &str) {
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

    /// Shows `read`, the Compare of `range` in `cwd`; a ref that named no commit points `scope` back at the base, to read again, and git failing to answer keeps what's shown.
    pub fn land(&mut self, cwd: &str, range: Range, read: Result<Option<Comparison>, String>, scope: &mut Scope) -> Landed {
        let Ok(read) = read else { return Landed::Same };
        if self.pending.as_ref().is_some_and(|(c, r)| c == cwd && *r == range) {
            self.pending = None;
        }
        if read.is_none() && self.fall_back(cwd, scope, &range.against) {
            return Landed::Again;
        }
        let shown = Shown { cwd: cwd.to_string(), range, comparison: read };
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
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Branch, tag or commit…"));
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

    /// `None` until git has listed them.
    fn project_trees(&self, cwd: &str) -> Option<Vec<git::Worktree>> {
        let projects = self.projects();
        self.project_of(cwd, &projects).and_then(|p| self.listed_trees(p))
    }

    /// What worktree `cwd`'s `scope` compares against; the project's worktrees are listed only when it names one.
    pub(crate) fn scope_target(&self, cwd: &str, scope: &Scope) -> Target {
        let base = self.repos.get(cwd).and_then(|r| r.base.as_deref());
        let trees = if matches!(scope.against, Against::Worktree(_)) { self.project_trees(cwd) } else { None };
        target(&scope.against, base, trees.as_deref())
    }

    /// What the shown worktree compares; `None` in Uncommitted, or until it names a ref.
    fn compare_range(&self) -> Option<Range> {
        let cwd = self.cwd()?;
        let scope = self.scope(&cwd);
        if !scope.compare {
            return None;
        }
        self.scope_target(&cwd, &scope).range(&scope)
    }

    /// Reads the shown worktree's Compare off the UI thread, pointing its scope back at the base once the worktree it compared against is gone.
    pub(crate) fn load_compare(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        let mut scope = self.scope(&cwd);
        if !scope.compare {
            return;
        }
        let target = self.scope_target(&cwd, &scope);
        if let Target::Gone(name) = &target
            && self.compare.view.fall_back(&cwd, &mut scope, name)
        {
            self.set_scope(cwd, scope, cx);
            cx.notify();
            return self.load_compare(cx);
        }
        let Some(range) = target.range(&scope) else { return };
        self.compare.view.pending = Some((cwd.clone(), range.clone()));
        let untracked = self.store.git.untracked;
        let task = cx.background_executor().spawn({
            let (cwd, range) = (cwd.clone(), range.clone());
            async move { git::compare::read(&hosts::runner_for(&cwd), &cwd, &range, untracked) }
        });
        cx.spawn(async move |this, cx| {
            let read = task.await;
            this.update(cx, |d, cx| {
                if d.cwd().as_ref() != Some(&cwd) || d.compare_range().as_ref() != Some(&range) {
                    return;
                }
                let mut scope = d.scope(&cwd);
                match d.compare.view.land(&cwd, range, read, &mut scope) {
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
        self.compare.view.changed(&cwd);
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

    /// Opens the ref menu emptied and focused, listing the worktree's branches once git has read them.
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

    /// The refs the ref menu offers for what its search holds.
    pub(crate) fn compare_choices(&self, cx: &App) -> Vec<Choice> {
        let Some((cwd, repo)) = self.cwd().and_then(|c| Some((c.clone(), self.repos.get(&c)?))) else { return Vec::new() };
        let name = |w: &git::Worktree| self.agents.given_name(&w.path).map_or_else(|| tree_place(w), str::to_string);
        let trees = siblings(&self.project_trees(&cwd).unwrap_or_default(), &cwd, name);
        choices(repo.base.as_deref(), &trees, &repo.branch, &self.compare.branches, &self.compare.remote, &self.compare.search.read(cx).value())
    }

    /// Opens `path` as the Compare on screen shows it.
    pub(crate) fn open_compare_file(&mut self, path: String, pin: bool, cx: &mut Context<Self>) {
        let Some(Range { against, direct, working }) = self.compare.view.range().cloned() else { return };
        self.open_doc(Doc::CompareFile { against, direct, working, path }, pin, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{Choice, CompareView, Landed, Sibling, Target, choices, siblings, switch_label, target};
    use git::compare::{Comparison, Range};
    use store::scope::{Against, Scope};

    fn worktree(path: &str, branch: &str) -> git::Worktree {
        git::Worktree { path: path.into(), branch: branch.into(), main: false }
    }

    fn sibling(name: &str, branch: &str) -> Sibling {
        Sibling { path: format!("/w/{name}"), name: name.into(), branch: branch.into() }
    }

    fn range(against: &str) -> Range {
        Range { against: against.into(), direct: false, working: false }
    }

    #[test]
    fn a_compare_targets_the_base_another_worktrees_branch_or_the_ref_given() {
        let trees = [worktree("/w/fix", "fix/a"), worktree("/w/look", "detached")];
        let worktree_at = |path: &str| Against::Worktree(path.into());
        assert_eq!(target(&Against::Base, Some("origin/main"), None), Target::Ref("origin/main".into()));
        assert_eq!(target(&Against::Base, None, None), Target::NoBase);
        assert_eq!(target(&worktree_at("/w/fix"), None, Some(&trees)), Target::Ref("fix/a".into()));
        assert_eq!(target(&Against::Ref("v1.2".into()), None, None), Target::Ref("v1.2".into()));
    }

    #[test]
    fn a_worktree_compared_against_is_gone_only_once_the_worktrees_are_listed() {
        let trees = [worktree("/w/look", "detached")];
        let worktree_at = |path: &str| Against::Worktree(path.into());
        assert_eq!(target(&worktree_at("/w/fix"), None, None), Target::Unlisted);
        assert_eq!(target(&worktree_at("/w/fix"), None, Some(&trees)), Target::Gone("fix".into()));
        assert_eq!(target(&worktree_at("/w/look"), None, Some(&trees)), Target::Gone("look".into()), "a detached tree names no branch");
    }

    #[test]
    fn a_target_reads_the_scopes_kind_of_compare_once_it_names_a_ref() {
        let scope = Scope { compare: true, direct: true, working: true, ..Scope::default() };
        assert_eq!(Target::Ref("main".into()).range(&scope), Some(Range { against: "main".into(), direct: true, working: true }));
        assert_eq!(Target::Unlisted.range(&scope), None);
    }

    #[test]
    fn the_menu_offers_the_other_worktrees_on_a_branch_by_their_name() {
        let trees = [worktree("/w/here", "here"), worktree("/w/fix", "fix/a"), worktree("/w/look", "detached")];
        assert_eq!(siblings(&trees, "/w/here", |w| format!("named {}", w.branch)), [Sibling { path: "/w/fix".into(), name: "named fix/a".into(), branch: "fix/a".into() }]);
    }

    #[test]
    fn a_read_shows_once_and_again_only_when_it_changed() {
        let mut view = CompareView { pending: Some(("/w/fix".into(), range("main"))), ..CompareView::default() };
        let mut scope = Scope { compare: true, ..Scope::default() };
        let read = Ok(Some(Comparison { counts: (1, 0), ..Comparison::default() }));
        assert_eq!(view.land("/w/fix", range("main"), read.clone(), &mut scope), Landed::Changed);
        assert_eq!(view.pending, None);
        assert_eq!(view.land("/w/fix", range("main"), read, &mut scope), Landed::Same);
        assert!(view.get("/w/fix", &range("main")).is_some());
        assert!(view.get("/w/other", &range("main")).is_none());
        assert!(view.get("/w/fix", &range("dev")).is_none());
    }

    #[test]
    fn git_failing_to_answer_keeps_what_is_shown_and_the_scope_and_waits_for_the_next_refresh() {
        let mut view = CompareView::default();
        let mut scope = Scope { compare: true, against: Against::Ref("feat".into()), ..Scope::default() };
        assert_eq!(view.land("/w/fix", range("feat"), Ok(Some(Comparison::default())), &mut scope), Landed::Changed);
        view.pending = Some(("/w/fix".into(), range("feat")));
        assert_eq!(view.land("/w/fix", range("feat"), Err("not connected".into()), &mut scope), Landed::Same);
        assert_eq!(scope.against, Against::Ref("feat".into()));
        assert!(view.get("/w/fix", &range("feat")).is_some());
        assert_eq!(view.pending, Some(("/w/fix".into(), range("feat"))), "a redraw doesn't ask again");
    }

    #[test]
    fn a_ref_that_went_away_falls_back_to_the_base_and_says_so_in_that_worktree_until_its_scope_changes() {
        let mut view = CompareView::default();
        let mut scope = Scope { compare: true, against: Against::Ref("old-branch".into()), direct: true, working: false };
        assert_eq!(view.land("/w/fix", range("old-branch"), Ok(None), &mut scope), Landed::Again);
        assert_eq!(scope, Scope { compare: true, against: Against::Base, direct: true, working: false });
        assert_eq!(view.notice("/w/fix"), Some("Can't find old-branch, so this compares with the base."));
        assert_eq!(view.notice("/w/other"), None);
        view.changed("/w/other");
        assert!(view.notice("/w/fix").is_some());
        view.changed("/w/fix");
        assert_eq!(view.notice("/w/fix"), None);
    }

    #[test]
    fn a_base_that_names_no_commit_shows_as_missing_instead_of_falling_back() {
        let mut view = CompareView::default();
        let mut scope = Scope { compare: true, ..Scope::default() };
        assert_eq!(view.land("/w/fix", range("origin/main"), Ok(None), &mut scope), Landed::Changed);
        assert_eq!(view.get("/w/fix", &range("origin/main")).map(|s| s.comparison.is_none()), Some(true));
        assert_eq!(view.notice("/w/fix"), None);
    }

    fn labels(choices: &[Choice]) -> Vec<String> {
        choices
            .iter()
            .map(|c| match c {
                Choice::Base(r) => format!("base {r}"),
                Choice::Worktree(t) => format!("tree {} {}", t.name, t.branch),
                Choice::Branch(r) => format!("branch {r}"),
                Choice::Typed(r) => format!("typed {r}"),
            })
            .collect()
    }

    #[test]
    fn the_menu_offers_the_base_then_worktrees_then_branches_no_worktree_is_on() {
        let trees = [sibling("other", "feat/x")];
        let branches = [("feat/x".to_string(), None), ("main".to_string(), None), ("mine".to_string(), None), ("old".to_string(), None)];
        let got = choices(Some("origin/main"), &trees, "mine", &branches, &["remote-only".into()], "");
        assert_eq!(labels(&got), ["base origin/main", "tree other feat/x", "branch main", "branch old"]);
    }

    #[test]
    fn a_search_filters_every_kind_and_offers_what_was_typed_when_nothing_is_named_that() {
        let trees = [sibling("other", "feat/x")];
        let branches = [("feat/y".to_string(), None), ("main".to_string(), None)];
        let remote = ["feat/z".to_string(), "main".to_string()];
        assert_eq!(labels(&choices(Some("origin/main"), &trees, "mine", &branches, &remote, "feat")), ["tree other feat/x", "branch feat/y", "branch origin/feat/z", "typed feat"]);
        assert_eq!(labels(&choices(Some("origin/main"), &trees, "mine", &branches, &remote, "v1.2")), ["typed v1.2"]);
        assert_eq!(labels(&choices(Some("origin/main"), &trees, "mine", &branches, &remote, "main")), ["base origin/main", "branch main"]);
    }

    #[test]
    fn picking_a_choice_compares_against_what_it_names() {
        assert_eq!(Choice::Base("origin/main".into()).against(), Against::Base);
        assert_eq!(Choice::Worktree(sibling("other", "feat/x")).against(), Against::Worktree("/w/other".into()));
        assert_eq!(Choice::Branch("origin/feat/z".into()).against(), Against::Ref("origin/feat/z".into()));
        assert_eq!(Choice::Typed("abc123".into()).against(), Against::Ref("abc123".into()));
    }

    #[test]
    fn the_switch_says_uncommitted_or_the_ref_with_its_counts() {
        let compare = Scope { compare: true, ..Scope::default() };
        assert_eq!(switch_label(&Scope::default(), Some("origin/main"), Some((4, 2))), "Uncommitted");
        assert_eq!(switch_label(&compare, Some("origin/main"), None), "vs origin/main");
        assert_eq!(switch_label(&compare, Some("origin/main"), Some((4, 2))), "vs origin/main · 4 ahead · 2 behind");
        assert_eq!(switch_label(&compare, None, None), "vs base");
    }
}
