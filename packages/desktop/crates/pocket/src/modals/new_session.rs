pub(crate) mod picker;

use agents::locals::Local;
use agents::{Agents, CreateReply, Event};
use chat::Action;
use crate::chat::composer::send_button::send_button;
use crate::chat::composer::suggest::Prompt;
use crate::chat::composer::{MAX_ROWS, PLACEHOLDER};
use crate::chat::mention;
use crate::composer::{self, Composer};
use crate::creating::Create;
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::field::Field;
use crate::fs::fs_for;
use crate::modals::form::default_base;
use crate::terminals::link;
use crate::util::{LOCAL_ICON, WORKTREE_ICON};
use git::github;
use gpui_kit::component::input::{InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use picker::Picker;
use serde_json::{Value, json};
use store::LaunchPick;
use std::collections::HashSet;
use std::time::Instant;
use theme::*;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Access {
    Ask,
    Edits,
    Auto,
}

impl Access {
    pub fn wire(self) -> &'static str {
        match self {
            Access::Ask => "ask",
            Access::Edits => "edits",
            Access::Auto => "auto",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Access::Ask => "Ask",
            Access::Edits => "Auto-accept edits",
            Access::Auto => "Auto",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            Access::Ask => "Ask before commands and file changes.",
            Access::Edits => "Auto-approve edits, ask before other actions.",
            Access::Auto => "A reviewer model approves or denies actions.",
        }
    }
}

pub struct NewForm {
    /// The new-session page fills the main area.
    pub(crate) shown: bool,
    pub(crate) composer: Composer,
    search: Entity<InputState>,
    pr: Entity<InputState>,
    draft: Draft,
    field: Field,
}

/// Where a new worktree's branch comes from.
#[derive(Clone, PartialEq, Debug, Default)]
pub(crate) enum Source {
    /// A new branch named after the worktree, from the base.
    #[default]
    New,
    /// An existing branch, local or on origin.
    Branch(String),
    /// A GitHub pull request's head, as typed.
    Pr(String),
}

impl Source {
    /// What a create's `checkout.new` asked for.
    fn of(new: &Value) -> Self {
        match (new["branch"].as_str(), new["pr"].as_str()) {
            (Some(b), _) => Source::Branch(b.to_string()),
            (_, Some(pr)) => Source::Pr(pr.to_string()),
            _ => Source::New,
        }
    }
}

/// The form's choices besides its text inputs.
struct Draft {
    /// Branches and worktree folders a new worktree's name must not reuse.
    taken: HashSet<String>,
    seed: usize,
    picked_checkout: Target,
    /// pocketd takes Locals; without them every session gets a Worktree, whatever was picked.
    locals_offered: bool,
    /// The Local an earlier send made, so a retry after a failed create starts there instead of making another.
    local: Option<String>,
    repo: Option<String>,
    branches: Vec<(String, Option<i64>)>,
    /// Branches on origin with no local twin.
    remote_branches: Vec<String>,
    source: Source,
    base: usize,
    /// The base to pick once the branches load, instead of the repo's.
    want_base: Option<String>,
    reply: CreateReply,
    picker: Option<Picker>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            taken: HashSet::new(),
            seed: 0,
            picked_checkout: Target::NewLocal,
            locals_offered: true,
            local: None,
            repo: None,
            branches: Vec::new(),
            remote_branches: Vec::new(),
            source: Source::New,
            base: 0,
            want_base: None,
            reply: CreateReply::default(),
            picker: None,
        }
    }
}

impl Draft {
    fn base_branch(&self) -> String {
        self.branches.get(self.base).map(|(b, _)| b.clone()).unwrap_or_default()
    }

    fn auto_name(&self) -> String {
        free_name(self.seed, &self.taken)
    }

    /// The new worktree's name, or the branch or pull request to open.
    fn name(&self) -> String {
        match &self.source {
            Source::New => self.auto_name(),
            Source::Branch(name) | Source::Pr(name) => name.clone(),
        }
    }

    /// The number of the linked pull request.
    fn linked_pr(&self) -> Option<u32> {
        match &self.source {
            Source::Pr(typed) => parse_pr(typed),
            _ => None,
        }
    }

    /// pocketd names a new branch from its prompt; the name sent is a placeholder.
    fn auto_names(&self, prompt: &str, offered: bool) -> bool {
        offered && self.in_worktree() && self.source == Source::New && !prompt.trim().is_empty()
    }

    /// A pull request always gets its own worktree; the checkout picked returns once it is unlinked.
    fn in_worktree(&self) -> bool {
        self.picked_checkout == Target::Worktree || !self.locals_offered || matches!(self.source, Source::Pr(_))
    }

    fn link_pr(&mut self, typed: &str) {
        if parse_pr(typed).is_some() {
            (self.source, self.picker) = (Source::Pr(typed.trim().to_string()), None);
        }
    }

    fn unlink_pr(&mut self) {
        self.source = Source::New;
    }

    /// A new branch from the `i`th local branch.
    fn pick_base(&mut self, i: usize) {
        (self.base, self.source, self.picker) = (i, Source::New, None);
    }

    fn target(&self) -> Target {
        if self.in_worktree() { Target::Worktree } else { self.picked_checkout }
    }

    fn targets(&self) -> Vec<Target> {
        [Some(Target::Worktree), self.locals_offered.then_some(Target::NewLocal)].into_iter().flatten().collect()
    }

