pub(crate) mod launch;
pub(crate) mod picker;

use agents::{CreateReply, Event};
use crate::creating::Create;
use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::modals::form::{default_base, home};
use crate::terminals::link;
use git::github;
use gpui_kit::component::input::{InputEvent, InputState, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use picker::Picker;
use serde_json::{Value, json};
use store::LaunchPick;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
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
    prompt: Entity<TextareaState>,
    search: Entity<InputState>,
    pr: Entity<InputState>,
    draft: Draft,
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
    worktree: bool,
    repo: Option<String>,
    branches: Vec<(String, Option<i64>)>,
    /// Branches on origin with no local twin.
    remote_branches: Vec<String>,
    source: Source,
    base: usize,
    /// The base to pick once the branches load, instead of the repo's.
    want_base: Option<String>,
    copy_env: bool,
    run_setup: bool,
    provider: &'static str,
    model: String,
    effort: String,
    access: String,
    reply: CreateReply,
    picker: Option<Picker>,
    images: Vec<Attachment>,
}

/// A pasted image and the file the agent reads it from.
struct Attachment {
    path: PathBuf,
    image: Arc<Image>,
}

impl Default for Draft {
    fn default() -> Self {
        Self {
            taken: HashSet::new(),
            seed: 0,
            worktree: false,
            repo: None,
            branches: Vec::new(),
            remote_branches: Vec::new(),
            source: Source::New,
            base: 0,
            want_base: None,
            copy_env: false,
            run_setup: false,
            provider: "claude",
            model: String::new(),
            effort: String::new(),
            access: String::new(),
            reply: CreateReply::default(),
            picker: None,
            images: Vec::new(),
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
        offered && self.worktree && self.source == Source::New && !prompt.trim().is_empty()
    }

    /// A pull request always gets its own worktree; the checkout picked returns once it is unlinked.
    fn in_worktree(&self) -> bool {
        self.worktree || matches!(self.source, Source::Pr(_))
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

    fn pick_checkout(&mut self, worktree: bool) {
        (self.worktree, self.picker) = (worktree, None);
    }

    fn open_branch(&mut self, branch: String) {
        (self.source, self.picker) = (Source::Branch(branch), None);
    }

    /// `in_tree`: a worktree is open to start the session in.
    fn ready(&self, name: &str, in_tree: bool) -> bool {
        let place = match (self.in_worktree(), &self.source) {
            (false, _) => in_tree,
            (true, Source::New) => self.repo.is_some() && !self.branches.is_empty() && name_problem(name, &self.taken).is_none(),
            (true, Source::Branch(_)) => self.repo.is_some() && !name.is_empty(),
            (true, Source::Pr(_)) => self.repo.is_some() && parse_pr(name).is_some(),
        };
        place && !self.reply.waiting()
    }

    /// The folder pocketd will likely make for `name`; `Creates::started` swaps in the one it made.
    fn folder(&self, name: &str) -> String {
        match self.source {
            Source::New => name.to_string(),
            Source::Branch(_) => name.replace('/', "-"),
            Source::Pr(_) => format!("pr-{}", parse_pr(name).unwrap_or_default()),
        }
    }

    fn pick_provider(&mut self, provider: &'static str) {
        let pick = self.pick().switched(provider);
        (self.model, self.effort, self.access) = (pick.model, pick.effort, pick.access);
        (self.provider, self.picker) = (provider, None);
    }

    fn spec(&self, project: &str, tree: &str, name: &str, prompt: &str) -> Value {
        let checkout = match (self.in_worktree(), &self.source) {
            (false, _) => json!({"worktree": tree}),
            (true, Source::New) => json!({"new": {"name": name, "base": self.base_branch(), "copy": self.copy_env, "setup": self.run_setup}}),
            (true, Source::Branch(_)) => json!({"new": {"branch": github::base_branch(name), "copy": self.copy_env, "setup": self.run_setup}}),
            (true, Source::Pr(_)) => json!({"new": {"pr": name, "copy": self.copy_env, "setup": self.run_setup}}),
        };
        self.pick().spec(project, checkout, &with_images(prompt.trim(), &self.images))
    }

    /// What to remember for the Project.
    fn pick(&self) -> LaunchPick {
        LaunchPick { provider: self.provider.to_string(), model: self.model.clone(), effort: self.effort.clone(), access: self.access.clone() }
    }

    fn open(&mut self, last: &LaunchPick) {
        self.provider = LaunchPick::known_provider(&last.provider);
        (self.model, self.effort, self.access) = (last.model.clone(), last.effort.clone(), last.access.clone());
        self.reply = CreateReply::default();
    }

    /// Reopening the sheet clears its error row, so a failure that arrives while it is closed goes to the page error line.
    fn failed(&mut self, request: &str, message: String, detail: String, open: bool) -> Option<String> {
        if open && self.reply.answer(request, Some((message.clone(), detail))) {
            return None;
        }
        Some(message)
    }

    /// Whether the image is new to this draft and its file still needs writing.
    fn attach(&mut self, attachment: Attachment) -> bool {
        let new = !self.images.iter().any(|a| a.path == attachment.path);
        if new {
            self.images.push(attachment);
        }
        new
    }

    /// Drops the image whose file couldn't be written, if this draft still holds it.
    fn unsaved(&mut self, path: &Path, detail: String) {
        let held = self.images.len();
        self.images.retain(|a| a.path != path);
        if self.images.len() < held {
            self.reply.error = Some(("Couldn't save the pasted image".into(), detail));
        }
    }
}

/// A checkout choice as its chip and menu row show it.
struct Checkout {
    glyph: &'static str,
    label: String,
    hint: &'static str,
}

/// Not a worktree means the open tree: the project's own checkout, or the worktree named `other`.
fn checkout_choice(worktree: bool, other: Option<String>) -> Checkout {
    if worktree {
        return Checkout { glyph: "worktree", label: "Worktree".into(), hint: "Its own branch and copy of the files" };
    }
    match other {
        Some(label) => Checkout { glyph: "worktree", label, hint: "This worktree as it is: files and branch are shared" },
        None => Checkout { glyph: "laptop", label: "Local".into(), hint: "The project's checkout as it is: files and branch are shared" },
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

/// The prompt goes to the agent as text, so images go in as paths it can read.
fn with_images(prompt: &str, images: &[Attachment]) -> String {
    if images.is_empty() {
        return prompt.to_string();
    }
    let list: Vec<String> = images.iter().map(|a| format!("- {}", a.path.display())).collect();
    let block = format!("# Attached images\n\n{}", list.join("\n"));
    if prompt.is_empty() { block } else { format!("{prompt}\n\n{block}") }
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

const ADJECTIVES: [&str; 8] = ["brave", "calm", "eager", "fuzzy", "keen", "lucky", "quiet", "swift"];
const NOUNS: [&str; 8] = ["otter", "heron", "maple", "comet", "falcon", "cedar", "koala", "lynx"];

/// Case-insensitive because APFS is; a branch `a/b` owns the name `a`.
fn is_taken(name: &str, taken: &HashSet<String>) -> bool {
    taken.iter().any(|t| t.split('/').next().unwrap_or(t).eq_ignore_ascii_case(name))
}

fn free_name(seed: usize, taken: &HashSet<String>) -> String {
    let count = ADJECTIVES.len() * NOUNS.len();
    (0..count).map(|i| (seed + i) % count).map(|n| format!("{}-{}", ADJECTIVES[n / NOUNS.len()], NOUNS[n % NOUNS.len()])).find(|n| !is_taken(n, taken)).unwrap_or_else(|| unique("worktree", taken))
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

/// The image a paste carries when it has no text; text, copied files included, pastes as itself.
fn pasted_image(item: &ClipboardItem) -> Option<Image> {
    if item.text().is_some() {
        return None;
    }
    item.entries().iter().find_map(|e| match e {
        ClipboardEntry::Image(image) => Some(image.clone()),
        _ => None,
    })
}

fn attachment_path(image: &Image, dir: &Path) -> PathBuf {
    dir.join(format!("{}.{}", image.id, image.format.extension()))
}

/// Owner-only, like the rest of Pocket's files: screenshots can hold secrets.
fn save_image(image: &Image, path: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt;
    if let Some(dir) = path.parent() {
        std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)?;
    }
    std::fs::write(path, &image.bytes)
}

/// The image shows at once and its file is written behind it, so starting never waits on a write; the agent opens it long after.
fn paste_image(desktop: &WeakEntity<Desktop>, item: &ClipboardItem, window: &mut Window, cx: &mut App) -> bool {
    let (Some(image), Some(d)) = (pasted_image(item), desktop.upgrade()) else { return false };
    let image = Arc::new(image);
    let path = attachment_path(&image, &d.read(cx).store.home().join("attachments"));
    let new = d.update(cx, |d, cx| {
        cx.notify();
        d.new_form.draft.attach(Attachment { path: path.clone(), image: image.clone() })
    });
    if !new {
        return true;
    }
    let save = cx.background_executor().spawn({
        let path = path.clone();
        async move { save_image(&image, &path) }
    });
    let desktop = desktop.clone();
    window
        .spawn(cx, async move |cx| {
            if let Err(e) = save.await {
                desktop
                    .update(cx, |d, cx| {
                        d.new_form.draft.unsaved(&path, e.to_string());
                        cx.notify();
                    })
                    .ok();
            }
        })
        .detach();
    true
}

fn default_first(branches: &mut [(String, Option<i64>)], preferred: &str, current: &str) {
    let default = default_base(branches.iter().map(|(b, _)| b.as_str()), preferred, current);
    if !branches.is_empty() {
        branches[..=default].rotate_right(1);
    }
}

impl NewForm {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let prompt = cx.new(|cx| TextareaState::new(window, cx).placeholder("Describe what the agent should do…").rows(4));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search branches…"));
        let pr = cx.new(|cx| InputState::new(window, cx).placeholder("123, #123 or a PR URL"));
        let subs = vec![
            cx.subscribe_in(&prompt, window, |this, _, ev: &InputEvent, window, cx| {
                if let InputEvent::PressEnter { secondary: true, .. } = ev {
                    this.start_session(window, cx);
                }
            }),
            cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify()),
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
        (Self { prompt, search, pr, draft: Draft::default() }, subs)
    }
}

impl Desktop {
    /// The Project's own agent, else what it last started, with an agent this app knows.
    pub(crate) fn last_pick(&self) -> LaunchPick {
        let repo = self.project.as_ref().and_then(|p| self.store.repos.get(p));
        self.store.agents.pick(&repo.map(|r| r.agent.clone()).unwrap_or_default(), repo.map(|r| r.launch.clone()).unwrap_or_default())
    }

    pub fn reset_new_form(&mut self, prompt: Option<String>, worktree: bool, window: &mut Window, cx: &mut Context<Self>) {
        let last = self.last_pick();
        let worktree = worktree || self.store.agents.new_worktree;
        let text = prompt.unwrap_or_default();
        let f = &mut self.new_form;
        f.draft.seed = crate::util::now_ms() as usize;
        f.draft.taken.clear();
        f.draft.source = Source::New;
        f.prompt.update(cx, |s, cx| {
            s.set_value(text, window, cx);
            s.focus(window, cx);
        });
        f.draft.worktree = worktree;
        f.draft.images.clear();
        f.draft.open(&last);
        (f.draft.picker, f.draft.want_base) = (None, None);
        match self.project.clone() {
            Some(repo) => self.pick_repo(repo, window, cx),
            None => {
                f.draft.repo = None;
                f.draft.branches.clear();
            }
        }
    }

    fn pick_repo(&mut self, repo: String, window: &mut Window, cx: &mut Context<Self>) {
        let cfg = self.store.repos.get(&repo).cloned().unwrap_or_default();
        let folders = self.store.worktrees_dir(&repo, &home());
        let f = &mut self.new_form.draft;
        f.repo = Some(repo.clone());
        f.branches.clear();
        f.remote_branches.clear();
        f.base = 0;
        f.copy_env = !cfg.copy.is_empty();
        f.run_setup = !cfg.setup.is_empty();
        let dir = repo.clone();
        let task = cx.background_executor().spawn(async move {
            let current = git::read(&dir, true).map(|r| r.branch).unwrap_or_default();
            let branches = git::dated_branches(&dir);
            let entries = std::fs::read_dir(&folders).into_iter().flatten().flatten();
            let taken: HashSet<String> = branches.iter().map(|(b, _)| b.clone()).chain(entries.filter_map(|e| e.file_name().into_string().ok())).collect();
            (current, branches, taken, git::remote_branches(&dir))
        });
        cx.spawn_in(window, async move |this, cx| {
            let (current, mut branches, taken, remote) = task.await;
            this.update(cx, |d, cx| {
                let f = &mut d.new_form;
                if f.draft.repo.as_ref() != Some(&repo) {
                    return;
                }
                default_first(&mut branches, f.draft.want_base.as_deref().unwrap_or(&cfg.base), &current);
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

    fn session_ready(&self) -> bool {
        self.new_form.draft.ready(&self.new_form.draft.name(), self.cwd().is_some())
    }

    fn open_branch(&mut self, branch: String, window: &mut Window, cx: &mut Context<Self>) {
        self.new_form.draft.open_branch(branch);
        self.start_session(window, cx);
    }

    /// A new worktree's sheet closes at once and its progress shows in the main view; a session's waits for pocketd.
    fn start_session(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.session_ready() {
            return;
        }
        if self.terminals.link.stale().is_some() {
            self.error = Some(link::UPDATING.into());
            return cx.notify();
        }
        let name = self.new_form.draft.name();
        let prompt = self.new_form.prompt.read(cx).value().to_string();
        let tree = self.cwd().unwrap_or_default();
        let f = &self.new_form.draft;
        let Some(project) = f.repo.clone() else { return };
        let (mut spec, worktree, folder) = (f.spec(&project, &tree, &name, &prompt), f.in_worktree(), f.folder(&name));
        if f.auto_names(&prompt, self.agents.names_offered()) {
            spec["checkout"]["new"]["autoName"] = true.into();
        }
        self.store.agents.launch(&mut spec);
        self.store.repos.entry(project.clone()).or_default().launch = f.pick();
        if worktree {
            self.store.collapsed.remove(&project);
        }
        self.store.save();
        let request = self.outbox.create(spec.clone());
        if !worktree {
            self.new_form.draft.reply.sent(request);
            return cx.notify();
        }
        let path = format!("{}/{folder}", self.store.worktrees_dir(&project, &home()));
        self.creates.list.push(Create::new(request, path.clone(), spec, Instant::now()));
        self.close_overlay(window, cx);
        self.select_tree(project, Some(path), cx);
    }

    /// pocketd's replies to a create. The Terminal opens once pocketd lists it (`Terminals::arrived`).
    pub(crate) fn on_launch(&mut self, ev: Event, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            Event::Creating { request, terminal, cwd, setup } => {
                self.terminals.created(&request, terminal.clone(), cwd.clone(), setup);
                self.daemon.send(json!({"op": "list"}));
                if self.creates.get(&request).is_some() {
                    self.store.track(&cwd);
                    self.save_soon(cx);
                }
                if let Some(expected) = self.creates.started(&request, cwd.clone(), &mut self.worktrees) {
                    if self.worktree.as_ref() == Some(&expected) {
                        self.worktree = Some(cwd);
                    }
                    self.refresh_git(cx);
                } else if self.empty_pane.launch.reply.answer(&request, None) {
                    self.terminals.aim(&terminal, self.empty_pane.launch.pane);
                } else if self.new_form.draft.reply.answer(&request, None) && self.overlay == Some(Overlay::NewSession) {
                    self.close_overlay(window, cx);
                }
            }
            Event::Progress { request, step, note } => {
                if let Some(c) = self.creates.get(&request) {
                    c.reach(&step, &note, Instant::now());
                }
            }
            Event::Created { request, .. } => {
                self.creates.remove(&request);
            }
            Event::CreateFailed { request, message, detail, .. } => {
                if let Some(c) = self.creates.get(&request) {
                    c.fail(message, detail, Instant::now());
                } else if !self.empty_prompt_failed(&request, &message, &detail, window, cx) {
                    let open = self.overlay == Some(Overlay::NewSession);
                    if let Some(message) = self.new_form.draft.failed(&request, message, detail, open) {
                        self.error = Some(message);
                    }
                }
            }
            _ => {}
        }
        cx.notify();
    }

    pub fn new_worktree(&mut self, _: &crate::actions::NewWorktree, window: &mut Window, cx: &mut Context<Self>) {
        if self.agents.observe_only() {
            return;
        }
        self.close_menus();
        self.overlay = Some(Overlay::NewSession);
        self.reset_new_form(None, true, window, cx);
        cx.notify();
    }

    /// The sheet again, filled in as it was for a create that failed before git made its worktree.
    pub(crate) fn reopen_new_worktree(&mut self, c: &Create, window: &mut Window, cx: &mut Context<Self>) {
        self.new_worktree(&crate::actions::NewWorktree, window, cx);
        let f = &mut self.new_form.draft;
        f.want_base = Some(c.base().to_string());
        f.source = Source::of(c.asked());
        self.new_form.prompt.update(cx, |s, cx| s.set_value(c.prompt().to_string(), window, cx));
    }

    pub fn new_session_view(&mut self, _: &mut Window, cx: &mut Context<Self>) -> Div {
        let f = &self.new_form.draft;
        let repo = f.repo.clone().unwrap_or_default();
        let name = self.repo_name(&repo);
        let close = ui::icon_button_sized("form-close", "x", 28., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let header = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(4.))
            .pb(px(2.))
            .child(div().text_size(px(16.)).font_weight(FontWeight::BOLD).child(if f.in_worktree() { "New worktree" } else { "New session" }))
            .child(div().flex().items_center().gap(px(6.)).text_size(px(13.)).text_color(TEXT_3).child(ui::repo_tile(&crate::util::initials(&name), 18., false, None)).child(name))
            .child(div().ml_auto().child(close));
        let agent = self.agent_select(cx);
        let model = self.model_select(cx);
        let access = self.access_select(cx);
        let checkout = self.checkout_select(cx);
        let pr = f.linked_pr();
        let branch = match pr {
            Some(n) => Some(div().h(px(30.)).px(px(6.)).flex().flex_none().items_center().gap(px(7.)).text_color(TEXT_3).child(icon("pull-request", 14., TEXT_3)).child(format!("based off PR #{n}"))),
            None => f.worktree.then(|| self.branch_select(cx)),
        };
        let link = (pr.is_none() && self.agents.opens()).then(|| self.pr_select(cx));
        let ready = self.session_ready();
        let send = ui::primary(div().id("form-start").size(px(32.)).flex().flex_none().items_center().justify_center().rounded(px(16.)).cursor_pointer())
            .child(icon("arrow-up", 16., ON_PRIMARY))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.start_session(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let desktop = cx.entity().downgrade();
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
            div().h(px(26.)).pl(px(9.)).pr(px(4.)).flex().flex_none().items_center().gap(px(6.)).rounded(px(8.)).bg(FILL_2).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(icon("pull-request", 13., TEXT_2)).child(format!("#{n}")).child(unlink)
        });
        let attached = (pill.is_some() || !f.images.is_empty()).then(|| {
            div().flex().items_center().gap(px(8.)).pt(px(14.)).px(px(16.)).children(pill).children(f.images.iter().enumerate().map(|(i, a)| {
                let remove = div()
                    .id(("image-remove", i))
                    .absolute()
                    .top(px(-6.))
                    .right(px(-6.))
                    .size(px(18.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(9.))
                    .bg(POPOVER)
                    .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
                    .cursor_pointer()
                    .active(|s| s.opacity(0.6))
                    .child(icon("x", 10., TEXT_2))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.new_form.draft.images.remove(i);
                        cx.notify();
                    }));
                div().relative().size(px(56.)).flex_none().rounded(px(10.)).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]).child(img(a.image.clone()).size_full().rounded(px(10.)).object_fit(ObjectFit::Cover)).child(remove)
            }))
        });
        let composer = div()
            .flex()
            .flex_col()
            .rounded(px(14.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(rgba(0x1111130a), 1., 2.)])
            .children(attached)
            // The textarea pads itself 8px × 10px and wraps 10px short of its edge; the frame restores the design's 14/16/4 and its line breaks.
            .child(div().pt(px(6.)).pl(px(6.)).mr(px(-6.)).child(Textarea::new(&self.new_form.prompt).appearance(false).h(px(105.)).text_size(px(15.)).line_height(px(23.25)).on_paste(move |item, window, cx| paste_image(&desktop, item, window, cx))))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .pt(px(6.))
                    .px(px(10.))
                    .pb(px(10.))
                    .text_size(px(13.))
                    .child(agent)
                    .child(checkout)
                    .children(branch)
                    .child(div().flex_1())
                    .children(link)
                    .child(send),
            );
        let error = f.reply.error.clone().map(|(message, detail)| ui::failure(message, detail));
        let footer = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .px(px(6.))
            .text_size(px(13.))
            .child(model)
            .child(access)
            .child(div().ml_auto().text_size(px(12.)).text_color(TEXT_4).whitespace_nowrap().child("⌘↵ to start · esc to cancel"));
        div().absolute().top(px(110.)).left_0().right_0().flex().justify_center().child(
            // The design's 0.5px border renders 1px wide and insets the sheet's content.
            ui::pop(div().w(px(640.)).pt(px(17.)).px(px(17.)).pb(px(15.)).flex().flex_col().gap(px(10.))).rounded(px(R_DIALOG)).occlude().child(header).child(composer).children(error).child(footer),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Attachment, Draft, Source, attachment_path, checkout_choice, default_first, listed_branches, name_problem, parse_pr, pasted_image, save_image};
    use agents::CreateReply;
    use gpui_kit::{ClipboardItem, Image, ImageFormat};
    use serde_json::json;
    use store::LaunchPick;
    use std::collections::HashSet;

    fn sent(request: &str) -> CreateReply {
        let mut reply = CreateReply::default();
        reply.sent(request.into());
        reply
    }

    #[test]
    fn a_session_in_the_open_tree_needs_only_an_open_tree() {
        let draft = Draft::default();
        assert_eq!((draft.ready("", true), draft.ready("fix", false)), (true, false));
    }

    #[test]
    fn local_is_the_projects_checkout_or_names_the_open_worktree() {
        let choice = |worktree, other: Option<&str>| {
            let c = checkout_choice(worktree, other.map(String::from));
            (c.glyph, c.label)
        };
        assert_eq!(choice(false, None), ("laptop", "Local".to_string()));
        assert_eq!(choice(false, Some("Fix login")), ("worktree", "Fix login".to_string()));
        assert_eq!(choice(true, Some("Fix login")), ("worktree", "Worktree".to_string()));
    }

    #[test]
    fn a_local_session_sends_only_the_open_tree_whatever_the_worktree_choices_were() {
        let draft = Draft { worktree: false, source: Source::Branch("fix/login".into()), repo: Some("/p".into()), branches: vec![("main".into(), None)], copy_env: true, ..Draft::default() };
        assert!(draft.ready("", true));
        assert_eq!(draft.spec("/p", "/wt/calm-cedar", "", "")["checkout"], json!({"worktree": "/wt/calm-cedar"}));
    }

    #[test]
    fn a_new_worktree_needs_a_repository_its_branches_and_a_usable_name() {
        let taken: HashSet<String> = ["main".to_string()].into();
        let draft = Draft { worktree: true, repo: Some("/src/app".into()), branches: vec![("main".into(), None)], taken, ..Draft::default() };
        assert!(draft.ready("fix-ci", false));
        assert!(!draft.ready("main", true));
        assert!(!draft.ready("fix ci", true));
        assert!(!Draft { repo: None, ..draft }.ready("fix-ci", true));
        let draft = Draft { worktree: true, repo: Some("/src/app".into()), ..Draft::default() };
        assert!(!draft.ready("fix-ci", true));
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
        let taken: HashSet<String> = ["brave-otter", "Brave-Heron", "release/1.0"].map(String::from).into();
        let draft = Draft { worktree: true, taken, ..Draft::default() };
        assert_eq!((draft.auto_name(), draft.name()), ("brave-maple".to_string(), "brave-maple".to_string()));
        let fresh = |seed| Draft { seed, ..Draft::default() }.auto_name();
        assert_eq!((fresh(63), fresh(64)), ("swift-lynx".to_string(), "brave-otter".to_string()));
    }

    #[test]
    fn an_image_paste_is_saved_to_a_private_file_and_text_pastes_as_itself() {
        use std::os::unix::fs::PermissionsExt;
        let root = std::env::temp_dir().join(format!("pocket-paste-test-{}", std::process::id()));
        let image = pasted_image(&ClipboardItem::new_image(&Image::from_bytes(ImageFormat::Png, vec![1, 2, 3]))).unwrap();
        let path = attachment_path(&image, &root.join("attachments"));
        save_image(&image, &path).unwrap();
        assert_eq!((path.extension().and_then(|e| e.to_str()), std::fs::read(&path).unwrap()), (Some("png"), vec![1, 2, 3]));
        assert_eq!(std::fs::metadata(root.join("attachments")).unwrap().permissions().mode() & 0o777, 0o700);
        assert!(pasted_image(&ClipboardItem::new_string("fix ci".into())).is_none());
        std::fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn the_same_image_attaches_once_and_one_that_failed_to_save_is_dropped_with_why() {
        let image = std::sync::Arc::new(Image::from_bytes(ImageFormat::Png, vec![1]));
        let at = |path: &str| Attachment { path: path.into(), image: image.clone() };
        let mut draft = Draft::default();
        assert!(draft.attach(at("/h/a.png")));
        assert!(!draft.attach(at("/h/a.png")));
        assert!(draft.attach(at("/h/b.png")));
        draft.unsaved("/h/c.png".as_ref(), "disk full".into());
        assert_eq!((draft.images.len(), draft.reply.error.is_none()), (2, true));
        draft.unsaved("/h/a.png".as_ref(), "disk full".into());
        assert_eq!(draft.images.iter().map(|a| a.path.to_str().unwrap()).collect::<Vec<_>>(), ["/h/b.png"]);
        assert_eq!(draft.reply.error, Some(("Couldn't save the pasted image".to_string(), "disk full".to_string())));
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
    fn attached_images_follow_the_prompt_as_paths() {
        let image = std::sync::Arc::new(Image::from_bytes(ImageFormat::Png, vec![1]));
        let at = |path: &str| Attachment { path: path.into(), image: image.clone() };
        let draft = Draft { images: vec![at("/h/attachments/a.png"), at("/h/attachments/b.png")], ..Draft::default() };
        let block = "# Attached images\n\n- /h/attachments/a.png\n- /h/attachments/b.png";
        assert_eq!(draft.spec("/p", "/p/w", "", " Fix CI ")["prompt"], format!("Fix CI\n\n{block}"));
        assert_eq!(draft.spec("/p", "/p/w", "", "")["prompt"], block);
    }

    #[test]
    fn a_spec_leaves_access_to_the_app_without_planning_and_carries_no_argv() {
        let want = json!({"project": "/p", "checkout": {"worktree": "/p/w"}, "provider": "claude", "plan": false, "prompt": "Fix CI"});
        assert_eq!(Draft::default().spec("/p", "/p/w", "", "  Fix CI  "), want);
        let new = Draft { worktree: true, branches: vec![("main".into(), None)], copy_env: true, ..Draft::default() };
        let want = json!({"project": "/p", "checkout": {"new": {"name": "fix-ci", "base": "main", "copy": true, "setup": false}}, "provider": "claude", "plan": false});
        assert_eq!(new.spec("/p", "/p", "fix-ci", " \n "), want);
    }

    #[test]
    fn an_existing_branch_needs_only_a_repository_and_a_branch() {
        let draft = Draft { worktree: true, source: Source::Branch("fix/login".into()), repo: Some("/src/app".into()), taken: ["main".to_string()].into(), ..Draft::default() };
        assert!(draft.ready("main", false));
        assert!(!draft.ready("", true));
        assert!(!Draft { repo: None, ..draft }.ready("main", true));
    }

    #[test]
    fn an_existing_branch_is_sent_without_a_name_or_base_and_lands_in_a_dashed_folder() {
        let draft = Draft { worktree: true, source: Source::Branch("fix/login".into()), branches: vec![("main".into(), None)], run_setup: true, ..Draft::default() };
        let want = json!({"project": "/p", "checkout": {"new": {"branch": "fix/login", "copy": false, "setup": true}}, "provider": "claude", "plan": false});
        assert_eq!(draft.spec("/p", "/p", "fix/login", ""), want);
        assert_eq!(draft.spec("/p", "/p", "origin/fix/login", ""), want);
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
        let draft = Draft { worktree: true, source: Source::Pr("#7".into()), repo: Some("/p".into()), copy_env: true, ..Draft::default() };
        assert!(draft.ready("#7", false) && !draft.ready("seven", false));
        let want = json!({"project": "/p", "checkout": {"new": {"pr": "#7", "copy": true, "setup": false}}, "provider": "claude", "plan": false});
        assert_eq!(draft.spec("/p", "/p", "#7", ""), want);
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
    fn picks_are_remembered_per_project() {
        let draft = Draft { provider: "codex", model: "gpt-5".into(), effort: "high".into(), access: "auto".into(), ..Draft::default() };
        let pick = draft.pick();
        assert_eq!(pick, LaunchPick { provider: "codex".into(), model: "gpt-5".into(), effort: "high".into(), access: "auto".into() });
        let mut next = Draft::default();
        next.open(&pick);
        assert_eq!((next.provider, next.model.as_str(), next.effort.as_str(), next.access.as_str()), ("codex", "gpt-5", "high", "auto"));
        next.open(&LaunchPick::default());
        assert_eq!((next.provider, next.model.as_str(), next.effort.as_str(), next.access.as_str()), ("claude", "", "", ""));
    }

    #[test]
    fn a_remembered_model_and_effort_start_the_session_until_the_agent_changes() {
        let mut draft = Draft { model: "opus".into(), effort: "high".into(), ..Draft::default() };
        let spec = draft.spec("/p", "/p", "", "");
        assert_eq!((&spec["model"], &spec["effort"]), (&json!("opus"), &json!("high")));
        draft.pick_provider("claude");
        assert_eq!(draft.spec("/p", "/p", "", "")["model"], "opus");
        draft.pick_provider("codex");
        let spec = draft.spec("/p", "/p", "", "");
        assert_eq!((spec.get("model"), spec.get("effort")), (None, None));
    }

    #[test]
    fn a_picked_access_starts_the_session_whatever_the_agent() {
        let mut draft = Draft { access: "edits".into(), ..Draft::default() };
        draft.pick_provider("codex");
        assert_eq!(draft.spec("/p", "/p", "", "")["access"], "edits");
    }

    #[test]
    fn a_create_error_keeps_the_draft() {
        let mut draft = Draft { provider: "codex", reply: sent("r1"), ..Draft::default() };
        assert!(!draft.ready("", true));
        assert!(!draft.reply.answer("r0", None));
        assert!(draft.reply.answer("r1", Some(("Setup exited 1".into(), "npm ERR!".into()))));
        assert_eq!((draft.provider, draft.reply.waiting(), draft.ready("", true)), ("codex", false, true));
        assert_eq!(draft.reply.error, Some(("Setup exited 1".to_string(), "npm ERR!".to_string())));
    }

    #[test]
    fn a_create_error_after_the_sheet_closed_goes_to_the_page() {
        let mut draft = Draft { reply: sent("r1"), ..Draft::default() };
        assert_eq!(draft.failed("r1", "Setup exited 1".into(), "npm ERR!".into(), false), Some("Setup exited 1".to_string()));
        assert_eq!(draft.reply.error, None);
        assert_eq!(draft.failed("r1", "Setup exited 1".into(), "npm ERR!".into(), true), None);
        assert_eq!(draft.reply.error, Some(("Setup exited 1".to_string(), "npm ERR!".to_string())));
    }

    #[test]
    fn pocketd_names_only_a_new_branch_that_has_a_prompt() {
        let new = Draft { worktree: true, source: Source::New, ..Draft::default() };
        assert!(new.auto_names("fix the login", true));
        assert!(!new.auto_names("  ", true));
        assert!(!new.auto_names("fix the login", false));
        assert!(!Draft { worktree: true, source: Source::Branch("fix".into()), ..Draft::default() }.auto_names("fix", true));
        assert!(!Draft::default().auto_names("fix the login", true));
    }
}
