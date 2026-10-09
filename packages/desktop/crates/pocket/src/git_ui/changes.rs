mod commit_box;
mod header;
mod rows;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, empty};
use git::FileStat;
use git::github;
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::*;
use std::collections::{BTreeMap, HashSet};
use std::ops::Range;

#[derive(Clone, Copy, PartialEq)]
pub enum CommitKind {
    Commit,
    Push,
    Amend,
}

#[derive(Debug, PartialEq)]
enum CommitStep {
    Skip,
    AskMessage,
    /// Nothing is staged and the settings say to commit only what is.
    StageFirst,
    /// `stage` holds every changed path when none is staged, so the commit takes them all.
    Run { stage: Vec<String>, busy: &'static str },
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

#[derive(Clone, Copy, PartialEq)]
enum Section {
    Staged,
    Changes,
}

impl Section {
    fn key(self) -> &'static str {
        match self {
            Section::Staged => "staged",
            Section::Changes => "changes",
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
        if files.is_empty() {
            continue;
        }
        rows.push(Row::Header(s, files.iter().map(|f| f.path.clone()).collect()));
        if folded.contains(s.key()) {
            continue;
        }
        let mut items = Vec::new();
        if as_tree {
            tree(&files, "", 0, s, folded, &mut items);
        } else {
            items.extend(files.iter().map(|&f| Item::File { file: f.clone(), depth: 0 }));
        }
        rows.extend(items.into_iter().map(|item| Row::Item(s, item)));
    }
    rows
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

pub struct ChangesState {
    pub(crate) input: Entity<TextareaState>,
    /// What the commit button says while git works.
    pub(crate) busy: Option<&'static str>,
    pub(crate) writing: bool,
    pub(crate) error: Option<String>,
    pub(crate) commit_menu: bool,
    pub(crate) menu: bool,
    pub(crate) folded: HashSet<String>,
    pub(crate) scroll: UniformListScrollHandle,
}

impl ChangesState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Message (⌘↩ to commit)").auto_grow(1, 8));
        let subs = vec![cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { secondary: true, .. } => this.commit(CommitKind::Commit, window, cx),
            InputEvent::Change => cx.notify(),
            _ => {}
        })];
        let state = Self {
            input,
            busy: None,
            writing: false,
            error: None,
            commit_menu: false,
            menu: false,
            folded: HashSet::new(),
            scroll: UniformListScrollHandle::new(),
        };
        (state, subs)
    }
}

impl Desktop {
    pub fn changes_list(&mut self, cx: &mut Context<Self>) -> Div {
        let panel = div().flex_1().min_h_0().flex().flex_col();
        let Some(repo) = self.repo().cloned() else {
            return panel.child(empty("Not a git repository."));
        };
        let rows = change_rows(&repo.files, self.store.diff.tree, &self.changes.folded);
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
        .track_scroll(&self.changes.scroll)
        .size_full()
        .px(px(8.))
        .pt(px(6.))
        .pb(px(8.));
        let list = div().relative().flex_1().min_h_0().child(list).vertical_scrollbar(&self.changes.scroll);
        let changes = if repo.files.is_empty() { div().flex_1().child(empty("No changes.")) } else { list };
        panel
            .child(self.changes_header(&repo, cx))
            .child(self.commit_box(&repo, cx))
            .child(self.with_graph(changes, cx))
    }

    /// Flips the rows at once: `git add -A` and `git reset` leave each path wholly staged or wholly unstaged.
    pub fn stage(&mut self, paths: Vec<String>, staged: bool, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        if let Some(repo) = self.repos.get_mut(&cwd) {
            flip_staged(&mut repo.files, &paths, staged);
        }
        // Drops refreshes already running: they read the index from before this change.
        self.git_run += 1;
        cx.notify();
        let task = cx.background_executor().spawn(async move { git::set_staged(&cwd, &paths, staged) });
        cx.spawn(async move |this, cx| {
            task.await;
            this.update(cx, |d, cx| d.refresh_git(cx)).ok();
        })
        .detach();
    }

    fn ask_discard(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        self.confirm = Some(Confirm::Discard(paths));
        self.overlay = Some(Overlay::Confirm);
        cx.notify();
    }

