use crate::desktop::Desktop;
use crate::desktop::chrome::{doc_bar, empty};
use crate::git_ui::diff::{self, changed, code, code_text, hunk};
use crate::syntax::Spans;
use git::{CommitFile, Kind, Line};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use theme::*;
use workspace::Doc;
use workspace::tree::PaneId;

const CHUNK: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row {
    Header(usize),
    Code(usize, usize),
}

/// One file's diff in a commit, coloured.
#[derive(Default)]
pub struct FileDiff {
    lines: Vec<Line>,
    hl: Vec<Spans>,
}

fn read_file(cwd: &str, sha: &str, parent: Option<&str>, file: &CommitFile, options: git::Options) -> FileDiff {
    let (old, new) = git::commit_texts(&hosts::runner_for(cwd), cwd, sha, parent, file);
    let lines = git::diff_texts(&old, &new, &HashSet::new(), options);
    let syntax = diff::syntax(&file.path, &old, &new);
    let hl = diff::highlights(&lines, &syntax.0, &syntax.1);
    FileDiff { lines, hl }
}

/// Every file a commit changed, diffed one under another and read a few files at a time.
pub struct CommitState {
    pub(crate) sha: Option<String>,
    run: u64,
    files: Vec<CommitFile>,
    diffs: Vec<FileDiff>,
    rows: Vec<Row>,
    pub(crate) list: ListState,
}

impl Default for CommitState {
    fn default() -> Self {
        Self { sha: None, run: 0, files: Vec::new(), diffs: Vec::new(), rows: Vec::new(), list: ListState::new(0, ListAlignment::Top, px(400.)) }
    }
}

impl CommitState {
    /// Shows commit `sha` from the top; the returned run tags the reads for it.
    pub fn select(&mut self, sha: String) -> u64 {
        self.sha = Some(sha);
        self.files.clear();
        self.diffs.clear();
        self.rows.clear();
        self.list.reset(0);
        self.restart()
    }

    pub fn select_new(&mut self, sha: String) -> Option<u64> {
        (self.sha.as_ref() != Some(&sha)).then(|| self.select(sha))
    }

    /// Reads the shown commit again, dropping the reads already running.
    pub fn restart(&mut self) -> u64 {
        self.run += 1;
        self.run
    }

    pub fn set_files(&mut self, run: u64, files: Vec<CommitFile>) -> bool {
        if run != self.run {
            return false;
        }
        if files != self.files {
            self.files = files;
            self.diffs.clear();
        }
        self.relayout();
        true
    }

    /// Fills in the diffs of the files from `start` on.
    pub fn set_diffs(&mut self, run: u64, start: usize, diffs: Vec<FileDiff>) -> bool {
        if run != self.run {
            return false;
        }
        let end = start + diffs.len();
        if self.diffs.len() < end {
            self.diffs.resize_with(end, FileDiff::default);
        }
        self.diffs.splice(start..end, diffs);
        self.relayout();
        true
    }

    fn relayout(&mut self) {
        let lines = |f: usize| self.diffs.get(f).map_or(0, |d| d.lines.len());
        let rows: Vec<Row> = (0..self.files.len()).flat_map(|f| std::iter::once(Row::Header(f)).chain((0..lines(f)).map(move |j| Row::Code(f, j)))).collect();
        let (range, count) = changed(&self.rows, &rows);
        self.list.splice(range, count);
        self.rows = rows;
    }
}

pub(crate) enum CommitEvent {
    OpenFile(Doc, bool),
}

pub struct CommitView {
    pub(crate) state: CommitState,
    subject: Option<String>,
    code_size: u32,
}

impl EventEmitter<CommitEvent> for CommitView {}

impl CommitView {
    fn new() -> Self {
        Self { state: CommitState::default(), subject: None, code_size: 0 }
    }

