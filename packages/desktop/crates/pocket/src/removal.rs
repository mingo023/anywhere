use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use crate::fs::fs_for;
use crate::util::basename;
use agents::{Agents, Summary};
use store::{Leftover, Subtree};
use gpui_kit::*;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Worktrees being deleted: their rows leave at once, a toast stays until they're gone, and a second delete of one is ignored.
#[derive(Default)]
pub(crate) struct Removals {
    running: HashSet<String>,
    /// Hosts whose sessions, then terminals, arrived since their link last came up, so a sweep sees who works where.
    listed: HashSet<String>,
    terminals: HashSet<String>,
    /// The host leftover whose sheet is being read for or shown; one at a time.
    asking: Option<String>,
    /// Host leftovers whose delete failed, left until their host comes back again rather than retried each sweep.
    failed: HashSet<String>,
}

impl Removals {
    pub(crate) fn running(&self, tree: &str) -> bool {
        self.running.contains(tree)
    }

    pub(crate) fn host_listed(&mut self, host: &str) {
        self.listed.insert(host.to_string());
    }

    pub(crate) fn host_terminals_listed(&mut self, host: &str) {
        self.terminals.insert(host.to_string());
    }

    pub(crate) fn host_dropped(&mut self, host: &str) {
        self.listed.remove(host);
        self.terminals.remove(host);
        self.failed.retain(|f| hosts::split(f).0 != Some(host));
    }

    /// Reserves the sheet for `folder`'s leftover; false while another's is pending.
    fn ask(&mut self, folder: &str) -> bool {
        self.asking.get_or_insert_with(|| folder.to_string()) == folder
    }

