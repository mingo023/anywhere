use super::catalog::{Look, Setting};
use crate::desktop::Desktop;
use crate::git_ui::diff::{self, code, hunk};
use git::{Kind, Line};
use gpui_kit::*;
use std::collections::HashSet;
use theme::*;

const OLD: &str = "const { session } = props;
const ready = isRestored;
if (!ready) return null;
return (
  <Terminal pane={pane} />
);
";
const NEW: &str = "const { session } = props;
const ready = useTerminalSettled(session.id);
if (!ready) return null;
return (
    <Terminal pane={pane} />
);
";

fn reread(d: &mut Desktop, cx: &mut Context<Desktop>) {
    d.diff.options = diff::options(&d.store.diff);
    d.recolor_diff(cx);
    d.recolor_commit(cx);
}

pub(super) const ROWS: &[Setting] = &[
    Setting::choice("diff-style", "Diff", "Diff style", &["Unified", "Split"], Look::Segmented, |s| usize::from(s.diff.split), |s, i| s.diff.split = i == 1)
        .effect(|d, _| d.apply_diff_style()),
    Setting::choice("changes-list", "Diff", "Changes list", &["List", "Tree"], Look::Segmented, |s| usize::from(s.diff.tree), |s, i| s.diff.tree = i == 1),
    Setting::stepper("context", "Diff", "Context lines", (0, 20, 1), " lines", |s| s.diff.context as i32, |s, n| s.diff.context = n as usize)
        .hint("Unchanged lines shown around each change")
        .effect(reread),
    Setting::switch("ignore-whitespace", "Diff", "Ignore whitespace changes", |s| s.diff.ignore_whitespace, |s, on| s.diff.ignore_whitespace = on)
        .effect(reread),
    Setting::custom("preview", "Diff", "Preview", |d, s, _| d.diff_preview(s)),
    Setting::choice("graph-from", "Graph", "Show commits from", &["Base branch only", "All branches"], Look::Segmented, |s| usize::from(s.diff.all_branches), |s, i| s.diff.all_branches = i == 1)
        .effect(|d, cx| {
            d.graph.all_branches = d.store.diff.all_branches;
            d.refresh_graph(cx);
        }),
    Setting::stepper("graph-page", "Graph", "Commits per page", (20, 500, 10), "", |s| s.diff.page as i32, |s, n| s.diff.page = n as usize)
        .advanced()
        .effect(|d, _| d.graph.page = d.store.diff.page),
];

/// The sample diff as the settings show it; a header that hides no lines is left out.
fn preview_lines(options: git::Options) -> Vec<Line> {
    let lines = git::diff_texts(OLD, NEW, &HashSet::new(), options);
    let mut out: Vec<Line> = Vec::new();
    for l in lines {
        let shown = out.iter().rev().find_map(|l| l.new).unwrap_or(0);
        if l.kind != Kind::Hunk || git::hunk_start(&l.text, '+') > shown + 1 {
            out.push(l);
        }
    }
    out
}

impl Desktop {
    fn diff_preview(&self, setting: &'static Setting) -> Div {
        let lines = preview_lines(diff::options(&self.store.diff));
        let side = |i: Option<usize>| match i {
            Some(i) => code(&lines[i], None, vec![lines[i].new.or(lines[i].old)], false).flex_1().min_w_0(),
            None => div().flex_1().bg(FILL_1),
        };
        let rows: Vec<Div> = if self.store.diff.split {
            git::split(&lines)
                .into_iter()
                .map(|(l, r)| match (l, r) {
                    (Some(i), _) if lines[i].kind == Kind::Hunk => hunk(&lines, i),
                    _ => div().flex().child(side(l)).child(side(r)),
                })
                .collect()
        } else {
            (0..lines.len()).map(|i| if lines[i].kind == Kind::Hunk { hunk(&lines, i) } else { code(&lines[i], None, vec![lines[i].new.or(lines[i].old)], false) }).collect()
        };
        div()
            .px(px(14.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(div().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(setting.label))
            .child(crate::git_ui::diff::code_text(div(), self.store.appearance.code_size()).py(px(4.)).rounded(px(8.)).overflow_hidden().bg(WINDOW_SOLID).children(rows))
    }
}

#[cfg(test)]
mod tests {
    use super::preview_lines;
    use git::{Kind, Options};

    fn kinds(options: Options) -> Vec<Kind> {
        preview_lines(options).iter().map(|l| l.kind).collect()
    }

    #[test]
    fn the_preview_shows_the_whole_sample_with_default_context() {
        let lines = preview_lines(Options::default());
        assert_eq!(lines.first().and_then(|l| l.new), Some(1));
        assert!(!lines.iter().any(|l| l.kind == Kind::Hunk));
    }

    #[test]
    fn the_preview_folds_unchanged_lines_without_context() {
        let k = kinds(Options { context: 0, ignore_whitespace: false });
        assert_eq!(k.iter().filter(|k| **k == Kind::Hunk).count(), 2);
        assert!(!k.contains(&Kind::Context));
    }

    #[test]
    fn the_preview_drops_the_reindented_line_when_whitespace_is_ignored() {
        let changed = |o| kinds(o).iter().filter(|k| **k == Kind::Add).count();
        assert_eq!(changed(Options::default()), 2);
        assert_eq!(changed(Options { context: 3, ignore_whitespace: true }), 1);
    }
}
