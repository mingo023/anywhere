use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::inbox;
use crate::status::{self, Alert, Status};
use crate::util::basename;
use agents::{Agents, Decision, Event, Permission, Summary};
use gpui_kit::*;
use std::collections::HashMap;
use workspace::Tab;

pub const ALLOW: &str = "allow";
pub const DENY: &str = "deny";
const LINE_MAX: usize = 240;

pub struct Alerts {
    pub(crate) viewing: Option<Vec<String>>,
    pub(crate) statuses: HashMap<String, Alert>,
    /// The ask each shown banner offers to answer, by agent.
    asks: HashMap<String, String>,
}

/// "{place}\n{line}", the line cut to `LINE_MAX` characters.
pub fn body(place: &str, line: &str) -> String {
    let mut cut: String = line.chars().take(LINE_MAX).collect();
    if cut.len() < line.len() {
        cut.push('…');
    }
    if place.is_empty() { cut } else { format!("{place}\n{cut}") }
}

/// The banner's second line: the ask, the reply's first paragraph, or the status.
fn line(agents: &Agents, a: &Summary, status: Status) -> String {
    let text = match status {
        Status::NeedsYou => agents.pending.iter().find(|p| p.agent_id == a.id).map(Permission::ask),
        Status::Done => agents.last_text(&a.id).and_then(|t| t.split("\n\n").map(|p| p.split_whitespace().collect::<Vec<_>>().join(" ")).find(|p| !p.is_empty())),
        _ => None,
    };
    text.unwrap_or_else(|| status.label().to_string())
}

impl Alerts {
    pub fn new() -> Self {
        Self { viewing: None, statuses: HashMap::new(), asks: HashMap::new() }
    }

    /// Takes the agents' new statuses and asks; returns the notifications to show. A `quiet` sync shows none.
    /// Shown ones stay until the user clears them or the agent's next one replaces them, as in Superset.
    pub fn sync(&mut self, agents: &Agents, place: impl Fn(&Summary) -> String, quiet: bool) -> Vec<Notice> {
        let ask = |id: &str| agents.pending.iter().find(|p| p.agent_id == id).map(|p| p.request_id.clone());
        let now: HashMap<String, Alert> = agents.list.iter().filter_map(|a| Some((a.id.clone(), Alert { status: Status::of(a)?, ask: ask(&a.id) }))).collect();
        let show = status::alerts(&self.statuses, &now, self.viewing.as_deref().unwrap_or_default());
        let mut notices = Vec::new();
        let fresh: Vec<String> = show.into_iter().filter(|id| now[id].ask.is_none() || self.asks.get(id) != now[id].ask.as_ref()).collect();
        for id in fresh.into_iter().filter(|_| !quiet) {
            let Some(a) = agents.get(&id) else { continue };
            let title = if a.title.is_empty() { theme::provider_name(&a.provider).to_string() } else { a.title.clone() };
            let Alert { status, ask } = now[&id].clone();
            match &ask {
                Some(q) => self.asks.insert(id.clone(), q.clone()),
                None => self.asks.remove(&id),
            };
            notices.push(Notice { body: body(&place(a), &line(agents, a, status)), id, title, ask });
        }
        self.statuses = now;
        notices
    }

    /// The ask `agent`'s banner showed, while it is still pending; never a newer one.
    pub fn answerable(&self, agent: &str, pending: &[Permission]) -> Option<&str> {
        self.asks.get(agent).map(String::as_str).filter(|q| pending.iter().any(|p| p.request_id == *q))
    }

    /// Views the live agents in `panes`; returns them when that changed what is viewed.
    pub fn view(&mut self, panes: &[String], agents: &[Summary]) -> Option<Vec<String>> {
        let ids = status::view_set(panes, agents);
        if self.viewing.as_ref() == Some(&ids) {
            return None;
        }
        self.viewing = Some(ids.clone());
        Some(ids)
    }
}

#[derive(Debug, PartialEq)]
pub struct Notice {
    pub id: String,
    pub title: String,
    pub body: String,
    pub ask: Option<String>,
}

impl Notice {
    /// Allow and Deny and stop, on a permission ask only.
    pub fn actions(&self) -> Vec<SystemNotificationAction> {
        let action = |id: &'static str, label: &'static str| SystemNotificationAction { id: id.into(), label: label.into() };
        if self.ask.is_some() { vec![action(ALLOW, "Allow"), action(DENY, "Deny and stop")] } else { Vec::new() }
    }
}

