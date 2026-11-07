//! What the capsule, its alert and its list say of a team: plain data from `Team`, drawn elsewhere.

use super::plan::after_label;
use super::{PROPOSED, plural};
use agents::team::state::{RUNNING, WAITING};
use agents::{Team, TeamTask};

/// How a task's state shows, as a ring on the capsule and a glyph in the list.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    Waiting,
    Running,
    NeedsYou,
    Done,
    Failed,
    Stopped,
    Merged,
}

pub(crate) fn mark(state: &str) -> Mark {
    match state {
        RUNNING => Mark::Running,
        "needsYou" => Mark::NeedsYou,
        "done" => Mark::Done,
        "failed" => Mark::Failed,
        "stopped" => Mark::Stopped,
        "merged" => Mark::Merged,
        _ => Mark::Waiting,
    }
}

/// How much of a task's ring is drawn. A task reports no progress until it hands back, so a running one is a quarter arc that turns.
pub(crate) fn fill(m: Mark) -> f32 {
    match m {
        Mark::Waiting => 0.,
        Mark::Running => 0.25,
        _ => 1.,
    }
}

#[derive(Debug, Default, PartialEq)]
pub(crate) struct Counts {
    pub(crate) working: usize,
    pub(crate) needs: usize,
    pub(crate) waiting: usize,
    pub(crate) done: usize,
    pub(crate) failed: usize,
    pub(crate) stopped: usize,
}

pub(crate) fn counts(tasks: &[TeamTask]) -> Counts {
    let mut c = Counts::default();
    for t in tasks {
        match mark(&t.state) {
            Mark::Running => c.working += 1,
            Mark::NeedsYou => c.needs += 1,
            Mark::Waiting => c.waiting += 1,
            Mark::Done | Mark::Merged => c.done += 1,
            Mark::Failed => c.failed += 1,
            Mark::Stopped => c.stopped += 1,
        }
    }
    c
}

/// "1 needs you · 2 working · 1 of 4 done": the list's header.
pub(crate) fn summary(c: &Counts, n: usize) -> String {
    let parts = [(c.needs, "needs you"), (c.working, "working"), (c.waiting, "queued"), (c.stopped, "stopped"), (c.failed, "failed")];
    parts.iter().filter(|(k, _)| *k > 0).map(|(k, w)| format!("{k} {w}")).chain([format!("{} of {n} done", c.done)]).collect::<Vec<_>>().join(" · ")
}

/// What a task is doing, for its row in the list.
pub(crate) fn now_text(t: &TeamTask, tasks: &[TeamTask]) -> String {
    match t.state.as_str() {
        "planned" | WAITING => {
            let unfinished: Vec<String> = t.after.iter().filter(|a| tasks.iter().any(|x| &x.id == *a && mark(&x.state) != Mark::Done)).cloned().collect();
            let ids: Vec<&str> = tasks.iter().map(|t| t.id.as_str()).collect();
            after_label(&unfinished, &ids).map_or_else(|| "Queued".into(), |a| format!("Starts {a}"))
        }
        RUNNING => "Working".into(),
        "needsYou" => "Needs you".into(),
        "done" => "Handed back".into(),
        "failed" => "Failed".into(),
        "stopped" => "Stopped".into(),
        "merged" => "Merged".into(),
        other => other.into(),
    }
}

/// The first task waiting on you; the alert is about it.
pub(crate) fn needing(team: &Team) -> Option<&TeamTask> {
    team.tasks.iter().find(|t| t.state == "needsYou")
}

/// What a press on the capsule opens.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Click {
    Sheet,
    List,
}

/// The capsule's button.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Cta {
    Review,
    Merge { ready: bool },
}

#[derive(Debug, PartialEq)]
pub(crate) struct Capsule {
    pub(crate) label: String,
    pub(crate) detail: String,
    /// A ring per task leads the label.
    pub(crate) rings: bool,
    pub(crate) click: Click,
    pub(crate) cta: Option<Cta>,
    /// Amber: something failed or isn't ready.
    pub(crate) warn: bool,
}

/// How long a merged team's capsule stays, in ms.
const MERGED_FOR: i64 = 4_000;

