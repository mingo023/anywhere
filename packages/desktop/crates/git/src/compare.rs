//! What a worktree changed against another ref: the Compare scope of the Changes panel.

use crate::{Blobs, Commit, FileStat, MAX_READ, Runner, ahead_behind, changed_files, git, git_bytes, lines, output, untracked_files};
use std::collections::HashMap;
use std::path::Path;

/// The most commits a Compare lists on each side.
const MAX_COMMITS: usize = 200;

/// What a Compare diffs: HEAD from where it left `against` (`<against>...HEAD`, as a PR shows it), or with `direct` from `against`'s tip (`<against>..HEAD`); with `working` the working tree in place of HEAD.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct Range {
    pub against: String,
    pub direct: bool,
    pub working: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Comparison {
    pub files: Vec<FileStat>,
    /// Commits HEAD has that `against` doesn't, newest first.
    pub ahead: Vec<Commit>,
    /// Commits `against` has that HEAD doesn't, newest first; read only for a direct Compare.
    pub behind: Vec<Commit>,
    /// How many commits each side has that the other doesn't: (ahead, behind).
    pub counts: (usize, usize),
}

/// The commit `range` diffs from: `against` itself when direct, else where HEAD left it, or `against` when they share no history.
fn old_side(run: &Runner, cwd: &str, range: &Range) -> String {
    let fork = if range.direct { None } else { git(run, cwd, &["merge-base", &range.against, "HEAD"]) };
    fork.map_or_else(|| range.against.clone(), |f| f.trim().to_string())
}

fn diff_args<'a>(old: &'a str, working: bool, format: &'a str) -> Vec<&'a str> {
    let mut args = vec!["diff", "-z", "-M", format, old];
    if !working {
        args.push("HEAD");
    }
    args
}

/// What `range` shows in `cwd`, untracked files only with `untracked` and `range.working`: `Ok(None)` when `range.against` names no commit, `Err` when git couldn't be asked, as on a host that isn't connected.
pub fn read(run: &Runner, cwd: &str, range: &Range, untracked: bool) -> Result<Option<Comparison>, String> {
    let found = output(run, cwd, &["rev-parse", "--verify", "--quiet", &format!("{}^{{commit}}", range.against)]).map_err(|e| e.to_string())?;
    if !found.success() {
        return Ok(None);
    }
    let old = old_side(run, cwd, range);
    let counts = numstat_z(&git(run, cwd, &diff_args(&old, range.working, "--numstat")).unwrap_or_default());
    let mut files: Vec<FileStat> = changed_files(&git(run, cwd, &diff_args(&old, range.working, "--name-status")).unwrap_or_default())
        .into_iter()
        .map(|f| {
            let (added, removed) = counts.get(&f.path).copied().unwrap_or_default();
            FileStat { path: f.path, old_path: f.old_path, added, removed, staged: false, unstaged: false, status: f.status }
        })
        .collect();
    if range.working && untracked {
        files.extend(untracked_files(run, cwd));
    }
    let behind = if range.direct { commits(run, cwd, &format!("HEAD..{}", range.against)) } else { Vec::new() };
    let ahead = commits(run, cwd, &format!("{}..HEAD", range.against));
    Ok(Some(Comparison { files, ahead, behind, counts: ahead_behind(run, cwd, &range.against) }))
}

/// `path` on each side of `range`, the old side read from its name before a rename.
pub fn blobs(run: &Runner, cwd: &str, range: &Range, path: &str) -> Blobs {
    let old = old_side(run, cwd, range);
    let show = |p: &str| git_bytes(run, cwd, &["show", &format!("{old}:{p}")]);
    let renamed_from = || changed_files(&git(run, cwd, &diff_args(&old, range.working, "--name-status")).unwrap_or_default()).into_iter().find(|f| f.path == path)?.old_path;
    let before = show(path).or_else(|| show(&renamed_from()?)).unwrap_or_default();
    let after = if range.working {
        run.read(&Path::new(cwd).join(path).to_string_lossy(), MAX_READ).unwrap_or_default()
    } else {
        git_bytes(run, cwd, &["show", &format!("HEAD:{path}")]).unwrap_or_default()
    };
    (before, after)
}

