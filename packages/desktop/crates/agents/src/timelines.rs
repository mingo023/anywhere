use crate::event::Frame;
use crate::{Event, Item, Summary};
use serde_json::{Value, json};
use std::collections::{BTreeMap, HashMap};

/// The most items pocketd sends in one reply.
pub(crate) const PAGE: usize = 500;
/// Ids of the requests the link tells apart by their replies: the newest page, the page above what is held, and what is newer than it.
pub(crate) const NEWEST: &str = "t-";
pub(crate) const OLDER: &str = "o-";
pub(crate) const SINCE: &str = "s-";

pub(crate) fn request(prefix: &str, agent: &str) -> Value {
    json!({"type": "agent.timeline", "id": format!("{prefix}{agent}"), "agentId": agent, "limit": PAGE})
}

#[derive(Default)]
pub(crate) struct Timelines {
    held: BTreeMap<String, Held>,
    resets: HashMap<String, u64>,
}

struct Held {
    /// Whether the newest page came; until then the ask is still owed.
    loaded: bool,
    max_seq: i64,
    oldest: i64,
    has_older: bool,
    /// The `beforeSeq` of the older page on its way.
    older: Option<i64>,
    /// Tool calls not yet finished, by item id: pocketd rewrites them, as it does a streaming draft, under the same seq.
    running: HashMap<String, i64>,
}

impl Held {
    fn asked() -> Self {
        Held { loaded: false, max_seq: 0, oldest: i64::MAX, has_older: false, older: None, running: HashMap::new() }
    }

    fn took(&mut self, items: &[Item], max_seq: i64) {
        for i in items {
            self.oldest = self.oldest.min(i.seq);
            self.max_seq = self.max_seq.max(i.seq);
            match i.call.as_ref().filter(|c| c.status == "running") {
                Some(_) => self.running.insert(i.id.clone(), i.seq),
                None => self.running.remove(&i.id),
            };
        }
        self.max_seq = self.max_seq.max(max_seq);
    }

    fn newest(items: &[Item], max_seq: i64, has_older: bool) -> Self {
        let mut held = Held { loaded: true, has_older, ..Held::asked() };
        held.took(items, max_seq);
        held
    }

    fn resume_below_rewritable(&self) -> i64 {
        let open = self.running.values().min().map_or(i64::MAX, |seq| seq - 1);
        (self.max_seq - 1).min(open).max(0)
    }
}

impl Timelines {
    pub(crate) fn ask(&mut self, mut m: Value) -> Option<Value> {
        if m["type"] != "agent.timeline" {
            return Some(m);
        }
        let agent = m["agentId"].as_str()?.to_string();
        match m["id"].as_str()?.get(..2)? {
            NEWEST if self.held.contains_key(&agent) => None,
            NEWEST => {
                self.held.insert(agent, Held::asked());
                Some(m)
            }
            OLDER => {
                let held = self.held.get_mut(&agent).filter(|h| h.loaded && h.has_older && h.older.is_none() && h.oldest < i64::MAX)?;
                held.older = Some(held.oldest);
                m["beforeSeq"] = held.oldest.into();
                Some(m)
            }
            _ => Some(m),
        }
    }

    /// The event for a reply to one of those asks. An older page only adds. What is newer merges too, unless it fills a
    /// page, which may leave a gap under it: that one replaces what is held, as the newest page does.
    pub(crate) fn reply(&mut self, f: &mut Frame) -> Option<Event> {
        let (agent, items) = (f.agent_id.clone(), std::mem::take(&mut f.items));
        let held = self.held.get_mut(&agent);
        match f.id.get(..2) {
            Some(OLDER) => {
                let held = held?;
                let before = held.older.take()?;
                let oldest = items.iter().map(|i| i.seq).min().filter(|oldest| *oldest < before);
                held.has_older = f.has_older && oldest.is_some();
                held.took(&items, 0);
                Some(Event::Items(agent, items))
            }
            Some(SINCE) if items.len() < PAGE && held.as_ref().is_some_and(|h| h.loaded) => {
                held?.took(&items, f.max_seq);
                Some(Event::Items(agent, items))
            }
            _ => {
                self.held.insert(agent.clone(), Held::newest(&items, f.max_seq, f.has_older));
                Some(Event::Page { agent, items, max_seq: f.max_seq })
            }
        }
    }

