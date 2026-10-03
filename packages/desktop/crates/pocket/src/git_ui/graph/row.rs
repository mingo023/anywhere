use super::ROW_GROUP;
use super::lanes::{self, Dot, H, MID, Mark, Step};
use crate::desktop::Desktop;
use git::Tip;
use git::graph::LaneColor;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;
use theme::*;
use ui::icon_button_sized;
use workspace::Doc;

fn lane_color(c: LaneColor) -> Hsla {
    match c {
        LaneColor::Head => GRAPH_HEAD.into(),
        LaneColor::Upstream => GRAPH_UPSTREAM.into(),
        LaneColor::Base => GRAPH_BASE.into(),
        LaneColor::Lane(i) => GRAPH_LANES[i % GRAPH_LANES.len()].into(),
    }
}

/// Paints `marks`; with `fade` lines fade out downwards, for the row still loading.
fn graph(marks: Vec<Mark>, width: f32, fade: bool) -> impl IntoElement {
    canvas(
        |_, _, _| {},
        move |bounds, _, win, _| {
            let at = |x: f32, y: f32| point(bounds.origin.x + px(x), bounds.origin.y + px(y));
            for mark in &marks {
                match mark {
                    Mark::Rect { x, y, w, h, color } => {
                        let c = lane_color(*color);
                        let bg: Background = if fade { linear_gradient(180., linear_color_stop(c, 0.), linear_color_stop(c.opacity(0.), 1.)) } else { c.into() };
                        win.paint_quad(fill(Bounds::new(at(*x, *y), size(px(*w), px(*h))), bg));
                    }
                    Mark::Curve { from, steps, color } => {
                        let mut path = PathBuilder::stroke(px(1.));
                        path.move_to(at(from.0, from.1));
                        for step in steps {
                            match *step {
                                Step::Line(x, y) => path.line_to(at(x, y)),
                                Step::Arc(r, clockwise, x, y) => path.arc_to(point(px(r), px(r)), px(0.), false, clockwise, at(x, y)),
                            }
                        }
                        if let Ok(path) = path.build() {
                            win.paint_path(path, lane_color(*color));
                        }
                    }
                    Mark::Dot { x, kind, color } => paint_dot(win, at(*x, MID), *kind, lane_color(*color)),
                }
            }
        },
    )
    .w(px(width))
    .h(px(H))
    .flex_none()
}

fn paint_dot(win: &mut Window, center: Point<Pixels>, kind: Dot, color: Hsla) {
    let disc = |d: f32| Bounds::new(point(center.x - px(d / 2.), center.y - px(d / 2.)), size(px(d), px(d)));
    match kind {
        Dot::Commit => win.paint_quad(fill(disc(8.), color).corner_radii(px(4.))),
        Dot::Merge => {
            win.paint_quad(quad(disc(10.), px(5.), transparent_black(), px(1.), color, BorderStyle::default()));
            win.paint_quad(fill(disc(4.), color).corner_radii(px(2.)));
        }
        Dot::Head => win.paint_quad(quad(disc(12.), px(6.), transparent_black(), px(2.), color, BorderStyle::default())),
    }
}

