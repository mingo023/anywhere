use serde::Deserialize;
use std::collections::HashMap;

/// One session a chat delegated to, as pocketd lists it.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Delegation {
    pub task_id: String,
    pub parent_id: String,
    pub child_id: String,
    pub title: String,
    /// The provider the child runs on.
    pub agent: String,
    /// `queued`, `starting`, `working`, `completed`, `failed` or `cancelled`.
    pub status: String,
    /// What the child asks the user, while it waits on the answer.
    pub question: String,
    /// Unix ms.
    pub created_at: i64,
    /// Unix ms; 0 until it settles.
    pub settled_at: i64,
}

/// Where a delegation stands, as its row shows it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Starting,
    Working,
    /// Waiting on the user's answer.
    Asking,
    Done,
    Failed,
    Stopped,
}

impl Phase {
    pub fn settled(self) -> bool {
        matches!(self, Phase::Done | Phase::Failed | Phase::Stopped)
    }
}

impl Delegation {
    pub fn phase(&self) -> Phase {
        match self.status.as_str() {
            "queued" | "starting" => Phase::Starting,
            "completed" => Phase::Done,
            "failed" => Phase::Failed,
            "cancelled" => Phase::Stopped,
            _ if !self.question.is_empty() => Phase::Asking,
            _ => Phase::Working,
        }
    }

    /// Ms from creation to `now`, or to when it settled.
    pub fn elapsed_ms(&self, now: i64) -> i64 {
        let end = if self.phase().settled() && self.settled_at > 0 { self.settled_at } else { now };
        (end - self.created_at).max(0)
    }
}

/// How many of a chat's delegations are in each phase.
#[derive(Default, Clone, Copy, Debug, PartialEq, Eq)]
pub struct Counts {
    pub total: usize,
    pub starting: usize,
    pub working: usize,
    pub asking: usize,
    pub done: usize,
    pub failed: usize,
    pub stopped: usize,
}

/// What a collapsed panel says first.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Headline {
    Asking(usize),
    Working(usize),
    /// All settled: how many ended well and how many failed.
    Settled { done: usize, failed: usize },
}

impl Counts {
    pub fn headline(&self) -> Headline {
        if self.asking > 0 {
            Headline::Asking(self.asking)
        } else if self.starting + self.working > 0 {
            Headline::Working(self.starting + self.working)
        } else {
            Headline::Settled { done: self.done + self.stopped, failed: self.failed }
        }
    }
}

/// Each chat's delegations, by the chat's agent id.
#[derive(Default, Debug)]
pub struct Delegations {
    by_parent: HashMap<String, Vec<Delegation>>,
}

impl Delegations {
    pub fn of(&self, parent: &str) -> &[Delegation] {
        self.by_parent.get(parent).map_or(&[], Vec::as_slice)
    }

    /// Takes `parent`'s whole list, as `agent.delegations` answers it.
    pub fn replace(&mut self, parent: &str, mut items: Vec<Delegation>) {
        items.sort_by_key(|d| d.created_at);
        if items.is_empty() {
            self.by_parent.remove(parent);
        } else {
            self.by_parent.insert(parent.to_string(), items);
        }
    }

    /// Takes one change, as `delegation.update` pushes it: a new task joins at its creation time.
    pub fn update(&mut self, parent: &str, d: Delegation) {
        let list = self.by_parent.entry(parent.to_string()).or_default();
        match list.iter_mut().find(|x| x.task_id == d.task_id) {
            Some(x) => *x = d,
            None => {
                let at = list.partition_point(|x| x.created_at <= d.created_at);
                list.insert(at, d);
            }
        }
    }

    /// Whether `parent` delegated and every task has ended.
    pub fn all_settled(&self, parent: &str) -> bool {
        let list = self.of(parent);
        !list.is_empty() && list.iter().all(|d| d.phase().settled())
    }

    pub fn counts(&self, parent: &str) -> Counts {
        let mut c = Counts::default();
        for d in self.of(parent) {
            c.total += 1;
            match d.phase() {
                Phase::Starting => c.starting += 1,
                Phase::Working => c.working += 1,
                Phase::Asking => c.asking += 1,
                Phase::Done => c.done += 1,
                Phase::Failed => c.failed += 1,
                Phase::Stopped => c.stopped += 1,
            }
        }
        c
    }

