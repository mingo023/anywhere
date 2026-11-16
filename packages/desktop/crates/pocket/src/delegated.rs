//! The sessions a chat delegated to: the panel over the chat that lists them, the rows in the transcript for what pocketd told the chat of them, and a delegated session's chat with its way back.

mod child_view;
mod delivery;
mod panel;

pub(crate) use delivery::delivered;

use crate::desktop::Desktop;
use agents::Summary;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};

pub(crate) struct DelegatedState {
    /// Each delegating chat's panel, by agent id.
    panels: HashMap<String, panel::Panel>,
    /// The chats whose delegations were asked for on this connection.
    asked: HashSet<String>,
}

impl DelegatedState {
    pub(crate) fn new() -> Self {
        Self { panels: HashMap::new(), asked: HashSet::new() }
    }
}

/// The session `link` opens, for `anywhere://session/<id>`.
pub(crate) fn session_link(link: &str) -> Option<&str> {
    link.strip_prefix("anywhere://session/").map(|id| id.trim_end_matches('/')).filter(|id| !id.is_empty())
}

/// The chats in `list` that have a delegated session and weren't asked about yet.
fn unasked(list: &[Summary], asked: &HashSet<String>) -> Vec<String> {
    let mut parents: Vec<String> = list.iter().filter(|a| crate::status::is_child(a) && !asked.contains(&a.parent)).map(|a| a.parent.clone()).collect();
    parents.sort();
    parents.dedup();
    parents
}

impl Desktop {
    /// Asks pocketd for the delegations of each chat that just showed a delegated session; it pushes the rest.
    pub(crate) fn delegated_listed(&mut self) {
        for parent in unasked(&self.agents.list, &self.delegated.asked) {
            self.outbox.delegations(&parent);
            self.delegated.asked.insert(parent);
        }
    }

    /// A new pocketd connection has told nothing yet.
    pub(crate) fn delegated_reconnected(&mut self) {
        self.delegated.asked.clear();
    }

    pub(crate) fn delegated_open(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.focus_agent(agent, window, cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{session_link, unasked};
    use agents::Summary;
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
    fn a_chat_is_asked_for_its_delegations_once_a_delegated_session_shows() {
        let list = [child("a", "", ""), child("b", "p", "subagent"), child("c", "p", "subagent"), child("d", "q", "fork"), child("e", "r", "subagent")];
        assert_eq!(unasked(&list, &HashSet::new()), ["p", "r"]);
        assert_eq!(unasked(&list, &HashSet::from(["p".to_string()])), ["r"]);
    }
}
