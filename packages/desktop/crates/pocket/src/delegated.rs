//! The sessions a chat delegated to: the panel over the chat that lists them, the rows in the transcript for what pocketd told the chat of them, and a delegated session's chat with its way back.

mod child_view;
mod delivery;
mod panel;
mod push;

pub(crate) use delivery::{Arrivals, delivered};

use crate::desktop::Desktop;
use agents::Summary;
use agents::delegations::Delegation;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};

pub(crate) struct DelegatedState {
    /// Each delegating chat's panel, by agent id.
    panels: HashMap<String, panel::Panel>,
    /// The chats whose delegations were asked for on this connection.
    asked: HashSet<String>,
    /// Tasks the user cleared from a panel: pocketd lists them again after a reconnect, and they stay hidden.
    cleared: HashSet<String>,
    /// The session arriving from the one the user just left, while it moves.
    push: Option<push::Push>,
    /// A time to draw the push at instead of now, so a capture can hold it midway.
    pub(crate) clock: Option<Instant>,
}

impl DelegatedState {
    pub(crate) fn new() -> Self {
        Self { panels: HashMap::new(), asked: HashSet::new(), cleared: HashSet::new(), push: None, clock: None }
    }

    fn clear_settled(&mut self, list: &[Delegation]) {
        self.cleared.extend(list.iter().filter(|d| d.phase().settled()).map(|d| d.task_id.clone()));
    }

    /// A new pocketd connection has told nothing yet.
    fn reconnected(&mut self) {
        self.asked.clear();
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

/// The chats in `list` that have a delegated session, and those `viewing`, that weren't asked about yet. A chat whose delegated
/// sessions all run on linked hosts has none in `list`.
fn unasked(list: &[Summary], viewing: &[String], asked: &HashSet<String>) -> Vec<String> {
    let listed = list.iter().filter(|a| crate::status::is_child(a)).map(|a| &a.parent);
    let mut parents: Vec<String> = listed.chain(viewing).filter(|p| !asked.contains(*p)).cloned().collect();
    parents.sort();
    parents.dedup();
    parents
}

/// The folders delegated sessions run in that are not a project's own: pocketd made each as a worktree for its session, and the sidebar lists it as one made here.
fn child_trees(list: &[Summary], projects: &[String], tracked: impl Fn(&str) -> bool) -> Vec<String> {
    let mut trees: Vec<String> = list.iter().filter(|a| crate::status::is_child(a) && !a.cwd.is_empty() && !projects.contains(&a.cwd) && !tracked(&a.cwd)).map(|a| a.cwd.clone()).collect();
    trees.sort();
    trees.dedup();
    trees
}

impl Desktop {
    /// Asks pocketd for the delegations of each chat that just showed a delegated session or is on screen; it pushes the rest.
    pub(crate) fn delegated_listed(&mut self, cx: &mut Context<Self>) {
        let chats = unasked(&self.agents.list, self.alerts.viewing.as_deref().unwrap_or_default(), &self.delegated.asked);
        self.delegated_ask(chats);
        let trees = child_trees(&self.agents.list, &self.store.projects, |t| self.store.is_tracked(t));
        if !trees.is_empty() {
            trees.iter().for_each(|t| self.store.track(t));
            self.save_soon(cx);
            self.refresh_git(cx);
        }
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
        let from = self.session().map(str::to_string);
        self.delegated.push = from.and_then(|from| push::direction(&self.agents.list, &from, agent).map(|dir| push::Push::begin(self.delegated.push.as_ref(), from, agent.to_string(), dir)));
        self.focus_agent(agent, window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{DelegatedState, child_trees, session_link, shown, unasked};
    use agents::Summary;
    use agents::delegations::Delegation;
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

    #[test]
    fn a_delegated_session_in_a_folder_of_its_own_makes_that_folder_a_listed_worktree() {
        let with_cwd = |id: &str, parent: &str, relationship: &str, cwd: &str| Summary { cwd: cwd.into(), ..child(id, parent, relationship) };
        let list = [
            with_cwd("p", "", "", "/code/app"),
            with_cwd("a", "p", "subagent", "/code/app.worktrees/tests"),
            with_cwd("b", "p", "subagent", "/code/app.worktrees/tests"),
            with_cwd("c", "p", "subagent", "/code/app"),
            with_cwd("d", "p", "subagent", "/code/app.worktrees/docs"),
            with_cwd("f", "p", "fork", "/code/app.worktrees/fork"),
        ];
        let projects = ["/code/app".to_string()];
        assert_eq!(child_trees(&list, &projects, |_| false), ["/code/app.worktrees/docs", "/code/app.worktrees/tests"]);
        assert_eq!(child_trees(&list, &projects, |t| t.ends_with("docs")), ["/code/app.worktrees/tests"]);
    }
}
