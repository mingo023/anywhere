use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, state};
use crate::explorer::{self, preview};
use crate::fs::fs_for;
use crate::git_ui::diff;
use crate::status::{self, Card, Status};
use crate::util;
use agents::Summary;
use agents::locals::is_local;
use git::Repo;
use gpui_kit::*;
use std::collections::{BTreeSet, HashMap};
use std::path::{Path, PathBuf};
use theme::*;
use ui::State;
use workspace::Workspace;

impl Desktop {
    pub fn projects(&self) -> Vec<String> {
        project::projects(&self.store.projects, self.terminals.sessions.items.iter().map(|s| s.info.cwd.as_str()), &self.worktrees)
    }

    pub fn project_of<'a>(&self, cwd: &str, projects: &'a [String]) -> Option<&'a String> {
        project::project_of(cwd, projects, &self.worktrees)
    }

    pub fn worktree_of(&self, cwd: &str) -> Option<&git::Worktree> {
        project::worktree_of(cwd, &self.worktrees, |t| self.store.lists(t))
    }

    /// The worktrees `p` lists; `None` until git has listed them.
    pub fn listed_trees(&self, p: &str) -> Option<Vec<git::Worktree>> {
        self.worktrees.get(p).map(|w| project::listed(w, |t| self.store.lists(t)))
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
        let projects = self.projects();
        let mut out: Vec<Card> = self
            .agents
            .list
            .iter()
            .filter_map(|a| Some((a, self.terminals.sessions.get(&a.terminal_id)?)))
            .filter(|(_, s)| self.project_of(&s.info.cwd, &projects).is_some_and(|p| p == project))
            .map(|(a, s)| status::Card { local: s.info.local.clone(), ..status::card(a, &s.info.cwd) })
            .collect();
        status::sort(&mut out, self.store.sidebar.sort);
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
        let (mut mine, mut theirs) = (Vec::new(), Vec::new());
        for s in &self.terminals.sessions.items {
            match self.place_of(&s.info) {
                Some(t) if t == tree => mine.push(s.info.id.clone()),
                Some(_) => theirs.push(s.info.id.clone()),
                None => {}
            }
        }
        let w = self.workspaces.entry(tree.to_string()).or_default();
        w.sync(&mine, &theirs);
        w
    }

    pub(crate) fn project_terminals(&self, p: &str) -> Vec<String> {
        let projects = self.projects();
        self.terminals.sessions.items.iter().filter(|s| self.project_of(&s.info.cwd, &projects).is_some_and(|o| o == p)).map(|s| s.info.id.clone()).collect()
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
        let files: Vec<_> = self.preview.panes.iter().filter_map(|(p, f)| {
            let f = f.file.clone()?;
            let changed = self.file_status(&f).is_some();
            Some((*p, f, changed))
        }).collect();
        let root = self.cwd();
        let known: BTreeSet<String> = self.store.tracked.iter().cloned().chain(self.worktrees.values().flatten().filter(|w| !w.main).map(|w| w.path.clone())).collect();
        let (untracked, options) = (self.store.git.untracked, self.diff.options);
        let diffs: Vec<_> = self.diff.panes.iter().filter_map(|(p, v)| Some((*p, v.working_file()?.to_string(), v.open.clone(), v.lines.clone()))).collect();
        let mut dirs: Vec<PathBuf> = self.explorer.tree.keys().cloned().collect();
        dirs.extend(self.explore_root().map(PathBuf::from));
        self.git_run += 1;
        let run = self.git_run;
        let task = cx.background_executor().spawn(async move {
            let repos: Vec<(String, Option<Repo>)> = cwds.into_iter().map(|c| {
                let r = git::read(&hosts::runner_for(&c), &c, untracked);
                (c, r)
            }).collect();
            let diffs: Vec<_> = root.map(|cwd| diffs.into_iter().map(|(p, path, open, shown)| (p, diff::read_diff(&cwd, path, diff::At::Working, open, options, &shown))).collect()).unwrap_or_default();
            let initials = repos.first().map(|(c, _)| git::user_initials(&hosts::runner_for(c), c)).unwrap_or_default();
            let tree: HashMap<PathBuf, Vec<(bool, PathBuf)>> = dirs.into_iter().map(|d| {
                let listing = util::list_dir(&d, &shown);
                (d, listing)
            }).collect();
            let (projects, unread): (Vec<String>, Vec<String>) = projects.into_iter().partition(|p| hosts::split(p).0.is_none_or(|h| hosts::remote::connected(h).is_some()));
            let worktrees: HashMap<String, Vec<git::Worktree>> = projects.into_iter().map(|p| {
                let w = git::worktrees(&hosts::runner_for(&p), &p);
                (p, w)
            }).collect();
            let files: Vec<_> = files.into_iter().map(|(p, f, changed)| (p, preview::load(&f, changed))).collect();
            let gone = project::vanished(known, &worktrees, |t| fs_for(t).exists(Path::new(t)));
            (repos, diffs, initials, tree, worktrees, unread, files, gone)
        });
        cx.spawn(async move |this, cx| {
            let (repos, diffs, initials, tree, worktrees, unread, files, gone) = task.await;
            this.update(cx, |d, cx| {
                if run != d.git_run {
                    return;
                }
                d.git_done = run;
                let tree = explorer::merge_tree(tree, &d.explorer.tree, d.explore_root().as_deref().map(std::path::Path::new));
                // Before `worktrees` is replaced, while it still places their terminals.
                for t in &gone {
                    d.forget_tree(t, cx);
                }
                let worktrees = project::refreshed(worktrees, &d.worktrees, &unread);
                let repos_changed = merge_repos(&mut d.repos, repos);
                let mut changed = !gone.is_empty() || repos_changed || tree != d.explorer.tree || initials != d.initials || worktrees != d.worktrees;
                (d.explorer.tree, d.initials, d.worktrees) = (tree, initials, worktrees);
                d.poll_prs(cx);
                d.detect_gh_account(cx);
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
        self.store.repo_name(path)
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
        let only = [p.to_string()];
        self.repos.retain(|cwd, _| project::project_of(cwd, &only, &self.worktrees).is_none());
        self.worktrees.remove(p);
        if self.project.as_deref() == Some(p) {
            self.set_project(None);
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

/// Applies `read`, keeping folders it skipped so switching back shows them at once. Returns whether anything changed.
fn merge_repos(repos: &mut HashMap<String, Repo>, read: Vec<(String, Option<Repo>)>) -> bool {
    let mut changed = false;
    for (cwd, repo) in read {
        changed |= match repo {
            Some(r) if repos.get(&cwd) != Some(&r) => {
                repos.insert(cwd, r);
                true
            }
            Some(_) => false,
            None => repos.remove(&cwd).is_some(),
        };
    }
    changed
}

#[cfg(test)]
mod tests {
    use super::{HashMap, Repo, merge_repos};

    fn repo(branch: &str) -> Repo {
        Repo { branch: branch.into(), ..Repo::default() }
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
}
