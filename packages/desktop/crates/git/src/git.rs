use imara_diff::{Algorithm, Diff, InternedInput};
use std::collections::HashSet;
use std::ops::Range;
use std::path::Path;
use std::process::Command;

pub mod github;
pub mod graph;
use graph::LaneColor;

#[derive(Clone, Debug, PartialEq)]
pub struct FileStat {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    pub staged: bool,
    pub unstaged: bool,
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
    let branch = git(cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).or_else(|| git(cwd, &["symbolic-ref", "--short", "HEAD"]))?.trim().to_string();
    let staged = lines(git(cwd, &["diff", "--cached", "--name-only"]));
    let unstaged = lines(git(cwd, &["diff", "--name-only"]));
    let statuses = name_status(&git(cwd, &["diff", "HEAD", "--name-status"]).unwrap_or_default());
    let mut files: Vec<FileStat> = numstat(&git(cwd, &["diff", "HEAD", "--numstat", "--histogram"]).unwrap_or_default())
        .into_iter()
        .map(|(path, added, removed)| {
            let status = statuses.iter().find(|(p, _)| *p == path).map_or('M', |(_, s)| *s);
            FileStat { staged: staged.contains(&path), unstaged: unstaged.contains(&path), path, added, removed, status }
        })
        .collect();
    for path in lines(git(cwd, &["ls-files", "--others", "--exclude-standard"])) {
        let added = std::fs::read_to_string(std::path::Path::new(cwd).join(&path)).map(|s| s.lines().count()).unwrap_or(0);
        files.push(FileStat { path, added, removed: 0, staged: false, unstaged: true, status: 'A' });
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

/// The repository's checkouts: the main one, then the rest oldest first.
pub fn worktrees(cwd: &str) -> Vec<Worktree> {
    let mut out = parse_worktrees(&git(cwd, &["worktree", "list", "--porcelain"]).unwrap_or_default());
    out.sort_by_cached_key(|w| (!w.main, std::fs::metadata(&w.path).and_then(|m| m.created()).ok()));
    out
}

/// Local branches, most recently committed first.
pub fn branches(cwd: &str) -> Vec<String> {
    lines(git(cwd, &["for-each-ref", "--sort=-committerdate", "--format=%(refname:short)", "refs/heads"]))
}

/// Branches on origin that have no local twin, most recently committed first.
pub fn remote_branches(cwd: &str) -> Vec<String> {
    let local: HashSet<String> = branches(cwd).into_iter().collect();
    let remote = lines(git(cwd, &["for-each-ref", "--sort=-committerdate", "--format=%(refname:lstrip=3)", "refs/remotes/origin"]));
    remote.into_iter().filter(|b| b != "HEAD" && !local.contains(b)).collect()
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

/// Deletes the worktree's folder, uncommitted changes included; its branch stays.
pub fn remove_worktree(repo: &str, path: &str) -> Result<(), String> {
    let out = Command::new("git").arg("-C").arg(repo).args(["worktree", "remove", "--force", path]).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

/// Commits that deleting `branch` would lose, or with no branch the detached HEAD of `cwd`: those no other branch, remote or tag holds.
pub fn lost_commits(cwd: &str, branch: Option<&str>) -> usize {
    let (tip, exclude) = match branch {
        // `--exclude` matches the names `--branches` lists, which lack `refs/heads/`.
        Some(b) => (format!("refs/heads/{b}"), Some(format!("--exclude={b}"))),
        None => ("HEAD".to_string(), None),
    };
    let mut args = vec!["rev-list", "--count", tip.as_str(), "--not"];
    args.extend(exclude.as_deref());
    args.extend(["--branches", "--remotes", "--tags"]);
    git(cwd, &args).and_then(|n| n.trim().parse().ok()).unwrap_or(0)
}

/// Deletes `branch`, merged or not; git refuses while a worktree has it checked out.
pub fn delete_branch(repo: &str, branch: &str) -> Result<(), String> {
    let out = Command::new("git").arg("-C").arg(repo).args(["branch", "-D", "--", branch]).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

pub fn clone(url: &str, dest: &str) -> Result<(), String> {
    let out = Command::new("git").args(["clone", "--", url, dest]).output().map_err(|e| e.to_string())?;
    if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
}

fn with_paths<'a>(args: &[&'a str], paths: &'a [String]) -> Vec<&'a str> {
    args.iter().copied().chain(["--"]).chain(paths.iter().map(String::as_str)).collect()
}

pub fn set_staged(cwd: &str, paths: &[String], staged: bool) {
    let args: &[&str] = if staged { &["add", "-A"] } else { &["reset", "-q"] };
    git(cwd, &with_paths(args, paths));
}

/// Throws away the unstaged edits to `paths`, deleting the untracked ones.
pub fn discard(cwd: &str, paths: &[String]) {
    let untracked = lines(git(cwd, &with_paths(&["ls-files", "--others", "--exclude-standard"], paths)));
    let tracked: Vec<String> = paths.iter().filter(|p| !untracked.contains(p)).cloned().collect();
    if !untracked.is_empty() {
        git(cwd, &with_paths(&["clean", "-fq"], &untracked));
    }
    if !tracked.is_empty() {
        git(cwd, &with_paths(&["checkout", "-q"], &tracked));
    }
}

/// `git commit` taking `message` on stdin. Amending keeps the last message when `message` is empty.
pub fn commit_argv(amend: bool, message: &str) -> Vec<&'static str> {
    let mut argv = vec!["git", "commit", "-q"];
    argv.extend(if amend { ["--amend"].as_slice() } else { &[] });
    argv.extend(if message.is_empty() { ["--no-edit"].as_slice() } else { ["-F", "-"].as_slice() });
    argv
}

/// Pushes the branch, setting its upstream on the first push.
pub const PUSH: &[&str] = &["git", "-c", "push.autoSetupRemote=true", "push"];

const MAX_CONTEXT: usize = 60_000;

/// What a commit message is written from: recent subjects for style, then the diff to commit (only the staged part when `staged`).
pub fn commit_context(cwd: &str, staged: bool) -> String {
    let subjects = lines(git(cwd, &["log", "-n", "10", "--format=%s"])).join("\n");
    let diff = git(cwd, if staged { &["diff", "--cached"] } else { &["diff", "HEAD"] }).unwrap_or_default();
    let new = if staged { Vec::new() } else { lines(git(cwd, &["ls-files", "--others", "--exclude-standard"])) };
    let mut out = format!("Recent commit subjects:\n{subjects}\n\nDiff:\n{diff}");
    out.extend(new.iter().map(|p| format!("\nNew file: {p}")));
    if out.len() > MAX_CONTEXT {
        out.truncate(out.floor_char_boundary(MAX_CONTEXT));
    }
    out
}

pub fn user_initials(cwd: &str) -> String {
    let name = git(cwd, &["config", "user.name"]).unwrap_or_default();
    let initials: String = name.split_whitespace().filter_map(|w| w.chars().next()).take(2).collect();
    if initials.is_empty() { "ME".into() } else { initials.to_uppercase() }
}

pub fn file_diff(cwd: &str, path: &str) -> Vec<Line> {
    let (old, new) = texts(cwd, path);
    diff_texts(&old, &new, &HashSet::new())
}

const CONTEXT: usize = 3;

/// The file at HEAD (empty when untracked) and in the working tree (empty when deleted); `path` is relative to `cwd` or absolute.
pub fn texts(cwd: &str, path: &str) -> (String, String) {
    let full = Path::new(cwd).join(path);
    let rel = full.strip_prefix(cwd).unwrap_or(&full).to_string_lossy().into_owned();
    let old = git(cwd, &["show", &format!("HEAD:./{rel}")]).unwrap_or_default();
    let new = std::fs::read(&full).map(|b| String::from_utf8_lossy(&b).into_owned()).unwrap_or_default();
    (old, new)
}

/// A unified diff of two texts with three lines of context around each change. A gap whose first new-side line is in `open` shows in full instead of folding.
pub fn diff_texts(old: &str, new: &str, open: &HashSet<usize>) -> Vec<Line> {
    if old.contains('\0') || new.contains('\0') {
        return Vec::new();
    }
    let (ol, nl): (Vec<&str>, Vec<&str>) = (old.lines().collect(), new.lines().collect());
    let input = InternedInput::new(old, new);
    let mut diff = Diff::compute(Algorithm::Histogram, &input);
    diff.postprocess_lines(&input);
    let hunks: Vec<_> = diff.hunks().collect();
    if hunks.is_empty() {
        return Vec::new();
    }
    let mut out = Vec::new();
    let (mut o, mut n) = (0, 0);
    for k in 0..=hunks.len() {
        let last = k == hunks.len();
        let (bs, be, as_, ae) = hunks.get(k).map_or((ol.len(), ol.len(), nl.len(), nl.len()), |h| {
            (h.before.start as usize, h.before.end as usize, h.after.start as usize, h.after.end as usize)
        });
        let gap = bs - o;
        let lead = if k == 0 { 0 } else { gap.min(CONTEXT) };
        let trail = if last { 0 } else { (gap - lead).min(CONTEXT) };
        let fold = if last || open.contains(&(n + lead + 1)) { 0 } else { gap - lead - trail };
        let same = |i: usize| Line { kind: Kind::Context, old: Some(o + i + 1), new: Some(n + i + 1), text: ol[o + i].into() };
        out.extend((0..lead).map(same));
        if k == 0 || fold > 0 {
            let (a, b) = (o + lead + fold + 1, n + lead + fold + 1);
            out.push(Line { kind: Kind::Hunk, old: None, new: None, text: format!("@@ -{a} +{b} @@") });
        }
        out.extend((lead + fold..if last { lead } else { gap }).map(same));
        out.extend((bs..be).map(|i| Line { kind: Kind::Del, old: Some(i + 1), new: None, text: ol[i].into() }));
        out.extend((as_..ae).map(|i| Line { kind: Kind::Add, old: None, new: Some(i + 1), text: nl[i].into() }));
        (o, n) = (be, ae);
    }
    out
}

fn tokens(s: &str) -> Vec<(usize, &str)> {
    let mut out = Vec::new();
    let mut start = None;
    for (i, c) in s.char_indices() {
        if c.is_alphanumeric() || c == '_' {
            start.get_or_insert(i);
            continue;
        }
        if let Some(a) = start.take() {
            out.push((a, &s[a..i]));
        }
        out.push((i, &s[i..i + c.len_utf8()]));
    }
    if let Some(a) = start {
        out.push((a, &s[a..]));
    }
    out
}

/// Byte ranges that differ between two versions of a line, compared word by word: (removed from `old`, added in `new`).
pub fn words(old: &str, new: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    let (a, b) = (tokens(old), tokens(new));
    let mut input = InternedInput::default();
    input.update_before(a.iter().map(|t| t.1));
    input.update_after(b.iter().map(|t| t.1));
    let span = |t: &[(usize, &str)], r: Range<u32>| t[r.start as usize].0..t[r.end as usize - 1].0 + t[r.end as usize - 1].1.len();
    let (mut del, mut add) = (Vec::new(), Vec::new());
    for h in Diff::compute(Algorithm::Histogram, &input).hunks() {
        if !h.before.is_empty() {
            del.push(span(&a, h.before));
        }
        if !h.after.is_empty() {
            add.push(span(&b, h.after));
        }
    }
    (del, add)
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

/// A commit as the graph shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphCommit {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    pub subject: String,
}

/// `n` commits reachable from `revs`, children before parents, after skipping `skip`.
pub fn log_graph(cwd: &str, revs: &[String], skip: usize, n: usize) -> Vec<GraphCommit> {
    let (skip, n) = (format!("--skip={skip}"), format!("-n{n}"));
    let mut args = vec!["log", "--format=%H%x00%P%x00%aN%x00%s", "--topo-order", skip.as_str(), n.as_str()];
    args.extend(revs.iter().map(String::as_str));
    args.push("--");
    lines(git(cwd, &args))
        .into_iter()
        .filter_map(|l| {
            let mut p = l.splitn(4, '\0');
            Some(GraphCommit {
                sha: p.next()?.into(),
                parents: p.next()?.split_whitespace().map(str::to_string).collect(),
                author: p.next()?.into(),
                subject: p.next()?.into(),
            })
        })
        .collect()
}

/// A branch the graph labels: its short name, the commit it points at, and whether it is a remote-tracking branch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tip {
    pub name: String,
    pub sha: String,
    pub remote: bool,
}

/// What the graph follows: HEAD, the checked-out branch, its upstream and the base branch it grew from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tips {
    pub head: String,
    pub branch: Option<Tip>,
    pub upstream: Option<Tip>,
    pub base: Option<Tip>,
}

impl Tips {
    /// The commits the graph's history starts from, each once.
    pub fn revs(&self) -> Vec<String> {
        let mut revs = vec![self.head.clone()];
        for t in [&self.upstream, &self.base].into_iter().flatten() {
            if !revs.contains(&t.sha) {
                revs.push(t.sha.clone());
            }
        }
        revs
    }

