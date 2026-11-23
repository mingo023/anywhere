mod row;
use row::{add_button, fold_start};
pub(crate) use row::{code, hunk};

use crate::add_to_chat::{Body, Quote};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Side, doc_bar, empty};
use crate::explorer::preview::image_format;
use crate::syntax::{Spans, language_for, line_spans};
use crate::util::{ago_long, now_ms};
use git::github::Thread;
use git::{self, Kind, Line};
use gpui_kit::component::input::TextareaState;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::ops::{Range, RangeInclusive};
use std::sync::Arc;
use theme::*;
use ui::{self, Segment};
use workspace::Doc;
use workspace::tree::PaneId;

const NUM: f32 = 44.;
const SIGN: f32 = 18.;
pub(crate) const ROW: f32 = 22.;

/// Styles `d` as code in the chosen code font at `size` points, its lines never shorter than a row.
pub(crate) fn code_text(d: Div, size: u32) -> Div {
    d.font_family(code_font()).text_size(px(size as f32)).line_height(px(code_line(size)))
}

fn code_line(size: u32) -> f32 {
    (size as f32 * ROW / 12.5).round().max(ROW)
}

/// How far past the top a pane's rows can show, in pixels, before its first layout says: only the cards of rows within it are built.
const CARD_REACH: f32 = 2400.;
const CARD_SLACK: usize = 25;
const MAX_COLORED: usize = 512 * 1024;
const MAX_WORD_LINES: usize = 5;

type Sides = (Vec<Spans>, Vec<Spans>);
/// An image before and after; `None` on a side where it doesn't exist.
type Images = (Option<Arc<Image>>, Option<Arc<Image>>);

/// What a diff compares: the working tree with HEAD, a commit with its parent, HEAD with where it left a base branch, or HEAD with where it left another branch as the Changes panel's Compare does.
#[derive(Clone, Debug, Default, PartialEq)]
pub enum At {
    #[default]
    Working,
    Commit(String),
    Base(String),
    Compare(String),
}

impl At {
    /// Whether the review threads of a PR into `pr_base` belong on this diff: its lines are the PR's, from where the branch left that base to HEAD.
    fn pins_threads(&self, pr_base: Option<&str>) -> bool {
        match self {
            At::Base(_) => true,
            At::Compare(against) => pr_base == Some(against.as_str()),
            At::Working | At::Commit(_) => false,
        }
    }
}

pub struct DiffLoad {
    path: String,
    at: At,
    open: HashSet<usize>,
    options: git::Options,
    lines: Vec<Line>,
    source: (String, String),
    colors: Option<(Sides, Vec<Spans>)>,
    dark: bool,
    images: Option<Images>,
}

/// Reads and diffs `path` as `at` compares it, colouring it only when the lines differ from `shown`. Run off the UI thread.
pub fn read_diff(cwd: &str, path: String, at: At, open: HashSet<usize>, options: git::Options, shown: &[Line]) -> DiffLoad {
    let dark = theme::is_dark();
    let runner = hosts::runner_for(cwd);
    let (old, new) = match &at {
        At::Working => git::blobs(&runner, cwd, &path),
        At::Commit(sha) => git::blobs_at(&runner, cwd, sha, &path),
        At::Base(base) => git::blobs_between(&runner, cwd, base, &path),
        At::Compare(against) => git::compare::blobs(&runner, cwd, against, &path),
    };
    if let Some(format) = image_format(&path) {
        let image = |bytes: Vec<u8>| (!bytes.is_empty()).then(|| Arc::new(Image::from_bytes(format, bytes)));
        let images = Some((image(old), image(new)));
        return DiffLoad { path, at, open, options, lines: Vec::new(), source: Default::default(), colors: None, dark, images };
    }
    let (old, new) = (String::from_utf8_lossy(&old).into_owned(), String::from_utf8_lossy(&new).into_owned());
    let lines = git::diff_texts(&old, &new, &open, options);
    let colors = (lines != shown).then(|| {
        let syntax = syntax(&path, &old, &new);
        let hl = highlights(&lines, &syntax.0, &syntax.1);
        (syntax, hl)
    });
    DiffLoad { path, at, open, options, lines, source: (old, new), colors, dark, images: None }
}

/// How the diff settings read the diff.
pub fn options(prefs: &store::prefs::Diff) -> git::Options {
    git::Options { context: prefs.context, ignore_whitespace: prefs.ignore_whitespace }
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
    /// The review thread pinned at this index.
    Thread(usize),
}

/// A review thread left on line `line` of the PR's head, or of its base when `left`.
#[derive(Clone, Debug, PartialEq)]
pub struct Pin {
    pub(crate) id: String,
    pub(crate) line: usize,
    pub(crate) left: bool,
}

/// The threads on `path` that still point at a line; an outdated one's line is in code the branch has since changed.
pub fn pins(path: &str, threads: &[Thread]) -> Vec<Pin> {
    threads.iter().filter(|t| t.path == path && !t.outdated).filter_map(|t| Some(Pin { id: t.id.clone(), line: t.line? as usize, left: t.left })).collect()
}