    /// Drops the ended tasks from `parent`'s list.
    pub fn clear_settled(&mut self, parent: &str) {
        if let Some(list) = self.by_parent.get_mut(parent) {
            list.retain(|d| !d.phase().settled());
            if list.is_empty() {
                self.by_parent.remove(parent);
            }
        }
    }

    /// Drops what host `source` held, whose chat ids start with `source/`.
    pub fn forget(&mut self, source: &str) {
        self.by_parent.retain(|id, _| id.split_once('/').map(|(h, _)| h) != Some(source));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn task(id: &str, status: &str, at: i64) -> Delegation {
        Delegation { task_id: id.into(), title: id.into(), status: status.into(), created_at: at, ..Default::default() }
    }

    #[test]
    fn a_working_task_with_a_question_is_asking() {
        let asks = Delegation { question: "Which runner?".into(), ..task("a", "working", 1) };
        assert_eq!([task("a", "queued", 1).phase(), task("a", "working", 1).phase(), asks.phase(), task("a", "completed", 1).phase(), task("a", "failed", 1).phase(), task("a", "cancelled", 1).phase()], [Phase::Starting, Phase::Working, Phase::Asking, Phase::Done, Phase::Failed, Phase::Stopped]);
    }

    #[test]
    fn a_settled_task_stops_counting_its_time() {
        let done = Delegation { settled_at: 9_000, ..task("a", "completed", 4_000) };
        assert_eq!((done.elapsed_ms(60_000), task("b", "working", 4_000).elapsed_ms(10_000), task("c", "working", 4_000).elapsed_ms(1_000)), (5_000, 6_000, 0));
    }

    #[test]
    fn an_update_replaces_its_task_and_a_new_one_joins_in_order_of_creation() {
        let mut d = Delegations::default();
        d.replace("p", vec![task("b", "working", 2), task("a", "working", 1)]);
        d.update("p", task("a", "completed", 1));
        d.update("p", task("c", "starting", 3));
        let got: Vec<_> = d.of("p").iter().map(|t| (t.task_id.as_str(), t.phase())).collect();
        assert_eq!(got, [("a", Phase::Done), ("b", Phase::Working), ("c", Phase::Starting)]);
    }

    #[test]
    fn all_settled_needs_a_task_and_every_one_ended() {
        let mut d = Delegations::default();
        assert!(!d.all_settled("p"));
        d.update("p", task("a", "completed", 1));
        d.update("p", task("b", "failed", 2));
        assert!(d.all_settled("p"));
        d.update("p", task("c", "working", 3));
        assert!(!d.all_settled("p"));
    }

    #[test]
    fn counts_read_asking_before_working_and_settled_once_none_is_left() {
        let mut d = Delegations::default();
        d.update("p", task("a", "working", 1));
        d.update("p", Delegation { question: "?".into(), ..task("b", "working", 2) });
        d.update("p", task("c", "completed", 3));
        assert_eq!(d.counts("p").headline(), Headline::Asking(1));
        d.update("p", task("b", "failed", 2));
        assert_eq!(d.counts("p").headline(), Headline::Working(1));
        d.update("p", task("a", "completed", 1));
        assert_eq!(d.counts("p").headline(), Headline::Settled { done: 2, failed: 1 });
    }

    #[test]
    fn clearing_keeps_the_tasks_still_at_work() {
        let mut d = Delegations::default();
        d.update("p", task("a", "completed", 1));
        d.update("p", task("b", "working", 2));
        d.clear_settled("p");
        assert_eq!(d.of("p").len(), 1);
        d.update("p", task("b", "failed", 2));
        d.clear_settled("p");
        assert!(d.of("p").is_empty());
    }

    #[test]
    fn a_host_forgotten_takes_only_its_own_chats() {
        let mut d = Delegations::default();
        d.update("h/a", task("a", "working", 1));
        d.update("a", task("b", "working", 1));
        d.forget("h");
        assert!(d.of("h/a").is_empty() && !d.of("a").is_empty());
    }

    #[test]
    fn the_golden_frames_decode() {
        #[derive(Deserialize)]
        struct Frame {
            items: Vec<Delegation>,
        }
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/agent_delegations.json")).unwrap();
        assert_eq!((f.items[0].phase(), f.items[1].phase(), f.items[0].agent.as_str()), (Phase::Working, Phase::Done, "codex"));
    }
}
