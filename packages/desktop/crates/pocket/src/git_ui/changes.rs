pub(crate) mod commit_box;
mod compare;
mod header;
mod rows;
mod scope;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, empty};
use crate::git_ui::diff::At;
use crate::git_ui::graph::{Graph, with_graph};
use crate::git_ui::pull_requests::PrItem;
use crate::renames::Slot;
use compare::CompareView;
use git::github::Pr;
use git::{FileStat, Repo};
use gpui_kit::component::input::{InputEvent, InputState, TextareaState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::*;
use std::collections::{BTreeMap, HashSet};
use std::ops::Range;
use std::sync::Arc;
use std::time::Instant;
use store::scope::Scope;
use theme::{TEXT_3, Token};
use workspace::Doc;

#[derive(Clone, Copy, PartialEq)]
pub enum CommitKind {
    Commit,
    Push,
    Amend,
    /// Commit and push, then open the PR composer.
    Ship,
}

#[derive(Debug, PartialEq)]
enum CommitStep {
    Skip,
    AskMessage,
    /// Nothing is staged and the settings say to commit only what is.
    StageFirst,
    /// `stage` holds every changed path when none is staged, so the commit takes them all.
    Run {
        stage: Vec<String>,
        busy: &'static str,
    },
}

/// Commits the staged files, or with `commit_all` every change when none is staged. Amending keeps the last message unless a new one is typed.
fn commit_step(files: &[FileStat], kind: CommitKind, message: &str, busy: bool, commit_all: bool) -> CommitStep {
    let amend = kind == CommitKind::Amend;
    if busy || (!amend && files.is_empty()) {
        return CommitStep::Skip;
    }
    let none_staged = !files.iter().any(|f| f.staged);
    if !amend && none_staged && !commit_all {
        return CommitStep::StageFirst;
    }
    if !amend && message.is_empty() {
        return CommitStep::AskMessage;
    }
    let stage = if amend || !none_staged { Vec::new() } else { files.iter().map(|f| f.path.clone()).collect() };
    CommitStep::Run { stage, busy: if amend { "Amending…" } else { "Committing…" } }
}

fn commit_label(files: &[FileStat], busy: Option<&'static str>, commit_all: bool) -> &'static str {
    match busy {
        Some(busy) => busy,
        None if commit_all && !files.is_empty() && !files.iter().any(|f| f.staged) => "Commit All",
        None => "Commit",
    }
}

fn commit_ready(files: &[FileStat], message: &str, busy: bool, commit_all: bool) -> bool {
    !files.is_empty() && !message.trim().is_empty() && !busy && (commit_all || files.iter().any(|f| f.staged))
}

/// A foldable group of rows: Uncommitted's staged and unstaged files, or a Compare's files.
#[derive(Clone, Copy, PartialEq)]
enum Section {
    Staged,
    Changes,
    Compared,
}

impl Section {
    fn key(self) -> &'static str {
        match self {
            Section::Staged => "staged",
            Section::Changes => "changes",
            Section::Compared => "compared",
        }
    }
}

enum Item {
    Dir { key: String, label: String, depth: usize, open: bool },
    File { file: FileStat, depth: usize },
}

enum Row {
    Header(Section, Vec<String>),
    Item(Section, Item),
}

/// The folder every file in `files` continues into below `base`, if they share one.
fn common_dir<'a>(files: &[&'a FileStat], base: &str) -> Option<&'a str> {
    let mut dirs = files.iter().map(|f| f.path[base.len()..].split_once('/').map(|(d, _)| d));
    let first = dirs.next()??;
    dirs.all(|d| d == Some(first)).then_some(first)
}

/// Folders first, each chain of single-folder folders merged into one row, then the files directly under `base`.
fn tree(files: &[&FileStat], base: &str, depth: usize, section: Section, folded: &HashSet<String>, out: &mut Vec<Item>) {
    let mut dirs: BTreeMap<&str, Vec<&FileStat>> = BTreeMap::new();
    let mut leaves = Vec::new();
    for &f in files {
        match f.path[base.len()..].split_once('/') {
            Some((dir, _)) => dirs.entry(dir).or_default().push(f),
            None => leaves.push(f),
        }
    }
    for (dir, sub) in dirs {
        let mut path = format!("{base}{dir}/");
        while let Some(next) = common_dir(&sub, &path) {
            path = format!("{path}{next}/");
        }
        let key = format!("{}:{path}", section.key());
        let open = !folded.contains(&key);
        out.push(Item::Dir { key, label: path[base.len()..path.len() - 1].to_string(), depth, open });
        if open {
            tree(&sub, &path, depth + 1, section, folded, out);
        }
    }
    out.extend(leaves.into_iter().map(|file| Item::File { file: file.clone(), depth }));
}

