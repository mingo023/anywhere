use super::{NUM, ROW, SIGN};
use crate::syntax::Spans;
use git::{Kind, Line};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

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
pub(super) fn fold_start(lines: &[Line], i: usize) -> Option<usize> {
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

pub(super) fn add_button(left: f32) -> Div {
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