    /// Streamed items move where a reconnect picks up.
    pub(crate) fn streamed(&mut self, agent: &str, item: &Item) {
        if let Some(held) = self.held.get_mut(agent).filter(|h| h.loaded) {
            held.took(std::slice::from_ref(item), 0);
        }
    }

    pub(crate) fn refused(&mut self, id: &str) {
        let agent = id.get(2..).unwrap_or_default();
        match id.get(..2) {
            Some(NEWEST) if self.held.get(agent).is_some_and(|h| !h.loaded) => {
                self.held.remove(agent);
            }
            Some(OLDER) => {
                if let Some(held) = self.held.get_mut(agent) {
                    held.older = None;
                }
            }
            _ => {}
        }
    }

    pub(crate) fn seen(&mut self, a: &Summary) -> Option<Value> {
        if a.status == "closed" {
            self.resets.remove(&a.id);
            self.held.remove(&a.id);
            return None;
        }
        let over = self.resets.insert(a.id.clone(), a.resets).is_some_and(|was| was != a.resets);
        let held = self.held.get_mut(&a.id).filter(|h| over && h.loaded)?;
        *held = Held::asked();
        Some(request(NEWEST, &a.id))
    }

    pub(crate) fn again(&self) -> Vec<Value> {
        let mut asks = Vec::new();
        for (agent, held) in &self.held {
            if !held.loaded {
                asks.push(request(NEWEST, agent));
                continue;
            }
            let mut since = request(SINCE, agent);
            since["sinceSeq"] = held.resume_below_rewritable().into();
            asks.push(since);
            if let Some(before) = held.older {
                let mut older = request(OLDER, agent);
                older["beforeSeq"] = before.into();
                asks.push(older);
            }
        }
        asks
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Call;

    fn item(seq: i64) -> Item {
        Item { id: format!("i{seq}"), seq, kind: "assistant".into(), ..Item::default() }
    }

    fn running(seq: i64) -> Item {
        Item { call: Some(Call { status: "running".into(), ..Call::default() }), ..item(seq) }
    }

    fn page(id: &str, seqs: std::ops::RangeInclusive<i64>, older: bool) -> Frame {
        Frame {
            kind: "agent.timeline".into(),
            id: id.into(),
            agent_id: "a1".into(),
            items: seqs.map(item).collect(),
            has_older: older,
            max_seq: 9,
            ..Frame::default()
        }
    }

    fn loaded(t: &mut Timelines, seqs: std::ops::RangeInclusive<i64>, older: bool) {
        t.ask(request(NEWEST, "a1"));
        t.reply(&mut page("t-a1", seqs, older));
    }

    fn asks_older(t: &mut Timelines) -> Option<Value> {
        t.ask(request(OLDER, "a1"))
    }

    #[test]
    fn the_newest_page_is_asked_once_however_often_the_app_wants_it() {
        let mut t = Timelines::default();
        assert!(t.ask(request(NEWEST, "a1")).is_some());
        assert!(t.ask(request(NEWEST, "a1")).is_none());
        loaded(&mut t, 5..=9, true);
        assert!(t.ask(request(NEWEST, "a1")).is_none());
        assert!(t.ask(request(NEWEST, "a2")).is_some());
    }

    #[test]
    fn an_older_page_is_asked_only_where_pocketd_said_there_is_one_and_none_is_on_its_way() {
        let mut t = Timelines::default();
        assert!(asks_older(&mut t).is_none(), "nothing loaded");
        t.ask(request(NEWEST, "a1"));
        assert!(asks_older(&mut t).is_none(), "the newest page is not here yet");
        t.reply(&mut page("t-a1", 5..=9, true));
        assert_eq!(asks_older(&mut t).unwrap()["beforeSeq"], 5);
        assert!(asks_older(&mut t).is_none(), "one is on its way");
        t.reply(&mut page("o-a1", 1..=4, false));
        assert!(asks_older(&mut t).is_none(), "pocketd said that was the first");
    }

    #[test]
    fn a_chat_with_no_older_items_never_asks_for_them() {
        let mut t = Timelines::default();
        loaded(&mut t, 1..=3, false);
        assert!(asks_older(&mut t).is_none());
    }

    #[test]
    fn a_pocketd_that_ignores_before_seq_is_not_asked_again() {
        let mut t = Timelines::default();
        loaded(&mut t, 5..=9, true);
        asks_older(&mut t);
        t.reply(&mut page("o-a1", 5..=9, true));
        assert!(asks_older(&mut t).is_none());
    }

    #[test]
    fn an_older_page_adds_to_what_is_held() {
        let mut t = Timelines::default();
        loaded(&mut t, 5..=9, true);
        asks_older(&mut t);
        assert!(matches!(t.reply(&mut page("o-a1", 1..=4, true)), Some(Event::Items(_, items)) if items.len() == 4));
        assert_eq!(asks_older(&mut t).unwrap()["beforeSeq"], 1);
    }

    #[test]
    fn a_reconnect_asks_only_what_is_newer_for_the_timelines_loaded() {
        let mut t = Timelines::default();
        loaded(&mut t, 5..=9, true);
        t.ask(request(NEWEST, "a2"));
        let again = t.again();
        assert_eq!(again[0], json!({"type": "agent.timeline", "id": "s-a1", "agentId": "a1", "sinceSeq": 8, "limit": 500}));
        assert_eq!(again[1], request(NEWEST, "a2"));
        assert_eq!(again.len(), 2);
    }

    #[test]
    fn a_reconnect_starts_from_the_oldest_tool_call_still_running_and_asks_again_for_an_older_page_on_its_way() {
        let mut t = Timelines::default();
        loaded(&mut t, 5..=9, true);
        t.streamed("a1", &running(7));
        t.streamed("a1", &item(10));
        asks_older(&mut t);
        let again = t.again();
        assert_eq!(again[0]["sinceSeq"], 6);
        assert_eq!(again[1], json!({"type": "agent.timeline", "id": "o-a1", "agentId": "a1", "beforeSeq": 5, "limit": 500}));
        t.streamed("a1", &item(7));
        assert_eq!(t.again()[0]["sinceSeq"], 9);
    }

    #[test]
    fn a_reconnect_starts_from_a_tool_call_still_running_in_an_older_page() {
        let mut t = Timelines::default();
        loaded(&mut t, 5..=9, true);
        asks_older(&mut t);
        let mut older = page("o-a1", 1..=4, true);
        older.items[1] = running(2);
        t.reply(&mut older);
        assert_eq!(t.again()[0]["sinceSeq"], 1);
    }

    #[test]
    fn what_is_newer_merges_and_leaves_what_pocketd_says_of_older_ones() {
        let mut t = Timelines::default();
        loaded(&mut t, 5..=9, false);
        assert!(matches!(t.reply(&mut page("s-a1", 9..=12, true)), Some(Event::Items(..))));
        assert!(asks_older(&mut t).is_none(), "a since reply always says hasOlder");
        assert_eq!(t.again()[0]["sinceSeq"], 11);
    }

    #[test]
    fn what_is_newer_by_a_whole_page_replaces_what_is_held() {
        let mut t = Timelines::default();
        loaded(&mut t, 1..=3, false);
        let mut full = page("s-a1", 10..=10, true);
        full.items = (10..10 + PAGE as i64).map(item).collect();
        assert!(matches!(t.reply(&mut full), Some(Event::Page { items, .. }) if items.len() == PAGE));
        assert_eq!(asks_older(&mut t).unwrap()["beforeSeq"], 10);
    }

    #[test]
    fn a_session_that_started_over_asks_for_its_newest_page_again() {
        let mut t = Timelines::default();
        let summary = |resets| Summary { id: "a1".into(), resets, ..Summary::default() };
        assert!(t.seen(&summary(0)).is_none());
        loaded(&mut t, 1..=3, false);
        assert!(t.seen(&summary(0)).is_none());
        assert_eq!(t.seen(&summary(1)), Some(request(NEWEST, "a1")));
        assert_eq!(t.again(), [request(NEWEST, "a1")]);
    }

    #[test]
    fn a_refused_ask_is_owed_again_and_a_closed_session_is_let_go() {
        let mut t = Timelines::default();
        t.ask(request(NEWEST, "a1"));
        t.refused("t-a1");
        assert!(t.ask(request(NEWEST, "a1")).is_some());
        t.seen(&Summary { id: "a1".into(), status: "closed".into(), ..Summary::default() });
        assert!(t.again().is_empty());
    }

    #[test]
    fn other_messages_pass_through() {
        let mut t = Timelines::default();
        let m = json!({"type": "agent.seen", "id": "seen", "agentIds": []});
        assert_eq!(t.ask(m.clone()), Some(m));
    }
}