fn pinned(lines: &[Line], pin: &Pin) -> Option<usize> {
    lines.iter().position(|l| match l.kind {
        Kind::Hunk => false,
        Kind::Del => pin.left && l.old == Some(pin.line),
        Kind::Add => !pin.left && l.new == Some(pin.line),
        Kind::Context => (if pin.left { l.old } else { l.new }) == Some(pin.line),
    })
}

/// Lays out the diff, with the composer and then the pinned threads under the row holding their line.
fn rows(lines: &[Line], split: bool, composer: Option<usize>, pins: &[Pin]) -> Vec<Row> {
    let base: Vec<Row> =
        if split { git::split(lines).into_iter().map(|(l, r)| Row::Split(l, r)).collect() } else { (0..lines.len()).map(Row::Unified).collect() };
    let at: Vec<Option<usize>> = pins.iter().map(|p| pinned(lines, p)).collect();
    let mut out = Vec::with_capacity(base.len() + 1 + pins.len());
    for row in base {
        out.push(row);
        let holds = |i: usize| match row {
            Row::Unified(j) => j == i,
            Row::Split(l, r) => l == Some(i) || r == Some(i),
            Row::Composer | Row::Thread(_) => false,
        };
        if composer.is_some_and(holds) {
            out.push(Row::Composer);
        }
        out.extend(at.iter().enumerate().filter(|(_, i)| i.is_some_and(holds)).map(|(k, _)| Row::Thread(k)));
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
            .map(|l| {
                format!(
                    "{}{}",
                    match l.kind {
                        Kind::Add => '+',
                        Kind::Del => '-',
                        _ => ' ',
                    },
                    l.text
                )
            })
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
    pub(crate) at: At,
    pub(crate) lines: Vec<git::Line>,
    pub(crate) rows: Vec<Row>,
    pub(crate) list: ListState,
    pub(crate) hl: Vec<Spans>,
    pub(crate) open: HashSet<usize>,
    pub(crate) source: (String, String),
    pub(crate) syntax: (Vec<Spans>, Vec<Spans>),
    pub(crate) pick: Pick,
    pub(crate) pins: Vec<Pin>,
    pub(crate) images: Option<Images>,
}

impl Default for DiffView {
    fn default() -> Self {
        Self {
            file: None,
            at: At::Working,
            lines: Vec::new(),
            rows: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            hl: Vec::new(),
            open: HashSet::new(),
            source: Default::default(),
            syntax: Default::default(),
            pick: Pick::default(),
            pins: Vec::new(),
            images: None,
        }
    }
}

impl DiffView {
    /// Whether this shows `path` as `at` compares it.
    pub fn shows(&self, path: &str, at: &At) -> bool {
        self.file.as_deref() == Some(path) && self.at == *at
    }

    pub fn working_file(&self) -> Option<&str> {
        self.file.as_deref().filter(|_| self.at == At::Working)
    }

    /// The file shown when its new side is this worktree's working tree or HEAD, so its diff follows git refreshes and its lines quote into a chat.
    pub fn followed_file(&self) -> Option<&str> {
        self.file.as_deref().filter(|_| matches!(self.at, At::Working | At::Compare(_)))
    }

    /// What the pick asks about, once the composer is open.
    pub fn quote(&self) -> Option<Quote> {
        self.pick.composing.then(|| self.pick.quote(&self.lines, self.followed_file()?)).flatten()
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
    pub(crate) fn layout(&mut self, reset: bool, split: bool) {
        let rows = rows(&self.lines, split, self.pick.composer_line(), &self.pins);
        if reset {
            self.list.reset(rows.len());
        } else {
            let (range, count) = changed(&self.rows, &rows);
            self.list.splice(range, count);
        }
        self.rows = rows;
    }

    /// Shows `load` unless another file, fold state, diff option or appearance was picked while it ran.
    fn apply(&mut self, load: DiffLoad, options: git::Options, split: bool) -> bool {
        if self.file.as_ref() != Some(&load.path)
            || self.at != load.at
            || load.open != self.open
            || load.options != options
            || load.colors.is_some() && load.dark != theme::is_dark()
        {
            return false;
        }
        self.source = load.source;
        let recolored = load.colors.map(|(syntax, hl)| (self.syntax, self.hl) = (syntax, hl)).is_some();
        let reimaged = self.images != load.images;
        if reimaged {
            self.images = load.images;
        }
        let changed = self.set_lines(load.lines);
        if changed {
            self.layout(true, split);
        }
        changed | recolored | reimaged
    }

    /// Shows `path` as `at` compares it, dropping the pick, folds and lines of another file.
    fn select(&mut self, path: String, at: At, split: bool) {
        if self.shows(&path, &at) {
            return;
        }
        self.pick.range = None;
        self.open.clear();
        self.pins.clear();
        self.images = None;
        if self.set_lines(Vec::new()) {
            self.layout(true, split);
        }
        (self.file, self.at) = (Some(path), at);
    }

    /// Shows the unchanged lines folded away from `start` on, keeping the scroll position.
    fn expand(&mut self, start: usize, options: git::Options, split: bool) {
        self.open.insert(start);
        let lines = git::diff_texts(&self.source.0, &self.source.1, &self.open, options);
        self.hl = highlights(&lines, &self.syntax.0, &self.syntax.1);
        if self.set_lines(lines) {
            self.layout(false, split);
        }
    }

    /// Pins `threads`, of a PR into `pr_base`, when this is a PR diff, keeping the scroll position.
    fn pin(&mut self, threads: &[Thread], pr_base: Option<&str>, split: bool) {
        if !self.at.pins_threads(pr_base) {
            return;
        }
        let pins = self.file.as_deref().map(|f| pins(f, threads)).unwrap_or_default();
        if pins != self.pins {
            self.pins = pins;
            self.layout(false, split);
        }
    }

    /// Drops the pick another pane's composer took over.
    fn drop_pick(&mut self, split: bool) {
        if self.pick != Pick::default() {
            self.pick = Pick::default();
            self.layout(false, split);
        }
    }

    /// Where thread `id` sits in the rows, once laid out.
    fn thread_row(&self, id: &str) -> Option<usize> {
        self.rows.iter().position(|r| matches!(r, Row::Thread(k) if self.pins[*k].id == id))
    }
}

/// Scrolls to the thread asked for, with a few lines of the code above it, once a pane shows it.
fn reveal_thread<'a>(reveal: &mut Option<String>, views: impl IntoIterator<Item = &'a DiffView>) {
    let Some(id) = reveal.as_deref() else { return };
    let found = views.into_iter().find_map(|v| Some((v, v.thread_row(id)?)));
    if let Some((v, ix)) = found {
        v.list.scroll_to(ListOffset { item_ix: ix.saturating_sub(3), offset_in_item: px(0.) });
        *reveal = None;
    }
}

/// Elements `Desktop` builds from its own state for the rows a pane can't: the composer and the PR threads. A row takes its card once; one not built draws empty.
#[derive(Default)]
pub(crate) struct Cards {
    pub(crate) composer: Option<AnyElement>,
    pub(crate) threads: HashMap<usize, AnyElement>,
}

pub(crate) enum DiffEvent {
    /// A line was picked or its "+" pressed: the composer is about this pane now.
    Claimed,
}

/// One pane's diff view as its own `Entity`: its rows and the picks made in them. `Desktop` hands it the layout, options, code size and cards each frame.
pub struct DiffPane {
    pub(crate) view: DiffView,
    pub(crate) split: bool,
    pub(crate) options: git::Options,
    pub(crate) code_size: u32,
    pub(crate) cards: Cards,
    chat: Entity<TextareaState>,
}

impl EventEmitter<DiffEvent> for DiffPane {}

impl DiffPane {
    fn new(chat: Entity<TextareaState>) -> Self {
        Self { view: DiffView::default(), split: false, options: git::Options::default(), code_size: 0, cards: Cards::default(), chat }
    }

    /// One side of a row: pressing picks its line, dragging or shift-clicking stretches the pick; "+" or a drag opens the composer.
    fn cell(&self, id: &'static str, i: usize, numbers: Vec<Option<usize>>, add_at: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let v = &self.view;
        if v.lines[i].kind == Kind::Hunk {
            return self.fold(id, i, cx);
        }
        let row = code(&v.lines[i], v.hl.get(i), numbers, v.pick.picked(&v.lines, i)).id((id, i));
        if v.followed_file().is_none() {
            return row;
        }
        let last = v.pick.last() == Some(i);
        row.group("diff-line")
            .cursor_pointer()
            .child(
                add_button(add_at)
                    .when(!last, |b| b.opacity(0.).group_hover("diff-line", |s| s.opacity(1.)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| this.open_composer(i, window, cx))),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, window, cx| this.select_line(i, ev.modifiers.shift, window, cx)))
            .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, window, cx| this.drag_to(i, ev.dragging(), window, cx)))
    }

    /// A hunk header; clicking it unfolds the lines hidden above it.
    fn fold(&self, id: &'static str, i: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let row = hunk(&self.view.lines, i).id((id, i));
        match fold_start(&self.view.lines, i) {
            Some(start) => row.cursor_pointer().hover(|s| s.bg(FILL_2)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.expand(start, cx))),
            None => row,
        }
    }

    fn row(&mut self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(&row) = self.view.rows.get(ix) else { return Empty.into_any_element() };
        match row {
            Row::Unified(i) => self.cell("line", i, vec![self.view.lines[i].old, self.view.lines[i].new], 2. * NUM - 10., cx).w_full().into_any_element(),
            Row::Split(Some(i), _) | Row::Split(None, Some(i)) if self.view.lines[i].kind == Kind::Hunk => self.fold("fold", i, cx).w_full().into_any_element(),
            Row::Split(l, r) => {
                let lines = &self.view.lines;
                let mut side = |id, i: Option<usize>, n: fn(&Line) -> Option<usize>| match i {
                    Some(i) => self.cell(id, i, vec![n(&lines[i])], NUM - 10., cx).flex_1().min_w_0().into_any_element(),
                    None => div().flex_1().min_h(px(ROW)).bg(SURFACE_SUNKEN).into_any_element(),
                };
                div().w_full().flex().child(side("old", l, |l| l.old)).child(side("new", r, |l| l.new)).into_any_element()
            }
            Row::Composer => div().w_full().children(self.cards.composer.take()).into_any_element(),
            Row::Thread(k) => div().w_full().children(self.cards.threads.remove(&k)).into_any_element(),
        }
    }

    fn expand(&mut self, start: usize, cx: &mut Context<Self>) {
        self.view.expand(start, self.options, self.split);
        cx.notify();
    }

    fn select_line(&mut self, i: usize, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.view.pick.press(&self.view.lines, i, extend) {
            self.chat.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.view.layout(false, self.split);
        cx.emit(DiffEvent::Claimed);
        cx.notify();
    }

    fn drag_to(&mut self, i: usize, pressed: bool, window: &mut Window, cx: &mut Context<Self>) {
        if !pressed && self.view.pick.held() {
            return self.end_drag(window, cx);
        }
        if self.view.pick.drag(&self.view.lines, i) {
            cx.notify();
        }
    }

    fn end_drag(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.view.pick.release() {
            return;
        }
        self.view.layout(false, self.split);
        if self.view.pick.composing {
            self.chat.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    pub(crate) fn open_composer(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if self.view.pick.open(&self.view.lines, i) {
            self.chat.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.view.layout(false, self.split);
        self.chat.update(cx, |s, cx| s.focus(window, cx));
        cx.emit(DiffEvent::Claimed);
        cx.notify();
    }
}

impl Render for DiffPane {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if let Some(images) = self.view.images.clone() {
            return image_sides(images).into_any_element();
        }
        let rows = list(self.view.list.clone(), cx.processor(|this, ix, _, cx| this.row(ix, cx))).pb(px(8.));
        code_text(div(), self.code_size)
            .flex_1()
            .min_h_0()
            .bg(PAGE)
            .border_t(px(0.5))
            .border_color(SEPARATOR)
            .overflow_hidden()
            .child(rows.size_full())
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, window, cx| this.end_drag(window, cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, window, cx| this.end_drag(window, cx)))
            .into_any_element()
    }
}

/// A pane's diff and what keeps its events coming.
pub(crate) struct Slot {
    pub(crate) view: Entity<DiffPane>,
    _sub: Subscription,
}

/// The diff each pane shows, the diff settings they share, and the pane whose pick is being asked about.
#[derive(Default)]
pub struct DiffState {
    pub(crate) panes: HashMap<PaneId, Slot>,
    pub(crate) split: bool,
    pub(crate) options: git::Options,
    /// The pane whose pick the composer is about; `MAIN` is 0, so it's the default.
    pub(crate) drafting: PaneId,
    /// The thread to scroll to once a pane lays it out.
    pub(crate) reveal: Option<String>,
}

impl DiffState {
    pub fn new(prefs: &store::prefs::Diff) -> Self {
        Self { split: prefs.split, options: options(prefs), ..Self::default() }
    }

    pub fn get(&self, pane: PaneId) -> Option<&Entity<DiffPane>> {
        self.panes.get(&pane).map(|s| &s.view)
    }

    pub fn view<'a>(&self, pane: PaneId, cx: &'a App) -> Option<&'a DiffView> {
        self.get(pane).map(|p| &p.read(cx).view)
    }

    /// What the pane the composer is in asks about.
    pub fn draft_quote(&self, cx: &App) -> Option<Quote> {
        self.view(self.drafting, cx)?.quote()
    }

    /// The panes' views, in no order.
    pub(crate) fn views<'a>(&'a self, cx: &'a App) -> impl Iterator<Item = (PaneId, &'a DiffView)> {
        self.panes.iter().map(|(p, s)| (*p, &s.view.read(cx).view))
    }

    /// Shows `load` in `pane` unless another file, fold state, diff option or appearance was picked while it ran.
    pub fn apply(&mut self, pane: PaneId, load: DiffLoad, cx: &mut App) -> bool {
        let Some(entity) = self.get(pane).cloned() else { return false };
        let (options, split) = (self.options, self.split);
        let changed = entity.update(cx, |p, cx| {
            let changed = p.view.apply(load, options, split);
            if changed {
                cx.notify();
            }
            changed
        });
        self.reveal_thread(cx);
        changed
    }

    /// Pins `threads`, of a PR into `pr_base`, to the PR diffs on show, keeping their scroll position, then scrolls to the thread asked for.
    pub fn pin(&mut self, threads: &[Thread], pr_base: Option<&str>, cx: &mut App) {
        let split = self.split;
        for s in self.panes.values() {
            s.view.update(cx, |p, cx| {
                p.view.pin(threads, pr_base, split);
                cx.notify();
            });
        }
        self.reveal_thread(cx);
    }

    fn reveal_thread(&mut self, cx: &App) {
        let Self { panes, reveal, .. } = self;
        reveal_thread(reveal, panes.values().map(|s| &s.view.read(cx).view));
    }

    /// Lays out every pane's rows for the current layout.
    pub fn relayout(&self, reset: bool, cx: &mut App) {
        let split = self.split;
        for s in self.panes.values() {
            s.view.update(cx, |p, cx| {
                p.split = split;
                p.view.layout(reset, split);
                cx.notify();
            });
        }
    }

    /// Moves the composer to `pane`, dropping the pick another pane held.
    pub fn draft_in(&mut self, pane: PaneId, cx: &mut App) {
        self.drafting = pane;
        let split = self.split;
        for s in self.panes.iter().filter(|(p, _)| **p != pane).map(|(_, s)| s) {
            s.view.update(cx, |p, cx| {
                p.view.drop_pick(split);
                cx.notify();
            });
        }
    }

    pub fn cancel_draft(&self, cx: &mut App) {
        let split = self.split;
        if let Some(entity) = self.get(self.drafting) {
            entity.update(cx, |p, cx| {
                p.view.pick.cancel();
                p.view.layout(false, split);
                cx.notify();
            });
        }
    }

    pub fn retain_panes(&mut self, panes: &[PaneId]) {
        self.panes.retain(|p, _| panes.contains(p));
    }
}

