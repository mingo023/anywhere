use std::process::Command;

#[derive(Clone, Debug, PartialEq)]
pub struct FileStat {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    pub staged: bool,
    /// Git's status letter: M, A or D.
    pub status: char,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Repo {
    pub branch: String,
    pub base: Option<String>,
    pub ahead: usize,
    pub behind: usize,
    pub files: Vec<FileStat>,
    pub commits: Vec<Commit>,
}

impl Repo {
    pub fn totals(&self) -> (usize, usize) {
        self.files.iter().fold((0, 0), |(a, r), f| (a + f.added, r + f.removed))
    }
}

fn git(cwd: &str, args: &[&str]) -> Option<String> {
    let out = Command::new("git").arg("-C").arg(cwd).args(args).output().ok()?;
    out.status.success().then(|| String::from_utf8_lossy(&out.stdout).into_owned())
}

fn lines(s: Option<String>) -> Vec<String> {
    s.unwrap_or_default().lines().filter(|l| !l.is_empty()).map(str::to_string).collect()
}

fn numstat(out: &str) -> Vec<(String, usize, usize)> {
    out.lines()
        .filter_map(|l| {
            let mut parts = l.splitn(3, '\t');
            let added = parts.next()?.parse().unwrap_or(0);
            let removed = parts.next()?.parse().unwrap_or(0);
            Some((parts.next()?.to_string(), added, removed))
        })
        .collect()
}

pub fn read(cwd: &str) -> Option<Repo> {
    let branch = git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"])?.trim().to_string();
    let staged = lines(git(cwd, &["diff", "--cached", "--name-only"]));
    let statuses = name_status(&git(cwd, &["diff", "HEAD", "--name-status"]).unwrap_or_default());
    let mut files: Vec<FileStat> = numstat(&git(cwd, &["diff", "HEAD", "--numstat"]).unwrap_or_default())
        .into_iter()
        .map(|(path, added, removed)| {
            let status = statuses.iter().find(|(p, _)| *p == path).map_or('M', |(_, s)| *s);
            FileStat { staged: staged.contains(&path), path, added, removed, status }
        })
        .collect();
    for path in lines(git(cwd, &["ls-files", "--others", "--exclude-standard"])) {
        let added = std::fs::read_to_string(std::path::Path::new(cwd).join(&path)).map(|s| s.lines().count()).unwrap_or(0);
        files.push(FileStat { path, added, removed: 0, staged: false, status: 'A' });
    }
    let base = if branch == "main" || branch == "master" {
        git(cwd, &["rev-parse", "--abbrev-ref", "@{u}"]).map(|s| s.trim().to_string())
    } else {
        ["main", "master"].into_iter().find(|b| git(cwd, &["rev-parse", "--verify", "--quiet", b]).is_some()).map(str::to_string)
    };
    let (ahead, behind, commits) = match &base {
        Some(b) => {
            let range = format!("{b}..HEAD");
            let (ahead, behind) = ahead_behind(cwd, b);
            let commits = lines(git(cwd, &["log", "--format=%h\t%s", "-n", "20", &range]))
                .into_iter()
                .filter_map(|l| l.split_once('\t').map(|(sha, subject)| Commit { sha: sha.into(), subject: subject.into() }))
                .collect();
            (ahead, behind, commits)
        }
        None => (0, 0, Vec::new()),
    };
    Some(Repo { branch, base, ahead, behind, files, commits })
}

fn name_status(out: &str) -> Vec<(String, char)> {
    out.lines()
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let status = parts.next()?.chars().next()?;
            Some((parts.last()?.to_string(), if status == 'R' { 'M' } else { status }))
        })
        .collect()
}

