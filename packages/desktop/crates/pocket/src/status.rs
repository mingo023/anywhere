use agents::Summary;
use std::cmp::Reverse;
use std::collections::HashMap;
use store::prefs::sidebar::SessionSort;

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
    pub model: String,
    pub title: String,
    pub cwd: String,
    pub at: i64,
    pub created: i64,
    pub status: Status,
    pub kind: Kind,
    pub notice: Option<&'static str>,
    pub pinned: bool,
}

/// An agent's card, placed by the folder its terminal started in.
pub fn card(a: &Summary, cwd: &str) -> Card {
    let kind = if a.attached { Kind::Agent } else { Kind::NotAttached };
    Card {
        id: a.id.clone(),
        provider: a.provider.clone(),
        model: model(a),
        title: if a.title.is_empty() { "New session".into() } else { a.title.clone() },
        cwd: cwd.to_string(),
        at: a.updated_at,
        created: a.created_at,
        status: Status::of(a).unwrap_or(Status::Idle),
        kind,
        notice: notice(a),
        pinned: a.pinned,
    }
}

/// "Opus 5.5 · high": the model, else the provider, then the effort it was started with.
fn model(a: &Summary) -> String {
    let name = match a.model.as_deref() {
        Some(m) if !m.is_empty() => agents::model_label(a),
        _ => theme::provider_name(&a.provider).to_string(),
    };
    match a.effort.as_deref() {
        Some(e) if !e.is_empty() => format!("{name} · {e}"),
        _ => name,
    }
}

/// Orders a project's sessions as Sort sessions by says.
pub fn sort(cards: &mut [Card], by: SessionSort) {
    match by {
        SessionSort::Newest => cards.sort_by_key(|c| Reverse(c.created)),
        SessionSort::Status => cards.sort_by_key(|c| (c.status, Reverse(c.created))),
        SessionSort::Activity => cards.sort_by_key(|c| Reverse(c.at)),
        SessionSort::Name => cards.sort_by_cached_key(|c| (c.title.to_lowercase(), Reverse(c.created))),
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

/// How a pocketd restart brought the agent back, until its next turn.
pub fn notice(a: &Summary) -> Option<&'static str> {
    match a.restore.as_str() {
        "resumed" => Some("Resumed"),
        "interrupted" => Some("Interrupted by restart"),
        "access_lowered" => Some("Full access resumed as Ask"),
        "failed" => Some("Couldn't resume"),
        _ => None,
    }
}

/// A failed restore, else why Pocket can't see the status of a live agent that is not attached, else the restore notice.
pub fn banner(a: &Summary) -> Option<&'static str> {
    if a.restore == "failed" {
        return notice(a);
    }
    let hint = match a.provider.as_str() {
        _ if a.attached => None,
        "claude" => Some("Claude skips hooks in folders it doesn't trust. Trust this folder in Claude to see status."),
        "codex" => Some("This codex runs without the app-server, so Pocket can't see its status."),
        _ => None,
    };
    hint.or_else(|| notice(a))
}

/// The agents in `panes`, sorted so an unchanged view compares equal.
pub fn view_set(panes: &[String], agents: &[Summary]) -> Vec<String> {
    let mut ids: Vec<String> = agents.iter().filter(|a| panes.contains(&a.terminal_id)).map(|a| a.id.clone()).collect();
    ids.sort();
    ids
}

/// An agent's status and the id of its oldest pending permission ask.
#[derive(Clone, Debug, PartialEq)]
pub struct Alert {
    pub status: Status,
    pub ask: Option<String>,
}

impl Alert {
    /// Whether going from `before` to this is news: a new status, or a new ask. An ask clearing is not.
    fn news(&self, before: &Alert) -> bool {
        self.status != before.status || self.ask.is_some() && self.ask != before.ask
    }
}

/// The agents to notify about, once alerts went from `before` to `now`.
pub fn alerts(before: &HashMap<String, Alert>, now: &HashMap<String, Alert>, viewing: &[String]) -> Vec<String> {
    now.iter().filter(|(id, a)| a.status.alerting() && before.get(*id).is_some_and(|b| a.news(b)) && !viewing.contains(id)).map(|(id, _)| id.clone()).collect()
}

/// The sessions that want a look: Needs you, then Failed, then Done, each oldest transition first.
pub fn up_next(cards: &[Card]) -> Vec<&Card> {
    let mut out: Vec<&Card> = cards.iter().filter(|c| c.status.alerting()).collect();
    out.sort_by(|a, b| (a.status, a.at, &a.id).cmp(&(b.status, b.at, &b.id)));
    out
}

/// The top of Up next other than `current`.
pub fn next_up(cards: &[Card], current: Option<&str>) -> Option<String> {
    up_next(cards).into_iter().find(|c| Some(c.id.as_str()) != current).map(|c| c.id.clone())
}

