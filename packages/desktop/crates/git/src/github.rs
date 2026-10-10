use serde_json::Value;

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
    /// How long it ran, once it finished.
    pub secs: Option<u64>,
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
    pub author: String,
    pub body: String,
    /// Unix milliseconds.
    pub at: u64,
    pub hunk: String,
}

/// Why `gh` can't be asked at all, as opposed to a branch having no PR.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum GhProblem {
    Missing,
    LoggedOut,
}

/// The PR for the branch checked out in the working directory.
pub const VIEW: &[&str] = &[
    "gh",
    "pr",
    "view",
    "--json",
    "number,title,url,state,isDraft,statusCheckRollup,reviewDecision,latestReviews,reviewRequests,mergeable,mergeStateStatus,headRefOid,baseRefName",
];

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

const THREADS: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){reviewThreads(first:100){nodes{id isResolved isOutdated path line originalLine diffSide comments(first:50){nodes{author{login} body createdAt diffHunk}}}}}}}";
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
                    .map(|c| Comment { author: author(&c["author"]), body: str(&c["body"]), at: epoch_ms(c["createdAt"].as_str().unwrap_or("")).unwrap_or(0), hunk: str(&c["diffHunk"]) })
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
    Ready { version: String, login: String },
}

/// `gh --version`, then `gh auth status` with stderr merged into its output.
pub const VERSION: &[&str] = &["gh", "--version"];
pub const AUTH: &[&str] = &["sh", "-c", "gh auth status --hostname github.com 2>&1"];

/// Reads what [`VERSION`] and [`AUTH`] returned through `daemon::run_login`.
pub fn status(version: Result<String, String>, auth: Result<String, String>) -> GhStatus {
    let Some(version) = version.ok().and_then(|v| v.split_whitespace().nth(2).map(str::to_string)) else {
        return GhStatus::Missing;
    };
    match auth.ok().and_then(|a| login(&a)) {
        Some(login) => GhStatus::Ready { version, login },
        None => GhStatus::SignedOut { version },
    }
}

/// The active github.com account: newer gh lists every account and marks one active, older gh says "as <login>".
fn login(auth: &str) -> Option<String> {
    let mut found = None;
    for line in auth.lines().map(str::trim) {
        if let Some(rest) = line.split("Logged in to github.com account ").nth(1).or_else(|| line.split("Logged in to github.com as ").nth(1)) {
            found = rest.split_whitespace().next().map(str::to_string);
            if line.contains(" as ") {
                return found;
            }
        } else if line.contains("Active account: true") && found.is_some() {
            return found;
        }
    }
    None
}

/// Reads what [`VIEW`] returned through `daemon::run_login`. No PR, or any other failure, is `Ok(None)`.
pub fn view_result(out: Result<String, String>) -> Result<Option<Pr>, GhProblem> {
    match out {
        Ok(json) => Ok(parse(&json)),
        // The login shell reports a missing command: zsh and bash say "command not found", fish "Unknown command".
        Err(e) if e.contains("command not found") || e.contains("Unknown command") => Err(GhProblem::Missing),
        Err(e) if e.contains("gh auth login") => Err(GhProblem::LoggedOut),
        Err(_) => Ok(None),
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
    })
}

fn str(v: &Value) -> String {
    v.as_str().unwrap_or("").to_string()
}

/// An author's login; a deleted account has none.
fn author(author: &Value) -> String {
    author["login"].as_str().unwrap_or("ghost").to_string()
}

