mod comment;
mod composer;
mod row;
mod target;
pub(crate) use row::{code, hunk};

use git::{self, Kind, Line};
use theme::*;
use ui::{self, Segment, Variant, checkbox, dot};
use crate::desktop::{Desktop, MAIN};
use crate::desktop::chrome::{Side, doc_bar, empty};
use crate::explorer::status_word;
use crate::syntax::{Spans, language_for, line_spans};
use crate::util::{ago_long, now_ms};
use gpui_kit::component::input::{InputEvent, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use serde_json::json;
use std::collections::{HashMap, HashSet};
use std::ops::{Range, RangeInclusive};
use workspace::Doc;
use workspace::tree::PaneId;

/// A comment sent to an agent about lines of a file, kept so the diff can show it until resolved.
#[derive(Debug, PartialEq)]
pub struct Comment {
    pub path: String,
    pub lines: (usize, usize),
    pub old_side: bool,
    pub label: String,
    pub text: String,
    pub at: i64,
}

const NUM: f32 = 44.;
const SIGN: f32 = 18.;
pub(crate) const ROW: f32 = 22.;

const MAX_COLORED: usize = 512 * 1024;
const MAX_WORD_LINES: usize = 5;

type Sides = (Vec<Spans>, Vec<Spans>);

pub struct DiffLoad {
    path: String,
    at: Option<String>,
    open: HashSet<usize>,
    lines: Vec<Line>,
    source: (String, String),
    colors: Option<(Sides, Vec<Spans>)>,
    dark: bool,
}

/// Reads and diffs `path` in the working tree, or with `at` in that commit, colouring it only when the lines differ from `shown`. Run off the UI thread.
pub fn read_diff(cwd: &str, path: String, at: Option<String>, open: HashSet<usize>, shown: &[Line]) -> DiffLoad {
    let dark = theme::is_dark();
    let (old, new) = match &at {
        Some(sha) => git::texts_at(cwd, sha, &path),
        None => git::texts(cwd, &path),
    };
    let lines = git::diff_texts(&old, &new, &open);
    let colors = (lines != shown).then(|| {
        let syntax = syntax(&path, &old, &new);
        let hl = highlights(&lines, &syntax.0, &syntax.1);
        (syntax, hl)
    });
    DiffLoad { path, at, open, lines, source: (old, new), colors, dark }
}

/// Syntax colours for every line of the old and new file; none when either is too big to parse quickly. Run off the UI thread.
pub fn syntax(path: &str, old: &str, new: &str) -> (Vec<Spans>, Vec<Spans>) {
    if old.len().max(new.len()) > MAX_COLORED {
        return Default::default();
    }
    let language = language_for(path);
    (line_spans(language, old), line_spans(language, new))
}

fn tint(syntax: Spans, words: Vec<Range<usize>>, color: Token) -> Spans {
    let bg = HighlightStyle { background_color: Some(color.into()), ..Default::default() };
    combine_highlights(syntax, words.into_iter().map(|r| (r, bg))).collect()
}

/// Colours for each diff line: syntax from its side of the file, plus word tints where a run of up to five deletions is replaced line for line.
pub fn highlights(lines: &[Line], old: &[Spans], new: &[Spans]) -> Vec<Spans> {
    let syntax = |l: &Line| {
        let side = match l.kind {
            Kind::Hunk => None,
            Kind::Del => l.old.and_then(|n| old.get(n - 1)),
            Kind::Add | Kind::Context => l.new.and_then(|n| new.get(n - 1)),
        };
        let fits = |r: &Range<usize>| r.end <= l.text.len() && l.text.is_char_boundary(r.start) && l.text.is_char_boundary(r.end);
        side.map(|s| s.iter().filter(|(r, _)| fits(r)).cloned().collect()).unwrap_or_default()
    };
    let mut out: Vec<Spans> = lines.iter().map(syntax).collect();
    let mut i = 0;
    while i < lines.len() {
        let dels = lines[i..].iter().take_while(|l| l.kind == Kind::Del).count();
        let adds = lines[i + dels..].iter().take_while(|l| l.kind == Kind::Add).count();
        if dels == 0 {
            i += 1;
            continue;
        }
        if dels == adds && dels <= MAX_WORD_LINES {
            for k in 0..dels {
                let (d, a) = (i + k, i + dels + k);
                let (del, add) = git::words(&lines[d].text, &lines[a].text);
                out[d] = tint(std::mem::take(&mut out[d]), del, DIFF_DEL_WORD);
                out[a] = tint(std::mem::take(&mut out[a]), add, DIFF_ADD_WORD);
            }
        }
        i += dels + adds;
    }
    out
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Row {
    Unified(usize),
    Split(Option<usize>, Option<usize>),
    Composer,
    Comment(usize),
}

/// Lays out the diff, with each `(line, comment)` note and then the composer under the row holding their line.
fn rows(lines: &[Line], split: bool, composer: Option<usize>, notes: &[(usize, usize)]) -> Vec<Row> {
    let base: Vec<Row> =
        if split { git::split(lines).into_iter().map(|(l, r)| Row::Split(l, r)).collect() } else { (0..lines.len()).map(Row::Unified).collect() };
    let mut out = Vec::with_capacity(base.len() + notes.len() + 1);
    for row in base {
        out.push(row);
        let holds = |i: usize| match row {
            Row::Unified(j) => j == i,
            Row::Split(l, r) => l == Some(i) || r == Some(i),
            Row::Composer | Row::Comment(_) => false,
        };
        out.extend(notes.iter().filter(|(line, _)| holds(*line)).map(|&(_, c)| Row::Comment(c)));
        if composer.is_some_and(&holds) {
            out.push(Row::Composer);
        }
    }
    out
}

/// The smallest splice turning `old` into `new`, so the list keeps its scroll offset and measured heights.
pub(crate) fn changed<T: PartialEq>(old: &[T], new: &[T]) -> (Range<usize>, usize) {
    let head = old.iter().zip(new).take_while(|(a, b)| a == b).count();
    let tail = old[head..].iter().rev().zip(new[head..].iter().rev()).take_while(|(a, b)| a == b).count();
    (head..old.len() - tail, new.len() - head - tail)
}

pub(crate) fn ordered((a, b): (usize, usize)) -> RangeInclusive<usize> {
    a.min(b)..=a.max(b)
}

/// First and last line numbers picked, counted on the new side unless the range only removes lines (then `true`).
pub(crate) fn span(lines: &[Line], range: RangeInclusive<usize>) -> Option<(usize, usize, bool)> {
    let picked: Vec<&Line> = lines.get(range)?.iter().filter(|l| l.kind != Kind::Hunk).collect();
    let new: Vec<usize> = picked.iter().filter_map(|l| l.new).collect();
    let old_side = new.is_empty();
    let nums = if old_side { picked.iter().filter_map(|l| l.old).collect() } else { new };
    Some((*nums.iter().min()?, *nums.iter().max()?, old_side))
}

/// "Line 54" or "Lines 50–54".
fn label(lines: &[Line], range: RangeInclusive<usize>) -> Option<String> {
    let (lo, hi, _) = span(lines, range)?;
    Some(if lo == hi { format!("Line {lo}") } else { format!("Lines {lo}–{hi}") })
}

/// The line a sent comment hangs under: its last line, on the side it was made.
fn anchor(lines: &[Line], c: &Comment) -> Option<usize> {
    lines.iter().position(|l| if c.old_side { l.new.is_none() && l.old == Some(c.lines.1) } else { l.new == Some(c.lines.1) })
}

/// `(line, comment)` for each comment on `path` whose line is in the diff.
fn notes(comments: &[Comment], path: Option<&str>, lines: &[Line]) -> Vec<(usize, usize)> {
    comments.iter().enumerate().filter(|(_, c)| Some(c.path.as_str()) == path).filter_map(|(i, c)| Some((anchor(lines, c)?, i))).collect()
}

/// The session a comment goes to: the one picked, else the open one, else the newest of the project's `live` ones.
fn comment_target(picked: Option<String>, open: Option<String>, live: &[String]) -> Option<String> {
    let alive = |id: &String| live.contains(id);
    picked.filter(alive).or_else(|| open.filter(alive)).or_else(|| live.first().cloned())
}

pub(crate) fn line_label((lo, hi): (usize, usize)) -> String {
    if lo == hi { format!("L{lo}") } else { format!("L{lo}-{hi}") }
}

/// Where line `i` of `old` sits in `new`, so a selection survives the diff refreshing under it.
fn remap(old: &[Line], new: &[Line], i: usize) -> Option<usize> {
    let l = old.get(i)?;
    let same = |n: &&Line| n.kind == l.kind && n.text == l.text;
    new.iter().position(|n| n == l).or_else(|| new.iter().enumerate().filter(|(_, n)| same(n)).min_by_key(|(j, _)| j.abs_diff(i)).map(|(j, _)| j))
}

fn same_hunk(lines: &[Line], a: usize, b: usize) -> bool {
    !lines[a.min(b)..=a.max(b)].iter().any(|l| l.kind == Kind::Hunk)
}

/// The diff lines picked for a comment: `range` runs from where the pick started to where it ends.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Pick {
    pub(crate) range: Option<(usize, usize)>,
    pub(crate) dragging: bool,
    pub(crate) composing: bool,
}

impl Pick {
    /// Starts a pick on line `i`, or with `extend` stretches the current one to it; true when it started afresh.
    pub fn press(&mut self, lines: &[Line], i: usize, extend: bool) -> bool {
        let kept = self.range.map(|(a, _)| a).filter(|&a| extend && same_hunk(lines, a, i));
        if kept.is_none() {
            self.composing = false;
        }
        self.range = Some((kept.unwrap_or(i), i));
        self.dragging = true;
        kept.is_none()
    }

    pub fn held(&self) -> bool {
        self.dragging && self.range.is_some()
    }

    /// Stretches a held pick to line `i` without leaving its hunk; true when it moved.
    pub fn drag(&mut self, lines: &[Line], i: usize) -> bool {
        let Some((anchor, end)) = self.range.filter(|_| self.dragging) else { return false };
        let moved = end != i && same_hunk(lines, anchor, i);
        if moved {
            self.range = Some((anchor, i));
        }
        moved
    }

    /// Lets go of a held pick, opening the composer when it spans several lines; false when nothing was held.
    pub fn release(&mut self) -> bool {
        if !std::mem::take(&mut self.dragging) {
            return false;
        }
        if self.range.is_some_and(|(a, b)| a != b) {
            self.composing = true;
        }
        true
    }

    /// Opens the composer on the pick holding line `i`, or on `i` alone; true when the pick started afresh.
    pub fn open(&mut self, lines: &[Line], i: usize) -> bool {
        let fresh = !self.picked(lines, i);
        if fresh {
            self.range = Some((i, i));
        }
        self.composing = true;
        fresh
    }

    /// Moves the pick from `old` lines onto `new` ones, dropping it when either end is gone.
    pub fn remap(&mut self, old: &[Line], new: &[Line]) {
        self.range = self.range.and_then(|(a, b)| Some((remap(old, new, a)?, remap(old, new, b)?)));
    }

    pub fn cancel(&mut self) {
        self.range = None;
        self.composing = false;
    }

    pub fn picked(&self, lines: &[Line], i: usize) -> bool {
        self.range.is_some_and(|s| ordered(s).contains(&i)) && lines[i].kind != Kind::Hunk
    }

    pub fn last(&self) -> Option<usize> {
        self.range.map(|s| *ordered(s).end())
    }

    pub fn composer_line(&self) -> Option<usize> {
        self.last().filter(|_| self.composing && !self.dragging)
    }

    pub fn label(&self, lines: &[Line]) -> Option<String> {
        label(lines, ordered(self.range?))
    }

    pub fn comment(&self, lines: &[Line], path: String, label: String, text: String, at: i64) -> Option<Comment> {
        let (lo, hi, old_side) = span(lines, ordered(self.range?))?;
        Some(Comment { path, lines: (lo, hi), old_side, label, text, at })
    }
}

/// One pane's diff: the file it shows, its lines, and the lines picked there for a comment.
pub struct DiffView {
    pub(crate) file: Option<String>,
    pub(crate) at: Option<String>,
    pub(crate) lines: Vec<git::Line>,
    pub(crate) rows: Vec<Row>,
    pub(crate) list: ListState,
    pub(crate) hl: Vec<Spans>,
    pub(crate) open: HashSet<usize>,
    pub(crate) source: (String, String),
    pub(crate) syntax: (Vec<Spans>, Vec<Spans>),
    pub(crate) pick: Pick,
}

impl Default for DiffView {
    fn default() -> Self {
        Self {
            file: None,
            at: None,
            lines: Vec::new(),
            rows: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            hl: Vec::new(),
            open: HashSet::new(),
            source: Default::default(),
            syntax: Default::default(),
            pick: Pick::default(),
        }
    }
}

impl DiffView {
    /// Whether this shows `path` in the working tree, or with `at` in that commit.
    pub fn shows(&self, path: &str, at: Option<&str>) -> bool {
        self.file.as_deref() == Some(path) && self.at.as_deref() == at
    }

    pub fn working_file(&self) -> Option<&str> {
        self.file.as_deref().filter(|_| self.at.is_none())
    }

    fn set_lines(&mut self, lines: Vec<Line>) -> bool {
        if lines == self.lines {
            return false;
        }
        self.pick.remap(&self.lines, &lines);
        self.lines = lines;
        true
    }

    /// Rebuilds the rows; without `reset` only the rows that changed are remeasured and the scroll position stays.
    fn layout(&mut self, reset: bool, split: bool, comments: &[Comment]) {
        let notes = notes(comments, self.working_file(), &self.lines);
        let rows = rows(&self.lines, split, self.pick.composer_line(), &notes);
        if reset {
            self.list.reset(rows.len());
        } else {
            let (range, count) = changed(&self.rows, &rows);
            self.list.splice(range, count);
        }
        self.rows = rows;
    }
}

/// The diff each pane shows, and the comments, one of them being written, shared by all. The input is `I` so the rules test without GPUI.
pub struct DiffState<I = Entity<TextareaState>> {
    pub(crate) panes: HashMap<PaneId, DiffView>,
    pub(crate) split: bool,
    pub(crate) input: I,
    /// The pane whose pick the comment being written is about.
    pub(crate) drafting: PaneId,
    pub(crate) target: Option<String>,
    pub(crate) target_menu: bool,
    pub(crate) comments: Vec<Comment>,
    pub(crate) viewed: HashSet<String>,
}

impl DiffState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Ask the agent about these lines…").rows(3));
        let subs = vec![cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { secondary: true, .. } => this.submit_comment(window, cx),
            InputEvent::Change => cx.notify(),
            _ => {}
        })];
        (Self::with(input), subs)
    }
}