/// The Needs you session after `current`, oldest transition first, wrapping.
pub fn next_needs_you(cards: &[Card], current: Option<&str>) -> Option<String> {
    let ids: Vec<String> = up_next(cards).into_iter().filter(|c| c.status == Status::NeedsYou).map(|c| c.id.clone()).collect();
    cycle(&ids, current, true)
}

/// The id after `current` in `ids`, or before it when `forward` is false, wrapping; with none selected, the first or the last.
pub fn cycle(ids: &[String], current: Option<&str>, forward: bool) -> Option<String> {
    let n = ids.len();
    let i = match (ids.iter().position(|id| Some(id.as_str()) == current), forward) {
        (Some(i), true) => (i + 1) % n,
        (Some(i), false) => (i + n - 1) % n,
        (None, true) => 0,
        (None, false) => n.checked_sub(1)?,
    };
    ids.get(i).cloned()
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
    fn sessions_sort_newest_first_by_status_by_activity_or_by_name() {
        let at = |title: &str, status: &str, created: i64, updated: i64| card(&Summary { title: title.into(), created_at: created, updated_at: updated, ..agent(title, status) }, "/w");
        let cards = vec![at("b", "idle", 1, 9), at("C", "needsYou", 2, 5), at("a", "working", 3, 1)];
        let order = |by| {
            let mut c = cards.clone();
            sort(&mut c, by);
            c.into_iter().map(|c| c.title).collect::<Vec<_>>()
        };
        assert_eq!(order(SessionSort::Newest), ["a", "C", "b"]);
        assert_eq!(order(SessionSort::Status), ["C", "a", "b"]);
        assert_eq!(order(SessionSort::Activity), ["b", "C", "a"]);
        assert_eq!(order(SessionSort::Name), ["a", "b", "C"]);
    }

    #[test]
    fn sorts_by_urgency() {
        let mut s = vec![Status::Idle, Status::Working, Status::Done, Status::Failed, Status::NeedsYou];
        s.sort();
        assert_eq!(s, vec![Status::NeedsYou, Status::Failed, Status::Done, Status::Working, Status::Idle]);
    }

    #[test]
    fn a_card_is_its_agent_in_its_terminals_folder() {
        let a = Summary { title: "Fix".into(), cwd: "/elsewhere".into(), created_at: 2, updated_at: 5, ..agent("t1", "working") };
        let c = card(&a, "/w");
        assert_eq!((c.id.as_str(), c.title.as_str(), c.cwd.as_str(), c.created, c.at, c.status, c.kind), ("agent-t1", "Fix", "/w", 2, 5, Status::Working, Kind::Agent));
    }

    #[test]
    fn a_card_names_its_model_and_effort_else_its_provider() {
        let named = |model: Option<&str>, effort: Option<&str>| card(&Summary { provider: "claude".into(), model: model.map(Into::into), effort: effort.map(Into::into), ..agent("t1", "idle") }, "/w").model;
        assert_eq!(named(Some("claude-opus-5-5"), Some("high")), "Opus 5.5 · high");
        assert_eq!(named(Some("claude-opus-5-5"), None), "Opus 5.5");
        assert_eq!(named(None, Some("high")), "Claude Code · high");
        assert_eq!(named(Some(""), Some("")), "Claude Code");
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

    fn restored(outcome: &str) -> Summary {
        Summary { provider: "claude".into(), restore: outcome.into(), ..agent("t", "idle") }
    }

    #[test]
    fn each_restore_outcome_has_its_notice() {
        let got: Vec<_> = ["resumed", "interrupted", "access_lowered", "failed"].iter().map(|o| notice(&restored(o))).collect();
        assert_eq!(got, vec![Some("Resumed"), Some("Interrupted by restart"), Some("Full access resumed as Ask"), Some("Couldn't resume")]);
    }

    #[test]
    fn an_unknown_outcome_shows_nothing() {
        assert_eq!(notice(&restored("")), None);
        assert_eq!(notice(&restored("rewound")), None);
        assert_eq!(banner(&restored("rewound")), None);
    }

    #[test]
    fn a_failed_restore_banner_beats_the_trust_hint() {
        assert_eq!(banner(&Summary { attached: false, ..restored("failed") }), Some("Couldn't resume"));
    }

    #[test]
    fn an_untrusted_resumed_claude_still_shows_the_trust_hint() {
        let hint = Some("Claude skips hooks in folders it doesn't trust. Trust this folder in Claude to see status.");
        assert_eq!(banner(&Summary { attached: false, ..restored("resumed") }), hint);
        assert_eq!(banner(&restored("resumed")), Some("Resumed"));
    }

    #[test]
    fn a_card_carries_the_notice() {
        assert_eq!(card(&restored("interrupted"), "/p").notice, Some("Interrupted by restart"));
        assert_eq!(card(&agent("t", "idle"), "/p").notice, None);
    }

    #[test]
    fn views_the_agents_of_visible_panes() {
        let agents = [Summary { id: "b".into(), ..agent("t2", "idle") }, Summary { id: "a".into(), ..agent("t1", "working") }, agent("t3", "idle")];
        assert_eq!(view_set(&["t1".into(), "t2".into()], &agents), vec!["a", "b"]);
    }

    fn statuses(list: &[(&str, Status)]) -> HashMap<String, Alert> {
        list.iter().map(|(id, s)| (id.to_string(), Alert { status: *s, ask: None })).collect()
    }

    #[test]
    fn a_new_ask_re_alerts_and_a_cleared_one_does_not() {
        let asking = |ask: Option<&str>| Alert { status: Status::NeedsYou, ask: ask.map(String::from) };
        let before = HashMap::from([("a".to_string(), asking(Some("q1"))), ("b".to_string(), asking(Some("q1"))), ("c".to_string(), asking(None))]);
        let now = HashMap::from([("a".to_string(), asking(Some("q2"))), ("b".to_string(), asking(None)), ("c".to_string(), asking(Some("q3")))]);
        let mut show = alerts(&before, &now, &[]);
        show.sort();
        assert_eq!(show, vec!["a", "c"]);
    }

    #[test]
    fn alerts_when_an_unseen_agent_enters_needs_you_or_done() {
        let before = statuses(&[("a", Status::Working), ("b", Status::Working), ("c", Status::Idle), ("d", Status::Done), ("f", Status::Working), ("g", Status::Working), ("h", Status::Done)]);
        let now = statuses(&[("a", Status::Done), ("b", Status::Done), ("c", Status::Working), ("d", Status::Done), ("e", Status::NeedsYou), ("f", Status::NeedsYou), ("g", Status::Failed), ("h", Status::Failed)]);
        let mut show = alerts(&before, &now, &["b".into()]);
        show.sort();
        assert_eq!(show, vec!["a", "f", "g", "h"]);
    }

    fn at(id: &str, status: &str, at: i64) -> Card {
        card(&Summary { id: id.into(), updated_at: at, ..agent(id, status) }, "/w")
    }

    #[test]
    fn up_next_ranks_needs_you_then_failed_then_done_oldest_first() {
        let failed = card(&Summary { id: "failed".into(), failed: true, updated_at: 9, ..agent("f", "done") }, "/w");
        let cards = [at("done-new", "done", 8), at("ask-new", "needsYou", 7), failed, at("busy", "working", 1), at("done-old", "done", 2), at("ask-old", "needsYou", 3), at("idle", "idle", 0)];
        let ids: Vec<&str> = up_next(&cards).into_iter().map(|c| c.id.as_str()).collect();
        assert_eq!(ids, vec!["ask-old", "ask-new", "failed", "done-old", "done-new"]);
    }

    #[test]
    fn go_to_up_next_visits_three_done_sessions_in_turn() {
        let mut cards = vec![at("d1", "done", 1), at("d2", "done", 2), at("d3", "done", 3)];
        let mut current = None;
        let mut visited = Vec::new();
        while let Some(id) = next_up(&cards, current.as_deref()) {
            cards.iter_mut().filter(|c| c.id == id).for_each(|c| c.status = Status::Idle);
            visited.push(id.clone());
            current = Some(id);
        }
        assert_eq!(visited, vec!["d1", "d2", "d3"]);
    }

    #[test]
    fn next_needs_you_wraps_oldest_first() {
        let cards = [at("new", "needsYou", 9), at("done", "done", 1), at("old", "needsYou", 2)];
        assert_eq!(next_needs_you(&cards, None).as_deref(), Some("old"));
        assert_eq!(next_needs_you(&cards, Some("old")).as_deref(), Some("new"));
        assert_eq!(next_needs_you(&cards, Some("new")).as_deref(), Some("old"));
        assert_eq!(next_needs_you(&cards, Some("done")).as_deref(), Some("old"));
    }

    #[test]
    fn cycle_starts_at_either_end_with_none_selected() {
        let ids: Vec<String> = ["a", "b", "c"].map(String::from).to_vec();
        assert_eq!(cycle(&ids, None, true).as_deref(), Some("a"));
        assert_eq!(cycle(&ids, None, false).as_deref(), Some("c"));
        assert_eq!(cycle(&ids, Some("c"), true).as_deref(), Some("a"));
        assert_eq!(cycle(&ids, Some("a"), false).as_deref(), Some("c"));
        assert_eq!(cycle(&[], None, true), None);
    }
}