fn runs(rollup: &Value) -> Vec<Check> {
    let mut runs = Vec::new();
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
        // GitHub can stamp a skipped run as completing before it started.
        let secs = (outcome != Outcome::Pending).then(|| Some(epoch_ms(field("completedAt"))?.saturating_sub(epoch_ms(field("startedAt"))?) / 1000)).flatten();
        runs.push(Check { name: name.to_string(), outcome, url: url.to_string(), secs });
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
    fn an_open_pr_lists_each_check_with_its_duration_and_log() {
        let pr = view_result(Ok(CLI.into())).unwrap().unwrap();
        let build = &pr.runs[1];
        assert_eq!((build.name.as_str(), build.outcome, build.secs), ("CodeQL-Build", Outcome::Passed, Some(174)));
        assert_eq!(build.url, "https://github.com/cli/cli/actions/runs/37048540763/job/110975885942");
        assert_eq!((pr.runs[2].outcome, pr.runs[2].secs), (Outcome::Pending, None));
    }

    #[test]
    fn a_check_that_completes_before_it_starts_lasts_no_time() {
        let pr = view_result(Ok(CLI.into())).unwrap().unwrap();
        assert_eq!((pr.runs[0].outcome, pr.runs[0].secs), (Outcome::Skipped, Some(0)));
    }

    #[test]
    fn an_open_pr_reads_its_review_and_mergeability() {
        let pr = view_result(Ok(CLI.into())).unwrap().unwrap();
        assert_eq!((pr.review, pr.mergeable, pr.blocked), (Review::Approved, Mergeable::Clean, false));
        assert_eq!(pr.reviews, [("babakks".to_string(), Review::Approved), ("andyfeller".to_string(), Review::ChangesRequested)]);
        assert_eq!(pr.requested, ["williammartin", "cli"]);
        assert_eq!((pr.base.as_str(), pr.head.as_str()), ("bagtoad/artifact-skill-docs", "5f85ec828f6d2fe28d81ba71a61b8a2181edfb6a"));
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
            {"author":{"login":"rina"},"body":"This retries forever.","createdAt":"2026-10-02T18:37:02Z","diffHunk":"@@ -38,3 +38,5 @@\n fn sync() {\n+    loop {\n+        retry();"},
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
        assert_eq!(threads[0].comments[0], Comment { author: "rina".into(), body: "This retries forever.".into(), at: 1_790_966_222_000, hunk: "@@ -38,3 +38,5 @@\n fn sync() {\n+    loop {\n+        retry();".into() });
        assert_eq!((threads[1].line, threads[1].left, threads[1].outdated, threads[1].resolved), (Some(7), true, true, true));
        assert_eq!(threads[1].comments[0].author, "ghost");
        assert_eq!(threads_result(r#"{"errors":[{"message":"Could not resolve"}]}"#), None);
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
    fn a_branch_without_a_pr_or_a_failing_gh_has_none() {
        assert_eq!(view_result(Err("no pull requests found for branch \"fix/restore-handoff\"".into())), Ok(None));
        assert_eq!(view_result(Err("fatal: not a git repository (or any of the parent directories): .git".into())), Ok(None));
        assert_eq!(view_result(Ok("not json".into())), Ok(None));
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

    #[test]
    fn gh_status_reads_the_version_and_the_active_account() {
        let auth = "github.com\n  ✓ Logged in to github.com account old-me (keyring)\n  - Active account: false\n  ✓ Logged in to github.com account minh-ngo (keyring)\n  - Active account: true\n  - Git operations protocol: https\n";
        assert_eq!(status(Ok(VERSION_OUT.into()), Ok(auth.into())), GhStatus::Ready { version: "2.81.0".into(), login: "minh-ngo".into() });
        let old = "github.com\n  ✓ Logged in to github.com as minh-ngo (oauth_token)\n";
        assert_eq!(status(Ok(VERSION_OUT.into()), Ok(old.into())), GhStatus::Ready { version: "2.81.0".into(), login: "minh-ngo".into() });
    }

    #[test]
    fn gh_status_tells_a_missing_gh_from_a_signed_out_one() {
        assert_eq!(status(Err("zsh:1: command not found: gh".into()), Err(String::new())), GhStatus::Missing);
        let signed_out = "You are not logged into any GitHub hosts. To log in, run: gh auth login\n";
        assert_eq!(status(Ok(VERSION_OUT.into()), Err(signed_out.into())), GhStatus::SignedOut { version: "2.81.0".into() });
    }
}
