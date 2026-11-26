pub(crate) mod machine;
mod page;
pub(crate) mod picker;
pub(crate) mod side;

use crate::chat::composer::suggest::{Prompt, Suggest};
use crate::chat::composer::{MAX_ROWS, MIN_ROWS, PLACEHOLDER};
use crate::chat::mention;
use crate::composer::{Composer, ComposerEvent};
use crate::creating::Create;
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::field::Field;
use crate::fs::fs_for;
use crate::modals::form::default_base;
use crate::modals::host_finder::Opener;
use crate::spaces::{self, Reach, Space, in_space};
use crate::terminals::link;
use crate::util::{LOCAL_ICON, WORKTREE_ICON};
use agents::locals::Local;
use agents::{Agents, CreateReply, Event};
use git::github;
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::*;
use machine::{Blocker, Landing, blocker, landing};
use picker::Picker;
use serde_json::{Value, json};
use side::{Project, Side};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::time::Instant;
use store::LaunchPick;

pub struct NewForm {
    /// The new-session page fills the main area.
    shown: bool,
    pub(crate) composer: Entity<Composer>,
    pub(crate) prompt: Entity<TextareaState>,
    pub(crate) suggest: Suggest,
    search: Entity<InputState>,
    pr: Entity<InputState>,
    draft: Draft,
    field: Field,
    side: Option<Side>,
    _subs: Vec<Subscription>,
}

pub(crate) enum NewFormEvent {
    Start,
    Typed,
    Suggest(usize),
    Project(String),
    OpenFolder,
    CloneProject,
    AddHost,
    Machine(Space),
    Reconnect(String),
}

impl EventEmitter<NewFormEvent> for NewForm {}

pub(crate) const NO_PROJECT: &str = "No project";

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
    /// The session starts in a Scratch folder: `repo`, or with none, a new one made when it's sent.
    scratch: bool,
    /// The new Scratch folder is being made, so a second send would make another.
    making_scratch: bool,
    /// The attachments are being copied to the project's host, so a second send would copy them again.
    copying: bool,
    /// The project whose host holds copies of the attachments, and each file's copy; kept for a resend of this draft only.
    copied: Option<(String, Vec<(PathBuf, String)>)>,
    branches: Vec<(String, Option<i64>)>,
    /// Branches on origin with no local twin.
    remote_branches: Vec<String>,
    source: Source,
    base: usize,
    /// The base to pick once the branches load, instead of the repo's.
    want_base: Option<String>,
    reply: CreateReply,
    picker: Option<Picker>,
    /// Each project's branches as last read, shown at once when it's picked again while a fresh read runs.
    read: HashMap<String, RepoBranches>,
}

