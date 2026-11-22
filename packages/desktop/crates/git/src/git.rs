use std::collections::HashSet;
use std::path::Path;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

pub mod compare;
pub mod github;
pub mod graph;
mod runner;
pub use runner::{Exec, MAX_READ, NotConnected, Output, Runner, Stat, too_large};
mod diff;
pub use diff::{Kind, Line, Options, blobs, diff_texts, file_diff, hunk_start, parse, split, texts, words};
pub use graph::{GraphCommit, Tip, Tips, log_graph, tips};

#[derive(Clone, Debug, PartialEq)]
pub struct FileStat {
    pub path: String,
    pub added: usize,
    pub removed: usize,
    pub staged: bool,
    pub unstaged: bool,
    /// Git's status letter: M, A or D, and R for a Compare's renames.
    pub status: char,
    /// The path it had before a rename.
    pub old_path: Option<String>,
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
    /// Commits not on the branch's upstream yet; `None` before its first push.
    pub unpushed: Option<usize>,
    pub files: Vec<FileStat>,
    pub commits: Vec<Commit>,
    pub remotes: Vec<String>,
}

impl Repo {
    pub fn totals(&self) -> (usize, usize) {
        totals(&self.files)
    }
}

pub fn totals(files: &[FileStat]) -> (usize, usize) {
    files.iter().fold((0, 0), |(a, r), f| (a + f.added, r + f.removed))
}

/// How long a git command may take on a host.
const TIMEOUT: Duration = Duration::from_secs(30);
/// How long a clone or a worktree's removal may take on a host: they move a whole tree.
const LONG: Duration = Duration::from_secs(10 * 60);

/// `-C` rather than the exec's `cwd`, so a missing folder fails with git's message; `-C ""` stays put.
fn output(run: &Runner, cwd: &str, args: &[&str]) -> std::io::Result<Output> {
    output_within(run, cwd, args, TIMEOUT)
}

fn output_within(run: &Runner, cwd: &str, args: &[&str], timeout: Duration) -> std::io::Result<Output> {
    let argv: Vec<&str> = ["git", "-C", cwd].into_iter().chain(args.iter().copied()).collect();
    run.exec("", &argv, None, timeout)
}

fn git(run: &Runner, cwd: &str, args: &[&str]) -> Option<String> {
    git_bytes(run, cwd, args).map(|b| String::from_utf8_lossy(&b).into_owned())
}

fn git_bytes(run: &Runner, cwd: &str, args: &[&str]) -> Option<Vec<u8>> {
    let out = output(run, cwd, args).ok()?;
    out.success().then_some(out.stdout)
}

/// A file's bytes before and after a change; empty on a side where it doesn't exist.
pub type Blobs = (Vec<u8>, Vec<u8>);

fn lossy((old, new): Blobs) -> (String, String) {
    (String::from_utf8_lossy(&old).into_owned(), String::from_utf8_lossy(&new).into_owned())
}

/// `git` for a command whose failure the user reads.
fn git_result(run: &Runner, cwd: &str, args: &[&str]) -> Result<(), String> {
    git_result_within(run, cwd, args, TIMEOUT)
}