    fn pick_checkout(&mut self, target: Target) {
        (self.picked_checkout, self.picker) = (target, None);
    }

    fn open_branch(&mut self, branch: String) {
        (self.source, self.picker) = (Source::Branch(branch), None);
    }

    fn ready(&self, name: &str) -> bool {
        let place = match (self.in_worktree(), &self.source) {
            (false, _) => self.repo.is_some(),
            (true, Source::New) => self.repo.is_some() && !self.branches.is_empty() && name_problem(name, &self.taken).is_none(),
            (true, Source::Branch(_)) => self.repo.is_some() && !name.is_empty(),
            (true, Source::Pr(_)) => self.repo.is_some() && parse_pr(name).is_some(),
        };
        place && !self.reply.waiting()
    }

    fn retry_local<'a>(&self, mut locals: impl Iterator<Item = &'a Local>) -> Option<String> {
        let id = self.local.as_ref()?;
        locals.any(|l| &l.id == id).then(|| id.clone())
    }

    /// The folder pocketd will likely make for `name`; `Creates::started` swaps in the one it made.
    fn folder(&self, name: &str) -> String {
        match self.source {
            Source::New => name.to_string(),
            Source::Branch(_) => name.replace('/', "-"),
            Source::Pr(_) => format!("pr-{}", parse_pr(name).unwrap_or_default()),
        }
    }

    /// `local` is the new Local's id the session starts in; without one it gets a new worktree.
    fn spec(&self, pick: &LaunchPick, project: &str, local: Option<&str>, name: &str, prompt: &str) -> Value {
        let checkout = match (local, &self.source) {
            (Some(id), _) => agents::locals::checkout(id),
            (None, Source::New) => json!({"new": {"name": name, "base": self.base_branch()}}),
            (None, Source::Branch(_)) => json!({"new": {"branch": github::base_branch(name)}}),
            (None, Source::Pr(_)) => json!({"new": {"pr": name}}),
        };
        pick.spec(project, checkout, prompt.trim())
    }

    /// Reopening the page clears its error row, so a failure that arrives while it is gone goes to the window's error line.
    fn failed(&mut self, request: &str, message: String, detail: String, open: bool) -> Option<String> {
        if open && self.reply.answer(request, Some((message.clone(), detail))) {
            return None;
        }
        Some(message)
    }

    /// The page opened again: a kept draft keeps its checkout, base and Local, and waits on a send still out.
    fn reopen(&mut self, keep: bool, worktree: bool, locals_offered: bool) {
        self.want_base = Some(self.base_branch()).filter(|b| keep && !b.is_empty());
        if !keep {
            (self.source, self.local) = (Source::New, None);
            self.picked_checkout = if worktree { Target::Worktree } else { Target::NewLocal };
        }
        if !self.reply.waiting() {
            self.reply = CreateReply::default();
        }
        (self.locals_offered, self.picker) = (locals_offered, None);
        self.taken.clear();
    }

    /// Another project's base, branch and Local don't carry over.
    fn pick_repo(&mut self, repo: String) {
        if self.repo.as_ref() != Some(&repo) {
            (self.source, self.want_base, self.local) = (Source::New, None, None);
            self.taken.clear();
        }
        self.repo = Some(repo);
        self.branches.clear();
        self.remote_branches.clear();
        self.base = 0;
    }
}

/// A checkout choice as its chip and menu row show it.
struct Checkout {
    glyph: &'static str,
    label: &'static str,
    hint: &'static str,
}

/// Where a session starts.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Target {
    Worktree,
    NewLocal,
}

fn checkout_choice(target: Target) -> Checkout {
    match target {
        Target::Worktree => Checkout { glyph: WORKTREE_ICON, label: "Worktree", hint: "Its own branch and copy of the files" },
        Target::NewLocal => Checkout { glyph: LOCAL_ICON, label: "Local", hint: "A new place over the project's checkout: files and branch are shared" },
    }
}

/// Branches the branch menu shows while it has no search.
const RECENT: usize = 20;

/// The most rows a branch search lists.
const FOUND: usize = 50;

/// The branch menu's rows: the recent local branches, or every local then origin branch holding `search`. Each carries its index among the local branches, or none when only origin has it.
fn listed_branches<'a>(branches: &'a [(String, Option<i64>)], remote: &'a [String], search: &str) -> Vec<(Option<usize>, &'a str)> {
    let search = search.trim().to_lowercase();
    let local = branches.iter().enumerate().map(|(i, (b, _))| (Some(i), b.as_str()));
    if search.is_empty() {
        return local.take(RECENT).collect();
    }
    local.chain(remote.iter().map(|b| (None, b.as_str()))).filter(|(_, b)| b.to_lowercase().contains(&search)).take(FOUND).collect()
}

/// The number in "123", "#123" or a pull request URL.
fn parse_pr(s: &str) -> Option<u32> {
    let s = s.trim();
    let n = match s.strip_prefix("https://").or_else(|| s.strip_prefix("http://")) {
        Some(url) => match url.split('/').collect::<Vec<_>>()[..] {
            [_, _, _, "pull", n, ..] => n.split(['?', '#']).next().unwrap_or(n),
            _ => return None,
        },
        None => s.strip_prefix('#').unwrap_or(s),
    };
    n.bytes().all(|b| b.is_ascii_digit()).then(|| n.parse().ok()).flatten().filter(|&n| n > 0)
}