/// One uniform-height row per section header, folder and file, so the list only renders what's in view.
fn change_rows(files: &[FileStat], as_tree: bool, folded: &HashSet<String>) -> Vec<Row> {
    let mut rows = Vec::new();
    for s in [Section::Staged, Section::Changes] {
        let files: Vec<&FileStat> = files.iter().filter(|f| if s == Section::Staged { f.staged } else { f.unstaged }).collect();
        push_files(&mut rows, s, &files, as_tree, folded);
    }
    rows
}

/// A Compare's files under one header, as Uncommitted lists its own.
fn compare_rows(files: &[FileStat], as_tree: bool, folded: &HashSet<String>) -> Vec<Row> {
    let mut rows = Vec::new();
    push_files(&mut rows, Section::Compared, &files.iter().collect::<Vec<_>>(), as_tree, folded);
    rows
}

/// What a file renamed from `old` shows besides its name: the old name, or the whole old path when it moved folders.
fn renamed_from<'a>(old: &'a str, path: &str) -> &'a str {
    let (old_dir, old_name) = old.rsplit_once('/').unwrap_or(("", old));
    let dir = path.rsplit_once('/').map_or("", |(d, _)| d);
    if old_dir == dir { old_name } else { old }
}

/// `files` under `s`'s header, as a tree or flat, unless folded.
fn push_files(rows: &mut Vec<Row>, s: Section, files: &[&FileStat], as_tree: bool, folded: &HashSet<String>) {
    if files.is_empty() {
        return;
    }
    rows.push(Row::Header(s, files.iter().map(|f| f.path.clone()).collect()));
    if folded.contains(s.key()) {
        return;
    }
    let mut items = Vec::new();
    if as_tree {
        tree(files, "", 0, s, folded, &mut items);
    } else {
        items.extend(files.iter().map(|&f| Item::File { file: f.clone(), depth: 0 }));
    }
    rows.extend(items.into_iter().map(|item| Row::Item(s, item)));
}

fn flip_staged(files: &mut [FileStat], paths: &[String], staged: bool) {
    let picked: HashSet<&String> = paths.iter().collect();
    for f in files.iter_mut().filter(|f| picked.contains(&f.path)) {
        (f.staged, f.unstaged) = (staged, !staged);
    }
}

fn toggle_fold(folded: &mut HashSet<String>, key: &str) {
    if !folded.remove(key) {
        folded.insert(key.to_string());
    }
}

/// A pull request the panel offers for the shown branch, owned so `Desktop` can hand it down.
pub(crate) enum Offer {
    Open(String),
    Create,
    Hint(&'static str),
}

impl Offer {
    fn of(item: PrItem) -> Self {
        match item {
            PrItem::Open(url) => Offer::Open(url.to_string()),
            PrItem::Create => Offer::Create,
            PrItem::Hint(text) => Offer::Hint(text),
        }
    }

    fn item(&self) -> PrItem<'_> {
        match self {
            Offer::Open(url) => PrItem::Open(url),
            Offer::Create => PrItem::Create,
            Offer::Hint(text) => PrItem::Hint(text),
        }
    }
}

/// What `Desktop` hands the panel before it draws or is called: the shown worktree and what the panel reads of the rest of the app.
#[derive(Default)]
pub(crate) struct Inputs {
    pub(crate) cwd: Option<String>,
    pub(crate) repo: Option<Arc<Repo>>,
    pub(crate) scope: Scope,
    pub(crate) tree: bool,
    pub(crate) git: store::prefs::Git,
    pub(crate) message_argv: Vec<String>,
    /// The file the focused pane's diff shows, and what it compares.
    pub(crate) shown: Option<(String, At)>,
    pub(crate) pr: Option<(Pr, Token)>,
    pub(crate) offer: Option<Offer>,
    pub(crate) composing: bool,
    pub(crate) ship_menu: bool,
    /// What the branch was named before a rename that is still decoding.
    pub(crate) branch_was: Option<(String, Instant)>,
    pub(crate) seamed: bool,
    pub(crate) viewing: Option<(String, String)>,
}

