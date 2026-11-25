use crate::Event;
use crate::event::Frame;
use crate::link::refused;
use serde_json::Value;
use std::time::{Duration, Instant};

pub(super) const DOWN_LIMIT: Duration = Duration::from_secs(120);
pub(super) const UNREACHABLE: &str = "can't reach Anywhere's service";

/// Where a sent prompt stands between the link, pocketd and the agent.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PromptState {
    /// On its way; nothing to say about it yet.
    Sending,
    /// The link is down; the prompt goes out again when it is back.
    Resending,
    /// pocketd committed it and the agent hasn't started it.
    Delivered,
    /// The agent took it, or holds it in its queue; its echo is what's left to wait for.
    Taken,
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Stage {
    Unsent,
    /// Written on a link that has since dropped; goes out again if the next pocketd keeps receipts.
    Lost,
    Written,
    Acked,
}

struct Entry {
    frame: Value,
    stage: Stage,
}

/// The `agent.prompt` frames not yet answered by pocketd's `agent.delivery` or an error. With `delivery.v1` a prompt
/// resent under its id is delivered once, so the link resends what a drop left in doubt; without it, a prompt sent
/// before a drop fails, as resending could type it twice.
pub(super) struct Resend {
    entries: Vec<Entry>,
    receipts: bool,
    /// Since when the link has had no `hello.ok`; failed attempts leave it be.
    down_since: Option<Instant>,
    notes: Vec<String>,
}

pub(super) fn is_prompt(m: &Value) -> bool {
    m["type"] == "agent.prompt"
}

impl Resend {
    pub(super) fn new(now: Instant) -> Self {
        Resend { entries: Vec::new(), receipts: false, down_since: Some(now), notes: Vec::new() }
    }

    pub(super) fn hold(&mut self, frame: Value, now: Instant) -> Vec<Event> {
        let event = state_event(&frame, PromptState::Resending);
        self.entries.push(Entry { frame, stage: Stage::Unsent });
        let mut events = vec![event];
        events.extend(self.expire(now));
        events
    }

    pub(super) fn sent(&mut self, frame: &Value) {
        match self.entries.iter_mut().find(|e| e.frame["id"] == frame["id"]) {
            Some(e) => e.stage = Stage::Written,
            None => self.entries.push(Entry { frame: frame.clone(), stage: Stage::Written }),
        }
    }

    pub(super) fn lost(&mut self, now: Instant) -> Vec<Event> {
        self.down_since.get_or_insert(now);
        let receipts = std::mem::take(&mut self.receipts);
        let mut events = Vec::new();
        let mut kept = Vec::new();
        for mut e in std::mem::take(&mut self.entries) {
            match e.stage {
                Stage::Unsent | Stage::Lost => kept.push(e),
                Stage::Written | Stage::Acked if receipts => {
                    e.stage = Stage::Lost;
                    events.push(state_event(&e.frame, PromptState::Resending));
                    kept.push(e);
                }
                Stage::Written | Stage::Acked => events.extend(self.fail(&e.frame, UNREACHABLE)),
            }
        }
        self.entries = kept;
        events
    }

    pub(super) fn waiting(&self) -> bool {
        self.entries.iter().any(|e| e.stage == Stage::Lost)
    }

    pub(super) fn linked(&mut self, receipts: bool, now: Instant) -> (Vec<Value>, Vec<Event>) {
        let mut events = self.expire(now);
        self.down_since = None;
        self.receipts = receipts;
        let mut again = Vec::new();
        let mut kept = Vec::new();
        for mut e in std::mem::take(&mut self.entries) {
            match e.stage {
                Stage::Lost if receipts => {
                    e.stage = Stage::Written;
                    events.push(state_event(&e.frame, PromptState::Sending));
                    self.notes.push(format!("resent prompt {} to {}", id(&e.frame), agent8(&e.frame)));
                    again.push(e.frame.clone());
                    kept.push(e);
                }
                Stage::Lost => events.extend(self.fail(&e.frame, UNREACHABLE)),
                _ => kept.push(e),
            }
        }
        self.entries = kept;
        (again, events)
    }