fn git_result_within(run: &Runner, cwd: &str, args: &[&str], timeout: Duration) -> Result<(), String> {
    let out = output_within(run, cwd, args, timeout).map_err(|e| e.to_string())?;
    if out.success() { Ok(()) } else { Err(String::from_utf8_lossy(&out.stderr).trim().to_string()) }
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

/// Whether `git worktree add` is still filling this tree. It lists the tree before the first checkout writes the index, and until then git calls all of HEAD deleted and each file written so far untracked. Reads the `.git` file rather than asking git, as this runs on every refresh of every tree.
fn checking_out(run: &Runner, cwd: &str) -> bool {
    let root = Path::new(cwd);
    let Ok(file) = run.read(&root.join(".git").to_string_lossy(), 4096) else { return false };
    let file = String::from_utf8_lossy(&file);
    let Some(dir) = file.strip_prefix("gitdir: ") else { return false };
    matches!(run.stat(&root.join(dir.trim()).join("index").to_string_lossy()), Ok(None))
        && git(run, cwd, &["rev-parse", "--verify", "--quiet", "HEAD"]).is_some()
}

fn changes(run: &Runner, cwd: &str, untracked: bool) -> Vec<FileStat> {
    let staged = lines(git(run, cwd, &["diff", "--cached", "--name-only"]));
    let unstaged = lines(git(run, cwd, &["diff", "--name-only"]));
    let statuses = name_status(&git(run, cwd, &["diff", "HEAD", "--name-status"]).unwrap_or_default());
    let mut files: Vec<FileStat> = numstat(&git(run, cwd, &["diff", "HEAD", "--numstat", "--histogram"]).unwrap_or_default())
        .into_iter()
        .map(|(path, added, removed)| {
            let status = statuses.iter().find(|(p, _)| *p == path).map_or('M', |(_, s)| *s);
            FileStat { staged: staged.contains(&path), unstaged: unstaged.contains(&path), path, added, removed, status, old_path: None }
        })
        .collect();
    let new = if untracked { lines(git(run, cwd, &["ls-files", "--others", "--exclude-standard"])) } else { Vec::new() };
    for path in new {
        let full = Path::new(cwd).join(&path);
        let added = run.read(&full.to_string_lossy(), MAX_READ).ok().and_then(|b| String::from_utf8(b).ok()).map_or(0, |s| s.lines().count());
        files.push(FileStat { path, added, removed: 0, staged: false, unstaged: true, status: 'A', old_path: None });
    }
    files
}

/// The worktree's branch and changes; untracked files only with `untracked`.
pub fn read(run: &Runner, cwd: &str, untracked: bool) -> Option<Repo> {
    let branch = git(run, cwd, &["rev-parse", "--abbrev-ref", "HEAD"]).or_else(|| git(run, cwd, &["symbolic-ref", "--short", "HEAD"]))?.trim().to_string();
    let files = if checking_out(run, cwd) { Vec::new() } else { changes(run, cwd, untracked) };
    let upstream = || git(run, cwd, &["rev-parse", "--abbrev-ref", "@{u}"]).map(|s| s.trim().to_string());
    let base = if branch == "main" || branch == "master" {
        upstream()
    } else {
        match base_of(run, cwd, &branch) {
            Some(b) if github::base_branch(&b) == branch => upstream(),
            b => b,
        }
    };
    let (ahead, behind, commits) = match &base {
        Some(b) => {
            let range = format!("{b}..HEAD");
            let (ahead, behind) = ahead_behind(run, cwd, b);
            let commits = lines(git(run, cwd, &["log", "--format=%h\t%s", "-n", "20", &range]))
                .into_iter()
                .filter_map(|l| l.split_once('\t').map(|(sha, subject)| Commit { sha: sha.into(), subject: subject.into() }))
                .collect();
            (ahead, behind, commits)
        }
        None => (0, 0, Vec::new()),
    };
    let unpushed = git(run, cwd, &["rev-list", "--count", "@{u}..HEAD"]).and_then(|n| n.trim().parse().ok());
    Some(Repo { branch, base, ahead, behind, unpushed, files, commits, remotes: remote_names(run, cwd) })
}

/// What `branch` was created from per its reflog, else origin's default branch, else main or master; origin's copy beats a local one, which may be stale.
fn base_of(run: &Runner, cwd: &str, branch: &str) -> Option<String> {
    let exists = |r: &str| git(run, cwd, &["rev-parse", "--verify", "--quiet", r]).is_some();
    let oldest = lines(git(run, cwd, &["log", "-g", "--format=%gs", &format!("refs/heads/{branch}"), "--"])).pop();
    oldest
        .as_deref()
        .and_then(created_from)
        .and_then(|n| [("refs/remotes/", format!("origin/{n}")), ("refs/heads/", n.to_string())].into_iter().find(|(at, b)| exists(&format!("{at}{b}"))))
        .map(|(_, b)| b)
        .or_else(|| {
            let default = git(run, cwd, &["symbolic-ref", "-q", "--short", "refs/remotes/origin/HEAD"]).map(|s| s.trim().to_string());
            default.into_iter().chain(["origin/main", "origin/master", "main", "master"].map(String::from)).find(|b| exists(b))
        })
}

/// The branch a reflog's first entry says it was created from, without `refs/…` or `origin/`; `None` when made from HEAD.
fn created_from(entry: &str) -> Option<&str> {
    let from = entry.strip_prefix("branch: Created from ")?;
    let from = from.strip_prefix("refs/remotes/").or_else(|| from.strip_prefix("refs/heads/")).unwrap_or(from);
    Some(from.strip_prefix("origin/").unwrap_or(from)).filter(|f| *f != "HEAD")
}

fn name_status(out: &str) -> Vec<(String, char)> {
    out.lines()
        .filter_map(|l| {
            let mut parts = l.split('\t');
            let status = parts.next()?.chars().next()?;
            Some((parts.next_back()?.to_string(), if status == 'R' { 'M' } else { status }))
        })
        .collect()
}

/// Commits HEAD has that `base` doesn't, and the reverse.
pub fn ahead_behind(run: &Runner, cwd: &str, base: &str) -> (usize, usize) {
    let out = git(run, cwd, &["rev-list", "--left-right", "--count", &format!("{base}...HEAD")]).unwrap_or_default();
    let mut n = out.split_whitespace().map(|w| w.parse().unwrap_or(0));
    let behind = n.next().unwrap_or(0);
    (n.next().unwrap_or(0), behind)
}

#[derive(Clone, Debug, PartialEq)]
pub struct Worktree {
    pub path: String,
    pub branch: String,
    pub main: bool,
    /// Anywhere made it, whatever folder it is in.
    pub made: bool,
    /// When its folder was made, if known.
    pub created: Option<SystemTime>,
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
            Some(Worktree { path: path?, branch, main: false, made: false, created: None })
        })
        .enumerate()
        .map(|(i, w)| Worktree { main: i == 0, ..w })
        .collect()
}

/// The repository's checkouts: the main one, then the rest oldest first.
pub fn worktrees(run: &Runner, cwd: &str) -> Vec<Worktree> {
    let mut out = parse_worktrees(&git(run, cwd, &["worktree", "list", "--porcelain"]).unwrap_or_default());
    for w in &mut out {
        w.path = run.place(&w.path);
        w.made = !w.main && marked(run, &w.path);
        w.created = run.stat(&w.path).ok().flatten().and_then(|s| s.created);
    }
    out.sort_by_key(|w| (!w.main, w.created));
    out
}

/// Whether pocketd left its marker in `tree`'s git folder, as it does for each worktree it makes (`worktree.Marker`).
fn marked(run: &Runner, tree: &str) -> bool {
    marker(run, tree).is_some()
}

/// When pocketd made the worktree at `tree`, in Unix milliseconds on the machine holding it: its marker is written once.
pub fn made_at(run: &Runner, tree: &str) -> Option<u64> {
    let made = marker(run, tree)?.modified?.duration_since(UNIX_EPOCH).ok()?;
    Some(made.as_millis() as u64)
}

fn marker(run: &Runner, tree: &str) -> Option<Stat> {
    let pointer = run.read(&format!("{tree}/.git"), 4096).ok()?;
    let pointer = String::from_utf8_lossy(&pointer);
    let dir = pointer.trim().strip_prefix("gitdir: ")?;
    let dir = if dir.starts_with('.') { format!("{tree}/{dir}") } else { run.place(dir) };
    run.stat(&format!("{dir}/anywhere")).ok().flatten()
}

/// Local branches, most recently committed first.
pub fn branches(run: &Runner, cwd: &str) -> Vec<String> {
    lines(git(run, cwd, &["for-each-ref", "--sort=-committerdate", "--format=%(refname:short)", "refs/heads"]))
}

/// Branches on origin that have no local twin, most recently committed first.
pub fn remote_branches(run: &Runner, cwd: &str) -> Vec<String> {
    let local: HashSet<String> = branches(run, cwd).into_iter().collect();
    origin_branches(run, cwd).into_iter().filter(|b| !local.contains(b)).collect()
}

/// Branches on origin, without `origin/`, most recently committed first.
pub fn origin_branches(run: &Runner, cwd: &str) -> Vec<String> {
    let remote = lines(git(run, cwd, &["for-each-ref", "--sort=-committerdate", "--format=%(refname:lstrip=3)", "refs/remotes/origin"]));
    remote.into_iter().filter(|b| b != "HEAD").collect()
}

pub fn remotes(run: &Runner, cwd: &str) -> usize {
    lines(git(run, cwd, &["remote"])).len()
}