/// Commits HEAD has that `base` doesn't, and the reverse.
pub fn ahead_behind(cwd: &str, base: &str) -> (usize, usize) {
    let out = git(cwd, &["rev-list", "--left-right", "--count", &format!("{base}...HEAD")]).unwrap_or_default();
    let mut n = out.split_whitespace().map(|w| w.parse().unwrap_or(0));
    let behind = n.next().unwrap_or(0);
    (n.next().unwrap_or(0), behind)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Worktree {
    pub path: String,
    pub branch: String,
    pub main: bool,
}

fn parse_worktrees(out: &str) -> Vec<Worktree> {
    out.split("\n\n")
        .filter_map(|block| {
            let mut path = None;
            let mut branch = String::new();
            for l in block.lines() {
                if let Some(p) = l.strip_prefix("worktree ") {
                    path = Some(p.to_string());
                } else if let Some(b) = l.strip_prefix("branch ") {
                    branch = b.strip_prefix("refs/heads/").unwrap_or(b).to_string();
                } else if l == "detached" {
                    branch = "detached".into();
                }
            }
            Some(Worktree { path: path?, branch, main: false })
        })
        .enumerate()
        .map(|(i, w)| Worktree { main: i == 0, ..w })
        .collect()
}

/// The repository's checkouts, main one first.
pub fn worktrees(cwd: &str) -> Vec<Worktree> {
    parse_worktrees(&git(cwd, &["worktree", "list", "--porcelain"]).unwrap_or_default())
}

/// Local branches, most recently committed first.
pub fn branches(cwd: &str) -> Vec<String> {
    lines(git(cwd, &["for-each-ref", "--sort=-committerdate", "--format=%(refname:short)", "refs/heads"]))
}

/// Branches already merged into `base`, other than `base` itself.
pub fn merged(cwd: &str, base: &str) -> Vec<String> {
    lines(git(cwd, &["branch", "--merged", base, "--format=%(refname:short)"])).into_iter().filter(|b| b != base).collect()
}

pub fn remotes(cwd: &str) -> usize {
    lines(git(cwd, &["remote"])).len()
}

/// Seconds since the epoch of the last commit on `rev`.
pub fn committed_at(cwd: &str, rev: &str) -> Option<i64> {
    git(cwd, &["log", "-1", "--format=%ct", rev])?.trim().parse().ok()
}

/// Tracked and untracked files, relative to `cwd`.
pub fn ls_files(cwd: &str) -> Vec<String> {
    lines(git(cwd, &["ls-files", "--cached", "--others", "--exclude-standard"]))
}

pub fn add_worktree(repo: &str, path: &str, branch: &str, base: &str) -> Result<(), String> {
    let out = Command::new("git").arg("-C").arg(repo).args(["worktree", "add", "-b", branch, path, base]).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

pub fn clone(url: &str, dest: &str) -> Result<(), String> {
    let out = Command::new("git").args(["clone", "--", url, dest]).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

pub fn remove_worktree(repo: &str, path: &str) -> bool {
    git(repo, &["worktree", "remove", path]).is_some()
}

pub fn set_staged(cwd: &str, path: &str, staged: bool) {
    let args: &[&str] = if staged { &["add", "--", path] } else { &["reset", "-q", "--", path] };
    git(cwd, args);
}

pub fn user_initials(cwd: &str) -> String {
    let name = git(cwd, &["config", "user.name"]).unwrap_or_default();
    let initials: String = name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
    if initials.is_empty() { "ME".into() } else { initials.to_uppercase() }
}

pub fn file_diff(cwd: &str, path: &str) -> Vec<Line> {
    let tracked = git(cwd, &["diff", "HEAD", "--", path]).filter(|s| !s.is_empty());
    let out = tracked.unwrap_or_else(|| {
        let out = Command::new("git").arg("-C").arg(cwd).args(["diff", "--no-index", "--", "/dev/null", path]).output();
        out.map(|o| String::from_utf8_lossy(&o.stdout).into_owned()).unwrap_or_default()
    });
    parse(&out)
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Hunk,
    Context,
    Add,
    Del,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Line {
    pub kind: Kind,
    pub old: Option<usize>,
    pub new: Option<usize>,
    pub text: String,
}

pub fn hunk_start(header: &str, sign: char) -> usize {
    header
        .split_whitespace()
        .find_map(|w| w.strip_prefix(sign))
        .and_then(|w| w.split(',').next()?.parse().ok())
        .unwrap_or(1)
}

pub fn parse(diff: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let (mut old, mut new) = (0, 0);
    let mut body = false;
    for l in diff.lines() {
        if l.starts_with("@@") {
            body = true;
            old = hunk_start(l, '-');
            new = hunk_start(l, '+');
            out.push(Line { kind: Kind::Hunk, old: None, new: None, text: l.into() });
            continue;
        }
        if !body {
            continue;
        }
        let (kind, text) = match l.split_at_checked(1) {
            Some(("+", t)) => (Kind::Add, t),
            Some(("-", t)) => (Kind::Del, t),
            Some((" ", t)) => (Kind::Context, t),
            None => (Kind::Context, ""),
            _ => continue,
        };
        let line = Line {
            kind,
            old: (kind != Kind::Add).then_some(old),
            new: (kind != Kind::Del).then_some(new),
            text: text.into(),
        };
        old += (kind != Kind::Add) as usize;
        new += (kind != Kind::Del) as usize;
        out.push(line);
    }
    out
}

/// Pairs each run of deletions with the additions that follow it, side by side.
pub fn split(lines: &[Line]) -> Vec<(Option<usize>, Option<usize>)> {
    let mut out = Vec::new();
    let mut i = 0;
    while i < lines.len() {
        if matches!(lines[i].kind, Kind::Hunk | Kind::Context) {
            out.push((Some(i), Some(i)));
            i += 1;
            continue;
        }
        let dels = lines[i..].iter().take_while(|l| l.kind == Kind::Del).count();
        let adds = lines[i + dels..].iter().take_while(|l| l.kind == Kind::Add).count();
        for k in 0..dels.max(adds) {
            out.push(((k < dels).then_some(i + k), (k < adds).then_some(i + dels + k)));
        }
        i += dels + adds;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const DIFF: &str = "diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -45,4 +45,5 @@ fn a() {\n ctx\n-old\n+new1\n+new2\n ctx2\n\\ No newline at end of file\n";

    #[test]
    fn parses_hunks_with_line_numbers() {
        let l = parse(DIFF);
        let got: Vec<_> = l.iter().map(|l| (l.kind, l.old, l.new, l.text.as_str())).collect();
        assert_eq!(
            got,
            vec![
                (Kind::Hunk, None, None, "@@ -45,4 +45,5 @@ fn a() {"),
                (Kind::Context, Some(45), Some(45), "ctx"),
                (Kind::Del, Some(46), None, "old"),
                (Kind::Add, None, Some(46), "new1"),
                (Kind::Add, None, Some(47), "new2"),
                (Kind::Context, Some(47), Some(48), "ctx2"),
            ]
        );
    }

    #[test]
    fn split_pairs_deletions_with_additions() {
        let l = parse(DIFF);
        let text = |i: Option<usize>| i.map(|i| l[i].text.as_str());
        let rows: Vec<_> = split(&l).into_iter().map(|(a, b)| (text(a), text(b))).collect();
        assert_eq!(rows[2], (Some("old"), Some("new1")));
        assert_eq!(rows[3], (None, Some("new2")));
        assert_eq!(rows.len(), 5);
    }

    #[test]
    fn reads_worktrees_main_first() {
        let out = "worktree /r\nHEAD 1\nbranch refs/heads/main\n\nworktree /w/fix\nHEAD 2\nbranch refs/heads/fix/a\n\nworktree /w/d\nHEAD 3\ndetached\n";
        let w = parse_worktrees(out);
        assert_eq!(w.len(), 3);
        assert_eq!((w[0].path.as_str(), w[0].branch.as_str(), w[0].main), ("/r", "main", true));
        assert_eq!((w[1].branch.as_str(), w[1].main), ("fix/a", false));
        assert_eq!(w[2].branch, "detached");
    }

    #[test]
    fn name_status_reads_renames_as_modified() {
        assert_eq!(name_status("M\ta\nA\tb\nR100\told\tnew\nD\tc\n"), vec![("a".into(), 'M'), ("b".into(), 'A'), ("new".into(), 'M'), ("c".into(), 'D')]);
    }

    #[test]
    fn numstat_reads_binary_as_zero() {
        assert_eq!(numstat("3\t1\ta.rs\n-\t-\timg.png\n"), vec![("a.rs".into(), 3, 1), ("img.png".into(), 0, 0)]);
    }
}