/// What the panel asks `Desktop` for: what writes `repos`, the store, the overlay or the pull request.
pub(crate) enum ChangesEvent {
    Stage {
        paths: Vec<String>,
        staged: bool,
    },
    Discard(Vec<String>),
    Open {
        path: String,
        pin: bool,
    },
    OpenDoc(Doc, bool),
    Scope(String, Scope),
    ToggleTree,
    ToggleDraft,
    ShowPr,
    OpenPr(String),
    CreatePr,
    ShipMenu(bool),
    Refresh,
    Compared {
        cwd: String,
        against: String,
        read: Result<Option<Vec<FileStat>>, String>,
    },
    /// A commit made and pushed, with the repo read after it, so the PR composer can title itself from its commits.
    Shipped(String, Repo),
}

pub struct Changes {
    input: Entity<TextareaState>,
    /// What the commit button says while git works.
    busy: Option<&'static str>,
    writing: bool,
    error: Option<String>,
    pub(crate) commit_menu: bool,
    pub(crate) menu: bool,
    folded: HashSet<String>,
    scroll: UniformListScrollHandle,
    view: CompareView,
    pub(crate) compare_menu: bool,
    search: Entity<InputState>,
    /// The local branches, with their last commit's time, and origin's, of the worktree the menu last opened in.
    branches: Vec<(String, Option<i64>)>,
    remote: Vec<String>,
    graph: Entity<Graph>,
    inputs: Inputs,
    _subs: Vec<Subscription>,
}

impl EventEmitter<ChangesEvent> for Changes {}

