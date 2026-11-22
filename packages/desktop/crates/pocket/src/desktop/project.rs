use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, Screen, Side, state};
use crate::explorer::{self, preview};
use crate::fs::fs_for;
use crate::git_ui::diff;
use crate::status::{self, Card, Status};
use crate::terminals::sessions::Session;
use crate::util;
use agents::Summary;
use agents::locals::is_local;
use git::Repo;
use gpui_kit::*;
use std::collections::{BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::Instant;
use store::prefs::sidebar::SessionSort;
use theme::*;
use ui::State;
use workspace::Workspace;

/// A terminal, the tree it is placed in if any, and whether it is hidden: a delegated session's or a sign-in's.
type Placed = (String, Option<String>, bool);

fn hidden_terminals<'a>(list: &'a [Summary], sessions: &'a [Session]) -> HashSet<&'a str> {
    let mut hidden = status::hidden_terminals(list);
    hidden.extend(sessions.iter().filter(|s| s.info.signin).map(|s| s.info.id.as_str()));
    hidden
}

/// The terminals `tree` holds, and those it doesn't: a delegated session is held by no tree, so it never gets a tab there.
fn held(tree: &str, terminals: &[Placed]) -> (Vec<String>, Vec<String>) {
    let mut split = (Vec::new(), Vec::new());
    for (id, place, hidden) in terminals {
        match place {
            Some(t) if t == tree && !hidden => split.0.push(id.clone()),
            Some(_) => split.1.push(id.clone()),
            None if *hidden => split.1.push(id.clone()),
            None => {}
        }
    }
    split
}

/// The trees that hold delegated sessions and nothing else; they exist for the sake of those.
fn child_only(terminals: &[Placed]) -> HashSet<&str> {
    let held_by = |hidden: bool| terminals.iter().filter(move |t| t.2 == hidden).filter_map(|t| t.1.as_deref());
    let others: HashSet<&str> = held_by(false).collect();
    held_by(true).filter(|t| !others.contains(t)).collect()
}

/// `trees` without those made for delegated sessions; a project's main tree always stays.
fn without_child_only(trees: Vec<git::Worktree>, terminals: &[Placed]) -> Vec<git::Worktree> {
    let only = child_only(terminals);
    trees.into_iter().filter(|w| w.main || !only.contains(w.path.as_str())).collect()
}

/// Every project, and each one's sessions as cards, worked out once while the sidebar draws.
pub(crate) struct Placement {
    projects: Vec<String>,
    cards: HashMap<String, Vec<Card>>,
}

/// Each project's cards, sorted as one project's are.
fn by_project<'a>(placed: impl Iterator<Item = (&'a String, Card)>, by: SessionSort) -> HashMap<String, Vec<Card>> {
    let mut cards: HashMap<String, Vec<Card>> = HashMap::new();
    for (p, card) in placed {
        cards.entry(p.clone()).or_default().push(card);
    }
    for list in cards.values_mut() {
        status::sort(list, by);
    }
    cards
}

impl Desktop {
    pub fn projects(&self) -> Vec<String> {
        if let Some(placed) = self.sidebar.placement.borrow().as_ref() {
            return placed.projects.clone();
        }
        let placeable = self.git_done > 0 || project::all_listed(&self.store.projects, &self.worktrees, readable);
        let hidden = hidden_terminals(&self.agents.list, &self.terminals.sessions.items);
        let sessions = if placeable { self.terminals.sessions.items.as_slice() } else { &[] };
        project::projects(&self.store.projects, sessions.iter().filter(|s| !hidden.contains(s.info.id.as_str())).map(|s| s.info.cwd.as_str()), &self.worktrees)
    }

    fn placed_terminals(&self) -> Vec<Placed> {
        let hidden = hidden_terminals(&self.agents.list, &self.terminals.sessions.items);
        self.terminals.sessions.items.iter().map(|s| (s.info.id.clone(), self.place_of(&s.info), hidden.contains(s.info.id.as_str()))).collect()
    }