/// An image changed: each side that exists, labelled Before and After when both do.
fn image_sides((old, new): Images) -> Div {
    let both = old.is_some() && new.is_some();
    let side =
        |label: &'static str, image: Arc<Image>| {
            div().flex_1().min_w_0().flex().flex_col().gap(px(8.)).when(both, |d| d.child(div().text_size(px(11.5)).text_color(TEXT_3).child(label))).child(
                div().flex_1().min_h_0().flex().items_center().justify_center().child(img(image).max_w_full().max_h_full().object_fit(ObjectFit::Contain)),
            )
        };
    div()
        .flex_1()
        .min_h_0()
        .flex()
        .gap(px(24.))
        .p(px(24.))
        .bg(SURFACE_SUNKEN)
        .border_t(px(0.5))
        .border_color(SEPARATOR)
        .children(old.map(|i| side("Before", i)))
        .children(new.map(|i| side("After", i)))
}

impl Desktop {
    pub(crate) fn diff_entity(&mut self, pane: PaneId, cx: &mut Context<Self>) -> Entity<DiffPane> {
        let (split, options, code_size) = (self.diff.split, self.diff.options, self.store.appearance.code_size());
        let entity = match self.diff.get(pane) {
            Some(entity) => entity.clone(),
            None => {
                let chat = self.chat.read(cx).input.clone();
                let entity = cx.new(|_| DiffPane::new(chat));
                let sub = cx.subscribe(&entity, move |this, _, ev: &DiffEvent, cx| this.on_diff_pane(pane, ev, cx));
                self.diff.panes.insert(pane, Slot { view: entity.clone(), _sub: sub });
                entity
            }
        };
        entity.update(cx, |p, _| (p.split, p.options, p.code_size) = (split, options, code_size));
        entity
    }