/// A branch label; only the first on a commit shows its name, the rest just their icon.
fn pill(color: LaneColor, tip: &Tip, named: bool) -> Div {
    div()
        .h(px(18.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(10.))
        .bg(lane_color(color))
        .child(div().size(px(18.)).flex().flex_none().items_center().justify_center().child(if tip.remote { icon("cloud", 14., CUTOUT) } else { icon("branch", 12., CUTOUT) }))
        .when(named, |d| d.child(div().max_w(px(100.)).truncate().pr(px(6.)).text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(CUTOUT).child(tip.name.clone())))
}

fn status_letter(status: char) -> Div {
    div()
        .w(px(14.))
        .flex()
        .flex_none()
        .justify_center()
        .font_family(MONO)
        .text_size(px(11.))
        .font_weight(FontWeight::BOLD)
        .text_color(ui::git_color(status))
        .child(status.to_string())
}

impl Desktop {
    pub(super) fn commit_row(&self, i: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let (commit, row, tips) = (&self.graph.commits[i], &self.graph.rows[i], &self.graph.tips);
        let sha = commit.sha.clone();
        let head = commit.sha == tips.head;
        let dot = if head {
            Dot::Head
        } else if commit.parents.len() > 1 {
            Dot::Merge
        } else {
            Dot::Commit
        };
        let marks = lanes::marks(row, commit, dot, self.graph.expanded.contains(&commit.sha));
        let pills: Vec<Div> = tips.pointing_at(&commit.sha).enumerate().map(|(k, (c, tip))| pill(c, tip, k == 0)).collect();
        div()
            .id(("graph-commit", i))
            .group(ROW_GROUP)
            .w_full()
            .h(px(H))
            .pr(px(4.))
            .flex()
            .flex_none()
            .items_center()
            .rounded(px(6.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_1))
            .child(graph(marks, lanes::width(row), false))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_baseline()
                    .gap(px(6.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(div().flex_none().max_w_full().truncate().text_size(px(13.)).when(head, |d| d.font_weight(FontWeight::SEMIBOLD)).child(commit.subject.clone()))
                    .child(div().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_3).when(head, |d| d.font_weight(FontWeight::SEMIBOLD)).child(commit.author.clone())),
            )
            .when(!pills.is_empty(), |d| d.child(div().ml(px(4.)).flex().flex_none().gap(px(4.)).children(pills)))
            // Collapsed by width, not `hidden()`: gpui panics when a group hover flips `display` between prepaint and paint.
            .child(
                div().flex().flex_none().w_0().overflow_hidden().group_hover(ROW_GROUP, |s| s.w_auto()).child(
                    icon_button_sized(("graph-open", i), "diff-multiple", 20., TEXT_2)
                        .ml(px(4.))
                        .flex_none()
                        .rounded(px(5.))
                        .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                            cx.stop_propagation();
                            this.open_doc(Doc::Commit(sha.clone()), ev.click_count() > 1, cx);
                        })),
                ),
            )
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_commit(i, cx)))
    }

    pub(super) fn file_row(&self, ix: usize, i: usize, j: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let (commit, row) = (&self.graph.commits[i], &self.graph.rows[i]);
        let f = &self.graph.files[&commit.sha][j];
        let (dir, name) = f.path.rsplit_once('/').unwrap_or(("", &f.path));
        let selected = self.diff.at.as_ref() == Some(&commit.sha) && self.diff.file.as_ref() == Some(&f.path);
        div()
            .id(("graph-file", ix))
            .w_full()
            .h(px(H))
            .pr(px(4.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(6.))
            .rounded(px(6.))
            .cursor_pointer()
            .when(selected, |d| d.bg(FILL_3))
            .when(!selected, |d| d.hover(|s| s.bg(FILL_1)))
            .child(graph(lanes::through(row, !commit.parents.is_empty()), lanes::width(row), false))
            .child(file_icon(name, false, false, 14.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_baseline()
                    .gap(px(6.))
                    .overflow_hidden()
                    .whitespace_nowrap()
                    .child(div().flex_none().max_w_full().truncate().text_size(px(13.)).when(f.status == 'D', |d| d.line_through()).child(name.to_string()))
                    .when(!dir.is_empty(), |d| d.child(div().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_4).child(dir.to_string()))),
            )
            .child(status_letter(f.status))
            .on_click(cx.listener(move |this, ev: &ClickEvent, _, cx| {
                if let Some(doc) = this.graph.file_doc(i, j) {
                    this.open_doc(doc, ev.click_count() > 1, cx);
                }
            }))
    }

    /// The row under the last commit while more history may follow; drawing it loads the next page.
    pub(super) fn more_row(&self, cx: &mut Context<Self>) -> Div {
        let (marks, width) = self.graph.rows.last().map(|r| (lanes::through(r, false), lanes::width(r))).unwrap_or_default();
        let bar = div().flex_1().h(px(14.)).mr(px(8.)).rounded(px(4.)).bg(FILL_2);
        let bar = if cx.reduce_motion() {
            bar.opacity(0.5).into_any_element()
        } else {
            bar.with_animation("graph-more", Animation::new(Duration::from_millis(1500)).repeat().with_easing(pulsating_between(0.3, 0.7)), |d, t| d.opacity(t)).into_any_element()
        };
        div().w_full().h(px(H)).flex().flex_none().items_center().child(graph(marks, width, true)).child(bar)
    }
}
