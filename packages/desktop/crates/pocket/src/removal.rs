use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use crate::fs::fs_for;
use crate::util::basename;
use agents::{Agents, Summary};
use store::Subtree;
use gpui_kit::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Worktrees being deleted: their rows leave at once, a toast stays until they're gone, and a second delete of one is ignored.
#[derive(Default)]
pub(crate) struct Removals {
    running: HashSet<String>,
}

impl Removals {
    pub(crate) fn running(&self, tree: &str) -> bool {
        self.running.contains(tree)
    }

    /// Whether `tree` wasn't already being deleted.
    pub(crate) fn start(&mut self, tree: &str) -> bool {
        self.running.insert(tree.to_string())
    }

    pub(crate) fn hide(&self, mut trees: Vec<git::Worktree>) -> Vec<git::Worktree> {
        trees.retain(|w| !self.running(&w.path));
        trees
    }

    pub(crate) fn label(&self) -> Option<String> {
        match self.running.len() {
            0 => None,
            1 => self.running.iter().next().map(|t| format!("Deleting {}…", basename(t))),
            n => Some(format!("Deleting {n} worktrees…")),
        }
    }
}

/// A worktree to delete, and what goes with it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Removal {
    pub(crate) project: String,
    pub(crate) tree: String,
    /// `None` when the tree is detached or on a branch the project builds on, which always stays.
    pub(crate) branch: Option<String>,
    pub(crate) delete_branch: bool,
    pub(crate) teardown: bool,
}

impl Removal {
    /// Commits only this deletion loses: a detached tree's, or its branch's when that goes too.
    pub(crate) fn lost(&self) -> usize {
        match (&self.branch, self.delete_branch) {
            (None, _) => git::lost_commits(&hosts::runner_for(&self.tree), &self.tree, None),
            (Some(b), true) => git::lost_commits(&hosts::runner_for(&self.project), &self.project, Some(b)),
            (Some(_), false) => 0,
        }
    }
}

#[derive(Debug, PartialEq)]
enum Failure {
    /// Nothing was deleted; holds the end of the script's output.
    Teardown(String),
    Remove(String),
}

/// A deleted worktree.
#[derive(Debug, PartialEq)]
struct Removed {
    /// The set-aside folder, still holding the tree's files.
    aside: Option<PathBuf>,
    /// A branch that stayed, and why.
    kept: Option<(String, String)>,
}

/// Whether "Delete worktree and branch" may take `branch`: not a detached tree, nor a trunk branch.
pub(crate) fn branch_deletable(branch: &str) -> bool {
    !["", "detached", "main", "master"].contains(&branch)
}

/// Moves `tree` into a folder of its own in the temp folder, an instant rename where deleting a `target/` takes seconds.
/// It keeps its name there, so it reaches the Trash under it. `None` when it can't, e.g. across volumes: git then deletes it in place.
fn set_aside(tree: &str) -> Option<PathBuf> {
    fs_for(tree).local().ok()?;
    let name = Path::new(tree).file_name()?;
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_nanos();
    let holder = std::env::temp_dir().join(format!("pocket-deleted-{nanos}"));
    std::fs::create_dir(&holder).ok()?;
    let aside = holder.join(name);
    match std::fs::rename(tree, &aside) {
        Ok(()) => Some(aside),
        Err(_) => {
            std::fs::remove_dir(&holder).ok();
            None
        }
    }
}

/// Purges a set-aside tree, or moves it to the Trash, then drops the folder that held it.
pub(crate) fn dispose(aside: &Path, trash: bool) {
    if !(trash && crate::util::trash(aside).is_some()) {
        std::fs::remove_dir_all(aside).ok();
    }
    if let Some(holder) = aside.parent() {
        std::fs::remove_dir(holder).ok();
    }
}

/// Where a repo names its own scripts, committed with its code; pocketd reads the setup from the same file.
const CONFIG_FILE: &str = ".anywhere/config.json";

/// The script a repo can keep instead of naming its teardown in `CONFIG_FILE`, like pocketd's `.anywhere/setup.sh`.
/// It may be gitignored, so only the project checkout has it.
const TEARDOWN_SCRIPT: &str = ".anywhere/teardown.sh";

/// The teardown `r` runs: the project settings' `own`, else the one the repo names in `CONFIG_FILE`, else its `TEARDOWN_SCRIPT`;
/// each the tree's before the project checkout's.
fn teardown_for(r: &Removal, own: String) -> String {
    if !own.is_empty() {
        return own;
    }
    let dirs = [&r.tree, &r.project];
    dirs.into_iter().find_map(|dir| repo_teardown(dir)).or_else(|| dirs.into_iter().find_map(|dir| teardown_script(dir))).unwrap_or_default()
}