/// Origin's branches count: closing a worktree deletes its folder and local branch, but its pushed branch and pull request live on.
fn taken_names(branches: &[(String, Option<i64>)], remote: &[String], folders: impl Iterator<Item = String>) -> HashSet<String> {
    branches.iter().map(|(b, _)| b.clone()).chain(remote.iter().cloned()).chain(folders).collect()
}

/// Case-insensitive because APFS is; a branch `a/b` owns the name `a`.
fn is_taken(name: &str, taken: &HashSet<String>) -> bool {
    taken.iter().any(|t| t.split('/').next().unwrap_or(t).eq_ignore_ascii_case(name))
}

fn free_name(seed: usize, taken: &HashSet<String>) -> String {
    let words = petname::Petnames::medium();
    let (adjectives, nouns) = (&words.adjectives, &words.nouns);
    let count = adjectives.len() * nouns.len();
    (0..count).map(|i| (seed + i) % count).map(|n| format!("{}-{}", adjectives[n / nouns.len()], nouns[n % nouns.len()])).find(|n| !is_taken(n, taken)).unwrap_or_else(|| unique("worktree", taken))
}

/// `base`, or `base-2`, `base-3`… when taken.
fn unique(base: &str, taken: &HashSet<String>) -> String {
    std::iter::once(base.to_string()).chain((2..).map(|n| format!("{base}-{n}"))).find(|n| !is_taken(n, taken)).unwrap()
}

/// Why `name` can't name a new worktree and its branch, if it can't.
fn name_problem(name: &str, taken: &HashSet<String>) -> Option<&'static str> {
    let valid = !name.is_empty()
        && name.chars().all(|c| c.is_ascii_alphanumeric() || "-_.".contains(c))
        && !name.starts_with(['-', '.'])
        && !name.ends_with('.')
        && !name.ends_with(".lock")
        && !name.contains("..")
        && name != "HEAD";
    if is_taken(name, taken) {
        Some("A worktree or branch with this name already exists")
    } else if !valid {
        Some("Use letters, digits, - _ or .")
    } else {
        None
    }
}

/// `typed`, with `added` after it once there are words to keep.
fn appended(typed: &str, added: Option<String>) -> String {
    match added {
        Some(added) if !typed.trim().is_empty() => format!("{}\n\n{added}", typed.trim_end()),
        Some(added) => added,
        None => typed.to_string(),
    }
}

pub(crate) fn may_start(agents: &Agents) -> bool {
    !agents.observe_only()
}

fn default_first(branches: &mut [(String, Option<i64>)], preferred: &str, current: &str) {
    let default = default_base(branches.iter().map(|(b, _)| b.as_str()), preferred, current);
    if !branches.is_empty() {
        branches[..=default].rotate_right(1);
    }
}

impl NewForm {
    /// The project the session starts in.
    pub(crate) fn project(&self) -> Option<&str> {
        self.draft.repo.as_deref()
    }

    /// pocketd's caps change on every connect.
    pub(crate) fn offer_locals(&mut self, offered: bool) {
        self.draft.locals_offered = offered;
    }

    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let dropped = composer::drop_images(cx.entity().downgrade(), slot);
        let prompt = cx.new(|cx| TextareaState::new(window, cx).placeholder(PLACEHOLDER).auto_grow(1, MAX_ROWS).submit_on_enter(true).on_drop_paths(dropped).decorate(mention::decorations));
        let models = cx.new(|cx| InputState::new(window, cx).placeholder("Search models"));
        let composer = Composer::new(prompt.clone(), models.clone(), slot, |d| d.new_form.draft.picker = None, |d, error| d.new_form.draft.reply.error = Some(error));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search branches…"));
        let pr = cx.new(|cx| InputState::new(window, cx).placeholder("123, #123 or a PR URL"));
        let subs = vec![
            cx.subscribe_in(&prompt, window, |this, _, ev: &InputEvent, window, cx| {
                match ev {
                    InputEvent::PressEnter { shift: false, .. } => this.start_session(window, cx),
                    InputEvent::Change => this.draft_changed(Prompt::Launch, cx),
                    _ => {}
                }
            }),
            cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&models, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe_in(&pr, window, |this, pr, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary, .. } = ev {
                    let typed = pr.read(cx).value().to_string();
                    this.new_form.draft.link_pr(&typed);
                    if *secondary {
                        this.start_session(window, cx);
                    }
                }
                cx.notify()
            }),
        ];
        (Self { shown: false, composer, search, pr, draft: Draft::default(), field: Field::default() }, subs)
    }
}

pub(crate) fn slot(d: &mut Desktop) -> &mut Composer {
    &mut d.new_form.composer
}

impl Desktop {
    /// What `project` last started, else the default agent, with an agent this app knows.
    pub(crate) fn last_pick(&self, project: Option<&str>) -> LaunchPick {
        let repo = project.and_then(|p| self.store.repos.get(p));
        self.store.agents.pick(repo.map(|r| r.launch.clone()).unwrap_or_default())
    }