    fn host_ready(&self, host: &str) -> bool {
        self.listed.contains(host) && self.terminals.contains(host)
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

/// The subagents' folders on hosts, for each host to resolve to its worktree and check it's still theirs.
#[derive(Default, Debug, PartialEq)]
pub(crate) struct OnHosts {
    /// Each folder, and when its subagent started there on its host's clock: a worktree the host made since is someone else's.
    pub folders: Vec<(String, Option<u64>)>,
    /// Hosts' folders live sessions outside the line work in: a worktree holding one stays.
    pub others: Vec<String>,
    /// Hosts back but not yet relisted, whose `others` may be stale: their folders wait for the leftover sweep.
    pub unready: Vec<String>,
}

impl OnHosts {
    /// The folders to read off their hosts now.
    fn ready(&self) -> impl Iterator<Item = &(String, Option<u64>)> {
        self.folders.iter().filter(|(f, _)| hosts::split(f).0.is_none_or(|h| !self.unready.iter().any(|u| u == h)))
    }
}

/// The deletions that go with `r`'s: each worktree Anywhere made that holds only sessions `r.tree`'s sessions delegated to, and theirs in turn.
/// Their branches go only if `r`'s does. `place` is the tree a session's terminal sits in. Also the folders of those on hosts, for
/// `host_removals` to read off the host.
fn subagent_removals(
    r: &Removal,
    agents: &Agents,
    place: impl Fn(&Summary) -> Option<String>,
    worktrees: &project::Worktrees,
    subtrees: &BTreeMap<String, String>,
    started: impl Fn(&str) -> Option<u64>,
) -> (Vec<Removal>, OnHosts) {
    let tree = r.tree.as_str();
    let roots: Vec<String> = agents.list.iter().filter(|a| place(a).as_deref() == Some(tree)).map(|a| a.id.clone()).collect();
    let subagents = agents.subagents_of(&roots);
    let theirs = |t: &str| agents.list.iter().filter(|a| place(a).as_deref() == Some(t)).all(|a| subagents.iter().any(|s| s.id == a.id));
    let trees = below(std::iter::once(tree.to_string()).chain(subagents.iter().filter_map(|a| place(a))).collect(), subtrees);
    let mut out: Vec<Removal> = Vec::new();
    let mut on_hosts = OnHosts::default();
    let line = |a: &&Summary| roots.contains(&a.id) || subagents.iter().any(|s| s.id == a.id);
    on_hosts.others = agents.list.iter().filter(|a| !line(a)).filter_map(&place).filter(|p| hosts::split(p).0.is_some()).collect();
    for t in trees.into_iter().skip(1) {
        if t == tree || out.iter().any(|o| o.tree == t) || on_hosts.folders.iter().any(|(f, _)| *f == t) || !theirs(&t) {
            continue;
        }
        if hosts::split(&t).0.is_some() {
            let live = agents.list.iter().filter(|a| place(a).as_deref() == Some(t.as_str())).map(|a| a.created_at.max(0) as u64).max();
            let at = started(&t).or(live);
            on_hosts.folders.push((t, at));
        } else if let Some((project, w)) = worktrees.iter().find_map(|(p, ws)| Some((p, ws.iter().find(|w| w.path == t && w.made && !w.main)?))) {
            let branch = Some(w.branch.clone()).filter(|b| branch_deletable(b));
            out.push(Removal { project: project.clone(), tree: t, delete_branch: r.delete_branch && branch.is_some(), branch, teardown: true });
        }
    }
    (out, on_hosts)
}

/// Deletes a leftover's worktree the sweep found clean, unless its branch now holds commits alone: one at a time, so
/// each sees the branches the one before deleted. `Err` hands it back to ask about.
fn sweep_one(r: Removal, own: String, limit: Duration) -> Result<Ordered, Removal> {
    static ONE: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _one = ONE.lock().unwrap_or_else(|e| e.into_inner());
    if r.lost() > 0 {
        return Err(r);
    }
    Ok(remove_in_order(vec![(r, own)], limit))
}

/// The worktrees the hosts holding `trees` say their subagents worked in, as their lineage holds them, with when each
/// was made, for the sweep to tell from a newer one at the same path. That lineage also knows the ones that started and
/// closed while a host's link was down, never listed here. Saved links follow these, as they know when each started.
/// Each comes with the tree whose lineage named it: this Mac's pocketd's for its own trees, a host's for its. A pocketd
/// that couldn't say fails the lot: once the tree goes, nothing would name them again.
fn lineage(trees: &[String]) -> Result<Vec<Named>, String> {
    let mut out = Vec::new();
    for t in trees {
        let below = match hosts::split(t).0 {
            None => daemon::subagent_worktrees(t).map(|ts| ts.into_iter().map(|x| (if x.host.is_empty() { x.path } else { hosts::place(&x.host, &x.path) }, x.since)).collect()),
            Some(id) => match hosts::remote::connected(id) {
                Some(host) => host.subagent_worktrees(t).map_err(|e| e.to_string()),
                None => continue,
            },
        };
        let below: Vec<(String, u64)> = below.map_err(|e| format!("Kept {}, as its subagents couldn't be read: {e}", basename(t)))?;
        out.extend(below.into_iter().map(|(f, since)| (t.clone(), (f, Some(since).filter(|s| *s > 0)))));
    }
    Ok(out)
}

/// The deletions of the worktrees on this Mac that `named` holds, each Anywhere made and none the main checkout.
fn local_removals(named: &[Named], delete_branch: bool) -> Vec<Removal> {
    let mut out: Vec<Removal> = Vec::new();
    for (_, (t, _)) in named.iter().filter(|(_, (t, _))| hosts::split(t).0.is_none()) {
        let ws = git::worktrees(&git::Runner::Local, t);
        let (Some(main), Some(w)) = (ws.iter().find(|w| w.main), ws.iter().find(|w| w.path == *t && w.made && !w.main)) else { continue };
        let branch = Some(w.branch.clone()).filter(|b| branch_deletable(b));
        if !out.iter().any(|o| o.tree == *t) {
            out.push(Removal { project: main.path.clone(), tree: t.clone(), delete_branch: delete_branch && branch.is_some(), branch, teardown: true });
        }
    }
    out
}

/// What `named` holds on hosts, for the sweep.
fn hosted(named: Vec<Named>) -> impl Iterator<Item = (String, Option<u64>)> {
    named.into_iter().map(|(_, f)| f).filter(|(f, _)| hosts::split(f).0.is_some())
}

/// A folder a tree's lineage named, with that tree: `(tree, (folder, since))`.
type Named = (String, (String, Option<u64>));

/// The ones of `lineage` that `trees` named.
fn named_by(lineage: &[Named], trees: &[String]) -> Vec<(String, Option<u64>)> {
    lineage.iter().filter(|(of, _)| trees.contains(of)).map(|(_, f)| f.clone()).collect()
}

/// `trees`, then the trees `links` (child to parent) hang below them, every generation down.
fn below(mut trees: Vec<String>, links: &BTreeMap<String, String>) -> Vec<String> {
    let mut i = 0;
    while let Some(t) = trees.get(i).cloned() {
        trees.extend(links.iter().filter(|(c, p)| **p == t && !trees.contains(c)).map(|(c, _)| c.clone()).collect::<Vec<_>>());
        i += 1;
    }
    trees
}

/// The deletions of the worktrees Anywhere made on hosts that hold `on`'s folders, read off each host; their branches go if `delete_branch`.
pub(crate) fn host_removals(on: &OnHosts, delete_branch: bool) -> Vec<Removal> {
    let mut out: Vec<Removal> = Vec::new();
    for (f, made) in on.ready() {
        let run = hosts::runner_for(f);
        let ws = git::worktrees(&run, f);
        let removal = host_removal(&ws, f, *made, |t| git::made_at(&run, t), &on.others, delete_branch);
        if let Some(r) = removal.filter(|r| !out.iter().any(|o| o.tree == r.tree)) {
            out.push(r);
        }
    }
    out
}

/// What a host holds at a subagent's folder.
#[derive(Debug, PartialEq)]
enum Found {
    /// The worktree Anywhere made for it, to delete.
    Theirs(Removal),
    /// That worktree, which a live session outside the line works in too.
    Busy,
    /// No worktree Anywhere made, or one made after the subagent started.
    Gone,
    /// The host's worktrees or the worktree's make time couldn't be read.
    Unread,
}

/// What the worktree Anywhere made among a host's `ws` that holds `folder` is to the subagent that `started` there, with
/// `others` the folders of live sessions outside its line.
fn host_found(ws: &[git::Worktree], folder: &str, started: Option<u64>, made_at: impl Fn(&str) -> Option<u64>, others: &[String], delete_branch: bool) -> Found {
    // Every repo lists its main worktree, so a listing without one is a failed read.
    let Some(main) = ws.iter().find(|w| w.main) else { return Found::Unread };
    let Some(w) = holding(ws, folder) else { return Found::Gone };
    // Its marker was just listed, so a missing make time is a failed read.
    match (started, made_at(&w.path)) {
        (Some(_), None) => return Found::Unread,
        (Some(s), Some(m)) if m > s => return Found::Gone,
        _ => {}
    }
    if others.iter().any(|o| holds(&w.path, o)) {
        return Found::Busy;
    }
    let branch = Some(w.branch.clone()).filter(|b| branch_deletable(b));
    Found::Theirs(Removal { project: main.path.clone(), tree: w.path.clone(), delete_branch: delete_branch && branch.is_some(), branch, teardown: true })
}

fn host_removal(ws: &[git::Worktree], folder: &str, started: Option<u64>, made_at: impl Fn(&str) -> Option<u64>, others: &[String], delete_branch: bool) -> Option<Removal> {
    match host_found(ws, folder, started, made_at, others, delete_branch) {
        Found::Theirs(r) => Some(r),
        Found::Busy | Found::Gone | Found::Unread => None,
    }
}

/// What became of a subagent's worktree left on a host, once the host is back.
enum Swept {
    /// Still there for a later listing: a live session works in it, or the host dropped again or couldn't be read.
    Wait,
    /// Holds work nobody checked, so the user is asked.
    Ask(Removal),
    /// No longer the subagent's, or nothing to delete.
    Done,
    /// The subagent's, clean, to delete.
    Delete(Removal),
}

/// Why the sweep kept a leftover it meant to delete.
enum Held {
    /// Its branch now holds commits alone.
    Ask(Removal),
    /// Its host couldn't say what its subagents left.
    Unread(String),
}

/// The deletions done, and the one that failed with why.
type Ordered = (Vec<(Removal, Removed)>, Option<(Removal, Failure)>);

/// Deletes `jobs`, each with its project settings' teardown, in order, stopping at the first that fails.
fn remove_in_order(jobs: Vec<(Removal, String)>, limit: Duration) -> Ordered {
    let mut done = Vec::new();
    for (r, own) in jobs {
        match run(&r, &teardown_for(&r, own), limit) {
            Ok(removed) => done.push((r, removed)),
            Err(failure) => return (done, Some((r, failure))),
        }
    }
    (done, None)
}

/// `jobs`, subagents' then the tree asked for last, without the subagents' worktrees on hosts that aren't among those
/// `reached` again as still theirs.
fn still_theirs(jobs: Vec<(Removal, String)>, reached: &[String]) -> Vec<(Removal, String)> {
    let last = jobs.len().saturating_sub(1);
    jobs.into_iter().enumerate().filter(|(i, (r, _))| *i == last || hosts::split(&r.tree).0.is_none() || reached.contains(&r.tree)).map(|(_, j)| j).collect()
}

/// The hosts' subagent folders a delete didn't reach, with the deleted `trees`: a host offline at the sheet or since, or
/// whose worktree changed hands, is checked again once it's back.
fn leftovers(folders: Vec<(String, Option<u64>)>, trees: &[String], delete_branch: bool) -> Vec<(String, Leftover)> {
    let left = |f: &str| !trees.iter().any(|t| holds(t, f));
    folders.into_iter().filter(|(f, _)| left(f)).map(|(f, s)| (f, Leftover { started: s.unwrap_or(0), delete_branch })).collect()
}

/// The subagents' deletions whose worktrees hold none of `shells`.
fn unoccupied(subtrees: Vec<Removal>, shells: &[String]) -> Vec<Removal> {
    subtrees.into_iter().filter(|r| !shells.iter().any(|s| holds(&r.tree, s))).collect()
}

/// Drops the leftovers `tree` held, once it's deleted or the user chose to keep it.
fn settle(leftovers: &mut BTreeMap<String, Leftover>, tree: &str) {
    leftovers.retain(|f, _| !holds(tree, f));
}

/// Moves the links of `tree`'s own subagents onto `root`, so they stay reachable from it once `tree` is forgotten and a
/// delete that stopped part way can carry on.
fn reparent(subtrees: &mut BTreeMap<String, Subtree>, tree: &str, root: &str, tree_of: impl Fn(&str) -> Option<String>) {
    for s in subtrees.values_mut().filter(|s| tree_of(&s.parent).as_deref() == Some(tree)) {
        s.parent = root.to_string();
    }
}

/// Whether `folder` is on a host whose link is up.
fn connected(folder: &str) -> bool {
    hosts::split(folder).0.is_some_and(|h| hosts::remote::connected(h).is_some())
}

/// The worktree Anywhere made that holds `folder`, the innermost.
fn holding<'a>(ws: &'a [git::Worktree], folder: &str) -> Option<&'a git::Worktree> {
    ws.iter().filter(|w| w.made && !w.main && holds(&w.path, folder)).max_by_key(|w| w.path.len())
}