/// Local branches with their last commit's time in seconds since the epoch, most recently committed first.
pub fn dated_branches(run: &Runner, cwd: &str) -> Vec<(String, Option<i64>)> {
    let refs = lines(git(run, cwd, &["for-each-ref", "--sort=-committerdate", "--format=%(committerdate:unix) %(refname:short)", "refs/heads"]));
    refs.into_iter().filter_map(|l| l.split_once(' ').map(|(at, b)| (b.to_string(), at.parse().ok()))).collect()
}

/// Tracked and untracked files, relative to `cwd`.
pub fn ls_files(run: &Runner, cwd: &str) -> Vec<String> {
    lines(git(run, cwd, &["ls-files", "--cached", "--others", "--exclude-standard"]))
}

/// Deletes the worktree's folder, uncommitted changes included; its branch stays.
pub fn remove_worktree(run: &Runner, repo: &str, path: &str) -> Result<(), String> {
    git_result_within(run, repo, &["worktree", "remove", "--force", path], LONG)
}

/// Commits that deleting `branch` would lose, or with no branch the detached HEAD of `cwd`: those no other branch, remote or tag holds.
pub fn lost_commits(run: &Runner, cwd: &str, branch: Option<&str>) -> usize {
    match branch {
        Some(b) => lost_branch_commits(run, cwd, &[b]),
        None => git(run, cwd, &["rev-list", "--count", "HEAD", "--not", "--branches", "--remotes", "--tags"]).and_then(|n| n.trim().parse().ok()).unwrap_or(0),
    }
}

/// The commits only `branches` hold, taken as one set: a commit two of them share counts once, and neither keeps it for the other.
pub fn lost_branch_commits(run: &Runner, cwd: &str, branches: &[&str]) -> usize {
    if branches.is_empty() {
        return 0;
    }
    let tips: Vec<String> = branches.iter().map(|b| format!("refs/heads/{b}")).collect();
    // `--exclude` matches the names `--branches` lists, which lack `refs/heads/`.
    let excludes: Vec<String> = branches.iter().map(|b| format!("--exclude={b}")).collect();
    let mut args = vec!["rev-list", "--count"];
    args.extend(tips.iter().map(String::as_str));
    args.push("--not");
    args.extend(excludes.iter().map(String::as_str));
    args.extend(["--branches", "--remotes", "--tags"]);
    git(run, cwd, &args).and_then(|n| n.trim().parse().ok()).unwrap_or(0)
}

/// Every commit a ref or HEAD reaches in `cwd`; `None` when git can't read it.
pub fn commit_count(run: &Runner, cwd: &str) -> Option<usize> {
    git(run, cwd, &["rev-list", "--count", "--all"]).and_then(|n| n.trim().parse().ok())
}

pub const INIT_BRANCH: &str = "main";

/// Makes the folder `dir` a repository on `INIT_BRANCH` with an empty first commit, so it has a HEAD to diff against.
/// The identity is given and hooks are skipped because the user's git config may have neither set up for this.
pub fn init_empty(run: &Runner, dir: &str) -> Result<(), String> {
    let commit = [
        "-c",
        "user.name=Anywhere",
        "-c",
        "user.email=anywhere@localhost",
        "-c",
        "commit.gpgsign=false",
        "commit",
        "-q",
        "--allow-empty",
        "--no-verify",
        "-m",
        "Start",
    ];
    let head = format!("refs/heads/{INIT_BRANCH}");
    // `git init -b` needs git 2.28, newer than some hosts have.
    for args in [&["init", "-q"][..], &["symbolic-ref", "HEAD", &head], &commit] {
        git_result(run, dir, args)?;
    }
    Ok(())
}

/// How many files in `cwd` differ from HEAD, untracked ones included; `None` when git can't read it.
pub fn dirty_files(run: &Runner, cwd: &str) -> Option<usize> {
    if checking_out(run, cwd) {
        return Some(0);
    }
    git(run, cwd, &["status", "--porcelain", "-uall"]).map(|out| out.lines().count())
}

/// Whether `branch` was once named `old`, per the entry `git branch -m` leaves in its reflog.
pub fn renamed_from(run: &Runner, cwd: &str, branch: &str, old: &str) -> bool {
    let moved = format!("Branch: renamed refs/heads/{old} to ");
    lines(git(run, cwd, &["log", "-g", "--format=%gs", &format!("refs/heads/{branch}"), "--"])).iter().any(|l| l.starts_with(&moved))
}

/// Deletes `branch`, merged or not; git refuses while a worktree has it checked out.
pub fn delete_branch(run: &Runner, repo: &str, branch: &str) -> Result<(), String> {
    git_result(run, repo, &["branch", "-D", "--", branch])
}

pub fn clone(run: &Runner, url: &str, dest: &str) -> Result<(), String> {
    git_result_within(run, "", &["clone", "--", url, dest], LONG)
}

fn with_paths<'a>(args: &[&'a str], paths: &'a [String]) -> Vec<&'a str> {
    args.iter().copied().chain(["--"]).chain(paths.iter().map(String::as_str)).collect()
}

pub fn set_staged(run: &Runner, cwd: &str, paths: &[String], staged: bool) {
    let args: &[&str] = if staged { &["add", "-A"] } else { &["reset", "-q"] };
    git(run, cwd, &with_paths(args, paths));
}

