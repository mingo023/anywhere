use crate::git_ui::diff::{code, code_text, highlights, syntax};
use crate::syntax::Spans;
use chat::{Diff, LineKind};
use git::{Kind, Line};
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::rc::Rc;
use theme::*;

/// An edit's lines as git's diff lines, each hunk under its header. The agent numbers a deletion by its old line and the
/// rest by their new one; a jump in numbering starts a new hunk.
pub(crate) fn lines(diff: &Diff) -> Vec<Line> {
    let mut out = Vec::with_capacity(diff.lines.len());
    // New minus old line number; the preview starts before the first change, so at zero.
    let mut shift: isize = 0;
    let mut next: Option<(usize, usize)> = None;
    for l in &diff.lines {
        let n = l.number.unwrap_or(0) as usize;
        let (old, new) = match l.kind {
            LineKind::Del => (n, (n as isize + shift) as usize),
            LineKind::Add | LineKind::Context => ((n as isize - shift).max(0) as usize, n),
        };
        let continues = match (l.kind, next) {
            (_, None) => false,
            (LineKind::Del, Some((o, _))) => old == o,
            (_, Some((_, w))) => new == w,
        };
        if !continues {
            out.push(Line { kind: Kind::Hunk, old: None, new: None, text: format!("@@ -{old} +{new} @@") });
        }
        let (kind, sides) = match l.kind {
            LineKind::Del => (Kind::Del, (Some(old), None)),
            LineKind::Add => (Kind::Add, (None, Some(new))),
            LineKind::Context => (Kind::Context, (Some(old), Some(new))),
        };
        out.push(Line { kind, old: sides.0, new: sides.1, text: l.text.clone() });
        match l.kind {
            LineKind::Del => shift -= 1,
            LineKind::Add => shift += 1,
            LineKind::Context => {}
        }
        next = Some(match l.kind {
            LineKind::Del => (old + 1, new),
            LineKind::Add => (old, new + 1),
            LineKind::Context => (old + 1, new + 1),
        });
    }
    out
}

/// Syntax and word colours for `lines` of `path`. Parses, so run it off the UI thread.
pub(crate) fn highlight(path: &str, lines: &[Line]) -> Vec<Spans> {
    // Only these lines are known, so each side's text is its lines in order, renumbered to match.
    let (mut old, mut new) = (String::new(), String::new());
    let (mut o, mut n) = (0, 0);
    let numbered: Vec<Line> = lines
        .iter()
        .map(|l| {
            let side = |text: &mut String, at: &mut usize, has: bool| {
                has.then(|| {
                    text.push_str(&l.text);
                    text.push('\n');
                    *at += 1;
                    *at
                })
            };
            let old = side(&mut old, &mut o, l.old.is_some() && l.kind != Kind::Hunk);
            let new = side(&mut new, &mut n, l.new.is_some() && l.kind != Kind::Hunk);
            Line { old, new, ..l.clone() }
        })
        .collect();
    let (old, new) = syntax(path, &old, &new);
    highlights(&numbered, &old, &new)
}

/// Diff colours by tool id, filled in the background once a chip opens.
#[derive(Default)]
pub(crate) struct Highlights {
    done: HashMap<String, Rc<Vec<Spans>>>,
    loading: HashSet<String>,
}

impl Highlights {
    pub(crate) fn get(&self, id: &str) -> Option<Rc<Vec<Spans>>> {
        self.done.get(id).cloned()
    }

    /// Whether `id` still needs colouring; it's then marked as on its way.
    pub(crate) fn start(&mut self, id: &str) -> bool {
        !self.done.contains_key(id) && self.loading.insert(id.to_string())
    }

    pub(crate) fn finish(&mut self, id: String, spans: Vec<Spans>) {
        self.loading.remove(&id);
        self.done.insert(id, Rc::new(spans));
    }
}

/// The diff's rows, as the changes view draws them.
pub(crate) fn body(lines: &[Line], hl: Option<&[Spans]>) -> Div {
    code_text(div().w_full().min_w_0().flex().flex_col().overflow_hidden().rounded(px(6.)).my(px(4.)), 12).children(lines.iter().enumerate().map(|(i, l)| match l.kind {
        Kind::Hunk => hunk(&l.text),
        // Long lines clip, as the rows keep one height.
        _ => code(l, hl.and_then(|h| h.get(i)), vec![l.old, l.new], false).whitespace_nowrap().overflow_hidden(),
    }))
}

/// A hunk's `@@` line; unlike the changes view's, it unfolds nothing.
fn hunk(text: &str) -> Div {
    div()
        .min_h(px(28.))
        .pl(px(16.))
        .flex()
        .items_center()
        .bg(FILL_1)
        .text_size(px(11.))
        .text_color(TEXT_3)
        .whitespace_nowrap()
        .overflow_hidden()
        .child(div().truncate().child(text.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{Highlights, lines};
    use chat::{Diff, DiffLine, LineKind};
    use git::{Kind, Line};

    fn d(number: u32, kind: LineKind, text: &str) -> DiffLine {
        DiffLine { number: Some(number), kind, text: text.into() }
    }

    fn g(kind: Kind, old: Option<usize>, new: Option<usize>, text: &str) -> Line {
        Line { kind, old, new, text: text.into() }
    }

    #[test]
    fn diff_lines_map_onto_git_lines() {
        let diff = Diff {
            lines: vec![
                d(4, LineKind::Context, "fn a() {"),
                d(5, LineKind::Del, "    old();"),
                d(6, LineKind::Del, "    gone();"),
                d(5, LineKind::Add, "    new();"),
                d(6, LineKind::Context, "}"),
                d(20, LineKind::Context, "fn b() {"),
                d(21, LineKind::Add, "    more();"),
            ],
            additions: 2,
            deletions: 2,
        };
        assert_eq!(
            lines(&diff),
            vec![
                g(Kind::Hunk, None, None, "@@ -4 +4 @@"),
                g(Kind::Context, Some(4), Some(4), "fn a() {"),
                g(Kind::Del, Some(5), None, "    old();"),
                g(Kind::Del, Some(6), None, "    gone();"),
                g(Kind::Add, None, Some(5), "    new();"),
                g(Kind::Context, Some(7), Some(6), "}"),
                g(Kind::Hunk, None, None, "@@ -21 +20 @@"),
                g(Kind::Context, Some(21), Some(20), "fn b() {"),
                g(Kind::Add, None, Some(21), "    more();"),
            ]
        );
    }

    #[test]
    fn a_diff_is_coloured_once() {
        let mut h = Highlights::default();
        assert!(h.start("t") && !h.start("t"));
        h.finish("t".into(), vec![]);
        assert!(!h.start("t") && h.get("t").is_some());
    }
}
