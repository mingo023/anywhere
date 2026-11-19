//! The sessions a chat delegated to: the panel over the chat that lists them, the rows in the transcript for what pocketd told the chat of them and the reports the chat took in, and a delegated session's chat with its way back.

mod child_view;
mod delivery;
mod panel;
mod push;
mod reports;

pub(crate) use delivery::{Arrivals, delivered};
pub(crate) use reports::{Group, Report, group, reading, spans, standing};

use crate::desktop::Desktop;
use crate::fleet::Links;
use agents::Summary;
use agents::delegations::{Delegation, Phase};
use agents::hosts::Link;
use gpui_kit::*;
use hosts::Host;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub(crate) struct DelegatedState {
    /// Each delegating chat's panel, by agent id.
    panels: HashMap<String, panel::Panel>,
    /// The chats whose delegations were asked for on this connection.
    asked: HashSet<String>,
    /// Tasks the user cleared from a panel: pocketd lists them again after a reconnect, and they stay hidden.
    cleared: HashSet<String>,
    /// The delegated session each chat's tab shows in place of the chat itself, by the chat's agent id.
    viewing: HashMap<String, String>,
    /// The session arriving from the one the user just left, while it moves.
    push: Option<push::Push>,
    /// A time to draw the push at instead of now, so a capture can hold it midway.
    pub(crate) clock: Option<Instant>,
}

impl DelegatedState {
    pub(crate) fn new() -> Self {
        Self { panels: HashMap::new(), asked: HashSet::new(), cleared: HashSet::new(), viewing: HashMap::new(), push: None, clock: None }
    }

    fn clear_settled(&mut self, list: &[Delegation]) {
        self.cleared.extend(list.iter().filter(|d| d.phase().settled()).map(|d| d.task_id.clone()));
    }

    /// A new pocketd connection has told nothing yet.
    fn reconnected(&mut self) {
        self.asked.clear();
    }

    /// Shows `agent` in its owner's tab, or the owner's own chat when it has a tab of its own.
    fn show(&mut self, list: &[Summary], agent: &str) -> String {
        match owner(list, agent) {
            Some(owner) => {
                self.viewing.insert(owner.to_string(), agent.to_string());
                owner.to_string()
            }
            None => {
                self.viewing.remove(agent);
                agent.to_string()
            }
        }
    }

    /// The terminal the tab holding `terminal` shows: that of the delegated session in its place, else its own.
    pub(crate) fn shown_term(&self, list: &[Summary], terminal: &str) -> String {
        let shown = list.iter().find(|a| a.terminal_id == terminal).and_then(|a| self.viewing.get(&a.id)).filter(|id| owner(list, id).is_some());
        shown.and_then(|id| list.iter().find(|a| a.id == *id)).map_or_else(|| terminal.to_string(), |a| a.terminal_id.clone())
    }

    /// The chat of `agent` shows again in its tab.
    pub(crate) fn leave(&mut self, agent: &str) {
        self.viewing.remove(agent);
    }

    /// The chats of `chats` not asked about on this connection, now taken as asked.
    fn ask(&mut self, chats: impl IntoIterator<Item = String>) -> Vec<String> {
        chats.into_iter().filter(|c| self.asked.insert(c.clone())).collect()
    }
}

fn shown<'a>(list: &'a [Delegation], cleared: &'a HashSet<String>) -> impl Iterator<Item = &'a Delegation> + Clone {
    list.iter().filter(|d| !cleared.contains(&d.task_id))
}

/// The session `link` opens, for `anywhere://session/<id>`.
pub(crate) fn session_link(link: &str) -> Option<&str> {
    link.strip_prefix("anywhere://session/").map(|id| id.trim_end_matches('/')).filter(|id| !id.is_empty())
}

/// The chat whose tab shows `agent`, when `agent` has no tab of its own: the nearest chat above it that is listed and isn't hidden itself.
fn owner<'a>(list: &'a [Summary], agent: &str) -> Option<&'a str> {
    let hidden = crate::status::hidden_children(list);
    let mut at = list.iter().find(|a| a.id == agent).filter(|a| hidden.contains(a.id.as_str()))?;
    for _ in 0..list.len() {
        at = list.iter().find(|a| a.id == at.parent)?;
        if !hidden.contains(at.id.as_str()) {
            return Some(at.id.as_str());
        }
    }
    None
}

