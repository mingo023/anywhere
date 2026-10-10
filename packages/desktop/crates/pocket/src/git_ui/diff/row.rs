use super::{At, NUM, ROW, Row, SIGN};
use crate::desktop::Desktop;
use crate::syntax::Spans;
use git::{Kind, Line};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use workspace::tree::PaneId;

fn colors(kind: Kind) -> (Option<Token>, Token, &'static str) {
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
        .text_color(if picked { ACCENT } else { TEXT_4 })
        .child(n.map(|n| n.to_string()).unwrap_or_default())
}

/// Lines hidden before hunk `i`, and the code context git printed after its header.
fn hunk_info(lines: &[Line], i: usize) -> (usize, String) {
    let text = &lines[i].text;
    let before = lines[..i].iter().rev().find_map(|l| l.new).unwrap_or(0);
    let context = text.splitn(3, "@@").nth(2).unwrap_or_default().trim().to_string();
    (git::hunk_start(text, '+').saturating_sub(before + 1), context)
}

pub(crate) fn hunk(lines: &[Line], i: usize) -> Div {
    let (hidden, context) = hunk_info(lines, i);
    div()
        .min_h(px(ROW))
        .pl(px(16.))
        .flex()
        .items_center()
        .gap(px(10.))
        .bg(FILL_1)
        .text_color(TEXT_4)
        .whitespace_nowrap()
        .overflow_hidden()
        .child(icon("unfold", 11., TEXT_4))
        .when(hidden > 0, |d| d.child(div().flex_none().font_family(ui_font()).text_size(px(12.5)).child(format!("{hidden} unchanged lines"))))
        .child(div().truncate().child(context))
}

/// The first new-side line folded away above hunk `i`, the key `git::diff_texts` opens it by.
fn fold_start(lines: &[Line], i: usize) -> Option<usize> {
    let (hidden, _) = hunk_info(lines, i);
    (hidden > 0).then(|| git::hunk_start(&lines[i].text, '+') - hidden)
}

pub(crate) fn code(l: &Line, hl: Option<&Spans>, numbers: Vec<Option<usize>>, picked: bool) -> Div {
    let (bg, fg, sign) = colors(l.kind);
    let bg = if picked { Some(if l.kind == Kind::Context { ACCENT_TINT } else { ACCENT_BG }) } else { bg };
    div()
        .relative()
        .min_h(px(ROW))
        .flex()
        .items_start()
        .when_some(bg, |d, bg| d.bg(bg))
        .when(picked, |d| d.child(div().absolute().left_0().top_0().bottom_0().w(px(3.)).bg(ACCENT_FILL)))
        .children(numbers.into_iter().map(|n| number(n, picked)))
        .child(div().w(px(SIGN)).flex_none().flex().justify_center().text_color(fg).child(sign))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .pr(px(20.))
                .text_color(TEXT_BODY)
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
        .bg(ACCENT)
        .shadow(vec![ui::shadow(ACCENT_GLOW, 1., 3.)])
        .child(icon("plus", 13., WHITE))
}

impl Desktop {
    /// One side of a row: pressing picks its line, dragging or shift-clicking stretches the pick; "+" or a drag opens the composer.
    fn cell(&self, pane: PaneId, id: &'static str, i: usize, numbers: Vec<Option<usize>>, add_at: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let v = &self.diff.panes[&pane];
        if v.lines[i].kind == Kind::Hunk {
            return self.fold(pane, id, i, cx);
        }
        let row = code(&v.lines[i], v.hl.get(i), numbers, v.pick.picked(&v.lines, i)).id((id, i));
        if v.at != At::Working {
            return row;
        }
        let last = v.pick.last() == Some(i);
        row.group("diff-line")
            .cursor_pointer()
            .child(
                add_button(add_at)
                    .when(!last, |b| b.opacity(0.).group_hover("diff-line", |s| s.opacity(1.)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| this.open_composer(pane, i, window, cx))),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, window, cx| this.select_line(pane, i, ev.modifiers.shift, window, cx)))
            .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, window, cx| this.drag_to(pane, i, ev.dragging(), window, cx)))
    }

    /// A hunk header; clicking it unfolds the lines hidden above it.
    fn fold(&self, pane: PaneId, id: &'static str, i: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let v = &self.diff.panes[&pane];
        let row = hunk(&v.lines, i).id((id, i));
        match fold_start(&v.lines, i) {
            Some(start) => row.cursor_pointer().hover(|s| s.bg(FILL_2)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.expand(pane, start, cx))),
            None => row,
        }
    }

    pub(super) fn diff_row(&self, pane: PaneId, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some((v, &row)) = self.diff.view(pane).and_then(|v| Some((v, v.rows.get(ix)?))) else { return Empty.into_any_element() };
        match row {
            Row::Unified(i) => self.cell(pane, "line", i, vec![v.lines[i].old, v.lines[i].new], 2. * NUM - 10., cx).w_full().into_any_element(),
            Row::Split(Some(i), _) | Row::Split(None, Some(i)) if v.lines[i].kind == Kind::Hunk => self.fold(pane, "fold", i, cx).w_full().into_any_element(),
            Row::Split(l, r) => {
                let mut side = |id, i: Option<usize>, n: fn(&Line) -> Option<usize>| match i {
                    Some(i) => self.cell(pane, id, i, vec![n(&v.lines[i])], NUM - 10., cx).flex_1().min_w_0().into_any_element(),
                    None => div().flex_1().min_h(px(ROW)).bg(SURFACE_SUNKEN).into_any_element(),
                };
                div().w_full().flex().child(side("old", l, |l| l.old)).child(side("new", r, |l| l.new)).into_any_element()
            }
            Row::Composer => div()
                .w_full()
                .children(self.diff.draft_quote().map(|q| {
                    self.chat_card(&q, false, cx).mt(px(6.)).mb(px(10.)).mr(px(20.)).ml(px(if self.diff.split { NUM + SIGN } else { 2. * NUM + SIGN }))
                }))
                .into_any_element(),
            Row::Thread(k) => div()
                .w_full()
                .children(v.pins.get(k).and_then(|p| self.pinned_thread(k, &p.id, cx)).map(|t| {
                    div()
                        .mt(px(6.))
                        .mb(px(10.))
                        .mr(px(20.))
                        .ml(px(if self.diff.split { NUM + SIGN } else { 2. * NUM + SIGN }))
                        .max_w(px(720.))
                        .font_family(ui_font())
                        .text_size(px(13.))
                        .line_height(relative(1.2))
                        .child(t)
                }))
                .into_any_element(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{fold_start, hunk_info};
    use git::parse;

    #[test]
    fn counts_lines_hidden_before_a_hunk() {
        let l = parse("@@ -40,2 +45,2 @@ export function f() {\n a\n b\n@@ -60,1 +65,1 @@\n c\n");
        assert_eq!(hunk_info(&l, 0), (44, "export function f() {".into()));
        assert_eq!(hunk_info(&l, 3), (18, String::new()));
    }

    #[test]
    fn finds_the_first_folded_line() {
        let l = parse("@@ -40,2 +45,2 @@ export function f() {\n a\n b\n@@ -60,1 +65,1 @@\n c\n");
        assert_eq!(fold_start(&l, 0), Some(1));
        assert_eq!(fold_start(&l, 3), Some(47));
        assert_eq!(fold_start(&parse("@@ -1,1 +1,1 @@\n a\n"), 0), None);
    }
}