    /// Fails what has waited for the link past `DOWN_LIMIT`, so it can never arrive late.
    pub(super) fn expire(&mut self, now: Instant) -> Vec<Event> {
        if !self.down_since.is_some_and(|since| now.duration_since(since) >= DOWN_LIMIT) {
            return Vec::new();
        }
        let (waiting, kept): (Vec<Entry>, Vec<Entry>) =
            std::mem::take(&mut self.entries).into_iter().partition(|e| matches!(e.stage, Stage::Unsent | Stage::Lost));
        self.entries = kept;
        waiting.iter().flat_map(|e| self.fail(&e.frame, UNREACHABLE)).collect()
    }

    pub(super) fn answer(&mut self, f: &Frame) -> Option<Event> {
        if !matches!(f.kind.as_str(), "ack" | "error" | "agent.delivery") {
            return None;
        }
        let i = self.entries.iter().position(|e| e.frame["id"] == f.id.as_str())?;
        if f.kind == "ack" {
            if !self.receipts {
                self.entries.remove(i);
                return None;
            }
            self.entries[i].stage = Stage::Acked;
            return Some(state_event(&self.entries[i].frame, PromptState::Delivered));
        }
        let e = self.entries.remove(i);
        match (f.kind.as_str(), f.state.as_str()) {
            ("agent.delivery", "failed") => self.fail(&e.frame, &f.error),
            ("agent.delivery", _) => Some(state_event(&e.frame, PromptState::Taken)),
            _ => self.fail(&e.frame, &f.message),
        }
    }

    pub(super) fn notes(&mut self) -> Vec<String> {
        std::mem::take(&mut self.notes)
    }

    fn fail(&mut self, frame: &Value, reason: &str) -> Option<Event> {
        self.notes.push(format!("prompt {} to {} not delivered: {reason}", id(frame), agent8(frame)));
        refused(frame, reason)
    }
}

fn state_event(frame: &Value, state: PromptState) -> Event {
    Event::PromptState { agent: frame["agentId"].as_str().unwrap_or_default().into(), request: id(frame).into(), state }
}

fn id(frame: &Value) -> &str {
    frame["id"].as_str().unwrap_or_default()
}