/// The chats in `list` that have a delegated session, and those `viewing`, that weren't asked about yet. A chat whose delegated
/// sessions all run on linked hosts has none in `list`.
fn unasked(list: &[Summary], viewing: &[String], asked: &HashSet<String>) -> Vec<String> {
    let listed = list.iter().filter(|a| crate::status::is_child(a)).map(|a| &a.parent);
    let mut parents: Vec<String> = listed.chain(viewing).filter(|p| !asked.contains(*p)).cloned().collect();
    parents.sort();
    parents.dedup();
    parents
}

/// The host holding `agent` when the session isn't listed here because that host isn't connected.
fn unlisted_host<'a>(list: &[Summary], agent: &'a str) -> Option<&'a str> {
    hosts::session::untag(agent).0.filter(|_| !list.iter().any(|a| a.id == agent))
}

/// The name of the linked host a working task's session runs on, while that host's link is down. A host still connecting, or not listed (the list may not have arrived), isn't flagged.
fn unreachable_host(d: &Delegation, list: &[Host], links: &Links) -> Option<String> {
    let host = hosts::session::untag(&d.child_id).0.filter(|_| d.phase() == Phase::Working)?;
    let host = list.iter().find(|h| h.id == host)?;
    matches!(links.get(&host.id), Some(Link::Retrying(_) | Link::Failed(_) | Link::Unknown)).then(|| host.name.clone())
}

/// What a task's row says in place of its phase while its host is down.
fn unreachable_label(host: &str) -> String {
    format!("Host {host} unreachable")
}

impl Desktop {
    /// Asks pocketd for the delegations of each chat that just showed a delegated session or is on screen; it pushes the rest.
    pub(crate) fn delegated_listed(&mut self) {
        let chats = unasked(&self.agents.list, self.alerts.viewing.as_deref().unwrap_or_default(), &self.delegated.asked);
        self.delegated_ask(chats);
    }

    /// Where `agent` is shown: its own tab, or for a delegated session the tab of the chat that delegated it, which then shows it. Returns that chat or `agent`.
    pub(crate) fn delegated_show(&mut self, agent: &str) -> String {
        self.delegated.show(&self.agents.list, agent)
    }

    /// The terminal the tab holding `terminal` shows.
    pub(crate) fn delegated_term(&self, terminal: &str) -> String {
        self.delegated.shown_term(&self.agents.list, terminal)
    }

    /// The session `agent` is arriving from while it arrives, and how far across and how faded in each of the two is.
    pub(crate) fn delegated_pushed(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) -> Option<(String, push::Frame)> {
        let reduce_motion = cx.reduce_motion();
        let now = self.delegated.clock.unwrap_or_else(Instant::now);
        let push = self.delegated.push.as_mut().filter(|p| p.to == agent)?;
        if !push.tick(now, reduce_motion) {
            self.delegated.push = None;
            return None;
        }
        window.request_animation_frame();
        Some((push.from.clone(), push.frame()))
    }

    /// Runs the push on `ms` ms, and holds it there.
    pub(crate) fn delegated_hold_push(&mut self, ms: u64, reduce_motion: bool) {
        let start = Instant::now();
        let Some(push) = self.delegated.push.as_mut() else { return };
        (0..=ms / 16).for_each(|i| _ = push.tick(start + Duration::from_millis(16 * i), reduce_motion));
        self.delegated.clock = Some(start + Duration::from_millis(ms));
    }

    /// Asks for the delegations of the chats in `chats` not asked about yet.
    pub(crate) fn delegated_ask(&mut self, chats: impl IntoIterator<Item = String>) {
        for chat in self.delegated.ask(chats) {
            self.outbox.delegations(&chat);
        }
    }

    pub(crate) fn delegated_reconnected(&mut self) {
        self.delegated.reconnected();
    }