    fn on_diff_pane(&mut self, pane: PaneId, ev: &DiffEvent, cx: &mut Context<Self>) {
        match ev {
            DiffEvent::Claimed => {
                self.chat.update(cx, |c, cx| c.clear_file(cx));
                self.diff.draft_in(pane, cx);
            }
        }
        cx.notify();
    }

    /// Shows `path` in `pane` as `at` compares it, dropping the pick, folds and lines of another file.
    pub(crate) fn select_diff(&mut self, pane: PaneId, path: String, at: At, cx: &mut Context<Self>) {
        let split = self.diff.split;
        self.diff_entity(pane, cx).update(cx, |p, _| p.view.select(path, at, split));
    }

    pub fn diff_view(&mut self, pane: PaneId, cx: &mut Context<Self>) -> Div {
        let Some((path, at)) = self.diff.view(pane, cx).and_then(|v| Some((v.file.clone()?, v.at.clone()))) else {
            return empty("No changes.");
        };
        let file = self.repo(cx).and_then(|r| r.files.iter().find(|f| f.path == path)).cloned().filter(|_| at == At::Working);
        let image = image_format(&path).is_some();
        let right = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .when(!image, |d| {
                d.child(div().id("diff-mode").child(ui::segmented(
                    vec![
                        Segment { icon: None, value: false, label: "Unified".into(), badge: None },
                        Segment { icon: None, value: true, label: "Split".into(), badge: None },
                    ],
                    self.diff.split,
                    true,
                    false,
                    |this, v, cx| {
                        this.store.diff.split = v;
                        this.apply_diff_style(cx);
                        this.save_soon(cx);
                        cx.notify();
                    },
                    cx,
                )))
            })
            .child(ui::icon_group([
                ui::group_button("diff-more", "more").on_click(cx.listener(|this, e: &ClickEvent, window, cx| this.open_more(e.position(), window, cx)))
            ]));
        let mut meta = Vec::new();
        if let At::Commit(sha) = &at {
            meta.push(ui::meta_item().child(ui::meta_value(git::short_sha(sha).to_string())).into_any_element());
            if let Some(c) = self.graph.read(cx).commit(sha) {
                meta.push(ui::meta_item().child(div().max_w(px(320.)).truncate().child(c.subject.clone())).into_any_element());
            }
        }
        if let At::Base(against) | At::Compare(against) = &at {
            meta.push(ui::meta_item().child("against").child(ui::meta_value(against.clone())).into_any_element());
        }
        if let Some(f) = &file {
            meta.push(ui::meta_item().child(ui::meta_diff(f.added, f.removed, 11.5)).into_any_element());
        }
        if at == At::Working {
            let abs = self.cwd(cx).map(|c| format!("{c}/{path}")).unwrap_or_default();
            if let Some((a, ts)) = self.agents.last_edit(&abs) {
                let by = format!("{} · {}", self.agent_name(&a.provider), ago_long(ts, now_ms()));
                meta.push(ui::meta_item().child(provider_icon(&a.provider, 13., TEXT_2)).child("by").child(ui::meta_value(by)).into_any_element());
            }
        }
        let (dir, name) = path.rsplit_once('/').map_or((None, path.clone()), |(d, n)| (Some(d.to_string()), n.to_string()));
        let first = match &at {
            At::Working => "Changes".to_string(),
            At::Commit(sha) => git::short_sha(sha).to_string(),
            At::Base(_) => "Pull request".to_string(),
            At::Compare(_) => "Compare".to_string(),
        };
        let crumbs = std::iter::once(first).chain(dir).chain([name]).collect();
        let cards = self.diff_cards(pane, cx);
        let entity = self.diff_entity(pane, cx);
        entity.update(cx, |p, _| p.cards = cards);
        div().flex_1().min_h_0().flex().flex_col().child(doc_bar(crumbs, meta, right)).child(entity)
    }