    fn row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let c = &self.state;
        match c.rows.get(ix).copied() {
            Some(Row::Header(f)) => self.file_header(f, cx).into_any_element(),
            Some(Row::Code(f, j)) => {
                let d = &c.diffs[f];
                let l = &d.lines[j];
                let row = if l.kind == Kind::Hunk { hunk(&d.lines, j) } else { code(l, d.hl.get(j), vec![l.old, l.new], false) };
                row.w_full().into_any_element()
            }
            None => Empty.into_any_element(),
        }
    }

    /// A file's name over its diff; clicking it opens that file alone.
    fn file_header(&self, f: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let c = &self.state;
        let file = &c.files[f];
        let (dir, name) = file.path.rsplit_once('/').unwrap_or(("", &file.path));
        let doc = c.sha.clone().map(|sha| Doc::CommitFile { sha, path: file.path.clone() });
        div()
            .id(("commit-file", f))
            .w_full()
            .h(px(34.))
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(8.))
            .bg(FILL_1)
            .border_t(px(0.5))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .font_family(ui_font())
            .cursor_pointer()
            .hover(|s| s.bg(FILL_2))
            .child(file_icon(name, false, false, 14.))
            .child(
                div()
                    .flex_none()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(TEXT)
                    .when(file.status == 'D', |d| d.line_through())
                    .child(name.to_string()),
            )
            .when(!dir.is_empty(), |d| d.child(div().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_4).child(dir.to_string())))
            .child(
                div()
                    .ml_auto()
                    .font_family(MONO)
                    .text_size(px(11.))
                    .font_weight(FontWeight::BOLD)
                    .text_color(ui::git_color(file.status))
                    .child(file.status.to_string()),
            )
            .on_click(cx.listener(move |_, ev: &ClickEvent, _, cx| {
                if let Some(doc) = doc.clone() {
                    cx.emit(CommitEvent::OpenFile(doc, ev.click_count() > 1));
                }
            }))
    }
}

impl Render for CommitView {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some((sha, n, state)) = self.state.sha.clone().map(|sha| (sha, self.state.files.len(), self.state.list.clone())) else {
            return empty("No commit.");
        };
        let meta = vec![ui::meta_item().child(ui::meta_value(format!("{n} file{}", if n == 1 { "" } else { "s" }))).into_any_element()];
        let crumbs = std::iter::once(git::short_sha(&sha).to_string()).chain(self.subject.clone()).collect();
        let rows = list(state, cx.processor(|this, ix, _, cx| this.row(ix, cx))).pb(px(8.));
        div().flex_1().min_h_0().flex().flex_col().child(doc_bar(crumbs, meta, div())).child(
            code_text(div(), self.code_size).flex_1().min_h_0().bg(PAGE).border_t(px(0.5)).border_color(SEPARATOR).overflow_hidden().child(rows.size_full()),
        )
    }
}

pub(crate) struct Slot {
    pub(crate) view: Entity<CommitView>,
    _sub: Subscription,
}

/// The commit each pane shows.
#[derive(Default)]
pub struct Commits {
    pub(crate) panes: HashMap<PaneId, Slot>,
}

impl Commits {
    pub fn get(&self, pane: PaneId) -> Option<&Entity<CommitView>> {
        self.panes.get(&pane).map(|s| &s.view)
    }

    pub fn shows(&self, pane: PaneId, sha: &str, cx: &App) -> bool {
        self.get(pane).is_some_and(|v| v.read(cx).state.sha.as_deref() == Some(sha))
    }

    pub fn retain_panes(&mut self, panes: &[PaneId]) {
        self.panes.retain(|p, _| panes.contains(p));
    }
}

impl Desktop {
    fn commit_entity(&mut self, pane: PaneId, cx: &mut Context<Self>) -> Entity<CommitView> {
        if let Some(view) = self.commit.get(pane) {
            return view.clone();
        }
        let view = cx.new(|_| CommitView::new());
        let sub = cx.subscribe(&view, |this, _, ev: &CommitEvent, cx| match ev {
            CommitEvent::OpenFile(doc, pin) => this.open_doc(doc.clone(), *pin, cx),
        });
        self.commit.panes.insert(pane, Slot { view: view.clone(), _sub: sub });
        view
    }

    pub(crate) fn show_commit(&mut self, pane: PaneId, sha: String, cx: &mut Context<Self>) {
        let view = self.commit_entity(pane, cx);
        if let Some(run) = view.update(cx, |v, _| v.state.select_new(sha)) {
            self.load_commit(pane, run, cx);
        }
    }

    /// Reads every pane's commit again, for a new appearance or diff option.
    pub(crate) fn recolor_commit(&mut self, cx: &mut Context<Self>) {
        let shown: Vec<(PaneId, u64)> =
            self.commit.panes.iter().filter_map(|(p, s)| Some((*p, s.view.update(cx, |v, _| v.state.sha.is_some().then(|| v.state.restart()))?))).collect();
        for (pane, run) in shown {
            self.load_commit(pane, run, cx);
        }
    }