impl<I> DiffState<I> {
    pub fn with(input: I) -> Self {
        Self { panes: HashMap::new(), split: false, input, drafting: MAIN, target: None, target_menu: false, comments: Vec::new(), viewed: HashSet::new() }
    }

    pub fn view(&self, pane: PaneId) -> Option<&DiffView> {
        self.panes.get(&pane)
    }

    /// The pane the comment is being written in.
    pub fn draft(&self) -> Option<&DiffView> {
        self.panes.get(&self.drafting)
    }

    /// Shows `load` in `pane` unless another file, fold state or appearance was picked there while it ran.
    pub fn apply(&mut self, pane: PaneId, load: DiffLoad) -> bool {
        let Some(v) = self.panes.get_mut(&pane) else { return false };
        if v.file.as_ref() != Some(&load.path) || v.at != load.at || load.open != v.open || load.colors.is_some() && load.dark != theme::is_dark() {
            return false;
        }
        v.source = load.source;
        let recolored = load.colors.map(|(syntax, hl)| (v.syntax, v.hl) = (syntax, hl)).is_some();
        let changed = v.set_lines(load.lines);
        if changed {
            v.layout(true, self.split, &self.comments);
        }
        changed | recolored
    }

    /// Shows `path` in `pane`, in the working tree or with `at` in that commit, dropping the pick, folds and lines of another file.
    pub fn select(&mut self, pane: PaneId, path: String, at: Option<String>) {
        let v = self.panes.entry(pane).or_default();
        if v.shows(&path, at.as_deref()) {
            return;
        }
        v.pick.range = None;
        v.open.clear();
        if v.set_lines(Vec::new()) {
            v.layout(true, self.split, &self.comments);
        }
        (v.file, v.at) = (Some(path), at);
    }

