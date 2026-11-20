//! Lanes of the commit graph, laid out one commit at a time like VS Code's Source Control graph (`scmHistory.ts`).

use crate::{Runner, base_of, git, github, lines};

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

/// A commit as the graph shows it.
#[derive(Clone, Debug, PartialEq)]
pub struct GraphCommit {
    pub sha: String,
    pub parents: Vec<String>,
    pub author: String,
    pub subject: String,
}

/// `n` commits reachable from `revs`, children before parents, after skipping `skip`.
pub fn log_graph(run: &Runner, cwd: &str, revs: &[String], skip: usize, n: usize) -> Vec<GraphCommit> {
    let (skip, n) = (format!("--skip={skip}"), format!("-n{n}"));
    let mut args = vec!["log", "--format=%H%x00%P%x00%aN%x00%s", "--topo-order", skip.as_str(), n.as_str()];
    args.extend(revs.iter().map(String::as_str));
    args.push("--");
    lines(git(run, cwd, &args))
        .into_iter()
        .filter_map(|l| {
            let mut p = l.splitn(4, '\0');
            Some(GraphCommit {
                sha: p.next()?.into(),
                parents: p.next()?.split_whitespace().map(str::to_string).collect(),
                author: p.next()?.into(),
                subject: p.next()?.into(),
            })
        })
        .collect()
}

/// A branch the graph labels: its short name, the commit it points at, and whether it is a remote-tracking branch.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tip {
    pub name: String,
    pub sha: String,
    pub remote: bool,
}

/// What the graph follows: HEAD, the checked-out branch, its upstream and the base branch it grew from.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tips {
    pub head: String,
    pub branch: Option<Tip>,
    pub upstream: Option<Tip>,
    pub base: Option<Tip>,
    /// Every other branch's commit, when the graph follows them all.
    pub others: Vec<String>,
}

impl Tips {
    /// The commits the graph's history starts from, each once.
    pub fn revs(&self) -> Vec<String> {
        let mut revs = vec![self.head.clone()];
        let shas = [&self.upstream, &self.base].into_iter().flatten().map(|t| &t.sha).chain(&self.others);
        for sha in shas {
            if !revs.contains(sha) {
                revs.push(sha.clone());
            }
        }
        revs
    }

    /// The tips pointing at `sha`, with the colour each labels its lane with.
    pub fn pointing_at<'a>(&'a self, sha: &'a str) -> impl Iterator<Item = (LaneColor, &'a Tip)> {
        [(LaneColor::Head, &self.branch), (LaneColor::Upstream, &self.upstream), (LaneColor::Base, &self.base)]
            .into_iter()
            .filter_map(move |(c, t)| Some((c, t.as_ref().filter(|t| t.sha == sha)?)))
    }

    /// The colour of the lane leaving `sha`, when a tip points at it.
    pub fn color(&self, sha: &str) -> Option<LaneColor> {
        (sha == self.head).then_some(LaneColor::Head).or_else(|| self.pointing_at(sha).next().map(|(c, _)| c))
    }
}

