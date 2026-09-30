use git::{self, Kind, Line};
use theme::*;
use ui::{self, Segment, Variant, checkbox, dot};
use crate::explore::status_word;
use crate::syntax::{Spans, language_for, line_spans};
use crate::view::{ago_long, doc_bar, empty, now_ms};
use crate::{Comment, Desktop, Overlay};
use gpui_kit::component::input::{Escape, Textarea};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashSet;
use std::ops::{Range, RangeInclusive};

const NUM: f32 = 44.;
const SIGN: f32 = 18.;
const ROW: f32 = 22.;

const MAX_COLORED: usize = 512 * 1024;
const MAX_WORD_LINES: usize = 5;

type Sides = (Vec<Spans>, Vec<Spans>);

pub struct DiffLoad {
    path: String,
    open: HashSet<usize>,
    lines: Vec<Line>,
    source: (String, String),
    colors: Option<(Sides, Vec<Spans>)>,
}

/// Reads and diffs `path`, colouring it only when the lines differ from `shown`. Run off the UI thread.
pub fn read_diff(cwd: &str, path: String, open: HashSet<usize>, shown: &[Line]) -> DiffLoad {
    let (old, new) = git::texts(cwd, &path);
    let lines = git::diff_texts(&old, &new, &open);
    let colors = (lines != shown).then(|| {
        let syntax = syntax(&path, &old, &new);
        let hl = highlights(&lines, &syntax.0, &syntax.1);
        (syntax, hl)
    });
    DiffLoad { path, open, lines, source: (old, new), colors }
}

/// Syntax colours for every line of the old and new file; none when either is too big to parse quickly. Run off the UI thread.
pub fn syntax(path: &str, old: &str, new: &str) -> (Vec<Spans>, Vec<Spans>) {
    if old.len().max(new.len()) > MAX_COLORED {
        return Default::default();
    }
    let language = language_for(path);
    (line_spans(language, old), line_spans(language, new))
}

