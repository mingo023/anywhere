mod avatars;
mod body;
mod card;
mod comments;
mod composer;
mod conversation;
mod desktop;
mod hero;
mod lists;
mod notice;
mod side;
mod timeline;

pub(crate) use card::{glyph_slot, review_icon, row, small};

use crate::git_ui::pull_requests::PrItem;
use crate::sidebar::fold::Closing;
use crate::status::Status;
use git::github::{self, Check, Gate, Mergeable, Method, Outcome, Pr, PrState, Review, Thread, ThreadState};
use git::{Commit, Repo};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;
use std::cell::RefCell;
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;
use theme::{FAILED_TEXT, MERGED, SUCCESS_TEXT, TEXT_3, TEXT_4, Token, WAITING_TEXT};

const NOTICE: Duration = Duration::from_millis(4000);

/// What the commit box offers once nothing is left to commit.
#[derive(Debug, PartialEq)]
pub(crate) enum Ship {
    Commit,
    Push { commits: usize, create: bool },
    Create,
}

/// Commits come first; then pushing what isn't pushed, offering to open a PR when the branch has none; then opening one.
pub(crate) fn ship(repo: &Repo, item: Option<&PrItem>) -> Ship {
    let create = item == Some(&PrItem::Create) && repo.ahead > 0;
    let unpushed = repo.unpushed.unwrap_or(repo.ahead);
    match () {
        _ if !repo.files.is_empty() => Ship::Commit,
        _ if unpushed > 0 => Ship::Push { commits: unpushed, create },
        _ if create => Ship::Create,
        _ => Ship::Commit,
    }
}

pub(crate) fn commits(n: usize) -> String {
    if n == 1 { "1 commit".into() } else { format!("{n} commits") }
}

/// A title and body from the branch's commits, given newest first, for when no agent writes them.
fn from_commits(commits: &[Commit]) -> (String, String) {
    let title = commits.last().map(|c| c.subject.clone()).unwrap_or_default();
    let body = if commits.len() > 1 { commits.iter().rev().map(|c| format!("- {}", c.subject)).collect::<Vec<_>>().join("\n") } else { String::new() };
    (title, body)
}

/// The last `n` lines of a review comment's hunk, which ends at the line it's on.
pub(crate) fn hunk_tail(hunk: &str, n: usize) -> Vec<&str> {
    let lines: Vec<&str> = hunk.lines().filter(|l| !l.starts_with("@@")).collect();
    lines[lines.len().saturating_sub(n)..].to_vec()
}

pub(crate) fn place(t: &Thread) -> String {
    t.line.map_or_else(|| t.path.clone(), |l| format!("{}:{l}", t.path))
}

/// A thread's file and line, without the folders: a narrow card has no room for both a path and what was said.
pub(crate) fn spot(t: &Thread) -> String {
    let file = t.path.rsplit('/').next().unwrap_or(&t.path);
    t.line.map_or(file.to_string(), |line| format!("{file}:{line}"))
}

/// The first line of what opened thread `t`.
pub(crate) fn opening(t: &Thread) -> String {
    t.comments.first().map(|c| c.body.lines().next().unwrap_or_default().to_string()).unwrap_or_default()
}

/// What an agent is asked to fix on PR `pr`: each failed check with its log, each review thread with the code it's on.
pub(crate) fn fix_prompt(pr: &Pr, checks: &[&Check], threads: &[&Thread]) -> String {
    let checks = checks.iter().map(|c| format!("The check \"{}\" failed. Its log: {}", c.name, c.url));
    let threads = threads.iter().map(|t| {
        let code = hunk_tail(t.comments.first().map_or("", |c| c.hunk.as_str()), 3).join("\n");
        let said: Vec<String> = t.comments.iter().map(|c| format!("{}: {}", c.author, c.body.trim())).collect();
        format!("A review comment on {}:\n```\n{code}\n```\n{}", place(t), said.join("\n"))
    });
    let items: Vec<String> = checks.chain(threads).enumerate().map(|(i, s)| format!("{}. {s}", i + 1)).collect();
    format!("Fix what holds back PR #{} ({}):\n\n{}\n\nThen commit and push.", pr.number, pr.url, items.join("\n\n"))
}

pub(crate) fn conflict_prompt(base: &str) -> String {
    format!("Merge origin/{} into this branch and resolve the conflicts, then push.", github::base_branch(base))
}

pub(crate) fn failed(pr: &Pr) -> Vec<&Check> {
    pr.runs.iter().filter(|c| c.outcome == Outcome::Failed).collect()
}

/// What an agent could still be sent: failed checks and open threads it wasn't handed since the last push.
pub(crate) fn to_fix<'a>(pr: &'a Pr, sent: Option<&Sent>) -> (Vec<&'a Check>, Vec<&'a Thread>) {
    let checks = failed(pr).into_iter().filter(|c| !sent.is_some_and(|s| s.checks.contains(&c.name))).collect();
    let threads = pr.threads.iter().filter(|t| t.state(sent.is_some_and(|s| s.threads.contains(&t.id))) == ThreadState::Open).collect();
    (checks, threads)
}

pub(crate) fn plural(n: usize, one: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") }
}

/// "3 things to fix", over "1 failed check · 2 review comments".
pub(crate) fn fix_summary(checks: usize, comments: usize) -> (String, String) {
    let parts: Vec<String> = [(checks, "failed check"), (comments, "review comment")].into_iter().filter(|(n, _)| *n > 0).map(|(n, w)| plural(n, w)).collect();
    (format!("{} to fix", plural(checks + comments, "thing")), parts.join(" · "))
}

/// The checks named one by one, failed before pending; the passed ones are only counted.
pub(crate) struct CheckRows<'a> {
    pub(crate) named: Vec<&'a Check>,
    pub(crate) passed: usize,
}

pub(crate) fn check_rows(pr: &Pr) -> CheckRows<'_> {
    let mut named: Vec<&Check> = pr.runs.iter().filter(|c| matches!(c.outcome, Outcome::Failed | Outcome::Pending)).collect();
    named.sort_by_key(|c| c.outcome != Outcome::Failed);
    CheckRows { named, passed: pr.runs.iter().filter(|c| c.outcome == Outcome::Passed).count() }
}

