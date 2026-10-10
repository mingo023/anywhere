mod card;
mod comments;
mod composer;
mod notice;

use crate::desktop::Desktop;
use crate::git_ui::changes::push;
use crate::git_ui::diff::At;
use crate::git_ui::pull_requests::PrItem;
use crate::removal::Removal;
use git::github::{self, Check, Gate, Method, Outcome, Pr, PrState, Review, Thread, ThreadState};
use git::{Commit, Repo};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use workspace::Doc;
use workspace::tree::PaneId;

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
/// Runs `argv` in `dir` through the login shell, so `gh` finds its auth as it does in a terminal.
pub(crate) fn run(argv: &[String], dir: &str, input: &str) -> Result<String, String> {
    daemon::run_login(&argv.iter().map(String::as_str).collect::<Vec<_>>(), dir, input)
}

pub(crate) fn hunk_tail(hunk: &str, n: usize) -> Vec<&str> {
    let lines: Vec<&str> = hunk.lines().filter(|l| !l.starts_with("@@")).collect();
    lines[lines.len().saturating_sub(n)..].to_vec()
}

pub(crate) fn place(t: &Thread) -> String {
    t.line.map_or_else(|| t.path.clone(), |l| format!("{}:{l}", t.path))
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

fn plural(n: usize, one: &str) -> String {
    if n == 1 { format!("1 {one}") } else { format!("{n} {one}s") }
}

/// "3 things to fix", over "1 failed check · 2 review comments".
pub(crate) fn fix_summary(checks: usize, comments: usize) -> (String, String) {
    let parts: Vec<String> = [(checks, "failed check"), (comments, "review comment")].into_iter().filter(|(n, _)| *n > 0).map(|(n, w)| plural(n, w)).collect();
    (format!("{} to fix", plural(checks + comments, "thing")), parts.join(" · "))
}

/// Failed checks first, then running ones, at most `max`; with how many were left out.
pub(crate) fn shown_checks(pr: &Pr, max: usize) -> (Vec<&Check>, usize) {
    let rank = |o: Outcome| match o {
        Outcome::Failed => 0,
        Outcome::Pending => 1,
        Outcome::Passed => 2,
        Outcome::Skipped => 3,
    };
    let mut runs: Vec<&Check> = pr.runs.iter().collect();
    runs.sort_by_key(|c| rank(c.outcome));
    let hidden = runs.len().saturating_sub(max);
    runs.truncate(max);
    (runs, hidden)
}

pub(crate) fn duration(secs: u64) -> String {
    match (secs / 60, secs % 60) {
        (0, s) => format!("{s}s"),
        (m, 0) => format!("{m}m"),
        (m, s) => format!("{m}m {s}s"),
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
    pub(crate) threads: HashSet<String>,
    pub(crate) checks: HashSet<String>,
    pub(crate) conflict: bool,
}

impl Sent {
    pub(crate) fn items(&self) -> usize {
        self.threads.len() + self.checks.len()
    }
}

/// The PR card's choices and what agents were sent, apart from the inputs so it tests without a window.
pub(crate) struct Track {
    sent: HashMap<String, Sent>,
    kept: HashSet<String>,
    pub(crate) method: Method,
    pub(crate) delete_after: bool,
    /// The Comments view replaces Changes.
    pub(crate) comments: bool,
    /// It lists resolved threads too.
    pub(crate) all: bool,
    /// Resolved threads shown whole.
    pub(crate) expanded: HashSet<String>,
}

impl Default for Track {
    fn default() -> Self {
        Self { sent: HashMap::new(), kept: HashSet::new(), method: Method::default(), delete_after: true, comments: false, all: false, expanded: HashSet::new() }
    }
}

impl Track {
    /// Records `threads`, `checks` or the conflict as sent for `tree`'s PR; what was sent before its head moved is done.
    pub(crate) fn send(&mut self, tree: &str, pr: &Pr, threads: &[&Thread], checks: &[&Check], conflict: bool) {
        let sent = self.sent.entry(tree.to_string()).or_default();
        if sent.head != pr.head {
            *sent = Sent { head: pr.head.clone(), ..Sent::default() };
        }
        sent.threads.extend(threads.iter().map(|t| t.id.clone()));
        sent.checks.extend(checks.iter().map(|c| c.name.clone()));
        sent.conflict |= conflict;
    }

    /// What the agent still works on: it pushes when done, which moves the head.
    pub(crate) fn fixing(&self, tree: &str, pr: &Pr) -> Option<&Sent> {
        self.sent.get(tree).filter(|s| s.head == pr.head && (s.items() > 0 || s.conflict))
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
        if !self.expanded.remove(thread) {
            self.expanded.insert(thread.to_string());
        }
    }
}

/// A resolve to flip back, offered on the notice it raised.
#[derive(Clone)]
pub(crate) struct Undo {
    tree: String,
    thread: String,
    resolved: bool,
}

pub(crate) struct Notice {
    pub(crate) text: String,
    pub(crate) undo: Option<Undo>,
    _hide: Task<()>,
}

pub(crate) struct PullRequest {
    pub(crate) title: Entity<InputState>,
    pub(crate) body: Entity<TextareaState>,
    pub(crate) reply: Entity<TextareaState>,
    /// The worktree whose PR the composer writes.
    pub(crate) composing: Option<String>,
    pub(crate) writing: bool,
    /// What the composer's or card's button says while `gh` works.
    pub(crate) busy: Option<&'static str>,
    pub(crate) error: Option<String>,
    pub(crate) ship_menu: bool,
    pub(crate) merge_menu: bool,
    /// The thread the reply box is under.
    pub(crate) replying: Option<String>,
    pub(crate) notice: Option<Notice>,
    pub(crate) track: Track,
}

impl PullRequest {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let title = cx.new(|cx| InputState::new(window, cx).placeholder("Title"));
        let body = cx.new(|cx| TextareaState::new(window, cx).placeholder("What this changes, and why").rows(8));
        let reply = cx.new(|cx| TextareaState::new(window, cx).placeholder("Reply…").auto_grow(2, 8));
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
        ];
        let state = Self {
            title,
            body,
            reply,
            composing: None,
            writing: false,
            busy: None,
            error: None,
            ship_menu: false,
            merge_menu: false,
            replying: None,
            notice: None,
            track: Track::default(),
        };
        (state, subs)
    }
}

