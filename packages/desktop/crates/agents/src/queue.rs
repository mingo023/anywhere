use serde::Deserialize;
use std::collections::{HashMap, HashSet};

/// A message pocketd holds for a session until its turn ends.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Entry {
    pub id: String,
    pub text: String,
    /// `user`, or `wake` or `question` for a notice pocketd sends the session on its own.
    pub origin: String,
    /// `pending`, `sending`, or `held` while a Stop of the user's waits for their next send.
    pub state: String,
    pub order: f64,
}

impl Entry {
    pub fn held(&self) -> bool {
        self.state == "held"
    }
}

/// Each session's queue as pocketd lists it, by the session's agent id. pocketd owns it: a change comes back as the new list.
#[derive(Default, Debug)]
pub struct Queues {
    by_agent: HashMap<String, Vec<Entry>>,
    /// The sessions whose queue was asked for on this connection.
    asked: HashSet<String>,
}

impl Queues {
    pub fn of(&self, agent: &str) -> &[Entry] {
        self.by_agent.get(agent).map_or(&[], Vec::as_slice)
    }

    /// Takes `agent`'s whole queue, as `queue.list` answers and `queue.update` pushes it.
    pub fn replace(&mut self, agent: &str, mut entries: Vec<Entry>) {
        entries.sort_by(|a, b| a.order.total_cmp(&b.order));
        if entries.is_empty() {
            self.by_agent.remove(agent);
        } else {
            self.by_agent.insert(agent.to_string(), entries);
        }
    }

    /// A Stop of the user's holds their entries until their next send.
    pub fn paused(&self, agent: &str) -> bool {
        self.of(agent).iter().any(Entry::held)
    }

    /// The `before` for moving `entry` one place, "" for the end; None at that edge or when not queued.
    pub fn before_move(&self, agent: &str, entry: &str, up: bool) -> Option<String> {
        let list = self.of(agent);
        let at = list.iter().position(|e| e.id == entry)?;
        if up {
            return at.checked_sub(1).map(|i| list[i].id.clone());
        }
        (at + 1 < list.len()).then(|| list.get(at + 2).map_or(String::new(), |e| e.id.clone()))
    }

    pub fn ask(&mut self, agent: &str) -> bool {
        self.asked.insert(agent.to_string())
    }

    /// A new connection to the pocketd that holds `source`'s sessions (this Mac's for `None`) has told nothing yet.
    pub fn reconnected(&mut self, source: Option<&str>) {
        self.asked.retain(|id| !of_source(id, source));
    }

    pub fn forget(&mut self, source: &str) {
        self.by_agent.retain(|id, _| !of_source(id, Some(source)));
        self.reconnected(Some(source));
    }
}

fn of_source(id: &str, source: Option<&str>) -> bool {
    id.split_once('/').map(|(h, _)| h) == source
}

#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Frame {
    agent_id: String,
    entries: Vec<Entry>,
}

