use crate::sessions::Session;
use crate::view::command_line;
use agents::Summary;
use std::cmp::Reverse;
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
    /// Its last agent exited; the card keeps that agent's title and a faded provider badge.
    Ended,
    /// No agent ever ran here, so the card reads what the terminal does.
    Shell(String),
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

/// A session's card. `terms[0]` is its top-level terminal; the rest are its tabs and splits.
pub fn card(terms: &[&Session], agents: &[Summary]) -> Card {
    let top = terms[0];
    let rank = |a: &&Summary| (Status::of(a).is_none(), a.status == "closed", Status::of(a), a.terminal_id != top.info.id, Reverse(a.updated_at));
    let lead = agents.iter().filter(|a| terms.iter().any(|t| t.info.id == a.terminal_id)).min_by_key(rank);
    let kind = match lead {
        None if top.info.last_provider.is_empty() => Kind::Shell(top.activity()),
        Some(a) if a.status != "closed" && a.attached => Kind::Agent,
        Some(a) if a.status != "closed" => Kind::NotAttached,
        _ => Kind::Ended,
    };
    let (provider, title) = match lead {
        Some(a) => (a.provider.clone(), a.title.clone()),
        None if top.info.last_provider.is_empty() => (top.info.cmd.clone(), command_line(&top.info)),
        None => (top.info.last_provider.clone(), top.info.last_title.clone()),
    };
    Card {
        id: top.info.id.clone(),
        provider,
        title: if title.is_empty() { "New session".into() } else { title },
        cwd: top.info.cwd.clone(),
        at: lead.map_or(0, |a| a.updated_at),
        status: match lead.and_then(Status::of) {
            Some(s) => s,
            None if kind == Kind::Ended && top.failed() && !top.exit_seen => Status::Failed,
            None => Status::Idle,
        },
        kind,
    }
}

pub const SECTIONS: [&str; 5] = ["Needs you", "Done", "Working", "Earlier today", "Earlier"];

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

pub fn roll_up_label((status, n): (Status, usize)) -> String {
    match status {
        Status::NeedsYou if n == 1 => "1 needs you".into(),
        Status::NeedsYou => format!("{n} need you"),
        Status::Failed | Status::Done => format!("{n} done"),
        _ => "Working".into(),
    }
}

/// Why Pocket can't see the status of a live agent that is not attached.
pub fn banner(a: &Summary) -> Option<&'static str> {
    if a.attached || a.status == "closed" {
        return None;
    }
    match a.provider.as_str() {
        "claude" => Some("Claude skips hooks in folders it doesn't trust. Trust this folder in Claude to see status."),
        "codex" => Some("This codex runs without the app-server, so Pocket can't see its status."),
        _ => None,
    }
}