impl Desktop {
    /// The PR of the worktree on screen, with the worktree.
    pub(crate) fn shown_pr(&self) -> Option<(String, &Pr)> {
        let tree = self.cwd()?;
        let pr = self.prs.get(&tree)?;
        Some((tree, pr))
    }

    /// Opens the composer for the worktree on screen, filled by the agent picked for commit messages or from the commits.
    pub(crate) fn open_pr_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        let (Some(tree), Some(repo)) = (self.cwd(), self.repo().cloned()) else { return };
        self.pr.composing = Some(tree);
        self.pr.error = None;
        if self.store.git.ai {
            self.write_pr(window, cx);
        } else {
            let (title, body) = from_commits(&repo.commits);
            self.fill_composer(title, body, window, cx);
        }
        self.pr.title.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    fn fill_composer(&mut self, title: String, body: String, window: &mut Window, cx: &mut Context<Self>) {
        self.pr.title.update(cx, |s, cx| s.set_value(title, window, cx));
        self.pr.body.update(cx, |s, cx| s.set_value(body, window, cx));
    }

    pub(crate) fn close_pr_composer(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pr.composing = None;
        self.pr.error = None;
        self.fill_composer(String::new(), String::new(), window, cx);
        cx.notify();
    }

    /// Asks the commit-message agent for a title and body from the branch's commits and diff.
    pub(crate) fn write_pr(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(cwd), Some(repo)) = (self.cwd(), self.repo()) else { return };
        let Some(base) = repo.base.clone().filter(|_| !self.pr.writing) else { return };
        let (tree, fallback) = (cwd.clone(), from_commits(&repo.commits));
        let argv = self.store.git.pr_argv(&self.store.agents.command(&self.store.git.agent));
        self.pr.writing = true;
        self.pr.error = None;
        let task = cx.background_executor().spawn(async move {
            let context = git::pr_context(&cwd, &base);
            run(&argv, &cwd, &context)
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |d, window, cx| {
                d.pr.writing = false;
                if d.pr.composing.as_deref() != Some(tree.as_str()) {
                    return cx.notify();
                }
                match res {
                    Ok(text) => {
                        let (title, body) = github::split_message(&text);
                        d.fill_composer(title, body, window, cx);
                    }
                    Err(e) => {
                        let (title, body) = fallback;
                        d.fill_composer(title, body, window, cx);
                        d.pr.error = Some(format!("Couldn't write the PR: {e}"));
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Pushes, then opens the PR the composer holds; the card takes its place at once.
    pub(crate) fn create_pr(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.pr.composing.clone() else { return };
        let Some(repo) = self.repos.get(&tree) else { return };
        let Some(base) = repo.base.clone() else { return };
        if self.pr.busy.is_some() || self.pr.writing {
            return;
        }
        let title = self.pr.title.read(cx).value().trim().to_string();
        if title.is_empty() {
            self.pr.title.update(cx, |s, cx| s.focus(window, cx));
            return cx.notify();
        }
        let body = self.pr.body.read(cx).value().to_string();
        let (dir, push_argv, draft) = (tree.clone(), self.push_argv(), self.store.git.draft);
        self.pr.busy = Some(if draft { "Creating draft PR…" } else { "Creating PR…" });
        self.pr.error = None;
        let argv = github::create_argv(&base, &title, draft);
        let task = cx.background_executor().spawn(async move {
            push(&dir, &push_argv)?;
            run(&argv, &dir, &body)
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |d, window, cx| {
                d.pr.busy = None;
                match res {
                    Ok(out) => {
                        let base = github::base_branch(&base).to_string();
                        let kind = if draft { "draft PR" } else { "PR" };
                        match github::created(&out) {
                            Some(number) => {
                                d.notify_pr(format!("Opened {kind} #{number} into {base}"), None, cx);
                                let url = out.lines().last().unwrap_or_default().trim().to_string();
                                d.prs.put(&tree, Pr { number, title, url, draft, base, ..Pr::default() }, Instant::now());
                            }
                            None => {
                                d.notify_pr(format!("Opened {kind} into {base}"), None, cx);
                                d.prs.expire(&tree, Instant::now());
                            }
                        }
                        d.close_pr_composer(window, cx);
                        d.poll_prs(cx);
                    }
                    Err(e) => d.pr.error = Some(e),
                }
                d.refresh_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn notify_pr(&mut self, text: String, undo: Option<Undo>, cx: &mut Context<Self>) {
        let hide = cx.spawn(async |this, cx| {
            cx.background_executor().timer(NOTICE).await;
            this.update(cx, |d, cx| {
                d.pr.notice = None;
                cx.notify();
            })
            .ok();
        });
        self.pr.notice = Some(Notice { text, undo, _hide: hide });
        cx.notify();
    }

    /// Runs `gh` in the worktree on screen while the card says `busy`, then `done` with what it printed.
    fn run_gh(&mut self, busy: &'static str, argv: Vec<String>, cx: &mut Context<Self>, done: impl FnOnce(&mut Self, String, &mut Context<Self>) + 'static) {
        let Some(dir) = self.cwd() else { return };
        if self.pr.busy.is_some() {
            return;
        }
        self.pr.busy = Some(busy);
        self.pr.merge_menu = false;
        let task = cx.background_executor().spawn(async move { run(&argv, &dir, "") });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                d.pr.busy = None;
                match res {
                    Ok(out) => done(d, out, cx),
                    Err(e) => d.error = Some(e),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Merges the PR on screen, then deletes its worktree if the card says to.
    pub(crate) fn merge_pr(&mut self, cx: &mut Context<Self>) {
        let Some((tree, pr)) = self.shown_pr() else { return };
        if pr.gate() != github::Gate::Ready {
            return;
        }
        let (method, number, base) = (self.pr.track.method, pr.number, pr.base.clone());
        self.run_gh("Merging…", github::merge_argv(number, method), cx, move |d, _, cx| {
            d.prs.edit(&tree, |pr| pr.state = PrState::Merged);
            d.prs.expire(&tree, Instant::now());
            d.notify_pr(format!("Merged PR #{number} into {base}"), None, cx);
            if d.pr.track.delete_after {
                d.delete_merged(&tree, cx);
            }
        });
    }

    pub(crate) fn mark_ready(&mut self, cx: &mut Context<Self>) {
        let Some((tree, pr)) = self.shown_pr() else { return };
        self.run_gh("Marking ready…", github::ready_argv(pr.number), cx, move |d, _, _| {
            d.prs.edit(&tree, |pr| pr.draft = false);
            d.prs.expire(&tree, Instant::now());
        });
    }

    /// A worktree of its own, unlike the project's checkout, which stays.
    pub(crate) fn deletable(&self, tree: &str) -> bool {
        self.project.as_ref().and_then(|p| self.worktrees.get(p)).is_some_and(|ws| ws.iter().any(|w| w.path == tree && !w.main))
    }

    /// The name of the agent fixes go to.
    pub(crate) fn fix_agent(&self) -> &'static str {
        self.chat_target(&self.chat_choices()).and_then(|id| self.agents.get(&id)).map_or("agent", |a| theme::provider_name(&a.provider))
    }

    /// Deletes a merged PR's worktree, keeping its branch; one with changes asks first.
    pub(crate) fn delete_merged(&mut self, tree: &str, cx: &mut Context<Self>) {
        let Some(project) = self.project.clone().filter(|_| self.deletable(tree)) else { return };
        let tree = tree.to_string();
        let dir = tree.clone();
        // Untracked files count even when Changes hides them, so this reads the tree afresh.
        let task = cx.background_executor().spawn(async move { git::read(&dir, true).is_none_or(|r| !r.files.is_empty()) });
        cx.spawn(async move |this, cx| {
            let dirty = task.await;
            this.update(cx, |d, cx| {
                if dirty {
                    d.ask_delete_worktree(project, tree, cx);
                } else {
                    d.delete_worktree(Removal { project, tree, branch: None, delete_branch: false, teardown: true }, cx);
                }
            })
            .ok();
        })
        .detach();
    }

    /// Opens the file thread `id` is on as the PR changes it, scrolled to the thread; one whose line is gone shows in Comments instead.
    pub(crate) fn view_thread(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some((_, pr)) = self.shown_pr() else { return };
        let Some(t) = pr.threads.iter().find(|t| t.id == id) else { return };
        if t.outdated || t.line.is_none() {
            self.pr.track.comments = true;
            return cx.notify();
        }
        let doc = Doc::PrFile { base: format!("{}/{}", self.store.git.remote, pr.base), path: t.path.clone() };
        self.diff.reveal = Some(id.to_string());
        self.open_doc(doc, false, cx);
    }

    /// Pins the threads of the PR on screen to the PR diffs.
    pub(crate) fn pin_threads(&mut self) {
        let threads = self.cwd().and_then(|tree| self.prs.get(&tree)).map_or(&[][..], |pr| pr.threads.as_slice());
        self.diff.pin(threads);
    }

    /// Pins the threads again and reads the PR diffs again, for what was pushed or said since.
    pub(crate) fn refresh_pr_diffs(&mut self, cx: &mut Context<Self>) {
        self.pin_threads();
        let panes: Vec<PaneId> = self.diff.panes.iter().filter(|(_, v)| matches!(v.at, At::Base(_))).map(|(p, _)| *p).collect();
        for pane in panes {
            self.load_diff(pane, cx);
        }
    }

    /// Pastes `prompt` into the agent this worktree's chat goes to and sends it.
    fn send_to_agent(&mut self, prompt: &str, cx: &mut Context<Self>) -> bool {
        if self.agents.observe_only() {
            return false;
        }
        let Some(target) = self.chat_target(&self.chat_choices()) else {
            self.error = Some("Start an agent in this worktree to send it this.".into());
            cx.notify();
            return false;
        };
        let Some((terminal, provider)) = self.agents.get(&target).map(|a| (a.terminal_id.clone(), a.provider.clone())) else { return false };
        if !self.terminals.sessions.term(&terminal).is_some_and(|t| t.mode(2004)) {
            self.error = Some("This agent can't take pasted text right now.".into());
            cx.notify();
            return false;
        }
        self.send_input(&terminal, &term::paste_bytes(prompt, true), cx);
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(crate::add_to_chat::SEND_AFTER).await;
            this.update(cx, |d, cx| d.send_input(&terminal, b"\r", cx))
        })
        .detach();
        self.notify_pr(format!("Sent to {}", theme::provider_name(&provider)), None, cx);
        true
    }

    /// Hands failed checks and review threads of the PR on screen to its agent.
    pub(crate) fn send_fix(&mut self, checks: &[String], threads: &[String], cx: &mut Context<Self>) {
        let Some((tree, pr)) = self.shown_pr() else { return };
        let pr = pr.clone();
        let checks: Vec<&Check> = pr.runs.iter().filter(|c| checks.contains(&c.name)).collect();
        let threads: Vec<&Thread> = pr.threads.iter().filter(|t| threads.contains(&t.id)).collect();
        if checks.is_empty() && threads.is_empty() {
            return;
        }
        if self.send_to_agent(&fix_prompt(&pr, &checks, &threads), cx) {
            self.pr.track.send(&tree, &pr, &threads, &checks, false);
        }
    }

    pub(crate) fn send_conflict(&mut self, cx: &mut Context<Self>) {
        let Some((tree, pr)) = self.shown_pr() else { return };
        let (prompt, pr) = (conflict_prompt(&pr.base), pr.clone());
        if self.send_to_agent(&prompt, cx) {
            self.pr.track.send(&tree, &pr, &[], &[], true);
        }
    }

    pub(crate) fn start_reply(&mut self, thread: String, window: &mut Window, cx: &mut Context<Self>) {
        let to = self.shown_pr().and_then(|(_, pr)| pr.threads.iter().find(|t| t.id == thread)?.comments.first().map(|c| c.author.clone()));
        self.pr.replying = Some(thread);
        self.pr.reply.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.set_placeholder(format!("Reply to {}…", to.unwrap_or_default()), window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn cancel_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pr.replying = None;
        self.pr.reply.update(cx, |s, cx| s.set_value("", window, cx));
        cx.notify();
    }

    /// Posts the reply at once and shows it as sent; a failure takes it back out.
    pub(crate) fn send_reply(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(dir), Some(thread)) = (self.cwd(), self.pr.replying.clone()) else { return };
        let body = self.pr.reply.read(cx).value().trim().to_string();
        if body.is_empty() {
            return;
        }
        let me = match &self.prs.gh {
            Some(Some(github::GhStatus::Ready { login, .. })) => login.clone(),
            _ => "You".into(),
        };
        let at = crate::util::now_ms() as u64;
        self.prs.edit(&dir, |pr| {
            if let Some(t) = pr.thread_mut(&thread) {
                t.comments.push(github::Comment { author: me, body: body.clone(), at, hunk: String::new() });
            }
        });
        self.cancel_reply(window, cx);
        let argv = github::reply_argv(&thread, &body);
        let tree = dir.clone();
        let task = cx.background_executor().spawn(async move { run(&argv, &dir, "") });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                if let Err(e) = res {
                    d.prs.expire(&tree, Instant::now());
                    d.error = Some(format!("Couldn't post the reply: {e}"));
                    d.poll_prs(cx);
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// Resolves or reopens a thread at once, with Undo; a failure flips it back.
    pub(crate) fn resolve_thread(&mut self, thread: String, resolved: bool, undo: bool, cx: &mut Context<Self>) {
        let Some(dir) = self.cwd() else { return };
        self.resolve_in(dir, thread, resolved, undo, cx);
    }

    fn resolve_in(&mut self, dir: String, thread: String, resolved: bool, undo: bool, cx: &mut Context<Self>) {
        self.prs.edit(&dir, |pr| {
            if let Some(t) = pr.thread_mut(&thread) {
                t.resolved = resolved;
            }
        });
        self.pr.track.expanded.remove(&thread);
        if undo {
            let text = if resolved { "Resolved the conversation" } else { "Reopened the conversation" };
            self.notify_pr(text.into(), Some(Undo { tree: dir.clone(), thread: thread.clone(), resolved: !resolved }), cx);
        } else {
            self.pr.notice = None;
        }
        let argv = github::resolve_argv(&thread, resolved);
        let tree = dir.clone();
        let task = cx.background_executor().spawn(async move { run(&argv, &dir, "") });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                if let Err(e) = res {
                    d.prs.edit(&tree, |pr| {
                        if let Some(t) = pr.thread_mut(&thread) {
                            t.resolved = !resolved;
                        }
                    });
                    d.error = Some(format!("Couldn't update the conversation: {e}"));
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    pub(crate) fn undo_pr(&mut self, cx: &mut Context<Self>) {
        let Some(Undo { tree, thread, resolved }) = self.pr.notice.take().and_then(|n| n.undo) else { return };
        self.resolve_in(tree, thread, resolved, false, cx);
    }
}

#[cfg(test)]
mod tests {
    use super::{Ship, Track, conflict_prompt, duration, fix_prompt, fix_summary, from_commits, gate_text, hunk_tail, review_lines, ship, shown_checks, to_fix};
    use crate::git_ui::pull_requests::PrItem;
    use git::github::{Check, Comment, Gate, Outcome, Pr, Review, Thread};
    use git::{Commit, FileStat, Repo};

    fn repo(files: usize, ahead: usize, unpushed: Option<usize>) -> Repo {
        let file = |i: usize| FileStat { path: format!("{i}.rs"), added: 1, removed: 0, staged: false, unstaged: true, status: 'M' };
        Repo { branch: "fix".into(), base: Some("origin/main".into()), ahead, unpushed, files: (0..files).map(file).collect(), ..Repo::default() }
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

    fn thread(id: &str) -> Thread {
        let said = |author: &str, body: &str| Comment { author: author.into(), body: body.into(), at: 0, hunk: "@@ -1,4 +1,4 @@\n a\n b\n-c\n+d\n e".into() };
        Thread { id: id.into(), path: "src/a.rs".into(), line: Some(42), comments: vec![said("mira", "Handle None here."), said("bo", "Agreed.")], ..Thread::default() }
    }

    #[test]
    fn a_fix_lists_each_failed_check_with_its_log_then_each_thread_with_the_code_it_is_on() {
        let check = Check { name: "test".into(), outcome: Outcome::Failed, url: "https://github.com/acme/app/actions/runs/1".into(), secs: Some(3) };
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
        track.send("/a", &pr(), &[&t], &[], false);
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
        track.send("/a", &pr(), &[&thread("T1")], &[], false);
        let pushed = Pr { head: "bbb".into(), ..pr() };
        track.send("/a", &pushed, &[], &[], true);
        let sent = track.fixing("/a", &pushed).unwrap();
        assert!(sent.threads.is_empty() && sent.conflict);
    }

    #[test]
    fn worktrees_are_deleted_after_merging_unless_unticked() {
        assert!(Track::default().delete_after);
    }

    fn check(name: &str, outcome: Outcome) -> Check {
        Check { name: name.into(), outcome, url: String::new(), secs: None }
    }

    #[test]
    fn what_is_left_to_fix_leaves_out_what_the_agent_already_has_and_threads_someone_answered() {
        let open = |id: &str| Thread { comments: thread(id).comments[..1].to_vec(), ..thread(id) };
        let pr = Pr { runs: vec![check("lint", Outcome::Failed), check("test", Outcome::Failed), check("build", Outcome::Passed)], threads: vec![open("T1"), thread("T2"), open("T3")], ..pr() };
        let mut track = Track::default();
        track.send("/a", &pr, &[&pr.threads[0]], &[&pr.runs[0]], false);
        let (checks, threads) = to_fix(&pr, track.fixing("/a", &pr));
        assert_eq!(checks.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["test"]);
        assert_eq!(threads.iter().map(|t| t.id.as_str()).collect::<Vec<_>>(), ["T3"]);
    }

    #[test]
    fn the_fix_callout_counts_things_then_names_them() {
        assert_eq!(fix_summary(1, 2), ("3 things to fix".into(), "1 failed check · 2 review comments".into()));
        assert_eq!(fix_summary(0, 1), ("1 thing to fix".into(), "1 review comment".into()));
    }

    #[test]
    fn failed_checks_show_first_then_running_ones_and_the_rest_are_counted() {
        let pr = Pr { runs: vec![check("a", Outcome::Passed), check("b", Outcome::Pending), check("c", Outcome::Failed), check("d", Outcome::Skipped)], ..pr() };
        let (shown, hidden) = shown_checks(&pr, 3);
        assert_eq!(shown.iter().map(|c| c.name.as_str()).collect::<Vec<_>>(), ["c", "b", "a"]);
        assert_eq!(hidden, 1);
    }

    #[test]
    fn durations_read_in_minutes_and_seconds() {
        assert_eq!([duration(42), duration(120), duration(75)], ["42s", "2m", "1m 15s"]);
    }

    #[test]
    fn the_review_lists_verdicts_then_who_is_still_asked() {
        let pr = Pr { reviews: vec![("mira".into(), Review::Approved), ("bo".into(), Review::None), ("li".into(), Review::ChangesRequested)], requested: vec!["kai".into(), "jo".into()], ..pr() };
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
}