fn agent8(frame: &Value) -> &str {
    let agent = frame["agentId"].as_str().unwrap_or_default();
    agent.get(..8).unwrap_or(agent)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn prompt(id: &str) -> Value {
        json!({"type": "agent.prompt", "id": id, "agentId": "a1b2c3d4e5", "text": "Fix the login bug"})
    }

    fn frame(raw: Value) -> Frame {
        serde_json::from_value(raw).unwrap()
    }

    fn after(t: Instant, secs: u64) -> Instant {
        t + Duration::from_secs(secs)
    }

    fn up(receipts: bool, now: Instant) -> Resend {
        let mut resend = Resend::new(now);
        resend.linked(receipts, now);
        resend
    }

    fn failed(events: &[Event]) -> Vec<(&str, &str)> {
        events
            .iter()
            .filter_map(|ev| if let Event::PromptFailed { request, reason, .. } = ev { Some((request.as_str(), reason.as_str())) } else { None })
            .collect()
    }

    fn states(events: &[Event]) -> Vec<(&str, PromptState)> {
        events.iter().filter_map(|ev| if let Event::PromptState { request, state, .. } = ev { Some((request.as_str(), *state)) } else { None }).collect()
    }

    #[test]
    fn a_prompt_sent_while_the_link_is_down_joins_the_list_and_reads_as_resending() {
        let t = Instant::now();
        let mut resend = Resend::new(t);
        let events = resend.hold(prompt("prompt-x-1"), after(t, 5));
        assert_eq!(states(&events), [("prompt-x-1", PromptState::Resending)]);
        assert!(!resend.waiting(), "never written, so nothing is in doubt");
        let (again, events) = resend.linked(true, after(t, 6));
        assert!(again.is_empty() && events.is_empty(), "the held message goes out as the link drains its queue");
        assert!(resend.expire(after(t, 600)).is_empty(), "an up link fails nothing");
        assert!(resend.entries.iter().any(|e| e.stage == Stage::Unsent));
    }

    #[test]
    fn an_unacked_prompt_goes_again_under_its_id_when_the_next_link_keeps_receipts() {
        let t = Instant::now();
        let mut resend = up(true, t);
        resend.sent(&prompt("prompt-x-1"));
        let events = resend.lost(after(t, 1));
        assert_eq!(states(&events), [("prompt-x-1", PromptState::Resending)]);
        assert!(resend.waiting());
        let (again, events) = resend.linked(true, after(t, 3));
        assert_eq!(again, [prompt("prompt-x-1")]);
        assert_eq!(states(&events), [("prompt-x-1", PromptState::Sending)]);
        assert_eq!(resend.notes(), ["resent prompt prompt-x-1 to a1b2c3d4"]);
        assert!(!resend.waiting());
    }

    #[test]
    fn an_unacked_prompt_fails_when_the_link_never_had_receipts() {
        let t = Instant::now();
        let mut resend = up(false, t);
        resend.sent(&prompt("prompt-x-1"));
        let events = resend.lost(after(t, 1));
        assert_eq!(failed(&events), [("prompt-x-1", UNREACHABLE)]);
        assert!(!resend.waiting() && resend.linked(true, after(t, 3)).0.is_empty(), "nothing is sent again");
    }

    #[test]
    fn a_prompt_in_doubt_fails_when_the_next_pocketd_has_no_receipts() {
        let t = Instant::now();
        let mut resend = up(true, t);
        resend.sent(&prompt("prompt-x-1"));
        resend.lost(after(t, 1));
        let (again, events) = resend.linked(false, after(t, 3));
        assert!(again.is_empty());
        assert_eq!(failed(&events), [("prompt-x-1", UNREACHABLE)]);
    }

    #[test]
    fn an_acked_prompt_without_a_delivery_goes_again_too() {
        let t = Instant::now();
        let mut resend = up(true, t);
        resend.sent(&prompt("prompt-x-1"));
        let acked = resend.answer(&frame(json!({"type": "ack", "id": "prompt-x-1"})));
        assert!(matches!(acked, Some(Event::PromptState { state: PromptState::Delivered, .. })));
        resend.lost(after(t, 1));
        assert_eq!(resend.linked(true, after(t, 2)).0, [prompt("prompt-x-1")]);
    }

    #[test]
    fn an_ack_settles_a_prompt_when_there_are_no_receipts() {
        let t = Instant::now();
        let mut resend = up(false, t);
        resend.sent(&prompt("prompt-x-1"));
        assert!(resend.answer(&frame(json!({"type": "ack", "id": "prompt-x-1"}))).is_none());
        assert!(resend.lost(after(t, 1)).is_empty(), "acked, so nothing to fail");
    }

    #[test]
    fn a_delivery_ends_the_wait_and_a_failed_one_says_why() {
        let t = Instant::now();
        let mut resend = up(true, t);
        for id in ["prompt-x-1", "prompt-x-2", "prompt-x-3"] {
            resend.sent(&prompt(id));
        }
        let delivery =
            |id: &str, state: &str, error: &str| frame(json!({"type": "agent.delivery", "id": id, "agentId": "a1b2c3d4e5", "state": state, "error": error}));
        assert!(matches!(resend.answer(&delivery("prompt-x-1", "started", "")), Some(Event::PromptState { state: PromptState::Taken, .. })));
        assert!(matches!(resend.answer(&delivery("prompt-x-2", "queued", "")), Some(Event::PromptState { state: PromptState::Taken, .. })));
        let refused = resend.answer(&delivery("prompt-x-3", "failed", "This session is still resuming"));
        assert!(matches!(refused, Some(Event::PromptFailed { request, reason, .. }) if request == "prompt-x-3" && reason == "This session is still resuming"));
        assert!(resend.lost(after(t, 1)).is_empty(), "all three left the list");
        assert!(resend.answer(&delivery("prompt-x-1", "started", "")).is_none(), "an answer to nothing in the list");
    }

    #[test]
    fn an_error_fails_the_prompt_with_pocketds_words() {
        let t = Instant::now();
        let mut resend = up(true, t);
        resend.sent(&prompt("prompt-x-1"));
        let refused = resend.answer(&frame(json!({"type": "error", "id": "prompt-x-1", "message": "Agent isn't in the foreground"})));
        assert!(
            matches!(refused, Some(Event::PromptFailed { agent, request, reason }) if agent == "a1b2c3d4e5" && request == "prompt-x-1" && reason == "Agent isn't in the foreground")
        );
        assert!(resend.answer(&frame(json!({"type": "error", "id": "prompt-x-1", "message": "again"}))).is_none());
    }

    #[test]
    fn a_queued_send_that_fails_gives_back_its_text() {
        let t = Instant::now();
        let mut resend = up(true, t);
        let mut queued = prompt("prompt-x-1");
        queued["queue"] = json!(true);
        resend.sent(&queued);
        let refused = resend.answer(&frame(json!({"type": "error", "id": "prompt-x-1", "message": "No such entry"})));
        assert!(
            matches!(refused, Some(Event::QueueFailed { agent, reason, text }) if agent == "a1b2c3d4e5" && reason == "No such entry" && text.as_deref() == Some("Fix the login bug"))
        );
    }

    #[test]
    fn a_prompt_waits_two_minutes_of_a_down_link_then_fails_and_leaves_the_list() {
        let t = Instant::now();
        let mut resend = up(true, t);
        resend.sent(&prompt("prompt-x-1"));
        resend.lost(after(t, 10));
        assert!(resend.expire(after(t, 129)).is_empty());
        let events = resend.expire(after(t, 130));
        assert_eq!(failed(&events), [("prompt-x-1", UNREACHABLE)]);
        assert!(!resend.waiting());
        assert!(resend.linked(true, after(t, 131)).0.is_empty(), "so it can never arrive late");
        assert!(resend.notes().iter().any(|n| n == "prompt prompt-x-1 to a1b2c3d4 not delivered: can't reach Anywhere's service"));
    }

    #[test]
    fn failed_tries_do_not_restart_the_two_minutes_but_a_hello_does() {
        let t = Instant::now();
        let mut resend = up(true, t);
        resend.sent(&prompt("prompt-x-1"));
        resend.lost(after(t, 10));
        resend.lost(after(t, 100));
        assert_eq!(failed(&resend.expire(after(t, 130))).len(), 1, "counted from the first drop");

        resend.linked(true, after(t, 140));
        resend.sent(&prompt("prompt-x-2"));
        resend.lost(after(t, 200));
        assert!(resend.expire(after(t, 319)).is_empty());
        assert_eq!(failed(&resend.expire(after(t, 320))).len(), 1);
    }

    #[test]
    fn a_prompt_sent_after_the_deadline_has_passed_fails_at_once() {
        let t = Instant::now();
        let mut resend = Resend::new(t);
        let events = resend.hold(prompt("prompt-x-1"), after(t, 121));
        assert_eq!(failed(&events), [("prompt-x-1", UNREACHABLE)]);
        assert!(resend.linked(true, after(t, 122)).0.is_empty());
    }

    #[test]
    fn a_prompt_already_written_on_a_link_awaiting_its_hello_is_not_failed_by_the_clock() {
        let t = Instant::now();
        let mut resend = Resend::new(t);
        resend.sent(&prompt("prompt-x-1"));
        assert!(resend.expire(after(t, 600)).is_empty());
    }

    #[test]
    fn the_log_lines_name_the_prompt_and_agent_but_never_its_text() {
        let t = Instant::now();
        let mut resend = up(true, t);
        resend.sent(&prompt("prompt-x-1"));
        resend.lost(after(t, 1));
        resend.linked(true, after(t, 2));
        resend.answer(&frame(json!({"type": "error", "id": "prompt-x-1", "message": "Agent isn't in the foreground"})));
        let notes = resend.notes();
        assert_eq!(notes.len(), 2);
        assert!(notes.iter().all(|n| !n.contains("Fix the login bug")));
        assert!(notes[1].ends_with("not delivered: Agent isn't in the foreground"));
        assert!(resend.notes().is_empty(), "each line is taken once");
    }
}