impl Desktop {
    pub(crate) fn on_agents(&mut self, ev: Event, window: &mut Window, cx: &mut Context<Self>) {
        if let Event::PairCode { .. } | Event::Paired(_) | Event::PairFailed(_) = ev {
            return self.on_pair(ev, cx);
        }
        if let Event::Creating { .. } | Event::Progress { .. } | Event::Created { .. } | Event::CreateFailed { .. } = ev {
            return self.on_launch(ev, window, cx);
        }
        if let Event::ConfigFailed(message) = ev {
            self.error = Some(message);
            return cx.notify();
        }
        let connected = matches!(ev, Event::Connected { .. });
        if connected {
            self.pair.lost();
            self.outbox.providers();
        }
        if let (Event::Connected { .. }, Some(ids)) = (&ev, &self.alerts.viewing) {
            self.outbox.view(ids);
        }
        self.agents.apply(ev);
        let agents = &self.agents;
        self.terminals.setups.retain(|term, _| !agents.list.iter().any(|a| &a.terminal_id == term));
        if self.screen == Screen::Inbox {
            (self.inbox.selected, self.terminal.focused) = inbox::reselect(&inbox::notes(&self.agents), self.terminal.focused.as_deref(), self.inbox.selected);
        }
        self.sync_alerts(cx);
        if !self.capturing {
            self.badge.show(&self.agents.list);
            self.chime(connected, cx);
        }
        cx.notify();
    }

    /// "{project} · {worktree}" for the folder the agent's terminal started in; empty outside every project.
    fn place(&self, a: &Summary) -> String {
        let projects = self.projects();
        let Some(cwd) = self.terminals.sessions.get(&a.terminal_id).map(|s| s.info.cwd.as_str()) else { return String::new() };
        match (self.project_of(cwd, &projects), self.tree_of(cwd)) {
            (Some(project), Some(tree)) => format!("{} · {}", self.repo_name(project), basename(&tree)),
            _ => String::new(),
        }
    }

    fn sync_alerts(&mut self, cx: &mut App) {
        let mut alerts = std::mem::replace(&mut self.alerts, Alerts::new());
        let show = alerts.sync(&self.agents, |a| self.place(a), self.capturing);
        self.alerts = alerts;
        for n in show {
            let actions = n.actions();
            cx.show_system_notification(SystemNotification { tag: n.id.into(), title: n.title.into(), body: n.body.into(), actions });
        }
    }

    /// Answers the ask `agent`'s banner showed, if it is still pending, and takes the banner down.
    pub(crate) fn answer_banner(&mut self, agent: &str, action: &str, cx: &mut App) {
        let decision = match action {
            ALLOW => Decision::Allow,
            DENY => Decision::Deny,
            _ => return,
        };
        if let Some(ask) = self.alerts.answerable(agent, &self.agents.pending) {
            self.outbox.resolve(ask, decision);
        }
        cx.dismiss_system_notification(agent);
    }

    /// The terminals on screen: those of the tabs the drawn panes show.
    fn visible_panes(&mut self) -> Vec<String> {
        let Some(tree) = self.cwd().filter(|_| self.screen == Screen::Sessions) else { return Vec::new() };
        self.workspace(&tree).shown().into_iter().filter_map(|(_, t)| if let Tab::Term(id) = t { Some(id.clone()) } else { None }).collect()
    }