/// The capsule over the lead's chat; none while the lead plans, nor once a merge is a few seconds old.
/// `kept` is how many tasks the sheet keeps, once it's been edited; `into` is the lead's branch.
pub(crate) fn capsule(team: &Team, kept: Option<usize>, lead_working: bool, into: &str, now: i64) -> Option<Capsule> {
    let (c, n) = (counts(&team.tasks), team.tasks.len());
    let at = |label: String, detail: String| Capsule { label, detail, rings: true, click: Click::List, cta: None, warn: false };
    Some(match team.state.as_str() {
        PROPOSED => Capsule { rings: false, click: Click::Sheet, cta: Some(Cta::Review), ..at("Team plan".into(), format!("· {}", plural(kept.unwrap_or(n), "agent", "agents"))) },
        RUNNING => {
            let label = if c.working > 0 {
                format!("{} working", c.working)
            } else if c.waiting > 0 {
                format!("{} queued", c.waiting)
            } else {
                "None working".into()
            };
            let failed = if c.failed > 0 { format!(" · {} failed", c.failed) } else { String::new() };
            Capsule { warn: c.failed > 0, ..at(label, format!("· {} of {n} done{failed}", c.done)) }
        }
        "review" => {
            let merge = Some(Cta::Merge { ready: team.ready });
            if team.ready {
                Capsule { cta: merge, ..at("Ready to merge".into(), format!("· {n} handed back")) }
            } else if lead_working {
                Capsule { cta: merge, ..at("The lead is reviewing…".into(), String::new()) }
            } else {
                Capsule { cta: merge, warn: true, ..at(format!("{n} handed back"), "· not marked ready".into()) }
            }
        }
        "merging" => at(format!("Merging into {into}…"), String::new()),
        "merged" if now - team.updated_at < MERGED_FOR => at(format!("Merged into {into}"), format!("· {}", plural(n, "branch", "branches"))),
        "conflict" => Capsule { warn: true, cta: Some(Cta::Merge { ready: team.ready }), ..at("Merge stopped".into(), format!("· conflict in {}", plural(team.conflict.len(), "file", "files"))) },
        "failed" => Capsule { warn: true, ..at("Team failed".into(), String::new()) },
        _ => return None,
    })
}

/// The list's last line once every task has handed back.
#[derive(Debug, PartialEq)]
pub(crate) struct Verdict {
    pub(crate) text: String,
    pub(crate) warn: bool,
    /// Where a merge conflicted.
    pub(crate) files: Vec<String>,
    /// Merge is offered; true when the lead said it's ready.
    pub(crate) merge: Option<bool>,
}

/// The lead's verdict and Merge, the merge underway or done, or why it stopped; none before every task hands back.
pub(crate) fn verdict(team: &Team, lead_working: bool, into: &str) -> Option<Verdict> {
    let v = |text: String, warn: bool| Verdict { text, warn, files: Vec::new(), merge: None };
    Some(match team.state.as_str() {
        "review" if team.ready => Verdict { merge: Some(true), ..v("The lead says it's ready to merge.".into(), false) },
        "review" if lead_working => Verdict { merge: Some(false), ..v("The lead is reviewing the handbacks…".into(), false) },
        "review" => Verdict { merge: Some(false), ..v("The lead hasn't said it's ready to merge. Read its review before you merge.".into(), true) },
        "merging" => v(format!("Merging into {into}…"), false),
        "merged" => v(format!("Merged into {into}. Worktrees removed, branches kept."), false),
        "conflict" => Verdict { files: team.conflict.clone(), merge: Some(team.ready), ..v("The merge conflicted in these files and was undone; nothing after it was merged. The lead was told. Merge again once they're fixed.".into(), true) },
        "failed" => v(if team.error.is_empty() { "The team stopped.".into() } else { team.error.clone() }, true),
        _ => return None,
    })
}

