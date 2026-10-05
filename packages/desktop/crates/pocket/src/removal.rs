use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use gpui_kit::*;
use std::collections::HashSet;
use std::path::Path;
use std::time::Duration;

const TEARDOWN_LIMIT: Duration = Duration::from_secs(120);

/// Worktrees being deleted: their rows say so, and a second delete of one is ignored.
#[derive(Default)]
pub(crate) struct Removals {
    running: HashSet<String>,
}

impl Removals {
    pub(crate) fn running(&self, tree: &str) -> bool {
        self.running.contains(tree)
    }
}

/// A worktree to delete, and what goes with it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Removal {
    pub(crate) project: String,
    pub(crate) tree: String,
    /// `None` when the tree is detached.
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
    /// The worktree is gone but this branch stayed, for this reason.
    Branch(String, String),
}

fn needs_confirm(dirty: usize, terminals: usize, lost: usize) -> bool {
    dirty + terminals + lost > 0
}

/// Whether "Delete worktree and branch" may take `branch`: not a detached tree, nor a branch the project builds on.
pub(crate) fn branch_deletable(branch: &str, base: &str) -> bool {
    !["", "detached", "main", "master", base].contains(&branch)
}

/// Teardown goes first: scripts like `docker compose down` need the folder.
fn run(r: &Removal, teardown: &str) -> Result<(), Failure> {
    if r.teardown && !teardown.is_empty() && Path::new(&r.tree).is_dir() {
        daemon::run_script(teardown, &r.tree, TEARDOWN_LIMIT).map_err(Failure::Teardown)?;
    }
    git::remove_worktree(&r.project, &r.tree).map_err(Failure::Remove)?;
    match r.branch.as_deref().filter(|_| r.delete_branch) {
        Some(b) => git::delete_branch(&r.project, b).map_err(|e| Failure::Branch(b.to_string(), e)),
        None => Ok(()),
    }
}

impl Desktop {
    /// Asks first when deleting would close terminals or lose uncommitted changes or commits.
    pub(crate) fn ask_delete_worktree(&mut self, project: String, tree: String, delete_branch: bool, cx: &mut Context<Self>) {
        if self.removals.running(&tree) {
            return;
        }
        let branch = self.worktrees.get(&project).into_iter().flatten().find(|w| w.path == tree).map(|w| w.branch.clone()).filter(|b| !b.is_empty() && b != "detached");
        let removal = Removal { project, tree, branch, delete_branch, teardown: true };
        let job = removal.clone();
        let task = cx.background_executor().spawn(async move { (git::read(&job.tree).map(|r| r.files.len()), job.lost()) });
        cx.spawn(async move |this, cx| {
            let (dirty, lost) = task.await;
            this.update(cx, |d, cx| {
                // An unreadable tree may still hold work: ask.
                if dirty.is_none_or(|n| needs_confirm(n, d.tree_terminals(&removal.tree).len(), lost)) {
                    d.confirm = Some(Confirm::DeleteWorktree { removal, dirty: dirty.unwrap_or(0), lost });
                    d.overlay = Some(Overlay::Confirm);
                } else {
                    d.delete_worktree(removal, cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn delete_worktree(&mut self, removal: Removal, cx: &mut Context<Self>) {
        if !self.removals.running.insert(removal.tree.clone()) {
            return;
        }
        let teardown = self.store.repos.get(&removal.project).map(|r| r.teardown.clone()).unwrap_or_default();
        let job = removal.clone();
        let task = cx.background_executor().spawn(async move { run(&job, &teardown) });
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
                    Err(Failure::Branch(branch, e)) => {
                        d.error = Some(format!("Deleted the worktree; kept branch {branch}: {e}"));
                        d.forget_tree(&removal.tree, cx);
                    }
                    Ok(()) => d.forget_tree(&removal.tree, cx),
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
        self.workspaces.remove(tree);
        self.prs.forget(tree);
        self.store.layouts.remove(tree);
        self.save_soon(cx);
        if self.worktree.as_deref() == Some(tree) {
            (self.worktree, self.session) = (None, None);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Failure, Removal, branch_deletable, needs_confirm, run};
    use std::path::{Path, PathBuf};
    use std::process::Command;

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

    #[test]
    fn deleting_asks_only_when_it_would_lose_work_or_close_terminals() {
        assert!(!needs_confirm(0, 0, 0));
        assert!(needs_confirm(2, 0, 0));
        assert!(needs_confirm(0, 1, 0));
        assert!(needs_confirm(0, 0, 3));
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
        assert_eq!(run(&r, ""), Ok(()));
        assert!(!Path::new(&r.tree).exists());
        assert!(!git::branches(&r.project).contains(&"feat".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_branch_that_cannot_be_deleted_leaves_the_worktree_deleted() {
        let (dir, r) = scratch("kept");
        let r = Removal { branch: Some("gone".into()), ..r };
        assert!(matches!(run(&r, ""), Err(Failure::Branch(b, _)) if b == "gone"));
        assert!(!Path::new(&r.tree).exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_failed_teardown_deletes_nothing() {
        let (dir, r) = scratch("teardown");
        assert!(matches!(run(&r, "echo nope; exit 3"), Err(Failure::Teardown(tail)) if tail.ends_with("nope")));
        assert!(Path::new(&r.tree).exists());
        assert!(git::branches(&r.project).contains(&"feat".to_string()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn teardown_is_skipped_once_the_folder_is_gone() {
        let (dir, r) = scratch("gone");
        std::fs::remove_dir_all(&r.tree).unwrap();
        assert_eq!(run(&Removal { delete_branch: false, ..r.clone() }, "exit 1"), Ok(()));
        assert_eq!(git::worktrees(&r.project).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