/// Throws away the unstaged edits to `paths`, deleting the untracked ones.
pub fn discard(run: &Runner, cwd: &str, paths: &[String]) {
    let untracked = lines(git(run, cwd, &with_paths(&["ls-files", "--others", "--exclude-standard"], paths)));
    let tracked: Vec<String> = paths.iter().filter(|p| !untracked.contains(p)).cloned().collect();
    if !untracked.is_empty() {
        git(run, cwd, &with_paths(&["clean", "-fq"], &untracked));
    }
    if !tracked.is_empty() {
        git(run, cwd, &with_paths(&["checkout", "-q"], &tracked));
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

/// [`PUSH`] to `remote` when given; with `lease`, a rewritten branch replaces the remote one unless it holds commits not yet fetched.
pub fn push_argv(remote: Option<&str>, lease: bool) -> Vec<String> {
    let pick = remote.map(|r| format!("remote.pushDefault={r}"));
    let (push, setup) = PUSH.split_last().unwrap();
    let mut argv: Vec<String> = setup.iter().map(|a| a.to_string()).collect();
    argv.extend(pick.into_iter().flat_map(|p| ["-c".to_string(), p]));
    argv.push(push.to_string());
    // `--force-if-includes` also refuses when a background fetch moved the remote-tracking ref past what we've seen.
    argv.extend(lease.then(|| ["--force-with-lease".to_string(), "--force-if-includes".to_string()]).into_iter().flatten());
    argv
}

/// The repository's remotes by name.
fn remote_names(run: &Runner, cwd: &str) -> Vec<String> {
    lines(git(run, cwd, &["remote"]))
}

const MAX_CONTEXT: usize = 60_000;

/// What a commit message is written from: recent subjects for style, then the diff to commit (only the staged part when `staged`, new files only with `untracked`).
pub fn commit_context(run: &Runner, cwd: &str, staged: bool, untracked: bool) -> String {
    let subjects = lines(git(run, cwd, &["log", "-n", "10", "--format=%s"])).join("\n");
    let diff = git(run, cwd, if staged { &["diff", "--cached"] } else { &["diff", "HEAD"] }).unwrap_or_default();
    let new = if staged || !untracked { Vec::new() } else { lines(git(run, cwd, &["ls-files", "--others", "--exclude-standard"])) };
    let mut out = format!("Recent commit subjects:\n{subjects}\n\nDiff:\n{diff}");
    out.extend(new.iter().map(|p| format!("\nNew file: {p}")));
    if out.len() > MAX_CONTEXT {
        out.truncate(out.floor_char_boundary(MAX_CONTEXT));
    }
    out
}

/// What a PR's title and description are written from: the commits on top of `base`, then their diff.
pub fn pr_context(run: &Runner, cwd: &str, base: &str) -> String {
    let log = git(run, cwd, &["log", "--format=- %s%n%b", &format!("{base}..HEAD")]).unwrap_or_default();
    let diff = git(run, cwd, &["diff", &format!("{base}...HEAD")]).unwrap_or_default();
    let mut out = format!("Commits:\n{}\n\nDiff:\n{diff}", log.trim());
    if out.len() > MAX_CONTEXT {
        out.truncate(out.floor_char_boundary(MAX_CONTEXT));
    }
    out
}

/// `path` where the branch left `base`, and at HEAD: what a PR shows for it.
pub fn blobs_between(run: &Runner, cwd: &str, base: &str, path: &str) -> Blobs {
    let at = |rev: &str| git_bytes(run, cwd, &["show", &format!("{rev}:{path}")]).unwrap_or_default();
    let fork = git(run, cwd, &["merge-base", base, "HEAD"]).map(|s| s.trim().to_string()).unwrap_or_default();
    (if fork.is_empty() { Vec::new() } else { at(&fork) }, at("HEAD"))
}

/// A file a commit changed; `old_path` is set for renames and copies.
#[derive(Clone, Debug, PartialEq)]
pub struct CommitFile {
    pub path: String,
    pub old_path: Option<String>,
    pub status: char,
}

pub fn first_parent(run: &Runner, cwd: &str, sha: &str) -> Option<String> {
    git(run, cwd, &["rev-parse", "--verify", "-q", &format!("{sha}^")]).map(|s| s.trim().to_string())
}

/// The files `sha` changed against `parent`, or every file it holds when it is a root commit.
pub fn commit_files(run: &Runner, cwd: &str, sha: &str, parent: Option<&str>) -> Vec<CommitFile> {
    let mut args = vec!["diff-tree", "-r", "-z", "-M", "--name-status", "--no-commit-id"];
    match parent {
        Some(p) => args.extend([p, sha]),
        None => args.extend(["--root", sha]),
    }
    changed_files(&git(run, cwd, &args).unwrap_or_default())
}

/// The files `--name-status -z` lists; `old_path` is set for renames and copies.
fn changed_files(out: &str) -> Vec<CommitFile> {
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
pub fn commit_texts(run: &Runner, cwd: &str, sha: &str, parent: Option<&str>, file: &CommitFile) -> (String, String) {
    lossy(commit_blobs(run, cwd, sha, parent, file))
}

fn commit_blobs(run: &Runner, cwd: &str, sha: &str, parent: Option<&str>, file: &CommitFile) -> Blobs {
    let show = |rev: &str, path: &str| git_bytes(run, cwd, &["show", &format!("{rev}:{path}")]).unwrap_or_default();
    let old = match parent {
        Some(p) if file.status != 'A' => show(p, file.old_path.as_deref().unwrap_or(&file.path)),
        _ => Vec::new(),
    };
    let new = if file.status == 'D' { Vec::new() } else { show(sha, &file.path) };
    (old, new)
}

/// The file at `path` before and after commit `sha`, following a rename back to the old name.
pub fn blobs_at(run: &Runner, cwd: &str, sha: &str, path: &str) -> Blobs {
    let parent = first_parent(run, cwd, sha);
    commit_files(run, cwd, sha, parent.as_deref())
        .into_iter()
        .find(|f| f.path == path)
        .map(|f| commit_blobs(run, cwd, sha, parent.as_deref(), &f))
        .unwrap_or_default()
}

pub fn short_sha(sha: &str) -> &str {
    &sha[..sha.len().min(7)]
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

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

    pub(crate) fn scratch_repo(tag: &str) -> std::path::PathBuf {
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
        let names: Vec<String> = worktrees(&Runner::Local, r).iter().map(|w| w.path.rsplit('/').next().unwrap().to_string()).collect();
        assert_eq!(names, ["repo", "zeta", "alpha", "mid"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_worktree_pocketd_marked_is_one_anywhere_made() {
        let dir = scratch_repo("made");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        for name in ["ours", "theirs"] {
            add_worktree(r, dir.join(name).to_str().unwrap(), name, "HEAD").unwrap();
        }
        std::fs::write(repo.join(".git/worktrees/ours/anywhere"), "").unwrap();
        let made: Vec<bool> = worktrees(&Runner::Local, r).iter().map(|w| w.made).collect();
        assert_eq!(made, [false, true, false]);
        let marker = std::fs::metadata(repo.join(".git/worktrees/ours/anywhere")).unwrap().modified().unwrap();
        let ms = marker.duration_since(UNIX_EPOCH).unwrap().as_millis() as u64;
        assert_eq!(made_at(&Runner::Local, dir.join("ours").to_str().unwrap()), Some(ms));
        assert_eq!(made_at(&Runner::Local, dir.join("theirs").to_str().unwrap()), None);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_host_that_is_not_connected_never_touches_the_same_path_here() {
        let dir = scratch_repo("offline");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        let offline = Runner::Remote(std::sync::Arc::new(NotConnected));
        assert_eq!(read(&offline, r, true), None);
        assert!(worktrees(&offline, r).is_empty());
        assert_eq!(texts(&offline, r, "a"), (String::new(), String::new()));
        assert!(remove_worktree(&offline, r, r).unwrap_err().contains("not connected"));
        assert!(repo.join("a").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A host that answers every command with success and keeps the timeout each was given.
    #[derive(Default)]
    struct Timeouts(std::sync::Mutex<Vec<Duration>>);

    impl Exec for Timeouts {
        fn exec(&self, _: &str, _: &[&str], _: Option<&[u8]>, timeout: Duration) -> std::io::Result<Output> {
            self.0.lock().unwrap().push(timeout);
            Ok(Output::default())
        }
        fn read(&self, _: &str, _: usize) -> std::io::Result<Vec<u8>> {
            Ok(Vec::new())
        }
        fn stat(&self, _: &str) -> std::io::Result<Option<Stat>> {
            Ok(None)
        }
        fn place(&self, path: &str) -> String {
            path.into()
        }
    }

    /// A host holding `files`, whose paths the desktop keys under `//box`.
    struct Files(std::collections::HashMap<&'static str, &'static str>);

    impl Exec for Files {
        fn exec(&self, _: &str, _: &[&str], _: Option<&[u8]>, _: Duration) -> std::io::Result<Output> {
            Ok(Output { stdout: self.0["worktree list"].into(), ..Output::default() })
        }
        fn read(&self, path: &str, _: usize) -> std::io::Result<Vec<u8>> {
            self.0.get(path).map(|f| f.as_bytes().to_vec()).ok_or_else(|| std::io::ErrorKind::NotFound.into())
        }
        fn stat(&self, path: &str) -> std::io::Result<Option<Stat>> {
            Ok(self.0.contains_key(path).then_some(Stat { dir: false, size: 0, created: None, modified: None }))
        }
        fn place(&self, path: &str) -> String {
            format!("//box{path}")
        }
    }

    #[test]
    fn a_hosts_worktree_is_one_anywhere_made_when_its_git_folder_there_holds_the_marker() {
        let host = Files(std::collections::HashMap::from([
            (
                "worktree list",
                "worktree /r\nbranch refs/heads/main\n\nworktree /w/ours\nbranch refs/heads/ours\n\nworktree /w/near\nbranch refs/heads/near\n\nworktree /w/theirs\nbranch refs/heads/theirs\n",
            ),
            ("//box/w/ours/.git", "gitdir: /r/.git/worktrees/ours\n"),
            ("//box/r/.git/worktrees/ours/anywhere", ""),
            ("//box/w/near/.git", "gitdir: ../../r/.git/worktrees/near\n"),
            ("//box/w/near/../../r/.git/worktrees/near/anywhere", ""),
            ("//box/w/theirs/.git", "gitdir: /r/.git/worktrees/theirs\n"),
        ]));
        let made: Vec<(String, bool)> = worktrees(&Runner::Remote(std::sync::Arc::new(host)), "//box/r").into_iter().map(|w| (w.path, w.made)).collect();
        assert_eq!(made, [("//box/r", false), ("//box/w/ours", true), ("//box/w/near", true), ("//box/w/theirs", false)].map(|(p, m)| (p.to_string(), m)));
    }

    #[test]
    fn a_clone_or_a_worktrees_removal_on_a_host_gets_longer_than_a_quick_command() {
        let host = std::sync::Arc::new(Timeouts::default());
        let run = Runner::Remote(host.clone());
        clone(&run, "https://x/r.git", "/home/me/r").unwrap();
        remove_worktree(&run, "/home/me/r", "/home/me/w").unwrap();
        delete_branch(&run, "/home/me/r", "fix").unwrap();
        assert_eq!(*host.0.lock().unwrap(), [LONG, LONG, TIMEOUT]);
    }

    #[test]
    fn a_file_over_the_read_limit_is_refused_not_cut_short() {
        let dir = scratch_repo("big");
        let file = dir.join("big.txt");
        std::fs::write(&file, "abcdef").unwrap();
        let path = file.to_str().unwrap();
        assert_eq!(Runner::Local.read(path, 6).unwrap(), b"abcdef");
        assert_eq!(Runner::Local.read(path, 5).unwrap_err().kind(), std::io::ErrorKind::FileTooLarge);
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
        remove_worktree(&Runner::Local, r, tree.to_str().unwrap()).unwrap();
        assert!(!tree.exists());
        assert!(branches(&Runner::Local, r).contains(&"fix".to_string()));
        assert_eq!(worktrees(&Runner::Local, r).len(), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn stages_unstages_and_discards() {
        let dir = scratch_repo("stage");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        std::fs::write(repo.join("a"), "edited\n").unwrap();
        std::fs::write(repo.join("new"), "fresh\n").unwrap();
        let flags = |path: &str| read(&Runner::Local, r, true).unwrap().files.iter().find(|f| f.path == path).map(|f| (f.staged, f.unstaged));
        assert_eq!((flags("a"), flags("new")), (Some((false, true)), Some((false, true))));
        set_staged(&Runner::Local, r, &["a".into(), "new".into()], true);
        assert_eq!((flags("a"), flags("new")), (Some((true, false)), Some((true, false))));
        std::fs::write(repo.join("a"), "edited twice\n").unwrap();
        assert_eq!(flags("a"), Some((true, true)));
        discard(&Runner::Local, r, &["a".into()]);
        assert_eq!(std::fs::read_to_string(repo.join("a")).unwrap(), "edited\n");
        set_staged(&Runner::Local, r, &["a".into(), "new".into()], false);
        assert_eq!(flags("a"), Some((false, true)));
        discard(&Runner::Local, r, &["a".into(), "new".into()]);
        assert!(read(&Runner::Local, r, true).unwrap().files.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn commit_context_holds_only_the_staged_diff_when_asked() {
        let dir = scratch_repo("context");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        std::fs::write(repo.join("a"), "staged\n").unwrap();
        set_staged(&Runner::Local, r, &["a".into()], true);
        std::fs::write(repo.join("b"), "loose\n").unwrap();
        let staged = commit_context(&Runner::Local, r, true, true);
        assert!(staged.starts_with("Recent commit subjects:\ninit\n"));
        assert!(staged.contains("+staged") && !staged.contains("New file: b"));
        assert!(commit_context(&Runner::Local, r, false, true).contains("New file: b"));
        assert!(!commit_context(&Runner::Local, r, false, false).contains("New file: b"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn run_argv(repo: &Path, argv: &[&str], input: &str) -> bool {
        use std::io::Write;
        let mut child = Command::new(argv[0]).args(&argv[1..]).current_dir(repo).stdin(std::process::Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();
        child.wait().unwrap().success()
    }

    pub(crate) fn committer(repo: &Path) {
        for (k, v) in [("user.name", "t"), ("user.email", "t@t"), ("commit.gpgsign", "false")] {
            assert!(Command::new("git").arg("-C").arg(repo).args(["config", k, v]).status().unwrap().success());
        }
    }

    /// A repo on a branch off its first one, with one commit and an uncommitted edit; returns it with the base's name.
    fn branched(tag: &str) -> (std::path::PathBuf, String) {
        let dir = scratch_repo(tag);
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        let base = git(&Runner::Local, r, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap().trim().to_string();
        git(&Runner::Local, r, &["checkout", "-qb", "cap-retries"]).unwrap();
        std::fs::write(repo.join("a"), "capped\n").unwrap();
        git(&Runner::Local, r, &["commit", "-qam", "Cap retries"]).unwrap();
        std::fs::write(repo.join("a"), "uncommitted\n").unwrap();
        (dir, base)
    }

    #[test]
    fn a_pr_is_written_from_the_branch_s_commits_and_their_diff() {
        let (dir, base) = branched("pr-context");
        let r = dir.join("repo");
        let context = pr_context(&Runner::Local, r.to_str().unwrap(), &base);
        assert!(context.starts_with("Commits:\n- Cap retries"), "{context}");
        assert!(context.contains("+capped") && !context.contains("uncommitted") && !context.contains("init"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_pr_shows_a_file_from_where_the_branch_left_its_base_to_head() {
        let (dir, base) = branched("between");
        let r = dir.join("repo");
        assert_eq!(blobs_between(&Runner::Local, r.to_str().unwrap(), &base, "a"), (b"a\n".to_vec(), b"capped\n".to_vec()));
        assert_eq!(blobs_between(&Runner::Local, r.to_str().unwrap(), &base, "missing"), (Vec::new(), Vec::new()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn unpushed_counts_commits_past_the_upstream_and_is_none_before_the_first_push() {
        let (dir, base) = branched("unpushed");
        let r = dir.join("repo");
        let r = r.to_str().unwrap();
        assert_eq!(read(&Runner::Local, r, false).unwrap().unpushed, None);
        git(&Runner::Local, r, &["branch", "--set-upstream-to", &base]).unwrap();
        assert_eq!(read(&Runner::Local, r, false).unwrap().unpushed, Some(1));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn commit_argv_commits_the_staged_files_with_the_message_from_stdin() {
        let dir = scratch_repo("commit");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        std::fs::write(repo.join("a"), "edited\n").unwrap();
        set_staged(&Runner::Local, r, &["a".into()], true);
        assert!(run_argv(&repo, &commit_argv(false, "Edit a"), "Edit a"));
        assert_eq!(lines(git(&Runner::Local, r, &["log", "--format=%s"])), ["Edit a", "init"]);
        assert!(read(&Runner::Local, r, true).unwrap().files.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn amending_without_a_message_keeps_the_last_one_and_with_one_replaces_it() {
        let dir = scratch_repo("amend");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        std::fs::write(repo.join("a"), "edited\n").unwrap();
        set_staged(&Runner::Local, r, &["a".into()], true);
        assert!(run_argv(&repo, &commit_argv(true, ""), ""));
        assert_eq!(lines(git(&Runner::Local, r, &["log", "--format=%s"])), ["init"]);
        assert!(read(&Runner::Local, r, true).unwrap().files.is_empty());
        assert!(run_argv(&repo, &commit_argv(true, "Start"), "Start"));
        assert_eq!(lines(git(&Runner::Local, r, &["log", "--format=%s"])), ["Start"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn the_first_push_sets_the_upstream() {
        let dir = scratch_repo("push");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        let remote = dir.join("remote.git");
        assert!(Command::new("git").args(["init", "-q", "--bare"]).arg(&remote).status().unwrap().success());
        assert!(git(&Runner::Local, r, &["remote", "add", "origin", remote.to_str().unwrap()]).is_some());
        assert!(git(&Runner::Local, r, &["switch", "-qc", "feature"]).is_some());
        assert!(run_argv(&repo, PUSH, ""));
        assert_eq!(git(&Runner::Local, r, &["rev-parse", "--abbrev-ref", "@{u}"]).unwrap().trim(), "origin/feature");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_branch_counts_ahead_of_origin_main_not_a_stale_local_main() {
        let dir = scratch_repo("stale-main");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        sh(&repo, &["branch", "-M", "main"]);
        sh(&repo, &["commit", "-q", "--allow-empty", "-m", "landed"]);
        sh(&repo, &["update-ref", "refs/remotes/origin/main", "HEAD"]);
        sh(&repo, &["reset", "-q", "--hard", "HEAD~1"]);
        sh(&repo, &["switch", "-qc", "feature", "--no-track", "origin/main"]);
        let got = read(&Runner::Local, r, true).unwrap();
        assert_eq!((got.base.as_deref(), got.ahead, got.behind, got.unpushed), (Some("origin/main"), 0, 0, None));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_worktree_still_checking_out_has_no_changes() {
        let dir = scratch_repo("checking-out");
        let tree = dir.join("tree");
        sh(&dir.join("repo"), &["worktree", "add", "-q", "--no-checkout", "-b", "fresh", tree.to_str().unwrap()]);
        std::fs::write(tree.join("a"), "a\n").unwrap();
        let got = read(&Runner::Local, tree.to_str().unwrap(), true).unwrap();
        assert_eq!((got.branch.as_str(), got.files), ("fresh", vec![]));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    /// A repo on master with origin's develop one commit past it.
    pub(crate) fn develop_ahead_of_master(tag: &str) -> std::path::PathBuf {
        let dir = scratch_repo(tag);
        let repo = dir.join("repo");
        committer(&repo);
        sh(&repo, &["branch", "-M", "master"]);
        sh(&repo, &["update-ref", "refs/remotes/origin/master", "HEAD"]);
        sh(&repo, &["commit", "-q", "--allow-empty", "-m", "developed"]);
        sh(&repo, &["update-ref", "refs/remotes/origin/develop", "HEAD"]);
        dir
    }

    #[test]
    fn a_branch_counts_against_the_branch_it_was_created_from() {
        let dir = develop_ahead_of_master("grew-from");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        sh(&repo, &["switch", "-qc", "feature", "--no-track", "origin/develop"]);
        let got = read(&Runner::Local, r, true).unwrap();
        assert_eq!((got.base.as_deref(), got.ahead, got.behind), (Some("origin/develop"), 0, 0));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn created_from_names_the_branch_without_its_ref_or_remote() {
        assert_eq!(created_from("branch: Created from origin/develop"), Some("develop"));
        assert_eq!(created_from("branch: Created from refs/remotes/origin/develop"), Some("develop"));
        assert_eq!(created_from("branch: Created from main"), Some("main"));
        assert_eq!(created_from("branch: Created from HEAD"), None);
        assert_eq!(created_from("commit (initial): init"), None);
    }

    #[test]
    fn a_branch_with_no_record_of_its_start_counts_against_origins_default_branch() {
        let dir = develop_ahead_of_master("origin-head");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        sh(&repo, &["symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/develop"]);
        sh(&repo, &["update-ref", "refs/heads/feature", "HEAD"]);
        sh(&repo, &["switch", "-q", "feature"]);
        let got = read(&Runner::Local, r, true).unwrap();
        assert_eq!((got.base.as_deref(), got.ahead, got.behind), (Some("origin/develop"), 0, 0));
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

    #[test]
    fn a_repository_without_commits_reads_as_one_on_its_branch() {
        let dir = std::env::temp_dir().join(format!("pocket-git-unborn-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let d = dir.to_str().unwrap();
        assert!(read(&Runner::Local, d, true).is_none());
        assert!(Command::new("git").arg("-C").arg(d).args(["init", "-q", "-b", "trunk"]).output().unwrap().status.success());
        std::fs::write(dir.join("a.txt"), "x\n").unwrap();
        let r = read(&Runner::Local, d, true).unwrap();
        assert_eq!(r.branch, "trunk");
        assert_eq!(r.files.iter().map(|f| f.path.as_str()).collect::<Vec<_>>(), ["a.txt"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn untracked_files_stay_out_when_asked() {
        let dir = scratch_repo("untracked");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        std::fs::write(repo.join("a"), "edited\n").unwrap();
        std::fs::write(repo.join("new"), "fresh\n").unwrap();
        let paths = |untracked| read(&Runner::Local, r, untracked).unwrap().files.into_iter().map(|f| f.path).collect::<Vec<_>>();
        assert_eq!((paths(true), paths(false)), (vec!["a".to_string(), "new".into()], vec!["a".to_string()]));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn pushing_with_a_remote_sends_the_branch_there() {
        let dir = scratch_repo("push-remote");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        for name in ["origin", "fork"] {
            let bare = dir.join(format!("{name}.git"));
            assert!(Command::new("git").args(["init", "-q", "--bare"]).arg(&bare).status().unwrap().success());
            assert!(git(&Runner::Local, r, &["remote", "add", name, bare.to_str().unwrap()]).is_some());
        }
        assert!(git(&Runner::Local, r, &["switch", "-qc", "feature"]).is_some());
        let argv = push_argv(Some("fork"), false);
        assert!(run_argv(&repo, &argv.iter().map(String::as_str).collect::<Vec<_>>(), ""));
        assert_eq!(git(&Runner::Local, r, &["rev-parse", "--abbrev-ref", "@{u}"]).unwrap().trim(), "fork/feature");
        assert_eq!(read(&Runner::Local, r, true).unwrap().remotes, ["fork", "origin"]);
        assert_eq!(push_argv(None, false), PUSH);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_rewritten_branch_pushes_with_lease_but_never_over_commits_not_yet_fetched() {
        let dir = scratch_repo("push-lease");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        let bare = dir.join("origin.git");
        assert!(Command::new("git").args(["init", "-q", "--bare"]).arg(&bare).status().unwrap().success());
        assert!(git(&Runner::Local, r, &["remote", "add", "origin", bare.to_str().unwrap()]).is_some());
        let push = |lease| run_argv(&repo, &push_argv(None, lease).iter().map(String::as_str).collect::<Vec<_>>(), "");
        assert!(push(true));
        assert!(git(&Runner::Local, r, &["commit", "-q", "--amend", "-m", "amended"]).is_some());
        assert!(!push(false));
        assert!(push(true));
        let other = dir.join("other");
        assert!(Command::new("git").args(["clone", "-q"]).arg(&bare).arg(&other).status().unwrap().success());
        committer(&other);
        let o = other.to_str().unwrap();
        assert!(git(&Runner::Local, o, &["commit", "-q", "--allow-empty", "-m", "theirs"]).is_some());
        assert!(git(&Runner::Local, o, &["push", "-q"]).is_some());
        assert!(git(&Runner::Local, r, &["commit", "-q", "--amend", "-m", "again"]).is_some());
        assert!(!push(true));
        assert!(git(&Runner::Local, r, &["fetch", "-q"]).is_some());
        assert!(!push(true), "fetched but not integrated");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    pub(crate) fn sh(repo: &Path, args: &[&str]) {
        assert!(Command::new("git").arg("-C").arg(repo).args(args).output().unwrap().status.success(), "git {args:?}");
    }

    pub(crate) fn sha(r: &str, rev: &str) -> String {
        git(&Runner::Local, r, &["rev-parse", rev]).unwrap().trim().to_string()
    }

    #[test]
    fn dated_branches_carry_each_branchs_last_commit_time_newest_first() {
        let dir = scratch_repo("dated");
        let repo = dir.join("repo");
        committer(&repo);
        let commit_at = |when: &str| {
            assert!(
                Command::new("git")
                    .arg("-C")
                    .arg(&repo)
                    .args(["commit", "-q", "--allow-empty", "-m", when])
                    .env("GIT_COMMITTER_DATE", when)
                    .status()
                    .unwrap()
                    .success()
            )
        };
        sh(&repo, &["branch", "-M", "main"]);
        sh(&repo, &["checkout", "-qb", "old"]);
        commit_at("@978307200 +0000");
        sh(&repo, &["checkout", "-q", "main"]);
        commit_at("@1000000000 +0000");
        let want = [("main".to_string(), Some(1000000000)), ("old".to_string(), Some(978307200))];
        assert_eq!(dated_branches(&Runner::Local, repo.to_str().unwrap()), want);
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
        let parent = first_parent(&Runner::Local, r, &head);
        assert_eq!(parent, Some(sha(r, "HEAD^")));
        let file = |path: &str, old: Option<&str>, status| CommitFile { path: path.into(), old_path: old.map(str::to_string), status };
        assert_eq!(commit_files(&Runner::Local, r, &head, parent.as_deref()), vec![file("b", Some("a"), 'R'), file("gone", None, 'D'), file("new", None, 'A')]);
        assert_eq!(blobs_at(&Runner::Local, r, &head, "b"), (b"one\ntwo\nthree\nfour\nfive\n".to_vec(), b"one\ntwo\nthree\nfour\nfive\nsix\n".to_vec()));
        assert_eq!(blobs_at(&Runner::Local, r, &head, "gone"), (b"x\n".to_vec(), Vec::new()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_root_commit_lists_every_file_as_added() {
        let dir = scratch_repo("root");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        let root = sha(r, "HEAD");
        assert_eq!(first_parent(&Runner::Local, r, &root), None);
        assert_eq!(commit_files(&Runner::Local, r, &root, None), vec![CommitFile { path: "a".into(), old_path: None, status: 'A' }]);
        assert_eq!(blobs_at(&Runner::Local, r, &root, "a"), (Vec::new(), b"a\n".to_vec()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn origin_branches_leave_out_head_and_remote_branches_those_already_local() {
        let dir = scratch_repo("remote-branches");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        sh(&repo, &["branch", "-M", "main"]);
        for b in ["main", "feat/x"] {
            sh(&repo, &["update-ref", &format!("refs/remotes/origin/{b}"), "HEAD"]);
        }
        sh(&repo, &["symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/main"]);
        assert_eq!(remote_branches(&Runner::Local, r), ["feat/x"]);
        assert_eq!(origin_branches(&Runner::Local, r), ["feat/x", "main"]);
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
        assert_eq!(lost_commits(&Runner::Local, r, Some("fix/a")), 0);
        commit_file(&tree, "b");
        commit_file(&tree, "c");
        assert_eq!(lost_commits(&Runner::Local, r, Some("fix/a")), 2);
        sh(&repo, &["update-ref", "refs/remotes/origin/fix/a", "fix/a~1"]);
        assert_eq!(lost_commits(&Runner::Local, r, Some("fix/a")), 1);
        sh(&repo, &["update-ref", "refs/remotes/origin/fix/a", "fix/a"]);
        assert_eq!(lost_commits(&Runner::Local, r, Some("fix/a")), 0);
        sh(&repo, &["update-ref", "-d", "refs/remotes/origin/fix/a"]);
        sh(&repo, &["merge", "-q", "--ff-only", "fix/a"]);
        assert_eq!(lost_commits(&Runner::Local, r, Some("fix/a")), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_commit_two_doomed_branches_share_is_lost_with_both() {
        let dir = scratch_repo("lost-set");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        sh(&repo, &["branch", "-M", "main"]);
        let tree = dir.join("feat");
        add_worktree(r, tree.to_str().unwrap(), "feat", "HEAD").unwrap();
        commit_file(&tree, "b");
        sh(&repo, &["branch", "child", "feat"]);
        assert_eq!([lost_commits(&Runner::Local, r, Some("feat")), lost_commits(&Runner::Local, r, Some("child"))], [0, 0]);
        assert_eq!(lost_branch_commits(&Runner::Local, r, &["feat", "child"]), 1);
        assert_eq!(lost_branch_commits(&Runner::Local, r, &[]), 0);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dirty_files_counts_edited_staged_and_each_untracked_file_but_not_ignored_ones() {
        let dir = scratch_repo("dirty");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        assert_eq!(dirty_files(&Runner::Local, r), Some(0));
        std::fs::write(repo.join("a"), "edited\n").unwrap();
        std::fs::write(repo.join("staged"), "s\n").unwrap();
        sh(&repo, &["add", "staged"]);
        std::fs::create_dir(repo.join("new")).unwrap();
        std::fs::write(repo.join("new/one"), "1\n").unwrap();
        std::fs::write(repo.join("new/two"), "2\n").unwrap();
        std::fs::write(repo.join(".git/info/exclude"), "ignored\n").unwrap();
        std::fs::write(repo.join("ignored"), "i\n").unwrap();
        assert_eq!(dirty_files(&Runner::Local, r), Some(4));
        assert_eq!(dirty_files(&Runner::Local, r), read(&Runner::Local, r, true).map(|got| got.files.len()));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn dirty_files_is_zero_while_a_worktree_checks_out_and_none_outside_a_repo() {
        let dir = scratch_repo("dirty-checkout");
        let tree = dir.join("tree");
        sh(&dir.join("repo"), &["worktree", "add", "-q", "--no-checkout", "-b", "fresh", tree.to_str().unwrap()]);
        std::fs::write(tree.join("a"), "a\n").unwrap();
        assert_eq!(dirty_files(&Runner::Local, tree.to_str().unwrap()), Some(0));
        assert_eq!(dirty_files(&Runner::Local, dir.join("gone").to_str().unwrap()), None);
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
        assert_eq!(lost_commits(&Runner::Local, t, None), 0);
        commit_file(&tree, "b");
        assert_eq!(lost_commits(&Runner::Local, t, None), 1);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_branch_deletes_once_its_worktree_is_removed() {
        let dir = scratch_repo("delete-branch");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        let tree = dir.join("fix");
        add_worktree(r, tree.to_str().unwrap(), "fix", "HEAD").unwrap();
        assert!(delete_branch(&Runner::Local, r, "fix").unwrap_err().contains("fix"));
        remove_worktree(&Runner::Local, r, tree.to_str().unwrap()).unwrap();
        delete_branch(&Runner::Local, r, "fix").unwrap();
        assert!(!branches(&Runner::Local, r).contains(&"fix".to_string()));
        assert!(delete_branch(&Runner::Local, r, "fix").is_err());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