/// The lines the handed-back tasks added and removed, from their stats.
pub(crate) fn totals(tasks: &[TeamTask]) -> Option<(usize, usize)> {
    let stats: Vec<(usize, usize, usize)> = tasks.iter().filter_map(|t| stat_counts(&t.stat)).collect();
    (!stats.is_empty()).then(|| stats.iter().fold((0, 0), |(a, r), s| (a + s.1, r + s.2)))
}

/// The (files, added, removed) a `git diff --stat` sums up on its last line; none without that line.
pub(crate) fn stat_counts(stat: &str) -> Option<(usize, usize, usize)> {
    let last = stat.lines().rev().find(|l| l.contains("changed"))?;
    let mut out = (0, 0, 0);
    for part in last.split(',') {
        let mut words = part.split_whitespace();
        let (Some(n), Some(word)) = (words.next().and_then(|n| n.parse().ok()), words.next()) else { continue };
        match word {
            w if w.starts_with("file") => out.0 = n,
            w if w.starts_with("insertion") => out.1 = n,
            w if w.starts_with("deletion") => out.2 = n,
            _ => {}
        }
    }
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, state: &str, after: &[&str]) -> TeamTask {
        TeamTask { id: id.into(), state: state.into(), after: after.iter().map(|a| a.to_string()).collect(), ..Default::default() }
    }

    fn team(state: &str, tasks: Vec<TeamTask>) -> Team {
        Team { lead: "lead".into(), state: state.into(), tasks, updated_at: 1_000, ..Default::default() }
    }

    #[test]
    fn each_task_state_has_its_mark_and_ring() {
        let states = ["planned", "waiting", "running", "needsYou", "done", "failed", "stopped", "merged"];
        let shown = states.map(|s| (mark(s), fill(mark(s))));
        assert_eq!(
            shown,
            [(Mark::Waiting, 0.), (Mark::Waiting, 0.), (Mark::Running, 0.25), (Mark::NeedsYou, 1.), (Mark::Done, 1.), (Mark::Failed, 1.), (Mark::Stopped, 1.), (Mark::Merged, 1.)]
        );
    }

    #[test]
    fn the_list_header_counts_what_each_task_is_doing() {
        let tasks = vec![task("t1", "done", &[]), task("t2", "waiting", &["t1"]), task("t3", "needsYou", &[]), task("t4", "running", &[])];
        assert_eq!(summary(&counts(&tasks), 4), "1 needs you · 1 working · 1 queued · 1 of 4 done");
    }

    #[test]
    fn a_waiting_task_says_which_task_it_starts_after_until_that_one_hands_back() {
        let waiting = |first: &str| vec![task("t1", first, &[]), task("t2", "waiting", &["t1"])];
        let (running, done) = (waiting("running"), waiting("done"));
        assert_eq!([now_text(&running[1], &running), now_text(&done[1], &done)], ["Starts after 1", "Queued"]);
    }

    #[test]
    fn a_proposed_team_s_capsule_offers_its_plan() {
        let t = team("proposed", vec![task("t1", "planned", &[]), task("t2", "planned", &[])]);
        assert_eq!(capsule(&t, None, false, "main", 0), Some(Capsule { label: "Team plan".into(), detail: "· 2 agents".into(), rings: false, click: Click::Sheet, cta: Some(Cta::Review), warn: false }));
    }

    #[test]
    fn a_proposed_team_s_capsule_counts_the_tasks_the_sheet_keeps() {
        let t = team("proposed", vec![task("t1", "planned", &[]), task("t2", "planned", &[])]);
        assert_eq!(capsule(&t, Some(1), false, "main", 0).map(|c| c.detail), Some("· 1 agent".into()));
    }

    #[test]
    fn a_running_team_s_capsule_counts_its_workers() {
        let t = team("running", vec![task("t1", "done", &[]), task("t2", "running", &[]), task("t3", "failed", &[])]);
        assert_eq!(capsule(&t, None, false, "main", 0), Some(Capsule { label: "1 working".into(), detail: "· 1 of 3 done · 1 failed".into(), rings: true, click: Click::List, cta: None, warn: true }));
    }

    #[test]
    fn a_team_still_planning_shows_no_capsule() {
        assert_eq!(capsule(&team("planning", Vec::new()), None, false, "main", 0), None);
    }

    #[test]
    fn the_alert_is_for_the_first_task_that_needs_you() {
        let t = team("running", vec![task("t1", "running", &[]), task("t2", "needsYou", &[]), task("t3", "needsYou", &[])]);
        assert_eq!(needing(&t).map(|t| t.id.as_str()), Some("t2"));
    }

    #[test]
    fn a_stat_sums_up_on_its_last_line() {
        let stats = [" a.rs | 3 ++-\n 2 files changed, 64 insertions(+), 9 deletions(-)", " 1 file changed, 1 insertion(+)", ""];
        assert_eq!(stats.map(stat_counts), [Some((2, 64, 9)), Some((1, 1, 0)), None]);
    }

    #[test]
    fn a_reviewed_team_s_capsule_says_whether_the_lead_passed_it() {
        let done = || vec![task("t1", "done", &[]), task("t2", "done", &[])];
        let ready = Team { ready: true, ..team("review", done()) };
        let merge = |ready| Some(Cta::Merge { ready });
        let shown = [capsule(&ready, None, false, "main", 0), capsule(&team("review", done()), None, true, "main", 0), capsule(&team("review", done()), None, false, "main", 0)];
        assert_eq!(
            shown,
            [
                Some(Capsule { label: "Ready to merge".into(), detail: "· 2 handed back".into(), rings: true, click: Click::List, cta: merge(true), warn: false }),
                Some(Capsule { label: "The lead is reviewing…".into(), detail: String::new(), rings: true, click: Click::List, cta: merge(false), warn: false }),
                Some(Capsule { label: "2 handed back".into(), detail: "· not marked ready".into(), rings: true, click: Click::List, cta: merge(false), warn: true }),
            ]
        );
    }

    #[test]
    fn a_merge_s_capsule_stays_a_few_seconds_and_a_conflict_s_until_it_s_dealt_with() {
        let merged = team("merged", vec![task("t1", "merged", &[])]);
        let conflict = Team { conflict: vec!["a.rs".into(), "b.rs".into()], ..team("conflict", vec![task("t1", "merged", &[]), task("t2", "done", &[])]) };
        let label = |c: Option<Capsule>| c.map(|c| (c.label, c.detail, c.warn));
        assert_eq!(
            [label(capsule(&merged, None, false, "main", 2_000)), label(capsule(&merged, None, false, "main", 9_000)), label(capsule(&conflict, None, false, "main", 9_000))],
            [Some(("Merged into main".to_string(), "· 1 branch".to_string(), false)), None, Some(("Merge stopped".to_string(), "· conflict in 2 files".to_string(), true))]
        );
    }

    #[test]
    fn the_list_ends_with_the_lead_s_verdict() {
        let done = || vec![task("t1", "done", &[])];
        let conflict = Team { conflict: vec!["a.rs".into()], ..team("conflict", done()) };
        let shown = [verdict(&Team { ready: true, ..team("review", done()) }, false, "main"), verdict(&team("review", done()), false, "main"), verdict(&conflict, false, "main"), verdict(&team("running", done()), false, "main")];
        let v = |text: &str, warn, files: &[&str], merge| Some(Verdict { text: text.into(), warn, files: files.iter().map(|f| f.to_string()).collect(), merge });
        assert_eq!(
            shown,
            [
                v("The lead says it's ready to merge.", false, &[], Some(true)),
                v("The lead hasn't said it's ready to merge. Read its review before you merge.", true, &[], Some(false)),
                v("The merge conflicted in these files and was undone; nothing after it was merged. The lead was told. Merge again once they're fixed.", true, &["a.rs"], Some(false)),
                None,
            ]
        );
    }

    #[test]
    fn totals_sum_the_handed_back_stats() {
        let stat = |s: &str| TeamTask { stat: s.into(), ..task("t", "done", &[]) };
        assert_eq!(totals(&[stat(" 2 files changed, 64 insertions(+), 9 deletions(-)"), stat(" 1 file changed, 3 insertions(+)"), task("t3", "running", &[])]), Some((67, 9)));
        assert_eq!(totals(&[task("t1", "running", &[])]), None);
    }
}