    /// The composer and the PR threads in view, for the pane's rows to take: built from the chat and PR state the pane can't read.
    fn diff_cards(&mut self, pane: PaneId, cx: &mut Context<Self>) -> Cards {
        let Some(v) = self.diff.view(pane, cx) else { return Cards::default() };
        let composing = v.rows.contains(&Row::Composer);
        let top = v.list.logical_scroll_top().item_ix;
        let reach = (v.list.viewport_bounds().size.height.max(px(CARD_REACH)) / px(ROW)) as usize + CARD_SLACK;
        let threads: Vec<(usize, String)> = v
            .rows
            .iter()
            .skip(top.saturating_sub(CARD_SLACK))
            .take(reach + CARD_SLACK)
            .filter_map(|r| match r {
                Row::Thread(k) => Some((*k, v.pins.get(*k)?.id.clone())),
                _ => None,
            })
            .collect();
        let margin = if self.diff.split { NUM + SIGN } else { 2. * NUM + SIGN };
        let composer = composing
            .then(|| self.diff.draft_quote(cx))
            .flatten()
            .map(|q| self.chat_card(&q, cx).mt(px(6.)).mb(px(10.)).mr(px(20.)).ml(px(margin)).into_any_element());
        let threads = threads
            .into_iter()
            .filter_map(|(k, id)| {
                let t = self.pinned_thread(k, &id, cx)?;
                let card = div()
                    .mt(px(6.))
                    .mb(px(10.))
                    .mr(px(20.))
                    .ml(px(margin))
                    .max_w(px(720.))
                    .font_family(ui_font())
                    .text_size(px(13.))
                    .line_height(relative(1.2))
                    .child(t);
                Some((k, card.into_any_element()))
            })
            .collect();
        Cards { composer, threads }
    }