/// The worktree's tips, with every branch and remote branch when `all`; `None` before its first commit.
pub fn tips(run: &Runner, cwd: &str, all: bool) -> Option<Tips> {
    let head = git(run, cwd, &["rev-parse", "--verify", "-q", "HEAD"])?.trim().to_string();
    let refs: Vec<(String, String, String)> = lines(git(run, cwd, &["for-each-ref", "--format=%(refname)%00%(objectname)%00%(upstream)", "refs/heads", "refs/remotes"]))
        .into_iter()
        .filter_map(|l| {
            let mut p = l.splitn(3, '\0');
            Some((p.next()?.to_string(), p.next()?.to_string(), p.next()?.to_string()))
        })
        .collect();
    let tip = |name: &str| {
        refs.iter().find(|(r, ..)| r == name).map(|(r, sha, _)| {
            let short = r.strip_prefix("refs/heads/").or_else(|| r.strip_prefix("refs/remotes/")).unwrap_or(r);
            Tip { name: short.to_string(), sha: sha.clone(), remote: r.starts_with("refs/remotes/") }
        })
    };
    let current = git(run, cwd, &["symbolic-ref", "-q", "HEAD"]).map(|s| s.trim().to_string());
    let branch = current.as_deref().and_then(tip);
    let upstream = current.as_deref().and_then(|c| refs.iter().find(|(r, ..)| r == c)).and_then(|(.., u)| tip(u));
    let name = current.as_deref().and_then(|c| c.strip_prefix("refs/heads/")).unwrap_or_default();
    let base = match name {
        "main" | "master" => None,
        _ => base_of(run, cwd, name).filter(|b| github::base_branch(b) != name).and_then(|b| tip(&format!("refs/remotes/{b}")).or_else(|| tip(&format!("refs/heads/{b}")))),
    };
    let others = if all { refs.into_iter().map(|(_, sha, _)| sha).collect() } else { Vec::new() };
    Some(Tips { head, branch, upstream, base, others })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tests::{committer, develop_ahead_of_master, scratch_repo, sh, sha};

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

    #[test]
    fn the_graph_follows_the_branch_a_branch_was_created_from() {
        let dir = develop_ahead_of_master("graph-grew-from");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        sh(&repo, &["switch", "-qc", "feature", "--no-track", "origin/develop"]);
        assert_eq!(tips(&Runner::Local, r, false).unwrap().base.map(|b| b.name).as_deref(), Some("origin/develop"));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn tips_follow_the_branch_its_upstream_and_main() {
        let dir = scratch_repo("tips");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        sh(&repo, &["branch", "-M", "main"]);
        sh(&repo, &["checkout", "-qb", "feature"]);
        sh(&repo, &["update-ref", "refs/remotes/origin/feature", "HEAD"]);
        for (k, v) in [("remote.origin.fetch", "+refs/heads/*:refs/remotes/origin/*"), ("branch.feature.remote", "origin"), ("branch.feature.merge", "refs/heads/feature")] {
            sh(&repo, &["config", k, v]);
        }
        std::fs::write(repo.join("b"), "b\n").unwrap();
        sh(&repo, &["add", "."]);
        sh(&repo, &["commit", "-qm", "b"]);
        let (head, main) = (sha(r, "HEAD"), sha(r, "main"));
        let t = tips(&Runner::Local, r, false).unwrap();
        assert_eq!(t.head, head);
        assert_eq!(t.branch, Some(Tip { name: "feature".into(), sha: head.clone(), remote: false }));
        assert_eq!(t.upstream, Some(Tip { name: "origin/feature".into(), sha: main.clone(), remote: true }));
        assert_eq!(t.base, Some(Tip { name: "main".into(), sha: main.clone(), remote: false }));
        assert_eq!(t.revs(), vec![head, main.clone()]);
        assert_eq!(t.color(&main), Some(LaneColor::Upstream));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn on_main_the_graph_has_no_base() {
        let dir = scratch_repo("tips-main");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        sh(&repo, &["branch", "-M", "main"]);
        let t = tips(&Runner::Local, r, false).unwrap();
        assert_eq!(t.revs(), vec![sha(r, "HEAD")]);
        assert_eq!(t.branch.map(|b| b.name).as_deref(), Some("main"));
        assert_eq!((t.upstream, t.base), (None, None));
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn following_every_branch_starts_the_graph_from_side_branches_too() {
        let dir = scratch_repo("tips-all");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        sh(&repo, &["branch", "-M", "main"]);
        sh(&repo, &["checkout", "-qb", "side"]);
        sh(&repo, &["commit", "-q", "--allow-empty", "-m", "side"]);
        sh(&repo, &["checkout", "-q", "main"]);
        let side = sha(r, "side");
        assert_eq!(tips(&Runner::Local, r, false).unwrap().revs(), vec![sha(r, "HEAD")]);
        assert_eq!(tips(&Runner::Local, r, true).unwrap().revs(), vec![sha(r, "HEAD"), side]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn log_graph_pages_through_history_children_first() {
        let dir = scratch_repo("log");
        let repo = dir.join("repo");
        let r = repo.to_str().unwrap();
        committer(&repo);
        for name in ["b", "c"] {
            std::fs::write(repo.join(name), name).unwrap();
            sh(&repo, &["add", "."]);
            sh(&repo, &["commit", "-qm", name]);
        }
        let head = vec![sha(r, "HEAD")];
        let all = log_graph(&Runner::Local, r, &head, 0, 50);
        assert_eq!(all.iter().map(|c| c.subject.as_str()).collect::<Vec<_>>(), ["c", "b", "init"]);
        assert_eq!(all[0].sha, head[0]);
        assert_eq!(all[0].parents, vec![all[1].sha.clone()]);
        assert_eq!(all[0].author, "t");
        assert!(all[2].parents.is_empty());
        assert_eq!(log_graph(&Runner::Local, r, &head, 1, 1), all[1..2].to_vec());
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