/// `dir`'s `TEARDOWN_SCRIPT` run with bash; `None` when it has none, or is on a Windows host, whose scripts run in PowerShell.
fn teardown_script(dir: &str) -> Option<String> {
    let path = hosts::split(dir).1;
    if hosts::on_windows(path) {
        return None;
    }
    let stat = hosts::runner_for(dir).stat(&format!("{dir}/{TEARDOWN_SCRIPT}")).ok()??;
    (!stat.dir).then(|| format!("bash '{}'", format!("{path}/{TEARDOWN_SCRIPT}").replace('\'', r"'\''")))
}

/// What a teardown is told, as pocketd tells a session's terminal: the project checkout's path, and the tree's path and name, on their machine.
fn script_env(r: &Removal) -> Vec<(String, String)> {
    let tree = hosts::split(&r.tree).1;
    [("ANYWHERE_ROOT_PATH", hosts::split(&r.project).1.to_string()), ("ANYWHERE_WORKSPACE_PATH", tree.to_string()), ("ANYWHERE_WORKSPACE_NAME", basename(tree))]
        .map(|(k, v)| (k.to_string(), v))
        .to_vec()
}

/// The teardown in `dir`'s `CONFIG_FILE`, its commands a line each; `None` when it names none or doesn't parse.
fn repo_teardown(dir: &str) -> Option<String> {
    let raw = hosts::runner_for(dir).read(&format!("{dir}/{CONFIG_FILE}"), 64 * 1024).ok()?;
    let config: serde_json::Value = serde_json::from_slice(&raw).ok()?;
    let commands = config.get("teardown")?.as_array()?.iter().map(|c| c.as_str()).collect::<Option<Vec<_>>>()?;
    (!commands.is_empty()).then(|| commands.join("\n"))
}

/// Runs a project's `script` in `tree` with `env` on the machine holding it, never here for a host's; the end of its output when it fails.
fn run_script(script: &str, tree: &str, env: &[(String, String)], limit: Duration) -> Result<(), String> {
    match hosts::runner_for(tree) {
        git::Runner::Local => daemon::run_script(script, tree, env, limit).map(drop),
        host => {
            let argv = hosts::script_argv(tree, script, env);
            let out = host.exec(tree, &argv.iter().map(String::as_str).collect::<Vec<_>>(), None, limit).map_err(|e| e.to_string())?;
            if out.code == 0 {
                return Ok(());
            }
            let text = String::from_utf8_lossy(&[out.stdout, out.stderr].concat()).into_owned();
            let lines: Vec<&str> = text.trim().lines().collect();
            Err(lines[lines.len().saturating_sub(20)..].join("\n"))
        }
    }
}

/// Teardown goes first: scripts like `docker compose down` need the folder.
fn run(r: &Removal, teardown: &str, limit: Duration) -> Result<Removed, Failure> {
    if r.teardown && !teardown.is_empty() && hosts::runner_for(&r.tree).stat(&r.tree).is_ok_and(|s| s.is_some_and(|s| s.dir)) {
        run_script(teardown, &r.tree, &script_env(r), limit).map_err(Failure::Teardown)?;
    }
    let runner = hosts::runner_for(&r.project);
    let branch = doomed_branch(r, &runner);
    let aside = set_aside(&r.tree);
    if let Err(e) = git::remove_worktree(&runner, &r.project, &r.tree) {
        if let Some(aside) = &aside {
            std::fs::rename(aside, &r.tree).ok();
        }
        return Err(Failure::Remove(e));
    }
    let kept = branch.and_then(|b| git::delete_branch(&runner, &r.project, &b).err().map(|e| (b, e)));
    Ok(Removed { aside, kept })
}

/// The branch going with `r`'s tree: the confirmed one, or, once a rename has moved it (pocketd names a new chat's branch), the tree's own.
/// The tree's branch goes only when its reflog shows the rename, so a branch the tree merely switched to is never taken.
fn doomed_branch(r: &Removal, runner: &git::Runner) -> Option<String> {
    let confirmed = r.branch.clone().filter(|_| r.delete_branch)?;
    if git::branches(runner, &r.project).contains(&confirmed) {
        return Some(confirmed);
    }
    let now = git::worktrees(runner, &r.project).into_iter().find(|w| w.path == r.tree).map(|w| w.branch);
    now.filter(|b| branch_deletable(b) && git::renamed_from(runner, &r.project, b, &confirmed)).or(Some(confirmed))
}

