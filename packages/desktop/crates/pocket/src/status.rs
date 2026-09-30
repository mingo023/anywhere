use agents::Summary;
use std::collections::{HashMap, HashSet};

/// Variants run from most to least urgent; cards and roll-ups sort on that order.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Status {
    NeedsYou,
    Failed,
    Done,
    Working,
    Idle,
}

impl Status {
    pub fn of(a: &Summary) -> Option<Status> {
        if !a.attached {
            return None;
        }
        match a.status.as_str() {
            "needsYou" => Some(Status::NeedsYou),
            "done" if a.failed => Some(Status::Failed),
            "done" => Some(Status::Done),
            "working" => Some(Status::Working),
            "idle" => Some(Status::Idle),
            _ => None,
        }
    }

    pub fn alerting(&self) -> bool {
        matches!(self, Status::NeedsYou | Status::Failed | Status::Done)
    }

    pub fn label(self) -> &'static str {
        match self {
            Status::NeedsYou => "Needs you",
            Status::Failed => "Failed",
            Status::Done => "Done",
            Status::Working => "Working",
            Status::Idle => "Idle",
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Agent,
    NotAttached,
}

#[derive(Clone)]
pub struct Card {
    pub id: String,
    pub provider: String,
    pub title: String,
    pub cwd: String,
    pub at: i64,
    pub status: Status,
    pub kind: Kind,
}

/// An agent's card, placed by the folder its terminal started in.
pub fn card(a: &Summary, cwd: &str) -> Card {
    let kind = if a.attached { Kind::Agent } else { Kind::NotAttached };
    Card {
        id: a.id.clone(),
        provider: a.provider.clone(),
        title: if a.title.is_empty() { "New session".into() } else { a.title.clone() },
        cwd: cwd.to_string(),
        at: a.updated_at,
        status: Status::of(a).unwrap_or(Status::Idle),
        kind,
    }
}

pub fn section(status: Status, today: bool) -> usize {
    match status {
        Status::NeedsYou => 0,
        Status::Failed | Status::Done => 1,
        Status::Working => 2,
        Status::Idle if today => 3,
        Status::Idle => 4,
    }
}

/// A group's most urgent status and how many of its cards share that section, so Failed counts as Done.
pub fn roll_up(statuses: impl IntoIterator<Item = Status>) -> Option<(Status, usize)> {
    let all: Vec<Status> = statuses.into_iter().collect();
    let top = all.iter().copied().min().filter(|s| *s != Status::Idle)?;
    Some((top, all.iter().filter(|s| section(**s, true) == section(top, true)).count()))
}

/// Why Pocket can't see the status of a live agent that is not attached.
pub fn banner(a: &Summary) -> Option<&'static str> {
    if a.attached {
        return None;
    }
    match a.provider.as_str() {
        "claude" => Some("Claude skips hooks in folders it doesn't trust. Trust this folder in Claude to see status."),
        "codex" => Some("This codex runs without the app-server, so Pocket can't see its status."),
        _ => None,
    }
}

/// The agents in `panes`, sorted so an unchanged view compares equal.
pub fn view_set(panes: &[String], agents: &[Summary]) -> Vec<String> {
    let mut ids: Vec<String> = agents.iter().filter(|a| panes.contains(&a.terminal_id)).map(|a| a.id.clone()).collect();
    ids.sort();
    ids
}

