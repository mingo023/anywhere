use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, state};
use crate::explorer::{self, preview};
use crate::git_ui::diff;
use crate::status::{self, Card, Status};
use crate::util::{self, basename};
use agents::Summary;
use git::Repo;
use gpui_kit::*;
use std::collections::HashMap;
use std::path::PathBuf;
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
        project::worktree_of(cwd, &self.worktrees)
    }

    pub fn tree_of(&self, cwd: &str) -> Option<String> {
        project::tree_of(cwd, &self.worktrees, || self.projects())
    }

    pub fn cards(&self, project: &str) -> Vec<Card> {
        let projects = self.projects();
        let mut out: Vec<Card> = self
            .agents
            .list
            .iter()
            .filter_map(|a| Some((a, self.terminals.sessions.get(&a.terminal_id)?)))
            .filter(|(_, s)| self.project_of(&s.info.cwd, &projects).is_some_and(|p| p == project))
            .map(|(a, s)| status::card(a, &s.info.cwd))
            .collect();
        out.sort_by_key(|c| std::cmp::Reverse(c.created));
        out
    }

    /// The agent in `terminal`.
    pub fn summary(&self, terminal: &str) -> Option<&Summary> {
        self.agents.list.iter().find(|a| a.terminal_id == terminal)
    }

    pub fn cwd_of(&self, id: &str) -> Option<String> {
        self.agents.get(id).map(|a| a.cwd.clone()).or_else(|| self.terminals.sessions.get(id).map(|s| s.info.cwd.clone()))
    }

    /// The worktree on screen: the one picked, else the project's main one.
    pub fn cwd(&self) -> Option<String> {
        self.worktree.clone().or_else(|| self.tree_of(self.project.as_deref()?))
    }

    pub fn repo(&self) -> Option<&Repo> {
        self.repos.get(&self.cwd()?)
    }

    pub(crate) fn workspace(&mut self, tree: &str) -> &mut Workspace {
        let (mut mine, mut theirs) = (Vec::new(), Vec::new());
        for s in &self.terminals.sessions.items {
            match self.tree_of(&s.info.cwd) {
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
        self.terminals.sessions.items.iter().filter(|s| self.tree_of(&s.info.cwd).as_deref() == Some(tree)).map(|s| s.info.id.clone()).collect()
    }

    fn git_cwds(&self) -> Vec<String> {
        let sessions = self.project.as_deref().map(|p| self.cards(p).into_iter().map(|c| c.cwd).collect()).unwrap_or_default();
        project::git_cwds(self.project.as_deref(), sessions, &self.worktrees, self.cwd())
    }

    pub fn refresh_git(&mut self, cx: &mut Context<Self>) {
        let cwds = self.git_cwds();
        let projects = self.store.projects.clone();
        let files: Vec<_> = self.preview.panes.iter().filter_map(|(p, f)| {
            let f = f.file.clone()?;
            let changed = self.file_status(&f).is_some();
            Some((*p, f, changed))
        }).collect();
        let root = self.cwd();
        let diffs: Vec<_> = self.diff.panes.iter().filter_map(|(p, v)| Some((*p, v.working_file()?.to_string(), v.open.clone(), v.lines.clone()))).collect();
        let mut dirs: Vec<PathBuf> = self.explorer.tree.keys().cloned().collect();
        dirs.extend(self.explore_root().map(PathBuf::from));
        self.git_run += 1;
        let run = self.git_run;
        let task = cx.background_executor().spawn(async move {
            let repos: Vec<(String, Option<Repo>)> = cwds.into_iter().map(|c| {
                let r = git::read(&c);
                (c, r)
            }).collect();
            let diffs: Vec<_> = root.map(|cwd| diffs.into_iter().map(|(p, path, open, shown)| (p, diff::read_diff(&cwd, path, None, open, &shown))).collect()).unwrap_or_default();
            let initials = repos.first().map(|(c, _)| git::user_initials(c)).unwrap_or_default();
            let tree: HashMap<PathBuf, Vec<(bool, PathBuf)>> = dirs.into_iter().map(|d| {
                let listing = util::list_dir(&d);
                (d, listing)
            }).collect();
            let worktrees: HashMap<String, Vec<git::Worktree>> = projects.into_iter().map(|p| {
                let w = git::worktrees(&p);
                (p, w)
            }).collect();
            let files: Vec<_> = files.into_iter().map(|(p, f, changed)| (p, preview::load(&f, changed))).collect();
            (repos, diffs, initials, tree, worktrees, files)
        });
        cx.spawn(async move |this, cx| {
            let (repos, diffs, initials, tree, worktrees, files) = task.await;
            this.update(cx, |d, cx| {
                if run != d.git_run {
                    return;
                }
                d.git_done = run;
                let repos: HashMap<String, Repo> = repos.into_iter().filter_map(|(c, r)| Some((c, r?))).collect();
                let tree = explorer::merge_tree(tree, &d.explorer.tree, d.explore_root().as_deref().map(std::path::Path::new));
                let mut changed = repos != d.repos || tree != d.explorer.tree || initials != d.initials || worktrees != d.worktrees;
                (d.repos, d.explorer.tree, d.initials, d.worktrees) = (repos, tree, initials, worktrees);
                d.poll_prs(cx);
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

    pub fn repo_name(&self, path: &str) -> String {
        self.store.repos.get(path).map(|r| r.name.clone()).filter(|n| !n.is_empty()).unwrap_or_else(|| basename(path))
    }

    pub fn repo_color(&self, path: &str) -> u32 {
        let fallback = || PALETTE[self.projects().iter().position(|p| p == path).unwrap_or(0) % PALETTE.len()];
        self.store.repos.get(path).map(|r| r.color).filter(|c| *c != 0).unwrap_or_else(fallback)
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

    /// Closes the project's terminals and takes it off the sidebar; its folder is untouched.
    pub(crate) fn remove_project(&mut self, p: &str, cx: &mut Context<Self>) {
        for id in self.project_terminals(p) {
            self.close_pane(&id, cx);
        }
        self.store.remove(p);
        self.store.save();
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
        if self.project_terminals(&p).is_empty() {
            self.remove_project(&p, cx);
        } else {
            self.confirm = Some(Confirm::RemoveProject(p));
            self.overlay = Some(Overlay::Confirm);
        }
        cx.notify();
    }
}