fn tint(syntax: Spans, words: Vec<Range<usize>>, color: u32) -> Spans {
    let bg = HighlightStyle { background_color: Some(rgba(color).into()), ..Default::default() };
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

fn colors(kind: Kind) -> (Option<u32>, u32, &'static str) {
    match kind {
        Kind::Hunk => (Some(FILL_2), TEXT_3, ""),
        Kind::Add => (Some(DIFF_ADD_BG), DIFF_ADD_TEXT, "+"),
        Kind::Del => (Some(DIFF_DEL_BG), DIFF_DEL_TEXT, "-"),
        Kind::Context => (None, TEXT_BODY, ""),
    }
}

fn number(n: Option<usize>, picked: bool) -> Div {
    div()
        .w(px(NUM))
        .flex_none()
        .pr(px(8.))
        .flex()
        .justify_end()
        .text_color(rgba(if picked { ACCENT } else { TEXT_4 }))
        .child(n.map(|n| n.to_string()).unwrap_or_default())
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
fn changed(old: &[Row], new: &[Row]) -> (Range<usize>, usize) {
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

/// Lines hidden before hunk `i`, and the code context git printed after its header.
fn hunk_info(lines: &[Line], i: usize) -> (usize, String) {
    let text = &lines[i].text;
    let before = lines[..i].iter().rev().find_map(|l| l.new).unwrap_or(0);
    let context = text.splitn(3, "@@").nth(2).unwrap_or_default().trim().to_string();
    (git::hunk_start(text, '+').saturating_sub(before + 1), context)
}

fn hunk(lines: &[Line], i: usize) -> Div {
    let (hidden, context) = hunk_info(lines, i);
    div()
        .min_h(px(ROW))
        .pl(px(16.))
        .flex()
        .items_center()
        .gap(px(10.))
        .bg(rgba(FILL_1))
        .text_color(rgba(TEXT_4))
        .whitespace_nowrap()
        .overflow_hidden()
        .child(icon("unfold", 11., TEXT_4))
        .when(hidden > 0, |d| d.child(div().flex_none().font_family(SANS).text_size(px(12.5)).child(format!("{hidden} unchanged lines"))))
        .child(div().truncate().child(context))
}

/// The first new-side line folded away above hunk `i`, the key `git::diff_texts` opens it by.
fn fold_start(lines: &[Line], i: usize) -> Option<usize> {
    let (hidden, _) = hunk_info(lines, i);
    (hidden > 0).then(|| git::hunk_start(&lines[i].text, '+') - hidden)
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

fn code(l: &Line, hl: Option<&Spans>, numbers: Vec<Option<usize>>, picked: bool) -> Div {
    let (bg, fg, sign) = colors(l.kind);
    let bg = if picked { Some(if l.kind == Kind::Context { ACCENT_TINT } else { ACCENT_BG }) } else { bg };
    div()
        .relative()
        .min_h(px(ROW))
        .flex()
        .items_start()
        .when_some(bg, |d, bg| d.bg(rgba(bg)))
        .when(picked, |d| d.child(div().absolute().left_0().top_0().bottom_0().w(px(3.)).bg(rgba(ACCENT))))
        .children(numbers.into_iter().map(|n| number(n, picked)))
        .child(div().w(px(SIGN)).flex_none().flex().justify_center().text_color(rgba(fg)).child(sign))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .pr(px(20.))
                .text_color(rgba(TEXT_BODY))
                .child(StyledText::new(SharedString::from(l.text.clone())).with_highlights(hl.cloned().unwrap_or_default())),
        )
}

fn add_button(left: f32) -> Div {
    div()
        .absolute()
        .left(px(left))
        .top(px(1.))
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .bg(rgba(ACCENT))
        .shadow(vec![ui::shadow(ACCENT_GLOW, 1., 3.)])
        .child(icon("plus", 13., WHITE))
}

impl Desktop {
    pub fn diff_view(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(path) = self.diff_file.clone() else {
            return empty("No changes.");
        };
        let file = self.repo().and_then(|r| r.files.iter().find(|f| f.path == path)).cloned();
        let viewed = self.viewed.contains(&path);
        let toggle = path.clone();
        let right = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().id("diff-mode").child(ui::segmented(
                vec![Segment { icon: None, value: false, label: "Unified".into(), badge: None }, Segment { icon: None, value: true, label: "Split".into(), badge: None }],
                self.diff_split,
                true,
                false,
                |this, v, cx| {
                    this.diff_split = v;
                    this.layout_diff(true);
                    cx.notify();
                },
                cx,
            )))
            .child(ui::button("viewed", Variant::Glass, None, div().flex().items_center().gap(px(7.)).child(checkbox(viewed)).child("Viewed")).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    if !this.viewed.remove(&toggle) {
                        this.viewed.insert(toggle.clone());
                    }
                    cx.notify();
                },
            )))
            .child(ui::icon_group([
                ui::group_button("diff-more", "more").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::More, window, cx))),
            ]));
        let (color, state) = status_word(file.as_ref().map(|f| f.status));
        let mut meta = vec![ui::meta_item().child(dot(7., color)).child(ui::meta_value(state)).into_any_element()];
        if let Some(f) = &file {
            meta.push(ui::meta_item().child(ui::meta_diff(f.added, f.removed, 11.5)).into_any_element());
        }
        let comments = self.comments.iter().filter(|c| c.path == path).count();
        if comments > 0 {
            let label = format!("{comments} comment{}", if comments == 1 { "" } else { "s" });
            meta.push(ui::meta_item().child(icon("comment", 13., TEXT_2)).child(ui::meta_value(label)).into_any_element());
        }
        let abs = self.cwd().map(|c| format!("{c}/{path}")).unwrap_or_default();
        if let Some((a, ts)) = self.agents.last_edit(&abs) {
            let by = format!("{} · {}", provider_name(&a.provider), ago_long(ts, now_ms()));
            meta.push(ui::meta_item().child(dot(7., provider_color(&a.provider))).child("by").child(ui::meta_value(by)).into_any_element());
        }
        let (dir, name) = path.rsplit_once('/').map_or((None, path.clone()), |(d, n)| (Some(d.to_string()), n.to_string()));
        let crumbs = std::iter::once("Changes".to_string()).chain(dir).chain([name]).collect();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(doc_bar(crumbs, meta, right))
            .child(self.diff_box(cx))
    }

    pub fn diff_box(&mut self, cx: &mut Context<Self>) -> Div {
        let rows = list(self.diff_list.clone(), cx.processor(|this, ix, _, cx| this.diff_row(ix, cx))).pb(px(8.));
        div()
            .flex_1()
            .min_h_0()
            .bg(rgba(PAGE))
            .border_t(px(0.5))
            .border_color(rgba(SEPARATOR))
            .overflow_hidden()
            .font_family(MONO)
            .text_size(px(12.5))
            .line_height(px(ROW))
            .child(rows.size_full())
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, window, cx| this.end_drag(window, cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, window, cx| this.end_drag(window, cx)))
    }

    /// Shows `load` unless another file or fold state was picked while it ran.
    pub fn apply_diff(&mut self, load: DiffLoad) -> bool {
        if self.diff_file.as_ref() != Some(&load.path) || load.open != self.diff_open {
            return false;
        }
        self.diff_source = load.source;
        if let Some((syntax, hl)) = load.colors {
            (self.diff_syntax, self.diff_hl) = (syntax, hl);
        }
        self.set_diff(load.lines, true)
    }

    pub fn set_diff(&mut self, lines: Vec<Line>, reset: bool) -> bool {
        if lines == self.diff {
            return false;
        }
        self.selection = self.selection.and_then(|(a, b)| Some((remap(&self.diff, &lines, a)?, remap(&self.diff, &lines, b)?)));
        self.diff = lines;
        self.layout_diff(reset);
        true
    }

    /// Shows the unchanged lines folded away from `start` on, keeping the scroll position.
    fn expand(&mut self, start: usize, cx: &mut Context<Self>) {
        self.diff_open.insert(start);
        let lines = git::diff_texts(&self.diff_source.0, &self.diff_source.1, &self.diff_open);
        self.diff_hl = highlights(&lines, &self.diff_syntax.0, &self.diff_syntax.1);
        self.set_diff(lines, false);
        cx.notify();
    }

    /// Rebuilds the rows; without `reset` only the rows that changed are remeasured and the scroll position stays.
    pub fn layout_diff(&mut self, reset: bool) {
        let composer = self.selection.filter(|_| self.composing && !self.dragging).map(|s| *ordered(s).end());
        let path = self.diff_file.as_deref();
        let notes: Vec<(usize, usize)> =
            self.comments.iter().enumerate().filter(|(_, c)| Some(c.path.as_str()) == path).filter_map(|(i, c)| Some((anchor(&self.diff, c)?, i))).collect();
        let rows = rows(&self.diff, self.diff_split, composer, &notes);
        if reset {
            self.diff_list.reset(rows.len());
        } else {
            let (range, count) = changed(&self.diff_rows, &rows);
            self.diff_list.splice(range, count);
        }
        self.diff_rows = rows;
    }

    pub fn selection_label(&self) -> Option<String> {
        label(&self.diff, ordered(self.selection?))
    }

    pub fn new_comment(&self, path: String, label: String, text: String) -> Option<Comment> {
        let (lo, hi, old_side) = span(&self.diff, ordered(self.selection?))?;
        Some(Comment { path, lines: (lo, hi), old_side, label, text, at: now_ms() })
    }

    fn resolve_comment(&mut self, i: usize, cx: &mut Context<Self>) {
        if i < self.comments.len() {
            self.comments.remove(i);
            self.layout_diff(false);
            cx.notify();
        }
    }

    pub fn same_hunk(&self, a: usize, b: usize) -> bool {
        !self.diff[a.min(b)..=a.max(b)].iter().any(|l| l.kind == Kind::Hunk)
    }

    pub fn picked(&self, i: usize) -> bool {
        self.selection.is_some_and(|s| ordered(s).contains(&i)) && self.diff[i].kind != Kind::Hunk
    }

    /// One side of a row: pressing picks its line, dragging or shift-clicking stretches the pick; "+" or a drag opens the composer.
    fn cell(&self, id: &'static str, i: usize, numbers: Vec<Option<usize>>, add_at: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        if self.diff[i].kind == Kind::Hunk {
            return self.fold(id, i, cx);
        }
        let row = code(&self.diff[i], self.diff_hl.get(i), numbers, self.picked(i)).id((id, i));
        let last = self.selection.is_some_and(|s| *ordered(s).end() == i);
        row.group("diff-line")
            .cursor_pointer()
            .child(
                add_button(add_at)
                    .when(!last, |b| b.opacity(0.).group_hover("diff-line", |s| s.opacity(1.)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| this.open_comment(i, window, cx))),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, window, cx| this.select_line(i, ev.modifiers.shift, window, cx)))
            .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, window, cx| this.drag_to(i, ev.dragging(), window, cx)))
    }

    /// A hunk header; clicking it unfolds the lines hidden above it.
    fn fold(&self, id: &'static str, i: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let row = hunk(&self.diff, i).id((id, i));
        match fold_start(&self.diff, i) {
            Some(start) => row.cursor_pointer().hover(|s| s.bg(rgba(FILL_2))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.expand(start, cx))),
            None => row,
        }
    }

    fn diff_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(&row) = self.diff_rows.get(ix) else { return Empty.into_any_element() };
        match row {
            Row::Unified(i) => self.cell("line", i, vec![self.diff[i].old, self.diff[i].new], 2. * NUM - 10., cx).w_full().into_any_element(),
            Row::Split(Some(i), _) | Row::Split(None, Some(i)) if self.diff[i].kind == Kind::Hunk => self.fold("fold", i, cx).w_full().into_any_element(),
            Row::Split(l, r) => {
                let mut side = |id, i: Option<usize>, n: fn(&Line) -> Option<usize>| match i {
                    Some(i) => self.cell(id, i, vec![n(&self.diff[i])], NUM - 10., cx).flex_1().min_w_0().into_any_element(),
                    None => div().flex_1().min_h(px(ROW)).bg(rgba(SURFACE_SUNKEN)).into_any_element(),
                };
                div().w_full().flex().child(side("old", l, |l| l.old)).child(side("new", r, |l| l.new)).into_any_element()
            }
            Row::Composer => div().w_full().child(self.composer(cx)).into_any_element(),
            Row::Comment(c) => div().w_full().child(self.comment_card(c, cx)).into_any_element(),
        }
    }

    fn composer(&self, cx: &mut Context<Self>) -> Div {
        let lines = self.selection_label().unwrap_or_default();
        let target = self.comment_target();
        let ready = target.is_some() && !self.comment_input.read(cx).value().trim().is_empty();
        let head = div()
            .px(px(16.))
            .pt(px(12.))
            .flex()
            .items_center()
            .justify_between()
            .text_size(px(12.))
            .child(div().font_family(MONO).font_weight(FontWeight::SEMIBOLD).text_color(rgba(ACCENT)).child(lines))
            .child(div().text_color(rgba(TEXT_3)).child("esc to dismiss"));
        let field = div().px(px(16.)).py(px(10.)).text_size(px(14.5)).line_height(px(21.75)).child(Textarea::new(&self.comment_input).appearance(false));
        let submit = ui::button("comment-submit", Variant::Accent, None, "Comment")
            .child(ui::button_kbd("⌘↵"))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.submit_comment(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let cancel =
            ui::button("comment-cancel", Variant::Ghost, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.cancel_comment(window, cx)));
        let foot = div()
            .px(px(12.))
            .pt(px(10.))
            .pb(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .border_t_1()
            .border_color(rgba(HAIRLINE))
            .child(self.target_picker(target, cx))
            .child(div().flex_1())
            .child(cancel)
            .child(submit);
        div()
            .mt(px(6.))
            .mb(px(10.))
            .mr(px(20.))
            .ml(px(if self.diff_split { NUM + SIGN } else { 2. * NUM + SIGN }))
            .flex()
            .flex_col()
            .rounded(px(16.))
            .bg(rgba(SURFACE))
            .shadow(vec![ui::ring(ACCENT_RING, 1.), ui::shadow(0x1111131a, 8., 24.), ui::shadow(0x1111130f, 1., 2.)])
            .font_family(SANS)
            .whitespace_normal()
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.cancel_comment(window, cx)))
            .child(head)
            .child(field)
            .child(foot)
    }

    fn comment_card(&self, i: usize, cx: &mut Context<Self>) -> Div {
        let Some(c) = self.comments.get(i) else { return div() };
        let head = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .child(div().font_family(MONO).font_weight(FontWeight::SEMIBOLD).text_color(rgba(WAITING_TEXT)).child(c.label.clone()))
            .child(div().text_color(rgba(TEXT_4)).child(format!("· {}", ago_long(c.at, now_ms()))))
            .child(div().flex_1())
            .child(div().flex().items_center().gap(px(4.)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(RUNNING_TEXT)).child(icon("check", 12., RUNNING_TEXT)).child("Sent"));
        let resolve = div()
            .id(("resolve", i))
            .h(px(24.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(5.))
            .rounded(px(12.))
            .bg(rgba(FILL_3))
            .hover(|s| s.bg(rgba(FILL_4)))
            .cursor_pointer()
            .text_size(px(12.5))
            .font_weight(FontWeight::MEDIUM)
            .child(icon("check", 12., TEXT_2))
            .child("Resolve")
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.resolve_comment(i, cx)));
        div()
            .mt(px(6.))
            .mb(px(10.))
            .mr(px(20.))
            .ml(px(if self.diff_split { NUM + SIGN } else { 2. * NUM + SIGN }))
            .px(px(16.))
            .py(px(12.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .rounded(px(12.))
            .bg(rgba(SURFACE))
            .shadow(vec![ui::ring(SEPARATOR, 0.5), ui::shadow(0x1111130f, 1., 3.)])
            .font_family(SANS)
            .whitespace_normal()
            .child(head)
            .child(div().text_size(px(14.5)).line_height(px(21.)).text_color(rgba(TEXT)).child(c.text.clone()))
            .child(div().flex().child(resolve))
    }

    fn session_chip(&self, id: &str) -> (u32, String, String) {
        let a = self.agents.get(id);
        let provider = a.map(|a| a.provider.clone()).unwrap_or_default();
        let branch = a.and_then(|a| self.sessions.get(&a.terminal_id)).and_then(|s| self.repos.get(&s.info.cwd)).map(|r| r.branch.clone()).unwrap_or_default();
        (provider_color(&provider), provider, branch)
    }

    fn target_picker(&self, target: Option<String>, cx: &mut Context<Self>) -> Div {
        let pill = div()
            .id("comment-target")
            .h(px(32.))
            .px(px(12.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(7.))
            .rounded(px(16.))
            .bg(rgba(FILL_3))
            .cursor_pointer()
            .hover(|s| s.bg(rgba(FILL_4)))
            .text_size(px(13.))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.target_menu = !this.target_menu;
                cx.notify();
            }));
        let pill = match &target {
            Some(id) => {
                let (color, name, branch) = self.session_chip(id);
                pill.child(dot(7., color))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                    .when(!branch.is_empty(), |d| {
                        d.child(div().text_color(rgba(TEXT_6)).child("·")).child(div().font_family(MONO).text_size(px(11.5)).text_color(rgba(TEXT_2)).child(branch))
                    })
            }
            None => pill.text_color(rgba(TEXT_2)).child("No session"),
        };
        let menu = self.target_menu.then(|| {
            let cards = self.project.as_deref().map(|p| self.cards(p)).unwrap_or_default();
            let items = cards.into_iter().enumerate().map(|(i, c)| {
                let (color, name, branch) = self.session_chip(&c.id);
                let picked = target.as_ref() == Some(&c.id);
                let id = c.id.clone();
                div()
                    .id(("target", i))
                    .h(px(34.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(10.))
                    .cursor_pointer()
                    .hover(|s| s.bg(rgba(FILL_3)))
                    .text_size(px(13.))
                    .child(dot(7., color))
                    .child(div().font_weight(FontWeight::SEMIBOLD).child(name))
                    .child(div().flex_1().min_w_0().truncate().text_color(rgba(TEXT_2)).child(c.title))
                    .child(div().font_family(MONO).text_size(px(11.5)).text_color(rgba(TEXT_3)).child(branch))
                    .child(div().size(px(14.)).when(picked, |d| d.child(icon("check", 14., TEXT))))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.comment_target = Some(id.clone());
                        this.target_menu = false;
                        cx.notify();
                    }))
            });
            deferred(
                anchored().anchor(Anchor::BottomLeft).offset(point(px(0.), px(-6.))).snap_to_window_with_margin(px(8.)).child(
                    ui::pop(div().id("target-menu"))
                        .w(px(360.))
                        .p(px(6.))
                        .flex()
                        .flex_col()
                        .children(items)
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.target_menu = false;
                            cx.notify();
                        })),
                ),
            )
            .with_priority(1)
        });
        div().relative().child(pill.child(icon("chevron-down", 12., TEXT_3))).children(menu)
    }
}

