use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::inbox;
use crate::status::{self, Status};
use agents::{Event, Summary};
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use workspace::Tab;

pub struct Alerts {
    pub(crate) viewing: Option<Vec<String>>,
    pub(crate) statuses: HashMap<String, Status>,
    pub(crate) alerted: HashSet<String>,
}

impl Alerts {
    pub fn new() -> Self {
        Self { viewing: None, statuses: HashMap::new(), alerted: HashSet::new() }
    }

    pub fn listed(&mut self, list: &[Summary]) {
        // Notifications outlive the app, so any listed agent may still have one from before a restart.
        self.alerted.extend(list.iter().map(|a| a.id.clone()));
    }

    /// Takes the agents' new statuses; returns the notifications to show and the ones to dismiss. A `quiet` sync shows none.
    pub fn sync(&mut self, agents: &[Summary], quiet: bool) -> (Vec<Notice>, Vec<String>) {
        let now: HashMap<String, Status> = agents.iter().filter_map(|a| Some((a.id.clone(), Status::of(a)?))).collect();
        let (show, dismiss) = status::alerts(&self.statuses, &now, &self.alerted, self.viewing.as_deref().unwrap_or_default());
        for id in &dismiss {
            self.alerted.remove(id);
        }
        let mut notices = Vec::new();
        for id in show.into_iter().filter(|_| !quiet) {
            let Some(a) = agents.iter().find(|a| a.id == id) else { continue };
            let title = if a.title.is_empty() { theme::provider_name(&a.provider).to_string() } else { a.title.clone() };
            self.alerted.insert(id.clone());
            notices.push(Notice { body: now[&id].label(), id, title });
        }
        self.statuses = now;
        (notices, dismiss)
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
    pub body: &'static str,
}

impl Desktop {
    pub(crate) fn on_agents(&mut self, ev: Event, cx: &mut Context<Self>) {
        if let (Event::Connected, Some(ids)) = (&ev, &self.alerts.viewing) {
            self.outbox.view(ids);
        }
        if let Event::Agents(list) = &ev {
            self.alerts.listed(list);
        }
        self.agents.apply(ev);
        let agents = &self.agents;
        self.terminals.setups.retain(|term, _| !agents.list.iter().any(|a| &a.terminal_id == term));
        if self.screen == Screen::Inbox {
            (self.inbox.selected, self.terminal.focused) = inbox::reselect(&inbox::notes(&self.agents), self.terminal.focused.as_deref(), self.inbox.selected);
        }
        self.sync_alerts(cx);
        cx.notify();
    }

    fn sync_alerts(&mut self, cx: &mut App) {
        let (show, dismiss) = self.alerts.sync(&self.agents.list, self.capturing);
        for id in dismiss {
            cx.dismiss_system_notification(&id);
        }
        for n in show {
            cx.show_system_notification(SystemNotification { tag: n.id.into(), title: n.title.into(), body: n.body.into(), actions: Vec::new() });
        }
    }

    /// The terminals on screen: the worktree's active tab's.
    fn visible_panes(&mut self) -> Vec<String> {
        let Some(tree) = self.cwd().filter(|_| self.screen == Screen::Sessions) else { return Vec::new() };
        match self.workspace(&tree).active() {
            Some(Tab::Term(rows)) => rows.concat(),
            _ => Vec::new(),
        }
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
    use super::{Alerts, Notice};
    use agents::Summary;

    fn agent(id: &str, status: &str) -> Summary {
        Summary { id: id.into(), terminal_id: format!("t-{id}"), title: id.to_uppercase(), status: status.into(), attached: true, ..Default::default() }
    }

    #[test]
    fn an_agent_that_starts_waiting_notifies_once() {
        let mut alerts = Alerts::new();
        alerts.sync(&[agent("a", "working")], false);
        let first = alerts.sync(&[agent("a", "needsYou")], false);
        let again = alerts.sync(&[agent("a", "needsYou")], false);
        assert_eq!((first.0, again.0), (vec![Notice { id: "a".into(), title: "A".into(), body: "Needs you" }], vec![]));
    }

    #[test]
    fn viewing_an_agents_pane_marks_it_seen_instead_of_notifying() {
        let mut alerts = Alerts::new();
        alerts.sync(&[agent("a", "working"), agent("b", "working")], false);
        alerts.sync(&[agent("a", "needsYou"), agent("b", "working")], false);
        let list = [agent("a", "needsYou"), agent("b", "done")];
        let viewed = alerts.view(&["t-a".into(), "t-b".into()], &list);
        let (show, dismiss) = alerts.sync(&list, false);
        assert_eq!((viewed, show, dismiss), (Some(vec!["a".to_string(), "b".into()]), vec![], vec!["a".to_string()]));
    }

    #[test]
    fn an_agent_notifies_once_its_pane_leaves_the_view() {
        let mut alerts = Alerts::new();
        alerts.view(&["t-a".into()], &[agent("a", "working")]);
        alerts.sync(&[agent("a", "working")], false);
        let viewed = alerts.view(&[], &[agent("a", "working")]);
        let (show, _) = alerts.sync(&[agent("a", "done")], false);
        assert_eq!((viewed, show), (Some(vec![]), vec![Notice { id: "a".into(), title: "A".into(), body: "Done" }]));
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
    fn a_listed_agent_loses_a_notification_left_from_before_a_restart() {
        let mut alerts = Alerts::new();
        let list = [agent("a", "working"), agent("b", "done")];
        alerts.listed(&list);
        let (show, mut dismiss) = alerts.sync(&list, false);
        dismiss.sort();
        assert_eq!((show, dismiss), (vec![], vec!["a".to_string()]));
    }

    #[test]
    fn a_quiet_sync_shows_nothing_and_does_not_show_it_later() {
        let mut alerts = Alerts::new();
        alerts.sync(&[agent("a", "working")], true);
        let quiet = alerts.sync(&[agent("a", "done")], true);
        let later = alerts.sync(&[agent("a", "done")], false);
        assert_eq!((quiet, later), ((vec![], vec![]), (vec![], vec![])));
    }
}