/// "5 passed · 1 skipped", leaving out what is none.
pub(crate) fn tally(parts: impl IntoIterator<Item = (usize, &'static str)>) -> String {
    parts.into_iter().filter(|(n, _)| *n > 0).map(|(n, w)| format!("{n} {w}")).collect::<Vec<_>>().join(" · ")
}

pub(crate) fn fixing_text(agent: &str, items: usize) -> String {
    format!("{agent} is fixing {}", plural(items, "item"))
}

pub(crate) fn resolving_text(agent: &str) -> String {
    format!("{agent} is resolving the conflict")
}

/// Whether the open PR holds something for someone to fix: a conflict, a failed check, asked-for changes or an unresolved thread, draft or not.
pub(crate) fn pr_wants_you(pr: &Pr) -> bool {
    pr.state == PrState::Open
        && (pr.mergeable == Mergeable::Conflicting || pr.checks.failed > 0 || pr.review == Review::ChangesRequested || pr.unresolved().next().is_some())
}

/// What `tree`'s card says while `gh` works for it; its open composer says it instead.
pub(crate) fn card_busy(busy: Option<&(String, &'static str)>, composing: Option<&str>, tree: &str) -> Option<&'static str> {
    busy.filter(|(t, _)| t == tree && composing != Some(tree)).map(|(_, text)| *text)
}

/// Where the PR stands in one line, in its colour: all a folded card says.
pub(crate) fn verdict(pr: &Pr, fixing: Option<&Sent>, busy: Option<&str>, agent: &str) -> (Token, String) {
    let items = fixing.map_or(0, Sent::items);
    match (pr.state, busy) {
        (PrState::Merged, _) => (MERGED, format!("Merged into {}", pr.base)),
        (PrState::Closed, _) => (TEXT_4, "Closed".into()),
        (_, Some(busy)) => (WAITING_TEXT, busy.into()),
        _ if items > 0 => (WAITING_TEXT, fixing_text(agent, items)),
        _ if fixing.is_some_and(|s| s.conflict) => (WAITING_TEXT, resolving_text(agent)),
        _ => {
            let gate = pr.gate();
            let c = pr.checks;
            match gate {
                Gate::Draft => (TEXT_4, "Draft".into()),
                Gate::Ready => (SUCCESS_TEXT, "Ready to merge".into()),
                Gate::Conflicts => (FAILED_TEXT, gate_text(gate, &pr.base)),
                Gate::ChecksFailed if pr.review == Review::ChangesRequested => {
                    (FAILED_TEXT, format!("{} failed · changes requested", plural(c.failed.into(), "check")))
                }
                Gate::ChecksFailed => (FAILED_TEXT, format!("{} failed", plural(c.failed.into(), "check"))),
                Gate::ChecksRunning => (WAITING_TEXT, format!("{} in progress", plural(c.pending.into(), "check"))),
                Gate::ChangesRequested => (FAILED_TEXT, "Changes requested".into()),
                _ => (TEXT_3, gate_text(gate, &pr.base)),
            }
        }
    }
}

/// Who approved or asked for changes, and who is still asked; "No review yet" when nobody.
pub(crate) fn review_lines(pr: &Pr) -> Vec<(Review, String)> {
    let mut lines: Vec<(Review, String)> = pr
        .reviews
        .iter()
        .filter_map(|(who, r)| match r {
            Review::Approved => Some((*r, format!("{who} approved"))),
            Review::ChangesRequested => Some((*r, format!("{who} requested changes"))),
            _ => None,
        })
        .collect();
    if !pr.requested.is_empty() {
        lines.push((Review::Required, format!("Review requested from {}", pr.requested.join(", "))));
    }
    if lines.is_empty() {
        lines.push((Review::None, "No review yet".into()));
    }
    lines
}

/// What holds the merge back, or that nothing does.
pub(crate) fn gate_text(gate: Gate, base: &str) -> String {
    match gate {
        Gate::Ready => format!("No conflicts with {base}"),
        Gate::Conflicts => format!("Conflicts with {base}"),
        Gate::Checking => "Checking for conflicts…".into(),
        Gate::ChecksRunning => "Checks in progress".into(),
        Gate::ChecksFailed => "Can't merge until checks pass".into(),
        Gate::ChangesRequested => "Can't merge until the requested changes are approved".into(),
        Gate::ReviewRequired => "Needs an approving review".into(),
        Gate::Blocked => "A branch rule holds the merge".into(),
        Gate::Draft => "Drafts can't be merged".into(),
        Gate::Done => String::new(),
    }
}

/// What was handed to an agent since the PR's head last moved.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Sent {
    head: String,
    agent: String,
    /// The agent has been seen working since it was sent this.
    started: bool,
    pub(crate) threads: HashSet<String>,
    pub(crate) checks: HashSet<String>,
    pub(crate) conflict: bool,
}

impl Sent {
    pub(crate) fn items(&self) -> usize {
        self.threads.len() + self.checks.len()
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) enum PrTab {
    #[default]
    Conversation,
    Checks,
    Commits,
    Files,
}

impl PrTab {
    pub(crate) const ALL: [PrTab; 4] = [PrTab::Conversation, PrTab::Checks, PrTab::Commits, PrTab::Files];

    pub(crate) fn label(self) -> &'static str {
        match self {
            PrTab::Conversation => "Conversation",
            PrTab::Checks => "Checks",
            PrTab::Commits => "Commits",
            PrTab::Files => "Files",
        }
    }
}

/// The panel's choices and what agents were sent, apart from the inputs so it tests without a window.
pub(crate) struct Track {
    sent: HashMap<String, Sent>,
    kept: HashSet<String>,
    pub(crate) method: Method,
    pub(crate) delete_after: bool,
    pub(crate) tab: PrTab,
    /// The Conversation lists resolved threads too.
    pub(crate) all: bool,
    /// Threads shown whole: a resolved one unfolded, or a long one with every reply.
    pub(crate) expanded: HashSet<String>,
    /// Descriptions, reviews and comments folded to their heading.
    folded: HashSet<String>,
    closing: Closing<String>,
    /// Folds opened a moment ago, whose body mounts afresh and so must spring from shut.
    opening: Closing<String>,
    /// Long bodies shown past [`CLIP`].
    whole: HashSet<String>,
    /// Written while painting, the only time a body's height is known.
    pub(crate) heights: Rc<RefCell<HashMap<String, f32>>>,
}

pub(crate) const CLIP: f32 = 240.;
/// A body only a line or two past [`CLIP`] shows whole: hiding so little behind Show more costs more than it saves.
const CLIP_SLACK: f32 = 40.;
/// Under this, a height change is layout rounding, not a new height.
const JITTER: f32 = 0.5;

/// How long a body runs, once laid out.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Length {
    Unmeasured,
    Short,
    Long(f32),
}

/// `pr`'s description among the folds.
pub(crate) fn about(pr: &Pr) -> String {
    format!("about:{}", pr.url)
}

impl Default for Track {
    fn default() -> Self {
        Self {
            sent: HashMap::new(),
            kept: HashSet::new(),
            method: Method::default(),
            delete_after: true,
            tab: PrTab::default(),
            all: false,
            expanded: HashSet::new(),
            folded: HashSet::new(),
            closing: Closing::default(),
            opening: Closing::default(),
            whole: HashSet::new(),
            heights: Rc::default(),
        }
    }
}

impl Track {
    /// Records `threads`, `checks` or the conflict as sent to `agent` for `tree`'s PR; what was sent before its head moved is done.
    pub(crate) fn send(&mut self, tree: &str, agent: &str, pr: &Pr, threads: &[&Thread], checks: &[&Check], conflict: bool) {
        let sent = self.sent.entry(tree.to_string()).or_default();
        if sent.head != pr.head {
            *sent = Sent { head: pr.head.clone(), ..Sent::default() };
        }
        (sent.agent, sent.started) = (agent.to_string(), false);
        sent.threads.extend(threads.iter().map(|t| t.id.clone()));
        sent.checks.extend(checks.iter().map(|c| c.name.clone()));
        sent.conflict |= conflict;
    }

    /// What the agent still works on: what it was sent since the head last moved, less what `pr` already shows done, as a fix that needed no push leaves the head where it was.
    pub(crate) fn fixing(&self, tree: &str, pr: &Pr) -> Option<Sent> {
        let sent = self.sent.get(tree).filter(|s| s.head == pr.head)?;
        let passed = |name: &String| {
            let mut runs = pr.runs.iter().filter(|c| c.name == *name).peekable();
            runs.peek().is_some() && runs.all(|c| matches!(c.outcome, Outcome::Passed | Outcome::Skipped))
        };
        let resolved = |id: &String| pr.threads.iter().any(|t| t.id == *id && t.resolved);
        let left = Sent {
            head: sent.head.clone(),
            threads: sent.threads.iter().filter(|id| !resolved(id)).cloned().collect(),
            checks: sent.checks.iter().filter(|name| !passed(name)).cloned().collect(),
            conflict: sent.conflict && pr.mergeable != Mergeable::Clean,
            ..Sent::default()
        };
        (left.items() > 0 || left.conflict).then_some(left)
    }

    /// Forgets what agents have finished with, by `status`: one that worked and stopped, or is gone. A fix that changed nothing moves neither the head nor the checks. True when anything was forgotten.
    pub(crate) fn settle(&mut self, status: impl Fn(&str) -> Option<Status>) -> bool {
        let before = self.sent.len();
        self.sent.retain(|_, sent| match status(&sent.agent) {
            Some(Status::Working) => {
                sent.started = true;
                true
            }
            Some(Status::NeedsYou) => true,
            Some(_) => !sent.started,
            None => false,
        });
        self.sent.len() != before
    }

    pub(crate) fn thread_sent(&self, tree: &str, pr: &Pr, thread: &str) -> bool {
        self.fixing(tree, pr).is_some_and(|s| s.threads.contains(thread))
    }

    pub(crate) fn keep(&mut self, tree: &str) {
        self.kept.insert(tree.to_string());
    }

    pub(crate) fn kept(&self, tree: &str) -> bool {
        self.kept.contains(tree)
    }

    pub(crate) fn toggle_expanded(&mut self, thread: &str) {
        flip(&mut self.expanded, thread);
    }

    pub(crate) fn folded(&self, key: &str) -> bool {
        self.folded.contains(key)
    }

    pub(crate) fn toggle_folded(&mut self, key: &str) {
        flip(&mut self.folded, key);
        if self.folded(key) {
            self.closing.start(key.to_string());
        } else {
            self.opening.start(key.to_string());
        }
    }

    pub(crate) fn opening(&self, key: &str) -> bool {
        self.opening.drawn(key, false)
    }

    pub(crate) fn drawn(&self, key: &str) -> bool {
        self.closing.drawn(key, !self.folded(key))
    }

    pub(crate) fn whole(&self, key: &str) -> bool {
        self.whole.contains(key)
    }

    pub(crate) fn toggle_whole(&mut self, key: &str) {
        flip(&mut self.whole, key);
    }

    pub(crate) fn height(&self, key: &str) -> Option<f32> {
        self.heights.borrow().get(key).copied()
    }

    pub(crate) fn length(&self, key: &str) -> Length {
        match self.height(key) {
            None => Length::Unmeasured,
            Some(h) if h > CLIP + CLIP_SLACK => Length::Long(h),
            Some(_) => Length::Short,
        }
    }
}

/// Whether `height` moved `key`'s, so what was drawn from the old one needs another frame.
pub(crate) fn record_height(heights: &RefCell<HashMap<String, f32>>, key: &str, height: f32) -> bool {
    let mut heights = heights.borrow_mut();
    let moved = heights.get(key).is_none_or(|h| (h - height).abs() > JITTER);
    if moved {
        heights.insert(key.to_string(), height);
    }
    moved
}

fn flip(set: &mut HashSet<String>, id: &str) {
    if !set.remove(id) {
        set.insert(id.to_string());
    }
}

/// A resolve to flip back, offered on the notice it raised.
#[derive(Clone)]
pub(crate) struct Undo {
    pub(crate) tree: String,
    pub(crate) thread: String,
    pub(crate) resolved: bool,
}

pub(crate) struct Notice {
    pub(crate) text: String,
    pub(crate) undo: Option<Undo>,
    _hide: Task<()>,
}

/// What the side says when the worktree has no PR.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum NoPr {
    Create,
    Hint(&'static str),
}

/// The worktree's branch, as the side names it.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Branch {
    pub(crate) name: String,
    pub(crate) base: Option<String>,
    pub(crate) ahead: usize,
}

