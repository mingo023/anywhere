use crate::git::{self, Kind, Line};
use crate::theme::*;
use crate::ds::{self, Segment, Variant, checkbox, diffstat, dot};
use crate::view::empty;
use crate::Desktop;
use gpui_kit::component::input::{Escape, Textarea};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::ops::{Range, RangeInclusive};

const NUM: f32 = 44.;
const SIGN: f32 = 18.;
const ROW: f32 = 22.;

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
}

/// Lays out the diff, with the composer under the row holding line `composer`.
fn rows(lines: &[Line], split: bool, composer: Option<usize>) -> Vec<Row> {
    let base: Vec<Row> =
        if split { git::split(lines).into_iter().map(|(l, r)| Row::Split(l, r)).collect() } else { (0..lines.len()).map(Row::Unified).collect() };
    let mut out = Vec::with_capacity(base.len() + 1);
    for row in base {
        out.push(row);
        let holds = match row {
            Row::Unified(i) => composer == Some(i),
            Row::Split(l, r) => composer.is_some_and(|c| l == Some(c) || r == Some(c)),
            Row::Composer => false,
        };
        if holds {
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

fn ordered((a, b): (usize, usize)) -> RangeInclusive<usize> {
    a.min(b)..=a.max(b)
}

/// "Line 54" or "Lines 50–54", counted on the new side unless the range only removes lines.
fn label(lines: &[Line], range: RangeInclusive<usize>) -> Option<String> {
    let picked: Vec<&Line> = lines.get(range)?.iter().filter(|l| l.kind != Kind::Hunk).collect();
    let new: Vec<usize> = picked.iter().filter_map(|l| l.new).collect();
    let nums = if new.is_empty() { picked.iter().filter_map(|l| l.old).collect() } else { new };
    let (lo, hi) = (*nums.iter().min()?, *nums.iter().max()?);
    Some(if lo == hi { format!("Line {lo}") } else { format!("Lines {lo}–{hi}") })
}

/// Where line `i` of `old` sits in `new`, so a selection survives the diff refreshing under it.
fn remap(old: &[Line], new: &[Line], i: usize) -> Option<usize> {
    let l = old.get(i)?;
    let same = |n: &&Line| n.kind == l.kind && n.text == l.text;
    new.iter().position(|n| n == l).or_else(|| new.iter().enumerate().filter(|(_, n)| same(n)).min_by_key(|(j, _)| j.abs_diff(i)).map(|(j, _)| j))
}

fn code(l: &Line, numbers: Vec<Option<usize>>, picked: bool) -> Div {
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
        .child(div().flex_1().min_w_0().pr(px(20.)).text_color(rgba(fg)).child(SharedString::from(l.text.clone())))
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
        .shadow(vec![ds::shadow(ACCENT_GLOW, 1., 3.)])
        .child(icon("plus", 13., WHITE))
}

impl Desktop {
    pub fn changes_list(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let list = div().id("changes").flex_1().overflow_y_scroll().flex().flex_col();
        let Some(repo) = self.repo().cloned() else {
            return list.child(empty("Not a git repository."));
        };
        let (added, removed) = repo.totals();
        let staged = repo.files.iter().filter(|f| f.staged).count();
        let label = if staged == repo.files.len() && staged > 0 { "Staged" } else { "Changes" };
        let branch = div()
            .h(px(40.))
            .mt(px(6.))
            .px(px(16.))
            .flex()
            .items_center()
            .gap(px(6.))
            .font_family(MONO)
            .text_size(px(11.5))
            .child(icon("worktree", 12., TEXT_4))
            .child(div().truncate().child(repo.branch.clone()))
            .when_some(repo.base.clone(), |d, base| d.child(icon("arrow-right", 12., TEXT_4)).child(div().text_color(rgba(TEXT_3)).child(base)))
            .child(div().flex_1())
            .child(div().font_family(SANS).text_size(px(12.)).text_color(rgba(TEXT_3)).child(format!("{} ahead", repo.ahead)));
        let heading = div()
            .px(px(16.))
            .pt(px(4.))
            .pb(px(4.))
            .flex()
            .items_center()
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(rgba(TEXT_3))
            .child(div().flex_1().child(format!("{label} · {} file{}", repo.files.len(), if repo.files.len() == 1 { "" } else { "s" })))
            .child(diffstat(added, removed).font_weight(FontWeight::NORMAL));
        let files = repo.files.iter().enumerate().map(|(i, f)| {
            let selected = self.diff_file.as_ref() == Some(&f.path);
            let (path, on) = (f.path.clone(), f.staged);
            let open = f.path.clone();
            let check = div().id(("stage", i)).flex().child(checkbox(on)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.stage(path.clone(), !on, cx);
            }));
            ds::change_row(("file", i), check, &f.path, selected, f.added, f.removed)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_changes(Some(open.clone()), cx)))
        });
        let commits = repo.commits.iter().map(|c| {
            div()
                .px(px(16.))
                .h(px(26.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(13.))
                .child(div().font_family(MONO).text_size(px(11.5)).text_color(rgba(RUNNING_TEXT)).child(c.sha.clone()))
                .child(div().truncate().text_color(rgba(TEXT_BODY)).child(c.subject.clone()))
        });
        list.child(branch)
            .child(heading)
            .child(div().px(px(8.)).flex().flex_col().gap(px(2.)).children(files))
            .child(div().mx(px(16.)).my(px(10.)).h(px(1.)).bg(rgba(HAIRLINE)))
            .child(div().px(px(16.)).pb(px(4.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT_3)).child("Commits on branch"))
            .children(commits)
    }

    pub fn diff_view(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(path) = self.diff_file.clone() else {
            return div().flex_1().flex().items_center().justify_center().text_size(px(14.)).text_color(rgba(TEXT_3)).child("No changes.");
        };
        let file = self.repo().and_then(|r| r.files.iter().find(|f| f.path == path)).cloned();
        let open = self.cwd().map(|c| std::path::Path::new(&c).join(&path));
        let header = div()
            .h(px(52.))
            .flex_none()
            .pl(px(20.))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(10.))
            .border_b_1()
            .border_color(rgba(HAIRLINE))
            .child(icon("file", 15., TEXT_3))
            .child(div().truncate().font_family(MONO).text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).child(path.clone()))
            .children(file.map(|f| diffstat(f.added, f.removed)))
            .child(div().flex_1())
            .child(ds::segmented(
                vec![Segment { value: false, label: "Unified".into(), badge: None }, Segment { value: true, label: "Split".into(), badge: None }],
                self.diff_split,
                true,
                false,
                |this, v, cx| {
                    this.diff_split = v;
                    this.layout_diff(true);
                    cx.notify();
                },
                cx,
            ))
            .child(ds::button("open-editor", Variant::Secondary, Some("external"), "Open in editor").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| {
                if let Some(p) = &open {
                    cx.open_with_system(p);
                }
            })));
        let rows = list(self.diff_list.clone(), cx.processor(|this, ix, _, cx| this.diff_row(ix, cx))).py(px(8.));
        let body = div()
            .flex_1()
            .min_h_0()
            .child(rows.size_full())
            .on_mouse_up(MouseButton::Left, cx.listener(|this, _, window, cx| this.end_drag(window, cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _, window, cx| this.end_drag(window, cx)));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(body.font_family(MONO).text_size(px(12.5)).line_height(px(ROW)))
    }

    pub fn set_diff(&mut self, lines: Vec<Line>) -> bool {
        if lines == self.diff {
            return false;
        }
        self.selection = self.selection.and_then(|(a, b)| Some((remap(&self.diff, &lines, a)?, remap(&self.diff, &lines, b)?)));
        self.diff = lines;
        self.layout_diff(true);
        true
    }

    /// Rebuilds the rows; without `reset` only the rows that changed are remeasured and the scroll position stays.
    pub fn layout_diff(&mut self, reset: bool) {
        let composer = self.selection.filter(|_| self.composing && !self.dragging).map(|s| *ordered(s).end());
        let rows = rows(&self.diff, self.diff_split, composer);
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

    pub fn same_hunk(&self, a: usize, b: usize) -> bool {
        !self.diff[a.min(b)..=a.max(b)].iter().any(|l| l.kind == Kind::Hunk)
    }

    pub fn picked(&self, i: usize) -> bool {
        self.selection.is_some_and(|s| ordered(s).contains(&i)) && self.diff[i].kind != Kind::Hunk
    }

    /// One side of a row: pressing picks its line, dragging or shift-clicking stretches the pick; "+" or a drag opens the composer.
    fn cell(&self, id: &'static str, i: usize, numbers: Vec<Option<usize>>, add_at: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let row = code(&self.diff[i], numbers, self.picked(i)).id((id, i));
        if self.diff[i].kind == Kind::Hunk {
            return row;
        }
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

    fn diff_row(&self, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(&row) = self.diff_rows.get(ix) else { return Empty.into_any_element() };
        match row {
            Row::Unified(i) => self.cell("line", i, vec![self.diff[i].old, self.diff[i].new], 2. * NUM - 10., cx).w_full().into_any_element(),
            Row::Split(l, r) => {
                let mut side = |id, i: Option<usize>, n: fn(&Line) -> Option<usize>| match i {
                    Some(i) => self.cell(id, i, vec![n(&self.diff[i])], NUM - 10., cx).flex_1().min_w_0().into_any_element(),
                    None => div().flex_1().min_h(px(ROW)).bg(rgba(SURFACE_SUNKEN)).into_any_element(),
                };
                div().w_full().flex().child(side("old", l, |l| l.old)).child(side("new", r, |l| l.new)).into_any_element()
            }
            Row::Composer => div().w_full().child(self.composer(cx)).into_any_element(),
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
        let submit = ds::button("comment-submit", Variant::Accent, None, "Comment")
            .child(ds::button_kbd("⌘↵"))
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.submit_comment(window, cx))))
            .when(!ready, |d| d.opacity(0.5).cursor_default());
        let cancel =
            ds::button("comment-cancel", Variant::Ghost, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.cancel_comment(window, cx)));
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
            .child(div().truncate().text_size(px(12.)).text_color(rgba(TEXT_3)).child("sends to this session’s terminal"))
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
            .rounded(px(12.))
            .bg(rgba(SURFACE))
            .shadow(vec![ds::ring(ACCENT_RING, 1.), ds::shadow(0x1111131a, 8., 24.), ds::shadow(0x1111130f, 1., 2.)])
            .font_family(SANS)
            .whitespace_normal()
            .on_action(cx.listener(|this, _: &Escape, window, cx| this.cancel_comment(window, cx)))
            .child(head)
            .child(field)
            .child(foot)
    }

    fn session_chip(&self, id: &str) -> (u32, String, String) {
        let provider = self.summary(id).map(|a| a.provider.clone()).unwrap_or_default();
        let branch = self.cwd_of(id).and_then(|c| self.repos.get(&c)).map(|r| r.branch.clone()).unwrap_or_default();
        (provider_color(&provider), self.pane_label(id), branch)
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
                    ds::pop(div().id("target-menu"))
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
    use super::{Row, changed, label, remap, rows};
    use crate::git::parse;

    const DIFF: &str = "@@ -1,2 +1,3 @@\n a\n-b\n+c\n+d\n";

    #[test]
    fn places_the_composer_under_its_line() {
        let l = parse(DIFF);
        assert_eq!(rows(&l, false, Some(3)), vec![Row::Unified(0), Row::Unified(1), Row::Unified(2), Row::Unified(3), Row::Composer, Row::Unified(4)]);
        assert_eq!(rows(&l, true, Some(2)), vec![Row::Split(Some(0), Some(0)), Row::Split(Some(1), Some(1)), Row::Split(Some(2), Some(3)), Row::Composer, Row::Split(None, Some(4))]);
        assert_eq!(rows(&l, false, None).len(), 5);
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
}
