use crate::desktop::Desktop;
use crate::desktop::chrome::Confirm;
use crate::util::basename;
use agents::worktrees::{Inspected, Refusal};
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Worktrees being deleted: their rows leave at once, a toast stays until they're gone, and a second delete of one is ignored.
#[derive(Default)]
pub(crate) struct Removals {
    running: HashSet<String>,
    /// The inspects sent, by request id, each waiting to show its sheet.
    asked: HashMap<String, Asked>,
    sent: HashMap<String, Removal>,
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

    /// Lets go of what waits on `host`'s pocketd (this Mac's for `None`): its link dropped, so no answer comes.
    pub(crate) fn forget(&mut self, host: Option<&str>) {
        let theirs = |request: &str| hosts::session::untag(request).0 == host;
        self.asked.retain(|request, _| !theirs(request));
        let running = &mut self.running;
        self.sent.retain(|request, r| {
            let gone = theirs(request);
            if gone {
                running.remove(&r.tree);
            }
            !gone
        });
    }
}

/// A worktree whose delete sheet waits on its pocketd's inspect; `quiet` deletes it at once, keeping its branch, when that
/// loses nothing.
pub(crate) struct Asked {
    pub(crate) project: String,
    pub(crate) tree: String,
    pub(crate) quiet: bool,
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

/// What the delete sheet says goes: uncommitted files and terminals across every tree, the subagents' trees, and the
/// commits lost if the branches go, and either way.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Inspection {
    pub(crate) dirty: usize,
    pub(crate) lost: usize,
    pub(crate) lost_anyway: usize,
    pub(crate) terminals: usize,
    pub(crate) subtrees: usize,
}

impl Inspection {
    /// Whether deleting `r` with its branch kept still loses work.
    fn loses_kept(&self, r: &Removal) -> bool {
        self.dirty > 0 || self.lost_anyway > 0 || (r.branch.is_none() && self.lost > 0)
    }
}

/// The deletion of `tree` pocketd's `reply` offers, its branch going if `delete_branch`, and what it takes.
fn inspected(project: String, tree: String, reply: &Inspected, delete_branch: bool) -> (Removal, Inspection) {
    let checked = reply.trees.first().map_or("", |t| t.branch.as_str());
    let branch = Some(checked.to_string()).filter(|b| branch_deletable(b));
    // A branch the project builds on stays, and keeps its commits.
    let lost = if checked.is_empty() || branch.is_some() { reply.lost } else { 0 };
    let takes = Inspection {
        dirty: reply.trees.iter().map(|t| t.dirty).sum(),
        lost,
        lost_anyway: reply.lost_anyway,
        terminals: reply.trees.iter().map(|t| t.terminals).sum(),
        subtrees: reply.trees.len().saturating_sub(1),
    };
    (Removal { project, tree, delete_branch: delete_branch && branch.is_some(), branch, teardown: true }, takes)
}

/// What a delete aimed at a pocketd without `worktrees.v1` says: `host`'s name, or `None` for this Mac's.
fn outdated(host: Option<&str>) -> String {
    match host {
        Some(name) => format!("Can't delete worktrees on {name}: Anywhere there is out of date. Update it on {name}, then try again."),
        None => agents::OUTDATED.into(),
    }
}

/// Whether "Delete worktree and branch" may take `branch`: not a detached tree, nor a trunk branch.
pub(crate) fn branch_deletable(branch: &str) -> bool {
    !["", "detached", "main", "master"].contains(&branch)
}

/// Purges a set-aside folder, or moves it to the Trash, then drops the folder that held it.
pub(crate) fn dispose(aside: &Path, trash: bool) {
    if !(trash && crate::util::trash(aside).is_some()) {
        std::fs::remove_dir_all(aside).ok();
    }
    if let Some(holder) = aside.parent() {
        std::fs::remove_dir(holder).ok();
    }
}

impl Desktop {
    /// Asks first, offering to delete the tree's branch too and warning about the work that would go.
    pub(crate) fn ask_delete_worktree(&mut self, project: String, tree: String, cx: &mut Context<Self>) {
        self.inspect_worktree(Asked { project, tree, quiet: false }, cx);
    }