fn commits(run: &Runner, cwd: &str, range: &str) -> Vec<Commit> {
    let n = MAX_COMMITS.to_string();
    lines(git(run, cwd, &["log", "--format=%H%x00%s", "-n", &n, range, "--"]))
        .into_iter()
        .filter_map(|l| l.split_once('\0').map(|(sha, subject)| Commit { sha: sha.into(), subject: subject.into() }))
        .collect()
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
    use std::path::PathBuf;

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

    fn range(against: &str, direct: bool, working: bool) -> Range {
        Range { against: against.into(), direct, working }
    }

    fn compare(repo: &Path, range: &Range, untracked: bool) -> Comparison {
        read(&Runner::Local, repo.to_str().unwrap(), range, untracked).unwrap().unwrap()
    }

    fn files(c: &Comparison) -> Vec<(&str, char, usize, usize)> {
        c.files.iter().map(|f| (f.path.as_str(), f.status, f.added, f.removed)).collect()
    }

    fn subjects(commits: &[Commit]) -> Vec<&str> {
        commits.iter().map(|c| c.subject.as_str()).collect()
    }

    #[test]
    fn a_compare_shows_what_the_branch_changed_since_it_left_the_ref() {
        let (dir, repo) = forked("since");
        let c = compare(&repo, &range("main", false, false), true);
        assert_eq!(files(&c), [("a", 'M', 1, 1), ("b", 'A', 1, 0)]);
        assert_eq!((subjects(&c.ahead), c.behind.len(), c.counts), (vec!["feature work"], 0, (1, 1)));
        assert_eq!(c.ahead[0].sha.len(), 40, "full shas, for the commit view");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_direct_compare_diffs_the_two_tips_and_lists_what_only_the_ref_has() {
        let (dir, repo) = forked("direct");
        let c = compare(&repo, &range("main", true, false), true);
        assert_eq!(files(&c), [("a", 'M', 1, 1), ("b", 'A', 1, 0), ("c", 'D', 0, 1)]);
        assert_eq!(subjects(&c.behind), ["main work"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn including_uncommitted_takes_the_working_tree_untracked_files_too() {
        let (dir, repo) = forked("working");
        std::fs::write(repo.join("b"), "b\nmore\n").unwrap();
        std::fs::write(repo.join("new"), "n\n").unwrap();
        let paths = |working, untracked| compare(&repo, &range("main", false, working), untracked).files.into_iter().map(|f| (f.path, f.added)).collect::<Vec<_>>();
        assert_eq!(paths(false, true), [("a".to_string(), 1), ("b".into(), 1)]);
        assert_eq!(paths(true, true), [("a".to_string(), 1), ("b".into(), 2), ("new".into(), 1)]);
        assert_eq!(paths(true, false), [("a".to_string(), 1), ("b".into(), 2)]);
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
        let from_feature = range("feature", false, false);
        let c = compare(&repo, &from_feature, true);
        assert_eq!(c.files.iter().map(|f| (f.path.as_str(), f.status, f.old_path.as_deref())).collect::<Vec<_>>(), [("z", 'R', Some("a"))]);
        let text = b"one\ntwo\nthree\nfour\n".to_vec();
        assert_eq!(blobs(&Runner::Local, repo.to_str().unwrap(), &from_feature, "z"), (text.clone(), text));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn blobs_read_the_refs_side_and_head_or_the_working_tree() {
        let (dir, repo) = forked("blobs");
        let r = repo.to_str().unwrap();
        std::fs::write(repo.join("a"), "uncommitted\n").unwrap();
        assert_eq!(blobs(&Runner::Local, r, &range("main", false, false), "a"), (b"a\n".to_vec(), b"changed\n".to_vec()));
        assert_eq!(blobs(&Runner::Local, r, &range("main", false, true), "a"), (b"a\n".to_vec(), b"uncommitted\n".to_vec()));
        assert_eq!(blobs(&Runner::Local, r, &range("main", true, false), "c"), (b"c\n".to_vec(), Vec::new()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_ref_that_names_no_commit_reads_as_none_but_an_unreachable_host_as_an_error() {
        let (dir, repo) = forked("gone");
        let r = repo.to_str().unwrap();
        assert_eq!(read(&Runner::Local, r, &range("deleted-branch", false, false), true), Ok(None));
        assert!(read(&Runner::Remote(std::sync::Arc::new(NotConnected)), r, &range("main", false, false), true).is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn numstat_reads_renames_by_their_new_path_and_binaries_as_zero() {
        let counts = numstat_z("1\t2\ta\0-\t-\timg.png\x003\t0\t\0old\0new\0");
        assert_eq!((counts["a"], counts["img.png"], counts["new"]), ((1, 2), (0, 0), (3, 0)));
        assert_eq!(counts.len(), 3);
    }
}