    pub fn open_changes(&mut self, path: Option<String>, pin: bool, cx: &mut Context<Self>) {
        let shown = self.diff.view(self.focused_pane(cx), cx).and_then(|v| v.working_file()).map(str::to_string);
        let path = path.or(shown).or_else(|| self.repo(cx)?.files.first().map(|f| f.path.clone()));
        (self.side, self.sidebar.panel_open) = (Side::Changes, true);
        match path {
            Some(path) => self.open_doc(Doc::Diff(path), pin, cx),
            None => cx.notify(),
        }
    }

    pub(crate) fn load_diff(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        let shown = self.diff.view(pane, cx).map(|v| v.lines.clone()).unwrap_or_default();
        self.read_diff_in_background(pane, shown, cx);
    }

    pub(crate) fn apply_diff_style(&mut self, cx: &mut Context<Self>) {
        self.diff.split = self.store.diff.split;
        self.diff.relayout(true, cx);
    }

    /// Reads every pane's diff again, for a new appearance or diff option.
    pub(crate) fn recolor_diff(&mut self, cx: &mut Context<Self>) {
        for pane in self.diff.panes.keys().copied().collect::<Vec<_>>() {
            self.read_diff_in_background(pane, Vec::new(), cx);
        }
    }

    fn read_diff_in_background(&mut self, pane: PaneId, shown: Vec<Line>, cx: &mut Context<Self>) {
        let Some((cwd, (path, at, open))) = self.cwd(cx).zip(self.diff.view(pane, cx).and_then(|v| Some((v.file.clone()?, v.at.clone(), v.open.clone()))))
        else {
            return;
        };
        let options = self.diff.options;
        let task = cx.background_executor().spawn(async move { read_diff(&cwd, path, at, open, options, &shown) });
        cx.spawn(async move |this, cx| {
            let load = task.await;
            this.update(cx, |d, cx| {
                if d.diff.apply(pane, load, cx) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    /// Picks the lines of `pane`'s diff from `i` on, as pressing "+" there does.
    pub fn open_composer(&mut self, pane: PaneId, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(entity) = self.diff.get(pane).cloned() {
            entity.update(cx, |p, cx| p.open_composer(i, window, cx));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{At, DiffLoad, DiffView, Pick, Pin, ROW, Row, changed, code_line, highlights, pins, remap, reveal_thread, rows};
    use crate::add_to_chat::{Body, Quote};
    use git::github::Thread;
    use git::parse;
    use gpui_kit::{HighlightStyle, Image, ImageFormat};
    use std::collections::HashSet;
    use std::sync::Arc;
    use theme::{DIFF_ADD_WORD, DIFF_DEL_WORD, SYN_FN, SYN_KEYWORD, SYN_STRING, Token};

    const DIFF: &str = "@@ -1,2 +1,3 @@\n a\n-b\n+c\n+d\n";
    const TWO_HUNKS: &str = "@@ -1,2 +1,2 @@\n a\n-b\n+c\n@@ -9,1 +9,1 @@\n-x\n+y\n";

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
        assert_eq!(rows(&l, false, Some(3), &[]), vec![Row::Unified(0), Row::Unified(1), Row::Unified(2), Row::Unified(3), Row::Composer, Row::Unified(4)]);
        assert_eq!(
            rows(&l, true, Some(2), &[]),
            vec![Row::Split(Some(0), Some(0)), Row::Split(Some(1), Some(1)), Row::Split(Some(2), Some(3)), Row::Composer, Row::Split(None, Some(4))]
        );
        assert_eq!(rows(&l, false, None, &[]).len(), 5);
    }

    fn pin(id: &str, line: usize, left: bool) -> Pin {
        Pin { id: id.into(), line, left }
    }

    #[test]
    fn a_thread_sits_under_the_line_it_was_left_on_in_either_layout() {
        let l = parse(DIFF);
        let pins = [pin("on-d", 3, false), pin("on-b", 2, true)];
        assert_eq!(
            rows(&l, false, None, &pins),
            vec![Row::Unified(0), Row::Unified(1), Row::Unified(2), Row::Thread(1), Row::Unified(3), Row::Unified(4), Row::Thread(0)]
        );
        assert_eq!(
            rows(&l, true, None, &pins),
            vec![
                Row::Split(Some(0), Some(0)),
                Row::Split(Some(1), Some(1)),
                Row::Split(Some(2), Some(3)),
                Row::Thread(1),
                Row::Split(None, Some(4)),
                Row::Thread(0)
            ]
        );
    }

    #[test]
    fn threads_on_one_line_follow_the_composer_in_the_order_they_came() {
        let l = parse(DIFF);
        let pins = [pin("first", 1, false), pin("second", 1, true)];
        assert_eq!(rows(&l, false, Some(1), &pins)[1..5], [Row::Unified(1), Row::Composer, Row::Thread(0), Row::Thread(1)]);
    }

    #[test]
    fn a_thread_on_a_line_the_diff_leaves_out_gets_no_row() {
        let l = parse(DIFF);
        assert_eq!(rows(&l, false, None, &[pin("folded", 40, false), pin("added-side", 3, true)]).len(), 5);
    }

    #[test]
    fn only_the_files_threads_on_lines_of_the_head_are_pinned() {
        let t = |id: &str, path: &str, line: Option<u32>, outdated: bool| Thread { id: id.into(), path: path.into(), line, outdated, ..Thread::default() };
        let threads =
            [t("here", "a.rs", Some(3), false), t("elsewhere", "b.rs", Some(3), false), t("gone", "a.rs", Some(7), true), t("unplaced", "a.rs", None, false)];
        assert_eq!(pins("a.rs", &threads), vec![pin("here", 3, false)]);
    }

    #[test]
    fn a_thread_asked_for_is_scrolled_to_once_its_file_is_laid_out() {
        let base = At::Base("origin/main".into());
        let mut v = DiffView::default();
        v.select("a.rs".into(), base.clone(), false);
        let mut reveal = Some("on-d".to_string());
        v.pin(&[Thread { id: "on-d".into(), path: "a.rs".into(), line: Some(3), ..Thread::default() }], Some("origin/main"), false);
        reveal_thread(&mut reveal, [&v]);
        assert_eq!(reveal.as_deref(), Some("on-d"));
        assert!(v.apply(DiffLoad { at: base, ..load("a.rs", DIFF) }, git::Options::default(), false));
        reveal_thread(&mut reveal, [&v]);
        assert_eq!(reveal, None);
        assert_eq!(v.rows[5], Row::Thread(0));
        assert_eq!(v.list.logical_scroll_top().item_ix, 2);
    }

    #[test]
    fn threads_pin_only_to_diffs_of_the_pr() {
        let mut v = showing("a.rs");
        v.pin(&[Thread { id: "t".into(), path: "a.rs".into(), line: Some(3), ..Thread::default() }], Some("origin/main"), false);
        assert!(v.pins.is_empty());
    }

    fn compare(against: &str) -> At {
        At::Compare(against.into())
    }

    #[test]
    fn threads_pin_to_a_compare_only_against_the_prs_base() {
        let thread = Thread { id: "t".into(), path: "a.rs".into(), line: Some(3), ..Thread::default() };
        for (at, pinned) in [(compare("origin/main"), true), (compare("origin/dev"), false)] {
            let mut v = DiffView::default();
            v.select("a.rs".into(), at.clone(), false);
            v.apply(DiffLoad { at: at.clone(), ..load("a.rs", DIFF) }, git::Options::default(), false);
            v.pin(std::slice::from_ref(&thread), Some("origin/main"), false);
            assert_eq!(!v.pins.is_empty(), pinned, "{at:?}");
        }
    }

    #[test]
    fn a_compare_diff_quotes_its_pick_into_a_chat() {
        let mut v = DiffView::default();
        v.select("a.rs".into(), compare("origin/main"), false);
        v.apply(DiffLoad { at: compare("origin/main"), ..load("a.rs", DIFF) }, git::Options::default(), false);
        v.pick = Pick { range: Some((3, 3)), dragging: false, composing: true };
        assert_eq!(v.quote().map(|q| q.path), Some("a.rs".to_string()));
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
        DiffLoad {
            path: path.into(),
            at: At::Working,
            open: HashSet::new(),
            options: git::Options::default(),
            lines: parse(text),
            source: Default::default(),
            colors: None,
            dark: false,
            images: None,
        }
    }

    fn showing(path: &str) -> DiffView {
        let mut v = DiffView::default();
        v.select(path.into(), At::Working, false);
        v.apply(load(path, DIFF), git::Options::default(), false);
        v
    }

    #[test]
    fn a_diff_load_lands_only_in_a_pane_still_showing_its_file() {
        let mut v = DiffView::default();
        v.select("b.rs".into(), At::Working, false);
        assert!(!v.apply(load("a.rs", DIFF), git::Options::default(), false));
        assert!(v.apply(load("b.rs", DIFF), git::Options::default(), false));
        assert!(v.shows("b.rs", &At::Working));
    }

    #[test]
    fn a_diff_read_before_the_options_changed_is_dropped() {
        let mut v = showing("a.rs");
        let options = git::Options { context: 5, ..git::Options::default() };
        assert!(!v.apply(DiffLoad { lines: Vec::new(), ..load("a.rs", DIFF) }, options, false));
        assert_eq!(v.lines, parse(DIFF));
    }

    #[test]
    fn asking_in_one_pane_drops_the_pick_in_another() {
        let mut v = showing("a.rs");
        v.pick.open(&parse(DIFF), 2);
        v.layout(false, false);
        assert!(v.rows.contains(&Row::Composer));
        v.drop_pick(false);
        assert_eq!(v.pick, Pick::default());
        assert!(!v.rows.contains(&Row::Composer));
    }

    #[test]
    fn an_image_shows_until_another_file_is_picked() {
        let mut v = showing("a.rs");
        let options = git::Options::default();
        v.select("logo.png".into(), At::Working, false);
        let png = Some(Arc::new(Image::from_bytes(ImageFormat::Png, b"\x89PNG".to_vec())));
        assert!(v.apply(DiffLoad { images: Some((None, png.clone())), ..load("logo.png", "") }, options, false));
        assert!(!v.apply(DiffLoad { images: Some((None, png.clone())), ..load("logo.png", "") }, options, false), "the same image again changes nothing");
        assert_eq!(v.images, Some((None, png)));
        v.select("a.rs".into(), At::Working, false);
        assert_eq!(v.images, None);
    }

    #[test]
    fn switching_to_split_lays_the_rows_out_in_pairs() {
        let mut v = showing("a.rs");
        assert!(v.rows.iter().all(|r| matches!(r, Row::Unified(..))));
        v.layout(true, true);
        assert!(v.rows.iter().all(|r| matches!(r, Row::Split(..))));
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
        let mut v = showing("a.rs");
        v.pick = Pick { range: Some((3, 3)), dragging: false, composing: false };
        assert_eq!(v.quote(), None);
        v.pick.composing = true;
        assert_eq!(v.quote().map(|q| q.path), Some("a.rs".to_string()));
    }

    #[test]
    fn code_lines_grow_with_the_code_size_but_never_below_a_row() {
        assert_eq!([10, 12, 14, 20].map(code_line), [ROW, ROW, 25., 35.]);
    }
}
