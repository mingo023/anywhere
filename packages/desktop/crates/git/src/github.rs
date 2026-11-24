use serde_json::Value;
use std::collections::HashMap;
use std::collections::hash_map::Entry;
use std::path::Path;
use std::process::Command;
use std::time::Duration;

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Pr {
    pub number: u32,
    pub title: String,
    pub url: String,
    pub state: PrState,
    pub draft: bool,
    pub checks: Checks,
    pub runs: Vec<Check>,
    pub review: Review,
    /// Each reviewer's latest verdict.
    pub reviews: Vec<(String, Review)>,
    pub requested: Vec<String>,
    pub mergeable: Mergeable,
    /// Branch protection holds the merge back.
    pub blocked: bool,
    pub base: String,
    pub head: String,
    pub threads: Vec<Thread>,
    pub body: String,
    pub author: String,
    /// When it was opened, in Unix milliseconds.
    pub opened: u64,
    pub labels: Vec<Label>,
    pub additions: u32,
    pub deletions: u32,
    pub commits: Vec<PrCommit>,
    pub files: Vec<PrFile>,
    /// What was said on the PR itself, outside any review.
    pub notes: Vec<Note>,
    /// Every review submitted, oldest first; a reply in a thread submits one with no body.
    pub submitted: Vec<Submitted>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Label {
    pub name: String,
    /// `0xRRGGBB`.
    pub color: u32,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrCommit {
    pub oid: String,
    pub subject: String,
    pub author: String,
    pub at: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct PrFile {
    pub path: String,
    pub additions: u32,
    pub deletions: u32,
}

/// A comment on the PR itself; one still being posted has no `id`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Note {
    pub id: String,
    pub author: String,
    pub body: String,
    pub at: u64,
    pub url: String,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Submitted {
    pub id: String,
    pub author: String,
    /// `Review::None` for a review that only commented.
    pub verdict: Review,
    pub body: String,
    pub at: u64,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum PrState {
    #[default]
    Open,
    Merged,
    Closed,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Checks {
    pub passed: u16,
    pub failed: u16,
    pub pending: u16,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Check {
    pub name: String,
    pub outcome: Outcome,
    /// Where its log is.
    pub url: String,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Passed,
    Failed,
    Pending,
    Skipped,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Review {
    #[default]
    None,
    Required,
    Approved,
    ChangesRequested,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Mergeable {
    /// GitHub is still working it out, as it does for a while after every push.
    #[default]
    Unknown,
    Clean,
    Conflicting,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Thread {
    pub id: String,
    pub path: String,
    pub line: Option<u32>,
    /// On the old side of the diff.
    pub left: bool,
    pub outdated: bool,
    pub resolved: bool,
    pub comments: Vec<Comment>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Comment {
    /// Empty while it is being posted.
    pub id: String,
    pub url: String,
    /// The review it was submitted in, which [`Submitted::id`] names.
    pub review: Option<String>,
    pub author: String,
    pub body: String,
    /// Unix milliseconds.
    pub at: u64,
    pub hunk: String,
}

/// Why `gh` gave no answer, as opposed to a branch having no PR.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GhProblem {
    Missing,
    LoggedOut,
    /// Any other failure, such as a rate limit or a network error, which the next poll may not hit.
    Failed,
}

const VIEW_FIELDS: &str = "number,title,url,state,isDraft,statusCheckRollup,reviewDecision,latestReviews,reviewRequests,mergeable,mergeStateStatus,headRefOid,baseRefName,body,author,createdAt,labels,additions,deletions,commits,files,comments,reviews";

/// The branch name on the remote, without `origin/`.
pub fn base_branch(base: &str) -> &str {
    base.strip_prefix("origin/").unwrap_or(base)
}

/// Opens a PR for the pushed, checked-out branch into `base`, its body read from stdin.
pub fn create_argv(base: &str, title: &str, draft: bool) -> Vec<String> {
    let mut argv: Vec<String> = ["gh", "pr", "create", "--base", base_branch(base), "--title", title, "--body-file", "-"].map(String::from).to_vec();
    if draft {
        argv.push("--draft".into());
    }
    argv
}

/// The PR's number in what [`create_argv`] printed: the new PR's URL.
pub fn created(out: &str) -> Option<u32> {
    out.lines().rev().find_map(|l| l.trim().rsplit_once("/pull/")?.1.parse().ok())
}

/// A title and body an agent wrote: the first line, without a heading mark, then the rest.
pub fn split_message(text: &str) -> (String, String) {
    let text = text.trim();
    let (title, body) = text.split_once('\n').unwrap_or((text, ""));
    (title.trim().trim_start_matches('#').trim().to_string(), body.trim().to_string())
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Method {
    #[default]
    Squash,
    Merge,
    Rebase,
}

impl Method {
    pub const ALL: [Method; 3] = [Method::Squash, Method::Merge, Method::Rebase];

    pub fn label(self) -> &'static str {
        match self {
            Method::Squash => "Squash and merge",
            Method::Merge => "Create a merge commit",
            Method::Rebase => "Rebase and merge",
        }
    }
}

/// Merges PR `number`, leaving its branch alone: the worktree that has it checked out goes separately.
pub fn merge_argv(number: u32, method: Method) -> Vec<String> {
    let flag = match method {
        Method::Squash => "--squash",
        Method::Merge => "--merge",
        Method::Rebase => "--rebase",
    };
    vec!["gh".into(), "pr".into(), "merge".into(), number.to_string(), flag.into()]
}

pub fn ready_argv(number: u32) -> Vec<String> {
    vec!["gh".into(), "pr".into(), "ready".into(), number.to_string()]
}

const THREADS: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100){nodes{id isResolved isOutdated path line originalLine diffSide comments(first:50){nodes{id url author{login} body createdAt diffHunk pullRequestReview{id}}}}}}}}";
const REPLY: &str = "mutation($id:ID!,$body:String!){addPullRequestReviewThreadReply(input:{pullRequestReviewThreadId:$id,body:$body}){comment{id}}}";
const RESOLVE: &str = "mutation($id:ID!){resolveReviewThread(input:{threadId:$id}){thread{isResolved}}}";
const UNRESOLVE: &str = "mutation($id:ID!){unresolveReviewThread(input:{threadId:$id}){thread{isResolved}}}";

/// PR `number`'s review threads; `gh` fills in the owner and name of the repo it runs in.
pub fn threads_argv(number: u32) -> Vec<String> {
    let number = format!("number={number}");
    let query = format!("query={THREADS}");
    ["gh", "api", "graphql", "-F", "owner={owner}", "-F", "name={repo}", "-F", &number, "-f", &query].map(String::from).to_vec()
}

pub fn reply_argv(thread: &str, body: &str) -> Vec<String> {
    let (id, body, query) = (format!("id={thread}"), format!("body={body}"), format!("query={REPLY}"));
    ["gh", "api", "graphql", "-f", &id, "-f", &body, "-f", &query].map(String::from).to_vec()
}

/// Comments on PR `number`, the body read from stdin.
pub fn comment_argv(number: u32) -> Vec<String> {
    ["gh", "pr", "comment", &number.to_string(), "--body-file", "-"].map(String::from).to_vec()
}

/// Everyone the PR shows a face for: who opened it, reviewed it, commented on it or wrote a commit in it.
pub fn people(pr: &Pr) -> Vec<&str> {
    let mut all: Vec<&str> = [pr.author.as_str()]
        .into_iter()
        .chain(pr.reviews.iter().map(|(who, _)| who.as_str()))
        .chain(pr.requested.iter().map(String::as_str))
        .chain(pr.submitted.iter().map(|s| s.author.as_str()))
        .chain(pr.notes.iter().map(|n| n.author.as_str()))
        .chain(pr.threads.iter().flat_map(|t| t.comments.iter().map(|c| c.author.as_str())))
        .chain(pr.commits.iter().map(|c| c.author.as_str()))
        .filter(|login| !login.is_empty())
        .collect();
    all.sort_unstable();
    all.dedup();
    all
}

/// `login`'s picture on the host the PR at `pr_url` is on; `None` for what can't be a login, like a commit author's plain name.
pub fn avatar_url(pr_url: &str, login: &str) -> Option<String> {
    let host = pr_url.splitn(4, '/').take(3).collect::<Vec<_>>().join("/");
    let login_like = !login.is_empty() && login.chars().all(|c| c.is_ascii_alphanumeric() || c == '-');
    (login_like && host.starts_with("https://")).then(|| format!("{host}/{login}.png?size=80"))
}

const AVATAR_KEPT: Duration = Duration::from_secs(7 * 24 * 3600);

/// Saves the picture at `url` to `to`, unless a copy younger than a week is there; an older copy stays when the download fails. Whether `to` holds one now.
pub fn fetch_avatar(url: &str, to: &Path) -> bool {
    let fresh = std::fs::metadata(to).and_then(|m| m.modified()).is_ok_and(|at| at.elapsed().is_ok_and(|age| age < AVATAR_KEPT));
    if fresh {
        return true;
    }
    let part = to.with_extension("part");
    let got = Command::new("curl").args(["-fsSL", "--max-time", "15", "-o"]).arg(&part).arg(url).status().is_ok_and(|s| s.success());
    (got && std::fs::rename(&part, to).is_ok()) || to.exists()
}

pub fn resolve_argv(thread: &str, resolved: bool) -> Vec<String> {
    let (id, query) = (format!("id={thread}"), format!("query={}", if resolved { RESOLVE } else { UNRESOLVE }));
    ["gh", "api", "graphql", "-f", &id, "-f", &query].map(String::from).to_vec()
}

/// Reads what [`threads_argv`] printed.
pub fn threads_result(json: &str) -> Option<Vec<Thread>> {
    let v: Value = serde_json::from_str(json).ok()?;
    let nodes = v["data"]["repository"]["pullRequest"]["reviewThreads"]["nodes"].as_array()?;
    Some(
        nodes
            .iter()
            .map(|t| Thread {
                id: str(&t["id"]),
                path: str(&t["path"]),
                line: t["line"].as_u64().or_else(|| t["originalLine"].as_u64()).and_then(|l| u32::try_from(l).ok()),
                left: t["diffSide"].as_str() == Some("LEFT"),
                outdated: t["isOutdated"].as_bool().unwrap_or(false),
                resolved: t["isResolved"].as_bool().unwrap_or(false),
                comments: t["comments"]["nodes"]
                    .as_array()
                    .into_iter()
                    .flatten()
                    .map(|c| Comment {
                        id: str(&c["id"]),
                        url: str(&c["url"]),
                        review: c["pullRequestReview"]["id"].as_str().map(str::to_string),
                        author: author(&c["author"]),
                        body: str(&c["body"]),
                        at: stamp(&c["createdAt"]),
                        hunk: str(&c["diffHunk"]),
                    })
                    .collect(),
            })
            .collect(),
    )
}

/// Why the merge button is off, the first thing that holds it back.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Gate {
    Ready,
    Done,
    Draft,
    Conflicts,
    Checking,
    ChecksFailed,
    ChecksRunning,
    ChangesRequested,
    ReviewRequired,
    Blocked,
}

impl Pr {
    pub fn gate(&self) -> Gate {
        match () {
            _ if self.state != PrState::Open => Gate::Done,
            _ if self.draft => Gate::Draft,
            _ if self.mergeable == Mergeable::Conflicting => Gate::Conflicts,
            _ if self.checks.failed > 0 => Gate::ChecksFailed,
            _ if self.checks.pending > 0 => Gate::ChecksRunning,
            _ if self.review == Review::ChangesRequested => Gate::ChangesRequested,
            _ if self.review == Review::Required => Gate::ReviewRequired,
            _ if self.mergeable == Mergeable::Unknown => Gate::Checking,
            _ if self.blocked => Gate::Blocked,
            _ => Gate::Ready,
        }
    }

    pub fn unresolved(&self) -> impl Iterator<Item = &Thread> {
        self.threads.iter().filter(|t| !t.resolved)
    }

    pub fn thread_mut(&mut self, id: &str) -> Option<&mut Thread> {
        self.threads.iter_mut().find(|t| t.id == id)
    }
}

/// How far a review thread has got.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ThreadState {
    Open,
    Replied,
    Sent,
    Resolved,
}

impl Thread {
    /// `sent` when it was handed to an agent since the last push.
    pub fn state(&self, sent: bool) -> ThreadState {
        let first = self.comments.first().map(|c| &c.author);
        match () {
            _ if self.resolved => ThreadState::Resolved,
            _ if sent => ThreadState::Sent,
            _ if self.comments.last().map(|c| &c.author) != first => ThreadState::Replied,
            _ => ThreadState::Open,
        }
    }
}

/// Whether `gh` is installed and signed in to github.com.
#[derive(Clone, Debug, PartialEq)]
pub enum GhStatus {
    Missing,
    SignedOut { version: String },
    /// `login` is the active one of `accounts`.
    Ready { version: String, login: String, accounts: Vec<String> },
}

/// `gh --version`, then `gh auth status` with stderr merged into its output.
pub const VERSION: &[&str] = &["gh", "--version"];
pub const AUTH: &[&str] = &["sh", "-c", "gh auth status --hostname github.com 2>&1"];

/// Reads what [`VERSION`] and [`AUTH`] returned through `daemon::run_login`.
pub fn status(version: Result<String, String>, auth: Result<String, String>) -> GhStatus {
    let Some(version) = version.ok().and_then(|v| v.split_whitespace().nth(2).map(str::to_string)) else {
        return GhStatus::Missing;
    };
    match auth.ok().map(|a| accounts(&a)) {
        Some((accounts, Some(login))) => GhStatus::Ready { version, login, accounts },
        _ => GhStatus::SignedOut { version },
    }
}

/// Every github.com account and the active one: newer gh lists each and marks one active, older gh says "as <login>".
fn accounts(auth: &str) -> (Vec<String>, Option<String>) {
    let (mut all, mut active) = (Vec::new(), None);
    for line in auth.lines().map(str::trim) {
        if let Some(rest) = line.split("Logged in to github.com account ").nth(1).or_else(|| line.split("Logged in to github.com as ").nth(1)) {
            all.extend(rest.split_whitespace().next().map(str::to_string));
            if line.contains(" as ") {
                active = all.last().cloned();
            }
        } else if line.contains("Active account: true") {
            active = all.last().cloned();
        }
    }
    (all, active)
}

/// The token `gh` keeps for `login`, which `GH_TOKEN` takes to act as that account.
pub fn token_argv(login: &str) -> [&str; 7] {
    ["gh", "auth", "token", "--hostname", "github.com", "--user", login]
}

/// `argv` run as `login` with the token `gh` keeps on the machine running it, so the token never leaves there; as the active account when it has none.
pub fn as_account(login: &str, argv: &[&str]) -> Vec<String> {
    let set = "t=$(gh auth token --hostname github.com --user \"$0\") && export GH_TOKEN=\"$t\"; exec \"$@\"";
    ["sh", "-c", set, login].into_iter().chain(argv.iter().copied()).map(str::to_string).collect()
}

/// GitHub refused the token, as it does one revoked by signing in again.
pub fn unauthorized(err: &str) -> bool {
    err.contains("HTTP 401")
}

/// What the account may do in the repo of the working directory.
pub const PERMISSION: &[&str] = &["gh", "repo", "view", "--json", "viewerPermission"];

/// Reads what [`PERMISSION`] returned: higher may do more, 0 can't see the repo.
pub fn permission(out: Result<String, String>) -> u8 {
    let v: Option<Value> = out.ok().and_then(|json| serde_json::from_str(&json).ok());
    match v.as_ref().and_then(|v| v["viewerPermission"].as_str()) {
        Some("ADMIN") => 5,
        Some("MAINTAIN") => 4,
        Some("WRITE") => 3,
        Some("TRIAGE") => 2,
        Some("READ") => 1,
        _ => 0,
    }
}

/// The account with the most access to a repo, the active one on a tie; none when no account can see it.
pub fn best<'a>(ranks: &[(&'a str, u8)], active: &str) -> Option<&'a str> {
    ranks.iter().filter(|(_, r)| *r > 0).max_by_key(|(login, r)| (*r, *login == active)).map(|(login, _)| *login)
}