/// The agents to notify about and those whose notification should go, once statuses went from `before` to `now`.
pub fn alerts(before: &HashMap<String, Status>, now: &HashMap<String, Status>, shown: &HashSet<String>, viewing: &[String]) -> (Vec<String>, Vec<String>) {
    let show = now.iter().filter(|(id, s)| s.alerting() && before.get(*id).is_some_and(|b| b != *s) && !viewing.contains(id)).map(|(id, _)| id.clone()).collect();
    let dismiss = shown.iter().filter(|id| !now.get(*id).is_some_and(Status::alerting) || viewing.contains(id)).cloned().collect();
    (show, dismiss)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn agent(terminal: &str, status: &str) -> Summary {
        Summary { id: format!("agent-{terminal}"), terminal_id: terminal.into(), status: status.into(), attached: true, ..Default::default() }
    }

    #[test]
    fn maps_the_wire_status() {
        let failed = Summary { failed: true, ..agent("t", "done") };
        let got: Vec<_> = [agent("t", "needsYou"), failed, agent("t", "done"), agent("t", "working"), agent("t", "idle"), agent("t", "closed")].iter().map(Status::of).collect();
        assert_eq!(got, vec![Some(Status::NeedsYou), Some(Status::Failed), Some(Status::Done), Some(Status::Working), Some(Status::Idle), None]);
    }

    #[test]
    fn ignores_the_status_of_an_agent_that_is_not_attached() {
        assert_eq!(Status::of(&Summary { attached: false, ..agent("t", "needsYou") }), None);
    }

    #[test]
    fn sorts_by_urgency() {
        let mut s = vec![Status::Idle, Status::Working, Status::Done, Status::Failed, Status::NeedsYou];
        s.sort();
        assert_eq!(s, vec![Status::NeedsYou, Status::Failed, Status::Done, Status::Working, Status::Idle]);
    }

    #[test]
    fn a_card_is_its_agent_in_its_terminals_folder() {
        let a = Summary { title: "Fix".into(), cwd: "/elsewhere".into(), updated_at: 5, ..agent("t1", "working") };
        let c = card(&a, "/w");
        assert_eq!((c.id.as_str(), c.title.as_str(), c.cwd.as_str(), c.at, c.status, c.kind), ("agent-t1", "Fix", "/w", 5, Status::Working, Kind::Agent));
    }

    #[test]
    fn a_detached_agent_reads_not_attached_and_idle() {
        let c = card(&Summary { attached: false, ..agent("t1", "needsYou") }, "/w");
        assert_eq!((c.kind, c.status), (Kind::NotAttached, Status::Idle));
    }

    #[test]
    fn an_untitled_agent_reads_new_session() {
        assert_eq!(card(&agent("t1", "working"), "/w").title, "New session");
    }

    #[test]
    fn sections_put_failed_with_done_and_split_idle_by_day() {
        let got: Vec<_> = [(Status::NeedsYou, false), (Status::Failed, false), (Status::Done, true), (Status::Working, true), (Status::Idle, true), (Status::Idle, false)].iter().map(|(s, t)| section(*s, *t)).collect();
        assert_eq!(got, vec![0, 1, 1, 2, 3, 4]);
    }

    #[test]
    fn rolls_up_the_most_urgent_status_counting_failed_as_done() {
        use Status::*;
        assert_eq!(roll_up([Working, Done, Failed, Idle]), Some((Failed, 2)));
        assert_eq!(roll_up([Working, NeedsYou, Done, NeedsYou]), Some((NeedsYou, 2)));
        assert_eq!(roll_up([Idle, Working]), Some((Working, 1)));
        assert_eq!(roll_up([Idle]), None);
    }

    #[test]
    fn banners_a_live_agent_that_is_not_attached() {
        let blind = |provider: &str, status: &str| Summary { provider: provider.into(), attached: false, ..agent("t", status) };
        assert_eq!(banner(&blind("claude", "idle")), Some("Claude skips hooks in folders it doesn't trust. Trust this folder in Claude to see status."));
        assert_eq!(banner(&blind("codex", "working")), Some("This codex runs without the app-server, so Pocket can't see its status."));
        assert_eq!(banner(&Summary { provider: "claude".into(), ..agent("t", "idle") }), None);
    }

    #[test]
    fn views_the_agents_of_visible_panes() {
        let agents = [Summary { id: "b".into(), ..agent("t2", "idle") }, Summary { id: "a".into(), ..agent("t1", "working") }, agent("t3", "idle")];
        assert_eq!(view_set(&["t1".into(), "t2".into()], &agents), vec!["a", "b"]);
    }

    fn statuses(list: &[(&str, Status)]) -> HashMap<String, Status> {
        list.iter().map(|(id, s)| (id.to_string(), *s)).collect()
    }

    #[test]
    fn alerts_when_an_unseen_agent_enters_needs_you_or_done() {
        let before = statuses(&[("a", Status::Working), ("b", Status::Working), ("c", Status::Idle), ("d", Status::Done), ("f", Status::Working), ("g", Status::Working), ("h", Status::Done)]);
        let now = statuses(&[("a", Status::Done), ("b", Status::Done), ("c", Status::Working), ("d", Status::Done), ("e", Status::NeedsYou), ("f", Status::NeedsYou), ("g", Status::Failed), ("h", Status::Failed)]);
        let (mut show, dismiss) = alerts(&before, &now, &HashSet::new(), &["b".into()]);
        show.sort();
        assert_eq!((show, dismiss), (vec!["a".to_string(), "f".into(), "g".into(), "h".into()], vec![]));
    }

    #[test]
    fn dismisses_once_seen_or_out_of_the_status() {
        let now = statuses(&[("a", Status::Done), ("b", Status::Working), ("c", Status::NeedsYou)]);
        let shown: HashSet<String> = ["a", "b", "c", "gone"].map(String::from).into();
        let (show, mut dismiss) = alerts(&now, &now, &shown, &["a".into()]);
        dismiss.sort();
        assert_eq!((show, dismiss), (vec![], vec!["a".to_string(), "b".into(), "gone".into()]));
    }
}