/// The live agents in `panes`, sorted so an unchanged view compares equal.
pub fn view_set(panes: &[String], agents: &[Summary]) -> Vec<String> {
    let mut ids: Vec<String> = agents.iter().filter(|a| a.status != "closed" && panes.contains(&a.terminal_id)).map(|a| a.id.clone()).collect();
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
    use daemon::Info;

    fn agent(terminal: &str, status: &str) -> Summary {
        Summary { id: format!("agent-{terminal}"), terminal_id: terminal.into(), status: status.into(), attached: true, ..Default::default() }
    }

    fn term(id: &str) -> Session {
        let info = Info { id: id.into(), cmd: "/bin/zsh".into(), args: vec!["-l".into()], cwd: "/w".into(), ..Default::default() };
        Session { info, term: None, exit: None, closed: false, exit_seen: false }
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
    fn a_card_shows_its_most_urgent_agent_across_tabs() {
        let (top, tab) = (term("t1"), term("t2"));
        let agents = [Summary { title: "Top".into(), updated_at: 5, ..agent("t1", "idle") }, Summary { title: "Fix".into(), updated_at: 1, ..agent("t2", "needsYou") }];
        let c = card(&[&top, &tab], &agents);
        assert_eq!((c.id.as_str(), c.title.as_str(), c.status, c.at, c.kind), ("t1", "Fix", Status::NeedsYou, 1, Kind::Agent));
    }

    #[test]
    fn agents_in_other_sessions_do_not_count() {
        let c = card(&[&term("t1")], &[agent("x", "working")]);
        assert_eq!((c.title.as_str(), c.provider.as_str(), c.status, c.at), ("zsh", "/bin/zsh", Status::Idle, 0));
        assert_eq!(c.kind, Kind::Shell("at prompt".into()));
    }

    #[test]
    fn an_agentless_card_shows_its_exit_but_never_fails() {
        let c = card(&[&Session { exit: Some(1), ..term("t1") }], &[]);
        assert_eq!((c.kind, c.status), (Kind::Shell("exited 1".into()), Status::Idle));
    }

    #[test]
    fn a_card_keeps_its_last_agent_after_it_exits() {
        let mut top = term("t1");
        (top.info.last_provider, top.info.last_title) = ("claude".into(), "Fix CI".into());
        let c = card(&[&top], &[]);
        assert_eq!((c.kind, c.provider.as_str(), c.title.as_str(), c.status), (Kind::Ended, "claude", "Fix CI", Status::Idle));
        top.exit = Some(1);
        assert_eq!(card(&[&top], &[]).status, Status::Failed);
    }

    #[test]
    fn a_crashed_agents_card_clears_once_its_exit_is_seen() {
        let mut top = Session { exit: Some(1), exit_seen: true, ..term("t1") };
        top.info.last_provider = "claude".into();
        assert_eq!(card(&[&top], &[]).status, Status::Idle);
    }

    #[test]
    fn closed_and_detached_agents_rank_below_attached_ones() {
        let (top, tab) = (term("t1"), term("t2"));
        let agents = [
            Summary { title: "Gone".into(), updated_at: 9, ..agent("t1", "closed") },
            Summary { title: "Blind".into(), attached: false, ..agent("t1", "working") },
            Summary { title: "Tab".into(), ..agent("t2", "idle") },
        ];
        assert_eq!(card(&[&top, &tab], &agents).title, "Tab");
        let blind = card(&[&top], &agents[..2]);
        assert_eq!((blind.title.as_str(), blind.kind, blind.status), ("Blind", Kind::NotAttached, Status::Idle));
        let gone = card(&[&top], &agents[..1]);
        assert_eq!((gone.title.as_str(), gone.kind), ("Gone", Kind::Ended));
    }

    #[test]
    fn an_untitled_agent_reads_new_session() {
        assert_eq!(card(&[&term("t1")], &[agent("t1", "working")]).title, "New session");
    }

    #[test]
    fn sections_put_failed_with_done_and_split_idle_by_day() {
        let got: Vec<_> = [(Status::NeedsYou, false), (Status::Failed, false), (Status::Done, true), (Status::Working, true), (Status::Idle, true), (Status::Idle, false)].iter().map(|(s, t)| SECTIONS[section(*s, *t)]).collect();
        assert_eq!(got, vec!["Needs you", "Done", "Done", "Working", "Earlier today", "Earlier"]);
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
    fn labels_a_roll_up() {
        let got: Vec<String> = [(Status::NeedsYou, 1), (Status::NeedsYou, 2), (Status::Done, 1), (Status::Failed, 3), (Status::Working, 2)].into_iter().map(roll_up_label).collect();
        assert_eq!(got, vec!["1 needs you", "2 need you", "1 done", "3 done", "Working"]);
    }

    #[test]
    fn banners_a_live_agent_that_is_not_attached() {
        let blind = |provider: &str, status: &str| Summary { provider: provider.into(), attached: false, ..agent("t", status) };
        assert_eq!(banner(&blind("claude", "idle")), Some("Claude skips hooks in folders it doesn't trust. Trust this folder in Claude to see status."));
        assert_eq!(banner(&blind("codex", "working")), Some("This codex runs without the app-server, so Pocket can't see its status."));
        assert_eq!(banner(&blind("claude", "closed")), None);
        assert_eq!(banner(&Summary { provider: "claude".into(), ..agent("t", "idle") }), None);
    }

    #[test]
    fn views_the_live_agents_of_visible_panes() {
        let agents = [Summary { id: "b".into(), ..agent("t2", "idle") }, Summary { id: "a".into(), ..agent("t1", "working") }, Summary { id: "c".into(), ..agent("t1", "closed") }, agent("t3", "idle")];
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