/// A project's branches as read from git.
#[derive(Clone)]
struct RepoBranches {
    current: String,
    branches: Vec<(String, Option<i64>)>,
    remote: Vec<String>,
    taken: HashSet<String>,
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
            scratch: false,
            making_scratch: false,
            copying: false,
            copied: None,
            branches: Vec::new(),
            remote_branches: Vec::new(),
            source: Source::New,
            base: 0,
            want_base: None,
            reply: CreateReply::default(),
            picker: None,
            read: HashMap::new(),
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
        self.picked_checkout == Target::Worktree || !self.locals_here() || matches!(self.source, Source::Pr(_))
    }

    /// Only this Mac's pocketd keeps Locals, so a project on a host has none.
    fn locals_here(&self) -> bool {
        self.locals_offered && self.repo.as_deref().is_none_or(|r| hosts::split(r).0.is_none())
    }

    fn link_pr(&mut self, typed: &str) {
        if parse_pr(typed).is_some() {
            (self.source, self.picker) = (Source::Pr(typed.trim().to_string()), None);
        }
    }

    fn unlink_pr(&mut self) {
        self.source = Source::New;
    }

    /// A new branch from the `i`th local branch, kept when the branches are read again.
    fn pick_base(&mut self, i: usize) {
        (self.base, self.source, self.picker) = (i, Source::New, None);
        self.want_base = self.branches.get(i).map(|(b, _)| b.clone());
    }

    fn target(&self) -> Target {
        if self.in_worktree() { Target::Worktree } else { self.picked_checkout }
    }

    fn targets(&self) -> Vec<Target> {
        [Some(Target::Worktree), self.locals_here().then_some(Target::NewLocal)].into_iter().flatten().collect()
    }

    fn pick_checkout(&mut self, target: Target) {
        (self.picked_checkout, self.picker) = (target, None);
    }

    fn open_branch(&mut self, branch: String) {
        (self.source, self.picker) = (Source::Branch(branch), None);
    }

    /// Whether a branch menu row is what the session will start from.
    fn is_picked(&self, local_ix: Option<usize>, name: &str) -> bool {
        match &self.source {
            Source::New => local_ix == Some(self.base),
            Source::Branch(branch) => branch == name,
            Source::Pr(_) => false,
        }
    }

    fn no_project(&self) -> bool {
        self.scratch && self.repo.is_none()
    }

    fn pick_no_project(&mut self) {
        self.pick_repo(None, true);
        self.picker = None;
    }

    fn ready(&self, name: &str) -> bool {
        if self.scratch {
            return !self.making_scratch && !self.copying && !self.reply.waiting();
        }
        let place = match (self.in_worktree(), &self.source) {
            (false, _) => self.repo.is_some(),
            (true, Source::New) => self.repo.is_some() && !self.branches.is_empty() && name_problem(name, &self.taken).is_none(),
            (true, Source::Branch(_)) => self.repo.is_some() && !name.is_empty(),
            (true, Source::Pr(_)) => self.repo.is_some() && parse_pr(name).is_some(),
        };
        place && !self.copying && !self.reply.waiting()
    }

    /// The copies made on `project`'s host, which its prompt names instead of the files here.
    fn copied_for(&self, project: &str) -> &[(PathBuf, String)] {
        self.copied.as_ref().filter(|(p, _)| p == project).map_or(&[], |(_, files)| files)
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

    /// Reopening the page clears its error row, so a failure that arrives while it is gone goes to the window's error line; either way it frees the page for the next start.
    fn failed(&mut self, request: &str, message: String, detail: String, open: bool) -> Option<String> {
        let shown = open.then(|| (message.clone(), detail));
        if self.reply.answer(request, shown) && open {
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
    fn pick_repo(&mut self, repo: Option<String>, scratch: bool) {
        self.scratch = scratch;
        if self.repo != repo {
            (self.source, self.want_base, self.local) = (Source::New, None, None);
            self.taken.clear();
        }
        self.repo = repo;
        self.branches.clear();
        self.remote_branches.clear();
        self.base = 0;
        if let Some(read) = self.repo.as_ref().and_then(|r| self.read.get(r)).cloned() {
            self.show(read);
        }
    }

    /// A fresh read is kept for `repo` and shown if it's still picked.
    fn branches_read(&mut self, repo: String, read: RepoBranches) {
        if self.repo.as_ref() == Some(&repo) {
            self.show(read.clone());
        }
        self.read.insert(repo, read);
    }

    fn show(&mut self, read: RepoBranches) {
        let RepoBranches { current, mut branches, remote, taken } = read;
        default_first(&mut branches, self.want_base.as_deref().unwrap_or_default(), &current);
        (self.base, self.branches, self.remote_branches, self.taken) = (0, branches, remote, taken);
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

/// The branch menu's rows: the recent local branches, or every branch holding `search`: the one it names, then local before origin, those starting with it first. Each carries its index among the local branches, or none when only origin has it.
pub(crate) fn listed_branches<'a>(branches: &'a [(String, Option<i64>)], remote: &'a [String], search: &str) -> Vec<(Option<usize>, &'a str)> {
    let search = search.trim().to_lowercase();
    let local = branches.iter().enumerate().map(|(i, (b, _))| (Some(i), b.as_str()));
    if search.is_empty() {
        return local.take(RECENT).collect();
    }
    let mut found: Vec<_> = local.chain(remote.iter().map(|b| (None, b.as_str()))).filter(|(_, b)| b.to_lowercase().contains(&search)).collect();
    found.sort_by_cached_key(|(i, b)| {
        let b = b.to_lowercase();
        (b != search, i.is_none(), !b.starts_with(&search))
    });
    found.truncate(FOUND);
    found
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
    (0..count)
        .map(|i| (seed + i) % count)
        .map(|n| format!("{}-{}", adjectives[n / nouns.len()], nouns[n % nouns.len()]))
        .find(|n| !is_taken(n, taken))
        .unwrap_or_else(|| unique("worktree", taken))
}

/// `base`, or `base-2`, `base-3`… when taken.
fn unique(base: &str, taken: &HashSet<String>) -> String {
    std::iter::once(base.to_string()).chain((2..).map(|n| format!("{base}-{n}"))).find(|n| !is_taken(n, taken)).expect("numbered names never run out")
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

/// pocketd names a Scratch folder from its first prompt until it has a name.
fn names_folder(offered: bool, given: Option<&str>, prompt: &str) -> bool {
    offered && given.is_none() && !prompt.is_empty()
}

pub(crate) fn default_first(branches: &mut [(String, Option<i64>)], preferred: &str, current: &str) {
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

    pub(crate) fn picker(&self) -> Option<Picker> {
        self.draft.picker
    }

    pub(crate) fn shut_picker(&mut self, cx: &mut Context<Self>) {
        self.draft.picker = None;
        cx.notify();
    }

    fn edit(&mut self, cx: &mut Context<Self>, change: impl FnOnce(&mut Draft)) {
        change(&mut self.draft);
        cx.notify();
    }

    pub(crate) fn shown(&self) -> bool {
        self.shown
    }

    pub(crate) fn set_shown(&mut self, shown: bool, cx: &mut Context<Self>) {
        if self.shown != shown {
            self.shown = shown;
            cx.notify();
        }
    }

    pub(crate) fn ready(&self) -> bool {
        self.draft.ready(&self.draft.name())
    }

    pub(crate) fn set(&mut self, side: Side) {
        self.side = Some(side);
    }

    /// The menu held the focus; Enter in the prompt is what starts the session.
    pub(crate) fn focus_prompt(&self, window: &mut Window, cx: &mut Context<Self>) {
        self.prompt.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub(crate) fn clear(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.composer.update(cx, |c, _| c.draft.clear());
        self.draft.copied = None;
        self.prompt.update(cx, |s, cx| s.set_value("", window, cx));
    }

    pub(crate) fn reopen(
        &mut self,
        prompt: Option<String>,
        worktree: bool,
        locals_offered: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> (bool, bool, Option<String>) {
        let typed = self.prompt.read(cx).value().to_string();
        let keep = !typed.trim().is_empty() || self.composer.read(cx).draft.has_attachments();
        let keep_project = keep && prompt.is_none();
        let text = appended(&typed, prompt);
        self.draft.seed = crate::util::now_ms() as usize;
        self.field.hide();
        self.prompt.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
        if !keep {
            self.composer.update(cx, |c, _| c.draft.clear());
            self.draft.copied = None;
        }
        self.draft.reopen(keep, worktree, locals_offered);
        self.composer.update(cx, |c, cx| c.shut_menu(cx));
        (keep_project, self.draft.no_project(), self.draft.repo.clone())
    }

    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let composer = cx.new(|cx| {
            let dropped = Composer::dropped(cx.weak_entity());
            let prompt = cx.new(|cx| {
                TextareaState::new(window, cx)
                    .placeholder(PLACEHOLDER)
                    .auto_grow(MIN_ROWS, MAX_ROWS)
                    .submit_on_enter(true)
                    .on_drop_paths(dropped)
                    .decorate(mention::decorations)
            });
            let models = cx.new(|cx| InputState::new(window, cx).placeholder("Search models"));
            Composer::new(prompt, models)
        });
        let (prompt, models) = {
            let c = composer.read(cx);
            (c.prompt.clone(), c.search.clone())
        };
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search branches…"));
        let pr = cx.new(|cx| InputState::new(window, cx).placeholder("123, #123 or a PR URL"));
        let _subs = vec![
            cx.subscribe(&composer, |this, _, ev: &ComposerEvent, cx| match ev {
                ComposerEvent::Opened => {
                    this.draft.picker = None;
                    cx.notify();
                }
                ComposerEvent::Unsaved(title, error) => {
                    this.draft.reply.error = Some((title.to_string(), error.clone()));
                    cx.notify();
                }
                _ => {}
            }),
            cx.observe(&composer, |_, _, cx| cx.notify()),
            cx.subscribe(&prompt, |_, _, ev: &InputEvent, cx| match ev {
                InputEvent::PressEnter { shift: false, .. } => cx.emit(NewFormEvent::Start),
                InputEvent::Change => cx.emit(NewFormEvent::Typed),
                _ => {}
            }),
            cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&models, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe(&pr, |this, pr, ev: &InputEvent, cx| {
                if let InputEvent::PressEnter { secondary, .. } = ev {
                    let typed = pr.read(cx).value().to_string();
                    this.draft.link_pr(&typed);
                    if *secondary {
                        cx.emit(NewFormEvent::Start);
                    }
                }
                cx.notify()
            }),
        ];
        Self { shown: false, composer, prompt, suggest: Suggest::default(), search, pr, draft: Draft::default(), field: Field::default(), side: None, _subs }
    }
}

impl Desktop {
    pub(crate) fn watch_new_form(form: &Entity<NewForm>, window: &mut Window, cx: &mut Context<Self>) -> Vec<Subscription> {
        let composer = form.read(cx).composer.clone();
        vec![
            cx.subscribe_in(form, window, Self::on_new_form_event),
            cx.observe(form, |_, _, cx| cx.notify()),
            cx.subscribe_in(&composer, window, |this, c, ev: &ComposerEvent, window, cx| this.composer_event(c, ev, window, cx)),
            cx.observe(&composer, |_, _, cx| cx.notify()),
        ]
    }

    fn on_new_form_event(&mut self, _: &Entity<NewForm>, ev: &NewFormEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            NewFormEvent::Start => self.start_session(window, cx),
            NewFormEvent::Typed => {
                self.pill_typed_command(Prompt::Launch, window, cx);
                self.draft_changed(Prompt::Launch, cx);
            }
            NewFormEvent::Suggest(i) => _ = self.suggest_pick(Prompt::Launch, *i, window, cx),
            NewFormEvent::Project(key) => {
                self.pick_repo(key.clone(), window, cx);
                cx.notify();
            }
            NewFormEvent::OpenFolder => self.open_folder(Opener::NewSession, window, cx),
            NewFormEvent::CloneProject => self.clone_project(window, cx),
            NewFormEvent::AddHost => self.add_host_from_sidebar(window, cx),
            NewFormEvent::Machine(space) => self.pick_machine(space.clone(), window, cx),
            NewFormEvent::Reconnect(id) => {
                self.retry_host(id);
                cx.notify();
            }
        }
    }

    /// What `project` last started, else the default agent, with an agent this app knows and the project shows.
    pub(crate) fn last_pick(&self, project: Option<&str>) -> LaunchPick {
        self.store.launch_pick(project, &self.agents)
    }

    /// A draft not yet sent keeps its words, images, checkout and project; `prompt` adds to its words,
    /// and since it quotes the Project on screen, the draft moves there.
    pub fn reset_new_form(&mut self, prompt: Option<String>, worktree: bool, window: &mut Window, cx: &mut Context<Self>) {
        let worktree = worktree || self.store.agents.new_worktree;
        let locals = self.agents.locals_offered();
        let (keep_project, none, repo) = self.new_form.update(cx, |f, cx| f.reopen(prompt, worktree, locals, window, cx));
        if keep_project && none {
            return;
        }
        let repo = if keep_project { repo } else { None };
        match repo.or_else(|| self.project.clone()) {
            Some(repo) => self.pick_repo(repo, window, cx),
            None => self.new_form.update(cx, |f, cx| f.edit(cx, |d| d.pick_no_project())),
        }
        if !keep_project {
            self.land_draft(window, cx);
        }
    }

    pub(super) fn pick_repo(&mut self, repo: String, window: &mut Window, cx: &mut Context<Self>) {
        let folders = self.store.worktrees_dir(&repo, &self.host_agents.shared_of(&repo));
        let scratch = self.store.is_scratch(&repo);
        self.new_form.update(cx, |f, cx| f.edit(cx, |d| d.pick_repo(Some(repo.clone()), scratch)));
        let dir = repo.clone();
        let (runner, fs) = (hosts::runner_for(&dir), fs_for(&dir));
        let task = cx.background_executor().spawn(async move {
            let current = git::read(&runner, &dir, true).map(|r| r.branch).unwrap_or_default();
            let branches = git::dated_branches(&runner, &dir);
            let entries = fs.local().and_then(|()| std::fs::read_dir(&folders)).into_iter().flatten().flatten();
            let remote = git::remote_branches(&runner, &dir);
            let taken = taken_names(&branches, &remote, entries.filter_map(|e| e.file_name().into_string().ok()));
            RepoBranches { current, branches, remote, taken }
        });
        cx.spawn_in(window, async move |this, cx| {
            let read = task.await;
            this.update(cx, |d, cx| {
                d.new_form.update(cx, |f, cx| {
                    f.draft.branches_read(repo, read);
                    cx.notify();
                });
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    /// The projects a session can start in: those with a git checkout. A Scratch folder counts only while it's the one picked.
    pub(crate) fn form_projects(&self, cx: &App) -> Vec<String> {
        let repo = self.new_form.read(cx).draft.repo.as_deref();
        self.store
            .projects
            .iter()
            .filter(|p| (repo == Some(p.as_str()) || !self.store.is_scratch(p)) && self.listed_trees(p).is_some_and(|t| !t.is_empty()))
            .cloned()
            .collect()
    }

    /// Moves the draft into the space on screen: its project's twin there, else the space's first.
    pub(crate) fn land_draft(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let to = self.space();
        let pool = if self.space_reach(&to) == Reach::Online { self.form_projects(cx) } else { self.space_projects(&to) };
        match landing(self.new_form.read(cx).draft.repo.as_deref(), &to, &pool) {
            Landing::Project(p) => self.pick_repo(p, window, cx),
            Landing::NoProject => self.new_form.update(cx, |f, cx| f.edit(cx, |d| d.pick_no_project())),
            Landing::Stay => {}
        }
    }

    /// The project the new-session page has drafted, while it shows.
    pub(crate) fn drafted_project(&self, cx: &App) -> Option<String> {
        let f = self.new_form.read(cx);
        f.draft.repo.clone().filter(|_| f.shown)
    }

    /// What stops the draft starting in the space on screen.
    fn draft_blocker(&self, cx: &App) -> Option<Blocker> {
        let space = self.space();
        blocker(self.new_form.read(cx).draft.repo.as_deref(), &space, self.space_reach(&space))
    }

    /// The machine pill's pick: the machine is the space, so this switches it.
    pub(crate) fn pick_machine(&mut self, space: Space, window: &mut Window, cx: &mut Context<Self>) {
        self.new_form.update(cx, |f, cx| f.shut_picker(cx));
        self.switch_space_with_draft(space, window, cx);
        self.focus_prompt(window, cx);
    }

    /// Takes `key`, a folder just chosen as a project on This Mac or a host, as the project the page starts in.
    pub(crate) fn pick_new_project(&mut self, key: String, window: &mut Window, cx: &mut Context<Self>) {
        self.save_folder(&key, cx);
        self.switch_space_with_draft(spaces::space_of(&key), window, cx);
        self.pick_repo(key, window, cx);
        self.focus_prompt(window, cx);
    }

    /// The agent the page starts, picked here or the Project's last.
    pub(crate) fn new_form_pick(&self, cx: &App) -> LaunchPick {
        let f = self.new_form.read(cx);
        let repo = f.draft.repo.as_deref();
        f.composer.read(cx).draft.pick(repo, || self.last_pick(repo))
    }

    fn session_ready(&self, cx: &App) -> bool {
        self.new_form.read(cx).ready() && self.draft_blocker(cx).is_none()
    }

    pub(crate) fn focus_prompt(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_form.update(cx, |f, cx| f.focus_prompt(window, cx));
        cx.notify();
    }

    /// A new worktree's page leaves at once for its progress; a session's waits for pocketd.
    pub(crate) fn start_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session_ready(cx) {
            return;
        }
        if self.terminals.read(cx).link.stale().is_some() {
            self.fail(link::UPDATING, cx);
            return cx.notify();
        }
        if self.new_form.read(cx).draft.scratch {
            return self.start_in_scratch(window, cx);
        }
        let Some(project) = self.new_form.read(cx).draft.repo.clone() else { return };
        if self.copy_attachments_first(&project, window, cx) {
            return cx.notify();
        }
        let (name, prompt, target) = {
            let f = self.new_form.read(cx);
            (f.draft.name(), f.prompt.read(cx).value().to_string(), f.draft.target())
        };
        let local = match target {
            Target::NewLocal => {
                let retry = self.new_form.read(cx).draft.retry_local(self.agents.locals_in(&project));
                let id = retry.unwrap_or_else(|| self.new_local(&project, cx));
                self.new_form.update(cx, |f, cx| f.edit(cx, |d| d.local = Some(id.clone())));
                self.store.collapsed.remove(&project);
                self.select_tree(project.clone(), Some(id.clone()), cx);
                Some(id)
            }
            Target::Worktree => None,
        };
        let pick = self.new_form_pick(cx);
        let (mut spec, worktree, folder, auto_name) = {
            let f = self.new_form.read(cx);
            let sent = f.composer.read(cx).draft.prompt_with(&prompt, f.draft.copied_for(&project));
            let d = &f.draft;
            (d.spec(&pick, &project, local.as_deref(), &name, &sent), d.in_worktree(), d.folder(&name), d.auto_names(&prompt, self.agents.names_offered()))
        };
        if auto_name {
            spec["checkout"]["new"]["autoName"] = true.into();
        }
        self.fill_launch(&mut spec, pick, &project, cx);
        if worktree {
            self.store.collapsed.remove(&project);
        }
        self.store.save();
        let Some(request) = self.send_create(spec.clone(), cx) else { return cx.notify() };
        if !worktree {
            self.creates.update(cx, |c, _| c.sent_in_place(request.clone(), spec["prompt"].as_str().unwrap_or_default()));
            self.new_form.update(cx, |f, cx| f.edit(cx, |d| d.reply.sent(request)));
            return cx.notify();
        }
        let path = format!("{}/{folder}", self.store.worktrees_dir(&project, &self.host_agents.shared_of(&project)));
        self.creates.update(cx, |c, _| c.list.push(Create::new(request, path.clone(), spec, Instant::now())));
        self.clear_new_form(window, cx);
        self.leave_new_session(window, cx);
        self.select_tree(project, Some(path), cx);
    }

    /// Copies the draft's attachments to `project`'s host, then starts the session again, so the agent there can read them.
    /// False when nothing needs copying.
    fn copy_attachments_first(&mut self, project: &str, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let copies = crate::composer::host_copies(&self.new_form.read(cx).composer.read(cx).draft, project, &self.store.home().join("attachments"));
        let copies = match copies {
            Ok(Some(copies)) if copies.files != self.new_form.read(cx).draft.copied_for(project) => copies,
            Ok(_) => return false,
            Err(host) => {
                self.fail(format!("{} isn't connected", self.host_name(&host)), cx);
                return true;
            }
        };
        self.new_form.update(cx, |f, cx| f.edit(cx, |d| d.copying = true));
        let (project, host) = (project.to_string(), self.host_name(hosts::split(project).0.unwrap_or_default()));
        let copied = cx.background_executor().spawn(async move { copies.make().map(|()| copies.files) });
        cx.spawn_in(window, async move |this, cx| {
            let copied = copied.await;
            this.update_in(cx, |d, window, cx| {
                d.new_form.update(cx, |f, cx| f.edit(cx, |d| d.copying = false));
                match copied {
                    Ok(files) => {
                        let f = d.new_form.read(cx);
                        let picked = f.draft.repo.as_deref() == Some(project.as_str());
                        let held = files.iter().all(|(from, _)| f.composer.read(cx).draft.holds(from));
                        if !held {
                            return cx.notify();
                        }
                        d.new_form.update(cx, |f, cx| f.edit(cx, |d| d.copied = Some((project, files))));
                        if picked {
                            d.start_session(window, cx);
                        }
                    }
                    Err(e) => {
                        d.new_form.update(cx, |f, cx| f.edit(cx, |d| d.reply.error = Some((format!("Couldn't copy the attachments to {host}"), e.to_string()))))
                    }
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        true
    }

    /// Starts the session in the Scratch folder picked, or in a new one when no project is.
    fn start_in_scratch(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let folder = self.new_form.read(cx).draft.repo.clone();
        if let Some(folder) = folder {
            if self.copy_attachments_first(&folder, window, cx) {
                return cx.notify();
            }
            let pick = self.new_form_pick(cx);
            let sent = {
                let f = self.new_form.read(cx);
                let prompt = f.prompt.read(cx).value().to_string();
                f.composer.read(cx).draft.prompt_with(&prompt, f.draft.copied_for(&folder))
            };
            return self.send_in_scratch(folder, pick, sent, cx);
        }
        self.new_form.update(cx, |f, cx| f.edit(cx, |d| d.making_scratch = true));
        let task = self.make_scratch(cx);
        cx.spawn_in(window, async move |this, cx| {
            let made = task.await;
            this.update_in(cx, |d, window, cx| {
                d.new_form.update(cx, |f, cx| f.edit(cx, |d| d.making_scratch = false));
                match made {
                    Ok(folder) => {
                        let pick = d.new_form_pick(cx);
                        d.adopt_scratch(&folder, cx);
                        // A send pocketd refuses is retried here, not in yet another new folder.
                        d.new_form.update(cx, |f, cx| {
                            f.draft.pick_repo(Some(folder.clone()), true);
                            f.composer.update(cx, |c, _| c.draft.choose(Some(folder), pick));
                        });
                        d.start_session(window, cx);
                    }
                    Err(e) => {
                        d.new_form.update(cx, |f, cx| f.edit(cx, |d| d.reply.error = Some(("Couldn't make a folder for the session".to_string(), e))));
                        cx.notify();
                    }
                }
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Gives `spec` what the settings add to it, and remembers `pick` as `project`'s.
    fn fill_launch(&mut self, spec: &mut serde_json::Value, pick: LaunchPick, project: &str, cx: &mut Context<Self>) {
        let efforts = self.launch_efforts(&pick.provider, &pick.model);
        self.store.agents.launch(spec, &self.agents, &efforts);
        self.store.repos.entry(project.to_string()).or_default().launch = pick;
        // Ahead of the create, so pocketd starts it as the project's settings now say.
        self.publish_registry(cx);
    }

    fn send_in_scratch(&mut self, folder: String, pick: LaunchPick, sent: String, cx: &mut Context<Self>) {
        let mut spec = pick.spec(&folder, json!({"worktree": folder}), sent.trim());
        if names_folder(self.agents.names_offered(), self.agents.given_name(&folder), sent.trim()) {
            spec["checkout"]["autoName"] = true.into();
        }
        self.fill_launch(&mut spec, pick, &folder, cx);
        self.store.save();
        self.select_tree(folder, None, cx);
        let Some(request) = self.send_create(spec.clone(), cx) else { return cx.notify() };
        self.creates.update(cx, |c, _| c.sent_in_place(request.clone(), spec["prompt"].as_str().unwrap_or_default()));
        self.new_form.update(cx, |f, cx| f.edit(cx, |d| d.reply.sent(request)));
        cx.notify();
    }

    /// pocketd's replies to a create. The Terminal opens once pocketd lists it (`Terminals::arrived`).
    pub(crate) fn on_launch(&mut self, ev: Event, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            Event::Creating { request, terminal, cwd, setup } => {
                if self.terminals.read(cx).tab_chats.contains_key(&request) {
                    self.store.chats.insert(terminal.clone());
                    self.save_soon(cx);
                }
                self.terminals.update(cx, |t, _| t.created(&request, terminal.clone(), cwd.clone(), setup));
                self.creates.update(cx, |c, _| c.made_terminal(&request, terminal.clone(), Instant::now()));
                self.daemon.send(json!({"op": "list"}));
                if let Some(chat) = self.creates.update(cx, |c, _| c.get(&request).map(|c| c.chat())) {
                    self.store.track(&cwd);
                    if chat {
                        self.store.chats.insert(terminal.clone());
                    }
                    self.save_soon(cx);
                }
                if let Some(expected) = self.creates.update(cx, |c, _| c.started(&request, cwd.clone(), &mut self.worktrees)) {
                    if self.worktree.as_ref() == Some(&expected) {
                        self.worktree = Some(cwd);
                    }
                    self.refresh_git(cx);
                } else if self.new_form.update(cx, |f, _| f.draft.reply.answer(&request, None)) {
                    if self.store.agents.starts_as_chat(&self.new_form_pick(cx).provider) {
                        self.store.chats.insert(terminal);
                        self.save_soon(cx);
                    }
                    self.clear_new_form(window, cx);
                    if self.new_form.read(cx).shown() {
                        self.leave_new_session(window, cx);
                    }
                }
            }
            Event::Progress { request, step, note } => {
                self.creates.update(cx, |cs, _| {
                    if let Some(c) = cs.get(&request) {
                        c.reach(&step, &note, Instant::now());
                    }
                });
            }
            Event::Created { request, agent } => {
                self.creates.update(cx, |cs, _| {
                    if let Some(c) = cs.get(&request) {
                        c.finish(agent, Instant::now());
                    }
                });
            }
            Event::CreateFailed { request, message, detail, .. } => {
                self.terminals.update(cx, |t, _| {
                    t.tab_chats.remove(&request);
                    t.tab_terms.remove(&request);
                });
                self.creates.update(cx, |c, _| c.failed_in_place(&request));
                let made = self.creates.update(cx, |cs, _| cs.get(&request).map(|c| c.fail(message.clone(), detail.clone(), Instant::now())).is_some());
                if !made && let Some(message) = self.new_form.update(cx, |f, _| f.draft.failed(&request, message, detail, f.shown)) {
                    self.fail(message, cx);
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
        self.close_menus(cx);
        self.screen = Screen::Sessions;
        self.new_form.update(cx, |f, cx| f.set_shown(true, cx));
        self.reset_new_form(prompt, worktree, window, cx);
        cx.notify();
    }

    fn clear_new_form(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_form.update(cx, |f, cx| f.clear(window, cx));
    }

    pub(crate) fn leave_new_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_form.update(cx, |f, cx| f.set_shown(false, cx));
        self.refocus(window, cx);
    }

    /// The page the Sessions + opens: a session with no project, unless one is picked on it.
    pub(crate) fn new_session_without_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.new_session(None, false, window, cx);
        self.new_form.update(cx, |f, cx| f.edit(cx, |d| d.pick_no_project()));
    }

    /// The page a project's + opens: a session in a new Worktree or Local of `project`.
    pub(crate) fn new_place(&mut self, project: String, window: &mut Window, cx: &mut Context<Self>) {
        self.select_tree(project.clone(), None, cx);
        self.new_worktree(&crate::actions::NewWorktree, window, cx);
        if self.new_form.read(cx).draft.repo.as_ref() != Some(&project) {
            self.pick_repo(project, window, cx);
        }
    }

    /// The page again, filled in as it was for a create that failed before git made its worktree.
    pub(crate) fn reopen_new_worktree(&mut self, c: &Create, window: &mut Window, cx: &mut Context<Self>) {
        self.new_session(Some(c.prompt().to_string()), true, window, cx);
        self.new_form.update(cx, |f, _| {
            f.draft.pick_checkout(Target::Worktree);
            f.draft.want_base = Some(c.base().to_string());
            f.draft.source = Source::of(c.asked());
        });
    }

    fn new_session_side(&self, cx: &App) -> Side {
        let space = self.space();
        let f = self.new_form.read(cx);
        let repo = f.draft.repo.clone();
        let project = |p: &str| {
            let name = self.repo_name(p);
            Project { key: p.to_string(), mark: self.project_mark(p, crate::util::initials(&name)), name }
        };
        let machines = if f.picker() == Some(Picker::Machine) { self.space_order().iter().map(|s| self.machine_of(s)).collect() } else { Vec::new() };
        let projects =
            if f.picker() == Some(Picker::Project) { in_space(&self.form_projects(cx), &space).iter().map(|p| project(p)).collect() } else { Vec::new() };
        let pick = self.new_form_pick(cx);
        Side {
            machine: self.machine_of(&space),
            machines,
            host: self.space_host_name(&space),
            blocker: self.draft_blocker(cx),
            project: repo.as_deref().map(project),
            projects,
            opens: self.agents.opens(),
            idle: self.idle_main(cx),
            background: self.store.appearance.background.clone(),
            chips: self.composer_chips(&f.composer, &pick, repo, cx),
            attachments: self.store.home().join("attachments"),
            suggest_key: self.suggest_target(Prompt::Launch, cx).map(|t| t.key),
        }
    }

    pub fn new_session_page(&mut self, cx: &mut Context<Self>) -> Div {
        let side = self.new_session_side(cx);
        let name = side.project.as_ref().map_or_else(|| NO_PROJECT.to_string(), |p| p.name.clone());
        let host = side.host.clone();
        self.new_form.update(cx, |f, _| f.set(side));
        div()
            .flex_1()
            .flex()
            .flex_col()
            .child(self.page_bar(spaces::crumbs(host, [name, "New session".into()]), Vec::new(), div(), cx))
            .child(self.new_form.clone())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        Draft, Local, RepoBranches, Source, Target, appended, checkout_choice, default_first, is_taken, listed_branches, name_problem, names_folder, parse_pr,
        taken_names,
    };
    use agents::CreateReply;
    use serde_json::json;
    use std::collections::HashSet;
    use store::LaunchPick;

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
    fn a_project_on_a_host_starts_in_a_new_worktree_since_its_pocketd_holds_no_locals() {
        let draft = Draft { repo: Some("//devbox/p".into()), picked_checkout: Target::NewLocal, ..Draft::default() };
        assert_eq!(draft.target(), Target::Worktree);
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
        draft.pick_repo(Some("/p".into()), false);
        assert_eq!((draft.source.clone(), draft.want_base.as_deref()), (Source::Branch("fix".into()), Some("dev")));
        draft.pick_repo(Some("/q".into()), false);
        assert_eq!((draft.source, draft.want_base, draft.local), (Source::New, None, None));
    }

    #[test]
    fn no_project_can_start_without_a_branch_and_picking_one_leaves_it() {
        let mut draft = kept();
        draft.pick_no_project();
        assert!(draft.no_project());
        assert_eq!((draft.repo.as_deref(), draft.source.clone(), draft.local.as_deref()), (None, Source::New, None));
        assert!(draft.ready(""));
        draft.pick_repo(Some("/p".into()), false);
        assert!(!draft.no_project());
    }

    #[test]
    fn a_scratch_folder_starts_at_once_but_not_while_a_send_is_out_its_folder_is_being_made_or_attachments_copy() {
        let scratch = Draft { repo: Some("/s".into()), scratch: true, ..Draft::default() };
        assert!(scratch.ready("") && !scratch.no_project());
        assert!(!Draft { reply: sent("r1"), repo: Some("/s".into()), scratch: true, ..Draft::default() }.ready(""));
        assert!(!Draft { making_scratch: true, scratch: true, ..Draft::default() }.ready(""));
        assert!(!Draft { copying: true, repo: Some("//devbox/s".into()), scratch: true, ..Draft::default() }.ready(""));
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
        assert_eq!(Draft { locals_offered: false, ..Draft::default() }.targets(), [Target::Worktree]);
        assert_eq!(Draft { repo: Some("//devbox/p".into()), ..draft }.targets(), [Target::Worktree]);
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
        let draft = Draft {
            picked_checkout: Target::NewLocal,
            source: Source::Branch("fix/login".into()),
            repo: Some("/p".into()),
            branches: vec![("main".into(), None)],
            ..Draft::default()
        };
        assert!(draft.ready(""));
        assert_eq!(draft.spec(&claude(), "/p", Some("local-1"), "", "")["checkout"], json!({"local": "local-1"}));
    }

    #[test]
    fn a_new_worktree_needs_a_repository_its_branches_and_a_usable_name() {
        let taken: HashSet<String> = ["main".to_string()].into();
        let draft =
            Draft { picked_checkout: Target::Worktree, repo: Some("/src/app".into()), branches: vec![("main".into(), None)], taken, ..Draft::default() };
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
        let draft = Draft {
            picked_checkout: Target::Worktree,
            source: Source::Branch("fix/login".into()),
            repo: Some("/src/app".into()),
            taken: ["main".to_string()].into(),
            ..Draft::default()
        };
        assert!(draft.ready("main"));
        assert!(!draft.ready(""));
        assert!(!Draft { repo: None, ..draft }.ready("main"));
    }

    #[test]
    fn an_existing_branch_is_sent_without_a_name_or_base_and_lands_in_a_dashed_folder() {
        let draft =
            Draft { picked_checkout: Target::Worktree, source: Source::Branch("fix/login".into()), branches: vec![("main".into(), None)], ..Draft::default() };
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
        for ok in [
            "123",
            " #123 ",
            "https://github.com/acme/app/pull/123",
            "https://github.com/acme/app/pull/123/files",
            "https://github.com/acme/app/pull/123?w=1",
            "https://github.com/acme/app/pull/123#issuecomment-1",
        ] {
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

    fn read(branches: &[&str], current: &str) -> RepoBranches {
        RepoBranches {
            current: current.into(),
            branches: branches.iter().map(|b| (b.to_string(), None)).collect(),
            remote: vec!["origin-only".into()],
            taken: HashSet::from(["taken".to_string()]),
        }
    }

    fn listed(draft: &Draft) -> Vec<&str> {
        draft.branches.iter().map(|(b, _)| b.as_str()).collect()
    }

    #[test]
    fn a_project_picked_again_shows_its_branches_as_last_read() {
        let mut draft = Draft::default();
        draft.pick_repo(Some("/a".into()), false);
        draft.branches_read("/a".into(), read(&["dev", "main"], "dev"));
        draft.pick_repo(Some("/b".into()), false);
        assert!(draft.branches.is_empty());
        draft.pick_repo(Some("/a".into()), false);
        assert_eq!(
            (listed(&draft), draft.base_branch(), draft.remote_branches.clone()),
            (vec!["main", "dev"], "main".to_string(), vec!["origin-only".to_string()])
        );
        assert!(draft.taken.contains("taken"));
    }

    #[test]
    fn a_read_for_a_project_left_is_kept_but_not_shown() {
        let mut draft = Draft::default();
        draft.pick_repo(Some("/b".into()), false);
        draft.branches_read("/a".into(), read(&["main"], "main"));
        assert!(draft.branches.is_empty());
        draft.pick_repo(Some("/a".into()), false);
        assert_eq!(listed(&draft), ["main"]);
    }

    #[test]
    fn a_fresh_read_keeps_the_base_picked_from_the_last_one() {
        let mut draft = Draft::default();
        draft.branches_read("/a".into(), read(&["main", "dev"], "main"));
        draft.pick_repo(Some("/a".into()), false);
        draft.pick_base(1);
        draft.branches_read("/a".into(), read(&["new", "main", "dev"], "main"));
        assert_eq!(draft.base_branch(), "dev");
    }

    #[test]
    fn a_retried_create_picks_its_base_over_the_one_last_shown() {
        let mut draft = Draft::default();
        draft.branches_read("/a".into(), read(&["main", "dev"], "main"));
        draft.pick_repo(Some("/a".into()), false);
        draft.want_base = Some("dev".into());
        draft.branches_read("/a".into(), read(&["main", "dev"], "main"));
        assert_eq!(draft.base_branch(), "dev");
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
    fn a_search_leads_with_the_exact_branch_then_those_starting_with_it() {
        let local: Vec<(String, Option<i64>)> = ["fix/develop-send", "Develop-tools", "fix/old", "develop"].map(|b| (b.to_string(), None)).to_vec();
        let remote = vec!["ci/develop".to_string(), "develop-next".to_string()];
        assert_eq!(
            listed_branches(&local, &remote, "develop"),
            [(Some(3), "develop"), (Some(1), "Develop-tools"), (Some(0), "fix/develop-send"), (None, "develop-next"), (None, "ci/develop")]
        );
    }

    #[test]
    fn a_branch_only_origin_has_leads_when_the_search_names_it_exactly() {
        let local: Vec<(String, Option<i64>)> = (0..60).map(|i| (format!("fix/develop-{i}"), None)).collect();
        let remote = vec!["develop".to_string()];
        let found = listed_branches(&local, &remote, "develop");
        assert_eq!((found[0], found.len()), ((None, "develop"), 50));
    }

    #[test]
    fn the_branch_menu_marks_the_base_or_the_branch_to_open() {
        let mut draft = Draft { branches: vec![("main".into(), None), ("dev".into(), None)], base: 1, ..Draft::default() };
        assert!(draft.is_picked(Some(1), "dev") && !draft.is_picked(Some(0), "main"));
        draft.open_branch("fix/login".into());
        assert!(draft.is_picked(None, "fix/login") && !draft.is_picked(Some(1), "dev"));
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
    fn a_create_error_after_the_page_left_goes_to_the_window_and_frees_the_page() {
        let mut draft = Draft { repo: Some("/p".into()), reply: sent("r1"), ..Draft::default() };
        assert_eq!(draft.failed("r1", "Setup exited 1".into(), "npm ERR!".into(), false), Some("Setup exited 1".to_string()));
        assert_eq!((draft.reply.error.clone(), draft.reply.waiting()), (None, false));
        assert!(draft.ready(""));
    }

    #[test]
    fn a_create_error_on_the_open_page_shows_there() {
        let mut draft = Draft { reply: sent("r1"), ..Draft::default() };
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

    #[test]
    fn pocketd_names_only_an_unnamed_scratch_folder_from_a_prompt() {
        assert!(names_folder(true, None, "check the disk"));
        assert!(!names_folder(true, Some("Checking disk space"), "check the disk"));
        assert!(!names_folder(true, None, ""));
        assert!(!names_folder(false, None, "check the disk"));
    }
}