    /// The tips pointing at `sha`, with the colour each labels its lane with.
    pub fn pointing_at<'a>(&'a self, sha: &'a str) -> impl Iterator<Item = (LaneColor, &'a Tip)> {
        [(LaneColor::Head, &self.branch), (LaneColor::Upstream, &self.upstream), (LaneColor::Base, &self.base)]
            .into_iter()
            .filter_map(move |(c, t)| Some((c, t.as_ref().filter(|t| t.sha == sha)?)))
    }

    /// The colour of the lane leaving `sha`, when a tip points at it.
    pub fn color(&self, sha: &str) -> Option<LaneColor> {
        (sha == self.head).then_some(LaneColor::Head).or_else(|| self.pointing_at(sha).next().map(|(c, _)| c))
    }
}

/// The worktree's tips; `None` before its first commit.
pub fn tips(cwd: &str) -> Option<Tips> {
    let head = git(cwd, &["rev-parse", "--verify", "-q", "HEAD"])?.trim().to_string();
    let refs: Vec<(String, String, String)> = lines(git(cwd, &["for-each-ref", "--format=%(refname)%00%(objectname)%00%(upstream)", "refs/heads", "refs/remotes"]))
        .into_iter()
        .filter_map(|l| {
            let mut p = l.splitn(3, '\0');
            Some((p.next()?.to_string(), p.next()?.to_string(), p.next()?.to_string()))
        })
        .collect();
    let tip = |name: &str| {
        refs.iter().find(|(r, ..)| r == name).map(|(r, sha, _)| {
            let short = r.strip_prefix("refs/heads/").or_else(|| r.strip_prefix("refs/remotes/")).unwrap_or(r);
            Tip { name: short.to_string(), sha: sha.clone(), remote: r.starts_with("refs/remotes/") }
        })
    };
    let current = git(cwd, &["symbolic-ref", "-q", "HEAD"]).map(|s| s.trim().to_string());
    let branch = current.as_deref().and_then(tip);
    let upstream = current.as_deref().and_then(|c| refs.iter().find(|(r, ..)| r == c)).and_then(|(.., u)| tip(u));
    let base = match current.as_deref() {
        Some("refs/heads/main" | "refs/heads/master") => None,
        _ => tip("refs/heads/main").or_else(|| tip("refs/heads/master")),
    };
    Some(Tips { head, branch, upstream, base })
}