    /// A draft not yet sent keeps its words, images, checkout and project; `prompt` adds to its words,
    /// and since it quotes the Project on screen, the draft moves there.
    pub fn reset_new_form(&mut self, prompt: Option<String>, worktree: bool, window: &mut Window, cx: &mut Context<Self>) {
        let worktree = worktree || self.store.agents.new_worktree;
        let f = &mut self.new_form;
        let typed = f.composer.prompt.read(cx).value().to_string();
        let keep = !typed.trim().is_empty() || !f.composer.draft.images.is_empty();
        let keep_project = keep && prompt.is_none();
        let text = appended(&typed, prompt);
        f.draft.seed = crate::util::now_ms() as usize;
        f.field.hide();
        f.composer.prompt.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
        if !keep {
            f.composer.draft.clear();
        }
        f.draft.reopen(keep, worktree, self.agents.locals_offered());
        f.composer.draft.menu = None;
        let repo = if keep_project { f.draft.repo.clone() } else { None };
        match repo.or_else(|| self.project.clone()) {
            Some(repo) => self.pick_repo(repo, window, cx),
            None => {
                f.draft.repo = None;
                f.draft.branches.clear();
            }
        }
    }

    pub(super) fn pick_repo(&mut self, repo: String, window: &mut Window, cx: &mut Context<Self>) {
        let folders = self.store.worktrees_dir(&repo, &self.host_agents.home_of(&repo));
        self.new_form.draft.pick_repo(repo.clone());
        let dir = repo.clone();
        let (runner, fs) = (hosts::runner_for(&dir), fs_for(&dir));
        let task = cx.background_executor().spawn(async move {
            let current = git::read(&runner, &dir, true).map(|r| r.branch).unwrap_or_default();
            let branches = git::dated_branches(&runner, &dir);
            let entries = fs.local().and_then(|()| std::fs::read_dir(&folders)).into_iter().flatten().flatten();
            let remote = git::remote_branches(&runner, &dir);
            let taken = taken_names(&branches, &remote, entries.filter_map(|e| e.file_name().into_string().ok()));
            (current, branches, taken, remote)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (current, mut branches, taken, remote) = task.await;
            this.update(cx, |d, cx| {
                let f = &mut d.new_form;
                if f.draft.repo.as_ref() != Some(&repo) {
                    return;
                }
                default_first(&mut branches, f.draft.want_base.as_deref().unwrap_or_default(), &current);
                f.draft.base = 0;
                f.draft.branches = branches;
                f.draft.remote_branches = remote;
                f.draft.taken = taken;
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The agent the page starts, picked here or the Project's last.
    pub(crate) fn new_form_pick(&self) -> LaunchPick {
        let repo = self.new_form.draft.repo.as_deref();
        self.new_form.composer.draft.pick(repo, || self.last_pick(repo))
    }

    fn session_ready(&self) -> bool {
        self.new_form.draft.ready(&self.new_form.draft.name())
    }

    fn open_branch(&mut self, branch: String, window: &mut Window, cx: &mut Context<Self>) {
        self.new_form.draft.open_branch(branch);
        self.start_session(window, cx);
    }

    /// A new worktree's page leaves at once for its progress; a session's waits for pocketd.
    pub(crate) fn start_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session_ready() {
            return;
        }
        if self.terminals.link.stale().is_some() {
            self.error = Some(link::UPDATING.into());
            return cx.notify();
        }
        let name = self.new_form.draft.name();
        let prompt = self.new_form.composer.prompt.read(cx).value().to_string();
        let Some(project) = self.new_form.draft.repo.clone() else { return };
        let local = match self.new_form.draft.target() {
            Target::NewLocal => {
                let id = self.new_form.draft.retry_local(self.agents.locals_in(&project)).unwrap_or_else(|| self.new_local(&project));
                self.new_form.draft.local = Some(id.clone());
                self.store.collapsed.remove(&project);
                self.select_tree(project.clone(), Some(id.clone()), cx);
                Some(id)
            }
            Target::Worktree => None,
        };
        let (pick, sent) = (self.new_form_pick(), self.new_form.composer.draft.prompt(&prompt));
        let f = &self.new_form.draft;
        let (mut spec, worktree, folder) = (f.spec(&pick, &project, local.as_deref(), &name, &sent), f.in_worktree(), f.folder(&name));
        if f.auto_names(&prompt, self.agents.names_offered()) {
            spec["checkout"]["new"]["autoName"] = true.into();
        }
        let info = self.agents.provider_info(&pick.provider);
        self.store.agents.launch(&mut spec, info.plan, &info.efforts);
        self.store.repos.entry(project.clone()).or_default().launch = pick;
        if worktree {
            self.store.collapsed.remove(&project);
        }
        self.store.save();
        let Some(request) = self.send_create(spec.clone()) else { return cx.notify() };
        if !worktree {
            self.creates.sent_in_place(request.clone(), spec["prompt"].as_str().unwrap_or_default());
            self.new_form.draft.reply.sent(request);
            return cx.notify();
        }
        let path = format!("{}/{folder}", self.store.worktrees_dir(&project, &self.host_agents.home_of(&project)));
        self.creates.list.push(Create::new(request, path.clone(), spec, Instant::now()));
        self.clear_new_form(window, cx);
        self.leave_new_session(window, cx);
        self.select_tree(project, Some(path), cx);
    }

    /// pocketd's replies to a create. The Terminal opens once pocketd lists it (`Terminals::arrived`).
    pub(crate) fn on_launch(&mut self, ev: Event, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            Event::Creating { request, terminal, cwd, setup } => {
                if self.terminals.tab_chats.contains_key(&request) {
                    self.store.chats.insert(terminal.clone());
                    self.save_soon(cx);
                }
                self.terminals.created(&request, terminal.clone(), cwd.clone(), setup);
                self.creates.made_terminal(&request, terminal.clone(), Instant::now());
                self.daemon.send(json!({"op": "list"}));
                if self.creates.get(&request).is_some() {
                    self.store.track(&cwd);
                    if self.store.agents.start_as_chat {
                        self.store.chats.insert(terminal.clone());
                    }
                    self.save_soon(cx);
                }
                if let Some(expected) = self.creates.started(&request, cwd.clone(), &mut self.worktrees) {
                    if self.worktree.as_ref() == Some(&expected) {
                        self.worktree = Some(cwd);
                    }
                    self.refresh_git(cx);
                } else if self.new_form.draft.reply.answer(&request, None) {
                    if self.store.agents.start_as_chat {
                        self.store.chats.insert(terminal);
                        self.save_soon(cx);
                    }
                    self.clear_new_form(window, cx);
                    if self.new_form.shown {
                        self.leave_new_session(window, cx);
                    }
                }
            }
            Event::Progress { request, step, note } => {
                if let Some(c) = self.creates.get(&request) {
                    c.reach(&step, &note, Instant::now());
                }
            }
            Event::Created { request, agent } => {
                if let Some(c) = self.creates.get(&request) {
                    c.finish(agent, Instant::now());
                }
            }
            Event::CreateFailed { request, message, detail, .. } => {
                self.terminals.tab_chats.remove(&request);
                self.creates.failed_in_place(&request);
                if let Some(c) = self.creates.get(&request) {
                    c.fail(message, detail, Instant::now());
                } else if let Some(message) = self.new_form.draft.failed(&request, message, detail, self.new_form.shown)
                {
                    self.error = Some(message);
                }
            }
            _ => {}
        }
        cx.notify();
    }

    pub fn new_worktree(&mut self, _: &crate::actions::NewWorktree, window: &mut Window, cx: &mut Context<Self>) {
        self.new_session(None, true, window, cx);
    }

    pub fn new_session(&mut self, prompt: Option<String>, worktree: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !may_start(&self.agents) {
            return;
        }
        self.close_menus();
        (self.screen, self.new_form.shown) = (Screen::Sessions, true);
        self.reset_new_form(prompt, worktree, window, cx);
        cx.notify();
    }

    fn clear_new_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_form.composer.draft.clear();
        self.new_form.composer.prompt.update(cx, |s, cx| s.set_value("", window, cx));
    }

    pub(crate) fn leave_new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_form.shown = false;
        self.refocus(window, cx);
    }

    /// The page a project's + opens: a session in a new Worktree or Local of `project`.
    pub(crate) fn new_place(&mut self, project: String, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tree(project.clone(), None, cx);
        self.new_worktree(&crate::actions::NewWorktree, window, cx);
        if self.new_form.draft.repo.as_ref() != Some(&project) {
            self.pick_repo(project, window, cx);
        }
    }

    /// The page again, filled in as it was for a create that failed before git made its worktree.
    pub(crate) fn reopen_new_worktree(&mut self, c: &Create, window: &mut Window, cx: &mut Context<Self>) {
        self.new_session(Some(c.prompt().to_string()), true, window, cx);
        let f = &mut self.new_form.draft;
        f.pick_checkout(Target::Worktree);
        f.want_base = Some(c.base().to_string());
        f.source = Source::of(c.asked());
    }

    pub fn new_session_page(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let repo = f.repo.clone().unwrap_or_default();
        let name = self.repo_name(&repo);
        let project = self.project_select(cx);
        let checkout = self.checkout_select(cx);
        let pr = f.linked_pr();
        let branch = match pr {
            Some(n) => Some(div().h(px(30.)).px(px(6.)).flex().flex_none().items_center().gap(px(7.)).text_color(TEXT_3).child(icon("pull-request", 14., TEXT_3)).child(format!("based off PR #{n}"))),
            None => (f.target() == Target::Worktree).then(|| self.branch_select(cx)),
        };
        let link = (pr.is_none() && self.agents.opens()).then(|| self.pr_select(cx));
        let prompt = &self.new_form.composer.prompt;
        let empty = prompt.read(cx).value().trim().is_empty() && self.new_form.composer.draft.images.is_empty();
        let typing = !empty || prompt.read(cx).focus_handle(cx).is_focused(window);
        let ready = self.session_ready();
        let send = send_button("form-start".into(), Action::Send, empty, !ready).when(ready && !empty, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_session(window, cx))));
        let pill = pr.map(|n| {
            let unlink = div()
                .id("pr-unlink")
                .size(px(18.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .cursor_pointer()
                .hover(|s| s.bg(FILL_3))
                .child(icon("x", 10., TEXT_3))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.new_form.draft.unlink_pr();
                    cx.notify();
                }));
            div().h(px(26.)).pl(px(9.)).pr(px(4.)).flex().flex_none().items_center().gap(px(6.)).rounded(px(8.)).bg(FILL_2).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(icon("pull-request", 13., TEXT_2)).child(format!("#{n}")).child(unlink).into_any_element()
        });
        let textarea = Textarea::new(&self.new_form.composer.prompt);
        let trailing = link.map(IntoElement::into_any_element).into_iter().collect();
        let composer = self.composer(&self.new_form.composer, f.repo.clone(), textarea, typing, pill.into_iter().collect(), trailing, send, cx);
        let composer = self.suggest_keys(Prompt::Launch, composer, cx).children(self.suggest_list(Prompt::Launch, cx));
        let error = f.reply.error.clone().map(|(message, detail)| ui::failure(message, detail).w_full().max_w(px(600.)).mt(px(10.)));
        let footer = div()
            .w_full()
            .max_w(px(600.))
            .mt(px(10.))
            .flex()
            .items_center()
            .gap(px(2.))
            .text_size(px(13.))
            .child(project)
            .child(div().text_color(TEXT_4).child("·"))
            .child(checkout)
            .children(branch)
            .when(!self.idle_main(), |d| d.child(div().ml_auto().text_size(px(12.)).text_color(TEXT_4).whitespace_nowrap().child("esc to cancel")));
        let content = div()
            .flex()
            .flex_col()
            .items_center()
            .child(div().text_size(px(21.)).line_height(px(25.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("What should we build next?"))
            .child(div().w_full().max_w(px(600.)).mt(px(20.)).child(composer))
            .children(error)
            .child(footer);
        let body = self.with_field(|d| &mut d.new_form.field, "new-session-body", content, cx);
        div().flex_1().flex().flex_col().child(self.page_bar(vec![name, "New session".into()], Vec::new(), div(), cx)).child(body)
    }
}