    /// Asks the pocketd holding the tree what deleting it takes; the sheet shows once it answers.
    pub(crate) fn inspect_worktree(&mut self, asked: Asked, cx: &mut Context<Self>) {
        if self.removals.running(&asked.tree) {
            return;
        }
        let host = hosts::split(&asked.tree).0;
        if self.agents.deletes_outdated(host) {
            return self.fail(outdated(host.map(|h| self.host_name(h)).as_deref()), cx);
        }
        match self.outbox.worktree_inspect(&asked.tree) {
            Some(request) => _ = self.removals.asked.insert(request, asked),
            None => self.fail(format!("{} isn't connected", self.host_name(host.unwrap_or_default())), cx),
        }
    }

    pub(crate) fn on_inspected(&mut self, request: String, reply: Result<Inspected, String>, cx: &mut Context<Self>) {
        let Some(asked) = self.removals.asked.remove(&request) else { return };
        let reply = match reply {
            Ok(reply) => reply,
            Err(e) => return self.fail(e, cx),
        };
        let (removal, takes) = inspected(asked.project, asked.tree, &reply, self.store.worktree.delete_branch);
        if asked.quiet && !takes.loses_kept(&removal) {
            return self.delete_worktree(Removal { delete_branch: false, ..removal }, cx);
        }
        self.ask(Confirm::DeleteWorktree { removal, takes }, cx);
        cx.notify();
    }

    /// Has the pocketd holding `removal`'s tree delete it, with the worktrees its subagents worked in.
    pub(crate) fn delete_worktree(&mut self, removal: Removal, cx: &mut Context<Self>) {
        if !self.removals.start(&removal.tree) {
            return;
        }
        if self.worktree.as_deref() == Some(&removal.tree) {
            self.select_tree(removal.project.clone(), None, cx);
        }
        // pocketd refuses a timeout outside 1..3600 seconds.
        let (trash, limit) = (self.store.worktree.trash, self.store.worktree.teardown_secs.clamp(1, 3600));
        match self.outbox.worktree_delete(&removal.tree, removal.delete_branch, trash, limit, removal.teardown) {
            Some(request) => _ = self.removals.sent.insert(request, removal),
            None => {
                self.removals.running.remove(&removal.tree);
                self.fail(format!("{} isn't connected", self.host_name(hosts::split(&removal.tree).0.unwrap_or_default())), cx);
            }
        }
        cx.notify();
    }

    pub(crate) fn on_deleted(&mut self, request: String, reply: Result<(), Refusal>, cx: &mut Context<Self>) {
        let Some(removal) = self.removals.sent.remove(&request) else { return };
        self.removals.running.remove(&removal.tree);
        match reply {
            Ok(()) => self.forget_tree(&removal.tree, cx),
            Err(r) if r.code == "teardown_failed" => {
                let failed = if r.path.is_empty() { removal.tree.clone() } else { r.path };
                self.ask(Confirm::TeardownFailed { removal, failed, tail: r.detail }, cx);
            }
            Err(r) => self.fail(r.message, cx),
        }
        self.refresh_git(cx);
        cx.notify();
    }