/// What `Desktop` hands the side before it draws, and again whenever any of it changes.
#[derive(Default)]
pub(crate) struct PrFrame {
    /// The worktree on screen.
    pub(crate) tree: Option<String>,
    /// `None` when it isn't a git repository.
    pub(crate) branch: Option<Branch>,
    pub(crate) pr: Option<Arc<Pr>>,
    pub(crate) offer: Option<NoPr>,
    /// `gh` is being asked about the worktree.
    pub(crate) asking: bool,
    /// The agent fixes go to.
    pub(crate) agent: &'static str,
    pub(crate) observe_only: bool,
    /// The worktree is one of its own, which merging may delete.
    pub(crate) deletable: bool,
    /// Who `gh` acts as here.
    pub(crate) me: String,
    pub(crate) ai: bool,
    pub(crate) draft: bool,
    /// The agent that writes a PR's title and description.
    pub(crate) writer: &'static str,
    /// Which minute it is, so what says how long ago is redrawn as time passes.
    pub(crate) minute: u64,
}

impl PartialEq for PrFrame {
    fn eq(&self, other: &Self) -> bool {
        let same_pr = match (&self.pr, &other.pr) {
            (Some(a), Some(b)) => Arc::ptr_eq(a, b),
            (None, None) => true,
            _ => false,
        };
        same_pr
            && self.tree == other.tree
            && self.branch == other.branch
            && self.offer == other.offer
            && self.asking == other.asking
            && self.agent == other.agent
            && self.observe_only == other.observe_only
            && self.deletable == other.deletable
            && self.me == other.me
            && self.ai == other.ai
            && self.draft == other.draft
            && self.writer == other.writer
            && self.minute == other.minute
    }
}