#[cfg(test)]
mod tests {
    use super::{Draft, Local, Source, Target, appended, checkout_choice, default_first, is_taken, listed_branches, name_problem, parse_pr, taken_names};
    use agents::CreateReply;
    use serde_json::json;
    use store::LaunchPick;
    use std::collections::HashSet;

    fn sent(request: &str) -> CreateReply {
        let mut reply = CreateReply::default();
        reply.sent(request.into());
        reply
    }

    fn claude() -> LaunchPick {
        LaunchPick { provider: "claude".into(), ..LaunchPick::default() }
    }

    fn kept() -> Draft {
        Draft {
            repo: Some("/p".into()),
            branches: vec![("main".into(), None), ("dev".into(), None)],
            base: 1,
            source: Source::Branch("fix".into()),
            picked_checkout: Target::NewLocal,
            local: Some("local-1".into()),
            ..Draft::default()
        }
    }

    #[test]
    fn a_kept_draft_comes_back_with_its_checkout_base_and_local() {
        let mut draft = kept();
        draft.reopen(true, true, true);
        assert_eq!((draft.picked_checkout, draft.source, draft.local), (Target::NewLocal, Source::Branch("fix".into()), Some("local-1".into())));
        assert_eq!(draft.want_base.as_deref(), Some("dev"));
    }