fn holds(tree: &str, folder: &str) -> bool {
    folder == tree || folder.starts_with(&format!("{tree}/"))
}

/// The deepest of `folder` and its parents that exists, so a deleted subfolder still leads to its worktree. `None` when one
/// couldn't be read.
fn nearest(folder: &str, exists: impl Fn(&str) -> std::io::Result<bool>) -> Option<String> {
    let mut at = folder;
    while !exists(at).ok()? {
        match at.rsplit_once('/') {
            Some((up, _)) if matches!(hosts::split(up), (Some(_), p) if p.len() > 1) => at = up,
            _ => break,
        }
    }
    Some(at.to_string())
}

/// A subagent's folder as the desktop keys it: a host's session listed by this Mac's pocketd carries the host's own path.
fn folder(a: &Summary) -> Option<String> {
    match hosts::session::untag(&a.id).0 {
        _ if a.cwd.is_empty() => None,
        Some(host) if hosts::split(&a.cwd).0.is_none() => Some(hosts::place(host, &a.cwd)),
        _ => Some(a.cwd.clone()),
    }
}

/// Adds the links not known yet, keeping the parent first seen; returns whether any was added.
/// A folder keeps the parent it was first seen with while that subagent lives. Once it's gone, a subagent started
/// there since is a new delegation, likely in a new worktree at the same path, and takes the folder over.
fn remember(subtrees: &mut BTreeMap<String, Subtree>, links: Vec<(String, Subtree)>) -> bool {
    let mut changed = false;
    for (child, link) in &links {
        let live = |s: &Subtree| links.iter().any(|(c, l)| c == child && l.started == s.started);
        if subtrees.get(child).is_none_or(|saved| link.started > saved.started && !live(saved)) {
            subtrees.insert(child.clone(), link.clone());
            changed = true;
        }
    }
    changed
}

/// Whether a worktree made at `created` can be the one a subagent that started at `started` worked in: one made
/// since is someone else's.
fn there_when_started(created: Option<SystemTime>, started: u64) -> bool {
    let created = created.and_then(|c| c.duration_since(UNIX_EPOCH).ok()).map(|d| d.as_millis() as u64);
    created.is_none_or(|c| c <= started)
}

/// The saved links whose folders still hold the worktrees they held: each child's tree to its parent's, as `tree_of`
/// resolves them, without those `created` says were made after the subagent started. The subagent's start is on its
/// machine's clock, so a parent's folder on another machine isn't held to it.
fn saved_links(subtrees: &BTreeMap<String, Subtree>, tree_of: impl Fn(&str) -> Option<String>, created: impl Fn(&str) -> Option<SystemTime>) -> BTreeMap<String, String> {
    let then = |f: &str, started: u64| tree_of(f).filter(|t| there_when_started(created(t), started));
    let parent = |c: &str, s: &Subtree| if hosts::split(c).0 == hosts::split(&s.parent).0 { then(&s.parent, s.started) } else { tree_of(&s.parent) };
    subtrees.iter().filter_map(|(c, s)| Some((then(c, s.started)?, parent(c, s)?))).collect()
}