/// The PR for the branch checked out where `run` runs `gh`: its number by branch, then the rest by number.
/// By branch, `gh` nests every field in a search of 30 PRs, so asking for them all there costs 33 GraphQL points instead of 2.
pub fn view(run: impl Fn(&[String]) -> Result<String, String>) -> Result<Option<Pr>, GhProblem> {
    let number = match run(&["gh", "pr", "view", "--json", "number"].map(String::from)) {
        Ok(json) => serde_json::from_str::<Value>(&json).ok().and_then(|v| u32::try_from(v["number"].as_u64()?).ok()).ok_or(GhProblem::Failed)?,
        Err(e) => return view_result(Err(e)),
    };
    view_result(run(&["gh", "pr", "view", &number.to_string(), "--json", VIEW_FIELDS].map(String::from)))
}

/// Reads what `gh pr view` returned through `daemon::run_login`: `Ok(None)` only when `gh` found no PR for the branch.
pub fn view_result(out: Result<String, String>) -> Result<Option<Pr>, GhProblem> {
    match out {
        Ok(json) => parse(&json).map(Some).ok_or(GhProblem::Failed),
        // The login shell reports a missing command: zsh and bash say "command not found", fish "Unknown command".
        Err(e) if e.contains("command not found") || e.contains("Unknown command") => Err(GhProblem::Missing),
        Err(e) if e.contains("gh auth login") => Err(GhProblem::LoggedOut),
        Err(e) if e.contains("no pull requests found") => Ok(None),
        Err(_) => Err(GhProblem::Failed),
    }
}