    #[test]
    fn an_empty_draft_starts_over() {
        let mut draft = kept();
        draft.reopen(false, true, true);
        assert_eq!((draft.picked_checkout, draft.source, draft.local, draft.want_base), (Target::Worktree, Source::New, None, None));
    }

    #[test]
    fn a_send_still_out_is_still_awaited_when_the_page_comes_back() {
        let mut draft = Draft { reply: sent("r1"), ..kept() };
        draft.reopen(true, false, true);
        assert!(draft.reply.answer("r1", None));
    }

    #[test]
    fn another_project_forgets_the_last_ones_branch_base_and_local() {
        let mut draft = Draft { want_base: Some("dev".into()), ..kept() };
        draft.pick_repo("/p".into());
        assert_eq!((draft.source.clone(), draft.want_base.as_deref()), (Source::Branch("fix".into()), Some("dev")));
        draft.pick_repo("/q".into());
        assert_eq!((draft.source, draft.want_base, draft.local), (Source::New, None, None));
    }

    #[test]
    fn a_prompt_opening_the_page_follows_the_words_already_typed() {
        assert_eq!(appended("Fix CI \n", Some("Explain this".into())), "Fix CI\n\nExplain this");
        assert_eq!(appended(" \n", Some("Explain this".into())), "Explain this");
        assert_eq!(appended("Fix CI", None), "Fix CI");
    }

    #[test]
    fn the_picker_offers_only_a_new_worktree_and_a_new_local() {
        let draft = Draft::default();
        assert_eq!(draft.targets(), [Target::Worktree, Target::NewLocal]);
        assert_eq!(Draft { locals_offered: false, ..draft }.targets(), [Target::Worktree]);
        let local = checkout_choice(Target::NewLocal);
        assert_eq!((local.glyph, local.label), ("laptop", "Local"));
    }

    #[test]
    fn a_session_in_a_local_names_it_and_a_new_local_needs_a_project_and_pocketd_taking_locals() {
        let mut draft = Draft::default();
        assert_eq!(draft.spec(&claude(), "/p", Some("local-1"), "", "")["checkout"], json!({"local": "local-1"}));
        draft.pick_checkout(Target::NewLocal);
        assert!(!draft.ready(""));
        draft.repo = Some("/p".into());
        assert_eq!((draft.target(), draft.ready("")), (Target::NewLocal, true));
        draft.pick_checkout(Target::Worktree);
        assert_eq!(draft.target(), Target::Worktree);
    }

    #[test]
    fn without_locals_a_picked_local_becomes_a_worktree_until_pocketd_takes_them_again() {
        let mut draft = Draft { repo: Some("/p".into()), locals_offered: false, ..Draft::default() };
        assert_eq!((draft.target(), draft.ready("")), (Target::Worktree, false));
        draft.locals_offered = true;
        assert_eq!((draft.target(), draft.ready("")), (Target::NewLocal, true));
    }

    #[test]
    fn a_retry_after_a_failed_create_reuses_the_local_it_made_while_it_still_exists() {
        let local = |id: &str| Local { id: id.into(), project: "/p".into(), name: "Local 2".into() };
        let draft = Draft::default();
        assert_eq!(draft.retry_local([local("l1")].iter()), None);
        let draft = Draft { local: Some("l1".into()), ..draft };
        assert_eq!(draft.retry_local([local("l0"), local("l1")].iter()), Some("l1".to_string()));
        assert_eq!(draft.retry_local([local("l0")].iter()), None);
    }