/// The deletions that go with `r`'s: each worktree Anywhere made that holds only sessions `r.tree`'s sessions delegated to, and theirs in turn.
/// Their branches go only if `r`'s does. `place` is the tree a session's terminal sits in.
fn subagent_removals(r: &Removal, agents: &Agents, place: impl Fn(&Summary) -> Option<String>, worktrees: &project::Worktrees, subtrees: &BTreeMap<String, String>) -> Vec<Removal> {
    let tree = r.tree.as_str();
    let roots: Vec<String> = agents.list.iter().filter(|a| place(a).as_deref() == Some(tree)).map(|a| a.id.clone()).collect();
    let subagents = agents.subagents_of(&roots);
    let theirs = |t: &str| agents.list.iter().filter(|a| place(a).as_deref() == Some(t)).all(|a| subagents.iter().any(|s| s.id == a.id));
    let mut trees: Vec<String> = std::iter::once(tree.to_string()).chain(subagents.iter().filter_map(|a| place(a))).collect();
    let mut i = 0;
    while let Some(t) = trees.get(i).cloned() {
        trees.extend(subtrees.iter().filter(|(c, p)| **p == t && !trees.contains(c)).map(|(c, _)| c.clone()).collect::<Vec<_>>());
        i += 1;
    }
    let mut out: Vec<Removal> = Vec::new();
    for t in trees.into_iter().skip(1) {
        if t == tree || out.iter().any(|o| o.tree == t) || !theirs(&t) {
            continue;
        }
        if let Some((project, w)) = worktrees.iter().find_map(|(p, ws)| Some((p, ws.iter().find(|w| w.path == t && w.made && !w.main)?))) {
            let branch = Some(w.branch.clone()).filter(|b| branch_deletable(b));
            out.push(Removal { project: project.clone(), tree: t, delete_branch: r.delete_branch && branch.is_some(), branch, teardown: true });
        }
    }
    out
}

/// Adds the links not known yet, keeping the parent first seen; returns whether any was added.
fn remember(subtrees: &mut BTreeMap<String, Subtree>, links: Vec<(String, String)>, seen: u64) -> bool {
    let before = subtrees.len();
    for (child, parent) in links {
        subtrees.entry(child).or_insert(Subtree { parent, seen });
    }
    subtrees.len() != before
}

/// Whether a worktree made at `created` can be the one a subagent was seen in at `seen`.
fn there_when_seen(created: Option<SystemTime>, seen: u64) -> bool {
    let created = created.and_then(|c| c.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64);
    created.is_none_or(|c| c <= seen)
}

/// `(child, parent)` for each live subagent placed apart from its parent.
fn subtree_links(agents: &Agents, place: impl Fn(&Summary) -> Option<String>) -> Vec<(String, String)> {
    let links = agents.list.iter().filter(|a| a.relationship == "subagent");
    links.filter_map(|a| Some((place(a)?, place(agents.get(&a.parent)?)?))).filter(|(c, p)| c != p).collect()
}

/// The commits deleting `jobs` together loses, a tree first and then its subagents': `(lost, lost_anyway)`.
/// `lost` counts the branches as if they all go, as one set, so it holds whichever way the user ticks the box.
/// `lost_anyway` is what the subagents' detached HEADs alone hold, which goes even if branches stay.
fn lost_together(jobs: &[Removal]) -> (usize, usize) {
    let Some((tree, subagents)) = jobs.split_first() else { return (0, 0) };
    let mut doomed: HashMap<&str, Vec<&str>> = HashMap::new();
    // A detached tree has no branch to delete, so its subagents' branches stay with it.
    for j in jobs.iter().filter(|_| tree.branch.is_some()) {
        doomed.entry(&j.project).or_default().extend(j.branch.as_deref());
    }
    let branches: usize = doomed.iter().map(|(project, bs)| git::lost_branch_commits(&hosts::runner_for(project), project, bs)).sum();
    let detached = if tree.branch.is_none() { tree.lost() } else { 0 };
    (detached + branches, subagents.iter().filter(|j| j.branch.is_none()).map(Removal::lost).sum())
}

impl Desktop {
    pub(crate) fn subagent_removals(&self, r: &Removal) -> Vec<Removal> {
        let created = |t: &str| std::fs::metadata(t).and_then(|m| m.created()).ok();
        let subtrees: BTreeMap<String, String> = self.store.subtrees.iter().filter_map(|(c, s)| {
            let tree = self.tree_of(c).filter(|t| there_when_seen(created(t), s.seen))?;
            Some((tree, self.tree_of(&s.parent)?))
        }).collect();
        subagent_removals(r, &self.agents, |a| self.session_place(a), &self.worktrees, &subtrees)
    }

    fn session_place(&self, a: &Summary) -> Option<String> {
        self.terminals.sessions.get(&a.terminal_id).and_then(|s| self.place_of(&s.info))
    }