/// A file a commit changed; `old_path` is set for renames and copies.
#[derive(Clone, Debug, PartialEq)]
pub struct CommitFile {
    pub path: String,
    pub old_path: Option<String>,
    pub status: char,
}

pub fn first_parent(cwd: &str, sha: &str) -> Option<String> {
    git(cwd, &["rev-parse", "--verify", "-q", &format!("{sha}^")]).map(|s| s.trim().to_string())
}

/// The files `sha` changed against `parent`, or every file it holds when it is a root commit.
pub fn commit_files(cwd: &str, sha: &str, parent: Option<&str>) -> Vec<CommitFile> {
    let mut args = vec!["diff-tree", "-r", "-z", "-M", "--name-status", "--no-commit-id"];
    match parent {
        Some(p) => args.extend([p, sha]),
        None => args.extend(["--root", sha]),
    }
    let out = git(cwd, &args).unwrap_or_default();
    let mut tokens = out.split('\0').filter(|t| !t.is_empty());
    let mut files = Vec::new();
    while let Some(status) = tokens.next().and_then(|s| s.chars().next()) {
        let Some(first) = tokens.next() else { break };
        let (old_path, path) = match status {
            'R' | 'C' => match tokens.next() {
                Some(new) => (Some(first.to_string()), new.to_string()),
                None => break,
            },
            _ => (None, first.to_string()),
        };
        files.push(CommitFile { path, old_path, status });
    }
    files
}

