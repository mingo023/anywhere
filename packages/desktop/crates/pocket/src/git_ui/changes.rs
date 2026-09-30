use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, empty};
use crate::git_ui::diff::{line_label, ordered, span};
use git::{FileStat, Repo};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{BTreeMap, HashSet};
use std::ops::Range;
use theme::*;
use ui::icon_button_sized;

const ROW_GROUP: &str = "change-row";
const SECTION_GROUP: &str = "change-section";
const PROMPT: &str = "Write a git commit message for the diff on stdin, in the style of the recent subjects. Reply with the message only.";

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
    /// `stage` holds every changed path when none is staged, so the commit takes them all.
    Run { stage: Vec<String>, busy: &'static str },
}

/// Commits the staged files, or every change when none is staged. Amending keeps the last message unless a new one is typed.
fn commit_step(files: &[FileStat], kind: CommitKind, message: &str, busy: bool) -> CommitStep {
    let amend = kind == CommitKind::Amend;
    if busy || (!amend && files.is_empty()) {
        return CommitStep::Skip;
    }
    if !amend && message.is_empty() {
        return CommitStep::AskMessage;
    }
    let stage = if amend || files.iter().any(|f| f.staged) { Vec::new() } else { files.iter().map(|f| f.path.clone()).collect() };
    CommitStep::Run { stage, busy: if amend { "Amending…" } else { "Committing…" } }
}

