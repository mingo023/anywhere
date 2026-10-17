use crate::Transcript;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Sending,
    /// Held here while the agent works, unsent; it goes out when the turn ends, or when steered.
    Queued,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    /// Stays with the send while others ahead of it settle, so a click finds it.
    pub id: u64,
    pub text: String,
    /// When it went to the agent, which starts its 120s grace; for a queued send, when it was queued.
    pub sent_at: i64,
    pub delivery: Delivery,
}

/// A prompt must reach the transcript within this long of being sent, or it reads as not delivered.
const GRACE_MS: i64 = 120_000;
/// How far the agent's clock may run behind ours when its prompt item is stamped.
const SKEW_MS: i64 = 10_000;
/// Enough claimed prompts to tell a repeated send from the one before it.
const CLAIMED: usize = 32;

#[derive(Default, Debug)]
pub struct Outgoing {
    sends: Vec<Pending>,
    /// Ids of prompts that already settled a send, so a repeat of the same text waits for its own.
    claimed: Vec<String>,
    /// Queued sends `settle` let go, for the caller to send.
    released: Vec<String>,
    next_id: u64,
}

impl Outgoing {
    /// Queued, and not to be sent yet, if the agent was working.
    pub fn push(&mut self, text: String, working: bool, now_ms: i64) {
        let delivery = if working { Delivery::Queued } else { Delivery::Sending };
        self.next_id += 1;
        self.sends.push(Pending { id: self.next_id, text, sent_at: now_ms, delivery });
    }

    /// Drops sends whose prompt reached the transcript and fails those older than 120s. Once the agent is idle
    /// with nothing on its way, lets the oldest queued send go: one per turn, in order.
    pub fn settle(&mut self, t: &Transcript, now_ms: i64) {
        if self.sends.is_empty() {
            return;
        }
        let claimed = &mut self.claimed;
        self.sends.retain(|send| {
            if send.delivery == Delivery::Queued {
                return true;
            }
            let prompt = t
                .turns
                .iter()
                .rev()
                .filter_map(|turn| turn.prompt.as_ref())
                .take_while(|p| p.at >= send.sent_at - SKEW_MS)
                .filter(|p| !claimed.contains(&p.id) && same_text(&p.text, &send.text))
                .last();
            let Some(prompt) = prompt else { return true };
            claimed.push(prompt.id.clone());
            false
        });
        if claimed.len() > CLAIMED {
            claimed.drain(..claimed.len() - CLAIMED);
        }
        for send in &mut self.sends {
            if send.delivery == Delivery::Sending && now_ms - send.sent_at > GRACE_MS {
                send.delivery = Delivery::Failed;
            }
        }
        let idle = t.turns.last().is_none_or(|turn| turn.end.is_some());
        if idle && !self.sends.iter().any(|s| s.delivery == Delivery::Sending) {
            let id = self.sends.iter().find(|s| s.delivery == Delivery::Queued).map(|s| s.id);
            if let Some(text) = id.and_then(|id| self.steer(id, now_ms)) {
                self.released.push(text);
            }
        }
    }

    /// The queued sends `settle` let go since last asked, to send to the agent.
    pub fn released(&mut self) -> Vec<String> {
        std::mem::take(&mut self.released)
    }

    /// Lets a queued send go now, even mid-turn; returns its text to send.
    pub fn steer(&mut self, id: u64, now_ms: i64) -> Option<String> {
        let send = self.sends.iter_mut().find(|s| s.id == id && s.delivery == Delivery::Queued)?;
        (send.delivery, send.sent_at) = (Delivery::Sending, now_ms);
        Some(send.text.clone())
    }

    /// Drops a queued send unsent, handing back its text.
    pub fn withdraw(&mut self, id: u64) -> Option<String> {
        let ix = self.sends.iter().position(|s| s.id == id && s.delivery == Delivery::Queued)?;
        Some(self.sends.remove(ix).text)
    }

    /// Takes a failed send back for resending.
    pub fn retry(&mut self, ix: usize) -> Option<String> {
        let failed = self.sends.get(ix)?.delivery == Delivery::Failed;
        failed.then(|| self.sends.remove(ix).text)
    }

    /// Whether a send waits its turn.
    pub fn holds(&self) -> bool {
        self.sends.iter().any(|s| s.delivery == Delivery::Queued)
    }

    pub fn list(&self) -> &[Pending] {
        &self.sends
    }