    pub(crate) fn sync_view(&mut self, window: &Window, cx: &mut App) {
        let panes = if window.is_window_active() { self.visible_panes() } else { Vec::new() };
        if let Some(ids) = self.alerts.view(&panes, &self.agents.list) {
            self.outbox.view(&ids);
            self.sync_alerts(cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ALLOW, Alerts, DENY, Notice, body};
    use agents::{Agents, Permission, Summary};

    fn agent(id: &str, status: &str) -> Summary {
        Summary { id: id.into(), terminal_id: format!("t-{id}"), title: id.to_uppercase(), status: status.into(), attached: true, ..Default::default() }
    }

    fn sync(alerts: &mut Alerts, list: &[Summary], quiet: bool) -> Vec<Notice> {
        asking(alerts, list, &[], quiet)
    }

    fn asking(alerts: &mut Alerts, list: &[Summary], asks: &[(&str, &str)], quiet: bool) -> Vec<Notice> {
        let pending = asks.iter().map(|(agent, q)| Permission { request_id: q.to_string(), agent_id: agent.to_string(), tool_name: "Bash".into(), ..Default::default() }).collect();
        alerts.sync(&Agents { list: list.to_vec(), pending, ..Default::default() }, |a| format!("app · {}", a.id), quiet)
    }

    fn notice(id: &str, line: &str, ask: Option<&str>) -> Notice {
        Notice { id: id.into(), title: id.to_uppercase(), body: format!("app · {id}\n{line}"), ask: ask.map(String::from) }
    }

    #[test]
    fn an_agent_that_starts_waiting_notifies_once() {
        let mut alerts = Alerts::new();
        sync(&mut alerts, &[agent("a", "working")], false);
        let first = sync(&mut alerts, &[agent("a", "needsYou")], false);
        let again = sync(&mut alerts, &[agent("a", "needsYou")], false);
        assert_eq!((first, again), (vec![notice("a", "Needs you", None)], vec![]));
    }

    #[test]
    fn viewing_an_agents_pane_marks_it_seen_instead_of_notifying() {
        let mut alerts = Alerts::new();
        sync(&mut alerts, &[agent("a", "working")], false);
        let list = [agent("a", "done")];
        let viewed = alerts.view(&["t-a".into()], &list);
        let show = sync(&mut alerts, &list, false);
        assert_eq!((viewed, show), (Some(vec!["a".to_string()]), vec![]));
    }

    #[test]
    fn an_agent_notifies_once_its_pane_leaves_the_view() {
        let mut alerts = Alerts::new();
        alerts.view(&["t-a".into()], &[agent("a", "working")]);
        sync(&mut alerts, &[agent("a", "working")], false);
        let viewed = alerts.view(&[], &[agent("a", "working")]);
        let show = sync(&mut alerts, &[agent("a", "done")], false);
        assert_eq!((viewed, show), (Some(vec![]), vec![notice("a", "Done", None)]));
    }

    #[test]
    fn an_unchanged_view_is_not_reported_again() {
        let mut alerts = Alerts::new();
        let panes = ["t-a".to_string(), "t-gone".into()];
        let first = alerts.view(&panes, &[agent("a", "working")]);
        let again = alerts.view(&panes, &[agent("a", "done")]);
        assert_eq!((first, again), (Some(vec!["a".to_string()]), None));
    }

    #[test]
    fn a_quiet_sync_shows_nothing_and_does_not_show_it_later() {
        let mut alerts = Alerts::new();
        sync(&mut alerts, &[agent("a", "working")], true);
        let quiet = sync(&mut alerts, &[agent("a", "done")], true);
        let later = sync(&mut alerts, &[agent("a", "done")], false);
        assert_eq!((quiet, later), (vec![], vec![]));
    }

    #[test]
    fn only_a_permission_ask_offers_allow_and_deny() {
        let mut alerts = Alerts::new();
        sync(&mut alerts, &[agent("a", "working"), agent("b", "working")], false);
        let shown = asking(&mut alerts, &[agent("a", "needsYou"), agent("b", "done")], &[("a", "q1")], false);
        let actions: Vec<Vec<(String, String)>> = shown.iter().map(|n| n.actions().into_iter().map(|x| (x.id.to_string(), x.label.to_string())).collect()).collect();
        let allow_deny = vec![(ALLOW.to_string(), "Allow".to_string()), (DENY.to_string(), "Deny and stop".to_string())];
        let mut got: Vec<_> = shown.iter().map(|n| n.id.as_str()).zip(actions).collect();
        got.sort();
        assert_eq!(got, vec![("a", allow_deny), ("b", vec![])]);
    }

    #[test]
    fn body_is_place_then_the_ask_cut_to_240() {
        let long = "é".repeat(300);
        assert_eq!(body("app · main", "Wants to run ls"), "app · main\nWants to run ls");
        assert_eq!(body("app · main", &long), format!("app · main\n{}…", "é".repeat(240)));
        assert_eq!(body("", "Done"), "Done");
    }

    #[test]
    fn an_ask_replayed_after_a_reconnect_does_not_notify_again() {
        let mut alerts = Alerts::new();
        sync(&mut alerts, &[agent("a", "working")], false);
        let first = asking(&mut alerts, &[agent("a", "needsYou")], &[("a", "q1")], false);
        asking(&mut alerts, &[agent("a", "needsYou")], &[], false);
        let replayed = asking(&mut alerts, &[agent("a", "needsYou")], &[("a", "q1")], false);
        let fresh = asking(&mut alerts, &[agent("a", "needsYou")], &[("a", "q2")], false);
        assert_eq!((first.len(), replayed, fresh.len()), (1, vec![], 1));
    }

    #[test]
    fn a_banner_never_answers_a_newer_ask() {
        let mut alerts = Alerts::new();
        sync(&mut alerts, &[agent("a", "working")], false);
        let shown = asking(&mut alerts, &[agent("a", "needsYou")], &[("a", "q1")], false);
        assert_eq!(shown, vec![notice("a", "Wants to use Bash", Some("q1"))]);
        let q = |id: &str| Permission { request_id: id.into(), agent_id: "a".into(), ..Default::default() };
        assert_eq!(alerts.answerable("a", &[q("q1")]), Some("q1"));
        assert_eq!(alerts.answerable("a", &[q("q2")]), None);
        assert_eq!(alerts.answerable("b", &[q("q1")]), None);
    }
}