    pub(crate) fn delegated_open(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(host) = unlisted_host(&self.agents.list, agent) {
            self.error = Some(format!("That session runs on {}, which isn't connected", self.host_name(host)));
            return cx.notify();
        }
        let from = self.session().map(str::to_string);
        self.delegated.push = from.and_then(|from| push::direction(&self.agents.list, &from, agent).map(|dir| push::Push::begin(self.delegated.push.as_ref(), from, agent.to_string(), dir)));
        self.focus_agent(agent, window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{DelegatedState, session_link, shown, unasked, unlisted_host, unreachable_host};
    use crate::fleet::Links;
    use agents::Summary;
    use agents::delegations::Delegation;
    use agents::hosts::{Failure, Link, Target};
    use hosts::Host;
    use std::collections::HashSet;

    fn child(id: &str, parent: &str, relationship: &str) -> Summary {
        Summary { id: id.into(), parent: parent.into(), relationship: relationship.into(), ..Default::default() }
    }

    #[test]
    fn a_session_link_names_the_session_it_opens() {
        assert_eq!(session_link("anywhere://session/a2"), Some("a2"));
        assert_eq!(session_link("anywhere://session/host/a2/"), Some("host/a2"));
        assert_eq!([session_link("anywhere://session/"), session_link("anywhere://pair?v=1"), session_link("https://x.dev/session/a2")], [None, None, None]);
    }

    #[test]
    fn a_chat_on_screen_is_asked_for_its_delegations_once_a_connection() {
        let mut state = DelegatedState::new();
        assert_eq!(state.ask(["p".to_string(), "q".to_string()]), ["p", "q"]);
        assert!(state.ask(["p".to_string()]).is_empty());
        state.reconnected();
        assert_eq!(state.ask(["p".to_string()]), ["p"]);
    }

    #[test]
    fn a_chat_is_asked_for_its_delegations_once_a_delegated_session_shows() {
        let list = [child("a", "", ""), child("b", "p", "subagent"), child("c", "p", "subagent"), child("d", "q", "fork"), child("e", "r", "subagent")];
        assert_eq!(unasked(&list, &[], &HashSet::new()), ["p", "r"]);
        assert_eq!(unasked(&list, &[], &HashSet::from(["p".to_string()])), ["r"]);
    }

    #[test]
    fn a_chat_on_screen_is_asked_for_its_delegations_with_none_of_them_listed() {
        let list = [child("b", "p", "subagent")];
        let viewing = ["s".to_string(), "p".to_string()];
        assert_eq!(unasked(&list, &viewing, &HashSet::new()), ["p", "s"]);
        assert_eq!(unasked(&list, &viewing, &HashSet::from(["s".to_string()])), ["p"]);
    }

    #[test]
    fn a_session_on_a_host_not_connected_is_not_listed_but_names_its_host() {
        let list = [child("devbox/c", "p", "subagent"), child("c2", "p", "subagent")];
        assert_eq!([unlisted_host(&list, "devbox/c"), unlisted_host(&list, "gpu/c"), unlisted_host(&list, "c2"), unlisted_host(&list, "gone")], [None, Some("gpu"), None, None]);
    }

    fn on_host(child: &str, status: &str) -> Delegation {
        Delegation { child_id: child.into(), status: status.into(), ..Default::default() }
    }

    fn unreachable_with(link: Option<Link>, d: &Delegation) -> Option<String> {
        let list = [Host { id: "gpu".into(), name: "GPU box".into(), target: Target::Alias { name: "gpu".into() }, windows: false, socket: Default::default() }];
        unreachable_host(d, &list, &Links(link.into_iter().map(|l| ("gpu".to_string(), l)).collect()))
    }

    #[test]
    fn a_working_task_on_a_host_whose_link_is_down_names_the_host() {
        let working = on_host("gpu/c", "working");
        let online = Some(Link::Online { version: "1.4.2".into(), uptime: 5 });
        assert_eq!(unreachable_with(online, &working), None);
        for down in [Link::Retrying(Failure::Dropped), Link::Failed(Failure::Auth), Link::Unknown] {
            assert_eq!(unreachable_with(Some(down), &working).as_deref(), Some("GPU box"));
        }
        assert_eq!(unreachable_with(Some(Link::Connecting), &working), None, "a link coming up");
    }

    #[test]
    fn a_local_task_a_settled_one_and_one_on_a_host_not_listed_are_never_unreachable() {
        let down = || Some(Link::Retrying(Failure::TimedOut));
        assert_eq!(unreachable_with(down(), &on_host("c", "working")), None);
        assert_eq!(unreachable_with(down(), &on_host("", "working")), None);
        for status in ["completed", "failed", "cancelled", "starting"] {
            assert_eq!(unreachable_with(down(), &on_host("gpu/c", status)), None, "{status}");
        }
        assert_eq!(unreachable_with(down(), &on_host("other/c", "working")), None);
        let asking = Delegation { question: "Which?".into(), ..on_host("gpu/c", "working") };
        assert_eq!(unreachable_with(down(), &asking), None, "a question still shows");
    }

    fn task(id: &str, status: &str) -> Delegation {
        Delegation { task_id: id.into(), status: status.into(), ..Default::default() }
    }

    fn ids(state: &DelegatedState, list: &[Delegation]) -> Vec<String> {
        shown(list, &state.cleared).map(|d| d.task_id.clone()).collect()
    }

    #[test]
    fn clearing_hides_the_ended_tasks_and_keeps_the_ones_at_work() {
        let mut state = DelegatedState::new();
        let list = [task("a", "completed"), task("b", "working"), task("c", "failed")];
        state.clear_settled(&list);
        assert_eq!(ids(&state, &list), ["b"]);
    }

    #[test]
    fn cleared_tasks_stay_hidden_when_a_reconnect_lists_them_again() {
        let mut state = DelegatedState::new();
        let list = [task("a", "completed"), task("b", "working")];
        state.clear_settled(&list);
        state.reconnected();
        assert_eq!(ids(&state, &list), ["b"]);
        let later = [task("a", "completed"), task("b", "completed"), task("c", "completed")];
        assert_eq!(ids(&state, &later), ["b", "c"], "only what was cleared stays cleared");
    }

    fn chat(id: &str, parent: &str, relationship: &str) -> Summary {
        Summary { terminal_id: format!("t-{id}"), ..child(id, parent, relationship) }
    }

    #[test]
    fn a_delegated_session_is_shown_in_the_tab_of_the_chat_that_delegated_it() {
        let list = [chat("p", "", ""), chat("c", "p", "subagent"), chat("g", "c", "subagent"), chat("o", "gone", "subagent")];
        let mut state = DelegatedState::new();
        assert_eq!(state.show(&list, "c"), "p");
        assert_eq!(state.shown_term(&list, "t-p"), "t-c");
        assert_eq!(state.show(&list, "g"), "p", "a grandchild shows in the same tab");
        assert_eq!(state.shown_term(&list, "t-p"), "t-g");
        assert_eq!(state.show(&list, "o"), "o", "a child whose chat is gone has a tab of its own");
        assert_eq!(state.shown_term(&list, "t-o"), "t-o");
        assert_eq!(state.show(&list, "p"), "p");
        assert_eq!(state.shown_term(&list, "t-p"), "t-p", "the chat itself shows again");
    }

    #[test]
    fn a_tab_shows_its_own_chat_once_the_session_in_its_place_is_no_longer_delegated() {
        let mut list = vec![chat("p", "", ""), chat("c", "p", "subagent")];
        let mut state = DelegatedState::new();
        state.show(&list, "c");
        list.remove(0);
        assert_eq!(state.shown_term(&list, "t-p"), "t-p");
        list.insert(0, chat("p", "", ""));
        assert_eq!(state.shown_term(&list, "t-p"), "t-c");
        list.pop();
        assert_eq!(state.shown_term(&list, "t-p"), "t-p");
        state.leave("p");
        assert!(state.viewing.is_empty());
    }
}