    /// Shows the unchanged lines folded away from `start` on in `pane`, keeping the scroll position.
    fn expand(&mut self, pane: PaneId, start: usize) {
        let Some(v) = self.panes.get_mut(&pane) else { return };
        v.open.insert(start);
        let lines = git::diff_texts(&v.source.0, &v.source.1, &v.open);
        v.hl = highlights(&lines, &v.syntax.0, &v.syntax.1);
        if v.set_lines(lines) {
            v.layout(false, self.split, &self.comments);
        }
    }

    pub fn layout(&mut self, pane: PaneId, reset: bool) {
        if let Some(v) = self.panes.get_mut(&pane) {
            v.layout(reset, self.split, &self.comments);
        }
    }

    pub fn relayout(&mut self, reset: bool) {
        for v in self.panes.values_mut() {
            v.layout(reset, self.split, &self.comments);
        }
    }

    /// Moves the comment being written to `pane`, dropping the pick another pane held.
    pub fn draft_in(&mut self, pane: PaneId) {
        self.drafting = pane;
        for (_, v) in self.panes.iter_mut().filter(|(p, v)| **p != pane && v.pick != Pick::default()) {
            v.pick = Pick::default();
            v.layout(false, self.split, &self.comments);
        }
    }

    pub fn cancel_draft(&mut self) {
        if let Some(v) = self.panes.get_mut(&self.drafting) {
            v.pick.cancel();
            v.layout(false, self.split, &self.comments);
        }
    }