/// Each live subagent placed apart from its parent: its folder, its parent's and when it started.
fn subtree_links(agents: &Agents, place: impl Fn(&Summary) -> Option<String>) -> Vec<(String, Subtree)> {
    let links = agents.list.iter().filter(|a| a.relationship == "subagent");
    let link = |a: &Summary| Some((place(a)?, Subtree { parent: place(agents.get(&a.parent)?)?, started: a.created_at.max(0) as u64 }));
    links.filter_map(link).filter(|(c, s)| *c != s.parent).collect()
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
    /// `subagent_removals` for `r`, here; the host folders in it go through `host_removals`, off the UI thread.
    pub(crate) fn subagent_removals(&self, r: &Removal) -> (Vec<Removal>, OnHosts) {
        let place = |a: &Summary| self.session_place(a).or_else(|| folder(a).filter(|f| hosts::split(f).0.is_some()));
        let started = |t: &str| self.saved_start(t);
        let (subtrees, mut on_hosts) = subagent_removals(r, &self.agents, place, &self.worktrees, &self.saved_links(), started);
        let shells = self.shells();
        on_hosts.others.extend(shells.iter().filter(|f| hosts::split(f).0.is_some()).cloned());
        let unready = on_hosts.folders.iter().filter_map(|(f, _)| hosts::split(f).0).filter(|h| !self.removals.host_ready(h)).map(str::to_string).collect();
        on_hosts.unready = unready;
        (unoccupied(subtrees, &shells), on_hosts)
    }

    /// The host folders live sessions and shells work in.
    fn host_occupants(&self) -> Vec<String> {
        let sessions = self.agents.list.iter().filter_map(|a| self.session_place(a).or_else(|| folder(a)));
        sessions.chain(self.shells()).filter(|f| hosts::split(f).0.is_some()).collect()
    }

    /// The saved subagent links, child tree to parent tree.
    fn saved_links(&self) -> BTreeMap<String, String> {
        // A host's folder has no creation time here; its host checks it against `started`.
        // From the last git refresh, read off the UI thread.
        let created = |t: &str| self.worktrees.values().flatten().find(|w| w.path == t).and_then(|w| w.created);
        saved_links(&self.store.subtrees, |f| self.folder_tree(f), created)
    }

    /// When the subagent saved in `tree` started.
    fn saved_start(&self, tree: &str) -> Option<u64> {
        self.store.subtrees.iter().find(|(c, _)| self.folder_tree(c).as_deref() == Some(tree)).map(|(_, s)| s.started)
    }

    /// The folders of live terminals no session runs in: a plain shell is someone working there too.
    fn shells(&self) -> Vec<String> {
        let agents = |id: &str| self.agents.list.iter().any(|a| a.terminal_id == id);
        let live = self.terminals.sessions.items.iter().filter(|s| s.exit.is_none() && !agents(&s.info.id));
        live.map(|s| s.info.cwd.clone()).collect()
    }

    /// The tree holding `folder`, or a host's folder as itself when that host's worktrees aren't listed here.
    fn folder_tree(&self, folder: &str) -> Option<String> {
        self.tree_of(folder).or_else(|| Some(folder.to_string()).filter(|f| hosts::split(f).0.is_some()))
    }

    fn session_place(&self, a: &Summary) -> Option<String> {
        self.terminals.sessions.get(&a.terminal_id).and_then(|s| self.place_of(&s.info))
    }

    /// Records each live subagent's folder: once the sessions close, nothing else says whose it was. Folders, not
    /// worktrees, as a subagent's worktree may be newer than the last git refresh.
    pub(crate) fn remember_subtrees(&mut self, cx: &mut Context<Self>) {
        if remember(&mut self.store.subtrees, subtree_links(&self.agents, folder)) {
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
        self.ask_delete(removal, None, cx);
    }

    /// `leftover` is the host folder the sweep asks about, for a sheet the user didn't ask for.
    fn ask_delete(&mut self, removal: Removal, leftover: Option<String>, cx: &mut Context<Self>) {
        let (mut subtrees, on_hosts) = self.subagent_removals(&removal);
        let main = removal.clone();
        let task = cx.background_executor().spawn(async move {
            // A delete that can't read it refuses; the sheet shows what it can.
            let named = lineage(std::slice::from_ref(&main.tree)).unwrap_or_default();
            let mut on_hosts = on_hosts;
            on_hosts.folders.extend(hosted(named.clone()).filter(|(f, _)| !on_hosts.folders.iter().any(|(o, _)| o == f)).collect::<Vec<_>>());
            for r in local_removals(&named, main.delete_branch) {
                if r.tree != main.tree && !subtrees.iter().any(|s| s.tree == r.tree) {
                    subtrees.push(r);
                }
            }
            subtrees.extend(host_removals(&on_hosts, main.delete_branch));
            let jobs: Vec<Removal> = std::iter::once(main).chain(subtrees.clone()).collect();
            let dirty: usize = jobs.iter().map(|j| git::dirty_files(&hosts::runner_for(&j.tree), &j.tree).unwrap_or(0)).sum();
            let (lost, anyway) = lost_together(&jobs);
            (dirty, lost, anyway, subtrees)
        });
        cx.spawn(async move |this, cx| {
            let (dirty, lost, lost_anyway, subtrees) = task.await;
            this.update(cx, |d, cx| {
                // Another sheet came up meanwhile, or this one's reservation went: a later sweep asks again.
                if let Some(f) = &leftover
                    && (d.overlay.is_some() || d.removals.asking.as_ref() != Some(f))
                {
                    d.removals.asking.take_if(|a| a == f);
                    return;
                }
                let requested = leftover.is_none();
                d.confirm = Some(Confirm::DeleteWorktree { removal, dirty, lost, lost_anyway, subtrees, requested });
                d.overlay = Some(Overlay::Confirm);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Deletes the `confirmed` subagent deletions, the ones the user was shown and whose work was checked, that are still
    /// `removal`'s subagents', then `removal`'s tree. A failure stops there, keeping the parent to delete again; a host
    /// that's offline gets its subagents' worktrees deleted once it's back. Their branches go only if `removal`'s does.
    pub(crate) fn delete_worktree(&mut self, removal: Removal, confirmed: &[Removal], cx: &mut Context<Self>) {
        if !self.removals.start(&removal.tree) {
            return;
        }
        if self.worktree.as_deref() == Some(&removal.tree) {
            self.select_tree(removal.project.clone(), None, cx);
        }
        // Read now: once its sessions close, pocketd closes their subagents and the lineage is gone.
        let (still, on_hosts) = self.subagent_removals(&removal);
        let known = |c: &Removal| still.iter().any(|s| s.tree == c.tree) || on_hosts.folders.iter().any(|(f, _)| holds(&c.tree, f));
        let confirmed: Vec<Removal> = confirmed.iter().map(|c| Removal { delete_branch: removal.delete_branch && c.branch.is_some(), ..c.clone() }).collect();
        // The rest of this Mac's go if its pocketd's lineage still names them, read off the UI thread.
        let (subagents, unsure): (Vec<Removal>, Vec<Removal>) = confirmed.into_iter().filter(|c| known(c) || hosts::split(&c.tree).0.is_none()).partition(known);
        let subagents: Vec<Removal> = subagents.into_iter().filter(|c| self.removals.start(&c.tree)).collect();
        let unsure: Vec<Removal> = unsure.into_iter().filter(|c| self.removals.start(&c.tree)).collect();
        let started: Vec<String> = subagents.iter().chain(&unsure).map(|s| s.tree.clone()).chain(std::iter::once(removal.tree.clone())).collect();
        let teardown = |r: &Removal| self.store.repos.get(&r.project).map(|x| x.teardown.clone()).unwrap_or_default();
        let jobs: Vec<(Removal, String)> = subagents.iter().map(|r| (r.clone(), teardown(r))).collect();
        let unsure: Vec<(Removal, String)> = unsure.into_iter().map(|r| { let t = teardown(&r); (r, t) }).collect();
        let main = (removal.clone(), teardown(&removal));
        let (limit, trash) = (Duration::from_secs(self.store.worktree.teardown_secs.into()), self.store.worktree.trash);
        let delete_branch = removal.delete_branch;
        let task = cx.background_executor().spawn(async move {
            // Read again: a host's worktree may have changed hands since the sheet read it.
            let reached: Vec<String> = host_removals(&on_hosts, false).into_iter().map(|r| r.tree).collect();
            let trees: Vec<String> = jobs.iter().chain(&unsure).chain(std::iter::once(&main)).map(|(r, _)| r.tree.clone()).collect();
            let theirs = match lineage(&trees) {
                Ok(theirs) => theirs,
                Err(e) => return ((vec![], Some((main.0, Failure::Remove(e)))), vec![], vec![]),
            };
            let named = |r: &Removal| theirs.iter().any(|(_, (f, _))| *f == r.tree);
            let jobs: Vec<(Removal, String)> = jobs.into_iter().chain(unsure.into_iter().filter(|(r, _)| named(r))).chain(std::iter::once(main)).collect();
            let jobs = still_theirs(jobs, &reached);
            let (done, failed) = remove_in_order(jobs, limit);
            let trees: Vec<String> = done.iter().map(|(r, _)| r.tree.clone()).collect();
            // Named only by a tree now gone: nothing could find them again once a later job fails.
            let orphans: Vec<(String, Option<u64>)> = named_by(&theirs, &trees).into_iter().filter(|(f, _)| hosts::split(f).0.is_some()).collect();
            let folders = hosted(theirs).chain(on_hosts.folders).collect();
            ((done, failed), leftovers(folders, &trees, delete_branch), leftovers(orphans, &trees, delete_branch))
        });
        cx.spawn(async move |this, cx| {
            let ((done, failed), leftovers, orphans) = task.await;
            this.update(cx, |d, cx| {
                for t in &started {
                    d.removals.running.remove(t);
                }
                let rest: Vec<Removal> = subagents.into_iter().filter(|s| !done.iter().any(|(r, _)| r.tree == s.tree)).collect();
                for (r, Removed { aside, kept }) in done {
                    if let Some(aside) = aside {
                        cx.background_executor().spawn(async move { dispose(&aside, trash) }).detach();
                    }
                    if let Some((branch, e)) = kept {
                        d.error = Some(format!("Deleted the worktree; kept branch {branch}: {e}"));
                    }
                    let mut subtrees = std::mem::take(&mut d.store.subtrees);
                    reparent(&mut subtrees, &r.tree, &removal.tree, |f| d.folder_tree(f));
                    d.store.subtrees = subtrees;
                    d.forget_tree(&r.tree, cx);
                }
                if failed.is_some() && !orphans.is_empty() {
                    d.store.host_leftovers.extend(orphans);
                    d.save_soon(cx);
                    d.sweep_host_leftovers(cx);
                }
                match failed {
                    None => {
                        d.store.host_leftovers.extend(leftovers);
                        d.save_soon(cx);
                        // Its host may have come back and listed already, with nothing left to trigger a sweep.
                        d.sweep_host_leftovers(cx);
                    }
                    Some((r, Failure::Teardown(tail))) => {
                        d.confirm = Some(Confirm::TeardownFailed { removal: removal.clone(), failed: r.tree, tail, subtrees: rest });
                        d.overlay = Some(Overlay::Confirm);
                    }
                    Some((r, Failure::Remove(e))) if r.tree == removal.tree => d.error = Some(e),
                    Some((r, Failure::Remove(e))) => d.error = Some(format!("Kept {}, as {} couldn't be deleted: {e}", basename(&removal.tree), basename(&r.tree))),
                }
                d.refresh_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Deletes the worktrees subagents left on hosts when their parent's went, once each host is back and has listed its
    /// sessions.
    /// One a live session still works in waits; one holding work nobody checked asks first.
    pub(crate) fn sweep_host_leftovers(&mut self, cx: &mut Context<Self>) {
        let ready = |f: &str| connected(f) && hosts::split(f).0.is_some_and(|h| self.removals.host_ready(h));
        let asked = |f: &str| self.removals.asking.as_deref() == Some(f) || self.removals.failed.contains(f);
        let back: Vec<(String, Leftover)> = self.store.host_leftovers.iter().filter(|(f, _)| ready(f) && !asked(f)).map(|(f, l)| (f.clone(), *l)).collect();
        let back: Vec<(String, Leftover)> = back.into_iter().filter(|(f, _)| self.removals.start(f)).collect();
        if back.is_empty() {
            return;
        }
        let others = self.host_occupants();
        let task = cx.background_executor().spawn(async move {
            let sweep = |(f, l): (String, Leftover)| {
                let run = hosts::runner_for(&f);
                let Some(at) = nearest(&f, |p| run.stat(p).map(|s| s.is_some())) else { return (f, Swept::Wait) };
                let found = host_found(&git::worktrees(&run, &at), &f, Some(l.started).filter(|s| *s > 0), |t| git::made_at(&run, t), &others, l.delete_branch);
                // A host that dropped mid-read lists no worktrees, which isn't an answer.
                let outcome = match found {
                    _ if !connected(&f) => Swept::Wait,
                    Found::Unread if matches!(run.stat(&f), Ok(None)) => Swept::Done,
                    Found::Busy | Found::Unread => Swept::Wait,
                    Found::Gone => Swept::Done,
                    Found::Theirs(r) if git::dirty_files(&run, &r.tree).is_none_or(|n| n > 0) || r.lost() > 0 => Swept::Ask(r),
                    Found::Theirs(r) => Swept::Delete(r),
                };
                (f, outcome)
            };
            back.into_iter().map(sweep).collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let swept = task.await;
            this.update(cx, |d, cx| {
                for (f, outcome) in swept {
                    d.removals.running.remove(&f);
                    match outcome {
                        Swept::Wait => continue,
                        // Kept until it's deleted or the user keeps it: the host may drop again before then. One sheet at
                        // a time; the rest are asked once it closes.
                        Swept::Ask(r) => {
                            if d.overlay.is_none() && d.removals.ask(&f) {
                                d.ask_delete(r, Some(f), cx);
                            }
                            continue;
                        }
                        Swept::Done => {}
                        Swept::Delete(r) => {
                            d.sweep_delete(f, r, cx);
                            continue;
                        }
                    }
                    d.store.host_leftovers.remove(&f);
                }
                d.save_soon(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Deletes `r`, the worktree the host leftover at `f` is in, and leaves its own subagents' on hosts to the sweep in
    /// turn: one may have delegated while its host was away.
    fn sweep_delete(&mut self, f: String, r: Removal, cx: &mut Context<Self>) {
        // Someone may have started working there while the sweep read the host; a later sweep sees.
        if self.host_occupants().iter().any(|o| holds(&r.tree, o)) || !self.removals.start(&f) {
            return;
        }
        let (_, mut on_hosts) = self.subagent_removals(&r);
        // Ones still closing, or whose parent left the list, are saved links only; each sweep tells whose each is.
        for t in below(vec![r.tree.clone()], &self.saved_links()).into_iter().skip(1).filter(|t| hosts::split(t).0.is_some()) {
            if !on_hosts.folders.iter().any(|(f, _)| *f == t) {
                let at = self.saved_start(&t);
                on_hosts.folders.push((t, at));
            }
        }
        let limit = Duration::from_secs(self.store.worktree.teardown_secs.into());
        let own = self.store.repos.get(&r.project).map(|x| x.teardown.clone()).unwrap_or_default();
        let delete_branch = r.delete_branch;
        let task = cx.background_executor().spawn(async move {
            let theirs = lineage(std::slice::from_ref(&r.tree)).map_err(Held::Unread)?;
            let (done, failed) = sweep_one(r, own, limit).map_err(Held::Ask)?;
            let trees: Vec<String> = done.iter().map(|(r, _)| r.tree.clone()).collect();
            let folders = hosted(theirs).chain(on_hosts.folders).collect();
            Ok(((done, failed), leftovers(folders, &trees, delete_branch)))
        });
        cx.spawn(async move |this, cx| {
            let swept = task.await;
            this.update(cx, |d, cx| {
                d.removals.running.remove(&f);
                let ((done, failed), below) = match swept {
                    Ok(swept) => swept,
                    Err(Held::Ask(r)) => {
                        if d.overlay.is_none() && d.removals.ask(&f) {
                            d.ask_delete(r, Some(f), cx);
                        }
                        return;
                    }
                    // Tried again once its host comes back.
                    Err(Held::Unread(e)) => {
                        d.error = Some(e);
                        d.removals.failed.insert(f);
                        cx.notify();
                        return;
                    }
                };
                for (r, Removed { aside, kept }) in done {
                    if let Some(aside) = aside {
                        let trash = d.store.worktree.trash;
                        cx.background_executor().spawn(async move { dispose(&aside, trash) }).detach();
                    }
                    if let Some((branch, e)) = kept {
                        d.error = Some(format!("Deleted the worktree; kept branch {branch}: {e}"));
                    }
                    d.forget_tree(&r.tree, cx);
                }
                match failed {
                    None => {
                        d.store.host_leftovers.remove(&f);
                        d.store.host_leftovers.extend(below);
                        d.sweep_host_leftovers(cx);
                    }
                    // Its sheet offers to delete it without the teardown, as for a tree the user deleted.
                    Some((r, Failure::Teardown(tail))) if d.overlay.is_none() && d.removals.ask(&f) => {
                        d.confirm = Some(Confirm::TeardownFailed { failed: r.tree.clone(), removal: r, tail, subtrees: vec![] });
                        d.overlay = Some(Overlay::Confirm);
                    }
                    Some((r, Failure::Teardown(e) | Failure::Remove(e))) => {
                        d.error = Some(format!("Couldn't delete {}: {e}", basename(&r.tree)));
                        d.removals.failed.insert(f);
                    }
                }
                d.save_soon(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// A sheet closed. Still holding a delete, the user cancelled it: a host leftover it asked about stays.
    pub(crate) fn sheet_closed(&mut self) {
        self.removals.asking = None;
        if let Some(Confirm::DeleteWorktree { removal, .. } | Confirm::TeardownFailed { removal, .. }) = &self.confirm {
            settle(&mut self.store.host_leftovers, &removal.tree);
        }
    }

    pub(crate) fn forget_tree(&mut self, tree: &str, cx: &mut Context<Self>) {
        settle(&mut self.store.host_leftovers, tree);
        // Before the tree leaves `worktrees`, while its folders still resolve to it.
        let gone: Vec<String> = self.store.subtrees.iter().filter(|(c, s)| [*c, &s.parent].iter().any(|f| self.folder_tree(f).as_deref() == Some(tree))).map(|(c, _)| c.clone()).collect();
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
    use super::{CONFIG_FILE, Failure, Removal, Removals, Removed, TEARDOWN_SCRIPT, branch_deletable, dispose, Found, below, named_by, host_found, host_removal, leftovers, nearest, lost_together, reparent, still_theirs, remove_in_order, run, settle, sweep_one, unoccupied, OnHosts, run_script, folder, remember, saved_links, subagent_removals, subtree_links, teardown_for, there_when_started};
    use std::collections::BTreeMap;
    use std::time::Duration;
    use store::Leftover;
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
        git::Worktree { path: path.into(), branch: String::new(), main, made: false, created: None }
    }

    #[test]
    fn a_worktree_takes_the_ones_anywhere_made_for_its_sessions_subagents_all_the_way_down() {
        let agent = |id: &str, parent: &str| agents::Summary { id: id.into(), terminal_id: id.into(), parent: parent.into(), relationship: if parent.is_empty() { "" } else { "subagent" }.into(), ..Default::default() };
        let list = vec![agent("p", ""), agent("c", "p"), agent("g", "c"), agent("in-main", "p"), agent("shared", "p"), agent("user", ""), agent("unmade", "p"), agent("beside", "p")];
        let at: std::collections::HashMap<&str, &str> =
            [("p", "/wt"), ("c", "/wt-c"), ("g", "/wt-g"), ("in-main", "/app"), ("shared", "/wt-s"), ("user", "/wt-s"), ("unmade", "/wt-u"), ("beside", "/wt")].into();
        let place = |a: &agents::Summary| at.get(a.terminal_id.as_str()).map(|t| t.to_string());
        let made = |path: &str, branch: &str, made: bool| git::Worktree { path: path.into(), branch: branch.into(), main: false, made, created: None };
        let main = git::Worktree { path: "/app".into(), branch: "main".into(), main: true, made: false, created: None };
        let trees = vec![main, made("/wt", "feat", true), made("/wt-c", "tests", true), made("/wt-g", "detached", true), made("/wt-s", "s", true), made("/wt-u", "u", false)];
        let worktrees = [("/app".to_string(), trees)].into();
        let agents = agents::Agents { list, ..Default::default() };
        let parent = |delete_branch| Removal { project: "/app".into(), tree: "/wt".into(), branch: Some("feat".into()), delete_branch, teardown: false };
        let child = |tree: &str, branch: Option<&str>, delete_branch| Removal { project: "/app".into(), tree: tree.into(), branch: branch.map(String::from), delete_branch, teardown: true };
        assert_eq!(subagent_removals(&parent(true), &agents, place, &worktrees, &Default::default(), |_| None).0, [child("/wt-c", Some("tests"), true), child("/wt-g", None, false)]);
        assert_eq!(subagent_removals(&parent(false), &agents, place, &worktrees, &Default::default(), |_| None).0, [child("/wt-c", Some("tests"), false), child("/wt-g", None, false)]);
    }

    #[test]
    fn a_worktree_takes_its_subagents_worktrees_after_their_sessions_closed() {
        let user = agents::Summary { id: "user".into(), terminal_id: "user".into(), ..Default::default() };
        let theirs = agents::Summary { id: "devbox/x".into(), terminal_id: "devbox/x".into(), ..Default::default() };
        let agents = agents::Agents { list: vec![user, theirs], ..Default::default() };
        let place = |a: &agents::Summary| Some(if a.id == "user" { "/wt-busy" } else { "//devbox/home/dev/wt/notes/docs" }.to_string());
        let made = |path: &str| git::Worktree { path: path.into(), branch: format!("b{path}"), main: false, made: true, created: None };
        let worktrees = [("/app".to_string(), vec![made("/wt"), made("/wt-c"), made("/wt-g"), made("/wt-busy"), made("/wt-other")]), ("//devbox/app".to_string(), vec![made("//devbox/wt-listed")])].into();
        let subtrees = [("/wt-c", "/wt"), ("/wt-g", "/wt-c"), ("/wt-busy", "/wt"), ("/wt-other", "/elsewhere"), ("//devbox/home/dev/wt/notes", "/wt-c"), ("//devbox/wt-listed", "/wt")].map(|(c, p)| (c.to_string(), p.to_string())).into();
        let parent = Removal { project: "/app".into(), tree: "/wt".into(), branch: Some("feat".into()), delete_branch: true, teardown: false };
        let started = |f: &str| (f == "//devbox/home/dev/wt/notes").then_some(7);
        let (removals, on_hosts) = subagent_removals(&parent, &agents, place, &worktrees, &subtrees, started);
        assert_eq!(removals.into_iter().map(|r| r.tree).collect::<Vec<_>>(), ["/wt-c", "/wt-g"]);
        assert_eq!(on_hosts.folders, [("//devbox/wt-listed".to_string(), None), ("//devbox/home/dev/wt/notes".to_string(), Some(7))], "a host's worktree, listed here or not, is checked on its host");
        assert_eq!(on_hosts.others, ["//devbox/home/dev/wt/notes/docs"]);
    }

    #[test]
    fn a_hosts_worktree_goes_only_while_its_the_one_made_then_and_nobody_else_works_in_it() {
        let tree = |path: &str, main: bool| git::Worktree { path: path.into(), branch: "notes".into(), main, made: !main, created: None };
        let ws = [tree("//devbox/app", true), tree("//devbox/wt/notes", false)];
        let made_at = |_: &str| Some(7);
        let found = |started: Option<u64>, others: &[&str]| {
            let others: Vec<String> = others.iter().map(|o| o.to_string()).collect();
            host_found(&ws, "//devbox/wt/notes/src", started, made_at, &others, true)
        };
        let theirs = Found::Theirs(Removal { project: "//devbox/app".into(), tree: "//devbox/wt/notes".into(), branch: Some("notes".into()), delete_branch: true, teardown: true });
        assert_eq!(found(Some(7), &[]), theirs);
        assert_eq!(found(None, &["//devbox/wt/notes-2", "//devbox/app"]), theirs);
        assert_eq!(found(Some(6), &[]), Found::Gone, "a worktree made at the path since");
        assert_eq!(found(Some(6), &["//devbox/wt/notes/docs"]), Found::Gone, "someone else's, busy or not");
        assert_eq!(found(Some(7), &["//devbox/wt/notes/docs"]), Found::Busy, "someone else works in another of its folders, for now");
        assert_eq!(host_found(&ws, "//devbox/wt/other", Some(7), made_at, &[], true), Found::Gone);
        assert_eq!(host_removal(&ws, "//devbox/wt/notes/src", Some(7), made_at, &["//devbox/wt/notes".into()], true), None);
        assert_eq!(host_found(&[], "//devbox/wt/notes/src", Some(7), made_at, &[], true), Found::Unread, "a listing that failed");
        assert_eq!(host_found(&ws, "//devbox/wt/notes/src", Some(7), |_: &str| None, &[], true), Found::Unread, "a marker that couldn't be read");
    }

    #[test]
    fn a_subagent_is_remembered_by_the_worktree_it_works_in_apart_from_its_parent() {
        let agent = |id: &str, parent: &str, relationship: &str| agents::Summary { id: id.into(), terminal_id: id.into(), parent: parent.into(), relationship: relationship.into(), created_at: 5, ..Default::default() };
        let list = vec![agent("p", "", ""), agent("c", "p", "subagent"), agent("shared", "p", "subagent"), agent("fork", "p", "fork"), agent("orphan", "gone", "subagent")];
        let at: std::collections::HashMap<&str, &str> = [("p", "/wt"), ("c", "/wt-c"), ("shared", "/wt"), ("fork", "/wt-f"), ("orphan", "/wt-o")].into();
        let place = |a: &agents::Summary| at.get(a.id.as_str()).map(|t| t.to_string());
        let links = subtree_links(&agents::Agents { list, ..Default::default() }, place);
        assert_eq!(links, [("/wt-c".to_string(), store::Subtree { parent: "/wt".into(), started: 5 })]);
    }

    #[test]
    fn a_subagents_folder_keeps_its_first_parent_while_that_subagent_lives() {
        let seen = |p: &str, at: u64| store::Subtree { parent: p.into(), started: at };
        let link = |c: &str, p: &str, at: u64| (c.to_string(), seen(p, at));
        let mut subtrees = std::collections::BTreeMap::new();
        assert!(remember(&mut subtrees, vec![link("/wt-c", "/wt", 1)]));
        assert!(!remember(&mut subtrees, vec![link("/wt-c", "/wt", 1), link("/wt-c", "/other", 2)]));
        assert!(remember(&mut subtrees, vec![link("/wt-g", "/wt-c", 3)]));
        assert!(!remember(&mut subtrees, vec![link("/wt-g", "/wt-c", 3)]));
        assert_eq!(subtrees, [("/wt-c".to_string(), seen("/wt", 1)), ("/wt-g".to_string(), seen("/wt-c", 3))].into());
        assert!(remember(&mut subtrees, vec![link("/wt-c", "/new", 5)]), "a new delegation at a path its first subagent left");
        assert!(!remember(&mut subtrees, vec![link("/wt-c", "/older", 4)]));
        assert_eq!(subtrees["/wt-c"], seen("/new", 5));
    }

    #[test]
    fn a_hosts_subagent_is_kept_by_its_folder_on_that_host_never_by_the_same_path_here() {
        let at = |id: &str, cwd: &str| folder(&agents::Summary { id: id.into(), cwd: cwd.into(), ..Default::default() });
        assert_eq!(at("c", "/home/dev/wt"), Some("/home/dev/wt".into()));
        assert_eq!(at("devbox/c", "/home/dev/wt"), Some("//devbox/home/dev/wt".into()));
        assert_eq!(at("devbox/c", "//devbox/home/dev/wt"), Some("//devbox/home/dev/wt".into()));
        assert_eq!(at("devbox/c", ""), None);
    }

    #[test]
    fn a_worktree_made_after_its_subagent_started_there_is_someone_elses() {
        let at = |ms: u64| Some(std::time::UNIX_EPOCH + Duration::from_millis(ms));
        assert!(there_when_started(at(10), 10));
        assert!(!there_when_started(at(11), 10));
        assert!(there_when_started(None, 10));
    }

    #[test]
    fn a_saved_link_goes_once_its_subagents_or_its_parents_folder_holds_a_newer_worktree() {
        let link = |p: &str| store::Subtree { parent: p.into(), started: 10 };
        let subtrees = [("/wt-c", link("/wt")), ("/wt-new", link("/wt")), ("/wt-g", link("/wt-reused")), ("//devbox/wt/notes", link("/wt")), ("//devbox/wt/other", link("/wt-reused"))].map(|(c, l)| (c.to_string(), l)).into();
        let made = |ms: u64| Some(std::time::UNIX_EPOCH + Duration::from_millis(ms));
        let created = |t: &str| match t {
            "/wt-new" | "/wt-reused" => made(11),
            t if t.starts_with("//") => None,
            _ => made(9),
        };
        let links = saved_links(&subtrees, |f| Some(f.to_string()), created);
        // A host's subagent started by the host's clock, which says nothing of a folder on this Mac.
        assert_eq!(links, [("/wt-c", "/wt"), ("//devbox/wt/notes", "/wt"), ("//devbox/wt/other", "/wt-reused")].map(|(c, p)| (c.to_string(), p.to_string())).into());
    }

    #[test]
    fn the_tree_asked_for_goes_even_on_a_host_while_a_hosts_subagent_goes_only_if_still_theirs() {
        let job = |tree: &str| (Removal { project: "//devbox/app".into(), tree: tree.into(), branch: None, delete_branch: false, teardown: true }, String::new());
        let jobs = vec![job("/wt-c"), job("//devbox/wt/kept"), job("//devbox/wt/theirs"), job("//devbox/wt/asked")];
        let trees: Vec<String> = still_theirs(jobs, &["//devbox/wt/theirs".to_string()]).into_iter().map(|(r, _)| r.tree).collect();
        assert_eq!(trees, ["/wt-c", "//devbox/wt/theirs", "//devbox/wt/asked"]);
    }

    #[test]
    fn a_subagents_worktree_with_someone_elses_shell_in_it_stays() {
        let removal = |tree: &str| Removal { project: "/p".into(), tree: tree.into(), branch: None, delete_branch: false, teardown: true };
        let subtrees = vec![removal("/wt/a"), removal("/wt/b"), removal("//devbox/wt/c")];
        let kept = unoccupied(subtrees, &["/wt/a/src".into(), "/wt/b-2".into(), "//devbox/wt/c".into()]);
        assert_eq!(kept, [removal("/wt/b")]);
    }

    #[test]
    fn a_hosts_leftover_that_failed_to_go_waits_for_its_host_to_come_back() {
        let mut r = Removals::default();
        r.failed.insert("//devbox/wt/a".into());
        r.failed.insert("//box2/wt/b".into());
        r.host_dropped("devbox");
        assert_eq!(r.failed, ["//box2/wt/b".to_string()].into());
    }

    #[test]
    fn a_host_not_yet_relisted_has_its_folders_wait() {
        let on = OnHosts { folders: vec![("//devbox/wt/a".into(), Some(1)), ("//box2/wt/b".into(), None)], unready: vec!["devbox".into()], ..OnHosts::default() };
        assert_eq!(on.ready().map(|(f, _)| f.as_str()).collect::<Vec<_>>(), ["//box2/wt/b"]);
    }

    #[test]
    fn a_hosts_leftovers_are_asked_about_one_sheet_at_a_time() {
        let mut r = Removals::default();
        assert!(r.ask("//devbox/wt/a"));
        assert!(!r.ask("//devbox/wt/b"), "a's sheet is still pending");
        r.asking = None;
        assert!(r.ask("//devbox/wt/b"));
    }

    #[test]
    fn a_host_is_swept_only_once_its_sessions_and_terminals_arrived_since_it_came_back() {
        let mut r = Removals::default();
        r.host_listed("devbox");
        assert!(!r.host_ready("devbox"), "its shells aren't known yet");
        r.host_terminals_listed("devbox");
        assert!(r.host_ready("devbox"));
        r.host_dropped("devbox");
        assert!(!r.host_ready("devbox"));
        r.host_terminals_listed("devbox");
        assert!(!r.host_ready("devbox"), "its sessions aren't known yet");
    }

    #[test]
    fn a_hosts_subagent_folder_the_delete_didnt_reach_waits_for_its_host() {
        let folders = vec![("//devbox/wt/notes".to_string(), Some(7)), ("//devbox/wt/gone".to_string(), Some(8)), ("//box/wt/x".to_string(), None)];
        let left = leftovers(folders, &["//devbox/wt/gone".to_string()], true);
        let wait = |f: &str, started: u64| (f.to_string(), store::Leftover { started, delete_branch: true });
        assert_eq!(left, [wait("//devbox/wt/notes", 7), wait("//box/wt/x", 0)]);
    }

    #[test]
    fn a_deleted_subagents_own_subagents_stay_reachable_from_the_parent() {
        let link = |p: &str| store::Subtree { parent: p.into(), started: 1 };
        let mut subtrees: std::collections::BTreeMap<String, store::Subtree> = [("/wt-c", link("/wt")), ("/wt-g", link("/wt-c/src")), ("/wt-o", link("/other"))].map(|(c, l)| (c.to_string(), l)).into();
        let tree_of = |f: &str| Some(f.trim_end_matches("/src").to_string());
        reparent(&mut subtrees, "/wt-c", "/wt", tree_of);
        assert_eq!(subtrees["/wt-g"].parent, "/wt");
        assert_eq!(subtrees["/wt-o"].parent, "/other");
        assert_eq!(saved_links(&subtrees, tree_of, |_| None).get("/wt-g").map(String::as_str), Some("/wt"));
    }

    #[test]
    fn subagents_go_before_their_parent_and_a_failure_keeps_the_parent() {
        let (dir, parent) = scratch("order");
        let sub = |name: &str| {
            let tree = dir.join(name);
            sh(Path::new(&parent.project), &["worktree", "add", "-q", "-b", name, tree.to_str().unwrap()]);
            Removal { tree: tree.to_string_lossy().into_owned(), branch: Some(name.into()), ..parent.clone() }
        };
        let (failing, next) = (sub("failing"), sub("next"));
        let jobs = |teardown: &str| vec![(failing.clone(), teardown.to_string()), (next.clone(), String::new()), (parent.clone(), String::new())];
        let (done, failed) = remove_in_order(jobs("exit 1"), LIMIT);
        assert!(done.is_empty());
        assert!(matches!(failed, Some((r, Failure::Teardown(_))) if r.tree == failing.tree));
        assert!([&failing, &next, &parent].iter().all(|r| Path::new(&r.tree).exists()), "the rest and the parent stay to delete again");
        let (done, failed) = remove_in_order(jobs(""), LIMIT);
        let gone: Vec<String> = done.iter().map(|(r, _)| r.tree.clone()).collect();
        assert_eq!((gone, failed), (vec![failing.tree.clone(), next.tree.clone(), parent.tree.clone()], None));
        for (_, removed) in done {
            removed.aside.inspect(|a| dispose(a, false));
        }
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_worktree_being_deleted_leaves_the_sidebar() {
        let removals = Removals { running: ["/wt".to_string()].into(), ..Default::default() };
        let shown = removals.hide(vec![tree("/p", true), tree("/wt", false), tree("/wt2", false)]);
        assert_eq!(shown, [tree("/p", true), tree("/wt2", false)]);
    }

    #[test]
    fn the_toast_names_the_worktree_being_deleted_or_counts_them() {
        let labels = [&[][..], &["/src/feat"], &["/src/feat", "/src/fix"]].map(|trees| Removals { running: trees.iter().map(|t| t.to_string()).collect(), ..Default::default() }.label());
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
    fn of_two_swept_leftovers_sharing_a_commit_only_the_first_goes_unasked() {
        let (dir, r) = scratch("swept-shared");
        sh(Path::new(&r.tree), &["commit", "-q", "--allow-empty", "-m", "b"]);
        sh(Path::new(&r.project), &["branch", "feat2", "feat"]);
        let other = dir.join("other");
        sh(Path::new(&r.project), &["worktree", "add", "-q", other.to_str().unwrap(), "feat2"]);
        let twin = Removal { tree: other.to_string_lossy().into_owned(), branch: Some("feat2".into()), ..r.clone() };
        assert_eq!((r.lost(), twin.lost()), (0, 0));
        assert!(sweep_one(r, String::new(), Duration::from_secs(5)).is_ok_and(|(_, failed)| failed.is_none()));
        assert_eq!(sweep_one(twin.clone(), String::new(), Duration::from_secs(5)).err(), Some(twin));
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

    #[test]
    fn a_hosts_leftover_settles_with_the_tree_that_held_it() {
        let left = Leftover { started: 1, delete_branch: true };
        let mut leftovers: BTreeMap<String, Leftover> = ["//devbox/w/a", "//devbox/w/a/src", "//devbox/w/a-2", "//devbox/w/b"].into_iter().map(|f| (f.to_string(), left)).collect();
        settle(&mut leftovers, "//devbox/w/a");
        assert_eq!(leftovers.keys().collect::<Vec<_>>(), ["//devbox/w/a-2", "//devbox/w/b"]);
    }

    #[test]
    fn a_deleted_subfolder_still_leads_to_the_worktree_holding_it() {
        let there = ["//devbox/home/wt", "//devbox/home"];
        let exists = |p: &str| Ok(there.contains(&p));
        assert_eq!(nearest("//devbox/home/wt/notes/src", exists).as_deref(), Some("//devbox/home/wt"));
        assert_eq!(nearest("//devbox/home/wt", exists).as_deref(), Some("//devbox/home/wt"));
        assert_eq!(nearest("//devbox/gone/deep", |_| Ok(false)).as_deref(), Some("//devbox/gone"));
        assert_eq!(nearest("//devbox/home/wt/src", |_| Err(std::io::Error::other("dropped"))), None);
    }

    #[test]
    fn a_failed_delete_keeps_what_the_lineage_of_the_trees_it_did_delete_named() {
        let lineage = [("//devbox/a".to_string(), ("//devbox/a-c".to_string(), Some(5))), ("//devbox/b".to_string(), ("//devbox/b-c".to_string(), None))];
        assert_eq!(named_by(&lineage, &["//devbox/a".to_string()]), [("//devbox/a-c".to_string(), Some(5))]);
        assert_eq!(named_by(&lineage, &[]), []);
    }

    #[test]
    fn the_trees_below_one_are_every_generation_its_saved_links_hang_under_it() {
        let links: BTreeMap<String, String> = [("/b", "/a"), ("/c", "/b"), ("/d", "/x")].into_iter().map(|(c, p)| (c.to_string(), p.to_string())).collect();
        assert_eq!(below(vec!["/a".to_string()], &links), ["/a", "/b", "/c"]);
    }
}
