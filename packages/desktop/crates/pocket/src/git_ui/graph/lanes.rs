use git::GraphCommit;
use git::graph::{GraphRow, LaneColor};

pub const W: f32 = 11.;
pub const H: f32 = 22.;
pub const MID: f32 = H / 2.;
const CURVE: f32 = 5.;

/// How a commit's dot is drawn: a disc, a ring around a disc for a merge, a thick ring for HEAD.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dot {
    Commit,
    Merge,
    Head,
}

impl Dot {
    pub fn radius(self) -> f32 {
        match self {
            Dot::Commit => 4.,
            Dot::Merge => 5.,
            Dot::Head => 6.,
        }
    }
}

/// A stretch of a curve: a line to a point, or an arc of a radius, clockwise or not, to a point.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Step {
    Line(f32, f32),
    Arc(f32, bool, f32, f32),
}

/// What a row of the graph paints, in pixels from the row's top-left.
#[derive(Clone, Debug, PartialEq)]
pub enum Mark {
    Rect { x: f32, y: f32, w: f32, h: f32, color: LaneColor },
    Curve { from: (f32, f32), steps: Vec<Step>, color: LaneColor },
    Dot { x: f32, kind: Dot, color: LaneColor },
}

fn x(i: usize) -> f32 {
    W * (i + 1) as f32
}

fn vline(x: f32, y0: f32, y1: f32, w: f32, color: LaneColor) -> Mark {
    Mark::Rect { x: x - w / 2., y: y0, w, h: y1 - y0, color }
}

fn hline(x0: f32, x1: f32, color: LaneColor) -> Mark {
    Mark::Rect { x: x0.min(x1), y: MID - 0.5, w: (x1 - x0).abs(), h: 1., color }
}

pub fn width(row: &GraphRow) -> f32 {
    W * (row.input.len().max(row.output.len()).max(row.column + 1) + 1) as f32
}

/// A commit's row. Lines stop short of the dot rather than hiding under a halo, because the sidebar behind them is translucent.
pub fn marks(row: &GraphRow, commit: &GraphCommit, dot: Dot, expanded: bool) -> Vec<Mark> {
    let ci = row.column;
    let cx = x(ci);
    let gap = dot.radius() + 2.;
    let theta = 2. * (gap / H).asin();
    let has_parents = !commit.parents.is_empty();
    let mut out = Vec::new();
    let mut o = 0;
    for (i, lane) in row.input.iter().enumerate() {
        if lane.sha == commit.sha {
            if i == ci {
                o += has_parents as usize;
            } else if i == ci + 1 {
                out.push(Mark::Curve { from: (x(i), 0.), steps: vec![Step::Arc(W, true, cx + W * theta.sin(), W * theta.cos())], color: lane.color });
            } else {
                out.push(Mark::Curve { from: (x(i), 0.), steps: vec![Step::Arc(W, true, W * i as f32, MID)], color: lane.color });
                out.push(hline(cx + gap, W * i as f32, lane.color));
            }
        } else {
            if i == o {
                out.push(vline(x(i), 0., H, 1., lane.color));
            } else {
                let (from, to) = (x(i), x(o));
                let steps = vec![Step::Line(from, MID - CURVE), Step::Arc(CURVE, true, from - CURVE, MID), Step::Line(to + CURVE, MID), Step::Arc(CURVE, false, to, MID + CURVE), Step::Line(to, H)];
                out.push(Mark::Curve { from: (from, 0.), steps, color: lane.color });
            }
            o += 1;
        }
    }
    for p in commit.parents.iter().skip(1) {
        let Some(po) = row.output.iter().rposition(|l| l.sha == *p) else { continue };
        let color = row.output[po].color;
        let start = if po == ci + 1 {
            (cx + W * theta.sin(), H - W * theta.cos())
        } else {
            out.push(hline(cx + gap, W * po as f32, color));
            (W * po as f32, MID)
        };
        out.push(Mark::Curve { from: start, steps: vec![Step::Arc(W, true, x(po), H)], color });
    }
    if let Some(lane) = row.input.get(ci).filter(|l| l.sha == commit.sha) {
        out.push(vline(cx, 0., MID - gap, 1., lane.color));
    }
    if has_parents {
        out.push(vline(cx, MID + gap, H, if expanded { 3. } else { 1. }, row.color));
    }
    out.push(Mark::Dot { x: cx, kind: dot, color: row.color });
    out
}