    /// The sends the transcript shows, with their index in `list`; a queued one waits in the composer's tray instead.
    pub fn echoed(&self) -> impl Iterator<Item = (usize, &Pending)> {
        self.sends.iter().enumerate().filter(|(_, p)| p.delivery != Delivery::Queued)
    }
}

/// Agents may tidy a prompt's whitespace before echoing it back.
fn same_text(a: &str, b: &str) -> bool {
    a.split_whitespace().eq(b.split_whitespace())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Send,
    Queue,
    Stop,
}

/// Stop while working with an empty draft, Queue while working with one, else Send. Enter never stops.
pub fn action(working: bool, empty: bool) -> Action {
    match (working, empty) {
        (true, true) => Action::Stop,
        (true, false) => Action::Queue,
        (false, _) => Action::Send,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{End, Message, Turn};

    fn turn(id: &str, at: i64, text: &str, ended: bool) -> Turn {
        let end = ended.then_some(End::Done { duration_ms: 1, usage: None });
        Turn { id: id.into(), prompt: Some(Message { id: id.into(), at, text: text.into() }), blocks: vec![], end }
    }

    fn deliveries(o: &Outgoing) -> Vec<Delivery> {
        o.list().iter().map(|p| p.delivery).collect()
    }

    #[test]
    fn a_send_while_idle_is_sending_and_while_working_is_queued() {
        let mut o = Outgoing::default();
        o.push("a".into(), false, 0);
        o.push("b".into(), true, 0);
        assert_eq!(deliveries(&o), vec![Delivery::Sending, Delivery::Queued]);
    }

    #[test]
    fn a_send_settles_when_its_prompt_reaches_the_transcript() {
        let mut o = Outgoing::default();
        o.push("fix  the\nbuild".into(), false, 50_000);
        o.settle(&Transcript { turns: vec![turn("u1", 1_000, "fix the build", true)] }, 50_500);
        assert_eq!(o.list().len(), 1, "a prompt from before the send is not this one");
        o.settle(&Transcript { turns: vec![turn("u1", 1_000, "fix the build", true), turn("u2", 49_000, "fix the build", false)] }, 51_000);
        assert!(o.list().is_empty());
    }

    #[test]
    fn a_repeated_send_waits_for_its_own_prompt() {
        let mut o = Outgoing::default();
        o.push("continue".into(), false, 10_000);
        let first = Transcript { turns: vec![turn("u1", 10_100, "continue", true)] };
        o.settle(&first, 10_200);
        o.push("continue".into(), false, 11_000);
        o.settle(&first, 11_100);
        assert_eq!(o.list().len(), 1);
        o.settle(&Transcript { turns: vec![turn("u1", 10_100, "continue", true), turn("u2", 11_200, "continue", false)] }, 11_300);
        assert!(o.list().is_empty());
    }

    #[test]
    fn a_send_not_seen_in_two_minutes_is_not_delivered() {
        let mut o = Outgoing::default();
        o.push("hello".into(), false, 0);
        let empty = Transcript::default();
        o.settle(&empty, 120_000);
        assert_eq!(deliveries(&o), vec![Delivery::Sending]);
        o.settle(&empty, 120_001);
        assert_eq!(deliveries(&o), vec![Delivery::Failed]);
    }

    fn texts(o: &Outgoing) -> Vec<&str> {
        o.list().iter().map(|p| p.text.as_str()).collect()
    }

    fn id(o: &Outgoing, text: &str) -> u64 {
        o.list().iter().find(|p| p.text == text).unwrap().id
    }

    #[test]
    fn a_send_queued_while_working_is_held_until_the_turn_ends() {
        let mut o = Outgoing::default();
        o.push("next".into(), true, 0);
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", false)] }, 300_000);
        assert_eq!(deliveries(&o), vec![Delivery::Queued], "held past two minutes, never failed");
        assert!(o.released().is_empty());
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", true)] }, 400_000);
        assert_eq!(o.released(), ["next"]);
        assert_eq!(deliveries(&o), vec![Delivery::Sending]);
        assert!(o.released().is_empty(), "handed over once");
    }

    #[test]
    fn queued_sends_go_one_per_turn_in_the_order_queued() {
        let mut o = Outgoing::default();
        o.push("first".into(), true, 0);
        o.push("second".into(), true, 1);
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", true)] };
        o.settle(&ended, 1_000);
        o.settle(&ended, 1_100);
        assert_eq!(o.released(), ["first"], "the second waits while the first is on its way");
        let first = Transcript { turns: vec![turn("u1", 0, "long job", true), turn("u2", 1_050, "first", false)] };
        o.settle(&first, 1_200);
        assert!(o.released().is_empty(), "the agent works on the first");
        assert_eq!(texts(&o), ["second"]);
        let done = Transcript { turns: vec![turn("u1", 0, "long job", true), turn("u2", 1_050, "first", true)] };
        o.settle(&done, 5_000);
        assert_eq!(o.released(), ["second"]);
    }

    #[test]
    fn a_released_send_gets_two_minutes_from_its_release() {
        let mut o = Outgoing::default();
        o.push("next".into(), true, 0);
        let ended = Transcript { turns: vec![turn("u1", 0, "next", true)] };
        o.settle(&Transcript { turns: vec![turn("u1", 0, "next", false)] }, 1_000);
        o.settle(&ended, 400_000);
        assert_eq!(deliveries(&o), vec![Delivery::Sending], "an earlier prompt of the same text is not this one");
        o.settle(&ended, 520_000);
        assert_eq!(deliveries(&o), vec![Delivery::Sending]);
        o.settle(&ended, 520_001);
        assert_eq!(deliveries(&o), vec![Delivery::Failed]);
    }

    #[test]
    fn only_a_queued_send_holds_the_outgoing() {
        let mut o = Outgoing::default();
        o.push("sent".into(), false, 0);
        assert!(!o.holds());
        o.push("next".into(), true, 0);
        assert!(o.holds());
        o.settle(&Transcript { turns: vec![turn("u1", 0, "sent", true)] }, 1_000);
        assert!(!o.holds(), "released once the agent is idle");
    }

    #[test]
    fn a_released_send_settles_when_its_prompt_arrives() {
        let mut o = Outgoing::default();
        o.push("next".into(), true, 0);
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", true)] }, 400_000);
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", true), turn("u2", 400_500, "next", false)] }, 401_000);
        assert!(o.list().is_empty());
    }

    #[test]
    fn withdrawing_drops_a_queued_send_unsent_and_hands_back_its_text() {
        let mut o = Outgoing::default();
        o.push("sent".into(), false, 0);
        o.push("a".into(), true, 0);
        o.push("b".into(), true, 0);
        assert_eq!(o.withdraw(id(&o, "sent")), None, "what already went can't be taken back");
        assert_eq!(o.withdraw(id(&o, "a")), Some("a".into()));
        assert_eq!(texts(&o), ["sent", "b"]);
        o.settle(&Transcript { turns: vec![turn("u1", 0, "sent", true)] }, 10);
        assert_eq!(o.released(), ["b"], "the next queued send moves up");
    }

    #[test]
    fn steering_sends_a_queued_send_mid_turn_and_starts_its_grace() {
        let mut o = Outgoing::default();
        o.push("a".into(), true, 0);
        o.push("b".into(), true, 0);
        let working = Transcript { turns: vec![turn("u1", 0, "long job", false)] };
        assert_eq!(o.steer(id(&o, "b"), 50_000), Some("b".into()));
        assert_eq!(o.steer(id(&o, "b"), 50_000), None, "steered once");
        assert_eq!(deliveries(&o), vec![Delivery::Queued, Delivery::Sending]);
        o.settle(&working, 170_000);
        assert_eq!(deliveries(&o), vec![Delivery::Queued, Delivery::Sending]);
        o.settle(&working, 170_001);
        assert_eq!(deliveries(&o), vec![Delivery::Queued, Delivery::Failed]);
    }

    #[test]
    fn retry_hands_back_a_failed_send() {
        let mut o = Outgoing::default();
        o.push("hello".into(), false, 0);
        assert_eq!(o.retry(0), None);
        o.settle(&Transcript::default(), 200_000);
        assert_eq!(o.retry(0), Some("hello".into()));
        assert!(o.list().is_empty());
        assert_eq!(o.retry(0), None);
    }

    #[test]
    fn working_with_an_empty_draft_offers_stop() {
        assert_eq!(action(true, true), Action::Stop);
    }

    #[test]
    fn working_with_a_draft_offers_queue() {
        assert_eq!(action(true, false), Action::Queue);
    }

    #[test]
    fn idle_offers_send() {
        assert_eq!(action(false, true), Action::Send);
        assert_eq!(action(false, false), Action::Send);
    }
}