    #[test]
    fn a_local_session_sends_only_its_local_whatever_the_worktree_choices_were() {
        let draft = Draft { picked_checkout: Target::NewLocal, source: Source::Branch("fix/login".into()), repo: Some("/p".into()), branches: vec![("main".into(), None)], ..Draft::default() };
        assert!(draft.ready(""));
        assert_eq!(draft.spec(&claude(), "/p", Some("local-1"), "", "")["checkout"], json!({"local": "local-1"}));
    }

    #[test]
    fn a_new_worktree_needs_a_repository_its_branches_and_a_usable_name() {
        let taken: HashSet<String> = ["main".to_string()].into();
        let draft = Draft { picked_checkout: Target::Worktree, repo: Some("/src/app".into()), branches: vec![("main".into(), None)], taken, ..Draft::default() };
        assert!(draft.ready("fix-ci"));
        assert!(!draft.ready("main"));
        assert!(!draft.ready("fix ci"));
        assert!(!Draft { repo: None, ..draft }.ready("fix-ci"));
        let draft = Draft { picked_checkout: Target::Worktree, repo: Some("/src/app".into()), ..Draft::default() };
        assert!(!draft.ready("fix-ci"));
    }

    #[test]
    fn the_default_base_leads_the_recent_branches() {
        let mut branches: Vec<(String, Option<i64>)> = ["feat", "fix", "main", "old"].map(|b| (b.to_string(), None)).to_vec();
        default_first(&mut branches, "", "fix");
        assert_eq!(branches.iter().map(|(b, _)| b.as_str()).collect::<Vec<_>>(), ["main", "feat", "fix", "old"]);
        let mut none: Vec<(String, Option<i64>)> = Vec::new();
        default_first(&mut none, "main", "main");
        assert_eq!(none, []);
    }

    #[test]
    fn an_empty_name_falls_back_to_a_free_one() {
        let fresh = |seed| Draft { seed, ..Draft::default() }.auto_name();
        let taken: HashSet<String> = [fresh(0).to_uppercase(), format!("{}/x", fresh(1)), "release/1.0".into()].into();
        let draft = Draft { picked_checkout: Target::Worktree, taken, ..Draft::default() };
        assert_eq!((draft.auto_name(), draft.name()), (fresh(2), fresh(2)));
        let words = petname::Petnames::medium();
        assert_eq!(fresh(words.adjectives.len() * words.nouns.len()), fresh(0));
    }

    #[test]
    fn a_closed_worktrees_pushed_branch_keeps_its_name_taken() {
        let taken = taken_names(&[("main".into(), None)], &["calm-lynx".into()], ["brave-otter".to_string()].into_iter());
        assert_eq!(name_problem("calm-lynx", &taken), Some("A worktree or branch with this name already exists"));
        assert!(is_taken("brave-otter", &taken));
        let first = Draft::default().auto_name();
        let pushed = taken_names(&[], std::slice::from_ref(&first), std::iter::empty());
        assert_ne!(Draft { taken: pushed, ..Draft::default() }.auto_name(), first);
    }

    #[test]
    fn a_name_must_be_free_and_valid_for_git() {
        let taken: HashSet<String> = ["main", "Foo", "fix/login"].map(String::from).into();
        assert_eq!(name_problem("fix-login_2.0", &taken), None);
        for used in ["main", "foo", "fix"] {
            assert_eq!(name_problem(used, &taken), Some("A worktree or branch with this name already exists"), "{used}");
        }
        for bad in ["", "fix login", "fix/login", "-x", ".x", "x.", "a..b", "x.lock", "tên", "HEAD"] {
            assert_eq!(name_problem(bad, &taken), Some("Use letters, digits, - _ or ."), "{bad}");
        }
    }

    #[test]
    fn a_spec_leaves_access_to_the_app_without_planning_and_carries_no_argv() {
        let want = json!({"project": "/p", "checkout": {"local": "local-1"}, "provider": "claude", "plan": false, "prompt": "Fix CI"});
        assert_eq!(Draft::default().spec(&claude(), "/p", Some("local-1"), "", "  Fix CI  "), want);
        let new = Draft { picked_checkout: Target::Worktree, branches: vec![("main".into(), None)], ..Draft::default() };
        let want = json!({"project": "/p", "checkout": {"new": {"name": "fix-ci", "base": "main"}}, "provider": "claude", "plan": false});
        assert_eq!(new.spec(&claude(), "/p", None, "fix-ci", " \n "), want);
    }

    #[test]
    fn an_existing_branch_needs_only_a_repository_and_a_branch() {
        let draft = Draft { picked_checkout: Target::Worktree, source: Source::Branch("fix/login".into()), repo: Some("/src/app".into()), taken: ["main".to_string()].into(), ..Draft::default() };
        assert!(draft.ready("main"));
        assert!(!draft.ready(""));
        assert!(!Draft { repo: None, ..draft }.ready("main"));
    }