impl Changes {
    pub fn new(graph: Entity<Graph>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let input = cx.new(|cx| TextareaState::new(window, cx).placeholder(format!("Message ({} to commit)", ui::os_keys("⌘↩"))).auto_grow(1, 8));
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search branches…"));
        let _subs = vec![
            cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { secondary: true, .. } => this.commit(CommitKind::Commit, window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
            cx.subscribe_in(&search, window, |this, _, ev: &InputEvent, _, cx| match ev {
                InputEvent::PressEnter { .. } => {
                    if let Some(choice) = this.compare_choices(cx).into_iter().next() {
                        this.pick_compare(choice, cx);
                    }
                }
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
        ];
        Self {
            input,
            busy: None,
            writing: false,
            error: None,
            commit_menu: false,
            menu: false,
            folded: HashSet::new(),
            scroll: UniformListScrollHandle::new(),
            view: CompareView::default(),
            compare_menu: false,
            search,
            branches: Vec::new(),
            remote: Vec::new(),
            graph,
            inputs: Inputs::default(),
            _subs,
        }
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.commit_menu || self.menu || self.compare_menu
    }

    pub(crate) fn shut_menus(&mut self) {
        (self.commit_menu, self.menu, self.compare_menu) = (false, false, false);
    }
}

impl Render for Changes {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let panel = div().flex_1().min_h_0().flex().flex_col();
        let Some(repo) = self.inputs.repo.clone() else {
            return panel.child(empty("Not a git repository."));
        };
        let cwd = self.inputs.cwd.clone().unwrap_or_default();
        let scope = self.inputs.scope.clone();
        let against = self.compare_branch();
        let changes = if scope.compare {
            self.compare_list(&cwd, against.as_deref(), cx)
        } else if repo.files.is_empty() {
            div().flex_1().child(empty("No changes."))
        } else {
            self.rows_list(change_rows(&repo.files, self.inputs.tree, &self.folded), cx)
        };
        let notice =
            self.view.notice(&cwd).filter(|_| scope.compare).map(|n| div().px(px(16.)).pb(px(6.)).text_size(px(12.)).text_color(TEXT_3).child(n.to_string()));
        let (graph, seamed, viewing) = (self.graph.clone(), self.inputs.seamed, self.inputs.viewing.clone());
        panel
            .child(self.changes_header(&repo, cx))
            .child(self.scope_bar(&scope, against.as_deref(), cx))
            .children(notice)
            .children((!scope.compare).then(|| self.commit_box(&repo, cx)))
            .child(with_graph(&graph, changes, self.inputs.cwd.clone(), seamed, viewing, cx))
    }
}

impl Changes {
    /// A Compare's files, once read; reading them here when the worktree or ref changed since and no read is under way.
    fn compare_list(&mut self, cwd: &str, against: Option<&str>, cx: &mut Context<Self>) -> Div {
        let Some(against) = against else {
            return div().flex_1().child(empty("No base to compare with."));
        };
        if self.view.needs_read(cwd, against) {
            self.load_compare(cx);
        }
        let Some(shown) = self.view.get(cwd, against) else {
            return div().flex_1().child(empty("Comparing…"));
        };
        match &shown.files {
            None => div().flex_1().child(empty(format!("Can't find {against}."))),
            Some(files) if files.is_empty() => div().flex_1().child(empty(format!("No differences with {against}."))),
            Some(files) => {
                let rows = compare_rows(files, self.inputs.tree, &self.folded);
                self.rows_list(rows, cx)
            }
        }
    }

    fn rows_list(&self, rows: Vec<Row>, cx: &mut Context<Self>) -> Div {
        let list = uniform_list(
            "changes",
            rows.len(),
            cx.processor(move |this, range: Range<usize>, _, cx| {
                rows[range]
                    .iter()
                    .map(|row| match row {
                        Row::Header(s, paths) => this.section(*s, paths, cx).into_any_element(),
                        Row::Item(s, item) => this.tree_item(*s, item, cx),
                    })
                    .collect::<Vec<_>>()
            }),
        )
        .track_scroll(&self.scroll)
        .size_full()
        .px(px(8.))
        .pt(px(6.))
        .pb(px(8.));
        div().relative().flex_1().min_h_0().child(list).vertical_scrollbar(&self.scroll)
    }

    fn stage(&mut self, paths: Vec<String>, staged: bool, cx: &mut Context<Self>) {
        cx.emit(ChangesEvent::Stage { paths, staged });
    }

    fn ask_discard(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        cx.emit(ChangesEvent::Discard(paths));
    }

    fn commit(&mut self, kind: CommitKind, window: &mut Window, cx: &mut Context<Self>) {
        self.commit_menu = false;
        let (Some(cwd), Some(repo)) = (self.inputs.cwd.clone(), self.inputs.repo.clone()) else { return };
        let message = self.input.read(cx).value().trim().to_string();
        let (all, busy) = match commit_step(&repo.files, kind, &message, self.busy.is_some(), self.inputs.git.commit_all) {
            CommitStep::Skip => return cx.notify(),
            CommitStep::StageFirst => {
                self.error = Some("Stage the changes to commit first.".into());
                return cx.notify();
            }
            CommitStep::AskMessage => {
                self.input.update(cx, |s, cx| s.focus(window, cx));
                return cx.notify();
            }
            CommitStep::Run { stage, busy } => (stage, busy),
        };
        let amend = kind == CommitKind::Amend;
        let (push_argv, untracked, tree) = (self.push_argv(), self.inputs.git.untracked, cwd.clone());
        self.busy = Some(busy);
        self.error = None;
        let task = cx.background_executor().spawn(async move {
            let runner = hosts::runner_for(&cwd);
            if !all.is_empty() {
                git::set_staged(&runner, &cwd, &all, true);
            }
            let committed = crate::util::run_login(&git::commit_argv(amend, &message), &cwd, &message).map(drop);
            let pushed = if committed.is_ok() && matches!(kind, CommitKind::Push | CommitKind::Ship) { push(&cwd, &push_argv) } else { Ok(()) };
            // The composer titles the PR from the commits, the one just made among them.
            let shipped = (kind == CommitKind::Ship && committed.is_ok() && pushed.is_ok()).then(|| git::read(&runner, &cwd, untracked)).flatten();
            (committed, pushed, shipped)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (committed, pushed, shipped) = task.await;
            this.update_in(cx, |c, window, cx| {
                c.busy = None;
                if committed.is_ok() {
                    c.input.update(cx, |s, cx| s.set_value("", window, cx));
                }
                c.error = committed.err().or(pushed.err());
                cx.emit(match shipped {
                    Some(repo) => ChangesEvent::Shipped(tree, repo),
                    None => ChangesEvent::Refresh,
                });
                cx.notify();
            })
        })
        .detach();
        cx.notify();
    }

    fn push(&mut self, cx: &mut Context<Self>) {
        self.menu = false;
        let Some(cwd) = self.inputs.cwd.clone().filter(|_| self.busy.is_none()) else { return cx.notify() };
        let argv = self.push_argv();
        self.busy = Some("Pushing…");
        self.error = None;
        let task = cx.background_executor().spawn(async move { push(&cwd, &argv) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |c, cx| {
                c.busy = None;
                c.error = res.err();
                cx.emit(ChangesEvent::Refresh);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn push_argv(&self) -> Vec<String> {
        push_argv(self.inputs.repo.as_deref(), &self.inputs.git)
    }

    /// Asks the agent picked in Settings for a message from what the commit would hold.
    fn write_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(cwd), Some(repo)) = (self.inputs.cwd.clone(), self.inputs.repo.clone()) else { return };
        if self.writing || repo.files.is_empty() {
            return;
        }
        let staged = repo.files.iter().any(|f| f.staged);
        let (argv, untracked) = (self.inputs.message_argv.clone(), self.inputs.git.untracked);
        self.writing = true;
        self.error = None;
        let task = cx.background_executor().spawn(async move {
            let context = git::commit_context(&hosts::runner_for(&cwd), &cwd, staged, untracked);
            crate::util::run_login(&argv.iter().map(String::as_str).collect::<Vec<_>>(), &cwd, &context)
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |c, window, cx| {
                c.writing = false;
                match res {
                    Ok(text) => c.input.update(cx, |s, cx| s.set_value(text, window, cx)),
                    Err(e) => c.error = Some(format!("Couldn't write a message: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}

/// Pushes to the remote picked in Settings when the repo has it.
pub(crate) fn push_argv(repo: Option<&Repo>, git: &store::prefs::Git) -> Vec<String> {
    git::push_argv(repo.filter(|r| r.remotes.contains(&git.remote)).map(|_| git.remote.as_str()), git.force_with_lease)
}

impl Desktop {
    fn changes_inputs(&self, cx: &App) -> Inputs {
        let cwd = self.cwd();
        let repo = self.repo_handle();
        let scope = cwd.as_deref().map(|c| self.scope(c)).unwrap_or_default();
        let shown = self.diff.view(self.focused_pane(), cx).and_then(|v| Some((v.file.clone()?, v.at.clone())));
        let pr = cwd.as_deref().and_then(|tree| {
            let pr = self.prs.get(tree)?;
            Some((pr.clone(), self.pr_verdict(tree, pr).0))
        });
        let offer = cwd.as_deref().zip(repo.as_deref()).and_then(|(tree, r)| self.prs.item(tree, &r.branch, r.base.as_deref())).map(Offer::of);
        let branch_was = cwd
            .as_deref()
            .zip(repo.as_deref())
            .and_then(|(tree, r)| self.renames.was(tree, Slot::Branch, &r.branch, Instant::now()).map(|(from, at)| (from.to_string(), at)));
        Inputs {
            composing: cwd.is_some() && self.pr.composing == cwd,
            cwd,
            repo,
            scope,
            tree: self.store.diff.tree,
            git: self.store.git.clone(),
            message_argv: self.store.git.message_argv(&self.store.agents.command(&self.store.git.agent)),
            shown,
            pr,
            offer,
            ship_menu: self.pr.ship_menu,
            branch_was,
            seamed: self.overlay.is_none(),
            viewing: self.viewing_commit_file(cx),
        }
    }

    /// Hands the panel what it reads of `Desktop`; before it draws and before anything calls into it.
    pub(crate) fn sync_changes(&mut self, cx: &mut Context<Self>) {
        let inputs = self.changes_inputs(cx);
        self.changes.update(cx, |c, _| c.inputs = inputs);
    }

    pub fn changes_list(&mut self, cx: &mut Context<Self>) -> Entity<Changes> {
        self.sync_changes(cx);
        self.changes.clone()
    }

    pub(crate) fn load_compare(&mut self, cx: &mut Context<Self>) {
        self.sync_changes(cx);
        self.changes.update(cx, |c, cx| c.load_compare(cx));
    }

    /// Flips the rows at once: `git add -A` and `git reset` leave each path wholly staged or wholly unstaged.
    fn stage(&mut self, paths: Vec<String>, staged: bool, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        if let Some(repo) = self.repos.get_mut(&cwd) {
            flip_staged(&mut Arc::make_mut(repo).files, &paths, staged);
        }
        // Drops refreshes already running: they read the index from before this change.
        self.git_run += 1;
        cx.notify();
        let task = cx.background_executor().spawn(async move { git::set_staged(&hosts::runner_for(&cwd), &cwd, &paths, staged) });
        cx.spawn(async move |this, cx| {
            task.await;
            this.update(cx, |d, cx| d.refresh_git(cx)).ok();
        })
        .detach();
    }

    fn ask_discard(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        self.ask(Confirm::Discard(paths), cx);
        cx.notify();
    }

    pub fn discard(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        let task = cx.background_executor().spawn(async move { git::discard(&hosts::runner_for(&cwd), &cwd, &paths) });
        cx.spawn(async move |this, cx| {
            task.await;
            this.update(cx, |d, cx| d.refresh_git(cx)).ok();
        })
        .detach();
    }

    /// Pushes to the remote picked in Settings when this repo has it.
    pub(crate) fn push_argv(&self) -> Vec<String> {
        push_argv(self.repo(), &self.store.git)
    }

    /// Opens a pull request where Settings say: a browser tab, or the system browser.
    pub(crate) fn open_pr(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.git.in_app && self.cwd().is_some() {
            self.open_browser(Some(url), window, cx);
        } else {
            cx.open_url(&url);
        }
    }

    pub(crate) fn on_changes_event(&mut self, _: &Entity<Changes>, ev: &ChangesEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            ChangesEvent::Stage { paths, staged } => self.stage(paths.clone(), *staged, cx),
            ChangesEvent::Compared { cwd, against, read } => {
                if self.cwd().as_ref() != Some(cwd) {
                    return;
                }
                self.sync_changes(cx);
                self.changes.update(cx, |c, cx| c.land_compare(cwd.clone(), against.clone(), read.clone(), cx));
            }
            ChangesEvent::Discard(paths) => self.ask_discard(paths.clone(), cx),
            ChangesEvent::Open { path, pin } => self.open_changes(Some(path.clone()), *pin, cx),
            ChangesEvent::OpenDoc(doc, pin) => self.open_doc(doc.clone(), *pin, cx),
            ChangesEvent::Scope(cwd, scope) => self.set_scope(cwd.clone(), scope.clone(), cx),
            ChangesEvent::ToggleTree => {
                self.store.diff.tree = !self.store.diff.tree;
                self.save_soon(cx);
                cx.notify();
            }
            ChangesEvent::ToggleDraft => {
                self.store.git.draft = !self.store.git.draft;
                self.save_soon(cx);
                cx.notify();
            }
            ChangesEvent::ShowPr => self.show_pr_side(cx),
            ChangesEvent::OpenPr(url) => self.open_pr(url.clone(), window, cx),
            ChangesEvent::CreatePr => self.open_pr_composer(window, cx),
            ChangesEvent::ShipMenu(open) => {
                self.pr.ship_menu = *open;
                cx.notify();
            }
            ChangesEvent::Refresh => {
                self.refresh_git(cx);
                cx.notify();
            }
            ChangesEvent::Shipped(tree, repo) => {
                let shown = self.cwd().as_ref() == Some(tree);
                self.repos.insert(tree.clone(), Arc::new(repo.clone()));
                if shown {
                    self.open_pr_composer(window, cx);
                }
                self.refresh_git(cx);
                cx.notify();
            }
        }
    }
}

pub(crate) fn push(cwd: &str, argv: &[String]) -> Result<(), String> {
    crate::util::run_login(&argv.iter().map(String::as_str).collect::<Vec<_>>(), cwd, "").map(drop)
}

#[cfg(test)]
mod tests {
    use super::{
        CommitKind, CommitStep, Item, Row, Section, change_rows, commit_label, commit_ready, commit_step, compare_rows, flip_staged, renamed_from, toggle_fold,
        tree,
    };
    use git::FileStat;
    use std::collections::HashSet;

    fn file(path: &str) -> FileStat {
        FileStat { path: path.into(), added: 0, removed: 0, staged: false, unstaged: true, status: 'M', old_path: None }
    }

    fn staged(path: &str) -> FileStat {
        FileStat { staged: true, unstaged: false, ..file(path) }
    }

    fn folds(folded: &[&str]) -> HashSet<String> {
        folded.iter().map(|k| k.to_string()).collect()
    }

    fn rows(files: &[FileStat], tree: bool, folded: &[&str]) -> Vec<String> {
        texts(change_rows(files, tree, &folds(folded)))
    }

    fn texts(rows: Vec<Row>) -> Vec<String> {
        rows.into_iter()
            .map(|row| match row {
                Row::Header(s, paths) => format!("{}: {}", s.key(), paths.join(" ")),
                Row::Item(s, Item::Dir { label, depth, .. }) => format!("{}{}:{label}/", "  ".repeat(depth + 1), s.key()),
                Row::Item(s, Item::File { file, depth }) => format!("{}{}:{}", "  ".repeat(depth + 1), s.key(), file.path),
            })
            .collect()
    }

    #[test]
    fn a_compare_lists_its_files_as_a_tree_under_one_header_that_folds() {
        let files = [file("src/a.rs"), file("b.rs")];
        assert_eq!(texts(compare_rows(&files, true, &folds(&[]))), ["compared: src/a.rs b.rs", "  compared:src/", "    compared:src/a.rs", "  compared:b.rs"]);
        assert_eq!(texts(compare_rows(&files, true, &folds(&["compared"]))), ["compared: src/a.rs b.rs"]);
    }

    #[test]
    fn a_renamed_file_shows_its_old_name_or_its_old_path_when_it_moved_folders() {
        assert_eq!(renamed_from("src/old.rs", "src/new.rs"), "old.rs");
        assert_eq!(renamed_from("old.rs", "new.rs"), "old.rs");
        assert_eq!(renamed_from("lib/a.rs", "src/a.rs"), "lib/a.rs");
        assert_eq!(renamed_from("a.rs", "src/a.rs"), "a.rs");
    }

    #[test]
    fn change_rows_list_staged_files_then_the_rest_each_under_its_header() {
        let untracked = FileStat { status: 'A', ..file("new.rs") };
        let both = FileStat { staged: true, ..file("both.rs") };
        let files = [file("a.rs"), staged("b.rs"), both, untracked];
        assert_eq!(
            rows(&files, false, &[]),
            [
                "staged: b.rs both.rs",
                "  staged:b.rs",
                "  staged:both.rs",
                "changes: a.rs both.rs new.rs",
                "  changes:a.rs",
                "  changes:both.rs",
                "  changes:new.rs"
            ]
        );
    }

    #[test]
    fn change_rows_skip_empty_sections_and_keep_a_folded_sections_header_with_all_its_paths() {
        let files = [file("a.rs"), file("src/b.rs")];
        assert_eq!(rows(&files, false, &["changes"]), ["changes: a.rs src/b.rs"]);
        assert_eq!(rows(&files, true, &["changes"]), ["changes: a.rs src/b.rs"]);
    }

    #[test]
    fn change_rows_fold_a_folder_only_in_its_own_section() {
        let files = [file("src/a.rs"), staged("src/b.rs")];
        assert_eq!(rows(&files, true, &["changes:src/"]), ["staged: src/b.rs", "  staged:src/", "    staged:src/b.rs", "changes: src/a.rs", "  changes:src/"]);
    }

    #[test]
    fn toggling_a_fold_twice_opens_it_again() {
        let mut folded = HashSet::from(["staged".to_string()]);
        toggle_fold(&mut folded, "changes:src/");
        assert_eq!(folded, HashSet::from(["staged".to_string(), "changes:src/".to_string()]));
        toggle_fold(&mut folded, "changes:src/");
        assert_eq!(folded, HashSet::from(["staged".to_string()]));
    }

    #[test]
    fn nothing_commits_while_git_works() {
        for kind in [CommitKind::Commit, CommitKind::Push, CommitKind::Amend, CommitKind::Ship] {
            assert_eq!(commit_step(&[staged("a.rs")], kind, "Edit a", true, true), CommitStep::Skip);
        }
    }

    #[test]
    fn with_nothing_changed_only_amend_runs() {
        assert_eq!(commit_step(&[], CommitKind::Commit, "Edit a", false, true), CommitStep::Skip);
        assert_eq!(commit_step(&[], CommitKind::Push, "Edit a", false, true), CommitStep::Skip);
        assert_eq!(commit_step(&[], CommitKind::Amend, "", false, true), CommitStep::Run { stage: vec![], busy: "Amending…" });
    }

    #[test]
    fn a_commit_without_a_message_asks_for_one_but_amend_keeps_the_last() {
        let files = [staged("a.rs")];
        assert_eq!(commit_step(&files, CommitKind::Commit, "", false, true), CommitStep::AskMessage);
        assert_eq!(commit_step(&files, CommitKind::Push, "", false, true), CommitStep::AskMessage);
        assert_eq!(commit_step(&files, CommitKind::Amend, "", false, true), CommitStep::Run { stage: vec![], busy: "Amending…" });
    }

    #[test]
    fn a_commit_takes_every_change_when_none_is_staged_and_only_the_staged_otherwise() {
        let loose = [file("a.rs"), FileStat { status: 'A', ..file("new.rs") }];
        assert_eq!(
            commit_step(&loose, CommitKind::Commit, "Edit", false, true),
            CommitStep::Run { stage: vec!["a.rs".into(), "new.rs".into()], busy: "Committing…" }
        );
        assert_eq!(
            commit_step(&loose, CommitKind::Push, "Edit", false, true),
            CommitStep::Run { stage: vec!["a.rs".into(), "new.rs".into()], busy: "Committing…" }
        );
        assert_eq!(commit_step(&loose, CommitKind::Amend, "Edit", false, true), CommitStep::Run { stage: vec![], busy: "Amending…" });
        let mixed = [file("a.rs"), staged("b.rs")];
        assert_eq!(commit_step(&mixed, CommitKind::Commit, "Edit", false, true), CommitStep::Run { stage: vec![], busy: "Committing…" });
    }

    #[test]
    fn the_commit_button_says_commit_all_when_nothing_is_staged_and_what_git_does_while_busy() {
        assert_eq!(commit_label(&[file("a.rs")], None, true), "Commit All");
        assert_eq!(commit_label(&[file("a.rs"), staged("b.rs")], None, true), "Commit");
        assert_eq!(commit_label(&[], None, true), "Commit");
        assert_eq!(commit_label(&[file("a.rs")], Some("Pushing…"), true), "Pushing…");
    }

    #[test]
    fn commit_is_ready_with_changes_a_message_and_git_idle() {
        let files = [file("a.rs")];
        assert!(commit_ready(&files, "Edit a", false, true));
        assert!(!commit_ready(&files, " \n", false, true));
        assert!(!commit_ready(&[], "Edit a", false, true));
        assert!(!commit_ready(&files, "Edit a", true, true));
    }

    #[test]
    fn staged_only_asks_to_stage_first_instead_of_committing_everything() {
        let loose = [file("a.rs")];
        assert_eq!(commit_step(&loose, CommitKind::Commit, "Edit", false, false), CommitStep::StageFirst);
        assert_eq!(commit_step(&loose, CommitKind::Amend, "", false, false), CommitStep::Run { stage: vec![], busy: "Amending…" });
        assert_eq!(commit_step(&[staged("a.rs")], CommitKind::Commit, "Edit", false, false), CommitStep::Run { stage: vec![], busy: "Committing…" });
        assert_eq!(commit_label(&loose, None, false), "Commit");
        assert!(!commit_ready(&loose, "Edit", false, false));
        assert!(commit_ready(&[staged("a.rs")], "Edit", false, false));
    }

    #[test]
    fn staging_leaves_each_picked_file_wholly_staged_or_wholly_unstaged() {
        let both = FileStat { staged: true, ..file("both.rs") };
        let mut files = [file("a.rs"), both.clone(), file("c.rs")];
        flip_staged(&mut files, &["a.rs".into(), "both.rs".into()], true);
        assert_eq!(files, [staged("a.rs"), staged("both.rs"), file("c.rs")]);
        let mut files = [staged("a.rs"), both];
        flip_staged(&mut files, &["a.rs".into(), "both.rs".into()], false);
        assert_eq!(files, [file("a.rs"), file("both.rs")]);
    }

    fn labels(files: &[FileStat], folded: &HashSet<String>) -> Vec<String> {
        let refs: Vec<&FileStat> = files.iter().collect();
        let mut out = Vec::new();
        tree(&refs, "", 0, Section::Changes, folded, &mut out);
        out.into_iter()
            .map(|i| match i {
                Item::Dir { label, depth, .. } => format!("{}{label}/", "  ".repeat(depth)),
                Item::File { file, depth } => format!("{}{}", "  ".repeat(depth), file.path.rsplit('/').next().unwrap()),
            })
            .collect()
    }

    #[test]
    fn tree_lists_folders_first_and_merges_single_folder_chains() {
        let files = [file("README.md"), file("src/hooks/use-a.ts"), file("src/hooks/use-b.ts"), file("src/app/main.ts"), file("docs/x/y/z.md")];
        assert_eq!(
            labels(&files, &HashSet::new()),
            ["docs/x/y/", "  z.md", "src/", "  app/", "    main.ts", "  hooks/", "    use-a.ts", "    use-b.ts", "README.md"]
        );
    }

    #[test]
    fn tree_hides_what_a_folded_folder_holds() {
        let files = [file("src/a.rs"), file("src/b/c.rs"), file("d.rs")];
        assert_eq!(labels(&files, &HashSet::from(["changes:src/".to_string()])), ["src/", "d.rs"]);
    }
}