#[cfg(test)]
mod tests {
    use super::{Row, changed, fold_start, highlights, hunk_info, label, remap, rows};
    use git::parse;
    use gpui_kit::{HighlightStyle, rgba};
    use theme::{DIFF_ADD_WORD, DIFF_DEL_WORD, SYN_FN, SYN_KEYWORD, SYN_STRING};

    const DIFF: &str = "@@ -1,2 +1,3 @@\n a\n-b\n+c\n+d\n";

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
    fn counts_lines_hidden_before_a_hunk() {
        let l = parse("@@ -40,2 +45,2 @@ export function f() {\n a\n b\n@@ -60,1 +65,1 @@\n c\n");
        assert_eq!(hunk_info(&l, 0), (44, "export function f() {".into()));
        assert_eq!(hunk_info(&l, 3), (18, String::new()));
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

    fn color(c: u32) -> Vec<(std::ops::Range<usize>, HighlightStyle)> {
        vec![(0..1, HighlightStyle { color: Some(rgba(c).into()), ..Default::default() })]
    }

    #[test]
    fn tints_changed_words_in_lines_replaced_one_for_one() {
        let l = parse("@@ -1,2 +1,2 @@\n-let a = 1;\n+let b = 1;\n ctx\n");
        let hl = highlights(&l, &[], &[]);
        let tint = |i: usize| hl[i].iter().filter_map(|(r, s)| Some((r.clone(), s.background_color?))).collect::<Vec<_>>();
        assert_eq!(tint(1), vec![(4..5, rgba(DIFF_DEL_WORD).into())]);
        assert_eq!(tint(2), vec![(4..5, rgba(DIFF_ADD_WORD).into())]);
        assert!(hl[3].is_empty());
    }

    #[test]
    fn takes_syntax_from_each_side_and_skips_uneven_runs() {
        let l = parse("@@ -2,1 +2,2 @@\n-x\n+y\n+z\n");
        let hl = highlights(&l, &[vec![], color(SYN_KEYWORD)], &[vec![], color(SYN_FN), color(SYN_STRING)]);
        assert_eq!(hl[1][0].1.color, Some(rgba(SYN_KEYWORD).into()));
        assert_eq!(hl[2][0].1.color, Some(rgba(SYN_FN).into()));
        assert_eq!(hl[3][0].1.color, Some(rgba(SYN_STRING).into()));
        assert!(hl.iter().flatten().all(|(_, s)| s.background_color.is_none()));
    }

    #[test]
    fn drops_syntax_that_no_longer_fits_the_line() {
        let l = parse("@@ -1,1 +1,1 @@\n a\n");
        let long = vec![(0..5, HighlightStyle { color: Some(rgba(SYN_FN).into()), ..Default::default() })];
        assert!(highlights(&l, &[], &[long])[1].is_empty());
    }

    #[test]
    fn finds_the_first_folded_line() {
        let l = parse("@@ -40,2 +45,2 @@ export function f() {\n a\n b\n@@ -60,1 +65,1 @@\n c\n");
        assert_eq!(fold_start(&l, 0), Some(1));
        assert_eq!(fold_start(&l, 3), Some(47));
        assert_eq!(fold_start(&parse("@@ -1,1 +1,1 @@\n a\n"), 0), None);
    }
}