fn parse(json: &str) -> Option<Pr> {
    let v: Value = serde_json::from_str(json).ok()?;
    let state = match v["state"].as_str()? {
        "MERGED" => PrState::Merged,
        "CLOSED" => PrState::Closed,
        _ => PrState::Open,
    };
    let runs = runs(&v["statusCheckRollup"]);
    let verdict = |s: &str| match s {
        "APPROVED" => Review::Approved,
        "CHANGES_REQUESTED" => Review::ChangesRequested,
        "REVIEW_REQUIRED" => Review::Required,
        _ => Review::None,
    };
    Some(Pr {
        number: u32::try_from(v["number"].as_u64()?).ok()?,
        title: v["title"].as_str()?.to_string(),
        url: v["url"].as_str()?.to_string(),
        state,
        draft: v["isDraft"].as_bool().unwrap_or(false),
        checks: runs.iter().fold(Checks::default(), |mut c, r| {
            match r.outcome {
                Outcome::Failed => c.failed += 1,
                Outcome::Pending => c.pending += 1,
                Outcome::Passed | Outcome::Skipped => c.passed += 1,
            }
            c
        }),
        runs,
        review: verdict(v["reviewDecision"].as_str().unwrap_or("")),
        reviews: v["latestReviews"].as_array().into_iter().flatten().map(|r| (author(&r["author"]), verdict(r["state"].as_str().unwrap_or("")))).collect(),
        requested: v["reviewRequests"].as_array().into_iter().flatten().filter_map(|r| r["login"].as_str().or(r["slug"].as_str()).or(r["name"].as_str()).map(str::to_string)).collect(),
        mergeable: match v["mergeable"].as_str() {
            Some("MERGEABLE") => Mergeable::Clean,
            Some("CONFLICTING") => Mergeable::Conflicting,
            _ => Mergeable::Unknown,
        },
        blocked: v["mergeStateStatus"].as_str() == Some("BLOCKED"),
        base: str(&v["baseRefName"]),
        head: str(&v["headRefOid"]),
        threads: Vec::new(),
        body: str(&v["body"]),
        author: author(&v["author"]),
        opened: stamp(&v["createdAt"]),
        labels: list(&v["labels"]).map(|l| Label { name: str(&l["name"]), color: u32::from_str_radix(l["color"].as_str().unwrap_or(""), 16).unwrap_or(0x8b8b94) }).collect(),
        additions: count(&v["additions"]),
        deletions: count(&v["deletions"]),
        commits: list(&v["commits"])
            .map(|c| PrCommit { oid: str(&c["oid"]), subject: str(&c["messageHeadline"]), author: c["authors"][0]["login"].as_str().or(c["authors"][0]["name"].as_str()).unwrap_or("").to_string(), at: stamp(&c["authoredDate"]) })
            .collect(),
        files: list(&v["files"]).map(|f| PrFile { path: str(&f["path"]), additions: count(&f["additions"]), deletions: count(&f["deletions"]) }).collect(),
        notes: list(&v["comments"]).map(|c| Note { id: str(&c["id"]), author: author(&c["author"]), body: str(&c["body"]), at: stamp(&c["createdAt"]), url: str(&c["url"]) }).collect(),
        submitted: list(&v["reviews"])
            .map(|r| Submitted { id: str(&r["id"]), author: author(&r["author"]), verdict: verdict(r["state"].as_str().unwrap_or("")), body: str(&r["body"]), at: stamp(&r["submittedAt"]) })
            .collect(),
    })
}