/// `file` before and after commit `sha`.
pub fn commit_texts(cwd: &str, sha: &str, parent: Option<&str>, file: &CommitFile) -> (String, String) {
    let show = |rev: &str, path: &str| git(cwd, &["show", &format!("{rev}:{path}")]).unwrap_or_default();
    let old = match parent {
        Some(p) if file.status != 'A' => show(p, file.old_path.as_deref().unwrap_or(&file.path)),
        _ => String::new(),
    };
    let new = if file.status == 'D' { String::new() } else { show(sha, &file.path) };
    (old, new)
}

/// The file at `path` before and after commit `sha`, following a rename back to the old name.
pub fn texts_at(cwd: &str, sha: &str, path: &str) -> (String, String) {
    let parent = first_parent(cwd, sha);
    commit_files(cwd, sha, parent.as_deref()).into_iter().find(|f| f.path == path).map(|f| commit_texts(cwd, sha, parent.as_deref(), &f)).unwrap_or_default()
}

pub fn short_sha(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
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

    fn add_worktree(repo: &str, path: &str, branch: &str, base: &str) -> Result<(), String> {
        let out = Command::new("git").arg("-C").arg(repo).args(["worktree", "add", "-b", branch, path, base]).output().map_err(|e| e.to_string())?;
        if out.status.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
    }

    fn scratch_repo(tag: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pocket-git-{tag}-{}", std::process::id()));
        std::fs::remove_dir_all(&dir).ok();
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        let run = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(&repo).args(args).output().unwrap().status.success());
        run(&["init", "-q"]);
        std::fs::write(repo.join("a"), "a\n").unwrap();
        run(&["add", "."]);
        run(&["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-qm", "init"]);
        dir
    }

    #[test]
    fn lists_worktrees_main_first_then_oldest_first() {
        let dir = scratch_repo("order");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        for name in ["zeta", "alpha", "mid"] {
            add_worktree(r, dir.join(name).to_str().unwrap(), name, "HEAD").unwrap();
        }
        let names: Vec<String> = worktrees(r).iter().map(|w| w.path.rsplit('/').next().unwrap().to_string()).collect();
        assert_eq!(names, ["repo", "zeta", "alpha", "mid"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn removing_a_worktree_deletes_its_folder_but_keeps_its_branch() {
        let dir = scratch_repo("remove");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        let tree = dir.join("fix");
        add_worktree(r, tree.to_str().unwrap(), "fix", "HEAD").unwrap();
        std::fs::write(tree.join("draft"), "unsaved\n").unwrap();
        remove_worktree(r, tree.to_str().unwrap()).unwrap();
        assert!(!tree.exists());
        assert!(branches(r).contains(&"fix".to_string()));
        assert_eq!(worktrees(r).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stages_unstages_and_discards() {
        let dir = scratch_repo("stage");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        std::fs::write(repo.join("a"), "edited\n").unwrap();
        std::fs::write(repo.join("new"), "fresh\n").unwrap();
        let flags = |path: &str| read(r).unwrap().files.iter().find(|f| f.path == path).map(|f| (f.staged, f.unstaged));
        assert_eq!((flags("a"), flags("new")), (Some((false, true)), Some((false, true))));
        set_staged(r, &["a".into(), "new".into()], true);
        assert_eq!((flags("a"), flags("new")), (Some((true, false)), Some((true, false))));
        std::fs::write(repo.join("a"), "edited twice\n").unwrap();
        assert_eq!(flags("a"), Some((true, true)));
        discard(r, &["a".into()]);
        assert_eq!(std::fs::read_to_string(repo.join("a")).unwrap(), "edited\n");
        set_staged(r, &["a".into(), "new".into()], false);
        assert_eq!(flags("a"), Some((false, true)));
        discard(r, &["a".into(), "new".into()]);
        assert!(read(r).unwrap().files.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn commit_context_holds_only_the_staged_diff_when_asked() {
        let dir = scratch_repo("context");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        std::fs::write(repo.join("a"), "staged\n").unwrap();
        set_staged(r, &["a".into()], true);
        std::fs::write(repo.join("b"), "loose\n").unwrap();
        let staged = commit_context(r, true);
        assert!(staged.starts_with("Recent commit subjects:\ninit\n"));
        assert!(staged.contains("+staged") && !staged.contains("New file: b"));
        assert!(commit_context(r, false).contains("New file: b"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn run_argv(repo: &Path, argv: &[&str], input: &str) -> bool {
        use std::io::Write;
        let mut child = Command::new(argv[0]).args(&argv[1..]).current_dir(repo).stdin(std::process::Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        child.wait().unwrap().success()
    }

    fn committer(repo: &Path) {
        for (k, v) in [("user.name", "t"), ("user.email", "t@t"), ("commit.gpgsign", "false")] {
            assert!(Command::new("git").arg("-C").arg(repo).args(["config", k, v]).status().unwrap().success());
        }
    }

    #[test]
    fn commit_argv_commits_the_staged_files_with_the_message_from_stdin() {
        let dir = scratch_repo("commit");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        std::fs::write(repo.join("a"), "edited\n").unwrap();
        set_staged(r, &["a".into()], true);
        assert!(run_argv(&repo, &commit_argv(false, "Edit a"), "Edit a"));
        assert_eq!(lines(git(r, &["log", "--format=%s"])), ["Edit a", "init"]);
        assert!(read(r).unwrap().files.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn amending_without_a_message_keeps_the_last_one_and_with_one_replaces_it() {
        let dir = scratch_repo("amend");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        std::fs::write(repo.join("a"), "edited\n").unwrap();
        set_staged(r, &["a".into()], true);
        assert!(run_argv(&repo, &commit_argv(true, ""), ""));
        assert_eq!(lines(git(r, &["log", "--format=%s"])), ["init"]);
        assert!(read(r).unwrap().files.is_empty());
        assert!(run_argv(&repo, &commit_argv(true, "Start"), "Start"));
        assert_eq!(lines(git(r, &["log", "--format=%s"])), ["Start"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_first_push_sets_the_upstream() {
        let dir = scratch_repo("push");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        let remote = dir.join("remote.git");
        assert!(Command::new("git").args(["init", "-q", "--bare"]).arg(&remote).status().unwrap().success());
        assert!(git(r, &["remote", "add", "origin", remote.to_str().unwrap()]).is_some());
        assert!(git(r, &["switch", "-qc", "feature"]).is_some());
        assert!(run_argv(&repo, PUSH, ""));
        assert_eq!(git(r, &["rev-parse", "--abbrev-ref", "@{u}"]).unwrap().trim(), "origin/feature");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn name_status_reads_renames_as_modified() {
        assert_eq!(name_status("M\ta\nA\tb\nR100\told\tnew\nD\tc\n"), vec![("a".into(), 'M'), ("b".into(), 'A'), ("new".into(), 'M'), ("c".into(), 'D')]);
    }

    #[test]
    fn numstat_reads_binary_as_zero() {
        assert_eq!(numstat("3\t1\ta.rs\n-\t-\timg.png\n"), vec![("a.rs".into(), 3, 1), ("img.png".into(), 0, 0)]);
    }

    fn numbered(n: usize) -> String {
        (1..=n).map(|i| format!("l{i}\n")).collect()
    }

    #[test]
    fn diffs_word_by_word() {
        assert_eq!(words("let a = 1;", "let b = 1;"), (vec![4..5], vec![4..5]));
        assert_eq!(words("foo(bar)", "foo(bar, baz)"), (vec![], vec![7..12]));
        assert_eq!(words("é = 1", "é = 2"), (vec![5..6], vec![5..6]));
    }

    #[test]
    fn a_repository_without_commits_reads_as_one_on_its_branch() {
        let dir = std::env::temp_dir().join(format!("pocket-git-unborn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_str().unwrap();
        assert!(read(d).is_none());
        assert!(Command::new("git").arg("-C").arg(d).args(["init", "-q", "-b", "trunk"]).output().unwrap().status.success());
        std::fs::write(dir.join("a.txt"), "x\n").unwrap();
        let r = read(d).unwrap();
        assert_eq!(r.branch, "trunk");
        assert_eq!(r.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["a.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn reads_head_and_working_texts() {
        let dir = std::env::temp_dir().join(format!("pocket-git-texts-{}", std::process::id()));
        std::fs::create_dir_all(dir.join("src")).unwrap();
        let d = dir.to_str().unwrap();
        let run = |args: &[&str]| assert!(Command::new("git").arg("-C").arg(d).args(args).output().unwrap().status.success());
        run(&["init", "-q"]);
        std::fs::write(dir.join("src/a.rs"), "old\n").unwrap();
        run(&["add", "."]);
        run(&["-c", "user.name=t", "-c", "user.email=t@t", "-c", "commit.gpgsign=false", "commit", "-qm", "init"]);
        std::fs::write(dir.join("src/a.rs"), "new\n").unwrap();
        std::fs::write(dir.join("b.rs"), "fresh\n").unwrap();
        assert_eq!(texts(d, "src/a.rs"), ("old\n".into(), "new\n".into()));
        assert_eq!(texts(&format!("{d}/src"), &format!("{d}/src/a.rs")), ("old\n".into(), "new\n".into()));
        assert_eq!(texts(d, "b.rs"), (String::new(), "fresh\n".into()));
        std::fs::write(dir.join("src/a.rs"), b"\x89PNG\0\xff").unwrap();
        assert!(file_diff(d, "src/a.rs").is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn diffs_texts_with_three_lines_of_context() {
        let old = numbered(20);
        let lines = diff_texts(&old, &old.replace("l10\n", "L10\n"), &HashSet::new());
        let got: Vec<_> = lines.iter().map(|l| (l.kind, l.old, l.new, l.text.as_str())).collect();
        assert_eq!(
            got,
            vec![
                (Kind::Hunk, None, None, "@@ -7 +7 @@"),
                (Kind::Context, Some(7), Some(7), "l7"),
                (Kind::Context, Some(8), Some(8), "l8"),
                (Kind::Context, Some(9), Some(9), "l9"),
                (Kind::Del, Some(10), None, "l10"),
                (Kind::Add, None, Some(10), "L10"),
                (Kind::Context, Some(11), Some(11), "l11"),
                (Kind::Context, Some(12), Some(12), "l12"),
                (Kind::Context, Some(13), Some(13), "l13"),
            ]
        );
    }

    #[test]
    fn folds_long_gaps_and_opens_them_on_request() {
        let old = numbered(30);
        let new = old.replace("l5\n", "L5\n").replace("l25\n", "L25\n");
        let folded = diff_texts(&old, &new, &HashSet::new());
        let headers: Vec<&str> = folded.iter().filter(|l| l.kind == Kind::Hunk).map(|l| l.text.as_str()).collect();
        assert_eq!(headers, ["@@ -2 +2 @@", "@@ -22 +22 @@"]);
        let open = diff_texts(&old, &new, &HashSet::from([9]));
        assert_eq!(open.iter().filter(|l| l.kind == Kind::Hunk).count(), 1);
        assert_eq!(open.len(), folded.len() - 1 + 13);
    }

    #[test]
    fn merges_nearby_changes_and_handles_edges() {
        let old = numbered(10);
        let near = diff_texts(&old, &old.replace("l1\n", "L1\n").replace("l6\n", "L6\n"), &HashSet::new());
        assert_eq!(near[0].text, "@@ -1 +1 @@");
        assert_eq!(near.iter().filter(|l| l.kind == Kind::Hunk).count(), 1);
        assert!(diff_texts(&old, &old, &HashSet::new()).is_empty());
        assert!(diff_texts("a\0", "b", &HashSet::new()).is_empty());
        let added = diff_texts("", "x\ny\n", &HashSet::new());
        assert_eq!(added.iter().map(|l| l.kind).collect::<Vec<_>>(), [Kind::Hunk, Kind::Add, Kind::Add]);
    }

    fn sh(repo: &Path, args: &[&str]) {
        assert!(Command::new("git").arg("-C").arg(repo).args(args).output().unwrap().status.success(), "git {args:?}");
    }

    fn sha(r: &str, rev: &str) -> String {
        git(r, &["rev-parse", rev]).unwrap().trim().to_string()
    }

    #[test]
    fn tips_follow_the_branch_its_upstream_and_main() {
        let dir = scratch_repo("tips");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        sh(&repo, &["branch", "-M", "main"]);
        sh(&repo, &["checkout", "-qb", "feature"]);
        sh(&repo, &["update-ref", "refs/remotes/origin/feature", "HEAD"]);
        for (k, v) in [("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"), ("branch.feature.remote", "origin"), ("branch.feature.merge", "refs/heads/feature")] {
            sh(&repo, &["config", k, v]);
        }
        std::fs::write(repo.join("b"), "b\n").unwrap();
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-qm", "b"]);
        let (head, main) = (sha(r, "HEAD"), sha(r, "main"));
        let t = tips(r).unwrap();
        assert_eq!(t.head, head);
        assert_eq!(t.branch, Some(Tip { name: "feature".into(), sha: head.clone(), remote: false }));
        assert_eq!(t.upstream, Some(Tip { name: "origin/feature".into(), sha: main.clone(), remote: true }));
        assert_eq!(t.base, Some(Tip { name: "main".into(), sha: main.clone(), remote: false }));
        assert_eq!(t.revs(), vec![head, main.clone()]);
        assert_eq!(t.color(&main), Some(LaneColor::Upstream));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn on_main_the_graph_has_no_base() {
        let dir = scratch_repo("tips-main");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        sh(&repo, &["branch", "-M", "main"]);
        let t = tips(r).unwrap();
        assert_eq!(t.revs(), vec![sha(r, "HEAD")]);
        assert_eq!(t.branch.map(|b| b.name).as_deref(), Some("main"));
        assert_eq!((t.upstream, t.base), (None, None));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn log_graph_pages_through_history_children_first() {
        let dir = scratch_repo("log");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        for name in ["b", "c"] {
            std::fs::write(repo.join(name), name).unwrap();
            sh(&repo, &["add", "."]);
            sh(&repo, &["commit", "-qm", name]);
        }
        let head = vec![sha(r, "HEAD")];
        let all = log_graph(r, &head, 0, 50);
        assert_eq!(all.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["c", "b", "init"]);
        assert_eq!(all[0].sha, head[0]);
        assert_eq!(all[0].parents, vec![all[1].sha.clone()]);
        assert_eq!(all[0].author, "t");
        assert!(all[2].parents.is_empty());
        assert_eq!(log_graph(r, &head, 1, 1), all[1..2].to_vec());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn commit_files_lists_what_a_commit_added_deleted_and_renamed() {
        let dir = scratch_repo("files");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        std::fs::write(repo.join("a"), "one\ntwo\nthree\nfour\nfive\n").unwrap();
        std::fs::write(repo.join("gone"), "x\n").unwrap();
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-qm", "two"]);
        sh(&repo, &["mv", "a", "b"]);
        std::fs::write(repo.join("b"), "one\ntwo\nthree\nfour\nfive\nsix\n").unwrap();
        std::fs::remove_file(repo.join("gone")).unwrap();
        std::fs::write(repo.join("new"), "n\n").unwrap();
        sh(&repo, &["add", "-A"]);
        sh(&repo, &["commit", "-qm", "three"]);
        let head = sha(r, "HEAD");
        let parent = first_parent(r, &head);
        assert_eq!(parent, Some(sha(r, "HEAD^")));
        let file = |path: &str, old: Option<&str>, status| CommitFile { path: path.into(), old_path: old.map(str::to_string), status };
        assert_eq!(commit_files(r, &head, parent.as_deref()), vec![file("b", Some("a"), 'R'), file("gone", None, 'D'), file("new", None, 'A')]);
        assert_eq!(texts_at(r, &head, "b"), ("one\ntwo\nthree\nfour\nfive\n".into(), "one\ntwo\nthree\nfour\nfive\nsix\n".into()));
        assert_eq!(texts_at(r, &head, "gone"), ("x\n".into(), String::new()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_root_commit_lists_every_file_as_added() {
        let dir = scratch_repo("root");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        let root = sha(r, "HEAD");
        assert_eq!(first_parent(r, &root), None);
        assert_eq!(commit_files(r, &root, None), vec![CommitFile { path: "a".into(), old_path: None, status: 'A' }]);
        assert_eq!(texts_at(r, &root, "a"), (String::new(), "a\n".into()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn remote_branches_leave_out_head_and_branches_already_local() {
        let dir = scratch_repo("remote-branches");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        sh(&repo, &["branch", "-M", "main"]);
        for b in ["main", "feat/x"] {
            sh(&repo, &["update-ref", &format!("refs/remotes/origin/{b}"), "HEAD"]);
        }
        sh(&repo, &["symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/main"]);
        assert_eq!(remote_branches(r), ["feat/x"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn commit_file(dir: &Path, name: &str) {
        std::fs::write(dir.join(name), "x\n").unwrap();
        sh(dir, &["add", "."]);
        sh(dir, &["commit", "-qm", name]);
    }

    #[test]
    fn lost_commits_are_those_no_other_branch_remote_or_tag_holds() {
        let dir = scratch_repo("lost");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        sh(&repo, &["branch", "-M", "main"]);
        let tree = dir.join("fix");
        add_worktree(r, tree.to_str().unwrap(), "fix/a", "HEAD").unwrap();
        assert_eq!(lost_commits(r, Some("fix/a")), 0);
        commit_file(&tree, "b");
        commit_file(&tree, "c");
        assert_eq!(lost_commits(r, Some("fix/a")), 2);
        sh(&repo, &["update-ref", "refs/remotes/origin/fix/a", "fix/a~1"]);
        assert_eq!(lost_commits(r, Some("fix/a")), 1);
        sh(&repo, &["update-ref", "refs/remotes/origin/fix/a", "fix/a"]);
        assert_eq!(lost_commits(r, Some("fix/a")), 0);
        sh(&repo, &["update-ref", "-d", "refs/remotes/origin/fix/a"]);
        sh(&repo, &["merge", "-q", "--ff-only", "fix/a"]);
        assert_eq!(lost_commits(r, Some("fix/a")), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_detached_tree_loses_the_commits_only_its_head_holds() {
        let dir = scratch_repo("lost-detached");
        let repo = dir.join("repo");
        committer(&repo);
        let tree = dir.join("look");
        sh(&repo, &["worktree", "add", "-q", "--detach", tree.to_str().unwrap()]);
        let t = tree.to_str().unwrap();
        assert_eq!(lost_commits(t, None), 0);
        commit_file(&tree, "b");
        assert_eq!(lost_commits(t, None), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_branch_deletes_once_its_worktree_is_removed() {
        let dir = scratch_repo("delete-branch");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        let tree = dir.join("fix");
        add_worktree(r, tree.to_str().unwrap(), "fix", "HEAD").unwrap();
        assert!(delete_branch(r, "fix").unwrap_err().contains("fix"));
        remove_worktree(r, tree.to_str().unwrap()).unwrap();
        delete_branch(r, "fix").unwrap();
        assert!(!branches(r).contains(&"fix".to_string()));
        assert!(delete_branch(r, "fix").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
