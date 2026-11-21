//! Packing a worktree on this Mac into one git bundle: its branch, plus a commit of everything not committed.

use std::path::Path;
use std::process::Command;

/// A worktree packed to move.
#[derive(Clone, Debug)]
pub struct Pack {
    pub branch: String,
    /// The branch's commit.
    pub tip: String,
    /// A commit on `tip` holding every uncommitted change, untracked files included; `None` when there were none.
    pub wip: Option<String>,
    /// The ref in `bundle` that holds `wip`, else `tip`.
    pub carry: String,
    pub bundle: Vec<u8>,
}

/// Where a handoff's commit is kept while it moves, on both machines.
pub fn carry_ref(id: &str) -> String {
    format!("refs/anywhere/handoff/{id}")
}

fn git(cwd: &str, env: &[(&str, &str)], args: &[&str]) -> Result<String, String> {
    let mut cmd = Command::new("git");
    cmd.args(["-c", "user.name=Anywhere", "-c", "user.email=handoff@anywhere.local"]).args(args).current_dir(cwd);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(String::from_utf8_lossy(&out.stdout).trim().to_string()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// The branch `cwd` is on, its commit, and the commit of its uncommitted changes, made through a scratch index so
/// the worktree and its own index stay as they are.
fn snapshot(cwd: &str, scratch: &Path) -> Result<(String, String, Option<String>), String> {
    let branch = git(cwd, &[], &["symbolic-ref", "-q", "--short", "HEAD"]).map_err(|_| "This worktree isn’t on a branch".to_string())?;
    let tip = git(cwd, &[], &["rev-parse", "HEAD"])?;
    let index = scratch.join("index");
    let env = [("GIT_INDEX_FILE", index.to_str().unwrap_or_default())];
    git(cwd, &env, &["read-tree", "HEAD"])?;
    git(cwd, &env, &["add", "-A"])?;
    let tree = git(cwd, &env, &["write-tree"])?;
    if tree == git(cwd, &[], &["rev-parse", "HEAD^{tree}"])? {
        return Ok((branch, tip, None));
    }
    let wip = git(cwd, &[], &["commit-tree", &tree, "-p", &tip, "-m", "Anywhere handoff: uncommitted changes"])?;
    Ok((branch, tip, Some(wip)))
}

/// Packs the worktree at `cwd` for handoff `id`. The bundle leaves out what origin already has, but always holds
/// the branch's last commit.
pub fn pack(cwd: &str, id: &str) -> Result<Pack, String> {
    let scratch = std::env::temp_dir().join(format!("anywhere-handoff-{id}"));
    std::fs::create_dir_all(&scratch).map_err(|e| e.to_string())?;
    let packed = bundle(cwd, id, &scratch);
    _ = std::fs::remove_dir_all(&scratch);
    packed
}

fn bundle(cwd: &str, id: &str, scratch: &Path) -> Result<Pack, String> {
    let (branch, tip, wip) = snapshot(cwd, scratch)?;
    let carry = carry_ref(id);
    git(cwd, &[], &["update-ref", &carry, wip.as_deref().unwrap_or(&tip)])?;
    let file = scratch.join("bundle");
    let path = file.to_str().unwrap_or_default();
    let head = format!("refs/heads/{branch}");
    // Git drops a ref the exclusions reach from the bundle, so a tip origin already has keeps its own commit and
    // leaves out only its parents. Other remotes may hold commits the host's clone of origin lacks.
    let on_origin = git(cwd, &[], &["rev-list", "--count", &tip, "--not", "--remotes=origin"]).is_ok_and(|n| n == "0");
    let leave_out = if on_origin { format!("{tip}^@") } else { "--remotes=origin".to_string() };
    let made = git(cwd, &[], &["bundle", "create", "-q", path, &head, &carry, "--not", &leave_out]);
    _ = git(cwd, &[], &["update-ref", "-d", &carry]);
    made?;
    let bundle = std::fs::read(&file).map_err(|e| e.to_string())?;
    Ok(Pack { branch, tip, wip, carry, bundle })
}

/// Whether `oid` is a commit `cwd`'s HEAD already holds, so moving the branch there to HEAD loses nothing.
pub fn holds(cwd: &str, oid: &str) -> bool {
    git(cwd, &[], &["merge-base", "--is-ancestor", oid, "HEAD"]).is_ok()
}

/// How many files in `cwd` differ from HEAD, untracked ones included.
pub fn changed_files(cwd: &str) -> usize {
    git(cwd, &[], &["status", "--porcelain"]).map_or(0, |s| s.lines().filter(|l| !l.is_empty()).count())
}

/// How many commits on `cwd`'s HEAD no remote branch has.
pub fn unpushed(cwd: &str) -> usize {
    git(cwd, &[], &["rev-list", "--count", "HEAD", "--not", "--remotes"]).ok().and_then(|n| n.parse().ok()).unwrap_or(0)
}

/// `cwd`'s origin, if it has one.
pub fn origin(cwd: &str) -> Option<String> {
    git(cwd, &[], &["remote", "get-url", "origin"]).ok().filter(|u| !u.is_empty())
}

#[cfg(test)]
mod tests {
    use super::{changed_files, holds, pack, unpushed};
    use crate::testing::{git, repos};
    use std::path::Path;

    #[test]
    fn a_clean_worktree_packs_its_branch_without_a_wip_commit() {
        let (_root, mac, _) = repos("clean", "feat");
        let p = pack(&mac, "c1").unwrap();
        assert_eq!(p.branch, "feat");
        assert_eq!(p.wip, None);
        assert!(!p.bundle.is_empty());
        assert_eq!(unpushed(&mac), 1);
        assert_eq!(git(Path::new(&mac), &["for-each-ref", "refs/anywhere"]), "", "the carry ref is gone once bundled");
    }

    #[test]
    fn uncommitted_and_untracked_changes_ride_in_a_commit_and_the_worktree_is_untouched() {
        let (_root, mac, _) = repos("dirty", "feat");
        let dir = Path::new(&mac);
        std::fs::write(dir.join("a.txt"), "three\n").unwrap();
        std::fs::write(dir.join("new.txt"), "new\n").unwrap();
        git(dir, &["add", "a.txt"]);
        let before = git(dir, &["status", "--porcelain"]);
        let p = pack(&mac, "d1").unwrap();
        let wip = p.wip.clone().unwrap();
        assert_eq!(git(dir, &["show", &format!("{wip}:new.txt")]), "new");
        assert_eq!(git(dir, &["show", &format!("{wip}:a.txt")]), "three");
        assert_eq!(git(dir, &["status", "--porcelain"]), before, "the index and files stay as they were");
        assert_eq!(changed_files(&mac), 2);
    }

    #[test]
    fn a_branch_already_on_origin_still_packs_its_last_commit() {
        let (_root, mac, _) = repos("pushed", "feat");
        git(Path::new(&mac), &["push", "-q", "origin", "feat"]);
        assert!(!pack(&mac, "p1").unwrap().bundle.is_empty());
    }

    #[test]
    fn a_pushed_branch_with_uncommitted_changes_still_carries_its_branch() {
        let (root, mac, _) = repos("pushed-dirty", "feat");
        let dir = Path::new(&mac);
        git(dir, &["push", "-q", "origin", "feat"]);
        std::fs::write(dir.join("a.txt"), "three\n").unwrap();
        let file = root.join("pd1.bundle");
        std::fs::write(&file, pack(&mac, "pd1").unwrap().bundle).unwrap();
        let heads = git(dir, &["bundle", "list-heads", file.to_str().unwrap()]);
        assert!(heads.contains("refs/heads/feat"), "{heads}");
    }

    #[test]
    fn a_detached_worktree_does_not_pack() {
        let (_root, mac, _) = repos("detached", "feat");
        git(Path::new(&mac), &["checkout", "-q", "--detach"]);
        assert!(pack(&mac, "x1").unwrap_err().contains("isn’t on a branch"));
    }

    #[test]
    fn head_holds_its_own_history_but_not_a_stranger() {
        let (_root, mac, _) = repos("holds", "feat");
        let first = git(Path::new(&mac), &["rev-parse", "main"]);
        assert!(holds(&mac, &first));
        assert!(!holds(&mac, "0123456789012345678901234567890123456789"));
    }
}
