//! Placing a packed worktree on the host: send its bundle, make a worktree on its branch, and lay the uncommitted
//! changes back on it as changes, not commits.

use crate::checks::basename;
use crate::pack::Pack;
use crate::{Host, run};

/// Sends `pack`'s bundle to the host for handoff `id`; where it landed.
pub fn send(host: &dyn Host, pack: &Pack, id: &str) -> Result<String, String> {
    let dir = format!("{}/.anywhere/handoff", host.home().map_err(|e| e.to_string())?);
    host.mkdir(&dir).map_err(|e| e.to_string())?;
    let path = format!("{dir}/{id}.bundle");
    host.write(&path, &pack.bundle).map_err(|e| e.to_string())?;
    Ok(path)
}

fn free_path(host: &dyn Host, root: &str, name: &str) -> Result<String, String> {
    for n in 1..100 {
        let path = if n == 1 { format!("{root}/{name}") } else { format!("{root}/{name}-{n}") };
        if host.size(&path).map_err(|e| e.to_string())?.is_none() {
            return Ok(path);
        }
    }
    Err(format!("{root} has no free folder for {name}"))
}

/// A worktree made on the host, and what making it added there.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    /// Its real path; empty until `git worktree add` made it.
    pub path: String,
    pub project: String,
    pub branch: String,
    /// The branch wasn't on the host before.
    pub made_branch: bool,
}

/// Makes the worktree on the host from the bundle at `bundle`, in `project`, named `name` like this Mac's. A host
/// missing commits the bundle builds on fetches origin once and tries again. A failure takes back what it added.
pub fn worktree(host: &dyn Host, project: &str, name: &str, bundle: &str, pack: &Pack) -> Result<Placed, String> {
    if run(host, project, &["git", "bundle", "verify", "-q", bundle]).is_err() {
        _ = run(host, project, &["git", "fetch", "-q", "origin"]);
        run(host, project, &["git", "bundle", "verify", "-q", bundle]).map_err(|e| format!("The host is missing commits this code builds on: {e}"))?;
    }
    let head = format!("refs/heads/{}", pack.branch);
    let made_branch = run(host, project, &["git", "rev-parse", "-q", "--verify", &head]).is_err();
    let mut placed = Placed { path: String::new(), project: project.into(), branch: pack.branch.clone(), made_branch };
    let made = run(host, project, &["git", "fetch", "-q", bundle, &format!("{head}:{head}"), &format!("+{}:{}", pack.carry, pack.carry)])
        .and_then(|_| add(host, name, pack, &mut placed));
    _ = run(host, project, &["git", "update-ref", "-d", &pack.carry]);
    _ = host.remove(bundle);
    match made {
        Ok(path) => Ok(Placed { path, ..placed }),
        Err(e) => {
            undo(host, &placed);
            Err(e)
        }
    }
}

/// Adds the worktree, noting its path in `placed` as soon as it exists, and returns its real path.
fn add(host: &dyn Host, name: &str, pack: &Pack, placed: &mut Placed) -> Result<String, String> {
    let home = host.home().map_err(|e| e.to_string())?;
    let root = format!("{home}/.anywhere/worktrees/{}", basename(&placed.project));
    host.mkdir(&root).map_err(|e| e.to_string())?;
    let path = free_path(host, &root, name)?;
    run(host, &placed.project, &["git", "worktree", "add", "-q", &path, &pack.branch])?;
    placed.path = path.clone();
    // pocketd lists a worktree in its folder as one Anywhere made once this marker is in its git dir.
    let gitdir = run(host, &path, &["git", "rev-parse", "--absolute-git-dir"])?;
    host.write(&format!("{gitdir}/anywhere"), b"").map_err(|e| e.to_string())?;
    if pack.wip.is_some() {
        run(host, &path, &["git", "reset", "-q", "--hard", &pack.carry])?;
        run(host, &path, &["git", "reset", "-q", &pack.tip])?;
    }
    run(host, &path, &["pwd", "-P"])
}

/// Takes back what `worktree` added on the host, so a retry, or the next check, finds it as it was.
pub fn undo(host: &dyn Host, p: &Placed) {
    if !p.path.is_empty() {
        _ = run(host, &p.project, &["git", "worktree", "remove", "--force", &p.path]);
    }
    if p.made_branch {
        _ = run(host, &p.project, &["git", "branch", "-D", &p.branch]);
    }
}

#[cfg(test)]
mod tests {
    use super::{send, undo, worktree};
    use crate::pack::pack;
    use crate::testing::{Folder, git, repos};
    use std::path::Path;