    pub(crate) fn forget_tree(&mut self, tree: &str, cx: &mut Context<Self>) {
        for id in self.tree_terminals(tree, cx) {
            self.close_pane(&id, cx);
        }
        // Its row would show again until git and pocketd list the worktrees again.
        for trees in self.worktrees.values_mut() {
            trees.retain(|w| w.path != tree);
        }
        self.agents.unlist(tree);
        self.workspaces.update(cx, |w, _| w.remove(tree));
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
    use super::{Asked, Inspection, Removal, Removals, branch_deletable, dispose, inspected, outdated};
    use agents::worktrees::{Inspected, InspectedTree};

    fn tree(path: &str, main: bool) -> git::Worktree {
        git::Worktree { path: path.into(), branch: String::new(), main, made: false, created: None }
    }

    fn reply(branch: &str, lost: usize, lost_anyway: usize) -> Inspected {
        let tree = |branch: &str, dirty, terminals| InspectedTree { branch: branch.into(), dirty, terminals };
        Inspected { trees: vec![tree(branch, 2, 1), tree("child", 0, 2), tree("", 1, 0)], lost, lost_anyway }
    }

    #[test]
    fn the_sheet_counts_what_every_tree_the_delete_takes_holds() {
        let (removal, takes) = inspected("/src/app".into(), "/src/feat".into(), &reply("feat", 3, 1), true);
        assert_eq!(takes, Inspection { dirty: 3, lost: 3, lost_anyway: 1, terminals: 3, subtrees: 2 });
        assert_eq!(removal, Removal { project: "/src/app".into(), tree: "/src/feat".into(), branch: Some("feat".into()), delete_branch: true, teardown: true });
        assert!(!inspected("/src/app".into(), "/src/feat".into(), &reply("feat", 3, 1), false).0.delete_branch);
    }

    #[test]
    fn a_trunk_branch_stays_and_keeps_its_commits_while_a_detached_tree_loses_them() {
        let (trunk, takes) = inspected("/src/app".into(), "/src/feat".into(), &reply("master", 3, 1), true);
        assert_eq!((trunk.branch, trunk.delete_branch, takes.lost, takes.lost_anyway), (None, false, 0, 1));
        let (detached, takes) = inspected("/src/app".into(), "/src/feat".into(), &reply("", 3, 0), true);
        assert_eq!((detached.branch, detached.delete_branch, takes.lost), (None, false, 3));
    }

    #[test]
    fn a_merged_worktree_goes_unasked_only_while_keeping_its_branch_loses_nothing() {
        let removal =
            |branch: Option<&str>| Removal { project: "/p".into(), tree: "/t".into(), branch: branch.map(String::from), delete_branch: false, teardown: true };
        let takes = |dirty, lost, lost_anyway| Inspection { dirty, lost, lost_anyway, ..Default::default() };
        assert!(!takes(0, 3, 0).loses_kept(&removal(Some("feat"))), "the kept branch holds its commits");
        assert!(takes(1, 0, 0).loses_kept(&removal(Some("feat"))));
        assert!(takes(0, 0, 2).loses_kept(&removal(Some("feat"))), "a detached subagent tree's commits go either way");
        assert!(takes(0, 3, 0).loses_kept(&removal(None)), "a detached tree's commits go with it");
    }

    #[test]
    fn a_delete_on_a_pocketd_too_old_to_do_it_names_its_host() {
        assert_eq!(outdated(Some("devbox")), "Can't delete worktrees on devbox: Anywhere there is out of date. Update it on devbox, then try again.");
        assert_eq!(outdated(None), agents::OUTDATED);
    }

    #[test]
    fn what_waits_on_a_pocketd_whose_link_dropped_is_let_go() {
        let removal = |tree: &str| Removal { project: "/p".into(), tree: tree.into(), branch: None, delete_branch: false, teardown: true };
        let asked = |tree: &str| Asked { project: "/p".into(), tree: tree.into(), quiet: false };
        let mut removals = Removals {
            running: ["/w/a", "//devbox/w/b"].map(String::from).into(),
            asked: [("wtinspect-1".to_string(), asked("/w/c")), ("devbox/wtinspect-2".to_string(), asked("//devbox/w/d"))].into(),
            sent: [("wtdelete-3".to_string(), removal("/w/a")), ("devbox/wtdelete-4".to_string(), removal("//devbox/w/b"))].into(),
        };
        removals.forget(Some("devbox"));
        assert_eq!((removals.running("/w/a"), removals.running("//devbox/w/b")), (true, false));
        assert_eq!(removals.asked.keys().collect::<Vec<_>>(), ["wtinspect-1"]);
        assert_eq!(removals.sent.keys().collect::<Vec<_>>(), ["wtdelete-3"]);
    }

    #[test]
    fn a_worktree_being_deleted_leaves_the_sidebar() {
        let removals = Removals { running: ["/wt".to_string()].into(), ..Default::default() };
        let shown = removals.hide(vec![tree("/p", true), tree("/wt", false), tree("/wt2", false)]);
        assert_eq!(shown, [tree("/p", true), tree("/wt2", false)]);
    }

    #[test]
    fn the_toast_names_the_worktree_being_deleted_or_counts_them() {
        let labels = [&[][..], &["/src/feat"], &["/src/feat", "/src/fix"]]
            .map(|trees| Removals { running: trees.iter().map(|t| t.to_string()).collect(), ..Default::default() }.label());
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
    fn a_disposed_folder_goes_with_the_folder_that_held_it() {
        let holder = std::env::temp_dir().join(format!("pocket-dispose-{}", std::process::id()));
        let aside = holder.join("feat");
        std::fs::create_dir_all(&aside).unwrap();
        std::fs::write(aside.join("big"), "x").unwrap();
        dispose(&aside, false);
        assert!(!holder.exists());
    }
}
