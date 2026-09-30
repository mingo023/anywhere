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

fn under(cwd: &str, project: &str) -> bool {
    cwd == project || cwd.strip_prefix(project).is_some_and(|rest| rest.starts_with('/'))
}

impl Desktop {
    pub fn projects(&self) -> Vec<String> {
        let mut out = self.store.projects.clone();
        for cwd in self.sessions.items.iter().map(|s| &s.info.cwd) {
            if self.project_of(cwd, &out).is_none() {
                out.push(cwd.clone());
            }
        }
        out
    }

    /// The project owning `cwd`: the one whose folder or worktree holds it most closely.
    pub fn project_of<'a>(&self, cwd: &str, projects: &'a [String]) -> Option<&'a String> {
        let reach = |p: &String| {
            let trees = self.worktrees.get(p).into_iter().flatten().map(|w| w.path.as_str());
            std::iter::once(p.as_str()).chain(trees).filter(|root| under(cwd, root)).map(str::len).max()
        };
        projects.iter().filter_map(|p| Some((reach(p)?, p))).max_by_key(|(n, _)| *n).map(|(_, p)| p)
    }

    pub fn worktree_of(&self, cwd: &str) -> Option<&git::Worktree> {
        self.worktrees.values().flatten().filter(|w| under(cwd, &w.path)).max_by_key(|w| w.path.len())
    }

    /// The worktree a folder belongs to: the git worktree holding it, else the project it is in.
    pub fn tree_of(&self, cwd: &str) -> Option<String> {
        self.worktree_of(cwd).map(|w| w.path.clone()).or_else(|| self.project_of(cwd, &self.projects()).cloned())
    }

    pub fn cards(&self, project: &str) -> Vec<Card> {
        let projects = self.projects();
        let mut out: Vec<Card> = self
            .agents
            .list
            .iter()
            .filter_map(|a| Some((a, self.sessions.get(&a.terminal_id)?)))
            .filter(|(_, s)| self.project_of(&s.info.cwd, &projects).is_some_and(|p| p == project))
            .map(|(a, s)| status::card(a, &s.info.cwd))
            .collect();
        out.sort_by_key(|c| std::cmp::Reverse(c.at));
        out
    }

    /// The live agent in `terminal`.
    pub fn summary(&self, terminal: &str) -> Option<&Summary> {
        self.agents.list.iter().find(|a| a.terminal_id == terminal && a.status != "closed")
    }

    pub fn cwd_of(&self, id: &str) -> Option<String> {
        self.agents.get(id).map(|a| a.cwd.clone()).or_else(|| self.sessions.get(id).map(|s| s.info.cwd.clone()))
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
        for s in &self.sessions.items {
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
        self.sessions.items.iter().filter(|s| self.project_of(&s.info.cwd, &projects).is_some_and(|o| o == p)).map(|s| s.info.id.clone()).collect()
    }

    pub(crate) fn tree_terminals(&self, tree: &str) -> Vec<String> {
        self.sessions.items.iter().filter(|s| self.tree_of(&s.info.cwd).as_deref() == Some(tree)).map(|s| s.info.id.clone()).collect()
    }

    fn git_cwds(&self) -> Vec<String> {
        let mut cwds: Vec<String> = self.project.iter().cloned().collect();
        if let Some(p) = &self.project {
            cwds.extend(self.cards(p).into_iter().map(|c| c.cwd));
            cwds.extend(self.worktrees.get(p).into_iter().flatten().map(|w| w.path.clone()));
        }
        cwds.extend(self.cwd());
        cwds.sort();
        cwds.dedup();
        cwds
    }

    pub fn refresh_git(&mut self, cx: &mut Context<Self>) {
        let cwds = self.git_cwds();
        let projects = self.store.projects.clone();
        let file = self.file.clone().map(|f| {
            let changed = self.file_status(&f).is_some();
            (f, changed)
        });
        let diff = self.cwd().zip(self.diff_file.clone());
        let shown = self.diff.clone();
        let open = self.diff_open.clone();
        let mut dirs: Vec<PathBuf> = self.tree.keys().cloned().collect();
        dirs.extend(self.explore_root().map(PathBuf::from));
        self.git_run += 1;
        let run = self.git_run;
        let task = cx.background_executor().spawn(async move {
            let repos: Vec<(String, Option<Repo>)> = cwds.into_iter().map(|c| {
                let r = git::read(&c);
                (c, r)
            }).collect();
            let diff = diff.map(|(cwd, path)| diff::read_diff(&cwd, path, open, &shown));
            let initials = repos.first().map(|(c, _)| git::user_initials(c)).unwrap_or_default();
            let tree: HashMap<PathBuf, Vec<(bool, PathBuf)>> = dirs.into_iter().map(|d| {
                let listing = util::list_dir(&d);
                (d, listing)
            }).collect();
            let worktrees: HashMap<String, Vec<git::Worktree>> = projects.into_iter().map(|p| {
                let w = git::worktrees(&p);
                (p, w)
            }).collect();
            let file = file.map(|(f, changed)| preview::load(&f, changed));
            (repos, diff, initials, tree, worktrees, file)
        });
        cx.spawn(async move |this, cx| {
            let (repos, diff, initials, tree, worktrees, file) = task.await;
            this.update(cx, |d, cx| {
                if run != d.git_run {
                    return;
                }
                d.git_done = run;
                let repos: HashMap<String, Repo> = repos.into_iter().filter_map(|(c, r)| Some((c, r?))).collect();
                let tree = explorer::merge_tree(tree, &d.tree, d.explore_root().as_deref().map(std::path::Path::new));
                let mut changed = repos != d.repos || tree != d.tree || initials != d.initials || worktrees != d.worktrees;
                (d.repos, d.tree, d.initials, d.worktrees) = (repos, tree, initials, worktrees);
                if let Some(file) = file {
                    changed |= d.apply_file(file);
                }
                if let Some(load) = diff {
                    changed |= d.apply_diff(load);
                }
                if changed {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
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

    /// Asks first when deleting would close terminals or lose uncommitted changes.
    pub(crate) fn ask_delete_worktree(&mut self, project: String, tree: String, cx: &mut Context<Self>) {
        let branch = self.worktrees.get(&project).into_iter().flatten().find(|w| w.path == tree).map(|w| w.branch.clone()).unwrap_or_default();
        let dir = tree.clone();
        let task = cx.background_executor().spawn(async move { git::read(&dir).map(|r| r.files.len()) });
        cx.spawn(async move |this, cx| {
            let dirty = task.await;
            this.update(cx, |d, cx| {
                if dirty == Some(0) && d.tree_terminals(&tree).is_empty() {
                    d.delete_worktree(project, tree, cx);
                } else {
                    d.confirm = Some(Confirm::DeleteWorktree { project, tree, branch, dirty: dirty.unwrap_or(0) });
                    d.overlay = Some(Overlay::Confirm);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Closes the worktree's terminals and deletes its folder; its branch stays.
    pub(crate) fn delete_worktree(&mut self, project: String, tree: String, cx: &mut Context<Self>) {
        let dir = tree.clone();
        let task = cx.background_executor().spawn(async move { git::remove_worktree(&project, &dir) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                match res {
                    Err(e) => d.error = Some(e),
                    Ok(()) => {
                        for id in d.tree_terminals(&tree) {
                            d.close_pane(&id, cx);
                        }
                        d.workspaces.remove(&tree);
                        if d.worktree.as_ref() == Some(&tree) {
                            (d.worktree, d.session) = (None, None);
                        }
                    }
                }
                d.refresh_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
