mod row;
pub(crate) use row::{code, hunk};

use git::{self, Kind, Line};
use theme::*;
use ui::{self, Segment, Variant, checkbox, dot};
use crate::add_to_chat::{Body, Quote};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Side, doc_bar, empty};
use crate::explorer::status_word;
use crate::syntax::{Spans, language_for, line_spans};
use crate::util::{ago_long, now_ms};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::ops::{Range, RangeInclusive};
use workspace::Doc;
use workspace::tree::PaneId;

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
}

/// Lays out the diff, with the composer under the row holding its line.
fn rows(lines: &[Line], split: bool, composer: Option<usize>) -> Vec<Row> {
    let base: Vec<Row> =
        if split { git::split(lines).into_iter().map(|(l, r)| Row::Split(l, r)).collect() } else { (0..lines.len()).map(Row::Unified).collect() };
    let mut out = Vec::with_capacity(base.len() + 1);
    for row in base {
        out.push(row);
        let holds = |i: usize| match row {
            Row::Unified(j) => j == i,
            Row::Split(l, r) => l == Some(i) || r == Some(i),
            Row::Composer => false,
        };
        if composer.is_some_and(holds) {
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

/// Where line `i` of `old` sits in `new`, so a selection survives the diff refreshing under it.
fn remap(old: &[Line], new: &[Line], i: usize) -> Option<usize> {
    let l = old.get(i)?;
    let same = |n: &&Line| n.kind == l.kind && n.text == l.text;
    new.iter().position(|n| n == l).or_else(|| new.iter().enumerate().filter(|(_, n)| same(n)).min_by_key(|(j, _)| j.abs_diff(i)).map(|(j, _)| j))
}

fn same_hunk(lines: &[Line], a: usize, b: usize) -> bool {
    !lines[a.min(b)..=a.max(b)].iter().any(|l| l.kind == Kind::Hunk)
}

/// The diff lines picked to ask about: `range` runs from where the pick started to where it ends.
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

    pub fn quote(&self, lines: &[Line], path: &str) -> Option<Quote> {
        let range = ordered(self.range?);
        let (lo, hi, removed) = span(lines, range.clone())?;
        let text = lines[range]
            .iter()
            .filter(|l| l.kind != Kind::Hunk)
            .map(|l| format!("{}{}", match l.kind { Kind::Add => '+', Kind::Del => '-', _ => ' ' }, l.text))
            .collect::<Vec<_>>()
            .join("\n");
        Some(Quote { path: path.to_string(), body: Body::Diff { lines: (lo, hi), removed, text } })
    }

    pub fn composer_line(&self) -> Option<usize> {
        self.last().filter(|_| self.composing && !self.dragging)
    }
}

/// One pane's diff: the file it shows, its lines, and the lines picked there to ask about.
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
    fn layout(&mut self, reset: bool, split: bool) {
        let rows = rows(&self.lines, split, self.pick.composer_line());
        if reset {
            self.list.reset(rows.len());
        } else {
            let (range, count) = changed(&self.rows, &rows);
            self.list.splice(range, count);
        }
        self.rows = rows;
    }
}

/// The diff each pane shows, and the pane whose pick is being asked about.
#[derive(Default)]
pub struct DiffState {
    pub(crate) panes: HashMap<PaneId, DiffView>,
    pub(crate) split: bool,
    /// The pane whose pick the composer is about; `MAIN` is 0, so it's the default.
    pub(crate) drafting: PaneId,
    pub(crate) viewed: HashSet<String>,
}

impl DiffState {
    pub fn view(&self, pane: PaneId) -> Option<&DiffView> {
        self.panes.get(&pane)
    }

    /// The pane the composer is in.
    pub fn draft(&self) -> Option<&DiffView> {
        self.panes.get(&self.drafting)
    }

    pub fn draft_quote(&self) -> Option<Quote> {
        let v = self.draft().filter(|v| v.pick.composing)?;
        v.pick.quote(&v.lines, v.working_file()?)
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
            v.layout(true, self.split);
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
            v.layout(true, self.split);
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
            v.layout(false, self.split);
        }
    }

    pub fn layout(&mut self, pane: PaneId, reset: bool) {
        if let Some(v) = self.panes.get_mut(&pane) {
            v.layout(reset, self.split);
        }
    }

    pub fn relayout(&mut self, reset: bool) {
        for v in self.panes.values_mut() {
            v.layout(reset, self.split);
        }
    }

    /// Moves the composer to `pane`, dropping the pick another pane held.
    pub fn draft_in(&mut self, pane: PaneId) {
        self.drafting = pane;
        for (_, v) in self.panes.iter_mut().filter(|(p, v)| **p != pane && v.pick != Pick::default()) {
            v.pick = Pick::default();
            v.layout(false, self.split);
        }
    }

    pub fn cancel_draft(&mut self) {
        if let Some(v) = self.panes.get_mut(&self.drafting) {
            v.pick.cancel();
            v.layout(false, self.split);
        }
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

    /// Picks line `i` of `pane`'s diff, or with `extend` stretches the pick to it.
    pub fn select_line(&mut self, pane: PaneId, i: usize, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.chat.file = None;
        self.diff.draft_in(pane);
        let Some(v) = self.diff.panes.get_mut(&pane) else { return };
        if v.pick.press(&v.lines, i, extend) {
            self.chat.input.update(cx, |s, cx| s.set_value("", window, cx));
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
            self.chat.input.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    pub fn open_composer(&mut self, pane: PaneId, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.chat.file = None;
        self.diff.draft_in(pane);
        let Some(v) = self.diff.panes.get_mut(&pane) else { return };
        if v.pick.open(&v.lines, i) {
            self.chat.input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.diff.layout(pane, false);
        self.chat.input.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{DiffLoad, DiffState, Pick, Row, changed, highlights, remap, rows};
    use crate::add_to_chat::{Body, Quote};
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
    fn places_the_composer_under_its_line() {
        let l = parse(DIFF);
        assert_eq!(rows(&l, false, Some(3)), vec![Row::Unified(0), Row::Unified(1), Row::Unified(2), Row::Unified(3), Row::Composer, Row::Unified(4)]);
        assert_eq!(rows(&l, true, Some(2)), vec![Row::Split(Some(0), Some(0)), Row::Split(Some(1), Some(1)), Row::Split(Some(2), Some(3)), Row::Composer, Row::Split(None, Some(4))]);
        assert_eq!(rows(&l, false, None).len(), 5);
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

    fn showing(panes: &[(PaneId, &str)]) -> DiffState {
        let mut state = DiffState::default();
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
    fn asking_in_one_pane_drops_the_pick_in_another() {
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
    fn a_closed_panes_diff_is_dropped() {
        let mut state = showing(&[(MAIN, "a.rs"), (OTHER, "b.rs")]);
        state.retain_panes(&[MAIN]);
        assert!(state.view(OTHER).is_none() && state.view(MAIN).is_some());
    }

    #[test]
    fn a_pick_quotes_its_lines_with_their_signs_numbered_on_the_new_side() {
        let l = parse(DIFF);
        let quote = Pick { range: Some((4, 1)), ..Pick::default() }.quote(&l, "a.rs");
        assert_eq!(quote, Some(Quote { path: "a.rs".into(), body: Body::Diff { lines: (1, 3), removed: false, text: " a\n-b\n+c\n+d".into() } }));
    }

    #[test]
    fn a_pick_of_removed_lines_alone_quotes_their_old_numbers() {
        let l = parse(DIFF);
        let quote = Pick { range: Some((2, 2)), ..Pick::default() }.quote(&l, "a.rs");
        assert_eq!(quote.map(|q| q.body), Some(Body::Diff { lines: (2, 2), removed: true, text: "-b".into() }));
        assert_eq!(Pick { range: Some((0, 0)), ..Pick::default() }.quote(&l, "a.rs"), None);
    }

    #[test]
    fn a_draft_quotes_its_pick_only_while_composing() {
        let mut state = showing(&[(MAIN, "a.rs")]);
        state.panes.get_mut(&MAIN).unwrap().pick = Pick { range: Some((3, 3)), dragging: false, composing: false };
        assert_eq!(state.draft_quote(), None);
        state.panes.get_mut(&MAIN).unwrap().pick.composing = true;
        assert_eq!(state.draft_quote().map(|q| q.path), Some("a.rs".to_string()));
    }
}