    /// Lists each saved project's worktrees on its own, so the sidebar fills in as each lands instead of after the first `refresh_git`.
    pub(crate) fn list_worktrees(&mut self, cx: &mut Context<Self>) {
        for p in self.store.projects.iter().filter(|p| readable(p)).cloned() {
            let task = cx.background_executor().spawn({
                let p = p.clone();
                async move { git::worktrees(&hosts::runner_for(&p), &p) }
            });
            cx.spawn(async move |this, cx| {
                let trees = task.await;
                this.update(cx, |d, cx| {
                    // A finished refresh_git has listed them already.
                    if d.git_done == 0 {
                        d.worktrees.entry(p).or_insert(trees);
                        cx.notify();
                    }
                })
                .ok();
            })
            .detach();
        }
    }

    /// Lists the worktrees of each session's repo that no project holds yet, as one a handoff or a host's own desktop
    /// made, so its session shows in its project without waiting for `refresh_git` to read every tree.
    pub(crate) fn look_up_trees(&mut self, cx: &mut Context<Self>) {
        // Until every project is listed, each session would look unheld.
        if self.git_done == 0 && !project::all_listed(&self.store.projects, &self.worktrees, readable) {
            return;
        }
        let cwds: Vec<&str> = self.terminals.sessions.items.iter().map(|s| s.info.cwd.as_str()).filter(|c| readable(c)).collect();
        for cwd in self.tree_lookups.ask(cwds, &self.store.projects, &self.worktrees) {
            let task = cx.background_executor().spawn(async move { git::worktrees(&hosts::runner_for(&cwd), &cwd) });
            cx.spawn(async move |this, cx| {
                let trees = task.await;
                this.update(cx, |d, cx| {
                    let Some(main) = trees.iter().find(|w| w.main).map(|w| w.path.clone()).filter(|m| d.store.projects.contains(m)) else { return };
                    d.tree_lookups.listed(&main, d.git_run);
                    d.worktrees.insert(main, trees);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }

    pub fn project_of<'a>(&self, cwd: &str, projects: &'a [String]) -> Option<&'a String> {
        project::project_of(cwd, projects, &self.worktrees)
    }

    /// The project `a` runs in: among `projects`, the one pocketd placed it in while its folder `cwd` is one of that
    /// project's, so its row shows it; else the one `cwd` is in.
    pub fn agent_project<'a>(&self, a: &agents::Summary, cwd: &str, projects: &'a [String]) -> Option<&'a String> {
        let host = hosts::session::untag(&a.id).0;
        let placed = Some(a.project_id.as_str()).filter(|id| !id.is_empty()).and_then(|id| self.store.project_with_id(host, id));
        let placed = placed.and_then(|p| projects.iter().find(|q| *q == p)).filter(|p| self.project_of(cwd, std::slice::from_ref(*p)).is_some());
        placed.or_else(|| self.project_of(cwd, projects))
    }

    pub fn worktree_of(&self, cwd: &str) -> Option<&git::Worktree> {
        project::worktree_of(cwd, &self.worktrees, |t| self.store.lists(t))
    }

    /// The worktrees `p` lists; `None` until git has listed them.
    pub fn listed_trees(&self, p: &str) -> Option<Vec<git::Worktree>> {
        let trees = self.worktrees.get(p).map(|w| project::listed(w, |t| self.store.lists(t)))?;
        if status::hidden_children(&self.agents.list).is_empty() {
            return Some(trees);
        }
        Some(without_child_only(trees, &self.placed_terminals()))
    }

    pub(crate) fn untracked_trees(&self, p: &str) -> Vec<git::Worktree> {
        self.worktrees.get(p).map(|w| project::untracked(w, |t| self.store.lists(t))).unwrap_or_default()
    }

    pub(crate) fn import_worktrees(&mut self, p: &str, cx: &mut Context<Self>) {
        for w in self.untracked_trees(p) {
            self.store.track(&w.path);
        }
        self.store.collapsed.remove(p);
        self.save_soon(cx);
        cx.notify();
    }

    pub fn tree_of(&self, cwd: &str) -> Option<String> {
        project::tree_of(cwd, &self.worktrees, |t| self.store.lists(t), || self.projects())
    }

    pub fn cards(&self, project: &str) -> Vec<Card> {
        if let Some(placed) = self.sidebar.placement.borrow().as_ref() {
            return placed.cards.get(project).cloned().unwrap_or_default();
        }
        let projects = self.projects();
        let mut out: Vec<Card> = self.placed_cards(&projects).filter(|(p, _)| *p == project).map(|(_, c)| c).collect();
        status::sort(&mut out, self.store.sidebar.sort);
        out
    }

    /// Each listed session's card, with the project it sits in.
    fn placed_cards<'a>(&'a self, projects: &'a [String]) -> impl Iterator<Item = (&'a String, Card)> + 'a {
        status::listed(&self.agents.list).into_iter().filter_map(|a| Some((a, self.terminals.sessions.get(&a.terminal_id)?))).filter_map(|(a, s)| {
            Some((self.agent_project(a, &s.info.cwd, projects)?, status::Card { local: s.info.local.clone(), ..status::card(a, &s.info.cwd) }))
        })
    }

    /// Runs `f` with `projects` and `cards` answered from one pass over the sessions. Every sidebar row asks for its
    /// project's cards, and placing each session again per row took most of a frame with 60 sessions.
    pub(crate) fn placed<R>(&self, f: impl FnOnce() -> R) -> R {
        let projects = self.projects();
        let cards = by_project(self.placed_cards(&projects), self.store.sidebar.sort);
        self.sidebar.placement.replace(Some(Placement { projects, cards }));
        let out = f();
        self.sidebar.placement.take();
        out
    }

    /// The agent in `terminal`.
    pub fn summary(&self, terminal: &str) -> Option<&Summary> {
        self.agents.in_terminal(terminal)
    }

    /// The selected session: the agent in the focused pane.
    pub fn session(&self) -> Option<&str> {
        self.summary(self.terminal.focused.as_deref()?).map(|a| a.id.as_str())
    }

    pub fn cwd_of(&self, id: &str) -> Option<String> {
        self.agents.get(id).map(|a| a.cwd.clone()).or_else(|| self.terminals.sessions.get(id).map(|s| s.info.cwd.clone()))
    }

    /// The tree on screen, by its worktree's path or its Local's id: the one picked, else the project's main worktree.
    pub fn place(&self) -> Option<String> {
        self.worktree.clone().or_else(|| self.tree_of(self.project.as_deref()?))
    }

    pub fn cwd(&self) -> Option<String> {
        self.folder_of(&self.place()?)
    }

    /// A tree's folder; a Local's is its project's main worktree.
    pub(crate) fn folder_of(&self, key: &str) -> Option<String> {
        if !is_local(key) {
            return Some(key.to_string());
        }
        let project = self.agents.local(key).map(|l| l.project.clone()).or_else(|| self.project.clone())?;
        self.tree_of(&project)
    }

    /// A tree's short name: the one it was given, else its folder's.
    pub(crate) fn place_name(&self, key: &str) -> String {
        self.agents.given_name(key).map_or_else(|| util::basename(key), str::to_string)
    }

    /// The tree a terminal belongs to: the Local it was opened in, else the worktree holding its folder.
    pub(crate) fn place_of(&self, info: &daemon::Info) -> Option<String> {
        if info.local.is_empty() { self.tree_of(&info.cwd) } else { Some(info.local.clone()) }
    }

    pub fn repo(&self) -> Option<&Repo> {
        self.repos.get(&self.cwd()?)
    }

    pub(crate) fn workspace(&mut self, tree: &str) -> &mut Workspace {
        let (mine, theirs) = held(tree, &self.placed_terminals());
        let w = self.workspaces.entry(tree.to_string()).or_default();
        w.sync(&mine, &theirs);
        w
    }

    pub(crate) fn project_terminals(&self, p: &str) -> Vec<String> {
        let projects = self.projects();
        self.terminals.sessions.items.iter().filter(|s| self.project_of(&s.info.cwd, &projects).is_some_and(|o| o == p)).map(|s| s.info.id.clone()).collect()
    }

    /// Whether `tree` has a terminal or a tab open.
    pub(crate) fn in_use(&self, tree: &str) -> bool {
        self.workspaces.get(tree).is_some_and(|w| !w.is_empty()) || !held(tree, &self.placed_terminals()).0.is_empty()
    }

    pub(crate) fn tree_terminals(&self, tree: &str) -> Vec<String> {
        self.terminals.sessions.items.iter().filter(|s| self.place_of(&s.info).as_deref() == Some(tree)).map(|s| s.info.id.clone()).collect()
    }

    fn git_cwds(&self) -> Vec<String> {
        let sessions = self.project.as_deref().map(|p| self.cards(p).into_iter().map(|c| c.cwd).collect()).unwrap_or_default();
        let listed: project::Worktrees = self.project.iter().filter_map(|p| Some((p.clone(), self.listed_trees(p)?))).collect();
        project::git_cwds(self.project.as_deref(), sessions, &listed, self.cwd())
    }

    pub fn refresh_git(&mut self, cx: &mut Context<Self>) {
        let cwds = self.git_cwds();
        let projects = self.store.projects.clone();
        let shown = self.store.files.clone();
        let files: Vec<_> = self
            .preview
            .panes
            .iter()
            .filter_map(|(p, f)| {
                let f = f.file.clone()?;
                let changed = self.file_status(&f).is_some();
                Some((*p, f, changed))
            })
            .collect();
        let root = self.cwd();
        let known: BTreeSet<String> = self
            .store
            .tracked
            .iter()
            .chain(&self.store.scratch)
            .cloned()
            .chain(self.worktrees.values().flatten().filter(|w| !w.main).map(|w| w.path.clone()))
            .collect();
        let (untracked, options) = (self.store.git.untracked, self.diff.options);
        let diffs: Vec<_> =
            self.diff.panes.iter().filter_map(|(p, v)| Some((*p, v.followed_file()?.to_string(), v.at.clone(), v.open.clone(), v.lines.clone()))).collect();
        let mut dirs: Vec<PathBuf> = self.explorer.tree.keys().cloned().collect();
        dirs.extend(self.explore_root().map(PathBuf::from));
        self.git_run += 1;
        let run = self.git_run;
        let task = cx.background_executor().spawn(async move {
            let repos: Vec<(String, Option<Repo>)> = cwds
                .into_iter()
                .map(|c| {
                    let r = git::read(&hosts::runner_for(&c), &c, untracked);
                    (c, r)
                })
                .collect();
            let diffs: Vec<_> = root
                .map(|cwd| diffs.into_iter().map(|(p, path, at, open, shown)| (p, diff::read_diff(&cwd, path, at, open, options, &shown))).collect())
                .unwrap_or_default();
            let tree = util::list_dirs(dirs, &shown);
            let (projects, unread): (Vec<String>, Vec<String>) = projects.into_iter().partition(|p| readable(p));
            let worktrees: HashMap<String, Vec<git::Worktree>> = projects
                .into_iter()
                .map(|p| {
                    let w = git::worktrees(&hosts::runner_for(&p), &p);
                    (p, w)
                })
                .collect();
            let files: Vec<_> = files.into_iter().map(|(p, f, changed)| (p, preview::load(&f, changed))).collect();
            let gone = project::vanished(known, &worktrees, |t| fs_for(t).exists(Path::new(t)));
            (repos, diffs, tree, worktrees, unread, files, gone)
        });
        cx.spawn(async move |this, cx| {
            let (repos, diffs, tree, worktrees, unread, files, gone) = task.await;
            this.update(cx, |d, cx| {
                if run != d.git_run {
                    return;
                }
                d.git_done = run;
                let tree = explorer::merge_tree(tree, &d.explorer.tree, d.explore_root().as_deref().map(std::path::Path::new));
                // Before `worktrees` is replaced, while it still places their terminals.
                for t in &gone {
                    if d.store.is_scratch(t) {
                        d.delete_scratch(t, cx);
                    } else {
                        d.forget_tree(t, cx);
                    }
                }
                let mut keep = unread;
                keep.extend(d.tree_lookups.since(run));
                let worktrees = project::refreshed(worktrees, &d.worktrees, &keep);
                d.renames.relisted(d.worktrees.values().flatten(), worktrees.values().flatten(), Instant::now());
                let repos_changed = merge_repos(&mut d.repos, repos);
                let mut changed = !gone.is_empty() || repos_changed || tree != d.explorer.tree || worktrees != d.worktrees;
                (d.explorer.tree, d.worktrees) = (tree, worktrees);
                d.activity_changed(cx);
                if d.screen == Screen::Board {
                    d.rescue_board_composer(cx);
                }
                d.poll_prs(cx);
                d.detect_gh_account(cx);
                if d.side == Side::Changes && d.sidebar.panel_open {
                    d.load_compare(cx);
                }
                for (pane, file) in files {
                    changed |= d.preview.apply(pane, file);
                }
                for (pane, load) in diffs {
                    changed |= d.diff.apply(pane, load);
                }
                if changed {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
        self.refresh_graph(cx);
    }

    /// Reads only the tree on screen, so a switch shows its changes without waiting for `refresh_git` to read every tree.
    pub(crate) fn load_shown_repo(&mut self, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        let (run, untracked) = (self.git_run, self.store.git.untracked);
        let task = cx.background_executor().spawn({
            let cwd = cwd.clone();
            async move { git::read(&hosts::runner_for(&cwd), &cwd, untracked) }
        });
        cx.spawn(async move |this, cx| {
            let repo = task.await;
            this.update(cx, |d, cx| {
                // A newer refresh or a stage reads the index after this one did.
                if run == d.git_run && d.cwd().as_ref() == Some(&cwd) && merge_repos(&mut d.repos, vec![(cwd, repo)]) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn repo_name(&self, path: &str) -> String {
        if self.store.is_scratch(path) { self.place_name(path) } else { self.store.repo_name(path) }
    }

    pub fn repo_color(&self, path: &str) -> u32 {
        let fallback = || PALETTE[self.projects().iter().position(|p| p == path).unwrap_or(0) % PALETTE.len()];
        self.store.repos.get(path).map(|r| r.color).filter(|c| *c != 0).unwrap_or_else(fallback)
    }

    /// The project's tile mark, with `letters` where it has no icon of its own.
    pub fn project_mark(&self, path: &str, letters: String) -> ui::Mark {
        util::mark(self.store.repos.get(path).and_then(|r| r.icon.as_ref()), letters)
    }

    pub fn roll_up(&self, p: &str) -> Option<(Status, usize)> {
        status::roll_up(self.cards(p).iter().map(|c| c.status))
    }

    pub fn project_state(&self, p: &str) -> Option<State> {
        self.roll_up(p).map(|(s, _)| state(s, 0, 0))
    }

    pub(crate) fn keep_project(&mut self, p: &str, cx: &mut Context<Self>) {
        self.store.add(p);
        self.store.save();
        self.publish_registry();
        self.refresh_git(cx);
        cx.notify();
    }

    /// Closes the project's terminals, deletes its Locals and takes it off the sidebar; its folder is untouched.
    pub(crate) fn remove_project(&mut self, p: &str, cx: &mut Context<Self>) {
        for id in self.project_terminals(p) {
            self.close_pane(&id, cx);
        }
        let locals: Vec<String> = self.agents.locals_removed_with(p).map(str::to_string).collect();
        for id in locals {
            self.delete_local(&id, cx);
        }
        self.store.remove(p);
        self.store.save();
        self.publish_registry();
        let only = [p.to_string()];
        self.repos.retain(|cwd, _| project::project_of(cwd, &only, &self.worktrees).is_none());
        self.worktrees.remove(p);
        if self.project.as_deref() == Some(p) {
            self.set_project(None, cx);
            if let Some(next) = self.projects().into_iter().next() {
                self.select_tree(next, None, cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn ask_remove_project(&mut self, p: String, cx: &mut Context<Self>) {
        if self.project_terminals(&p).is_empty() && self.agents.locals_removed_with(&p).next().is_none() {
            self.remove_project(&p, cx);
        } else {
            self.confirm = Some(Confirm::RemoveProject(p));
            self.overlay = Some(Overlay::Confirm);
        }
        cx.notify();
    }
}

/// Whether git can read `p` now: it is on this Mac, or on a connected host.
fn readable(p: &str) -> bool {
    hosts::split(p).0.is_none_or(|h| hosts::remote::connected(h).is_some())
}

/// Applies `read`, keeping folders it skipped so switching back shows them at once, and those on a host that went offline,
/// whose failed read says nothing about the repo. Returns whether anything changed.
fn merge_repos(repos: &mut HashMap<String, Repo>, read: Vec<(String, Option<Repo>)>) -> bool {
    let mut changed = false;
    for (cwd, repo) in read {
        changed |= match repo {
            Some(r) if repos.get(&cwd) != Some(&r) => {
                repos.insert(cwd, r);
                true
            }
            Some(_) => false,
            None if !readable(&cwd) => false,
            None => repos.remove(&cwd).is_some(),
        };
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::{HashMap, Placed, Repo, SessionSort, by_project, child_only, held, hidden_terminals, merge_repos, without_child_only};

    fn repo(branch: &str) -> Repo {
        Repo { branch: branch.into(), ..Repo::default() }
    }

    #[test]
    fn a_host_that_cannot_be_reached_keeps_the_repo_it_showed() {
        let mut repos = HashMap::from([("//devbox/home/me/app".to_string(), repo("main"))]);
        assert!(!merge_repos(&mut repos, vec![("//devbox/home/me/app".into(), None)]));
        assert_eq!(repos["//devbox/home/me/app"].branch, "main");
    }

    #[test]
    fn a_refresh_keeps_the_repos_it_did_not_read() {
        let mut repos = HashMap::from([("/a".to_string(), repo("main")), ("/b".to_string(), repo("main"))]);
        assert!(merge_repos(&mut repos, vec![("/a".into(), Some(repo("fix")))]));
        assert_eq!((repos["/a"].branch.as_str(), repos["/b"].branch.as_str()), ("fix", "main"));
    }

    #[test]
    fn a_folder_no_longer_a_repo_is_dropped() {
        let mut repos = HashMap::from([("/a".to_string(), repo("main"))]);
        assert!(merge_repos(&mut repos, vec![("/a".into(), None)]));
        assert!(repos.is_empty());
    }

    #[test]
    fn reading_the_same_state_changes_nothing() {
        let mut repos = HashMap::from([("/a".to_string(), repo("main"))]);
        assert!(!merge_repos(&mut repos, vec![("/a".into(), Some(repo("main"))), ("/b".into(), None)]));
    }

    #[test]
    fn each_project_gets_only_its_own_cards_in_sort_order() {
        let card = |id: &str, created: i64| crate::status::card(&agents::Summary { id: id.into(), created_at: created, ..Default::default() }, "/w");
        let (a, b) = ("/a".to_string(), "/b".to_string());
        let placed = vec![(&a, card("a-old", 1)), (&b, card("b", 5)), (&a, card("a-new", 9))];
        let ids = |cards: &[crate::status::Card]| cards.iter().map(|c| c.id.clone()).collect::<Vec<_>>();
        let grouped = by_project(placed.into_iter(), SessionSort::Newest);
        assert_eq!((ids(&grouped["/a"]), ids(&grouped["/b"]), grouped.len()), (vec!["a-new".to_string(), "a-old".to_string()], vec!["b".to_string()], 2));
    }

    fn at(id: &str, tree: Option<&str>, hidden: bool) -> Placed {
        (id.into(), tree.map(String::from), hidden)
    }

    #[test]
    fn a_delegated_session_is_held_by_no_tree_so_it_gets_no_tab() {
        let terminals = [
            at("mine", Some("/w"), false),
            at("other", Some("/v"), false),
            at("child", Some("/w"), true),
            at("lost-child", None, true),
            at("unplaced", None, false),
        ];
        assert_eq!(held("/w", &terminals), (vec!["mine".to_string()], vec!["other".to_string(), "child".to_string(), "lost-child".to_string()]));
    }

    #[test]
    fn a_projects_main_tree_stays_listed_when_only_delegated_sessions_run_in_it() {
        let tree = |path: &str, main: bool| git::Worktree { path: path.into(), branch: String::new(), main, made: false, created: None };
        let terminals = [at("a", Some("/w"), true), at("b", Some("/w/tests"), true), at("c", Some("/w/docs"), false)];
        let left = without_child_only(vec![tree("/w", true), tree("/w/tests", false), tree("/w/docs", false)], &terminals);
        assert_eq!(left.iter().map(|w| w.path.as_str()).collect::<Vec<_>>(), ["/w", "/w/docs"]);
    }

    #[test]
    fn a_tree_holding_only_delegated_sessions_exists_for_their_sake() {
        let terminals =
            [at("a", Some("/w/tests"), true), at("b", Some("/w/docs"), true), at("c", Some("/w/docs"), false), at("d", Some("/w"), false), at("e", None, true)];
        assert_eq!(child_only(&terminals).into_iter().collect::<Vec<_>>(), ["/w/tests"]);
    }

    #[test]
    fn a_sign_in_terminal_is_hidden_but_a_shell_in_the_same_folder_is_not() {
        let session = |id: &str, signin: bool| crate::terminals::sessions::Session {
            info: daemon::Info { id: id.into(), cwd: "/Users/me".into(), signin, ..Default::default() },
            term: None,
            exit: None,
        };
        let sessions = [session("login", true), session("shell", false)];
        assert_eq!(hidden_terminals(&[], &sessions).into_iter().collect::<Vec<_>>(), ["login"]);
    }
}
