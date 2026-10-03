//! Lanes of the commit graph, laid out one commit at a time like VS Code's Source Control graph (`scmHistory.ts`).

/// The colour of a lane: the checked-out branch, its upstream, the base branch, or one of the rotating colours.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum LaneColor {
    Head,
    Upstream,
    Base,
    Lane(usize),
}

/// A line of the graph heading down to commit `sha`.
#[derive(Clone, Debug, PartialEq)]
pub struct Lane {
    pub sha: String,
    pub color: LaneColor,
}

/// One commit's row: the lanes entering it from above, those leaving it below, and the column and colour of its dot.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphRow {
    pub input: Vec<Lane>,
    pub output: Vec<Lane>,
    pub column: usize,
    pub color: LaneColor,
}

/// The lanes left open by the commits pushed so far, so the next page of history continues the graph.
#[derive(Clone, Debug, Default)]
pub struct Lanes {
    open: Vec<Lane>,
    next: usize,
}

impl Lanes {
    /// Lays out the next commit, newest first; `label` colours the lane leaving a branch tip.
    pub fn push(&mut self, sha: &str, parents: &[String], label: impl Fn(&str) -> Option<LaneColor>) -> GraphRow {
        let input = std::mem::take(&mut self.open);
        let column = input.iter().position(|l| l.sha == sha).unwrap_or(input.len());
        let mut output = Vec::with_capacity(input.len() + parents.len());
        let mut placed = false;
        for lane in &input {
            if lane.sha != sha {
                output.push(lane.clone());
            } else if !placed && !parents.is_empty() {
                output.push(Lane { sha: parents[0].clone(), color: label(sha).unwrap_or(lane.color) });
                placed = true;
            }
        }
        for (i, p) in parents.iter().enumerate().skip(placed as usize) {
            let color = if i == 0 { label(sha) } else { label(p) }.unwrap_or_else(|| self.next_color());
            output.push(Lane { sha: p.clone(), color });
        }
        let color = if parents.is_empty() { input.get(column) } else { output.get(column) }.map_or(LaneColor::Head, |l| l.color);
        self.open = output.clone();
        GraphRow { input, output, column, color }
    }

    fn next_color(&mut self) -> LaneColor {
        let color = LaneColor::Lane(self.next);
        self.next += 1;
        color
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(shas: &[&str]) -> Vec<String> {
        shas.iter().map(|s| s.to_string()).collect()
    }

    fn shas(lanes: &[Lane]) -> Vec<&str> {
        lanes.iter().map(|l| l.sha.as_str()).collect()
    }

    fn none(_: &str) -> Option<LaneColor> {
        None
    }

    #[test]
    fn a_commit_takes_the_first_lane_waiting_for_it() {
        let mut lanes = Lanes::default();
        lanes.push("c", &s(&["b"]), none);
        let row = lanes.push("b", &s(&["a"]), none);
        assert_eq!((row.column, shas(&row.input), shas(&row.output)), (0, vec!["b"], vec!["a"]));
        assert_eq!(row.output[0].color, row.input[0].color);
    }

    #[test]
    fn a_merge_opens_a_lane_for_its_second_parent() {
        let mut lanes = Lanes::default();
        let row = lanes.push("m", &s(&["a", "b"]), none);
        assert_eq!(shas(&row.output), vec!["a", "b"]);
        assert_ne!(row.output[0].color, row.output[1].color);
    }

    #[test]
    fn two_lanes_waiting_for_one_commit_converge_on_it() {
        let mut lanes = Lanes::default();
        lanes.push("m", &s(&["a", "b"]), none);
        let b = lanes.push("b", &s(&["a"]), none);
        assert_eq!((b.column, shas(&b.output)), (1, vec!["a", "a"]));
        let a = lanes.push("a", &s(&["z"]), none);
        assert_eq!((a.column, shas(&a.output)), (0, vec!["z"]));
    }

    #[test]
    fn lanes_right_of_a_closed_lane_shift_left() {
        let mut lanes = Lanes::default();
        lanes.push("t", &s(&["a", "b"]), none);
        lanes.push("u", &s(&["a"]), none);
        lanes.push("v", &s(&["c"]), none);
        let row = lanes.push("a", &s(&["z"]), none);
        assert_eq!(shas(&row.output), vec!["z", "b", "c"]);
    }

    #[test]
    fn a_root_commit_keeps_the_other_lanes_open() {
        let mut lanes = Lanes::default();
        lanes.push("t", &s(&["a", "b"]), none);
        let row = lanes.push("a", &[], none);
        assert_eq!(shas(&row.output), vec!["b"]);
    }

    #[test]
    fn a_lane_takes_the_upstream_colour_below_the_upstream_tip() {
        let label = |sha: &str| match sha {
            "h" => Some(LaneColor::Head),
            "u" => Some(LaneColor::Upstream),
            _ => None,
        };
        let mut lanes = Lanes::default();
        lanes.push("h", &s(&["u"]), label);
        let row = lanes.push("u", &s(&["p"]), label);
        assert_eq!(row.input[0].color, LaneColor::Head);
        assert_eq!(row.output, vec![Lane { sha: "p".into(), color: LaneColor::Upstream }]);
        assert_eq!(row.color, LaneColor::Upstream);
    }
}