    /// Records each live subagent's folder: once the sessions close, nothing else says whose it was. Folders, not
    /// worktrees, as a subagent's worktree may be newer than the last git refresh.
    pub(crate) fn remember_subtrees(&mut self, cx: &mut Context<Self>) {
        let links = subtree_links(&self.agents, |a| Some(a.cwd.clone()).filter(|c| !c.is_empty()));
        let now = SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_millis() as u64);
        if remember(&mut self.store.subtrees, links, now) {
            self.save_soon(cx);
        }
    }

    /// Asks first, offering to delete the tree's branch too and warning about the work that would go.
    pub(crate) fn ask_delete_worktree(&mut self, project: String, tree: String, cx: &mut Context<Self>) {
        if self.removals.running(&tree) {
            return;
        }
        let branch = self.worktrees.get(&project).into_iter().flatten().find(|w| w.path == tree).map(|w| w.branch.clone()).filter(|b| branch_deletable(b));
        let removal = Removal { project, tree, delete_branch: branch.is_some() && self.store.worktree.delete_branch, branch, teardown: true };
        let subtrees = self.subagent_removals(&removal);
        let jobs: Vec<Removal> = std::iter::once(removal.clone()).chain(subtrees.clone()).collect();
        let task = cx.background_executor().spawn(async move {
            let dirty: usize = jobs.iter().map(|j| git::dirty_files(&hosts::runner_for(&j.tree), &j.tree).unwrap_or(0)).sum();
            let (lost, anyway) = lost_together(&jobs);
            (dirty, lost, anyway)
        });
        cx.spawn(async move |this, cx| {
            let (dirty, lost, lost_anyway) = task.await;
            this.update(cx, |d, cx| {
                d.confirm = Some(Confirm::DeleteWorktree { removal, dirty, lost, lost_anyway, subtrees });
                d.overlay = Some(Overlay::Confirm);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Deletes `removal`'s tree, then the `confirmed` subagent deletions, the ones the user was shown and whose work was checked,
    /// that are still its subagents'. Their branches go only if `removal`'s does.
    pub(crate) fn delete_worktree(&mut self, removal: Removal, confirmed: &[Removal], cx: &mut Context<Self>) {
        if !self.removals.start(&removal.tree) {
            return;
        }
        if self.worktree.as_deref() == Some(&removal.tree) {
            self.select_tree(removal.project.clone(), None, cx);
        }
        // Read now: once its sessions close, pocketd closes their subagents and the lineage is gone.
        let still = self.subagent_removals(&removal);
        let subagents: Vec<Removal> = confirmed
            .iter()
            .filter(|c| still.iter().any(|s| s.tree == c.tree))
            .map(|c| Removal { delete_branch: removal.delete_branch && c.branch.is_some(), ..c.clone() })
            .collect();
        let teardown = self.store.repos.get(&removal.project).map(|r| r.teardown.clone()).unwrap_or_default();
        let (limit, trash) = (Duration::from_secs(self.store.worktree.teardown_secs.into()), self.store.worktree.trash);
        let job = removal.clone();
        let task = cx.background_executor().spawn(async move { run(&job, &teardown_for(&job, teardown), limit) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                d.removals.running.remove(&removal.tree);
                match res {
                    Err(Failure::Teardown(tail)) => {
                        d.confirm = Some(Confirm::TeardownFailed { removal, tail, subtrees: subagents });
                        d.overlay = Some(Overlay::Confirm);
                    }
                    Err(Failure::Remove(e)) => d.error = Some(e),
                    Ok(Removed { aside, kept }) => {
                        if let Some(aside) = aside {
                            cx.background_executor().spawn(async move { dispose(&aside, trash) }).detach();
                        }
                        if let Some((branch, e)) = kept {
                            d.error = Some(format!("Deleted the worktree; kept branch {branch}: {e}"));
                        }
                        d.forget_tree(&removal.tree, cx);
                        // `subagents` already reaches every depth, so none of them takes its own.
                        for r in subagents {
                            d.delete_worktree(r, &[], cx);
                        }
                    }
                }
                d.refresh_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn forget_tree(&mut self, tree: &str, cx: &mut Context<Self>) {
        // Before the tree leaves `worktrees`, while its folders still resolve to it.
        let gone: Vec<String> = self.store.subtrees.iter().filter(|(c, s)| [*c, &s.parent].iter().any(|f| self.tree_of(f).as_deref() == Some(tree))).map(|(c, _)| c.clone()).collect();
        for c in gone {
            self.store.subtrees.remove(&c);
        }
        for id in self.tree_terminals(tree) {
            self.close_pane(&id, cx);
        }
        // Its row would show again until the git refresh lands.
        for trees in self.worktrees.values_mut() {
            trees.retain(|w| w.path != tree);
        }
        self.workspaces.remove(tree);
        self.repos.remove(tree);
        self.prs.forget(tree);
        self.store.layouts.remove(tree);
        self.store.scopes.remove(tree);
        self.store.untrack(tree);
        self.store.sections.unpin_tree(tree);
        self.save_soon(cx);
        if self.worktree.as_deref() == Some(tree) {
            self.worktree = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CONFIG_FILE, Failure, Removal, Removals, Removed, TEARDOWN_SCRIPT, branch_deletable, dispose, lost_together, run, run_script, remember, subagent_removals, subtree_links, teardown_for, there_when_seen};
    use std::time::Duration;
    use std::path::{Path, PathBuf};
    use std::process::Command;

    const LIMIT: Duration = Duration::from_secs(120);

    fn sh(dir: &Path, args: &[&str]) {
        let out = Command::new("git").arg("-C").arg(dir).args(["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false"]).args(args).output().unwrap();
        assert!(out.status.success(), "git {args:?}: {}", String::from_utf8_lossy(&out.stderr));
    }

    /// A repository with a worktree `feat` on branch `feat`, and the removal of both.
    fn scratch(tag: &str) -> (PathBuf, Removal) {
        let dir = std::env::temp_dir().canonicalize().unwrap().join(format!("pocket-removal-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let (repo, tree) = (dir.join("repo"), dir.join("feat"));
        std::fs::create_dir_all(&repo).unwrap();
        sh(&repo, &["init", "-q"]);
        sh(&repo, &["commit", "-q", "--allow-empty", "-m", "init"]);
        sh(&repo, &["worktree", "add", "-q", "-b", "feat", tree.to_str().unwrap()]);
        let removal = Removal { project: repo.to_string_lossy().into_owned(), tree: tree.to_string_lossy().into_owned(), branch: Some("feat".into()), delete_branch: true, teardown: true };
        (dir, removal)
    }

    fn tree(path: &str, main: bool) -> git::Worktree {
        git::Worktree { path: path.into(), branch: String::new(), main, made: false }
    }

    #[test]
    fn a_worktree_takes_the_ones_anywhere_made_for_its_sessions_subagents_all_the_way_down() {
        let agent = |id: &str, parent: &str| agents::Summary { id: id.into(), terminal_id: id.into(), parent: parent.into(), relationship: if parent.is_empty() { "" } else { "subagent" }.into(), ..Default::default() };
        let list = vec![agent("p", ""), agent("c", "p"), agent("g", "c"), agent("in-main", "p"), agent("shared", "p"), agent("user", ""), agent("unmade", "p"), agent("beside", "p")];
        let at: std::collections::HashMap<&str, &str> =
            [("p", "/wt"), ("c", "/wt-c"), ("g", "/wt-g"), ("in-main", "/app"), ("shared", "/wt-s"), ("user", "/wt-s"), ("unmade", "/wt-u"), ("beside", "/wt")].into();
        let place = |a: &agents::Summary| at.get(a.terminal_id.as_str()).map(|t| t.to_string());
        let made = |path: &str, branch: &str, made: bool| git::Worktree { path: path.into(), branch: branch.into(), main: false, made };
        let main = git::Worktree { path: "/app".into(), branch: "main".into(), main: true, made: false };
        let trees = vec![main, made("/wt", "feat", true), made("/wt-c", "tests", true), made("/wt-g", "detached", true), made("/wt-s", "s", true), made("/wt-u", "u", false)];
        let worktrees = [("/app".to_string(), trees)].into();
        let agents = agents::Agents { list, ..Default::default() };
        let parent = |delete_branch| Removal { project: "/app".into(), tree: "/wt".into(), branch: Some("feat".into()), delete_branch, teardown: false };
        let child = |tree: &str, branch: Option<&str>, delete_branch| Removal { project: "/app".into(), tree: tree.into(), branch: branch.map(String::from), delete_branch, teardown: true };
        assert_eq!(subagent_removals(&parent(true), &agents, place, &worktrees, &Default::default()), [child("/wt-c", Some("tests"), true), child("/wt-g", None, false)]);
        assert_eq!(subagent_removals(&parent(false), &agents, place, &worktrees, &Default::default()), [child("/wt-c", Some("tests"), false), child("/wt-g", None, false)]);
    }

    #[test]
    fn a_worktree_takes_its_subagents_worktrees_after_their_sessions_closed() {
        let user = agents::Summary { id: "user".into(), terminal_id: "user".into(), ..Default::default() };
        let agents = agents::Agents { list: vec![user], ..Default::default() };
        let place = |a: &agents::Summary| (a.id == "user").then(|| "/wt-busy".to_string());
        let made = |path: &str| git::Worktree { path: path.into(), branch: format!("b{path}"), main: false, made: true };
        let worktrees = [("/app".to_string(), vec![made("/wt"), made("/wt-c"), made("/wt-g"), made("/wt-busy"), made("/wt-other")])].into();
        let subtrees = [("/wt-c", "/wt"), ("/wt-g", "/wt-c"), ("/wt-busy", "/wt"), ("/wt-other", "/elsewhere")].map(|(c, p)| (c.to_string(), p.to_string())).into();
        let parent = Removal { project: "/app".into(), tree: "/wt".into(), branch: Some("feat".into()), delete_branch: true, teardown: false };
        let trees: Vec<String> = subagent_removals(&parent, &agents, place, &worktrees, &subtrees).into_iter().map(|r| r.tree).collect();
        assert_eq!(trees, ["/wt-c", "/wt-g"]);
    }

    #[test]
    fn a_subagent_is_remembered_by_the_worktree_it_works_in_apart_from_its_parent() {
        let agent = |id: &str, parent: &str, relationship: &str| agents::Summary { id: id.into(), terminal_id: id.into(), parent: parent.into(), relationship: relationship.into(), ..Default::default() };
        let list = vec![agent("p", "", ""), agent("c", "p", "subagent"), agent("shared", "p", "subagent"), agent("fork", "p", "fork"), agent("orphan", "gone", "subagent")];
        let at: std::collections::HashMap<&str, &str> = [("p", "/wt"), ("c", "/wt-c"), ("shared", "/wt"), ("fork", "/wt-f"), ("orphan", "/wt-o")].into();
        let place = |a: &agents::Summary| at.get(a.id.as_str()).map(|t| t.to_string());
        let links = subtree_links(&agents::Agents { list, ..Default::default() }, place);
        assert_eq!(links, [("/wt-c".to_string(), "/wt".to_string())]);
    }

    #[test]
    fn a_subagents_worktree_keeps_the_parent_it_was_first_seen_with() {
        let link = |c: &str, p: &str| (c.to_string(), p.to_string());
        let seen = |p: &str, at: u64| store::Subtree { parent: p.into(), seen: at };
        let mut subtrees = std::collections::BTreeMap::new();
        assert!(remember(&mut subtrees, vec![link("/wt-c", "/wt")], 1));
        assert!(!remember(&mut subtrees, vec![link("/wt-c", "/other")], 2));
        assert!(remember(&mut subtrees, vec![link("/wt-g", "/wt-c")], 3));
        assert_eq!(subtrees, [("/wt-c".to_string(), seen("/wt", 1)), ("/wt-g".to_string(), seen("/wt-c", 3))].into());
    }

    #[test]
    fn a_worktree_made_after_its_subagent_was_seen_there_is_someone_elses() {
        let at = |ms: u64| Some(std::time::UNIX_EPOCH + Duration::from_millis(ms));
        assert!(there_when_seen(at(10), 10));
        assert!(!there_when_seen(at(11), 10));
        assert!(there_when_seen(None, 10));
    }

    #[test]
    fn a_worktree_being_deleted_leaves_the_sidebar() {
        let removals = Removals { running: ["/wt".to_string()].into() };
        let shown = removals.hide(vec![tree("/p", true), tree("/wt", false), tree("/wt2", false)]);
        assert_eq!(shown, [tree("/p", true), tree("/wt2", false)]);
    }

    #[test]
    fn the_toast_names_the_worktree_being_deleted_or_counts_them() {
        let labels = [&[][..], &["/src/feat"], &["/src/feat", "/src/fix"]].map(|trees| Removals { running: trees.iter().map(|t| t.to_string()).collect() }.label());
        assert_eq!(labels, [None, Some("Deleting feat…".into()), Some("Deleting 2 worktrees…".into())]);
    }

    #[test]
    fn only_a_branch_that_is_not_a_trunk_can_go_with_its_worktree() {
        assert!(branch_deletable("feat"));
        for kept in ["main", "master", "detached", ""] {
            assert!(!branch_deletable(kept), "{kept}");
        }
    }

    #[test]
    fn lost_commits_count_only_when_the_branch_goes_or_the_tree_is_detached() {
        let (dir, r) = scratch("lost");
        sh(Path::new(&r.tree), &["commit", "-q", "--allow-empty", "-m", "b"]);
        assert_eq!(r.lost(), 1);
        assert_eq!(Removal { delete_branch: false, ..r.clone() }.lost(), 0);
        sh(Path::new(&r.tree), &["checkout", "-q", "--detach"]);
        sh(Path::new(&r.tree), &["commit", "-q", "--allow-empty", "-m", "c"]);
        assert_eq!(Removal { branch: None, delete_branch: false, ..r.clone() }.lost(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_commit_a_tree_and_its_subagent_share_is_lost_once_both_branches_go() {
        let (dir, r) = scratch("lost-together");
        sh(Path::new(&r.tree), &["commit", "-q", "--allow-empty", "-m", "b"]);
        sh(Path::new(&r.project), &["branch", "child", "feat"]);
        let child = Removal { tree: "/elsewhere".into(), branch: Some("child".into()), ..r.clone() };
        assert_eq!(lost_together(&[r.clone(), child.clone()]), (1, 0));
        let keep = |r: &Removal| Removal { delete_branch: false, ..r.clone() };
        assert_eq!(lost_together(&[keep(&r), keep(&child)]), (1, 0), "counted before the user ticks the box; the sheet hides it while branches stay");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_detached_tree_keeps_its_subagents_branches_so_their_commits_are_not_lost() {
        let (dir, r) = scratch("lost-detached");
        sh(Path::new(&r.project), &["branch", "child", "feat"]);
        let other = dir.join("child");
        sh(Path::new(&r.project), &["worktree", "add", "-q", other.to_str().unwrap(), "child"]);
        sh(&other, &["commit", "-q", "--allow-empty", "-m", "c"]);
        let tree = Removal { branch: None, delete_branch: false, ..r.clone() };
        let child = Removal { tree: other.to_string_lossy().into_owned(), branch: Some("child".into()), delete_branch: false, ..r.clone() };
        assert_eq!(lost_together(&[tree, child]), (0, 0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn deletes_the_worktree_then_its_branch() {
        let (dir, r) = scratch("both");
        let removed = run(&r, "", LIMIT).unwrap();
        dispose(&removed.aside.unwrap(), false);
        assert!(!Path::new(&r.tree).exists());
        assert!(!git::branches(&git::Runner::Local, &r.project).contains(&"feat".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_branch_renamed_after_the_sidebar_read_it_still_goes_with_its_worktree() {
        let (dir, r) = scratch("renamed");
        sh(Path::new(&r.tree), &["branch", "-m", "renamed"]);
        let removed = run(&r, "", LIMIT).unwrap();
        assert_eq!(removed.kept, None);
        assert!(!git::branches(&git::Runner::Local, &r.project).contains(&"renamed".to_string()));
        dispose(&removed.aside.unwrap(), false);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_renamed_branch_goes_even_when_its_folder_is_already_gone() {
        let (dir, r) = scratch("renamed-gone");
        sh(Path::new(&r.tree), &["branch", "-m", "renamed"]);
        std::fs::remove_dir_all(&r.tree).unwrap();
        assert_eq!(run(&r, "", LIMIT).unwrap().kept, None);
        assert!(!git::branches(&git::Runner::Local, &r.project).contains(&"renamed".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_branch_the_tree_switched_to_stays_when_the_confirmed_one_is_already_gone() {
        let (dir, r) = scratch("switched-gone");
        sh(Path::new(&r.tree), &["checkout", "-q", "-b", "other"]);
        sh(&dir.join("repo"), &["branch", "-D", "feat"]);
        let removed = run(&r, "", LIMIT).unwrap();
        assert_eq!(removed.kept.map(|(b, _)| b).as_deref(), Some("feat"));
        assert!(git::branches(&git::Runner::Local, &r.project).contains(&"other".to_string()));
        dispose(&removed.aside.unwrap(), false);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_tree_switched_to_another_branch_takes_only_the_confirmed_one() {
        let (dir, r) = scratch("switched");
        sh(Path::new(&r.tree), &["checkout", "-q", "-b", "other"]);
        let removed = run(&r, "", LIMIT).unwrap();
        let left = git::branches(&git::Runner::Local, &r.project);
        assert!(!left.contains(&"feat".to_string()) && left.contains(&"other".to_string()), "{left:?}");
        dispose(&removed.aside.unwrap(), false);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_folder_is_set_aside_so_its_files_are_purged_after_the_worktree_is_gone() {
        let (dir, r) = scratch("aside");
        std::fs::write(Path::new(&r.tree).join("big"), "x").unwrap();
        let aside = run(&r, "", LIMIT).unwrap().aside.unwrap();
        assert!(!Path::new(&r.tree).exists());
        assert_eq!(git::worktrees(&git::Runner::Local, &r.project).len(), 1);
        assert!(aside.join("big").exists());
        assert!(aside.ends_with("feat"));
        let holder = aside.parent().unwrap().to_path_buf();
        dispose(&aside, false);
        assert!(!holder.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_removal_git_refuses_leaves_the_folder_in_place() {
        let (dir, r) = scratch("refused");
        std::fs::write(Path::new(&r.tree).join("work"), "x").unwrap();
        let elsewhere = dir.join("elsewhere").to_string_lossy().into_owned();
        assert!(matches!(run(&Removal { project: elsewhere, ..r.clone() }, "", LIMIT), Err(Failure::Remove(_))));
        assert!(Path::new(&r.tree).join("work").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_branch_that_cannot_be_deleted_leaves_the_worktree_deleted() {
        let (dir, r) = scratch("kept");
        sh(&dir.join("repo"), &["worktree", "add", "-q", "--force", dir.join("twin").to_str().unwrap(), "feat"]);
        let removed = run(&r, "", LIMIT).unwrap();
        assert_eq!(removed.kept.map(|(b, _)| b).as_deref(), Some("feat"));
        dispose(&removed.aside.unwrap(), false);
        assert!(!Path::new(&r.tree).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_worktree_on_a_host_is_never_deleted_from_the_same_path_here() {
        let (dir, r) = scratch("host");
        let on_host = Removal { project: hosts::place("devbox", &r.project), tree: hosts::place("devbox", &r.tree), ..r.clone() };
        assert!(matches!(run(&on_host, "exit 1", LIMIT), Err(Failure::Remove(e)) if e.contains("not connected")));
        assert!(Path::new(&r.tree).exists());
        assert!(git::branches(&git::Runner::Local, &r.project).contains(&"feat".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_hosts_teardown_never_runs_on_this_mac() {
        let (dir, r) = scratch("script");
        let marker = dir.join("ran");
        let script = format!("touch {}", marker.display());
        assert!(run_script(&script, &hosts::place("devbox", &r.tree), &[], LIMIT).is_err());
        assert!(!marker.exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_teardown_deletes_nothing() {
        let (dir, r) = scratch("teardown");
        assert!(matches!(run(&r, "echo nope; exit 3", LIMIT), Err(Failure::Teardown(tail)) if tail.ends_with("nope")));
        assert!(Path::new(&r.tree).exists());
        assert!(git::branches(&git::Runner::Local, &r.project).contains(&"feat".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn write_config(dir: &str, raw: &str) {
        let path = Path::new(dir).join(CONFIG_FILE);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, raw).unwrap();
    }

    #[test]
    fn the_repo_names_the_teardown_when_the_project_settings_dont() {
        let (dir, r) = scratch("repo-teardown");
        write_config(&r.tree, r#"{"setup": ["make"], "teardown": ["echo a > ../ran", "echo b >> ../ran"]}"#);
        let teardown = teardown_for(&r, String::new());
        assert_eq!(teardown, "echo a > ../ran\necho b >> ../ran");
        run(&Removal { delete_branch: false, ..r.clone() }, &teardown, LIMIT).unwrap();
        assert_eq!(std::fs::read_to_string(dir.join("ran")).unwrap(), "a\nb\n");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_project_settings_teardown_wins_over_the_repos() {
        let (dir, r) = scratch("own-teardown");
        write_config(&r.tree, r#"{"teardown": ["make clean"]}"#);
        assert_eq!(teardown_for(&r, "docker compose down".into()), "docker compose down");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_project_checkouts_config_stands_in_when_the_trees_names_no_teardown() {
        for (name, in_tree) in [("missing", None), ("no teardown", Some(r#"{"setup": ["x"]}"#)), ("broken", Some(r#"{"teardown": "not a list"}"#))] {
            let (dir, r) = scratch("project-teardown");
            write_config(&r.project, r#"{"teardown": ["make clean"]}"#);
            if let Some(raw) = in_tree {
                write_config(&r.tree, raw);
            }
            assert_eq!(teardown_for(&r, String::new()), "make clean", "{name}");
            std::fs::remove_dir_all(&dir).unwrap();
        }
    }

    fn write_teardown_script(dir: &str, body: &str) -> String {
        let path = Path::new(dir).join(TEARDOWN_SCRIPT);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, body).unwrap();
        path.to_string_lossy().into_owned()
    }

    #[test]
    fn a_repo_without_config_runs_its_teardown_script_the_trees_first() {
        let (dir, r) = scratch("teardown-script");
        let in_project = write_teardown_script(&r.project, "true");
        assert_eq!(teardown_for(&r, String::new()), format!("bash '{in_project}'"));
        let in_tree = write_teardown_script(&r.tree, "true");
        assert_eq!(teardown_for(&r, String::new()), format!("bash '{in_tree}'"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_configs_teardown_wins_over_the_teardown_script() {
        let (dir, r) = scratch("config-over-script");
        write_teardown_script(&r.tree, "true");
        write_config(&r.project, r#"{"teardown": ["make clean"]}"#);
        assert_eq!(teardown_for(&r, String::new()), "make clean");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_teardown_script_is_told_the_project_and_the_tree() {
        let (dir, r) = scratch("teardown-env");
        write_teardown_script(&r.project, "env | grep -E '^ANYWHERE_(ROOT|WORKSPACE)_' | sort > ../env");
        run(&Removal { delete_branch: false, ..r.clone() }, &teardown_for(&r, String::new()), LIMIT).unwrap();
        let want = format!("ANYWHERE_ROOT_PATH={}\nANYWHERE_WORKSPACE_NAME=feat\nANYWHERE_WORKSPACE_PATH={}\n", r.project, r.tree);
        assert_eq!(std::fs::read_to_string(dir.join("env")).unwrap(), want);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_repo_without_config_has_no_teardown() {
        let (dir, r) = scratch("no-teardown");
        assert_eq!(teardown_for(&r, String::new()), "");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn teardown_is_skipped_once_the_folder_is_gone() {
        let (dir, r) = scratch("gone");
        std::fs::remove_dir_all(&r.tree).unwrap();
        assert_eq!(run(&Removal { delete_branch: false, ..r.clone() }, "exit 1", LIMIT), Ok(Removed { aside: None, kept: None }));
        assert_eq!(git::worktrees(&git::Runner::Local, &r.project).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