/// A queue frame's agent and entries: pocketd's `queue.list` answer or its `queue.update` push.
pub(crate) fn decode(kind: &str, raw: &str) -> Option<(String, Vec<Entry>)> {
    if !matches!(kind, "queue.list" | "queue.update") {
        return None;
    }
    let f: Frame = serde_json::from_str(raw).ok()?;
    Some((f.agent_id, f.entries))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Outbox;
    use serde_json::Value;

    macro_rules! golden_text {
        ($name:literal) => {
            include_str!(concat!("../../../../pocketd/internal/proto/testdata/golden/", $name))
        };
    }

    fn entry(id: &str, order: f64, state: &str) -> Entry {
        Entry { id: id.into(), text: format!("text {id}"), origin: "user".into(), state: state.into(), order }
    }

    fn ids(q: &Queues) -> Vec<&str> {
        q.of("a1").iter().map(|e| e.id.as_str()).collect()
    }

    #[test]
    fn the_golden_list_and_update_decode_in_the_order_pocketd_gave() {
        let (agent, entries) = decode("queue.list", golden_text!("server/queue_list.json")).unwrap();
        let mut q = Queues::default();
        q.replace(&agent, entries);
        assert_eq!((ids(&q), q.of("a1")[1].origin.as_str(), q.of("a1")[0].state.as_str()), (vec!["e1", "e2"], "wake", "pending"));
        let (agent, entries) = decode("queue.update", golden_text!("server/queue_update.json")).unwrap();
        q.replace(&agent, entries);
        assert!(q.of("a1").is_empty());
        assert!(decode("agent.list", "{}").is_none());
    }

    #[test]
    fn entries_go_in_order_whatever_order_pocketd_listed_them() {
        let mut q = Queues::default();
        q.replace("a1", vec![entry("c", 3., "pending"), entry("a", 1., "pending"), entry("b", 1.5, "pending")]);
        assert_eq!(ids(&q), ["a", "b", "c"]);
    }

    #[test]
    fn an_edit_changes_the_text_and_keeps_the_place() {
        let mut q = Queues::default();
        q.replace("a1", vec![entry("a", 1., "pending"), entry("b", 2., "pending")]);
        q.replace("a1", vec![Entry { text: "fix lint too".into(), ..entry("a", 1., "pending") }, entry("b", 2., "pending")]);
        assert_eq!((ids(&q), q.of("a1")[0].text.as_str()), (vec!["a", "b"], "fix lint too"));
    }

    #[test]
    fn a_cancelled_entry_and_one_sent_leave_the_list() {
        let mut q = Queues::default();
        q.replace("a1", vec![entry("a", 1., "pending"), entry("b", 2., "pending"), entry("c", 3., "pending")]);
        q.replace("a1", vec![entry("b", 2., "pending"), entry("c", 3., "pending")]);
        assert_eq!(ids(&q), ["b", "c"]);
        q.replace("a1", vec![entry("c", 3., "pending")]);
        assert_eq!(ids(&q), ["c"]);
    }

    #[test]
    fn a_reorder_comes_back_as_the_new_order_and_the_move_names_what_it_goes_ahead_of() {
        let mut q = Queues::default();
        q.replace("a1", vec![entry("a", 1., "pending"), entry("b", 2., "pending"), entry("c", 3., "pending")]);
        assert_eq!(
            [q.before_move("a1", "b", true), q.before_move("a1", "a", false), q.before_move("a1", "b", false)],
            [Some("a".into()), Some("c".into()), Some(String::new())]
        );
        assert_eq!([q.before_move("a1", "a", true), q.before_move("a1", "c", false), q.before_move("a1", "gone", true)], [None, None, None]);
        q.replace("a1", vec![entry("a", 1., "pending"), entry("b", 4., "pending"), entry("c", 3., "pending")]);
        assert_eq!(ids(&q), ["a", "c", "b"]);
    }

    #[test]
    fn a_stop_holds_the_users_entries_and_the_next_send_lets_them_go() {
        let mut q = Queues::default();
        q.replace("a1", vec![entry("a", 1., "pending")]);
        assert!(!q.paused("a1"));
        q.replace("a1", vec![entry("a", 1., "held"), entry("b", 2., "held")]);
        assert!(q.paused("a1"));
        q.replace("a1", vec![entry("a", 1., "pending"), entry("b", 2., "pending")]);
        assert!(!q.paused("a1"));
    }

    #[test]
    fn a_queue_is_asked_for_once_per_connection_and_a_host_forgotten_takes_only_its_own() {
        let mut q = Queues::default();
        assert!([q.ask("a1"), q.ask("a1"), q.ask("h/a2")] == [true, false, true]);
        q.reconnected(None);
        assert!([q.ask("a1"), q.ask("h/a2")] == [true, false]);
        q.replace("h/a2", vec![entry("a", 1., "pending")]);
        q.replace("a1", vec![entry("b", 1., "pending")]);
        q.forget("h");
        assert!(q.of("h/a2").is_empty() && !q.of("a1").is_empty() && q.ask("h/a2"));
    }

    #[test]
    fn queue_verbs_read_as_pocketds_goldens() {
        let (tx, rx) = std::sync::mpsc::channel();
        let out = Outbox::from(tx);
        let sent = || {
            let mut m: Value = rx.try_recv().unwrap();
            m["id"] = Value::Null;
            m
        };
        let golden = |raw: &str| {
            let mut m: Value = serde_json::from_str(raw).unwrap();
            m["id"] = Value::Null;
            m
        };
        out.queue_list("a1");
        assert_eq!(sent(), golden(golden_text!("client/queue_list.json")));
        out.queue_cancel("a1", "e1");
        assert_eq!(sent(), golden(golden_text!("client/queue_cancel.json")));
        out.queue_send_now("a1", "e1");
        assert_eq!(sent(), golden(golden_text!("client/queue_send_now.json")));
        out.queue_edit("a1", "e1", "fix lint too");
        assert_eq!(sent(), golden(golden_text!("client/queue_edit.json")));
        out.queue_reorder("a1", "e2", "e1");
        assert_eq!(sent(), golden(golden_text!("client/queue_reorder.json")));
        out.prompt_queued("a1", "after this");
        assert_eq!(sent(), golden(golden_text!("client/agent_prompt_queue.json")));
    }
}