    fn load_commit(&mut self, pane: PaneId, run: u64, cx: &mut Context<Self>) {
        let Some((cwd, sha)) = self.cwd(cx).zip(self.commit.get(pane).and_then(|v| v.read(cx).state.sha.clone())) else { return };
        let options = self.diff.options;
        cx.spawn(async move |this, cx| {
            let (parent, files) = cx
                .background_executor()
                .spawn({
                    let (cwd, sha) = (cwd.clone(), sha.clone());
                    async move {
                        let runner = hosts::runner_for(&cwd);
                        let parent = git::first_parent(&runner, &cwd, &sha);
                        let files = git::commit_files(&runner, &cwd, &sha, parent.as_deref());
                        (parent, files)
                    }
                })
                .await;
            let shown = this.update(cx, |d, cx| {
                d.commit.get(pane).is_some_and(|v| {
                    v.update(cx, |v, cx| {
                        let ok = v.state.set_files(run, files.clone());
                        if ok {
                            cx.notify();
                        }
                        ok
                    })
                })
            });
            if !matches!(shown, Ok(true)) {
                return;
            }
            for start in (0..files.len()).step_by(CHUNK) {
                let chunk = files[start..(start + CHUNK).min(files.len())].to_vec();
                let (cwd, sha, parent) = (cwd.clone(), sha.clone(), parent.clone());
                let diffs = cx
                    .background_executor()
                    .spawn(async move { chunk.iter().map(|f| read_file(&cwd, &sha, parent.as_deref(), f, options)).collect::<Vec<_>>() })
                    .await;
                let shown = this.update(cx, |d, cx| {
                    d.commit.get(pane).is_some_and(|v| {
                        v.update(cx, |v, cx| {
                            let ok = v.state.set_diffs(run, start, diffs);
                            if ok {
                                cx.notify();
                            }
                            ok
                        })
                    })
                });
                if !matches!(shown, Ok(true)) {
                    return;
                }
            }
        })
        .detach();
    }

    pub fn commit_view(&mut self, pane: PaneId, cx: &mut Context<Self>) -> Div {
        let Some(view) = self.commit.get(pane).cloned() else { return empty("No commit.") };
        let Some(sha) = view.read(cx).state.sha.clone() else { return empty("No commit.") };
        let subject = self.graph.read(cx).commit(&sha).map(|c| c.subject.clone());
        let code_size = self.store.appearance.code_size();
        view.update(cx, |v, _| (v.subject, v.code_size) = (subject, code_size));
        div().flex_1().min_h_0().flex().flex_col().child(view)
    }
}

#[cfg(test)]
mod tests {
    use super::{CommitState, FileDiff, Row};
    use git::{CommitFile, parse};

    fn file(path: &str) -> CommitFile {
        CommitFile { path: path.into(), old_path: None, status: 'M' }
    }

    fn diff(text: &str) -> FileDiff {
        FileDiff { lines: parse(text), hl: Vec::new() }
    }

    #[test]
    fn a_read_for_a_commit_no_longer_shown_is_ignored() {
        let mut c = CommitState::default();
        let old = c.select("a".into());
        let run = c.select("b".into());
        assert!(!c.set_files(old, vec![file("x")]));
        assert!(c.set_files(run, vec![file("y")]));
        assert!(!c.set_diffs(old, 0, vec![diff("@@ -1,1 +1,1 @@\n-a\n+b\n")]));
        assert_eq!(c.rows, [Row::Header(0)]);
    }

    #[test]
    fn each_file_header_sits_above_its_lines() {
        let mut c = CommitState::default();
        let run = c.select("a".into());
        c.set_files(run, vec![file("x"), file("y")]);
        assert_eq!(c.rows, [Row::Header(0), Row::Header(1)]);
        c.set_diffs(run, 0, vec![diff("@@ -1,1 +1,1 @@\n-a\n+b\n"), diff("@@ -1,1 +1,2 @@\n a\n+c\n")]);
        let lines = |f| (0..3).map(move |j| Row::Code(f, j));
        let want: Vec<Row> = [Row::Header(0)].into_iter().chain(lines(0)).chain([Row::Header(1)]).chain(lines(1)).collect();
        assert_eq!(c.rows, want);
    }

    #[test]
    fn reading_again_for_a_new_appearance_replaces_the_diffs_in_place() {
        let mut c = CommitState::default();
        let run = c.select("a".into());
        c.set_files(run, vec![file("x")]);
        c.set_diffs(run, 0, vec![diff("@@ -1,1 +1,1 @@\n-a\n+b\n")]);
        let again = c.restart();
        assert!(c.set_files(again, vec![file("x")]));
        assert_eq!(c.rows.len(), 4);
        assert!(c.set_diffs(again, 0, vec![diff("@@ -1,1 +1,1 @@\n-a\n+z\n")]));
        assert_eq!(c.diffs[0].lines[2].text, "z");
    }

    #[test]
    fn showing_the_commit_already_shown_reads_nothing_again() {
        let mut c = CommitState::default();
        let run = c.select_new("a".into()).unwrap();
        c.set_files(run, vec![file("x")]);
        assert!(c.select_new("a".into()).is_none());
        assert_eq!(c.rows, [Row::Header(0)]);
        assert!(c.select_new("b".into()).is_some());
        assert!(c.rows.is_empty());
    }
}