/// The lanes running straight past the rows under a commit: its files, or the row loading more; `thick` marks the commit's own lane as expanded.
pub fn through(row: &GraphRow, thick: bool) -> Vec<Mark> {
    row.output.iter().enumerate().map(|(i, l)| vline(x(i), 0., H, if thick && i == row.column { 3. } else { 1. }, l.color)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use git::graph::Lanes;

    fn commit(sha: &str, parents: &[&str]) -> GraphCommit {
        GraphCommit { sha: sha.into(), parents: parents.iter().map(|p| p.to_string()).collect(), author: String::new(), subject: String::new() }
    }

    fn rows(commits: &[GraphCommit]) -> Vec<GraphRow> {
        let mut lanes = Lanes::default();
        commits.iter().map(|c| lanes.push(&c.sha, &c.parents, |_| None)).collect()
    }

    fn dist(a: (f32, f32), b: (f32, f32)) -> f32 {
        ((a.0 - b.0).powi(2) + (a.1 - b.1).powi(2)).sqrt()
    }

    #[test]
    fn a_lane_passing_by_draws_one_straight_line() {
        let c = [commit("m", &["a", "b"]), commit("b", &["q"])];
        let row = &rows(&c)[1];
        let left: Vec<Mark> = marks(row, &c[1], Dot::Commit, false).into_iter().filter(|m| matches!(m, Mark::Rect { x, .. } if *x < W)).collect();
        assert_eq!(left, vec![Mark::Rect { x: 10.5, y: 0., w: 1., h: 22., color: row.input[0].color }]);
    }

    #[test]
    fn lines_stop_short_of_the_dot() {
        let c = [commit("m", &["a", "b"]), commit("b", &["q"])];
        let row = &rows(&c)[1];
        let m = marks(row, &c[1], Dot::Commit, false);
        assert!(m.contains(&Mark::Rect { x: 21.5, y: 0., w: 1., h: 5., color: row.input[1].color }));
        assert!(m.contains(&Mark::Rect { x: 21.5, y: 17., w: 1., h: 5., color: row.color }));
        assert_eq!(m.last(), Some(&Mark::Dot { x: 22., kind: Dot::Commit, color: row.color }));
    }

    #[test]
    fn a_merge_curve_starts_outside_the_dot() {
        let c = [commit("m", &["a", "b"])];
        let m = marks(&rows(&c)[0], &c[0], Dot::Merge, false);
        let Some(Mark::Curve { from, steps, .. }) = m.iter().find(|m| matches!(m, Mark::Curve { .. })) else { panic!("no curve") };
        assert!((dist(*from, (11., 11.)) - 7.).abs() < 1e-3);
        assert_eq!(steps, &vec![Step::Arc(11., true, 22., 22.)]);
    }

    #[test]
    fn a_converging_lane_ends_its_curve_outside_the_dot() {
        let c = [commit("m", &["a", "b"]), commit("b", &["a"]), commit("a", &["z"])];
        let m = marks(&rows(&c)[2], &c[2], Dot::Commit, false);
        let Some(Mark::Curve { from, steps, .. }) = m.iter().find(|m| matches!(m, Mark::Curve { .. })) else { panic!("no curve") };
        assert_eq!(*from, (22., 0.));
        let [Step::Arc(_, _, x, y)] = steps.as_slice() else { panic!("not one arc") };
        assert!((dist((*x, *y), (11., 11.)) - 6.).abs() < 1e-3);
    }
}
