use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::inbox;
use crate::status::{self, Alert, Status};
use crate::terminals::link;
use agents::{Agents, Decision, Event, Host, Permission, Summary};
use gpui_kit::*;
use std::collections::HashMap;
use std::time::Instant;
use store::Notifications;
use workspace::Tab;

pub const ALLOW: &str = "allow";
pub const DENY: &str = "deny";
const LINE_MAX: usize = 240;

/// Whether `n` lets a banner show for an agent entering `status`.
pub fn wants_banner(n: Notifications, status: Status) -> bool {
    n.banners
        && match status {
            Status::NeedsYou => n.needs_you,
            Status::Failed => n.failed,
            Status::Done => n.done,
            Status::Working | Status::Idle => false,
        }
}

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

fn who(a: &Summary) -> String {
    if a.title.is_empty() { theme::provider_name(&a.provider).to_string() } else { a.title.clone() }
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

    /// Takes the agents' new statuses and asks; returns the notifications to show, for the statuses `wanted` lets through.
    /// Sessions on screen get none unless `on_screen`.
    /// Shown ones stay until the user clears them or the agent's next one replaces them, as in Superset.
    pub fn sync(&mut self, agents: &Agents, place: impl Fn(&Summary) -> String, wanted: impl Fn(Status) -> bool, on_screen: bool) -> Vec<Notice> {
        let ask = |id: &str| agents.pending.iter().find(|p| p.agent_id == id).map(|p| p.request_id.clone());
        let now: HashMap<String, Alert> = agents.list.iter().filter_map(|a| Some((a.id.clone(), Alert { status: Status::of(a)?, ask: ask(&a.id) }))).collect();
        let viewing = if on_screen { &[] } else { self.viewing.as_deref().unwrap_or_default() };
        let show = status::alerts(&self.statuses, &now, viewing);
        let mut notices = Vec::new();
        let fresh: Vec<String> = show.into_iter().filter(|id| now[id].ask.is_none() || self.asks.get(id) != now[id].ask.as_ref()).collect();
        for id in fresh.into_iter().filter(|id| wanted(now[id].status)) {
            let Some(a) = agents.get(&id) else { continue };
            let title = who(a);
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
        self.on_agents_from(None, ev, window, cx);
    }

    /// An agent event from this Mac's pocketd, or from `host`'s already tagged; a host's hello says nothing of this Mac's pocketd.
    pub(crate) fn on_agents_from(&mut self, host: Option<&str>, ev: Event, window: &mut Window, cx: &mut Context<Self>) {
        if let Event::PairCode { .. } | Event::Paired(_) | Event::PairFailed(_) = ev {
            return self.on_pair(ev, cx);
        }
        if let Event::Creating { .. } | Event::Progress { .. } | Event::Created { .. } | Event::CreateFailed { .. } = ev {
            return self.on_launch(ev, window, cx);
        }
        if let Event::ConfigFailed(message) | Event::SettingsFailed(message) = ev {
            self.error = Some(message);
            return cx.notify();
        }
        if let Event::AutomationError(message) = ev {
            self.error = Some(message);
            return cx.notify();
        }
        if let Event::PromptFailed { agent, request } = &ev {
            // A chat not yet opened has no send to fail.
            if let Some(v) = self.chats.views.get_mut(agent) {
                v.session.outgoing.fail(request);
            }
            return cx.notify();
        }
        if let Event::InterruptFailed { agent, reason } = &ev {
            if let Some(v) = self.chats.views.get_mut(agent) {
                v.session.outgoing.stop_refused();
            }
            self.error = Some(stop_failed(self.agents.get(agent), reason));
            return cx.notify();
        }
        if let Event::Commands { agent, commands } = ev {
            self.chats.composer.suggest.listed(agent, commands);
            return self.suggest(crate::chat::composer::suggest::Prompt::Chat, cx);
        }
        if let Event::Skills { provider, account, project, commands } = ev {
            self.new_form.composer.suggest.listed(crate::chat::composer::suggest::launch_key(&provider, &account, &project), commands);
            return self.suggest(crate::chat::composer::suggest::Prompt::Launch, cx);
        }
        if let Event::Mcp { agent, servers, auth_url, acted } = ev {
            if !auth_url.is_empty() {
                cx.open_url(&auth_url);
            }
            self.chats.composer.suggest.mcp_listed(agent.clone(), servers, &auth_url, acted);
            self.recheck_servers(agent, cx);
            return self.suggest(crate::chat::composer::suggest::Prompt::Chat, cx);
        }
        if let Event::McpActFailed { agent, reason } = ev {
            self.chats.composer.suggest.mcp_act_failed(&agent);
            // The servers may have moved on from what the rows show, as when a sign-in ended.
            self.outbox.mcp(&agent);
            self.error = Some(reason);
            return cx.notify();
        }
        if let Event::Btw { request, answer } = ev {
            return self.chat_btw_answered(&request, answer, cx);
        }
        if let Event::NamingFailed(_) = ev {
            self.error = Some("Couldn't name the session from its prompt".into());
            return cx.notify();
        }
        if let Event::Host(host) = &ev
            && let Some(message) = upgrade_toast(self.agents.host.as_ref(), host)
        {
            self.error = Some(message);
        }
        let connected = matches!(ev, Event::Connected { .. });
        let automations = matches!(ev, Event::Automations { .. });
        let locals = matches!(ev, Event::Locals(_));
        if let (Event::Connected { version, .. }, None) = (&ev, host) {
            self.terminals.link.connected(channel::version(), version, channel::is_release(), Instant::now());
            if self.terminals.link.stale().is_some() {
                cx.spawn(async |this, cx| {
                    cx.background_executor().timer(link::STALE_AFTER).await;
                    this.update(cx, |_, cx| cx.notify())
                })
                .detach();
            }
            cx.notify();
            self.pair.lost();
            self.outbox.local.providers();
        }
        if let (Event::Connected { .. }, Some(ids)) = (&ev, &self.alerts.viewing) {
            self.outbox.view(ids);
        }
        let exited = exited_terminal(&self.agents.list, &ev).filter(|t| self.store.agents.close_on_exit && !self.capturing && !self.terminals.killed(t));
        let listed = matches!(ev, Event::Agents(_) | Event::Agent(_)).then(|| self.agents.list.clone());
        self.agents.apply_from(host, ev);
        for (agent, who) in listed.map(|before| gone(&before, &self.agents.list)).unwrap_or_default() {
            self.chat_closed(&agent, &who, window, cx);
        }
        self.hand_off_creates();
        self.focus_arrived_chat(window, cx);
        self.refresh_chats();
        if let Some(term) = exited.filter(|t| self.workspaces.values().any(|w| w.holds_term(t))) {
            self.close_pane(&term, cx);
        }
        if connected && host.is_none() {
            self.new_form.offer_locals(self.agents.locals_offered());
            self.leave_unoffered_automations();
        }
        if locals {
            self.leave_gone_local(cx);
        }
        let agents = &self.agents;
        self.terminals.setups.retain(|term, _| !agents.list.iter().any(|a| &a.terminal_id == term));
        if self.screen == Screen::Inbox {
            (self.inbox.selected, self.terminal.focused) = inbox::reselect(&self.shown_notes(), self.terminal.focused.as_deref(), self.inbox.selected);
        }
        if automations {
            self.automations_changed();
        }
        self.sync_alerts(cx);
        if !self.capturing {
            self.badge.show(&self.agents.list, self.store.notifications.badge);
            self.chime(connected, cx);
        }
        cx.notify();
    }

    /// "{project} · {worktree}" for the Local or worktree the agent's terminal belongs to; empty outside every project.
    pub(crate) fn alert_place(&self, a: &Summary) -> String {
        let projects = self.projects();
        let Some(info) = self.terminals.sessions.get(&a.terminal_id).map(|s| &s.info) else { return String::new() };
        match (self.project_of(&info.cwd, &projects), self.place_of(info)) {
            (Some(project), Some(tree)) => format!("{} · {}", self.repo_name(project), self.place_name(&tree)),
            _ => String::new(),
        }
    }

    fn sync_alerts(&mut self, cx: &mut App) {
        let mut alerts = std::mem::replace(&mut self.alerts, Alerts::new());
        let n = self.store.notifications;
        let show = alerts.sync(&self.agents, |a| self.alert_place(a), |s| !self.capturing && wants_banner(n, s), n.on_screen);
        self.alerts = alerts;
        self.sync_island(&show);
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
        let Some(tree) = self.place().filter(|_| self.screen == Screen::Sessions) else { return Vec::new() };
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

/// The terminal whose agent `ev` says exited, if this window knew it; not one whose restore failed, so its error stays readable.
pub(crate) fn exited_terminal(list: &[Summary], ev: &Event) -> Option<String> {
    let Event::Agent(a) = ev else { return None };
    let known = list.iter().find(|x| x.id == a.id)?;
    (a.status == "closed" && known.restore != "failed").then(|| known.terminal_id.clone())
}

/// The agents `before` held that `after` doesn't, with their names: closed, or left out of a host's full list after a reconnect.
fn gone(before: &[Summary], after: &[Summary]) -> Vec<(String, String)> {
    before.iter().filter(|a| !after.iter().any(|x| x.id == a.id)).map(|a| (a.id.clone(), who(a))).collect()
}

/// The toast for a stop pocketd refused or never got.
fn stop_failed(a: Option<&Summary>, reason: &str) -> String {
    let who = a.map_or_else(|| "the session".to_string(), who);
    format!("Couldn't stop {who}: {reason}")
}

/// The toast for a failed pocketd upgrade, once per version it failed on.
pub(crate) fn upgrade_toast(prev: Option<&Host>, next: &Host) -> Option<String> {
    let failed = &next.upgrade_failed;
    (!failed.is_empty() && prev.is_none_or(|p| &p.upgrade_failed != failed))
        .then(|| format!("Couldn't update pocketd to {failed}. The current one keeps going."))
}

#[cfg(test)]
mod tests {
    use super::{ALLOW, Alerts, DENY, Notice, body, exited_terminal, gone, stop_failed, upgrade_toast, wants_banner};
    use crate::status::Status;
    use store::Notifications;
    use agents::{Agents, Event, Host, Permission, Summary};

    fn agent(id: &str, status: &str) -> Summary {
        Summary { id: id.into(), terminal_id: format!("t-{id}"), title: id.to_uppercase(), status: status.into(), attached: true, ..Default::default() }
    }

    #[test]
    fn an_agent_a_full_list_leaves_out_is_gone_like_a_closed_one() {
        let before = [agent("a", "working"), agent("b", "idle"), Summary { title: String::new(), provider: "codex".into(), ..agent("c", "idle") }];
        let mut after = Agents { list: before.to_vec(), ..Default::default() };
        after.apply_from(None, Event::Agents(vec![agent("b", "idle")]));
        assert_eq!(gone(&before, &after.list), [("a".to_string(), "A".to_string()), ("c".to_string(), "Codex".to_string())]);
        after.apply_from(None, Event::Agent(agent("b", "closed")));
        assert_eq!(gone(&before[1..2], &after.list), [("b".to_string(), "B".to_string())]);
    }

    #[test]
    fn an_agent_that_exits_names_its_terminal_unless_its_restore_failed() {
        let failed = Summary { restore: "failed".into(), ..agent("b", "idle") };
        let list = [agent("a", "working"), failed];
        assert_eq!(exited_terminal(&list, &Event::Agent(agent("a", "closed"))).as_deref(), Some("t-a"));
        assert_eq!(exited_terminal(&list, &Event::Agent(agent("b", "closed"))), None);
        assert_eq!(exited_terminal(&list, &Event::Agent(agent("c", "closed"))), None);
        assert_eq!(exited_terminal(&list, &Event::Agent(agent("a", "done"))), None);
    }

    fn sync(alerts: &mut Alerts, list: &[Summary], quiet: bool) -> Vec<Notice> {
        asking(alerts, list, &[], quiet)
    }

    fn asking(alerts: &mut Alerts, list: &[Summary], asks: &[(&str, &str)], quiet: bool) -> Vec<Notice> {
        let pending = asks.iter().map(|(agent, q)| Permission { request_id: q.to_string(), agent_id: agent.to_string(), tool_name: "Bash".into(), ..Default::default() }).collect();
        alerts.sync(&Agents { list: list.to_vec(), pending, ..Default::default() }, |a| format!("app · {}", a.id), |_| !quiet, false)
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
    fn with_on_screen_alerts_on_a_session_in_view_notifies_too() {
        let mut alerts = Alerts::new();
        let list = [agent("a", "working")];
        alerts.view(&["t-a".into()], &list);
        alerts.sync(&Agents { list: list.to_vec(), ..Default::default() }, |a| format!("app · {}", a.id), |_| true, true);
        let show = alerts.sync(&Agents { list: vec![agent("a", "done")], ..Default::default() }, |a| format!("app · {}", a.id), |_| true, true);
        assert_eq!(show, vec![notice("a", "Done", None)]);
    }

    #[test]
    fn each_status_banner_follows_its_own_switch_and_the_master_one() {
        let n = Notifications { failed: false, ..Notifications::default() };
        assert_eq!([Status::NeedsYou, Status::Failed, Status::Done, Status::Working].map(|s| wants_banner(n, s)), [true, false, true, false]);
        let off = Notifications { banners: false, ..Notifications::default() };
        assert!(!wants_banner(off, Status::NeedsYou));
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

    #[test]
    fn a_refused_stop_names_the_session_and_why() {
        let a = agent("a", "working");
        assert_eq!(stop_failed(Some(&a), "This session is still resuming"), "Couldn't stop A: This session is still resuming");
        let untitled = Summary { title: String::new(), provider: "codex".into(), ..agent("b", "working") };
        assert_eq!(stop_failed(Some(&untitled), "Agent is no longer in the foreground"), "Couldn't stop Codex: Agent is no longer in the foreground");
        assert_eq!(stop_failed(None, "Agent not found"), "Couldn't stop the session: Agent not found");
    }

    #[test]
    fn an_upgrade_that_fails_shows_one_toast() {
        let failed = Host { upgrade_failed: "1.2.0".into(), ..Default::default() };
        let first = upgrade_toast(Some(&Host::default()), &failed);
        let again = upgrade_toast(Some(&failed), &failed);
        assert_eq!((first.is_some(), again), (true, None));
    }

    #[test]
    fn a_host_change_without_a_failed_upgrade_shows_no_toast() {
        assert_eq!(upgrade_toast(None, &Host { tailnet: true, ..Default::default() }), None);
    }
}
