use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use crate::util::basename;
use gpui_kit::*;
use std::collections::HashSet;
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
    fn lost(&self) -> usize {
        match (&self.branch, self.delete_branch) {
            (None, _) => git::lost_commits(&self.tree, None),
            (Some(b), true) => git::lost_commits(&self.project, Some(b)),
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

/// Whether "Delete worktree and branch" may take `branch`: not a detached tree, nor a branch the project builds on.
pub(crate) fn branch_deletable(branch: &str, base: &str) -> bool {
    !["", "detached", "main", "master", base].contains(&branch)
}

/// Moves `tree` into a folder of its own in the temp folder, an instant rename where deleting a `target/` takes seconds.
/// It keeps its name there, so it reaches the Trash under it. `None` when it can't, e.g. across volumes: git then deletes it in place.
fn set_aside(tree: &str) -> Option<PathBuf> {
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
fn dispose(aside: &Path, trash: bool) {
    if !(trash && crate::util::trash(aside)) {
        std::fs::remove_dir_all(aside).ok();
    }
    if let Some(holder) = aside.parent() {
        std::fs::remove_dir(holder).ok();
    }
}

/// Teardown goes first: scripts like `docker compose down` need the folder.
fn run(r: &Removal, teardown: &str, limit: Duration) -> Result<Removed, Failure> {
    if r.teardown && !teardown.is_empty() && Path::new(&r.tree).is_dir() {
        daemon::run_script(teardown, &r.tree, limit).map_err(Failure::Teardown)?;
    }
    let aside = set_aside(&r.tree);
    if let Err(e) = git::remove_worktree(&r.project, &r.tree) {
        if let Some(aside) = &aside {
            std::fs::rename(aside, &r.tree).ok();
        }
        return Err(Failure::Remove(e));
    }
    let kept = r.branch.as_deref().filter(|_| r.delete_branch).and_then(|b| git::delete_branch(&r.project, b).err().map(|e| (b.to_string(), e)));
    Ok(Removed { aside, kept })
}

impl Desktop {
    /// Asks first, offering to delete the tree's branch too and warning about the work that would go.
    pub(crate) fn ask_delete_worktree(&mut self, project: String, tree: String, cx: &mut Context<Self>) {
        if self.removals.running(&tree) {
            return;
        }
        let base = self.store.repos.get(&project).map_or("", |r| r.base.as_str());
        let branch = self.worktrees.get(&project).into_iter().flatten().find(|w| w.path == tree).map(|w| w.branch.clone()).filter(|b| branch_deletable(b, base));
        let removal = Removal { project, tree, delete_branch: branch.is_some() && self.store.worktree.delete_branch, branch, teardown: true };
        let job = removal.clone();
        let task = cx.background_executor().spawn(async move { (git::read(&job.tree, true).map_or(0, |r| r.files.len()), job.lost()) });
        cx.spawn(async move |this, cx| {
            let (dirty, lost) = task.await;
            this.update(cx, |d, cx| {
                d.confirm = Some(Confirm::DeleteWorktree { removal, dirty, lost });
                d.overlay = Some(Overlay::Confirm);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn delete_worktree(&mut self, removal: Removal, cx: &mut Context<Self>) {
        if !self.removals.start(&removal.tree) {
            return;
        }
        if self.worktree.as_deref() == Some(&removal.tree) {
            self.select_tree(removal.project.clone(), None, cx);
        }
        let teardown = self.store.repos.get(&removal.project).map(|r| r.teardown.clone()).unwrap_or_default();
        let (limit, trash) = (Duration::from_secs(self.store.worktree.teardown_secs.into()), self.store.worktree.trash);
        let job = removal.clone();
        let task = cx.background_executor().spawn(async move { run(&job, &teardown, limit) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                d.removals.running.remove(&removal.tree);
                match res {
                    Err(Failure::Teardown(tail)) => {
                        d.confirm = Some(Confirm::TeardownFailed { removal, tail });
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

    fn forget_tree(&mut self, tree: &str, cx: &mut Context<Self>) {
        for id in self.tree_terminals(tree) {
            self.close_pane(&id, cx);
        }
        // Its row would show again until the git refresh lands.
        for trees in self.worktrees.values_mut() {
            trees.retain(|w| w.path != tree);
        }
        self.workspaces.remove(tree);
        self.prs.forget(tree);
        self.store.layouts.remove(tree);
        self.store.untrack(tree);
        self.save_soon(cx);
        if self.worktree.as_deref() == Some(tree) {
            self.worktree = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Failure, Removal, Removals, Removed, branch_deletable, dispose, run};
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
        let dir = std::env::temp_dir().join(format!("pocket-removal-{tag}-{}", std::process::id()));
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
        git::Worktree { path: path.into(), branch: String::new(), main }
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
    fn only_a_branch_the_project_does_not_build_on_can_go_with_its_worktree() {
        assert!(branch_deletable("feat", "dev"));
        for kept in ["dev", "main", "master", "detached", ""] {
            assert!(!branch_deletable(kept, "dev"), "{kept}");
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
    fn deletes_the_worktree_then_its_branch() {
        let (dir, r) = scratch("both");
        let removed = run(&r, "", LIMIT).unwrap();
        dispose(&removed.aside.unwrap(), false);
        assert!(!Path::new(&r.tree).exists());
        assert!(!git::branches(&r.project).contains(&"feat".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_folder_is_set_aside_so_its_files_are_purged_after_the_worktree_is_gone() {
        let (dir, r) = scratch("aside");
        std::fs::write(Path::new(&r.tree).join("big"), "x").unwrap();
        let aside = run(&r, "", LIMIT).unwrap().aside.unwrap();
        assert!(!Path::new(&r.tree).exists());
        assert_eq!(git::worktrees(&r.project).len(), 1);
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
        let r = Removal { branch: Some("gone".into()), ..r };
        let removed = run(&r, "", LIMIT).unwrap();
        assert_eq!(removed.kept.map(|(b, _)| b).as_deref(), Some("gone"));
        dispose(&removed.aside.unwrap(), false);
        assert!(!Path::new(&r.tree).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_teardown_deletes_nothing() {
        let (dir, r) = scratch("teardown");
        assert!(matches!(run(&r, "echo nope; exit 3", LIMIT), Err(Failure::Teardown(tail)) if tail.ends_with("nope")));
        assert!(Path::new(&r.tree).exists());
        assert!(git::branches(&r.project).contains(&"feat".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn teardown_is_skipped_once_the_folder_is_gone() {
        let (dir, r) = scratch("gone");
        std::fs::remove_dir_all(&r.tree).unwrap();
        assert_eq!(run(&Removal { delete_branch: false, ..r.clone() }, "exit 1", LIMIT), Ok(Removed { aside: None, kept: None }));
        assert_eq!(git::worktrees(&r.project).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