    pub fn add_comment(&mut self, comment: Comment) {
        self.comments.push(comment);
        self.relayout(false);
    }

    fn resolve_comment(&mut self, i: usize) -> bool {
        let found = i < self.comments.len();
        if found {
            self.comments.remove(i);
            self.relayout(false);
        }
        found
    }

    pub fn retain_panes(&mut self, panes: &[PaneId]) {
        self.panes.retain(|p, _| panes.contains(p));
    }
}

impl Desktop {
    pub fn diff_view(&mut self, pane: PaneId, cx: &mut Context<Self>) -> Div {
        let Some((path, at)) = self.diff.view(pane).and_then(|v| Some((v.file.clone()?, v.at.clone()))) else {
            return empty("No changes.");
        };
        let file = self.repo().and_then(|r| r.files.iter().find(|f| f.path == path)).cloned().filter(|_| at.is_none());
        let viewed = self.diff.viewed.contains(&path);
        let toggle = path.clone();
        let right = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().id("diff-mode").child(ui::segmented(
                vec![Segment { icon: None, value: false, label: "Unified".into(), badge: None }, Segment { icon: None, value: true, label: "Split".into(), badge: None }],
                self.diff.split,
                true,
                false,
                |this, v, cx| {
                    this.diff.split = v;
                    this.diff.relayout(true);
                    cx.notify();
                },
                cx,
            )))
            .when(at.is_none(), |d| {
                d.child(ui::button("viewed", Variant::Glass, None, div().flex().items_center().gap(px(7.)).child(checkbox(viewed)).child("Viewed")).on_click(cx.listener(
                    move |this, _: &ClickEvent, _, cx| {
                        if !this.diff.viewed.remove(&toggle) {
                            this.diff.viewed.insert(toggle.clone());
                        }
                        cx.notify();
                    },
                )))
            })
            .child(ui::icon_group([
                ui::group_button("diff-more", "more").on_click(cx.listener(|this, e: &ClickEvent, window, cx| this.open_more(e.position(), window, cx))),
            ]));
        let status = match &at {
            Some(sha) => self.graph.file(sha, &path).map(|f| Some(f.status)),
            None => Some(file.as_ref().map(|f| f.status)),
        };
        let mut meta = Vec::new();
        if let Some(status) = status {
            let (color, state) = status_word(status);
            meta.push(ui::meta_item().child(dot(7., color)).child(ui::meta_value(state)).into_any_element());
        }
        if let Some(sha) = &at {
            meta.push(ui::meta_item().child(ui::meta_value(git::short_sha(sha).to_string())).into_any_element());
            if let Some(c) = self.graph.commit(sha) {
                meta.push(ui::meta_item().child(div().max_w(px(320.)).truncate().child(c.subject.clone())).into_any_element());
            }
        }
        if let Some(f) = &file {
            meta.push(ui::meta_item().child(ui::meta_diff(f.added, f.removed, 11.5)).into_any_element());
        }
        if at.is_none() {
            let comments = self.diff.comments.iter().filter(|c| c.path == path).count();
            if comments > 0 {
                let label = format!("{comments} comment{}", if comments == 1 { "" } else { "s" });
                meta.push(ui::meta_item().child(icon("comment", 13., TEXT_2)).child(ui::meta_value(label)).into_any_element());
            }
            let abs = self.cwd().map(|c| format!("{c}/{path}")).unwrap_or_default();
            if let Some((a, ts)) = self.agents.last_edit(&abs) {
                let by = format!("{} · {}", provider_name(&a.provider), ago_long(ts, now_ms()));
                meta.push(ui::meta_item().child(provider_icon(&a.provider, 13., TEXT_2)).child("by").child(ui::meta_value(by)).into_any_element());
            }
        }
        let (dir, name) = path.rsplit_once('/').map_or((None, path.clone()), |(d, n)| (Some(d.to_string()), n.to_string()));
        let first = at.as_deref().map_or_else(|| "Changes".to_string(), |sha| git::short_sha(sha).to_string());
        let crumbs = std::iter::once(first).chain(dir).chain([name]).collect();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(doc_bar(crumbs, meta, right))
            .child(self.diff_box(pane, cx))
    }

    pub fn diff_box(&mut self, pane: PaneId, cx: &mut Context<Self>) -> Div {
        let Some(state) = self.diff.view(pane).map(|v| v.list.clone()) else { return div().flex_1() };
        let rows = list(state, cx.processor(move |this, ix, _, cx| this.diff_row(pane, ix, cx))).pb(px(8.));
        div()
            .flex_1()
            .min_h_0()
            .bg(PAGE)
            .border_t(px(0.5))
            .border_color(SEPARATOR)
            .overflow_hidden()
            .font_family(MONO)
            .text_size(px(12.5))
            .line_height(px(ROW))
            .child(rows.size_full())
            .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| this.end_drag(pane, window, cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(move |this, _, window, cx| this.end_drag(pane, window, cx)))
    }

    fn expand(&mut self, pane: PaneId, start: usize, cx: &mut Context<Self>) {
        self.diff.expand(pane, start);
        cx.notify();
    }

    fn resolve_comment(&mut self, i: usize, cx: &mut Context<Self>) {
        if self.diff.resolve_comment(i) {
            cx.notify();
        }
    }

    pub fn open_changes(&mut self, path: Option<String>, pin: bool, cx: &mut Context<Self>) {
        let shown = self.diff.view(self.focused_pane()).and_then(|v| v.working_file()).map(str::to_string);
        let path = path.or(shown).or_else(|| self.repo()?.files.first().map(|f| f.path.clone()));
        self.side = Side::Changes;
        match path {
            Some(path) => self.open_doc(Doc::Diff(path), pin, cx),
            None => cx.notify(),
        }
    }

    pub(crate) fn load_diff(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        let shown = self.diff.view(pane).map(|v| v.lines.clone()).unwrap_or_default();
        self.read_diff_in_background(pane, shown, cx);
    }

    /// Colours every pane's diff again, for a new appearance.
    pub(crate) fn recolor_diff(&mut self, cx: &mut Context<Self>) {
        for pane in self.diff.panes.keys().copied().collect::<Vec<_>>() {
            self.read_diff_in_background(pane, Vec::new(), cx);
        }
    }

    fn read_diff_in_background(&mut self, pane: PaneId, shown: Vec<Line>, cx: &mut Context<Self>) {
        let Some((cwd, (path, at, open))) = self.cwd().zip(self.diff.view(pane).and_then(|v| Some((v.file.clone()?, v.at.clone(), v.open.clone())))) else { return };
        let task = cx.background_executor().spawn(async move { read_diff(&cwd, path, at, open, &shown) });
        cx.spawn(async move |this, cx| {
            let load = task.await;
            this.update(cx, |d, cx| {
                if d.diff.apply(pane, load) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Starts a comment on line `i` of `pane`'s diff, or with `extend` stretches the open one to it.
    pub fn select_line(&mut self, pane: PaneId, i: usize, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.diff.draft_in(pane);
        let Some(v) = self.diff.panes.get_mut(&pane) else { return };
        if v.pick.press(&v.lines, i, extend) {
            self.diff.input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.diff.layout(pane, false);
        cx.notify();
    }

    pub fn drag_to(&mut self, pane: PaneId, i: usize, pressed: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(v) = self.diff.panes.get_mut(&pane) else { return };
        if !pressed && v.pick.held() {
            return self.end_drag(pane, window, cx);
        }
        if v.pick.drag(&v.lines, i) {
            cx.notify();
        }
    }

    pub fn end_drag(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.diff.panes.get_mut(&pane).is_some_and(|v| v.pick.release()) {
            return;
        }
        self.diff.layout(pane, false);
        if self.diff.view(pane).is_some_and(|v| v.pick.composing) {
            self.diff.input.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    pub fn open_comment(&mut self, pane: PaneId, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.diff.draft_in(pane);
        let Some(v) = self.diff.panes.get_mut(&pane) else { return };
        if v.pick.open(&v.lines, i) {
            self.diff.input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.diff.layout(pane, false);
        self.diff.input.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub fn cancel_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.diff.cancel_draft();
        self.diff.target_menu = false;
        self.diff.input.update(cx, |s, cx| s.set_value("", window, cx));
        window.focus(&self.root, cx);
        cx.notify();
    }

    pub fn submit_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.agents.observe_only() {
            return;
        }
        let text = self.diff.input.read(cx).value().trim().to_string();
        let Some(v) = self.diff.draft() else { return };
        let (Some(target), Some(path), Some(lines)) = (self.comment_target(), v.file.clone(), v.pick.label(&v.lines)) else { return };
        if text.is_empty() {
            return;
        }
        let Some(terminal) = self.agents.get(&target).map(|a| a.terminal_id.clone()) else { return };
        self.daemon.send(json!({"op": "prompt", "id": terminal, "text": format!("{path} {}: {text}", lines.to_lowercase())}));
        if let Some(comment) = self.diff.draft().and_then(|v| v.pick.comment(&v.lines, path, lines, text, now_ms())) {
            self.diff.add_comment(comment);
        }
        self.cancel_comment(window, cx);
    }

    pub fn comment_target(&self) -> Option<String> {
        let live: Vec<String> = self.cards(self.project.as_deref()?).into_iter().map(|c| c.id).collect();
        comment_target(self.diff.target.clone(), self.session.clone(), &live)
    }
}

#[cfg(test)]
mod tests {
    use super::{Comment, DiffLoad, DiffState, Pick, Row, changed, comment_target, highlights, label, notes, remap, rows};
    use crate::desktop::MAIN;
    use git::parse;
    use gpui_kit::HighlightStyle;
    use std::collections::HashSet;
    use theme::{DIFF_ADD_WORD, DIFF_DEL_WORD, SYN_FN, SYN_KEYWORD, SYN_STRING, Token};
    use workspace::tree::PaneId;

    const DIFF: &str = "@@ -1,2 +1,3 @@\n a\n-b\n+c\n+d\n";
    const TWO_HUNKS: &str = "@@ -1,2 +1,2 @@\n a\n-b\n+c\n@@ -9,1 +9,1 @@\n-x\n+y\n";
    const OTHER: PaneId = 1;

    #[test]
    fn pressing_a_line_holds_it_as_a_fresh_pick() {
        let l = parse(DIFF);
        let mut pick = Pick { range: Some((1, 2)), dragging: false, composing: true };
        assert!(pick.press(&l, 3, false));
        assert_eq!(pick, Pick { range: Some((3, 3)), dragging: true, composing: false });
    }

    #[test]
    fn shift_pressing_stretches_the_pick_from_where_it_started() {
        let l = parse(DIFF);
        let mut pick = Pick { range: Some((1, 1)), dragging: false, composing: true };
        assert!(!pick.press(&l, 4, true));
        assert_eq!(pick, Pick { range: Some((1, 4)), dragging: true, composing: true });
    }

    #[test]
    fn shift_pressing_past_a_hunk_header_starts_afresh() {
        let l = parse(TWO_HUNKS);
        let mut pick = Pick { range: Some((1, 2)), dragging: false, composing: true };
        assert!(pick.press(&l, 5, true));
        assert_eq!(pick, Pick { range: Some((5, 5)), dragging: true, composing: false });
    }

    #[test]
    fn dragging_stretches_the_held_pick() {
        let l = parse(DIFF);
        let mut pick = Pick { range: Some((1, 1)), dragging: true, composing: false };
        assert!(pick.drag(&l, 3));
        assert!(!pick.drag(&l, 3));
        assert_eq!(pick.range, Some((1, 3)));
    }

    #[test]
    fn dragging_stops_at_the_edge_of_its_hunk() {
        let l = parse(TWO_HUNKS);
        let mut pick = Pick { range: Some((1, 2)), dragging: true, composing: false };
        assert!(!pick.drag(&l, 4));
        assert!(!pick.drag(&l, 6));
        assert_eq!(pick.range, Some((1, 2)));
    }

    #[test]
    fn moving_over_lines_without_a_held_pick_changes_nothing() {
        let l = parse(DIFF);
        let mut pick = Pick { range: Some((1, 1)), dragging: false, composing: false };
        assert!(!pick.drag(&l, 3));
        assert_eq!(pick.range, Some((1, 1)));
    }

    #[test]
    fn releasing_a_stretched_pick_opens_the_composer() {
        let mut pick = Pick { range: Some((1, 3)), dragging: true, composing: false };
        assert!(pick.release());
        assert_eq!(pick, Pick { range: Some((1, 3)), dragging: false, composing: true });
        assert!(!pick.release());
    }

    #[test]
    fn releasing_a_single_line_leaves_the_composer_closed() {
        let mut pick = Pick { range: Some((2, 2)), dragging: true, composing: false };
        assert!(pick.release());
        assert_eq!(pick, Pick { range: Some((2, 2)), dragging: false, composing: false });
    }

    #[test]
    fn picks_the_lines_between_its_ends_but_never_a_hunk_header() {
        let l = parse(DIFF);
        let pick = Pick { range: Some((3, 0)), dragging: false, composing: false };
        assert_eq!((0..5).map(|i| pick.picked(&l, i)).collect::<Vec<_>>(), [false, true, true, true, false]);
        assert!(!Pick::default().picked(&l, 1));
    }

    #[test]
    fn plus_on_a_picked_line_opens_the_composer_for_the_whole_pick() {
        let l = parse(DIFF);
        let mut pick = Pick { range: Some((1, 3)), dragging: false, composing: false };
        assert!(!pick.open(&l, 2));
        assert_eq!(pick, Pick { range: Some((1, 3)), dragging: false, composing: true });
    }

    #[test]
    fn plus_on_another_line_opens_the_composer_for_that_line_alone() {
        let l = parse(DIFF);
        let mut pick = Pick { range: Some((1, 2)), dragging: false, composing: true };
        assert!(pick.open(&l, 4));
        assert_eq!(pick, Pick { range: Some((4, 4)), dragging: false, composing: true });
    }

    #[test]
    fn composer_sits_under_the_last_picked_line_once_let_go() {
        let open = Pick { range: Some((3, 1)), dragging: false, composing: true };
        assert_eq!(open.composer_line(), Some(3));
        assert_eq!(Pick { dragging: true, ..open }.composer_line(), None);
        assert_eq!(Pick { composing: false, ..open }.composer_line(), None);
    }

    #[test]
    fn cancelling_drops_the_pick_and_closes_the_composer() {
        let mut pick = Pick { range: Some((1, 3)), dragging: true, composing: true };
        pick.cancel();
        assert_eq!(pick, Pick { range: None, dragging: true, composing: false });
    }

    #[test]
    fn a_pick_is_held_from_the_press_until_let_go_or_dropped() {
        let l = parse(DIFF);
        let mut pick = Pick::default();
        pick.press(&l, 1, false);
        assert!(pick.held());
        pick.cancel();
        assert!(!pick.held());
        pick.press(&l, 1, false);
        pick.release();
        assert!(!pick.held());
    }

    #[test]
    fn pick_follows_its_lines_when_the_diff_refreshes() {
        let (old, new) = (parse(DIFF), parse("@@ -1,2 +1,4 @@\n z\n a\n-b\n+c\n+d\n"));
        let mut pick = Pick { range: Some((2, 3)), dragging: false, composing: true };
        pick.remap(&old, &new);
        assert_eq!(pick, Pick { range: Some((3, 4)), dragging: false, composing: true });
    }

    #[test]
    fn pick_is_dropped_when_one_of_its_ends_leaves_the_diff() {
        let (old, new) = (parse(DIFF), parse("@@ -1,2 +1,2 @@\n a\n-b\n+c\n"));
        let mut pick = Pick { range: Some((1, 4)), dragging: false, composing: true };
        pick.remap(&old, &new);
        assert_eq!(pick.range, None);
    }

    #[test]
    fn labels_the_pick_by_its_new_lines_in_either_direction() {
        let l = parse(DIFF);
        assert_eq!(Pick { range: Some((4, 1)), ..Pick::default() }.label(&l).as_deref(), Some("Lines 1–3"));
        assert_eq!(Pick { range: Some((3, 3)), ..Pick::default() }.label(&l).as_deref(), Some("Line 2"));
        assert_eq!(Pick::default().label(&l), None);
    }

    #[test]
    fn comments_on_the_pick_by_its_new_lines() {
        let l = parse(DIFF);
        let pick = Pick { range: Some((4, 1)), ..Pick::default() };
        let comment = pick.comment(&l, "a.rs".into(), "Lines 1–3".into(), "why?".into(), 7);
        let want = Comment { path: "a.rs".into(), lines: (1, 3), old_side: false, label: "Lines 1–3".into(), text: "why?".into(), at: 7 };
        assert_eq!(comment, Some(want));
    }

    #[test]
    fn comments_on_removed_lines_alone_by_their_old_lines() {
        let l = parse(DIFF);
        let comment = Pick { range: Some((2, 2)), ..Pick::default() }.comment(&l, "a.rs".into(), "Line 2".into(), "gone?".into(), 7);
        assert_eq!(comment.map(|c| (c.lines, c.old_side)), Some(((2, 2), true)));
        assert_eq!(Pick { range: Some((0, 0)), ..Pick::default() }.comment(&l, "a.rs".into(), String::new(), String::new(), 7), None);
    }

    fn sent(path: &str, lines: (usize, usize), old_side: bool) -> Comment {
        Comment { path: path.into(), lines, old_side, label: String::new(), text: String::new(), at: 0 }
    }

    #[test]
    fn hangs_each_comment_of_the_file_under_its_last_line_on_its_side() {
        let l = parse(DIFF);
        let comments = [sent("a.rs", (1, 3), false), sent("b.rs", (1, 1), false), sent("a.rs", (2, 2), true)];
        assert_eq!(notes(&comments, Some("a.rs"), &l), vec![(4, 0), (2, 2)]);
        assert_eq!(notes(&comments, None, &l), vec![]);
    }

    #[test]
    fn comments_keep_their_line_number_when_the_diff_refreshes() {
        let comments = [sent("a.rs", (2, 2), false)];
        assert_eq!(notes(&comments, Some("a.rs"), &parse(DIFF)), vec![(3, 0)]);
        let refreshed = parse("@@ -1,2 +1,4 @@\n z\n a\n-b\n+c\n+d\n");
        assert_eq!(notes(&comments, Some("a.rs"), &refreshed), vec![(2, 0)]);
    }

    #[test]
    fn comments_whose_line_left_the_diff_are_not_shown() {
        let comments = [sent("a.rs", (3, 3), false), sent("a.rs", (2, 2), true)];
        assert_eq!(notes(&comments, Some("a.rs"), &parse("@@ -1,1 +1,1 @@\n a\n")), vec![]);
    }

    #[test]
    fn comments_go_to_the_picked_session_while_it_lives() {
        let live = ["new".to_string(), "open".into(), "picked".into()];
        assert_eq!(comment_target(Some("picked".into()), Some("open".into()), &live).as_deref(), Some("picked"));
    }

    #[test]
    fn comments_fall_back_to_the_open_session_then_the_newest() {
        let live = ["new".to_string(), "open".into()];
        assert_eq!(comment_target(Some("gone".into()), Some("open".into()), &live).as_deref(), Some("open"));
        assert_eq!(comment_target(Some("gone".into()), Some("elsewhere".into()), &live).as_deref(), Some("new"));
        assert_eq!(comment_target(None, None, &[]), None);
    }

    #[test]
    fn places_the_composer_under_its_line() {
        let l = parse(DIFF);
        assert_eq!(rows(&l, false, Some(3), &[]), vec![Row::Unified(0), Row::Unified(1), Row::Unified(2), Row::Unified(3), Row::Composer, Row::Unified(4)]);
        assert_eq!(rows(&l, true, Some(2), &[]), vec![Row::Split(Some(0), Some(0)), Row::Split(Some(1), Some(1)), Row::Split(Some(2), Some(3)), Row::Composer, Row::Split(None, Some(4))]);
        assert_eq!(rows(&l, false, None, &[]).len(), 5);
    }

    #[test]
    fn places_sent_comments_before_the_composer() {
        let l = parse(DIFF);
        assert_eq!(rows(&l, false, Some(3), &[(3, 0)])[4..], [Row::Comment(0), Row::Composer, Row::Unified(4)]);
    }

    #[test]
    fn labels_ranges_by_their_new_lines() {
        let l = parse(DIFF);
        assert_eq!(label(&l, 1..=1).as_deref(), Some("Line 1"));
        assert_eq!(label(&l, 0..=4).as_deref(), Some("Lines 1–3"));
        assert_eq!(label(&l, 2..=2).as_deref(), Some("Line 2"));
        assert_eq!(label(&l, 0..=0), None);
    }

    #[test]
    fn remaps_lines_when_the_diff_refreshes() {
        let old = parse(DIFF);
        let new = parse("@@ -1,2 +1,4 @@\n z\n a\n-b\n+c\n+d\n");
        assert_eq!(remap(&old, &new, 2), Some(3));
        assert_eq!(remap(&old, &parse("@@ -1,1 +1,1 @@\n-x\n+y\n"), 2), None);
    }

    #[test]
    fn splices_only_the_rows_that_changed() {
        let old = [Row::Unified(0), Row::Unified(1), Row::Unified(2)];
        assert_eq!(changed(&old, &[Row::Unified(0), Row::Unified(1), Row::Composer, Row::Unified(2)]), (2..2, 1));
        assert_eq!(changed(&old, &[Row::Unified(0), Row::Unified(2)]), (1..2, 0));
        assert_eq!(changed(&old, &old), (3..3, 0));
        assert_eq!(changed(&[], &old), (0..0, 3));
    }

    fn color(c: Token) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
        vec![(0..1, HighlightStyle { color: Some(c.into()), ..Default::default() })]
    }

    #[test]
    fn tints_changed_words_in_lines_replaced_one_for_one() {
        let l = parse("@@ -1,2 +1,2 @@\n-let a = 1;\n+let b = 1;\n ctx\n");
        let hl = highlights(&l, &[], &[]);
        let tint = |i: usize| hl[i].iter().filter_map(|(r, s)| Some((r.clone(), s.background_color?))).collect::<Vec<_>>();
        assert_eq!(tint(1), vec![(4..5, DIFF_DEL_WORD.into())]);
        assert_eq!(tint(2), vec![(4..5, DIFF_ADD_WORD.into())]);
        assert!(hl[3].is_empty());
    }

    #[test]
    fn takes_syntax_from_each_side_and_skips_uneven_runs() {
        let l = parse("@@ -2,1 +2,2 @@\n-x\n+y\n+z\n");
        let hl = highlights(&l, &[vec![], color(SYN_KEYWORD)], &[vec![], color(SYN_FN), color(SYN_STRING)]);
        assert_eq!(hl[1][0].1.color, Some(SYN_KEYWORD.into()));
        assert_eq!(hl[2][0].1.color, Some(SYN_FN.into()));
        assert_eq!(hl[3][0].1.color, Some(SYN_STRING.into()));
        assert!(hl.iter().flatten().all(|(_, s)| s.background_color.is_none()));
    }

    #[test]
    fn drops_syntax_that_no_longer_fits_the_line() {
        let l = parse("@@ -1,1 +1,1 @@\n a\n");
        let long = vec![(0..5, HighlightStyle { color: Some(SYN_FN.into()), ..Default::default() })];
        assert!(highlights(&l, &[], &[long])[1].is_empty());
    }

    fn load(path: &str, text: &str) -> DiffLoad {
        DiffLoad { path: path.into(), at: None, open: HashSet::new(), lines: parse(text), source: Default::default(), colors: None, dark: false }
    }

    fn showing(panes: &[(PaneId, &str)]) -> DiffState<()> {
        let mut state = DiffState::with(());
        for &(pane, path) in panes {
            state.select(pane, path.into(), None);
            state.apply(pane, load(path, DIFF));
        }
        state
    }

    #[test]
    fn a_diff_load_lands_only_in_a_pane_still_showing_its_file() {
        let mut state = showing(&[(MAIN, "a.rs")]);
        state.select(OTHER, "b.rs".into(), None);
        assert!(!state.apply(OTHER, load("a.rs", DIFF)));
        assert!(state.apply(OTHER, load("b.rs", DIFF)));
        assert_eq!(state.view(MAIN).unwrap().lines, parse(DIFF));
        assert!(state.view(OTHER).unwrap().shows("b.rs", None));
    }

    #[test]
    fn starting_a_comment_in_one_pane_drops_the_pick_in_another() {
        let mut state = showing(&[(MAIN, "a.rs"), (OTHER, "a.rs")]);
        state.draft_in(MAIN);
        state.panes.get_mut(&MAIN).unwrap().pick.open(&parse(DIFF), 2);
        state.layout(MAIN, false);
        assert!(state.view(MAIN).unwrap().rows.contains(&Row::Composer));
        state.draft_in(OTHER);
        assert_eq!(state.view(MAIN).unwrap().pick, Pick::default());
        assert!(!state.view(MAIN).unwrap().rows.contains(&Row::Composer));
        assert_eq!(state.drafting, OTHER);
    }

    #[test]
    fn switching_to_split_lays_out_every_pane() {
        let mut state = showing(&[(MAIN, "a.rs"), (OTHER, "b.rs")]);
        state.split = true;
        state.relayout(true);
        assert!(state.panes.values().all(|v| v.rows.iter().all(|r| matches!(r, Row::Split(..)))));
    }

    #[test]
    fn a_sent_comment_shows_in_every_pane_showing_its_file() {
        let mut state = showing(&[(MAIN, "a.rs"), (OTHER, "a.rs"), (2, "b.rs")]);
        state.add_comment(sent("a.rs", (3, 3), false));
        assert!(state.view(MAIN).unwrap().rows.contains(&Row::Comment(0)));
        assert!(state.view(OTHER).unwrap().rows.contains(&Row::Comment(0)));
        assert!(!state.view(2).unwrap().rows.contains(&Row::Comment(0)));
    }

    #[test]
    fn a_closed_panes_diff_is_dropped() {
        let mut state = showing(&[(MAIN, "a.rs"), (OTHER, "b.rs")]);
        state.retain_panes(&[MAIN]);
        assert!(state.view(OTHER).is_none() && state.view(MAIN).is_some());
    }
}