fn commit_label(files: &[FileStat], busy: Option<&'static str>) -> &'static str {
    match busy {
        Some(busy) => busy,
        None if !files.is_empty() && !files.iter().any(|f| f.staged) => "Commit All",
        None => "Commit",
    }
}

fn commit_ready(files: &[FileStat], message: &str, busy: bool) -> bool {
    !files.is_empty() && !message.trim().is_empty() && !busy
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

fn action(id: impl Into<ElementId>, name: &str) -> Stateful<Div> {
    icon_button_sized(id, name, 22., TEXT_2).rounded(px(6.))
}

fn count_pill(n: usize) -> Div {
    div()
        .min_w(px(18.))
        .h(px(18.))
        .px(px(5.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(9.))
        .bg(rgba(FILL_3))
        .text_size(px(11.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(rgba(TEXT_2))
        .child(n.to_string())
}

fn row(id: impl Into<ElementId>, depth: usize) -> Stateful<Div> {
    div().id(id).w_full().h(px(28.)).pl(px(8. + depth as f32 * 14.)).pr(px(6.)).flex().flex_none().items_center().gap(px(6.)).rounded(px(8.)).cursor_pointer()
}

pub struct ChangesState {
    pub(crate) input: Entity<TextareaState>,
    /// What the commit button says while git works.
    pub(crate) busy: Option<&'static str>,
    pub(crate) writing: bool,
    pub(crate) error: Option<String>,
    pub(crate) commit_menu: bool,
    pub(crate) menu: bool,
    pub(crate) tree: bool,
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
            tree: false,
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
        let rows = change_rows(&repo.files, self.changes.tree, &self.changes.folded);
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
        let notes = self.notes(&repo, cx);
        panel
            .child(self.changes_header(&repo, cx))
            .child(self.commit_box(&repo, cx))
            .map(|d| if repo.files.is_empty() { d.child(empty("No changes.")) } else { d.child(list) })
            .when(!notes.is_empty(), |d| d.child(div().id("change-notes").flex_none().max_h(px(240.)).overflow_y_scroll().px(px(8.)).pb(px(8.)).flex().flex_col().children(notes)))
    }

    fn changes_header(&self, repo: &Repo, cx: &mut Context<Self>) -> Div {
        let view = icon_button_sized("changes-view", if self.changes.tree { "list-flat" } else { "list-tree" }, 26., TEXT_3).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.changes.tree = !this.changes.tree;
                cx.notify();
            },
        ));
        let open = self.changes.menu;
        // Runs before the open menu's click-outside handler, which would otherwise close it only for this click to reopen it.
        let more = icon_button_sized("changes-more", "more", 26., TEXT_3).when(open, |d| d.bg(rgba(FILL_3))).capture_any_mouse_down(cx.listener(
            |this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.changes.menu = !this.changes.menu;
                this.changes.commit_menu = false;
                cx.notify();
            },
        ));
        div()
            .h(px(40.))
            .flex_none()
            .pl(px(16.))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().flex_none().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child("Changes"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_color(rgba(TEXT_3))
                    .child(icon("branch", 12., TEXT_3))
                    .child(div().truncate().font_family(MONO).text_size(px(12.)).child(repo.branch.clone())),
            )
            .child(view)
            .child(div().relative().child(more).when(open, |d| d.child(ui::dropdown(30., self.changes_menu_view(repo, cx)))))
    }

    fn changes_menu_view(&self, repo: &Repo, cx: &mut Context<Self>) -> Stateful<Div> {
        let info = |text: String| div().h(px(26.)).px(px(10.)).flex().items_center().text_size(px(12.5)).text_color(rgba(TEXT_3)).child(text);
        let base = repo.base.as_ref().map(|b| format!("{} → {b}", repo.branch));
        let counts = repo.base.is_some().then(|| format!("{} ahead · {} behind", repo.ahead, repo.behind));
        ui::pop(div().id("changes-menu"))
            .w(px(220.))
            .p(px(6.))
            .rounded(px(14.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.changes.menu = false;
                cx.notify();
            }))
            .child(ui::menu_row("changes-push", "arrow-up", "Push", None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.push(cx))))
            .when(base.is_some(), |d| d.child(ui::menu_divider()))
            .children(base.map(info))
            .children(counts.map(info))
    }

    fn commit_box(&self, repo: &Repo, cx: &mut Context<Self>) -> Div {
        let has_changes = !repo.files.is_empty();
        let write = div()
            .id("commit-write")
            .size(px(24.))
            .mt(px(5.))
            .mr(px(5.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .map(|d| if self.changes.writing { d.child(spinner("commit-writing", 13., TEXT_3)) } else { d.child(icon("sparkle", 14., TEXT_3)) })
            .when(has_changes && !self.changes.writing, |d| {
                d.cursor_pointer().hover(|s| s.bg(rgba(FILL_3))).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.write_message(window, cx)))
            })
            .when(!has_changes, |d| d.opacity(0.4));
        let field = div()
            .flex()
            .items_start()
            .rounded(px(10.))
            .bg(rgba(SURFACE))
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
            .text_size(px(13.))
            .child(div().flex_1().min_w_0().child(Textarea::new(&self.changes.input).appearance(false)))
            .child(write);
        let label = commit_label(&repo.files, self.changes.busy);
        let ready = commit_ready(&repo.files, &self.changes.input.read(cx).value(), self.changes.busy.is_some());
        let commit = div()
            .id("commit")
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .rounded_l(px(9.))
            .map(|d| if self.changes.busy.is_some() { d.child(spinner("commit-busy", 13., WHITE)) } else { d.child(icon("check", 14., WHITE)) })
            .child(label)
            .when(ready, |d| d.cursor_pointer().hover(|s| s.bg(rgba(0xffffff1a))))
            .when(!ready, |d| d.text_color(rgba(0xffffff8c)))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.commit(CommitKind::Commit, window, cx)));
        let menu_open = self.changes.commit_menu;
        let chevron = div()
            .id("commit-more")
            .w(px(30.))
            .h_full()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded_r(px(9.))
            .border_l(px(0.5))
            .border_color(rgba(0xffffff33))
            .cursor_pointer()
            .hover(|s| s.bg(rgba(0xffffff1a)))
            .when(menu_open, |d| d.bg(rgba(0xffffff1a)))
            .child(icon("chevron-down", 12., WHITE))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.changes.commit_menu = !this.changes.commit_menu;
                this.changes.menu = false;
                cx.notify();
            }));
        let menu = ui::pop(div().id("commit-menu"))
            .w(px(220.))
            .p(px(6.))
            .rounded(px(14.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.changes.commit_menu = false;
                cx.notify();
            }))
            .child(ui::menu_row("commit-push", "arrow-up", "Commit & Push", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.commit(CommitKind::Push, window, cx))))
            .child(ui::menu_row("commit-amend", "compose", "Amend Last Commit", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.commit(CommitKind::Amend, window, cx))));
        let button = ui::primary(div().relative().h(px(30.)).flex().rounded(px(9.)))
            .text_size(px(13.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgba(WHITE))
            .child(commit)
            .child(chevron)
            .when(menu_open, |d| d.child(ui::dropdown(34., menu)));
        let error = self.changes.error.clone().map(|e| {
            div()
                .id("commit-error")
                .max_h(px(120.))
                .overflow_y_scroll()
                .px(px(10.))
                .py(px(8.))
                .rounded(px(9.))
                .bg(rgba(FAILED_BG))
                .font_family(MONO)
                .text_size(px(11.5))
                .text_color(rgba(FAILED))
                .child(e)
        });
        div().flex_none().px(px(10.)).pb(px(6.)).flex().flex_col().gap(px(8.)).child(field).child(button).children(error)
    }

    fn section(&self, s: Section, paths: &[String], cx: &mut Context<Self>) -> Stateful<Div> {
        let key = s.key();
        let open = !self.changes.folded.contains(key);
        let count = paths.len();
        let paths = paths.to_vec();
        let actions = match s {
            Section::Staged => vec![action("unstage-all", "minus").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.stage(paths.clone(), false, cx);
            }))],
            Section::Changes => {
                let discard = paths.clone();
                vec![
                    action("discard-all", "discard").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.ask_discard(discard.clone(), cx);
                    })),
                    action("stage-all", "plus").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.stage(paths.clone(), true, cx);
                    })),
                ]
            }
        };
        div()
            .id(key)
            .group(SECTION_GROUP)
            .w_full()
            .h(px(28.))
            .pl(px(6.))
            .pr(px(6.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .cursor_pointer()
            .hover(|st| st.bg(rgba(FILL_1)))
            .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(rgba(TEXT_3))
                    .child(if s == Section::Staged { "Staged Changes" } else { "Changes" }),
            )
            .child(div().flex().gap(px(2.)).opacity(0.).group_hover(SECTION_GROUP, |st| st.opacity(1.)).children(actions))
            .child(count_pill(count))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                toggle_fold(&mut this.changes.folded, key);
                cx.notify();
            }))
    }

    fn tree_item(&self, s: Section, item: &Item, cx: &mut Context<Self>) -> AnyElement {
        match item {
            Item::File { file, depth } => self.change_row(s, file, *depth, self.changes.tree, cx).into_any_element(),
            Item::Dir { key, label, depth, open } => {
                let (key, open) = (key.clone(), *open);
                row(ElementId::Name(key.clone().into()), *depth)
                    .hover(|st| st.bg(rgba(FILL_1)))
                    .text_size(px(13.5))
                    .font_weight(FontWeight(450.))
                    .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4))
                    .child(file_icon(label, true, open, 16.))
                    .child(div().flex_1().min_w_0().truncate().child(label.clone()))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        toggle_fold(&mut this.changes.folded, &key);
                        cx.notify();
                    }))
                    .into_any_element()
            }
        }
    }

    /// In the tree a file sits under its folder, so only its name shows, indented past the folders' chevrons.
    fn change_row(&self, s: Section, f: &FileStat, depth: usize, in_tree: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.diff.file.as_ref() == Some(&f.path);
        let (dir, name) = f.path.rsplit_once('/').unwrap_or(("", &f.path));
        let comments = self.diff.comments.iter().filter(|c| c.path == f.path).count();
        let id = |kind: &str| ElementId::Name(format!("{kind}:{}:{}", s.key(), f.path).into());
        let path = f.path.clone();
        let actions = match s {
            Section::Staged => vec![action(id("unstage"), "minus").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.stage(vec![path.clone()], false, cx);
            }))],
            Section::Changes => {
                let discard = path.clone();
                vec![
                    action(id("discard"), "discard").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.ask_discard(vec![discard.clone()], cx);
                    })),
                    action(id("stage"), "plus").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.stage(vec![path.clone()], true, cx);
                    })),
                ]
            }
        };
        let open = f.path.clone();
        row(id("file"), depth)
            .group(ROW_GROUP)
            .when(selected, |d| d.bg(rgba(FILL_3)))
            .when(!selected, |d| d.hover(|st| st.bg(rgba(FILL_1))))
            .when(in_tree, |d| d.child(div().w(px(12.)).flex_none()))
            .child(file_icon(name, false, false, 16.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_baseline()
                    .gap(px(6.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(
                        div()
                            .flex_none()
                            .max_w_full()
                            .truncate()
                            .text_size(px(13.5))
                            .font_weight(if selected { FontWeight::SEMIBOLD } else { FontWeight(450.) })
                            .when(f.status == 'D', |d| d.line_through())
                            .child(name.to_string()),
                    )
                    .when(!in_tree && !dir.is_empty(), |d| d.child(div().min_w_0().truncate().text_size(px(12.)).text_color(rgba(TEXT_4)).child(dir.to_string()))),
            )
            .when(comments > 0, |d| {
                d.child(
                    div()
                        .flex()
                        .flex_none()
                        .items_center()
                        .gap(px(3.))
                        .text_size(px(11.5))
                        .font_weight(FontWeight::SEMIBOLD)
                        .text_color(rgba(WAITING_TEXT))
                        .child(icon("comment", 11., WAITING_TEXT))
                        .child(comments.to_string()),
                )
            })
            .child(div().flex().flex_none().gap(px(2.)).opacity(0.).group_hover(ROW_GROUP, |st| st.opacity(1.)).children(actions))
            .child(
                div()
                    .w(px(14.))
                    .flex()
                    .flex_none()
                    .justify_center()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(rgba(ui::git_color(f.status)))
                    .child(f.status.to_string()),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_changes(Some(open.clone()), cx)))
    }

    fn notes(&self, repo: &Repo, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let note = |id: ElementId, label: String, state: ui::State, text: String, path: String| {
            div()
                .id(id.clone())
                .px(px(10.))
                .py(px(6.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .rounded(px(10.))
                .cursor_pointer()
                .hover(|s| s.bg(rgba(FILL_1)))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(8.))
                        .child(div().font_family(MONO).text_size(px(11.5)).text_color(rgba(WAITING_TEXT)).child(label))
                        .child(ui::status(id, state)),
                )
                .child(div().truncate().text_size(px(13.5)).text_color(rgba(TEXT_BODY)).child(text))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_changes(Some(path.clone()), cx)))
        };
        let mut notes: Vec<Stateful<Div>> = self
            .diff
            .comments
            .iter()
            .enumerate()
            .filter(|(_, c)| repo.files.iter().any(|f| f.path == c.path))
            .map(|(i, c)| note(ElementId::NamedInteger("note".into(), i as u64), line_label(c.lines), ui::State::Sent, c.text.clone(), c.path.clone()))
            .collect();
        let draft = self.diff.input.read(cx).value().trim().to_string();
        if let (true, false, Some(path), Some((lo, hi, _))) =
            (self.diff.pick.composing, draft.is_empty(), self.diff.file.clone(), self.diff.pick.range.and_then(|s| span(&self.diff.lines, ordered(s))))
        {
            notes.push(note("draft".into(), line_label((lo, hi)), ui::State::Draft, draft, path));
        }
        if notes.is_empty() {
            return Vec::new();
        }
        std::iter::once(ui::section_header("Comments", Some(notes.len())).into_any_element()).chain(notes.into_iter().map(IntoElement::into_any_element)).collect()
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
        let (all, busy) = match commit_step(&repo.files, kind, &message, self.changes.busy.is_some()) {
            CommitStep::Skip => return cx.notify(),
            CommitStep::AskMessage => {
                self.changes.input.update(cx, |s, cx| s.focus(window, cx));
                return cx.notify();
            }
            CommitStep::Run { stage, busy } => (stage, busy),
        };
        let amend = kind == CommitKind::Amend;
        self.changes.busy = Some(busy);
        self.changes.error = None;
        let task = cx.background_executor().spawn(async move {
            if !all.is_empty() {
                git::set_staged(&cwd, &all, true);
            }
            let committed = daemon::run_login(&git::commit_argv(amend, &message), &cwd, &message).map(drop);
            let pushed = if committed.is_ok() && kind == CommitKind::Push { push(&cwd) } else { Ok(()) };
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
        self.changes.busy = Some("Pushing…");
        self.changes.error = None;
        let task = cx.background_executor().spawn(async move { push(&cwd) });
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

    /// Asks Claude Code for a message from what the commit would hold.
    fn write_message(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (Some(cwd), Some(repo)) = (self.cwd(), self.repo()) else { return };
        if self.changes.writing || repo.files.is_empty() {
            return;
        }
        let staged = repo.files.iter().any(|f| f.staged);
        self.changes.writing = true;
        self.changes.error = None;
        let task = cx.background_executor().spawn(async move {
            let context = git::commit_context(&cwd, staged);
            daemon::run_login(&["claude", "-p", "--model", "haiku", PROMPT], &cwd, &context)
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

fn push(cwd: &str) -> Result<(), String> {
    daemon::run_login(git::PUSH, cwd, "").map(drop)
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
            assert_eq!(commit_step(&[staged("a.rs")], kind, "Edit a", true), CommitStep::Skip);
        }
    }

    #[test]
    fn with_nothing_changed_only_amend_runs() {
        assert_eq!(commit_step(&[], CommitKind::Commit, "Edit a", false), CommitStep::Skip);
        assert_eq!(commit_step(&[], CommitKind::Push, "Edit a", false), CommitStep::Skip);
        assert_eq!(commit_step(&[], CommitKind::Amend, "", false), CommitStep::Run { stage: vec![], busy: "Amending…" });
    }

    #[test]
    fn a_commit_without_a_message_asks_for_one_but_amend_keeps_the_last() {
        let files = [staged("a.rs")];
        assert_eq!(commit_step(&files, CommitKind::Commit, "", false), CommitStep::AskMessage);
        assert_eq!(commit_step(&files, CommitKind::Push, "", false), CommitStep::AskMessage);
        assert_eq!(commit_step(&files, CommitKind::Amend, "", false), CommitStep::Run { stage: vec![], busy: "Amending…" });
    }

    #[test]
    fn a_commit_takes_every_change_when_none_is_staged_and_only_the_staged_otherwise() {
        let loose = [file("a.rs"), FileStat { status: 'A', ..file("new.rs") }];
        assert_eq!(commit_step(&loose, CommitKind::Commit, "Edit", false), CommitStep::Run { stage: vec!["a.rs".into(), "new.rs".into()], busy: "Committing…" });
        assert_eq!(commit_step(&loose, CommitKind::Push, "Edit", false), CommitStep::Run { stage: vec!["a.rs".into(), "new.rs".into()], busy: "Committing…" });
        assert_eq!(commit_step(&loose, CommitKind::Amend, "Edit", false), CommitStep::Run { stage: vec![], busy: "Amending…" });
        let mixed = [file("a.rs"), staged("b.rs")];
        assert_eq!(commit_step(&mixed, CommitKind::Commit, "Edit", false), CommitStep::Run { stage: vec![], busy: "Committing…" });
    }

    #[test]
    fn the_commit_button_says_commit_all_when_nothing_is_staged_and_what_git_does_while_busy() {
        assert_eq!(commit_label(&[file("a.rs")], None), "Commit All");
        assert_eq!(commit_label(&[file("a.rs"), staged("b.rs")], None), "Commit");
        assert_eq!(commit_label(&[], None), "Commit");
        assert_eq!(commit_label(&[file("a.rs")], Some("Pushing…")), "Pushing…");
    }

    #[test]
    fn commit_is_ready_with_changes_a_message_and_git_idle() {
        let files = [file("a.rs")];
        assert!(commit_ready(&files, "Edit a", false));
        assert!(!commit_ready(&files, " \n", false));
        assert!(!commit_ready(&[], "Edit a", false));
        assert!(!commit_ready(&files, "Edit a", true));
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