fn list(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array().into_iter().flatten()
}

fn count(v: &Value) -> u32 {
    v.as_u64().and_then(|n| u32::try_from(n).ok()).unwrap_or(0)
}

fn stamp(v: &Value) -> u64 {
    epoch_ms(v.as_str().unwrap_or("")).unwrap_or(0)
}

fn str(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

/// An author's login; a deleted account has none.
fn author(author: &Value) -> String {
    author["login"].as_str().unwrap_or("ghost").to_string()
}

/// Which of a check's runs came later.
#[derive(Clone, Copy)]
struct Arrival {
    /// An Actions job's run id, then its job id.
    ids: Option<(u64, u64)>,
    started: u64,
}

impl Arrival {
    /// Runs queued together can start in either order, so ids decide when both have them: run ids follow the events, and a re-run keeps its run id under a new job id.
    fn not_before(self, other: Self) -> bool {
        match (self.ids, other.ids) {
            (Some(a), Some(b)) => a >= b,
            _ => self.started >= other.started,
        }
    }
}

/// The run and job ids in an Actions job's URL.
fn actions_ids(url: &str) -> Option<(u64, u64)> {
    let (run, job) = url.split_once("/actions/runs/")?.1.split_once("/job/")?;
    Some((run.parse().ok()?, job.split(|c: char| !c.is_ascii_digit()).next()?.parse().ok()?))
}

/// The latest run of each check: re-running one, as a label change does, adds a run on the same head and leaves the old one in the rollup.
fn runs(rollup: &Value) -> Vec<Check> {
    let mut runs = Vec::new();
    let mut latest: HashMap<(String, String), (usize, Arrival)> = HashMap::new();
    for check in rollup.as_array().into_iter().flatten() {
        let field = |k: &str| check[k].as_str().unwrap_or("");
        // Commit statuses carry `state`; check runs carry `status` and `conclusion`.
        let (outcome, name, url) = if field("__typename") == "StatusContext" {
            let outcome = match field("state") {
                "FAILURE" | "ERROR" => Outcome::Failed,
                "PENDING" | "EXPECTED" => Outcome::Pending,
                _ => Outcome::Passed,
            };
            (outcome, field("context"), field("targetUrl"))
        } else {
            let outcome = match field("conclusion") {
                "FAILURE" | "TIMED_OUT" | "CANCELLED" | "ACTION_REQUIRED" | "STARTUP_FAILURE" => Outcome::Failed,
                _ if field("status") != "COMPLETED" => Outcome::Pending,
                "SKIPPED" | "NEUTRAL" => Outcome::Skipped,
                _ => Outcome::Passed,
            };
            (outcome, field("name"), field("detailsUrl"))
        };
        // A queued run has no start yet and is the newest.
        let arrival = Arrival { ids: actions_ids(url), started: epoch_ms(field("startedAt")).unwrap_or(u64::MAX) };
        let run = Check { name: name.to_string(), outcome, url: url.to_string() };
        match latest.entry((field("workflowName").to_string(), name.to_string())) {
            Entry::Occupied(mut e) if arrival.not_before(e.get().1) => {
                runs[e.get().0] = run;
                e.get_mut().1 = arrival;
            }
            Entry::Occupied(_) => {}
            Entry::Vacant(e) => {
                e.insert((runs.len(), arrival));
                runs.push(run);
            }
        }
    }
    runs
}

/// `2026-10-02T18:37:02Z` as Unix milliseconds.
fn epoch_ms(stamp: &str) -> Option<u64> {
    let (date, time) = stamp.strip_suffix('Z')?.split_once('T')?;
    let num = |s: &str| s.parse::<i64>().ok();
    let mut d = date.splitn(3, '-').map(num);
    let (y, m, day) = (d.next()??, d.next()??, d.next()??);
    let mut t = time.splitn(3, ':').map(|s| num(s.split('.').next().unwrap_or(s)));
    let (h, min, s) = (t.next()??, t.next()??, t.next()??);
    // Days from the civil date, after Howard Hinnant's `days_from_civil`.
    let y = if m <= 2 { y - 1 } else { y };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + day - 1;
    let days = era * 146_097 + yoe * 365 + yoe / 4 - yoe / 100 + doy - 719_468;
    u64::try_from(((days * 24 + h) * 60 + min) * 60 + s).ok().map(|secs| secs * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_avatar_comes_from_the_prs_own_host_and_only_for_a_login() {
        assert_eq!(avatar_url("https://github.com/acme/app/pull/7", "rina-k").as_deref(), Some("https://github.com/rina-k.png?size=80"));
        assert_eq!(avatar_url("https://ghe.corp.io/acme/app/pull/7", "rina").as_deref(), Some("https://ghe.corp.io/rina.png?size=80"));
        assert_eq!(avatar_url("https://github.com/acme/app/pull/7", "Rina Kato"), None);
        assert_eq!(avatar_url("https://github.com/acme/app/pull/7", "../x"), None);
        assert_eq!(avatar_url("", "rina"), None);
    }

    #[test]
    fn people_counts_each_login_once_and_skips_none() {
        let pr = Pr {
            author: "mingo".into(),
            reviews: vec![("rina".into(), Review::Approved)],
            requested: vec!["tom".into()],
            notes: vec![Note { author: "rina".into(), ..Default::default() }],
            commits: vec![PrCommit { author: String::new(), ..Default::default() }],
            ..Default::default()
        };
        assert_eq!(people(&pr), ["mingo", "rina", "tom"]);
    }

    const OPEN: &str = r#"{"isDraft":false,"number":42,"state":"OPEN","title":"Fix the flaky snapshot","url":"https://github.com/acme/app/pull/42","statusCheckRollup":[
        {"__typename":"CheckRun","name":"build","status":"COMPLETED","conclusion":"SUCCESS"},
        {"__typename":"CheckRun","name":"label","status":"COMPLETED","conclusion":"SKIPPED"},
        {"__typename":"CheckRun","name":"test","status":"COMPLETED","conclusion":"FAILURE"},
        {"__typename":"CheckRun","name":"e2e","status":"COMPLETED","conclusion":"TIMED_OUT"},
        {"__typename":"CheckRun","name":"lint","status":"IN_PROGRESS","conclusion":""},
        {"__typename":"StatusContext","context":"ci/circleci","state":"PENDING"},
        {"__typename":"StatusContext","context":"deploy","state":"ERROR"},
        {"__typename":"StatusContext","context":"vercel","state":"SUCCESS"}
    ]}"#;

    #[test]
    fn an_open_pr_counts_its_checks_by_outcome() {
        let pr = view_result(Ok(OPEN.into())).unwrap().unwrap();
        assert_eq!((pr.number, pr.title.as_str(), pr.url.as_str(), pr.state, pr.draft), (42, "Fix the flaky snapshot", "https://github.com/acme/app/pull/42", PrState::Open, false));
        assert_eq!(pr.checks, Checks { passed: 3, failed: 3, pending: 2 });
        let outcomes: Vec<_> = pr.runs.iter().map(|r| (r.name.as_str(), r.outcome)).collect();
        assert_eq!(outcomes[..3], [("build", Outcome::Passed), ("label", Outcome::Skipped), ("test", Outcome::Failed)]);
        assert_eq!(outcomes[5..], [("ci/circleci", Outcome::Pending), ("deploy", Outcome::Failed), ("vercel", Outcome::Passed)]);
    }

    /// `gh pr view` on cli/cli#14580, cut to three checks.
    const CLI: &str = r#"{"baseRefName":"bagtoad/artifact-skill-docs","headRefOid":"5f85ec828f6d2fe28d81ba71a61b8a2181edfb6a","isDraft":false,"latestReviews":[{"author":{"login":"babakks"},"state":"APPROVED","body":"LGTM","submittedAt":"2026-10-02T19:00:00Z"},{"author":{"login":"andyfeller"},"state":"CHANGES_REQUESTED","body":"","submittedAt":"2026-10-02T19:01:00Z"}],"mergeStateStatus":"CLEAN","mergeable":"MERGEABLE","number":14580,"reviewDecision":"APPROVED","reviewRequests":[{"__typename":"User","login":"williammartin"},{"__typename":"Team","name":"CLI","slug":"cli"}],"state":"OPEN","statusCheckRollup":[
        {"__typename":"CheckRun","completedAt":"2026-10-02T18:37:02Z","conclusion":"SKIPPED","detailsUrl":"https://github.com/cli/cli/actions/runs/37048637304/job/110976263673","name":"label-external","startedAt":"2026-10-02T18:37:12Z","status":"COMPLETED","workflowName":"PR Triaging"},
        {"__typename":"CheckRun","completedAt":"2026-10-02T18:39:08Z","conclusion":"SUCCESS","detailsUrl":"https://github.com/cli/cli/actions/runs/37048540763/job/110975885942","name":"CodeQL-Build","startedAt":"2026-10-02T18:36:14Z","status":"COMPLETED","workflowName":"Code Scanning"},
        {"__typename":"CheckRun","completedAt":"0001-01-01T00:00:00Z","conclusion":"","detailsUrl":"https://github.com/cli/cli/actions/runs/1/job/2","name":"lint","startedAt":"2026-10-02T18:35:50Z","status":"IN_PROGRESS","workflowName":"Lint"}
    ],"title":"`gh issue artifact` stack 10/11: Add `edit --restore-version`","url":"https://github.com/cli/cli/pull/14580"}"#;

    #[test]
    fn an_open_pr_lists_each_check_with_its_log() {
        let pr = view_result(Ok(CLI.into())).unwrap().unwrap();
        let build = &pr.runs[1];
        assert_eq!((build.name.as_str(), build.outcome), ("CodeQL-Build", Outcome::Passed));
        assert_eq!(build.url, "https://github.com/cli/cli/actions/runs/37048540763/job/110975885942");
        assert_eq!((pr.runs[0].outcome, pr.runs[2].outcome), (Outcome::Skipped, Outcome::Pending));
    }

    #[test]
    fn an_open_pr_reads_its_review_and_mergeability() {
        let pr = view_result(Ok(CLI.into())).unwrap().unwrap();
        assert_eq!((pr.review, pr.mergeable, pr.blocked), (Review::Approved, Mergeable::Clean, false));
        assert_eq!(pr.reviews, [("babakks".to_string(), Review::Approved), ("andyfeller".to_string(), Review::ChangesRequested)]);
        assert_eq!(pr.requested, ["williammartin", "cli"]);
        assert_eq!((pr.base.as_str(), pr.head.as_str()), ("bagtoad/artifact-skill-docs", "5f85ec828f6d2fe28d81ba71a61b8a2181edfb6a"));
    }

    /// `gh pr view` on mingo023/anywhere#53 after adding a label re-ran `type-label` on the same head.
    const RERUN: &str = r#"{"isDraft":false,"number":53,"state":"OPEN","title":"t","url":"u","statusCheckRollup":[
        {"__typename":"CheckRun","completedAt":"2026-10-14T14:44:07Z","conclusion":"FAILURE","detailsUrl":"https://github.com/mingo023/anywhere/actions/runs/38583145528/job/115918020001","name":"type-label","startedAt":"2026-10-14T14:44:04Z","status":"COMPLETED","workflowName":"PR type label"},
        {"__typename":"CheckRun","completedAt":"2026-10-14T14:45:01Z","conclusion":"SUCCESS","detailsUrl":"https://github.com/mingo023/anywhere/actions/runs/38583280097/job/115918493199","name":"type-label","startedAt":"2026-10-14T14:44:58Z","status":"COMPLETED","workflowName":"PR type label"},
        {"__typename":"CheckRun","completedAt":"0001-01-01T00:00:00Z","conclusion":"","detailsUrl":"","name":"build","startedAt":"2026-10-14T14:44:04Z","status":"COMPLETED","workflowName":"CI"},
        {"__typename":"CheckRun","completedAt":"0001-01-01T00:00:00Z","conclusion":"","detailsUrl":"","name":"build","startedAt":"0001-01-01T00:00:00Z","status":"QUEUED","workflowName":"CI"},
        {"__typename":"CheckRun","completedAt":"2026-10-14T14:44:07Z","conclusion":"FAILURE","detailsUrl":"","name":"build","startedAt":"2026-10-14T14:44:04Z","status":"COMPLETED","workflowName":"Release"}
    ]}"#;

    #[test]
    fn a_rerun_check_counts_only_its_latest_run() {
        let pr = view_result(Ok(RERUN.into())).unwrap().unwrap();
        let runs: Vec<_> = pr.runs.iter().map(|r| (r.name.as_str(), r.outcome)).collect();
        assert_eq!(runs, [("type-label", Outcome::Passed), ("build", Outcome::Pending), ("build", Outcome::Failed)]);
        assert_eq!(pr.runs[0].url, "https://github.com/mingo023/anywhere/actions/runs/38583280097/job/115918493199");
        assert_eq!(pr.checks, Checks { passed: 1, failed: 1, pending: 1 });
    }

    #[test]
    fn a_run_created_later_wins_even_when_the_earlier_one_started_after_it() {
        let pr = view_result(Ok(r#"{"isDraft":false,"number":210,"state":"OPEN","title":"t","url":"u","statusCheckRollup":[
            {"__typename":"CheckRun","conclusion":"FAILURE","detailsUrl":"https://github.com/mingo023/anywhere/actions/runs/42742056810/job/129585252967","name":"type-label","startedAt":"2026-11-10T02:20:22Z","status":"COMPLETED","workflowName":"PR type label"},
            {"__typename":"CheckRun","conclusion":"SUCCESS","detailsUrl":"https://github.com/mingo023/anywhere/actions/runs/42742057451/job/129585251826","name":"type-label","startedAt":"2026-11-10T02:20:21Z","status":"COMPLETED","workflowName":"PR type label"}
        ]}"#.into())).unwrap().unwrap();
        assert_eq!(pr.checks, Checks { passed: 1, failed: 0, pending: 0 });
    }

    #[test]
    fn a_rerun_job_beats_the_attempt_it_replaced() {
        let pr = view_result(Ok(r#"{"isDraft":false,"number":1,"state":"OPEN","title":"t","url":"u","statusCheckRollup":[
            {"__typename":"CheckRun","conclusion":"SUCCESS","detailsUrl":"https://github.com/o/r/actions/runs/5/job/9?pr=1","name":"test","startedAt":"2026-11-10T02:20:21Z","status":"COMPLETED","workflowName":"CI"},
            {"__typename":"CheckRun","conclusion":"FAILURE","detailsUrl":"https://github.com/o/r/actions/runs/5/job/7?pr=1","name":"test","startedAt":"2026-11-10T02:20:25Z","status":"COMPLETED","workflowName":"CI"}
        ]}"#.into())).unwrap().unwrap();
        assert_eq!(pr.checks, Checks { passed: 1, failed: 0, pending: 0 });
    }

    #[test]
    fn timestamps_read_as_unix_milliseconds() {
        assert_eq!(epoch_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(epoch_ms("2026-10-02T18:37:02Z"), Some(1_790_966_222_000));
        assert_eq!(epoch_ms("2024-02-29T12:00:00.5Z"), Some(1_709_208_000_000));
        assert_eq!(epoch_ms("yesterday"), None);
    }

    const THREADS_OUT: &str = r#"{"data":{"repository":{"pullRequest":{"reviewThreads":{"nodes":[
        {"id":"PRRT_a","isResolved":false,"isOutdated":false,"path":"src/sync.rs","line":42,"originalLine":40,"diffSide":"RIGHT","comments":{"nodes":[
            {"id":"PRRC_1","url":"https://github.com/acme/app/pull/7#discussion_r1","author":{"login":"rina"},"body":"This retries forever.","createdAt":"2026-10-02T18:37:02Z","diffHunk":"@@ -38,3 +38,5 @@\n fn sync() {\n+    loop {\n+        retry();","pullRequestReview":{"id":"PRR_1"}},
            {"author":{"login":"mingo"},"body":"Capped at 5.","createdAt":"2026-10-02T18:40:00Z","diffHunk":"@@"}
        ]}},
        {"id":"PRRT_b","isResolved":true,"isOutdated":true,"path":"README.md","line":null,"originalLine":7,"diffSide":"LEFT","comments":{"nodes":[
            {"author":null,"body":"Typo","createdAt":"2026-10-02T18:37:02Z","diffHunk":"@@"}
        ]}}
    ]}}}}}"#;

    #[test]
    fn review_threads_fall_back_to_their_original_line() {
        let threads = threads_result(THREADS_OUT).unwrap();
        assert_eq!((threads[0].id.as_str(), threads[0].path.as_str(), threads[0].line, threads[0].left, threads[0].resolved), ("PRRT_a", "src/sync.rs", Some(42), false, false));
        let first = Comment {
            id: "PRRC_1".into(),
            url: "https://github.com/acme/app/pull/7#discussion_r1".into(),
            review: Some("PRR_1".into()),
            author: "rina".into(),
            body: "This retries forever.".into(),
            at: 1_790_966_222_000,
            hunk: "@@ -38,3 +38,5 @@\n fn sync() {\n+    loop {\n+        retry();".into(),
        };
        assert_eq!(threads[0].comments[0], first);
        assert_eq!(threads[0].comments[1].review, None);
        assert_eq!((threads[1].line, threads[1].left, threads[1].outdated, threads[1].resolved), (Some(7), true, true, true));
        assert_eq!(threads[1].comments[0].author, "ghost");
        assert_eq!(threads_result(r#"{"errors":[{"message":"Could not resolve"}]}"#), None);
    }

    /// `gh pr view` on cli/cli#15083, cut to one of each and a label added.
    const DETAIL: &str = r#"{"number":15083,"state":"OPEN","title":"t","url":"u","isDraft":false,"additions":407,"deletions":62,
        "author":{"id":"MDQ6","is_bot":false,"login":"babakks","name":"Babak K. Shandiz"},"body":"<!--\nThank you\n-->\n## Summary","createdAt":"2026-11-17T17:44:23Z",
        "labels":[{"id":"LA_1","name":"enhancement","description":"","color":"a2eeef"},{"id":"LA_2","name":"odd","description":"","color":"nope"}],
        "commits":[{"authoredDate":"2026-11-17T17:22:38Z","authors":[{"email":"b@github.com","login":"babakks","name":"Babak K. Shandiz"},{"login":"Copilot","name":"Copilot"}],"messageBody":"When…","messageHeadline":"fix(eventstream): split a buffered line after a skipped LF","oid":"4be67a7009cd25fb24704b462e8a7f5dabba13f0"}],
        "files":[{"additions":10,"changeType":"MODIFIED","deletions":1,"path":"evals/src/commands.test.ts"}],
        "comments":[{"author":{"login":"babakks"},"body":"The latest automated review comments","createdAt":"2026-11-17T19:02:41Z","id":"IC_kwDODKw3uc8AAAABnrngRg","url":"https://github.com/cli/cli/pull/15083#issuecomment-6957948998"}],
        "reviews":[{"author":{"login":"copilot-pull-request-reviewer"},"body":"Changes recommended","id":"PRR_kwDODKw3uc8AAAABYkanbw","state":"COMMENTED","submittedAt":"2026-11-17T17:55:04Z"},
            {"author":{"login":"BagToad"},"body":"","id":"PRR_kwDODKw3uc8AAAABYlVB6Q","state":"APPROVED","submittedAt":"2026-11-17T18:59:36Z"}]}"#;

    #[test]
    fn a_pr_reads_its_description_author_labels_size_commits_and_files() {
        let pr = view_result(Ok(DETAIL.into())).unwrap().unwrap();
        assert_eq!((pr.author.as_str(), pr.opened, pr.additions, pr.deletions), ("babakks", 1_794_937_463_000, 407, 62));
        assert!(pr.body.ends_with("## Summary"));
        assert_eq!(pr.labels, [Label { name: "enhancement".into(), color: 0xa2eeef }, Label { name: "odd".into(), color: 0x8b8b94 }]);
        let commit = &pr.commits[0];
        assert_eq!((commit.oid.as_str(), commit.subject.as_str(), commit.author.as_str(), commit.at), ("4be67a7009cd25fb24704b462e8a7f5dabba13f0", "fix(eventstream): split a buffered line after a skipped LF", "babakks", 1_794_936_158_000));
        assert_eq!(pr.files, [PrFile { path: "evals/src/commands.test.ts".into(), additions: 10, deletions: 1 }]);
    }

    #[test]
    fn a_pr_reads_its_comments_and_every_submitted_review() {
        let pr = view_result(Ok(DETAIL.into())).unwrap().unwrap();
        let note = &pr.notes[0];
        assert_eq!((note.id.as_str(), note.author.as_str(), note.url.as_str()), ("IC_kwDODKw3uc8AAAABnrngRg", "babakks", "https://github.com/cli/cli/pull/15083#issuecomment-6957948998"));
        let verdicts: Vec<_> = pr.submitted.iter().map(|r| (r.id.as_str(), r.author.as_str(), r.verdict, r.body.as_str())).collect();
        assert_eq!(verdicts, [("PRR_kwDODKw3uc8AAAABYkanbw", "copilot-pull-request-reviewer", Review::None, "Changes recommended"), ("PRR_kwDODKw3uc8AAAABYlVB6Q", "BagToad", Review::Approved, "")]);
    }

    #[test]
    fn a_pr_without_the_new_fields_still_reads() {
        let pr = view_result(Ok(RERUN.into())).unwrap().unwrap();
        assert_eq!((pr.body.as_str(), pr.author.as_str(), pr.opened, pr.labels.len(), pr.commits.len(), pr.notes.len()), ("", "ghost", 0, 0, 0, 0));
    }

    #[test]
    fn a_comment_on_the_pr_reads_its_body_from_stdin() {
        assert_eq!(comment_argv(283), ["gh", "pr", "comment", "283", "--body-file", "-"]);
    }

    #[test]
    fn a_thread_reads_as_replied_once_someone_else_answers() {
        let threads = threads_result(THREADS_OUT).unwrap();
        let alone = Thread { comments: threads[0].comments[..1].to_vec(), ..threads[0].clone() };
        assert_eq!([alone.state(false), threads[0].state(false), alone.state(true), threads[1].state(true)], [ThreadState::Open, ThreadState::Replied, ThreadState::Sent, ThreadState::Resolved]);
    }

    #[test]
    fn merged_closed_and_draft_prs_keep_their_state() {
        let pr = |json: &str| view_result(Ok(json.into())).unwrap().unwrap();
        let merged = pr(r#"{"isDraft":false,"number":7,"state":"MERGED","title":"t","url":"u","statusCheckRollup":[]}"#);
        let closed = pr(r#"{"isDraft":false,"number":8,"state":"CLOSED","title":"t","url":"u","statusCheckRollup":[]}"#);
        let draft = pr(r#"{"isDraft":true,"number":9,"state":"OPEN","title":"t","url":"u","statusCheckRollup":[]}"#);
        assert_eq!((merged.state, closed.state, draft.state, draft.draft), (PrState::Merged, PrState::Closed, PrState::Open, true));
        assert_eq!(merged.checks, Checks::default());
    }

    #[test]
    fn a_missing_gh_differs_from_a_logged_out_one() {
        for missing in ["zsh:1: command not found: gh", "bash: line 1: gh: command not found", "fish: Unknown command: gh"] {
            assert_eq!(view_result(Err(missing.into())), Err(GhProblem::Missing), "{missing}");
        }
        let logged_out = "To get started with GitHub CLI, please run:  gh auth login\nAlternatively, populate the GH_TOKEN environment variable with a GitHub API authentication token.";
        assert_eq!(view_result(Err(logged_out.into())), Err(GhProblem::LoggedOut));
    }

    #[test]
    fn only_gh_finding_no_pr_means_the_branch_has_none() {
        assert_eq!(view_result(Err("no pull requests found for branch \"fix/restore-handoff\"".into())), Ok(None));
        for failure in ["GraphQL: API rate limit already exceeded for user ID 1234.", "fatal: not a git repository (or any of the parent directories): .git", "error connecting to api.github.com"] {
            assert_eq!(view_result(Err(failure.into())), Err(GhProblem::Failed), "{failure}");
        }
        assert_eq!(view_result(Ok("not json".into())), Err(GhProblem::Failed));
    }

    #[test]
    fn create_argv_targets_the_base_without_its_remote() {
        assert_eq!(create_argv("origin/main", "Fix it", false), ["gh", "pr", "create", "--base", "main", "--title", "Fix it", "--body-file", "-"]);
        assert_eq!(create_argv("develop", "Fix it", false)[4], "develop");
    }

    #[test]
    fn a_draft_pr_is_created_as_a_draft() {
        assert_eq!(create_argv("main", "t", true).last().map(String::as_str), Some("--draft"));
    }

    #[test]
    fn the_created_pr_is_read_from_its_url() {
        assert_eq!(created("Warning: 1 uncommitted change\nhttps://github.com/acme/app/pull/57\n"), Some(57));
        assert_eq!(created("pull request create failed"), None);
    }

    #[test]
    fn an_agent_s_reply_splits_into_a_title_and_a_body() {
        assert_eq!(split_message("\n# Cap sync retries\n\nRetries stop after 5.\n- Adds a test\n"), ("Cap sync retries".into(), "Retries stop after 5.\n- Adds a test".into()));
        assert_eq!(split_message("Only a title"), ("Only a title".into(), String::new()));
    }

    #[test]
    fn merging_never_deletes_the_branch() {
        assert_eq!(merge_argv(7, Method::Squash), ["gh", "pr", "merge", "7", "--squash"]);
        assert_eq!(merge_argv(7, Method::Rebase)[4], "--rebase");
        assert_eq!(merge_argv(7, Method::Merge)[4], "--merge");
        assert_eq!(ready_argv(7), ["gh", "pr", "ready", "7"]);
    }

    #[test]
    fn thread_calls_pass_their_text_verbatim() {
        let threads = threads_argv(57);
        assert_eq!(threads[..9], ["gh", "api", "graphql", "-F", "owner={owner}", "-F", "name={repo}", "-F", "number=57"]);
        assert!(threads[10].starts_with("query=query($owner"));
        let reply = reply_argv("PRRT_a", "@rina done\nsee 1a2b");
        assert_eq!(reply[..7], ["gh", "api", "graphql", "-f", "id=PRRT_a", "-f", "body=@rina done\nsee 1a2b"]);
        assert!(resolve_argv("PRRT_a", true)[6].contains("resolveReviewThread") && resolve_argv("PRRT_a", false)[6].contains("unresolveReviewThread"));
    }

    #[test]
    fn a_pr_merges_only_once_every_gate_is_green() {
        let ready = Pr { mergeable: Mergeable::Clean, checks: Checks { passed: 2, ..Checks::default() }, review: Review::Approved, ..Pr::default() };
        assert_eq!(ready.gate(), Gate::Ready);
        let cases = [
            (Pr { draft: true, ..ready.clone() }, Gate::Draft),
            (Pr { mergeable: Mergeable::Conflicting, checks: Checks { failed: 1, ..Checks::default() }, ..ready.clone() }, Gate::Conflicts),
            (Pr { checks: Checks { failed: 1, pending: 1, passed: 0 }, ..ready.clone() }, Gate::ChecksFailed),
            (Pr { checks: Checks { pending: 1, ..Checks::default() }, ..ready.clone() }, Gate::ChecksRunning),
            (Pr { review: Review::ChangesRequested, ..ready.clone() }, Gate::ChangesRequested),
            (Pr { review: Review::Required, ..ready.clone() }, Gate::ReviewRequired),
            (Pr { blocked: true, ..ready.clone() }, Gate::Blocked),
            (Pr { state: PrState::Merged, ..ready.clone() }, Gate::Done),
        ];
        for (pr, gate) in cases {
            assert_eq!(pr.gate(), gate);
        }
    }

    #[test]
    fn an_unknown_mergeability_never_reads_as_no_conflicts() {
        let pr = Pr { mergeable: Mergeable::Unknown, review: Review::Approved, ..Pr::default() };
        assert_eq!(pr.gate(), Gate::Checking);
    }

    const VERSION_OUT: &str = "gh version 2.81.0 (2025-10-01)\nhttps://github.com/cli/cli/releases/tag/v2.81.0\n";

    fn ready(login: &str, accounts: &[&str]) -> GhStatus {
        GhStatus::Ready { version: "2.81.0".into(), login: login.into(), accounts: accounts.iter().map(|a| a.to_string()).collect() }
    }

    #[test]
    fn gh_status_reads_the_version_every_account_and_the_active_one() {
        let auth = "github.com\n  ✓ Logged in to github.com account old-me (keyring)\n  - Active account: false\n  ✓ Logged in to github.com account minh-ngo (keyring)\n  - Active account: true\n  - Git operations protocol: https\n";
        assert_eq!(status(Ok(VERSION_OUT.into()), Ok(auth.into())), ready("minh-ngo", &["old-me", "minh-ngo"]));
        let old = "github.com\n  ✓ Logged in to github.com as minh-ngo (oauth_token)\n";
        assert_eq!(status(Ok(VERSION_OUT.into()), Ok(old.into())), ready("minh-ngo", &["minh-ngo"]));
    }

    #[test]
    fn gh_status_lists_three_accounts_with_the_first_active() {
        let auth = "github.com\n  ✓ Logged in to github.com account mingo023 (keyring)\n  - Active account: true\n  - Git operations protocol: https\n  - Token: gho_************************************\n  - Token scopes: 'gist', 'read:org', 'repo', 'workflow'\n\n  ✓ Logged in to github.com account hodfords-minhngo-be (keyring)\n  - Active account: false\n  - Git operations protocol: https\n  - Token: gho_************************************\n\n  ✓ Logged in to github.com account minhngovan-regask (keyring)\n  - Active account: false\n";
        assert_eq!(status(Ok(VERSION_OUT.into()), Ok(auth.into())), ready("mingo023", &["mingo023", "hodfords-minhngo-be", "minhngovan-regask"]));
    }

    #[cfg(unix)]
    #[test]
    fn a_command_as_an_account_takes_its_token_from_gh_there_and_keeps_its_args_and_stdin() {
        let bin = std::env::temp_dir().join(format!("gh-as-{}", std::process::id()));
        std::fs::create_dir_all(&bin).unwrap();
        let gh = bin.join("gh");
        std::fs::write(&gh, "#!/bin/sh\n[ \"$*\" = \"auth token --hostname github.com --user work\" ] && echo tok-work || exit 1\n").unwrap();
        std::fs::set_permissions(&gh, std::os::unix::fs::PermissionsExt::from_mode(0o755)).unwrap();
        let run = |login: &str| {
            let argv = as_account(login, &["sh", "-c", "printf '%s|%s|' \"$GH_TOKEN\" \"$1\"; cat", "-", "it's \"quoted\""]);
            let path = format!("{}:{}", bin.display(), std::env::var("PATH").unwrap());
            let mut child = Command::new(&argv[0]).args(&argv[1..]).env("PATH", path).env_remove("GH_TOKEN").stdin(std::process::Stdio::piped()).stdout(std::process::Stdio::piped()).spawn().unwrap();
            std::io::Write::write_all(&mut child.stdin.take().unwrap(), b"body").unwrap();
            String::from_utf8(child.wait_with_output().unwrap().stdout).unwrap()
        };
        assert_eq!(run("work"), "tok-work|it's \"quoted\"|body");
        assert_eq!(run("gone"), "|it's \"quoted\"|body");
        std::fs::remove_dir_all(&bin).unwrap();
    }

    #[test]
    fn a_repos_permission_ranks_admin_highest_and_no_access_as_none() {
        let read = |p: &str| permission(Ok(format!(r#"{{"viewerPermission":"{p}"}}"#)));
        assert_eq!(["ADMIN", "MAINTAIN", "WRITE", "TRIAGE", "READ"].map(read), [5, 4, 3, 2, 1]);
        assert_eq!(permission(Err("GraphQL: Could not resolve to a Repository with the name 'acme/app'. (repository)".into())), 0);
    }

    #[test]
    fn the_account_with_the_most_access_wins_and_the_active_one_breaks_a_tie() {
        assert_eq!(best(&[("work", 3), ("me", 5), ("client", 0)], "work"), Some("me"));
        assert_eq!(best(&[("work", 1), ("me", 1)], "me"), Some("me"));
        assert_eq!(best(&[("work", 1), ("me", 1)], "work"), Some("work"));
        assert_eq!(best(&[("work", 0), ("me", 0)], "me"), None);
    }

    #[test]
    fn a_pr_is_found_by_its_number_alone_then_read_whole_by_number() {
        let calls = std::cell::RefCell::new(Vec::new());
        let gh = |find: Result<&str, &str>, read: Result<&str, &str>| {
            calls.borrow_mut().clear();
            view(|argv| {
                calls.borrow_mut().push(argv.to_vec());
                let out = if calls.borrow().len() == 1 { find } else { read };
                out.map(str::to_string).map_err(str::to_string)
            })
        };
        let pr = r#"{"number":42,"title":"Fix it","url":"https://github.com/acme/app/pull/42","state":"MERGED"}"#;
        let found = gh(Ok(r#"{"number":42}"#), Ok(pr)).unwrap().unwrap();
        assert_eq!((found.number, found.state), (42, PrState::Merged));
        let calls_made = calls.borrow().clone();
        assert_eq!(calls_made[0], ["gh", "pr", "view", "--json", "number"]);
        assert_eq!(calls_made[1][..4], ["gh", "pr", "view", "42"]);
        assert!(calls_made[1].iter().any(|a| a.contains("commits")));

        assert_eq!(gh(Err("no pull requests found for branch \"fix\""), Ok(pr)), Ok(None));
        assert_eq!(calls.borrow().len(), 1);
        assert_eq!(gh(Ok(r#"{"number":42}"#), Err("GraphQL: API rate limit already exceeded for user ID 1.")), Err(GhProblem::Failed));
        assert_eq!(gh(Err("zsh:1: command not found: gh"), Ok(pr)), Err(GhProblem::Missing));
    }

    #[test]
    fn gh_status_tells_a_missing_gh_from_a_signed_out_one() {
        assert_eq!(status(Err("zsh:1: command not found: gh".into()), Err(String::new())), GhStatus::Missing);
        let signed_out = "You are not logged into any GitHub hosts. To log in, run: gh auth login\n";
        assert_eq!(status(Ok(VERSION_OUT.into()), Err(signed_out.into())), GhStatus::SignedOut { version: "2.81.0".into() });
    }
}