    #[test]
    fn an_existing_branch_is_sent_without_a_name_or_base_and_lands_in_a_dashed_folder() {
        let draft = Draft { picked_checkout: Target::Worktree, source: Source::Branch("fix/login".into()), branches: vec![("main".into(), None)], ..Draft::default() };
        let want = json!({"project": "/p", "checkout": {"new": {"branch": "fix/login"}}, "provider": "claude", "plan": false});
        assert_eq!(draft.spec(&claude(), "/p", None, "fix/login", ""), want);
        assert_eq!(draft.spec(&claude(), "/p", None, "origin/fix/login", ""), want);
        assert_eq!(draft.folder("fix/login"), "fix-login");
    }

    #[test]
    fn a_failed_open_reopens_on_the_branch_it_asked_for() {
        assert_eq!(Source::of(&json!({"branch": "fix/login", "copy": true})), Source::Branch("fix/login".into()));
        assert_eq!(Source::of(&json!({"name": "calm-otter", "base": "main"})), Source::New);
    }

    #[test]
    fn a_pr_is_a_number_a_hash_number_or_a_pull_url() {
        for ok in ["123", " #123 ", "https://github.com/acme/app/pull/123", "https://github.com/acme/app/pull/123/files", "https://github.com/acme/app/pull/123?w=1", "https://github.com/acme/app/pull/123#issuecomment-1"] {
            assert_eq!(parse_pr(ok), Some(123), "{ok}");
        }
        for bad in ["", "#", "+5", "0", "abc", "https://github.com/acme/app/issues/123"] {
            assert_eq!(parse_pr(bad), None, "{bad}");
        }
    }

    #[test]
    fn a_pr_is_sent_as_typed_and_reopens_as_a_pr() {
        let draft = Draft { picked_checkout: Target::Worktree, source: Source::Pr("#7".into()), repo: Some("/p".into()), ..Draft::default() };
        assert!(draft.ready("#7") && !draft.ready("seven"));
        let want = json!({"project": "/p", "checkout": {"new": {"pr": "#7"}}, "provider": "claude", "plan": false});
        assert_eq!(draft.spec(&claude(), "/p", None, "#7", ""), want);
        assert_eq!(draft.folder("#7"), "pr-7");
        assert_eq!(Source::of(&want["checkout"]["new"]), Source::Pr("#7".into()));
    }

    #[test]
    fn linking_a_pr_forces_a_worktree_until_it_is_unlinked_and_names_it_only_when_it_parses() {
        let mut draft = Draft::default();
        draft.link_pr("seven");
        assert_eq!((draft.linked_pr(), draft.in_worktree()), (None, false));
        draft.link_pr(" #7 ");
        assert_eq!((draft.linked_pr(), draft.in_worktree(), draft.name()), (Some(7), true, "#7".to_string()));
        draft.unlink_pr();
        assert_eq!((draft.linked_pr(), draft.in_worktree(), draft.name()), (None, false, draft.auto_name()));
    }

    #[test]
    fn picking_a_base_starts_a_new_branch_and_opening_one_names_it() {
        let mut draft = Draft { branches: vec![("main".into(), None), ("dev".into(), None)], ..Draft::default() };
        draft.open_branch("fix/login".into());
        assert_eq!(draft.name(), "fix/login");
        draft.pick_base(1);
        assert_eq!((draft.source.clone(), draft.base_branch()), (Source::New, "dev".to_string()));
    }

    #[test]
    fn the_branch_menu_lists_recent_branches_until_a_search_reaches_all_and_origins() {
        let local: Vec<(String, Option<i64>)> = (0..25).map(|i| (format!("b{i}"), None)).chain([("fix/login".to_string(), None)]).collect();
        let remote = vec!["fix/logout".to_string()];
        assert_eq!(listed_branches(&local, &remote, " ").len(), 20);
        assert_eq!(listed_branches(&local, &remote, "FIX/LOG"), [(Some(25), "fix/login"), (None, "fix/logout")]);
    }

    #[test]
    fn a_create_error_keeps_the_draft() {
        let mut draft = Draft { repo: Some("/p".into()), reply: sent("r1"), ..Draft::default() };
        assert!(!draft.ready(""));
        assert!(!draft.reply.answer("r0", None));
        assert!(draft.reply.answer("r1", Some(("Setup exited 1".into(), "npm ERR!".into()))));
        assert_eq!((draft.reply.waiting(), draft.ready("")), (false, true));
        assert_eq!(draft.reply.error, Some(("Setup exited 1".to_string(), "npm ERR!".to_string())));
    }

    #[test]
    fn a_create_error_after_the_page_left_goes_to_the_window() {
        let mut draft = Draft { reply: sent("r1"), ..Draft::default() };
        assert_eq!(draft.failed("r1", "Setup exited 1".into(), "npm ERR!".into(), false), Some("Setup exited 1".to_string()));
        assert_eq!(draft.reply.error, None);
        assert_eq!(draft.failed("r1", "Setup exited 1".into(), "npm ERR!".into(), true), None);
        assert_eq!(draft.reply.error, Some(("Setup exited 1".to_string(), "npm ERR!".to_string())));
    }

    #[test]
    fn pocketd_names_only_a_new_branch_that_has_a_prompt() {
        let new = Draft { picked_checkout: Target::Worktree, source: Source::New, ..Draft::default() };
        assert!(new.auto_names("fix the login", true));
        assert!(!new.auto_names("  ", true));
        assert!(!new.auto_names("fix the login", false));
        assert!(!Draft { picked_checkout: Target::Worktree, source: Source::Branch("fix".into()), ..Draft::default() }.auto_names("fix", true));
        assert!(!Draft::default().auto_names("fix the login", true));
    }
}