    #[test]
    #[cfg_attr(windows, ignore = "the stand-in host runs Git Bash, whose paths are MSYS's, not a unix host's")]
    fn the_host_worktree_has_the_branch_and_the_changes_left_uncommitted() {
        let (_root, mac, home) = repos("place", "feat");
        let dir = Path::new(&mac);
        std::fs::write(dir.join("a.txt"), "three\n").unwrap();
        std::fs::write(dir.join("new.txt"), "new\n").unwrap();
        let p = pack(&mac, "h1").unwrap();
        let host = Folder { home: home.clone() };
        let bundle = send(&host, &p, "h1").unwrap();
        let project = format!("{home}/code/app");
        let placed = worktree(&host, &project, "feat", &bundle, &p).unwrap();
        let path = placed.path.clone();
        assert_eq!(path, format!("{home}/.anywhere/worktrees/app/feat"));
        assert!(placed.made_branch);
        let tree = Path::new(&path);
        assert_eq!(git(tree, &["rev-parse", "HEAD"]), p.tip, "the branch is at this Mac's commit, not the wip one");
        assert_eq!(git(tree, &["symbolic-ref", "--short", "HEAD"]), "feat");
        assert_eq!(std::fs::read_to_string(tree.join("a.txt")).unwrap(), "three\n");
        assert_eq!(git(tree, &["status", "--porcelain"]), " M a.txt\n?? new.txt".trim());
        let gitdir = git(tree, &["rev-parse", "--absolute-git-dir"]);
        assert!(Path::new(&gitdir).join("anywhere").exists());
        assert!(!Path::new(&bundle).exists());
        assert_eq!(git(Path::new(&project), &["for-each-ref", "refs/anywhere"]), "");
    }

    #[test]
    fn a_taken_folder_name_gets_a_number() {
        let (_root, mac, home) = repos("taken", "feat");
        std::fs::create_dir_all(format!("{home}/.anywhere/worktrees/app/feat")).unwrap();
        let p = pack(&mac, "t1").unwrap();
        let host = Folder { home: home.clone() };
        let bundle = send(&host, &p, "t1").unwrap();
        let path = worktree(&host, &format!("{home}/code/app"), "feat", &bundle, &p).unwrap().path;
        assert!(path.ends_with("/feat-2"), "{path}");
    }

    #[test]
    #[cfg_attr(windows, ignore = "the stand-in host runs Git Bash, whose paths are MSYS's, not a unix host's")]
    fn a_host_behind_origin_fetches_what_the_bundle_builds_on() {
        let (root, mac, home) = repos("behind", "feat");
        let dir = Path::new(&mac);
        git(dir, &["checkout", "-q", "main"]);
        std::fs::write(dir.join("m.txt"), "m").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-qm", "main moved"]);
        git(dir, &["push", "-q", "origin", "main"]);
        git(dir, &["checkout", "-qb", "late"]);
        std::fs::write(dir.join("l.txt"), "l").unwrap();
        git(dir, &["add", "."]);
        git(dir, &["commit", "-qm", "late"]);
        let p = pack(&mac, "b1").unwrap();
        let host = Folder { home: home.clone() };
        let bundle = send(&host, &p, "b1").unwrap();
        let path = worktree(&host, &format!("{home}/code/app"), "late", &bundle, &p).unwrap().path;
        assert!(Path::new(&path).join("m.txt").exists());
        drop(root);
    }

    #[test]
    #[cfg_attr(windows, ignore = "the stand-in host runs Git Bash, whose paths are MSYS's, not a unix host's")]
    fn undoing_a_worktree_leaves_the_host_as_it_was() {
        let (_root, mac, home) = repos("undo", "feat");
        let p = pack(&mac, "u1").unwrap();
        let host = Folder { home: home.clone() };
        let bundle = send(&host, &p, "u1").unwrap();
        let project = format!("{home}/code/app");
        let placed = worktree(&host, &project, "feat", &bundle, &p).unwrap();
        undo(&host, &placed);
        assert!(!Path::new(&placed.path).exists());
        assert_eq!(git(Path::new(&project), &["branch", "--list", "feat"]), "");
    }

    #[test]
    fn a_worktree_that_fails_to_open_takes_back_its_branch() {
        let (_root, mac, home) = repos("fails", "feat");
        let p = pack(&mac, "f1").unwrap();
        let host = Folder { home: home.clone() };
        let bundle = send(&host, &p, "f1").unwrap();
        std::fs::write(format!("{home}/.anywhere/worktrees"), "a file where the folder goes").unwrap();
        let project = format!("{home}/code/app");
        assert!(worktree(&host, &project, "feat", &bundle, &p).is_err());
        assert_eq!(git(Path::new(&project), &["branch", "--list", "feat"]), "");
    }
}