    pub fn discard(&mut self, paths: Vec<String>, cx: &mut Context<Self>) {
        let Some(cwd) = self.cwd() else { return };
        let task = cx.background_executor().spawn(async move { git::discard(&cwd, &paths) });
        cx.spawn(async move |this, cx| {
            task.await;
            this.update(cx, |d, cx| d.refresh_git(cx)).ok();
        })
        .detach();
    }

    pub fn commit(&mut self, kind: CommitKind, window: &mut Window, cx: &mut Context<Self>) {
        self.changes.commit_menu = false;
        let (Some(cwd), Some(repo)) = (self.cwd(), self.repo()) else { return };
        let message = self.changes.input.read(cx).value().trim().to_string();
        let (all, busy) = match commit_step(&repo.files, kind, &message, self.changes.busy.is_some(), self.store.git.commit_all) {
            CommitStep::Skip => return cx.notify(),
            CommitStep::StageFirst => {
                self.changes.error = Some("Stage the changes to commit first.".into());
                return cx.notify();
            }
            CommitStep::AskMessage => {
                self.changes.input.update(cx, |s, cx| s.focus(window, cx));
                return cx.notify();
            }
            CommitStep::Run { stage, busy } => (stage, busy),
        };
        let amend = kind == CommitKind::Amend;
        let push_argv = self.push_argv();
        self.changes.busy = Some(busy);
        self.changes.error = None;
        let task = cx.background_executor().spawn(async move {
            if !all.is_empty() {
                git::set_staged(&cwd, &all, true);
            }
            let committed = daemon::run_login(&git::commit_argv(amend, &message), &cwd, &message).map(drop);
            let pushed = if committed.is_ok() && kind == CommitKind::Push { push(&cwd, &push_argv) } else { Ok(()) };
            (committed, pushed)
        });
        cx.spawn_in(window, async move |this, cx| {
            let (committed, pushed) = task.await;
            this.update_in(cx, |d, window, cx| {
                d.changes.busy = None;
                if committed.is_ok() {
                    d.changes.input.update(cx, |s, cx| s.set_value("", window, cx));
                }
                d.changes.error = committed.err().or(pushed.err());
                d.refresh_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    fn push(&mut self, cx: &mut Context<Self>) {
        self.changes.menu = false;
        let Some(cwd) = self.cwd().filter(|_| self.changes.busy.is_none()) else { return cx.notify() };
        let argv = self.push_argv();
        self.changes.busy = Some("Pushing…");
        self.changes.error = None;
        let task = cx.background_executor().spawn(async move { push(&cwd, &argv) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                d.changes.busy = None;
                d.changes.error = res.err();
                d.refresh_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Pushes to the remote picked in Settings when this repo has it.
    fn push_argv(&self) -> Vec<String> {
        let remote = &self.store.git.remote;
        git::push_argv(self.repo().filter(|r| r.remotes.contains(remote)).map(|_| remote.as_str()), self.store.git.force_with_lease)
    }

    /// Opens a pull request where Settings say: a browser tab, or the system browser.
    pub(crate) fn open_pr(&mut self, url: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.store.git.in_app && self.cwd().is_some() {
            self.open_browser(Some(url), window, cx);
        } else {
            cx.open_url(&url);
        }
    }

    fn create_pr(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.changes.menu = false;
        let (Some(tree), Some(base)) = (self.cwd(), self.repo().and_then(|r| r.base.clone())) else { return cx.notify() };
        if self.changes.busy.is_some() {
            return cx.notify();
        }
        self.changes.busy = Some("Creating PR…");
        self.changes.error = None;
        let (dir, push_argv, draft) = (tree.clone(), self.push_argv(), self.store.git.draft);
        let task = cx.background_executor().spawn(async move {
            push(&dir, &push_argv)?;
            daemon::run_login(&github::create_argv(&base, draft), &dir, "")
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |d, window, cx| {
                d.changes.busy = None;
                match res {
                    Ok(out) => {
                        if let Some(url) = out.lines().last() {
                            d.open_pr(url.to_string(), window, cx);
                        }
                        d.prs.forget(&tree);
                    }
                    Err(e) => d.changes.error = Some(e),
                }
                d.refresh_git(cx);
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }

    /// Asks the agent picked in Settings for a message from what the commit would hold.
    fn write_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(cwd), Some(repo)) = (self.cwd(), self.repo()) else { return };
        if self.changes.writing || repo.files.is_empty() {
            return;
        }
        let staged = repo.files.iter().any(|f| f.staged);
        let (argv, untracked) = (self.store.git.message_argv(&self.store.agents.command(&self.store.git.agent)), self.store.git.untracked);
        self.changes.writing = true;
        self.changes.error = None;
        let task = cx.background_executor().spawn(async move {
            let context = git::commit_context(&cwd, staged, untracked);
            daemon::run_login(&argv.iter().map(String::as_str).collect::<Vec<_>>(), &cwd, &context)
        });
        cx.spawn_in(window, async move |this, cx| {
            let res = task.await;
            this.update_in(cx, |d, window, cx| {
                d.changes.writing = false;
                match res {
                    Ok(text) => d.changes.input.update(cx, |s, cx| s.set_value(text, window, cx)),
                    Err(e) => d.changes.error = Some(format!("Couldn't write a message: {e}")),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
        cx.notify();
    }
}

fn push(cwd: &str, argv: &[String]) -> Result<(), String> {
    daemon::run_login(&argv.iter().map(String::as_str).collect::<Vec<_>>(), cwd, "").map(drop)
}

#[cfg(test)]
mod tests {
    use super::{CommitKind, CommitStep, Item, Row, Section, change_rows, commit_label, commit_ready, commit_step, flip_staged, toggle_fold, tree};
    use git::FileStat;
    use std::collections::HashSet;

    fn file(path: &str) -> FileStat {
        FileStat { path: path.into(), added: 0, removed: 0, staged: false, unstaged: true, status: 'M' }
    }

    fn staged(path: &str) -> FileStat {
        FileStat { staged: true, unstaged: false, ..file(path) }
    }

    fn rows(files: &[FileStat], tree: bool, folded: &[&str]) -> Vec<String> {
        let folded: HashSet<String> = folded.iter().map(|k| k.to_string()).collect();
        change_rows(files, tree, &folded)
            .into_iter()
            .map(|row| match row {
                Row::Header(s, paths) => format!("{}: {}", s.key(), paths.join(" ")),
                Row::Item(s, Item::Dir { label, depth, .. }) => format!("{}{}:{label}/", "  ".repeat(depth + 1), s.key()),
                Row::Item(s, Item::File { file, depth }) => format!("{}{}:{}", "  ".repeat(depth + 1), s.key(), file.path),
            })
            .collect()
    }

    #[test]
    fn change_rows_list_staged_files_then_the_rest_each_under_its_header() {
        let untracked = FileStat { status: 'A', ..file("new.rs") };
        let both = FileStat { staged: true, ..file("both.rs") };
        let files = [file("a.rs"), staged("b.rs"), both, untracked];
        assert_eq!(
            rows(&files, false, &[]),
            ["staged: b.rs both.rs", "  staged:b.rs", "  staged:both.rs", "changes: a.rs both.rs new.rs", "  changes:a.rs", "  changes:both.rs", "  changes:new.rs"]
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
        for kind in [CommitKind::Commit, CommitKind::Push, CommitKind::Amend] {
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
        assert_eq!(commit_step(&loose, CommitKind::Commit, "Edit", false, true), CommitStep::Run { stage: vec!["a.rs".into(), "new.rs".into()], busy: "Committing…" });
        assert_eq!(commit_step(&loose, CommitKind::Push, "Edit", false, true), CommitStep::Run { stage: vec!["a.rs".into(), "new.rs".into()], busy: "Committing…" });
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
        assert_eq!(labels(&files, &HashSet::new()), ["docs/x/y/", "  z.md", "src/", "  app/", "    main.ts", "  hooks/", "    use-a.ts", "    use-b.ts", "README.md"]);
    }

    #[test]
    fn tree_hides_what_a_folded_folder_holds() {
        let files = [file("src/a.rs"), file("src/b/c.rs"), file("d.rs")];
        assert_eq!(labels(&files, &HashSet::from(["changes:src/".to_string()])), ["src/", "d.rs"]);
    }
}
