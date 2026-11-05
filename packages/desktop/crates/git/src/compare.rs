//! What a worktree changed against another ref: the Compare scope of the Changes panel.

use crate::{Blobs, FileStat, Runner, changed_files, git, git_bytes, output};
use std::collections::HashMap;

/// Where HEAD left `against`, so a Compare shows what a PR would; `against` itself when they share no history.
fn fork(run: &Runner, cwd: &str, against: &str) -> String {
    git(run, cwd, &["merge-base", against, "HEAD"]).map_or_else(|| against.to_string(), |f| f.trim().to_string())
}

fn diff<'a>(fork: &'a str, format: &'a str) -> [&'a str; 6] {
    ["diff", "-z", "-M", format, fork, "HEAD"]
}

/// The files HEAD changed since it left `against` (`<against>...HEAD`): `Ok(None)` when `against` names no commit, `Err` when git couldn't be asked, as on a host that isn't connected.
pub fn read(run: &Runner, cwd: &str, against: &str) -> Result<Option<Vec<FileStat>>, String> {
    let found = output(run, cwd, &["rev-parse", "--verify", "--quiet", &format!("{against}^{{commit}}")]).map_err(|e| e.to_string())?;
    if !found.success() {
        return Ok(None);
    }
    let fork = fork(run, cwd, against);
    let counts = numstat_z(&git(run, cwd, &diff(&fork, "--numstat")).unwrap_or_default());
    let files = changed_files(&git(run, cwd, &diff(&fork, "--name-status")).unwrap_or_default())
        .into_iter()
        .map(|f| {
            let (added, removed) = counts.get(&f.path).copied().unwrap_or_default();
            FileStat { path: f.path, old_path: f.old_path, added, removed, staged: false, unstaged: false, status: f.status }
        })
        .collect();
    Ok(Some(files))
}

/// `path` where HEAD left `against`, read from its name before a rename, and at HEAD.
pub fn blobs(run: &Runner, cwd: &str, against: &str, path: &str) -> Blobs {
    let fork = fork(run, cwd, against);
    let show = |rev: &str, p: &str| git_bytes(run, cwd, &["show", &format!("{rev}:{p}")]);
    let renamed_from = || changed_files(&git(run, cwd, &diff(&fork, "--name-status")).unwrap_or_default()).into_iter().find(|f| f.path == path)?.old_path;
    let before = show(&fork, path).or_else(|| show(&fork, &renamed_from()?)).unwrap_or_default();
    (before, show("HEAD", path).unwrap_or_default())
}

/// Lines added and removed by path, from `git diff -z --numstat`; a binary file counts none.
fn numstat_z(out: &str) -> HashMap<String, (usize, usize)> {
    let mut tokens = out.split('\0');
    let mut counts = HashMap::new();
    while let Some(t) = tokens.next() {
        let mut parts = t.splitn(3, '\t');
        let (Some(added), Some(removed), Some(path)) = (parts.next(), parts.next(), parts.next()) else { continue };
        // A rename leaves the path empty and gives the old then the new one as the next two tokens.
        let path = if path.is_empty() { tokens.nth(1).unwrap_or_default() } else { path };
        counts.insert(path.to_string(), (added.parse().unwrap_or(0), removed.parse().unwrap_or(0)));
    }
    counts
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::NotConnected;
    use crate::tests::{committer, scratch_repo, sh};
    use std::path::{Path, PathBuf};

    /// main with `a`, then `feature` off it changing `a` and adding `b`, while main gains `c`; on `feature`. Returns the scratch folder and the repo in it.
    fn forked(tag: &str) -> (PathBuf, PathBuf) {
        let dir = scratch_repo(&format!("compare-{tag}"));
        let repo = dir.join("repo");
        committer(&repo);
        sh(&repo, &["branch", "-M", "main"]);
        sh(&repo, &["checkout", "-qb", "feature"]);
        std::fs::write(repo.join("a"), "changed\n").unwrap();
        std::fs::write(repo.join("b"), "b\n").unwrap();
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-qm", "feature work"]);
        sh(&repo, &["checkout", "-q", "main"]);
        std::fs::write(repo.join("c"), "c\n").unwrap();
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-qm", "main work"]);
        sh(&repo, &["checkout", "-q", "feature"]);
        (dir, repo)
    }

    fn compare(repo: &Path, against: &str) -> Vec<FileStat> {
        read(&Runner::Local, repo.to_str().unwrap(), against).unwrap().unwrap()
    }

    #[test]
    fn a_compare_shows_what_the_branch_committed_since_it_left_the_ref_and_nothing_uncommitted() {
        let (dir, repo) = forked("since");
        std::fs::write(repo.join("b"), "b\nmore\n").unwrap();
        std::fs::write(repo.join("new"), "n\n").unwrap();
        let files: Vec<_> = compare(&repo, "main").into_iter().map(|f| (f.path, f.status, f.added, f.removed)).collect();
        assert_eq!(files, [("a".to_string(), 'M', 1, 1), ("b".into(), 'A', 1, 0)], "c, which only main has, isn't the branch's change");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_renamed_file_shows_its_old_path_and_diffs_from_it() {
        let (dir, repo) = forked("rename");
        std::fs::write(repo.join("a"), "one\ntwo\nthree\nfour\n").unwrap();
        sh(&repo, &["commit", "-qam", "grow a"]);
        sh(&repo, &["checkout", "-qb", "moved"]);
        sh(&repo, &["mv", "a", "z"]);
        sh(&repo, &["commit", "-qm", "rename"]);
        let files = compare(&repo, "feature");
        assert_eq!(files.iter().map(|f| (f.path.as_str(), f.status, f.old_path.as_deref())).collect::<Vec<_>>(), [("z", 'R', Some("a"))]);
        let text = b"one\ntwo\nthree\nfour\n".to_vec();
        assert_eq!(blobs(&Runner::Local, repo.to_str().unwrap(), "feature", "z"), (text.clone(), text));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn blobs_read_where_the_branch_left_the_ref_and_head() {
        let (dir, repo) = forked("blobs");
        std::fs::write(repo.join("a"), "uncommitted\n").unwrap();
        assert_eq!(blobs(&Runner::Local, repo.to_str().unwrap(), "main", "a"), (b"a\n".to_vec(), b"changed\n".to_vec()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_ref_that_names_no_commit_reads_as_none_but_an_unreachable_host_as_an_error() {
        let (dir, repo) = forked("gone");
        let r = repo.to_str().unwrap();
        assert_eq!(read(&Runner::Local, r, "deleted-branch"), Ok(None));
        assert!(read(&Runner::Remote(std::sync::Arc::new(NotConnected)), r, "main").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn numstat_reads_renames_by_their_new_path_and_binaries_as_zero() {
        let counts = numstat_z("1\t2\ta\0-\t-\timg.png\x003\t0\t\0old\0new\0");
        assert_eq!((counts["a"], counts["img.png"], counts["new"]), ((1, 2), (0, 0), (3, 0)));
        assert_eq!(counts.len(), 3);
    }
}
