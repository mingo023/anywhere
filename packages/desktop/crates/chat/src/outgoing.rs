use crate::rows::at_work;
use crate::{End, Transcript};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Delivery {
    Sending,
    /// Sent into the turn underway, which takes it at its next step: it shows as sent, nothing to wait on.
    Steered,
    /// Held here while the agent works, unsent; it goes out when the turn ends, or when steered.
    Queued,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Pending {
    /// Stays with the send while others ahead of it settle, so a click finds it.
    pub id: u64,
    pub text: String,
    /// When it went to the agent; for a queued send, when it was queued. Its 120s grace runs from then, or from when
    /// the agent was next seen idle, as an agent may hold a prompt sent mid-turn until the turn ends.
    pub sent_at: i64,
    pub delivery: Delivery,
    /// The id it went to pocketd under, which a refusal names.
    request: Option<String>,
    /// A stop put it back in the queue after it went out; should the agent run it after all, its echo settles it.
    stopped: bool,
}

/// A prompt must reach the transcript within this long of being sent, or it reads as not delivered.
const GRACE_MS: i64 = 120_000;
/// How far the agent's clock may run behind ours when its prompt item is stamped.
const SKEW_MS: i64 = 10_000;
/// Enough claimed prompts to tell a repeated send from the one before it.
const CLAIMED: usize = 32;
/// How long pocketd's status may take to show a command at work; one it never shows did nothing to wait for.
const STATUS_LAG_MS: i64 = 3_000;
/// The longest a command reads as work, should its end never show.
const COMMAND_MS: i64 = 60_000;
/// The longest a stop reads as under way, should the turn never show it ended.
const STOP_MS: i64 = 10_000;

/// A command that went out. One that takes a turn echoes as its prompt; one that doesn't shows only in pocketd's status,
/// or the timeline moving.
#[derive(Clone, Copy, Debug)]
struct Ran {
    at: i64,
    /// pocketd's status showed the agent working since.
    worked: bool,
    /// pocketd's status showed the agent idle since. Until then, or `STATUS_LAG_MS`, working may be the turn before's.
    idled: bool,
}

impl Ran {
    fn new(at: i64) -> Self {
        Ran { at, worked: false, idled: false }
    }
}

#[derive(Default, Debug)]
pub struct Outgoing {
    sends: Vec<Pending>,
    /// Ids of prompts that already settled a send, so a repeat of the same text waits for its own.
    claimed: Vec<String>,
    /// Queued sends `settle` let go, for the caller to send.
    released: Vec<(u64, String)>,
    next_id: u64,
    /// The failed turn the user chose to carry on past.
    resumed: Option<String>,
    /// Whether the agent was on a turn when last settled.
    working: bool,
    /// When the agent was last seen to leave a turn.
    idle_since: i64,
    /// The agent refused a send, so the next would be refused too.
    refused: bool,
    /// A command went out and hasn't shown its end yet.
    command: Option<Ran>,
    /// When the user stopped the agent, until the turn shows it ended.
    stopping: Option<i64>,
    /// A send was queued while stopping: the user means to go on once the stop lands.
    after_stop: bool,
}

impl Outgoing {
    /// Queued, and not to be sent yet, if the agent was working. A command sent now isn't kept: it has no echo to wait for.
    /// Returns the id of a send now on its way.
    pub fn push(&mut self, text: String, working: bool, now_ms: i64) -> Option<u64> {
        if !working && command(&text) {
            self.command = Some(Ran::new(now_ms));
        }
        let delivery = if working { Delivery::Queued } else { Delivery::Sending };
        self.after_stop |= working && self.stopping.is_some();
        self.next_id += 1;
        self.sends.push(Pending { id: self.next_id, text, sent_at: now_ms, delivery, request: None, stopped: false });
        (!working).then_some(self.next_id)
    }

    /// Sends `text` into the turn underway; returns its id. A command shows too, until the turn it waits behind ends.
    pub fn steer_text(&mut self, text: String, now_ms: i64) -> u64 {
        if command(&text) {
            self.command = Some(Ran::new(now_ms));
        }
        self.next_id += 1;
        self.sends.push(Pending { id: self.next_id, text, sent_at: now_ms, delivery: Delivery::Steered, request: None, stopped: false });
        self.next_id
    }

    /// Notes the id send `id` went to pocketd under.
    pub fn sent_as(&mut self, id: u64, request: String) {
        if let Some(send) = self.sends.iter_mut().find(|s| s.id == id) {
            send.request = Some(request);
        }
    }

    /// Drops sends whose prompt reached the transcript and, while the agent is idle, fails those past their grace. Once
    /// the agent is idle with nothing on its way, and not paused, lets the oldest queued send go: one per turn, in order.
    /// `working` is pocketd's status, which decides only where the transcript can't; see `busy`.
    pub fn settle(&mut self, t: &Transcript, working: bool, now_ms: i64) {
        let idle = !on_turn(t, working);
        if idle && self.working {
            self.idle_since = now_ms;
        }
        self.working = !idle;
        if let Some(ran) = &mut self.command {
            let age = now_ms - ran.at;
            ran.idled |= !working;
            ran.worked |= working && (ran.idled || age > STATUS_LAG_MS);
            if (ran.worked && !working) || (!ran.worked && age > STATUS_LAG_MS) || age > COMMAND_MS {
                self.command = None;
            }
        }
        // A command steered in echoes nothing; it runs once the turn it came into ends.
        let since = self.idle_since;
        self.sends.retain(|s| !(s.delivery == Delivery::Steered && command(&s.text) && ((idle && s.sent_at <= since) || now_ms - s.sent_at > COMMAND_MS)));
        let (claimed, before) = (&mut self.claimed, self.sends.len());
        self.sends.retain(|send| {
            if send.delivery == Delivery::Queued && !send.stopped {
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
        // A command that never echoes goes once it reads as done.
        if self.command.is_none() {
            self.sends.retain(|s| !(s.delivery == Delivery::Sending && command(&s.text)));
        }
        if claimed.len() > CLAIMED {
            claimed.drain(..claimed.len() - CLAIMED);
        }
        if self.sends.len() < before {
            self.refused = false;
        }
        // The agent drops a steer it hadn't taken when stopped; it waits in the queue for the user rather than going on its own.
        if self.stopping.is_some() && idle {
            for send in self.sends.iter_mut().filter(|s| s.delivery == Delivery::Steered && !command(&s.text)) {
                (send.delivery, send.stopped) = (Delivery::Queued, true);
                self.after_stop = false;
            }
        }
        // A stop sent before its prompt echoed leaves pocketd idle at once, though the agent has yet to take the prompt.
        if !busy(t, self, working) || self.stopping.is_some_and(|at| now_ms - at > STOP_MS) {
            self.stopping = None;
        }
        if self.after_stop && self.stopping.is_none() {
            self.after_stop = false;
            self.resume(t);
        }
        if !idle {
            return;
        }
        for send in &mut self.sends {
            if in_flight(send.delivery) && !command(&send.text) && now_ms - send.sent_at.max(self.idle_since) > GRACE_MS {
                send.delivery = Delivery::Failed;
            }
        }
        if !self.paused(t) && !busy(t, self, working) {
            let id = self.sends.iter().find(|s| s.delivery == Delivery::Queued).map(|s| s.id);
            if let Some((id, text)) = id.and_then(|id| Some((id, self.release(id, now_ms, Delivery::Sending)?))) {
                self.released.push((id, text));
            }
        }
    }

    /// The queued sends `settle` let go since last asked, with their ids, to send to the agent.
    pub fn released(&mut self) -> Vec<(u64, String)> {
        std::mem::take(&mut self.released)
    }

    /// Lets a queued send go now, even mid-turn; returns its text to send.
    pub fn steer(&mut self, id: u64, now_ms: i64) -> Option<String> {
        self.release(id, now_ms, Delivery::Steered)
    }

    fn release(&mut self, id: u64, now_ms: i64, delivery: Delivery) -> Option<String> {
        let send = self.sends.iter_mut().find(|s| s.id == id && s.delivery == Delivery::Queued)?;
        if command(&send.text) {
            self.command = Some(Ran::new(now_ms));
        }
        (send.delivery, send.sent_at) = (delivery, now_ms);
        Some(send.text.clone())
    }

    /// Drops a queued send unsent, handing back its text.
    pub fn withdraw(&mut self, id: u64) -> Option<String> {
        self.take(id, Delivery::Queued)
    }

    /// Takes a failed send back for resending.
    pub fn retry(&mut self, id: u64) -> Option<String> {
        self.take(id, Delivery::Failed)
    }

    fn take(&mut self, id: u64, delivery: Delivery) -> Option<String> {
        let ix = self.sends.iter().position(|s| s.id == id && s.delivery == delivery)?;
        Some(self.sends.remove(ix).text)
    }

    /// Fails the send that went under `request` at once: the agent refused it. The queue waits until a send gets through or the user resumes.
    pub fn fail(&mut self, request: &str) {
        if let Some(send) = self.sends.iter_mut().find(|s| in_flight(s.delivery) && s.request.as_deref() == Some(request)) {
            send.delivery = Delivery::Failed;
            self.refused = true;
        }
    }

    /// Whether queued sends wait for the user because the last turn ended in a stop or an error, or is being stopped,
    /// or holds a steer the stop beat, or a send was refused.
    pub fn paused(&self, t: &Transcript) -> bool {
        let last = t.turns.last();
        let failed = last.is_some_and(|turn| matches!(turn.end, Some(End::Failed { .. })));
        let beaten = self.sends.iter().any(|s| s.stopped && s.delivery == Delivery::Queued);
        let resumed = last.is_some_and(|turn| self.resumed.as_ref() == Some(&turn.id));
        self.holds() && (self.refused || (self.stopping.is_some() || failed || beaten) && !resumed)
    }

    /// Lets the queue go on past the failed turn or refusal it waits behind.
    pub fn resume(&mut self, t: &Transcript) {
        self.resumed = t.turns.last().map(|turn| turn.id.clone());
        self.refused = false;
    }

    /// Takes out every send that won't reach the agent unless sent again, queued or failed, oldest first.
    pub fn drain(&mut self) -> Vec<String> {
        let (sending, rest): (Vec<_>, Vec<_>) = std::mem::take(&mut self.sends).into_iter().partition(|s| in_flight(s.delivery));
        self.sends = sending;
        rest.into_iter().map(|s| s.text).collect()
    }

    /// Whether a send waits its turn.
    pub fn holds(&self) -> bool {
        self.sends.iter().any(|s| s.delivery == Delivery::Queued)
    }

    /// The user stopped the agent: the queue pauses now rather than once the turn shows it.
    pub fn stop(&mut self, now_ms: i64) {
        self.stopping = Some(now_ms);
    }

    pub fn stop_refused(&mut self) {
        self.stopping = None;
    }

    /// Whether a stop is on its way and the turn doesn't show it yet.
    pub fn stopping(&self) -> bool {
        self.stopping.is_some()
    }

    /// The agent's timeline moved, so a command sent before it shows what it did, if anything.
    pub fn moved(&mut self) {
        self.command = None;
    }

    pub fn list(&self) -> &[Pending] {
        &self.sends
    }

    /// The sends the transcript shows, with their index in `list`; a queued one waits in the composer's tray instead.
    pub fn echoed(&self) -> impl Iterator<Item = (usize, &Pending)> {
        self.sends.iter().enumerate().filter(|(_, p)| p.delivery != Delivery::Queued)
    }
}

/// Whether the agent is on a turn as the chat sees it: a send steers into it and Stop shows, and the queue holds.
/// A send on its way counts, as the agent takes it as a turn. `working` is pocketd's status: the transcript shows a stop
/// at once where status trails it by up to a poll, so status decides only where the transcript can't, while it has no
/// turn (not fetched yet, or dropped as pocketd restarted). A command just sent counts until it shows its end; see `settle`.
/// Sub-agents left in the background don't: the agent takes a send as a new turn. The background tray stops them instead.
pub fn busy(t: &Transcript, out: &Outgoing, working: bool) -> bool {
    out.sends.iter().any(|p| in_flight(p.delivery)) || out.command.is_some() || on_turn(t, working)
}

fn in_flight(d: Delivery) -> bool {
    matches!(d, Delivery::Sending | Delivery::Steered)
}

fn on_turn(t: &Transcript, working: bool) -> bool {
    if t.turns.is_empty() { working } else { at_work(t) }
}

/// A slash command, which Claude may run without echoing a prompt; a prompt may start with a path instead.
fn command(text: &str) -> bool {
    crate::slash(text).is_some()
}

/// Agents may tidy a prompt's whitespace before echoing it back, and Claude echoes a bash command as `! cmd`. Text the
/// user left in Claude's input runs on into the prompt typed after it.
fn same_text(echo: &str, sent: &str) -> bool {
    let (echo, sent) = (tidy(echo), tidy(sent));
    echo.ends_with(&sent)
}

fn tidy(text: &str) -> String {
    let text = text.split_whitespace().collect::<Vec<_>>().join(" ");
    match text.strip_prefix("! ") {
        Some(command) => format!("!{command}"),
        None => text,
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Send,
    /// Sends into the turn underway; the agent reads it at its next step.
    Steer,
    /// A stop is on its way, so the send waits for the turn to end.
    Queue,
    Stop,
    /// A stop is on its way; nothing to press until the turn ends.
    Stopping,
}

/// Stop while working with an empty draft, Steer while working with one, else Send. Enter never stops, and once
/// `stopping`, nothing does again: a draft then queues.
pub fn action(working: bool, stopping: bool, empty: bool) -> Action {
    match (working, empty) {
        (true, true) if stopping => Action::Stopping,
        (true, true) => Action::Stop,
        (true, false) if stopping => Action::Queue,
        (true, false) => Action::Steer,
        (false, _) => Action::Send,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Block, End, Message, Turn};

    fn turn(id: &str, at: i64, text: &str, ended: bool) -> Turn {
        let end = ended.then_some(End::Done { duration_ms: 1, usage: None });
        Turn { id: id.into(), prompt: Some(Message { id: id.into(), at, text: text.into() }), blocks: vec![], end }
    }

    fn released(o: &mut Outgoing) -> Vec<String> {
        o.released().into_iter().map(|(_, text)| text).collect()
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
        o.settle(&Transcript { turns: vec![turn("u1", 1_000, "fix the build", true)] }, false, 50_500);
        assert_eq!(o.list().len(), 1, "a prompt from before the send is not this one");
        o.settle(&Transcript { turns: vec![turn("u1", 1_000, "fix the build", true), turn("u2", 49_000, "fix the build", false)] }, false, 51_000);
        assert!(o.list().is_empty());
    }

    #[test]
    fn a_repeated_send_waits_for_its_own_prompt() {
        let mut o = Outgoing::default();
        o.push("continue".into(), false, 10_000);
        let first = Transcript { turns: vec![turn("u1", 10_100, "continue", true)] };
        o.settle(&first, false, 10_200);
        o.push("continue".into(), false, 11_000);
        o.settle(&first, false, 11_100);
        assert_eq!(o.list().len(), 1);
        o.settle(&Transcript { turns: vec![turn("u1", 10_100, "continue", true), turn("u2", 11_200, "continue", false)] }, false, 11_300);
        assert!(o.list().is_empty());
    }

    #[test]
    fn a_send_not_seen_in_two_minutes_is_not_delivered() {
        let mut o = Outgoing::default();
        o.push("hello".into(), false, 0);
        let empty = Transcript::default();
        o.settle(&empty, false, 120_000);
        assert_eq!(deliveries(&o), vec![Delivery::Sending]);
        o.settle(&empty, false, 120_001);
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
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", false)] }, false, 300_000);
        assert_eq!(deliveries(&o), vec![Delivery::Queued], "held past two minutes, never failed");
        assert!(released(&mut o).is_empty());
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", true)] }, false, 400_000);
        assert_eq!(released(&mut o), ["next"]);
        assert_eq!(deliveries(&o), vec![Delivery::Sending]);
        assert!(released(&mut o).is_empty(), "handed over once");
    }

    #[test]
    fn queued_sends_go_one_per_turn_in_the_order_queued() {
        let mut o = Outgoing::default();
        o.push("first".into(), true, 0);
        o.push("second".into(), true, 1);
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", true)] };
        o.settle(&ended, false, 1_000);
        o.settle(&ended, false, 1_100);
        assert_eq!(released(&mut o), ["first"], "the second waits while the first is on its way");
        let first = Transcript { turns: vec![turn("u1", 0, "long job", true), turn("u2", 1_050, "first", false)] };
        o.settle(&first, false, 1_200);
        assert!(released(&mut o).is_empty(), "the agent works on the first");
        assert_eq!(texts(&o), ["second"]);
        let done = Transcript { turns: vec![turn("u1", 0, "long job", true), turn("u2", 1_050, "first", true)] };
        o.settle(&done, false, 5_000);
        assert_eq!(released(&mut o), ["second"]);
    }

    #[test]
    fn a_send_settles_when_text_left_in_the_terminal_ran_into_its_prompt() {
        let mut o = Outgoing::default();
        o.push("reply with just the word kappa".into(), false, 0);
        o.push("next".into(), true, 0);
        o.settle(&Transcript { turns: vec![turn("u1", 100, "draftreply with just the word kappa", true)] }, false, 1_000);
        assert_eq!(released(&mut o), vec!["next"]);
        assert_eq!(texts(&o), vec!["next"]);
    }

    #[test]
    fn a_bash_command_sent_from_the_chat_settles_by_its_echo() {
        let mut o = Outgoing::default();
        o.push("!ls".into(), false, 0);
        o.settle(&Transcript { turns: vec![turn("u1", 100, "! ls", true)] }, false, 1_000);
        assert!(o.list().is_empty());
    }

    #[test]
    fn a_released_send_gets_two_minutes_from_its_release() {
        let mut o = Outgoing::default();
        o.push("next".into(), true, 0);
        let ended = Transcript { turns: vec![turn("u1", 0, "next", true)] };
        o.settle(&Transcript { turns: vec![turn("u1", 0, "next", false)] }, false, 1_000);
        o.settle(&ended, false, 400_000);
        assert_eq!(deliveries(&o), vec![Delivery::Sending], "an earlier prompt of the same text is not this one");
        o.settle(&ended, false, 520_000);
        assert_eq!(deliveries(&o), vec![Delivery::Sending]);
        o.settle(&ended, false, 520_001);
        assert_eq!(deliveries(&o), vec![Delivery::Failed]);
    }

    #[test]
    fn only_a_queued_send_holds_the_outgoing() {
        let mut o = Outgoing::default();
        o.push("sent".into(), false, 0);
        assert!(!o.holds());
        o.push("next".into(), true, 0);
        assert!(o.holds());
        o.settle(&Transcript { turns: vec![turn("u1", 0, "sent", true)] }, false, 1_000);
        assert!(!o.holds(), "released once the agent is idle");
    }

    #[test]
    fn a_released_send_settles_when_its_prompt_arrives() {
        let mut o = Outgoing::default();
        o.push("next".into(), true, 0);
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", true)] }, false, 400_000);
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", true), turn("u2", 400_500, "next", false)] }, false, 401_000);
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
        o.settle(&Transcript { turns: vec![turn("u1", 0, "sent", true)] }, false, 10);
        assert_eq!(released(&mut o), ["b"], "the next queued send moves up");
    }

    #[test]
    fn steering_sends_a_queued_send_mid_turn() {
        let mut o = Outgoing::default();
        o.push("a".into(), true, 0);
        o.push("b".into(), true, 0);
        assert_eq!(o.steer(id(&o, "b"), 50_000), Some("b".into()));
        assert_eq!(o.steer(id(&o, "b"), 50_000), None, "steered once");
        assert_eq!(deliveries(&o), vec![Delivery::Queued, Delivery::Steered]);
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", false)] }, false, 50_100);
        assert_eq!(deliveries(&o), vec![Delivery::Queued, Delivery::Steered]);
    }

    #[test]
    fn retry_hands_back_a_failed_send() {
        let mut o = Outgoing::default();
        o.push("hello".into(), false, 0);
        let hello = id(&o, "hello");
        assert_eq!(o.retry(hello), None);
        o.settle(&Transcript::default(), false, 200_000);
        assert_eq!(o.retry(hello), Some("hello".into()));
        assert!(o.list().is_empty());
        assert_eq!(o.retry(hello), None);
    }

    fn compaction(id: &str) -> Turn {
        Turn { id: id.into(), prompt: None, blocks: vec![Block::Compacted { id: id.into(), auto: false }], end: None }
    }

    fn stopped(id: &str, at: i64, text: &str) -> Turn {
        Turn { end: Some(End::Failed { error: "interrupted".into(), duration_ms: 0 }), ..turn(id, at, text, false) }
    }

    #[test]
    fn a_turn_holding_only_a_compaction_lets_the_queue_go() {
        let mut o = Outgoing::default();
        o.push("next".into(), true, 0);
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", true), compaction("c1")] }, false, 1_000);
        assert_eq!(released(&mut o), ["next"]);
    }

    #[test]
    fn stopping_holds_the_queue_until_resumed() {
        let mut o = Outgoing::default();
        o.push("next".into(), true, 0);
        let t = Transcript { turns: vec![stopped("u1", 0, "long job")] };
        o.settle(&t, false, 1_000);
        assert!(released(&mut o).is_empty(), "a stop holds what was queued behind it");
        assert!(o.paused(&t));
        o.settle(&t, false, 300_000);
        assert_eq!(deliveries(&o), vec![Delivery::Queued], "held, never failed");
    }

    #[test]
    fn resuming_lets_the_next_queued_send_go() {
        let mut o = Outgoing::default();
        o.push("a".into(), true, 0);
        o.push("b".into(), true, 0);
        let t = Transcript { turns: vec![stopped("u1", 0, "long job")] };
        o.settle(&t, false, 1_000);
        o.resume(&t);
        assert!(!o.paused(&t));
        o.settle(&t, false, 2_000);
        assert_eq!(released(&mut o), ["a"]);
        assert_eq!(deliveries(&o), vec![Delivery::Sending, Delivery::Queued]);
    }

    #[test]
    fn a_new_failure_after_resuming_pauses_again() {
        let mut o = Outgoing::default();
        o.push("a".into(), true, 0);
        o.push("b".into(), true, 0);
        let first = Transcript { turns: vec![stopped("u1", 0, "long job")] };
        o.settle(&first, false, 1_000);
        o.resume(&first);
        o.settle(&first, false, 2_000);
        assert_eq!(released(&mut o), ["a"]);
        let second = Transcript { turns: vec![stopped("u1", 0, "long job"), stopped("u2", 2_100, "a")] };
        o.settle(&second, false, 3_000);
        assert!(o.paused(&second));
        assert!(released(&mut o).is_empty());
        assert_eq!(texts(&o), ["b"]);
    }

    #[test]
    fn a_slash_command_never_holds_the_queue() {
        let mut o = Outgoing::default();
        o.push("/compact".into(), true, 0);
        o.push("next".into(), true, 0);
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", true)] };
        o.settle(&ended, false, 1_000);
        assert_eq!(released(&mut o), ["/compact"]);
        assert_eq!(texts(&o), ["/compact", "next"]);
        o.moved();
        o.settle(&ended, false, 1_100);
        assert_eq!(released(&mut o), ["next"]);
    }

    #[test]
    fn a_slash_command_never_turns_failed() {
        let mut o = Outgoing::default();
        o.push(" /clear".into(), false, 0);
        o.push("/model opus".into(), true, 0);
        assert_eq!(o.steer(id(&o, "/model opus"), 0), Some("/model opus".into()));
        o.settle(&Transcript::default(), false, 200_000);
        assert!(o.list().is_empty());
    }

    fn sent(o: &mut Outgoing, text: &str, request: &str) {
        let id = o.push(text.into(), false, 0).unwrap();
        o.sent_as(id, request.into());
    }

    #[test]
    fn a_refused_send_fails_at_once_and_releases_nothing() {
        let mut o = Outgoing::default();
        sent(&mut o, "a", "prompt-1");
        o.push("a".into(), true, 0);
        o.fail("prompt-1");
        assert_eq!(deliveries(&o), vec![Delivery::Failed, Delivery::Queued], "a queued send of the same text never went");
        o.fail("prompt-1");
        assert_eq!(deliveries(&o), vec![Delivery::Failed, Delivery::Queued]);
        assert!(released(&mut o).is_empty());
    }

    #[test]
    fn a_refusal_holds_the_queue_until_resumed() {
        let mut o = Outgoing::default();
        sent(&mut o, "a", "prompt-1");
        o.push("b".into(), true, 0);
        o.push("c".into(), true, 0);
        o.fail("prompt-1");
        let idle = Transcript::default();
        o.settle(&idle, false, 1_000);
        assert!(released(&mut o).is_empty(), "the next send would be refused too");
        assert!(o.paused(&idle));
        o.resume(&idle);
        o.settle(&idle, false, 2_000);
        assert_eq!(released(&mut o), ["b"]);
    }

    #[test]
    fn a_send_getting_through_after_a_refusal_lets_the_queue_go() {
        let mut o = Outgoing::default();
        sent(&mut o, "a", "prompt-1");
        o.push("b".into(), true, 0);
        o.fail("prompt-1");
        let retried = o.retry(id(&o, "a")).unwrap();
        sent(&mut o, &retried, "prompt-2");
        let done = Transcript { turns: vec![turn("u1", 100, "a", true)] };
        o.settle(&done, false, 1_000);
        assert!(!o.paused(&done));
        assert_eq!(released(&mut o), ["b"]);
    }

    #[test]
    fn a_refusal_fails_the_send_it_names() {
        let mut o = Outgoing::default();
        sent(&mut o, "continue", "prompt-1");
        sent(&mut o, "continue", "prompt-2");
        o.fail("prompt-2");
        assert_eq!(deliveries(&o), vec![Delivery::Sending, Delivery::Failed], "the first went through; the second was refused");
    }

    #[test]
    fn a_prompt_starting_with_a_path_is_tracked_like_any_other() {
        let mut o = Outgoing::default();
        o.push("/Users/me/app/src/main.rs panics on start".into(), false, 0);
        assert_eq!(texts(&o), ["/Users/me/app/src/main.rs panics on start"]);
        o.push("./run.sh fails".into(), false, 0);
        assert_eq!(o.list().len(), 2);
    }

    #[test]
    fn draining_returns_queued_and_failed_text_in_order() {
        let mut o = Outgoing::default();
        o.push("lost".into(), false, 0);
        o.settle(&Transcript::default(), false, 200_000);
        o.push("sending".into(), false, 200_000);
        o.push("next".into(), true, 200_000);
        assert_eq!(o.drain(), ["lost", "next"]);
        assert_eq!(texts(&o), ["sending"]);
    }

    #[test]
    fn retrying_one_failed_send_leaves_the_others_alone() {
        let mut o = Outgoing::default();
        o.push("a".into(), false, 0);
        o.push("b".into(), false, 0);
        o.settle(&Transcript::default(), false, 200_000);
        assert_eq!(o.retry(id(&o, "b")), Some("b".into()));
        assert_eq!(texts(&o), ["a"]);
        assert_eq!(deliveries(&o), vec![Delivery::Failed]);
    }

    #[test]
    fn a_steered_send_waits_out_a_long_turn_without_failing() {
        let mut o = Outgoing::default();
        o.push("b".into(), true, 0);
        o.steer(id(&o, "b"), 50_000);
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", false)] }, false, 600_000);
        assert_eq!(deliveries(&o), vec![Delivery::Steered], "Claude holds it until its turn ends");
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", true)] };
        o.settle(&ended, false, 601_000);
        o.settle(&ended, false, 721_000);
        assert_eq!(deliveries(&o), vec![Delivery::Steered], "two minutes from when the agent was seen idle");
        o.settle(&ended, false, 721_001);
        assert_eq!(deliveries(&o), vec![Delivery::Failed], "an unsent prompt still fails after 120s idle");
    }

    #[test]
    fn a_queued_send_waits_while_status_says_working_and_the_transcript_has_no_turns() {
        let mut o = Outgoing::default();
        o.push("next".into(), true, 0);
        let refetching = Transcript::default();
        o.settle(&refetching, true, 1_000);
        assert!(released(&mut o).is_empty(), "pocketd restarted mid-turn and its timeline is still on its way");
        assert!(busy(&refetching, &o, true));
        o.settle(&refetching, false, 2_000);
        assert_eq!(released(&mut o), ["next"]);
    }

    #[test]
    fn a_command_just_sent_reads_as_work_before_status_shows_it() {
        let mut o = Outgoing::default();
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", true)] };
        o.push("/compact".into(), false, 0);
        assert!(busy(&ended, &o, false), "status trails it by a poll");
        o.settle(&ended, true, 1_000);
        assert!(busy(&ended, &o, true));
        o.settle(&ended, true, 300_000);
        assert!(o.list().is_empty(), "never Sending, never Failed");
        o.push("/compact".into(), false, 300_000);
        o.moved();
        o.settle(&ended, true, 300_001);
        assert!(!busy(&ended, &o, true), "once the timeline moves the transcript decides again");
    }

    #[test]
    fn a_queued_send_waits_behind_a_command_until_the_agent_shows_what_it_did() {
        let mut o = Outgoing::default();
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", true)] };
        o.push("/compact".into(), false, 0);
        o.push("next".into(), true, 0);
        o.settle(&ended, true, 1_000);
        assert!(released(&mut o).is_empty());
        o.moved();
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", true), compaction("c1")] }, true, 5_000);
        assert_eq!(released(&mut o), ["next"], "the compaction shows; status trailing it holds nothing");
    }

    #[test]
    fn a_command_that_shows_nothing_holds_the_queue_only_until_status_could_have_shown_it() {
        let mut o = Outgoing::default();
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", true)] };
        o.push("/model opus".into(), false, 0);
        o.push("next".into(), true, 0);
        o.settle(&ended, false, 3_000);
        assert!(released(&mut o).is_empty());
        o.settle(&ended, false, 3_001);
        assert_eq!(released(&mut o), ["next"]);
    }

    fn open(at: i64) -> Transcript {
        Transcript { turns: vec![turn("u1", at, "long job", false)] }
    }

    #[test]
    fn a_stop_pauses_the_queue_before_the_turn_shows_it_ended() {
        let mut o = Outgoing::default();
        o.push("next".into(), true, 0);
        o.settle(&open(0), true, 1_000);
        assert!(!o.stopping() && !o.paused(&open(0)));
        o.stop(2_000);
        assert!(o.stopping() && o.paused(&open(0)));
        o.settle(&open(0), true, 3_500);
        assert!(o.stopping() && o.paused(&open(0)), "pocketd's interrupted result is on its way");
        let ended = Transcript { turns: vec![stopped("u1", 0, "long job")] };
        o.settle(&ended, true, 4_000);
        assert!(!o.stopping(), "the turn ended");
        assert!(o.paused(&ended) && released(&mut o).is_empty(), "the stop holds the queue as before");
    }

    #[test]
    fn a_stop_the_turn_never_shows_lets_go_after_ten_seconds() {
        let mut o = Outgoing::default();
        o.push("next".into(), true, 0);
        o.stop(1_000);
        o.settle(&open(0), true, 11_000);
        assert!(o.stopping());
        o.settle(&open(0), true, 11_001);
        assert!(!o.stopping() && !o.paused(&open(0)));
    }

    #[test]
    fn a_stop_before_the_sent_prompt_echoes_reads_stopping_until_it_does() {
        let mut o = Outgoing::default();
        let id = o.push("hello".into(), false, 0).unwrap();
        o.sent_as(id, "r1".into());
        o.stop(100);
        o.settle(&Transcript::default(), false, 200);
        assert!(busy(&Transcript::default(), &o, false) && o.stopping(), "pocketd reads idle as soon as it sent the Esc");
        let t = Transcript { turns: vec![stopped("u1", 150, "hello")] };
        o.settle(&t, false, 1_000);
        assert!(!busy(&t, &o, false) && !o.stopping());
    }

    #[test]
    fn a_steer_the_stop_beat_goes_back_to_the_queue_paused() {
        let mut o = Outgoing::default();
        o.steer_text("refactor the parser".into(), 1_000);
        o.stop(2_000);
        o.settle(&open(0), true, 2_500);
        assert!(o.stopping() && busy(&open(0), &o, true));
        let ended = Transcript { turns: vec![stopped("u1", 0, "long job")] };
        o.settle(&ended, false, 3_000);
        assert_eq!(deliveries(&o), [Delivery::Queued], "the agent never took it");
        assert!(!o.stopping() && !busy(&ended, &o, false), "the composer is free as soon as the stop lands");
        assert!(o.paused(&ended) && released(&mut o).is_empty(), "it waits for the user");
        o.settle(&ended, false, 300_000);
        assert_eq!(deliveries(&o), [Delivery::Queued], "never reads as not delivered");
    }

    #[test]
    fn a_steer_the_stop_beat_waits_for_the_user_though_the_turn_ended_done() {
        let mut o = Outgoing::default();
        o.steer_text("refactor the parser".into(), 1_000);
        o.stop(2_000);
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", true)] };
        o.settle(&ended, false, 3_000);
        assert_eq!(deliveries(&o), [Delivery::Queued]);
        assert!(o.paused(&ended) && released(&mut o).is_empty(), "the user stopped; it waits for Resume");
        o.resume(&ended);
        o.settle(&ended, false, 4_000);
        assert_eq!(released(&mut o).len(), 1);
    }

    #[test]
    fn a_steer_the_agent_took_before_the_stop_stays_in_the_transcript() {
        let mut o = Outgoing::default();
        o.steer_text("refactor the parser".into(), 1_000);
        o.stop(2_000);
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", false), stopped("u2", 1_500, "refactor the parser")] };
        o.settle(&ended, false, 3_000);
        assert!(o.list().is_empty());
    }

    #[test]
    fn a_stopped_steer_the_agent_runs_after_all_settles_on_its_echo_and_is_never_sent_again() {
        let mut o = Outgoing::default();
        o.steer_text("refactor the parser".into(), 1_000);
        o.stop(2_000);
        let ended = Transcript { turns: vec![stopped("u1", 0, "long job")] };
        o.settle(&ended, false, 3_000);
        assert_eq!(deliveries(&o), [Delivery::Queued]);
        let ran = Transcript { turns: vec![stopped("u1", 0, "long job"), turn("u2", 3_500, "refactor the parser", false)] };
        o.settle(&ran, true, 4_000);
        assert!(o.list().is_empty(), "its echo settles it");
        let done = Transcript { turns: vec![stopped("u1", 0, "long job"), turn("u2", 3_500, "refactor the parser", true)] };
        o.settle(&done, false, 5_000);
        assert!(released(&mut o).is_empty());
    }

    #[test]
    fn a_queued_send_never_sent_ignores_an_echo_of_the_same_text() {
        let mut o = Outgoing::default();
        o.push("refactor the parser".into(), true, 1_000);
        let typed = Transcript { turns: vec![turn("u1", 0, "long job", true), turn("u2", 1_500, "refactor the parser", false)] };
        o.settle(&typed, true, 2_000);
        assert_eq!(deliveries(&o), [Delivery::Queued], "the user typed it in the terminal; this one still waits");
    }

    #[test]
    fn a_refused_stop_lets_go_at_once() {
        let mut o = Outgoing::default();
        o.push("next".into(), true, 0);
        o.stop(1_000);
        o.stop_refused();
        assert!(!o.stopping() && !o.paused(&open(0)));
    }

    #[test]
    fn resuming_while_stopping_lets_the_queue_go_once_the_turn_ends() {
        let mut o = Outgoing::default();
        o.push("next".into(), true, 0);
        o.stop(1_000);
        o.resume(&open(0));
        assert!(o.stopping() && !o.paused(&open(0)));
        o.settle(&Transcript { turns: vec![stopped("u1", 0, "long job")] }, false, 2_000);
        assert_eq!(released(&mut o), ["next"]);
    }

    #[test]
    fn a_send_queued_behind_a_compact_waits_while_status_has_yet_to_show_it() {
        let mut o = Outgoing::default();
        o.push("/compact".into(), true, 0);
        o.push("next".into(), true, 0);
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", true)] };
        o.settle(&ended, false, 1_000);
        assert_eq!(released(&mut o), ["/compact"]);
        o.settle(&ended, false, 1_500);
        assert!(released(&mut o).is_empty(), "pocketd's status trails the compaction by a poll");
        o.settle(&ended, true, 2_500);
        o.settle(&ended, true, 12_000);
        assert!(released(&mut o).is_empty(), "the compaction runs");
        o.settle(&ended, false, 13_000);
        assert_eq!(released(&mut o), ["next"], "idle after working: the compaction is over");
    }

    #[test]
    fn a_send_queued_behind_a_compact_waits_while_status_trails_the_turn_before_it() {
        let mut o = Outgoing::default();
        o.push("/compact".into(), true, 0);
        o.push("next".into(), true, 0);
        o.settle(&open(0), true, 500);
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", true)] };
        o.settle(&ended, true, 1_000);
        assert_eq!(released(&mut o), ["/compact"]);
        o.settle(&ended, true, 1_500);
        o.settle(&ended, false, 2_000);
        assert!(released(&mut o).is_empty(), "the working status was the last turn's, not the compaction's");
        o.settle(&ended, true, 2_500);
        o.settle(&ended, false, 9_000);
        assert_eq!(released(&mut o), ["next"]);
    }

    #[test]
    fn a_command_holds_the_queue_a_minute_at_most() {
        let mut o = Outgoing::default();
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", true)] };
        o.push("/compact".into(), false, 0);
        o.push("next".into(), true, 0);
        o.settle(&ended, true, 60_000);
        assert!(released(&mut o).is_empty());
        o.settle(&ended, true, 60_001);
        assert_eq!(released(&mut o), ["next"], "status stuck on working holds nothing forever");
    }

    #[test]
    fn a_steered_command_reads_as_work_too() {
        let mut o = Outgoing::default();
        let ended = Transcript { turns: vec![turn("u1", 0, "long job", true)] };
        o.push("/compact".into(), true, 0);
        o.steer(id(&o, "/compact"), 0);
        assert_eq!(deliveries(&o), [Delivery::Steered]);
        assert!(busy(&ended, &o, true));
    }

    #[test]
    fn a_command_shows_as_sending_until_its_turn_echoes_it() {
        let mut o = Outgoing::default();
        o.push("/goal tests pass".into(), false, 0);
        o.settle(&Transcript::default(), false, 1_000);
        assert_eq!(deliveries(&o), [Delivery::Sending], "a chat just made shows it while Claude starts");
        o.moved();
        o.settle(&Transcript { turns: vec![turn("u1", 2_000, "/goal tests pass", false)] }, true, 2_500);
        assert!(o.list().is_empty());
    }

    #[test]
    fn a_steer_shows_as_sent_and_settles_by_its_echo() {
        let mut o = Outgoing::default();
        o.steer_text("also check the docs".into(), 1_000);
        assert_eq!(deliveries(&o), vec![Delivery::Steered]);
        o.settle(&open(0), true, 2_000);
        assert_eq!(o.echoed().count(), 1);
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", true), turn("u2", 1_500, "also check the docs", false)] }, true, 3_000);
        assert!(o.list().is_empty());
    }

    #[test]
    fn a_command_steered_mid_turn_shows_until_the_turn_ends() {
        let mut o = Outgoing::default();
        o.steer_text("/compact".into(), 1_000);
        o.settle(&open(0), true, 5_000);
        assert_eq!(texts(&o), ["/compact"]);
        o.settle(&Transcript { turns: vec![turn("u1", 0, "long job", true)] }, false, 6_000);
        assert!(o.list().is_empty(), "it runs now, echoing nothing");
    }

    #[test]
    fn a_send_queued_while_stopping_goes_once_the_stop_lands() {
        let mut o = Outgoing::default();
        o.stop(1_000);
        o.push("next".into(), true, 1_500);
        o.settle(&open(0), true, 2_000);
        assert!(released(&mut o).is_empty());
        o.settle(&Transcript { turns: vec![stopped("u1", 0, "long job")] }, false, 3_000);
        assert_eq!(released(&mut o), ["next"], "no Resume to press");
    }

    #[test]
    fn a_sub_agent_in_the_background_leaves_the_chat_free_and_holds_no_queue() {
        use crate::{Block, SubAgent, ToolCall, ToolKind, ToolStatus};
        let agent = Block::Tool(ToolCall { id: "a".into(), name: String::new(), kind: ToolKind::Agent(SubAgent::default()), status: ToolStatus::Running, output: None, duration_ms: None, at: 0 });
        let t = Transcript { turns: vec![Turn { blocks: vec![agent], ..turn("u1", 0, "audit", true) }] };
        let mut o = Outgoing::default();
        assert!(!busy(&t, &o, false), "a send starts a turn rather than steering into one");
        o.push("next".into(), true, 0);
        o.settle(&t, false, 1_000);
        assert_eq!(released(&mut o), ["next"]);
    }

    #[test]
    fn working_with_an_empty_draft_offers_stop() {
        assert_eq!(action(true, false, true), Action::Stop);
    }

    #[test]
    fn working_with_a_draft_offers_steer() {
        assert_eq!(action(true, false, false), Action::Steer);
    }

    #[test]
    fn idle_offers_send() {
        assert_eq!(action(false, false, true), Action::Send);
        assert_eq!(action(false, false, false), Action::Send);
    }

    #[test]
    fn a_turn_being_stopped_offers_no_second_stop_but_still_queues() {
        assert_eq!(action(true, true, true), Action::Stopping);
        assert_eq!(action(true, true, false), Action::Queue);
        assert_eq!(action(false, true, true), Action::Send, "the turn ended before the stop cleared");
    }
}