/// What the side asks `Desktop` for: everything that runs `gh` or an agent, or reads or writes more than it was handed.
pub(crate) enum PrEvent {
    Refresh,
    OpenUrl(String),
    /// A file of the PR, as the PR changes it.
    OpenFile(String),
    OpenComposer,
    Write,
    Create,
    SetDraft(bool),
    Merge,
    Ready,
    Fix {
        checks: Vec<String>,
        threads: Vec<String>,
    },
    Conflict,
    Resolve {
        thread: String,
        resolved: bool,
    },
    Reply {
        thread: String,
        body: String,
    },
    Comment {
        tree: String,
        body: String,
    },
    ViewThread(String),
    DeleteMerged(String),
    Undo,
}

impl EventEmitter<PrEvent> for PrSide {}

/// The worktree's pull request, drawn in the panel and as cards under the PR diff's lines.
pub(crate) struct PrSide {
    pub(crate) frame: PrFrame,
    pub(crate) title: Entity<InputState>,
    pub(crate) body: Entity<TextareaState>,
    pub(crate) reply: Entity<TextareaState>,
    /// What is said on the PR itself, outside any thread.
    pub(crate) comment: Entity<TextareaState>,
    /// The worktree the comment box writes for, while it is open.
    pub(crate) commenting: Option<String>,
    pub(crate) comment_error: Option<String>,
    /// What each worktree's comment box held while another's was open, and why its post failed; back in the box when that worktree's opens.
    pub(crate) drafts: HashMap<String, (String, Option<String>)>,
    pub(crate) commits_scroll: UniformListScrollHandle,
    pub(crate) files_scroll: UniformListScrollHandle,
    pub(crate) conversation_scroll: ScrollHandle,
    /// The worktree whose PR the composer writes.
    pub(crate) composing: Option<String>,
    pub(crate) writing: bool,
    /// The worktree `gh` works for, and what its composer's or card's button says meanwhile.
    pub(crate) busy: Option<(String, &'static str)>,
    pub(crate) error: Option<String>,
    pub(crate) ship_menu: bool,
    pub(crate) merge_menu: bool,
    pub(crate) create_menu: bool,
    /// The thread the reply box is under.
    pub(crate) replying: Option<String>,
    pub(crate) notice: Option<Notice>,
    pub(crate) track: Track,
    pub(crate) avatars: avatars::Avatars,
    _subs: Vec<Subscription>,
}

impl PrSide {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Title"));
        let body = cx.new(|cx| TextareaState::new(window, cx).placeholder("What this changes, and why").rows(8));
        let reply = cx.new(|cx| TextareaState::new(window, cx).placeholder("Reply…").auto_grow(2, 8));
        let comment = cx.new(|cx| TextareaState::new(window, cx).placeholder("Leave a comment").auto_grow(3, 10));
        let subs = vec![
            cx.subscribe_in(&title, window, |_, _, ev: &InputEvent, _, cx| {
                if let InputEvent::Change = ev {
                    cx.notify();
                }
            }),
            cx.subscribe_in(&body, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary: true, .. } = ev {
                    this.create_pr(window, cx);
                }
            }),
            cx.subscribe_in(&reply, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { secondary: true, .. } => this.send_reply(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
            cx.subscribe_in(&comment, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { secondary: true, .. } => this.send_comment(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
        ];
        Self {
            frame: PrFrame::default(),
            title,
            body,
            reply,
            comment,
            commenting: None,
            drafts: HashMap::new(),
            comment_error: None,
            commits_scroll: UniformListScrollHandle::new(),
            files_scroll: UniformListScrollHandle::new(),
            conversation_scroll: ScrollHandle::new(),
            avatars: avatars::Avatars::default(),
            composing: None,
            writing: false,
            busy: None,
            error: None,
            ship_menu: false,
            merge_menu: false,
            create_menu: false,
            replying: None,
            notice: None,
            track: Track::default(),
            _subs: subs,
        }
    }

    /// Takes what `Desktop` hands the side; it redraws only when that changed.
    pub(crate) fn set(&mut self, frame: PrFrame, cx: &mut Context<Self>) {
        if self.frame != frame {
            self.frame = frame;
            cx.notify();
        }
    }

    /// The PR of the worktree on screen, with the worktree.
    pub(crate) fn shown_pr(&self) -> Option<(&str, &Pr)> {
        Some((self.frame.tree.as_deref()?, self.frame.pr.as_deref()?))
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.ship_menu || self.merge_menu || self.create_menu
    }

    pub(crate) fn shut_menus(&mut self) {
        (self.ship_menu, self.merge_menu, self.create_menu) = (false, false, false);
    }

    /// Forgets what agents have finished with, as [`Track::settle`] does.
    pub(crate) fn settle(&mut self, status: impl Fn(&str) -> Option<Status>, cx: &mut Context<Self>) {
        if self.track.settle(status) {
            cx.notify();
        }
    }

    /// Where the PR of `tree` stands, in its colour, as its card's header says it.
    pub(crate) fn verdict(&self, tree: &str, pr: &Pr, agent: &str) -> (Token, String) {
        let busy = card_busy(self.busy.as_ref(), self.composing.as_deref(), tree);
        verdict(pr, self.track.fixing(tree, pr).as_ref(), busy, agent)
    }

    pub(crate) fn open_pr(&self, url: String, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(PrEvent::OpenUrl(url));
    }

    pub(crate) fn open_pr_composer(&self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(PrEvent::OpenComposer);
    }

    pub(crate) fn write_pr(&self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(PrEvent::Write);
    }

    pub(crate) fn create_pr(&self, _: &mut Window, cx: &mut Context<Self>) {
        cx.emit(PrEvent::Create);
    }

    pub(crate) fn refresh_pr(&self, cx: &mut Context<Self>) {
        cx.emit(PrEvent::Refresh);
    }

    pub(crate) fn merge_pr(&self, cx: &mut Context<Self>) {
        cx.emit(PrEvent::Merge);
    }

    pub(crate) fn mark_ready(&self, cx: &mut Context<Self>) {
        cx.emit(PrEvent::Ready);
    }

    pub(crate) fn send_conflict(&self, cx: &mut Context<Self>) {
        cx.emit(PrEvent::Conflict);
    }

    pub(crate) fn send_fix(&self, checks: &[String], threads: &[String], cx: &mut Context<Self>) {
        cx.emit(PrEvent::Fix { checks: checks.to_vec(), threads: threads.to_vec() });
    }

    pub(crate) fn resolve_thread(&self, thread: String, resolved: bool, _undo: bool, cx: &mut Context<Self>) {
        cx.emit(PrEvent::Resolve { thread, resolved });
    }

    pub(crate) fn view_thread(&self, id: &str, cx: &mut Context<Self>) {
        cx.emit(PrEvent::ViewThread(id.to_string()));
    }

    pub(crate) fn delete_merged(&self, tree: &str, cx: &mut Context<Self>) {
        cx.emit(PrEvent::DeleteMerged(tree.to_string()));
    }

    /// Opens the composer for `tree`, filled from the commits unless an agent is to write it.
    pub(crate) fn open_composer(&mut self, tree: String, from_commits: Option<(String, String)>, window: &mut Window, cx: &mut Context<Self>) {
        self.composing = Some(tree);
        self.error = None;
        if let Some((title, body)) = from_commits {
            self.fill_composer(title, body, window, cx);
        }
        self.title.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn fill_composer(&mut self, title: String, body: String, window: &mut Window, cx: &mut Context<Self>) {
        self.title.update(cx, |s, cx| s.set_value(title, window, cx));
        self.body.update(cx, |s, cx| s.set_value(body, window, cx));
    }

    pub(crate) fn close_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composing = None;
        self.error = None;
        self.fill_composer(String::new(), String::new(), window, cx);
        cx.notify();
    }

    /// Starts an agent writing the composer, unless one already is.
    pub(crate) fn begin_write(&mut self, cx: &mut Context<Self>) -> bool {
        if self.writing {
            return false;
        }
        self.writing = true;
        self.error = None;
        cx.notify();
        true
    }

    /// Fills the composer with what the agent wrote for `tree`, or with `fallback` and why it failed, if the composer is still for it.
    pub(crate) fn written(&mut self, tree: &str, res: Result<String, String>, fallback: (String, String), window: &mut Window, cx: &mut Context<Self>) {
        self.writing = false;
        if self.composing.as_deref() == Some(tree) {
            match res {
                Ok(text) => {
                    let (title, body) = github::split_message(&text);
                    self.fill_composer(title, body, window, cx);
                }
                Err(e) => {
                    self.fill_composer(fallback.0, fallback.1, window, cx);
                    self.error = Some(format!("Couldn't write the PR: {e}"));
                }
            }
        }
        cx.notify();
    }

    /// What the composer holds to open, with `gh` marked busy for it; `None` when it can't open yet.
    pub(crate) fn begin_create(&mut self, tree: &str, draft: bool, window: &mut Window, cx: &mut Context<Self>) -> Option<(String, String)> {
        if self.busy.is_some() || self.writing {
            return None;
        }
        let title = self.title.read(cx).value().trim().to_string();
        if title.is_empty() {
            self.title.update(cx, |s, cx| s.focus(window, cx));
            cx.notify();
            return None;
        }
        let body = self.body.read(cx).value().to_string();
        self.busy = Some((tree.to_string(), if draft { "Creating draft PR…" } else { "Creating PR…" }));
        self.error = None;
        cx.notify();
        Some((title, body))
    }

    /// Marks `gh` busy for `tree` while its card says `busy`; false when it already is.
    pub(crate) fn begin_gh(&mut self, tree: &str, busy: &'static str, cx: &mut Context<Self>) -> bool {
        if self.busy.is_some() {
            return false;
        }
        self.busy = Some((tree.to_string(), busy));
        (self.merge_menu, self.create_menu) = (false, false);
        cx.notify();
        true
    }

    pub(crate) fn end_gh(&mut self, cx: &mut Context<Self>) {
        self.busy = None;
        cx.notify();
    }

    pub(crate) fn notify_pr(&mut self, text: String, undo: Option<Undo>, cx: &mut Context<Self>) {
        let hide = cx.spawn(async |this, cx| {
            cx.background_executor().timer(NOTICE).await;
            this.update(cx, |p, cx| {
                p.notice = None;
                cx.notify();
            })
            .ok();
        });
        self.notice = Some(Notice { text, undo, _hide: hide });
        cx.notify();
    }

    /// The resolve the notice offers to flip back, taking the notice down.
    pub(crate) fn take_undo(&mut self, cx: &mut Context<Self>) -> Option<Undo> {
        let undo = self.notice.take().and_then(|n| n.undo);
        cx.notify();
        undo
    }

    pub(crate) fn start_reply(&mut self, thread: String, window: &mut Window, cx: &mut Context<Self>) {
        let to = self.shown_pr().and_then(|(_, pr)| pr.threads.iter().find(|t| t.id == thread)?.comments.first().map(|c| c.author.clone()));
        self.replying = Some(thread);
        self.reply.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.set_placeholder(format!("Reply to {}…", to.unwrap_or_default()), window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn cancel_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.replying = None;
        self.reply.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    pub(crate) fn send_reply(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(thread) = self.replying.clone() else { return };
        let body = self.reply.read(cx).value().trim().to_string();
        if !body.is_empty() {
            cx.emit(PrEvent::Reply { thread, body });
        }
    }

    pub(crate) fn open_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.frame.tree.clone() else { return };
        self.track.tab = PrTab::Conversation;
        if self.commenting.as_ref() != Some(&tree) {
            let held = self.comment.read(cx).value().to_string();
            if let Some(left) = self.commenting.take().filter(|_| !held.trim().is_empty()) {
                self.drafts.insert(left, (held, self.comment_error.take()));
            }
            let (text, error) = self.drafts.remove(&tree).unwrap_or_default();
            self.comment_error = error;
            self.comment.update(cx, |s, cx| s.set_value(text, window, cx));
        }
        self.commenting = Some(tree);
        // The box sits at the end of the Conversation, which may be scrolled far above it.
        self.conversation_scroll.scroll_to_bottom();
        self.comment.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn cancel_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.commenting = None;
        self.comment_error = None;
        self.comment.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    /// Opens the comment box with `said` quoted after what it already holds: GitHub has no replies to a comment on the PR itself.
    pub(crate) fn quote_reply(&mut self, said: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.open_comment(window, cx);
        let held = self.comment.read(cx).value().trim_end().to_string();
        let text = if held.is_empty() { body::quote(said) } else { format!("{held}\n\n{}", body::quote(said)) };
        self.comment.update(cx, |s, cx| s.set_value(text, window, cx));
    }

    pub(crate) fn copy_link(&mut self, url: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(url));
        self.notify_pr("Copied the link".into(), None, cx);
    }

    pub(crate) fn send_comment(&mut self, _: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.commenting.clone() else { return };
        let body = self.comment.read(cx).value().trim().to_string();
        if !body.is_empty() {
            cx.emit(PrEvent::Comment { tree, body });
        }
    }

    /// Puts `body` back, which didn't post on `tree`'s PR: in the box if it is still open for `tree`, else in `tree`'s draft.
    pub(crate) fn comment_failed(&mut self, tree: String, body: String, error: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.commenting.as_ref().is_some_and(|t| *t != tree) {
            let draft = self.drafts.entry(tree).or_default();
            draft.0 = if draft.0.trim().is_empty() { body } else { format!("{body}\n\n{}", draft.0) };
            draft.1 = Some(format!("Couldn't post the comment: {error}"));
            self.notify_pr("Couldn't post the comment; it's back in its worktree's comment box".into(), None, cx);
        } else {
            let held = self.comment.read(cx).value().trim().to_string();
            let text = if held.is_empty() { body } else { format!("{body}\n\n{held}") };
            self.comment.update(cx, |s, cx| s.set_value(text, window, cx));
            self.commenting = Some(tree);
            self.comment_error = Some(format!("Couldn't post the comment: {error}"));
        }
        cx.notify();
    }

    /// Settles what a resolve left: the thread folds, and the notice offers Undo or goes.
    pub(crate) fn resolved(&mut self, thread: &str, notice: Option<(String, Undo)>, cx: &mut Context<Self>) {
        self.track.expanded.remove(thread);
        match notice {
            Some((text, undo)) => self.notify_pr(text, Some(undo), cx),
            None => self.notice = None,
        }
        cx.notify();
    }

    /// Fetches the pictures of everyone on `pr`, and `me`, each in its own background task.
    pub(crate) fn fetch_avatars(&mut self, pr: &Pr, me: &str, dir: PathBuf, cx: &mut Context<Self>) {
        let wanted = self.avatars.ask(github::people(pr).into_iter().chain([me]));
        for login in wanted {
            let Some(url) = github::avatar_url(&pr.url, &login) else { continue };
            let path = dir.join(format!("{login}.png"));
            let dir = dir.clone();
            let task =
                cx.background_executor().spawn(async move { (std::fs::create_dir_all(&dir).is_ok() && github::fetch_avatar(&url, &path)).then_some(path) });
            cx.spawn(async move |this, cx| {
                let Some(path) = task.await else { return };
                this.update(cx, |p, cx| {
                    p.avatars.saved.insert(login, path);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CLIP, Length, Ship, Status, Track, card_busy, check_rows, conflict_prompt, fix_prompt, fix_summary, from_commits, gate_text, hunk_tail, pr_wants_you,
        record_height, review_lines, ship, tally, to_fix, verdict,
    };
    use crate::git_ui::pull_requests::PrItem;
    use git::github::{Check, Checks, Comment, Gate, Mergeable, Outcome, Pr, PrState, Review, Thread};
    use git::{Commit, FileStat, Repo};
    use theme::{FAILED_TEXT, MERGED, SUCCESS_TEXT, TEXT_4, WAITING_TEXT};

    fn repo(files: usize, ahead: usize, unpushed: Option<usize>) -> Repo {
        let file = |i: usize| FileStat { path: format!("{i}.rs"), added: 1, removed: 0, staged: false, unstaged: true, status: 'M', old_path: None };
        Repo { branch: "fix".into(), base: Some("origin/main".into()), ahead, unpushed, files: (0..files).map(file).collect(), ..Repo::default() }
    }

    #[test]
    fn a_card_shows_its_own_merge_while_another_worktree_composes() {
        let merging = ("/a".to_string(), "Merging…");
        assert_eq!(card_busy(Some(&merging), Some("/b"), "/a"), Some("Merging…"));
        assert_eq!(card_busy(Some(&merging), None, "/b"), None);
        assert_eq!(card_busy(Some(&("/a".to_string(), "Creating PR…")), Some("/a"), "/a"), None);
    }

    #[test]
    fn changes_are_committed_before_anything_ships() {
        assert_eq!(ship(&repo(2, 3, Some(1)), Some(&PrItem::Create)), Ship::Commit);
    }

    #[test]
    fn unpushed_commits_push_and_offer_a_pr_only_when_the_branch_has_none() {
        assert_eq!(ship(&repo(0, 3, Some(1)), Some(&PrItem::Create)), Ship::Push { commits: 1, create: true });
        assert_eq!(ship(&repo(0, 3, Some(2)), Some(&PrItem::Open("https://github.com/a/b/pull/1"))), Ship::Push { commits: 2, create: false });
    }

    #[test]
    fn a_branch_never_pushed_pushes_every_commit_past_its_base() {
        assert_eq!(ship(&repo(0, 3, None), Some(&PrItem::Create)), Ship::Push { commits: 3, create: true });
    }

    #[test]
    fn a_pushed_branch_without_a_pr_offers_to_open_one_unless_it_adds_nothing() {
        assert_eq!(ship(&repo(0, 3, Some(0)), Some(&PrItem::Create)), Ship::Create);
        assert_eq!(ship(&repo(0, 0, Some(0)), Some(&PrItem::Create)), Ship::Commit);
        assert_eq!(ship(&repo(0, 3, Some(0)), None), Ship::Commit);
    }

    fn commit(subject: &str) -> Commit {
        Commit { sha: "abc1234".into(), subject: subject.into() }
    }

    #[test]
    fn without_an_agent_the_pr_is_titled_by_its_first_commit_and_lists_them_all() {
        assert_eq!(from_commits(&[commit("Fix the race")]), ("Fix the race".into(), String::new()));
        let (title, body) = from_commits(&[commit("Add a test"), commit("Fix the race")]);
        assert_eq!((title.as_str(), body.as_str()), ("Fix the race", "- Fix the race\n- Add a test"));
    }

    fn pr() -> Pr {
        Pr { number: 12, url: "https://github.com/acme/app/pull/12".into(), head: "aaa".into(), base: "main".into(), ..Pr::default() }
    }

    #[test]
    fn a_pr_wants_you_for_a_conflict_a_failed_check_asked_changes_or_an_open_thread_while_it_is_open() {
        assert!(!pr_wants_you(&pr()));
        assert!(pr_wants_you(&Pr { mergeable: Mergeable::Conflicting, ..pr() }));
        assert!(pr_wants_you(&Pr { checks: Checks { failed: 1, ..Checks::default() }, ..pr() }));
        assert!(pr_wants_you(&Pr { review: Review::ChangesRequested, ..pr() }));
        assert!(pr_wants_you(&Pr { threads: vec![thread("T1")], ..pr() }));
        assert!(!pr_wants_you(&Pr { threads: vec![thread("T1")], state: PrState::Merged, ..pr() }));
        assert!(pr_wants_you(&Pr { draft: true, checks: Checks { failed: 1, ..Checks::default() }, ..pr() }), "a draft's failure still wants you");
        assert!(
            pr_wants_you(&Pr { review: Review::ChangesRequested, checks: Checks { pending: 1, ..Checks::default() }, ..pr() }),
            "asked-for changes while checks run"
        );
    }

    fn thread(id: &str) -> Thread {
        let said = |author: &str, body: &str| Comment {
            author: author.into(),
            body: body.into(),
            at: 0,
            hunk: "@@ -1,4 +1,4 @@\n a\n b\n-c\n+d\n e".into(),
            ..Comment::default()
        };
        Thread {
            id: id.into(),
            path: "src/a.rs".into(),
            line: Some(42),
            comments: vec![said("mira", "Handle None here."), said("bo", "Agreed.")],
            ..Thread::default()
        }
    }

    #[test]
    fn a_fix_lists_each_failed_check_with_its_log_then_each_thread_with_the_code_it_is_on() {
        let check = Check { name: "test".into(), outcome: Outcome::Failed, url: "https://github.com/acme/app/actions/runs/1".into() };
        let t = thread("T1");
        assert_eq!(
            fix_prompt(&pr(), &[&check], &[&t]),
            "Fix what holds back PR #12 (https://github.com/acme/app/pull/12):\n\n\
             1. The check \"test\" failed. Its log: https://github.com/acme/app/actions/runs/1\n\n\
             2. A review comment on src/a.rs:42:\n```\n-c\n+d\n e\n```\nmira: Handle None here.\nbo: Agreed.\n\n\
             Then commit and push."
        );
    }

    #[test]
    fn a_hunk_tail_skips_its_header() {
        assert_eq!(hunk_tail("@@ -1 +1 @@\n+a", 3), ["+a"]);
        assert!(hunk_tail("", 3).is_empty());
    }

    #[test]
    fn a_conflict_is_resolved_by_merging_the_base_from_the_remote() {
        assert_eq!(conflict_prompt("origin/main"), "Merge origin/main into this branch and resolve the conflicts, then push.");
    }

    #[test]
    fn what_was_sent_is_being_fixed_until_the_agent_pushes() {
        let (mut track, t) = (Track::default(), thread("T1"));
        assert!(track.fixing("/a", &pr()).is_none());
        track.send("/a", "agent", &pr(), &[&t], &[], false);
        assert_eq!(track.fixing("/a", &pr()).map(|s| s.items()), Some(1));
        assert!(track.thread_sent("/a", &pr(), "T1"));
        assert!(track.fixing("/b", &pr()).is_none());
        let pushed = Pr { head: "bbb".into(), ..pr() };
        assert!(track.fixing("/a", &pushed).is_none());
        assert!(!track.thread_sent("/a", &pushed, "T1"));
    }

    #[test]
    fn sending_after_a_push_starts_a_new_fix() {
        let mut track = Track::default();
        track.send("/a", "agent", &pr(), &[&thread("T1")], &[], false);
        let pushed = Pr { head: "bbb".into(), ..pr() };
        track.send("/a", "agent", &pushed, &[], &[], true);
        let sent = track.fixing("/a", &pushed).unwrap();
        assert!(sent.threads.is_empty() && sent.conflict);
    }

    #[test]
    fn what_github_shows_done_stops_being_fixed_without_a_push() {
        let pr = Pr {
            runs: vec![check("lint", Outcome::Failed), check("test", Outcome::Failed)],
            threads: vec![thread("T1")],
            mergeable: Mergeable::Conflicting,
            ..pr()
        };
        let mut track = Track::default();
        track.send("/a", "agent", &pr, &[&pr.threads[0]], &[&pr.runs[0], &pr.runs[1]], true);
        assert_eq!(track.fixing("/a", &pr).map(|s| s.items()), Some(3));
        let later = Pr {
            runs: vec![check("lint", Outcome::Passed), check("test", Outcome::Pending)],
            threads: vec![Thread { resolved: true, ..thread("T1") }],
            mergeable: Mergeable::Clean,
            ..pr.clone()
        };
        let sent = track.fixing("/a", &later).unwrap();
        assert_eq!((sent.checks.iter().map(String::as_str).collect::<Vec<_>>(), sent.threads.len(), sent.conflict), (vec!["test"], 0, false));
        let green = Pr { runs: vec![check("lint", Outcome::Passed), check("test", Outcome::Skipped)], ..later };
        assert!(track.fixing("/a", &green).is_none());
    }

    #[test]
    fn a_fix_ends_once_its_agent_has_worked_and_stopped_even_with_nothing_pushed() {
        let pr = Pr { runs: vec![check("test", Outcome::Failed)], ..pr() };
        let mut track = Track::default();
        track.send("/a", "agent", &pr, &[], &[&pr.runs[0]], false);
        for (status, fixing) in [(Status::Done, true), (Status::Working, true), (Status::NeedsYou, true), (Status::Done, false)] {
            track.settle(|_| Some(status));
            assert_eq!(track.fixing("/a", &pr).is_some(), fixing, "{status:?}");
        }
    }

    #[test]
    fn a_fix_ends_when_its_agent_is_gone() {
        let pr = Pr { runs: vec![check("test", Outcome::Failed)], ..pr() };
        let mut track = Track::default();
        track.send("/a", "agent", &pr, &[], &[&pr.runs[0]], false);
        track.settle(|_| None);
        assert!(track.fixing("/a", &pr).is_none());
    }

    #[test]
    fn a_check_stays_fixing_while_any_run_of_it_still_fails() {
        let pr = Pr { runs: vec![check("test", Outcome::Failed)], ..pr() };
        let mut track = Track::default();
        track.send("/a", "agent", &pr, &[], &[&pr.runs[0]], false);
        let rerun = Pr { runs: vec![check("test", Outcome::Passed), check("test", Outcome::Failed)], ..pr.clone() };
        assert!(track.fixing("/a", &rerun).is_some());
    }

    #[test]
    fn worktrees_are_deleted_after_merging_unless_unticked() {
        assert!(Track::default().delete_after);
    }

    fn check(name: &str, outcome: Outcome) -> Check {
        Check { name: name.into(), outcome, url: String::new() }
    }

    #[test]
    fn what_is_left_to_fix_leaves_out_what_the_agent_already_has_and_threads_someone_answered() {
        let open = |id: &str| Thread { comments: thread(id).comments[..1].to_vec(), ..thread(id) };
        let pr = Pr {
            runs: vec![check("lint", Outcome::Failed), check("test", Outcome::Failed), check("build", Outcome::Passed)],
            threads: vec![open("T1"), thread("T2"), open("T3")],
            ..pr()
        };
        let mut track = Track::default();
        track.send("/a", "agent", &pr, &[&pr.threads[0]], &[&pr.runs[0]], false);
        let (checks, threads) = to_fix(&pr, track.fixing("/a", &pr).as_ref());
        assert_eq!(checks.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["test"]);
        assert_eq!(threads.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["T3"]);
    }

    #[test]
    fn the_fix_callout_counts_things_then_names_them() {
        assert_eq!(fix_summary(1, 2), ("3 things to fix".into(), "1 failed check · 2 review comments".into()));
        assert_eq!(fix_summary(0, 1), ("1 thing to fix".into(), "1 review comment".into()));
    }

    #[test]
    fn failed_checks_are_named_before_running_ones_and_the_rest_are_only_counted() {
        let pr = Pr {
            runs: vec![
                check("a", Outcome::Passed),
                check("b", Outcome::Pending),
                check("c", Outcome::Failed),
                check("d", Outcome::Skipped),
                check("e", Outcome::Passed),
            ],
            ..pr()
        };
        let rows = check_rows(&pr);
        assert_eq!(rows.named.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["c", "b"]);
        assert_eq!(rows.passed, 2);
    }

    #[test]
    fn counted_checks_leave_out_what_is_none() {
        assert_eq!(
            [tally([(5, "passed"), (1, "skipped")]), tally([(3, "passed"), (0, "skipped")]), tally([(0, "passed"), (2, "skipped")])],
            ["5 passed · 1 skipped", "3 passed", "2 skipped"]
        );
    }

    fn open(checks: Checks, review: Review) -> Pr {
        Pr { checks, review, mergeable: Mergeable::Clean, ..pr() }
    }

    #[test]
    fn a_folded_card_says_what_holds_the_pr_back_in_its_colour() {
        let failed = Checks { passed: 5, failed: 2, pending: 1 };
        assert_eq!(verdict(&open(failed, Review::ChangesRequested), None, None, "Claude"), (FAILED_TEXT, "2 checks failed · changes requested".into()));
        assert_eq!(
            verdict(&open(Checks { pending: 1, ..Checks::default() }, Review::None), None, None, "Claude"),
            (WAITING_TEXT, "1 check in progress".into())
        );
        assert_eq!(verdict(&open(Checks { passed: 3, ..Checks::default() }, Review::Approved), None, None, "Claude"), (SUCCESS_TEXT, "Ready to merge".into()));
        let conflicting = Pr { mergeable: Mergeable::Conflicting, ..open(failed, Review::None) };
        assert_eq!(verdict(&conflicting, None, None, "Claude"), (FAILED_TEXT, "Conflicts with main".into()));
        assert_eq!(verdict(&Pr { draft: true, ..open(failed, Review::None) }, None, None, "Claude"), (TEXT_4, "Draft".into()));
    }

    #[test]
    fn what_the_agent_or_gh_is_doing_comes_before_what_holds_the_pr_back() {
        let pr = open(Checks { failed: 2, ..Checks::default() }, Review::None);
        let mut track = Track::default();
        track.send("/a", "agent", &pr, &[], &[&check("lint", Outcome::Failed), &check("test", Outcome::Failed)], false);
        assert_eq!(verdict(&pr, track.fixing("/a", &pr).as_ref(), None, "Claude"), (WAITING_TEXT, "Claude is fixing 2 items".into()));
        assert_eq!(verdict(&pr, None, Some("Merging…"), "Claude"), (WAITING_TEXT, "Merging…".into()));
        let conflicting = Pr { mergeable: Mergeable::Conflicting, ..pr };
        let mut track = Track::default();
        track.send("/a", "agent", &conflicting, &[], &[], true);
        assert_eq!(verdict(&conflicting, track.fixing("/a", &conflicting).as_ref(), None, "Claude").1, "Claude is resolving the conflict");
    }

    #[test]
    fn a_merged_card_names_where_it_went() {
        assert_eq!(verdict(&Pr { state: PrState::Merged, ..pr() }, None, Some("Merging…"), "Claude"), (MERGED, "Merged into main".into()));
    }

    #[test]
    fn the_review_lists_verdicts_then_who_is_still_asked() {
        let pr = Pr {
            reviews: vec![("mira".into(), Review::Approved), ("bo".into(), Review::None), ("li".into(), Review::ChangesRequested)],
            requested: vec!["kai".into(), "jo".into()],
            ..pr()
        };
        let lines: Vec<String> = review_lines(&pr).into_iter().map(|(_, t)| t).collect();
        assert_eq!(lines, ["mira approved", "li requested changes", "Review requested from kai, jo"]);
        assert_eq!(review_lines(&super::Pr::default()), [(Review::None, "No review yet".to_string())]);
    }

    #[test]
    fn the_merge_row_says_what_holds_it_back() {
        assert_eq!(gate_text(Gate::Ready, "main"), "No conflicts with main");
        assert_eq!(gate_text(Gate::Conflicts, "main"), "Conflicts with main");
        assert_eq!(gate_text(Gate::ChecksRunning, "main"), "Checks in progress");
    }

    #[test]
    fn a_fold_stays_drawn_while_it_closes_and_others_stay_open() {
        let mut track = Track::default();
        track.toggle_folded("about");
        assert!(track.folded("about") && track.drawn("about"));
        assert!(!track.folded("review") && track.drawn("review"));
        track.toggle_folded("about");
        assert!(!track.folded("about") && track.drawn("about"));
        assert!(track.opening("about") && !track.opening("review"));
    }

    #[test]
    fn each_pr_keeps_its_own_description_state() {
        let pr = |n: u32| Pr { url: format!("https://github.com/acme/app/pull/{n}"), ..Pr::default() };
        assert_ne!(super::about(&pr(1)), super::about(&pr(2)));
    }

    #[test]
    fn only_a_body_well_past_the_clip_is_long_and_an_unmeasured_one_is_unknown() {
        let track = Track::default();
        assert_eq!(track.length("about"), Length::Unmeasured);
        record_height(&track.heights, "about", CLIP + 20.);
        assert_eq!(track.length("about"), Length::Short);
        record_height(&track.heights, "about", CLIP + 200.);
        assert_eq!(track.length("about"), Length::Long(CLIP + 200.));
    }

    #[test]
    fn a_height_counts_as_moved_only_past_layout_jitter() {
        let track = Track::default();
        assert!(record_height(&track.heights, "note", 100.));
        assert!(!record_height(&track.heights, "note", 100.3));
        assert!(record_height(&track.heights, "note", 120.));
        assert_eq!(track.height("note"), Some(120.));
    }
}
