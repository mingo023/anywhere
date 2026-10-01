mod detail;
mod list;

use crate::actions::OpenSession;
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::status::Status;
use crate::util::basename;
use agents::{Agents, Summary};
use gpui_kit::*;

pub struct Note {
    pub agent: String,
    pub terminal: String,
    pub status: Status,
    pub title: String,
    pub subtitle: String,
    pub at: i64,
}

fn first_line(s: &str) -> String {
    s.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default().to_string()
}

fn noted(a: &Summary) -> Option<Status> {
    Status::of(a).filter(Status::alerting)
}

/// Agents that need you, then the failed and done ones nobody has seen yet, each oldest transition first.
pub fn notes(agents: &Agents) -> Vec<Note> {
    let mut out: Vec<Note> = agents
        .list
        .iter()
        .filter_map(|a| {
            let status = noted(a)?;
            let (title, subtitle) = match (status, agents.pending.iter().find(|p| p.agent_id == a.id)) {
                (Status::NeedsYou, Some(p)) => (p.ask(), a.title.clone()),
                (Status::Failed, _) => (format!("{} failed", a.title), first_line(agents.last_result(&a.id).map_or("", |r| r.error.as_str()))),
                _ => (a.title.clone(), first_line(agents.last_text(&a.id).unwrap_or_default())),
            };
            Some(Note { agent: a.id.clone(), terminal: a.terminal_id.clone(), status, title, subtitle, at: a.updated_at })
        })
        .collect();
    out.sort_by_key(|n| (n.status, n.at));
    out
}

/// The heading above note `i` when it opens its status's section: "NEEDS YOU", "FAILED" or "DONE", and the section's size.
pub fn heading(notes: &[Note], i: usize) -> Option<(String, usize)> {
    let status = notes.get(i)?.status;
    if i > 0 && notes[i - 1].status == status {
        return None;
    }
    Some((status.label().to_uppercase(), notes.iter().filter(|n| n.status == status).count()))
}

pub fn count(agents: &Agents) -> usize {
    agents.list.iter().filter(|a| noted(a).is_some()).count()
}

/// Keeps the selection on the focused note as the list changes. If that note left, drops focus
/// so keystrokes never reach a terminal the user didn't pick.
pub fn reselect(notes: &[Note], focused: Option<&str>, i: usize) -> (usize, Option<String>) {
    match notes.iter().position(|n| focused == Some(n.terminal.as_str())) {
        Some(j) => (j, Some(notes[j].terminal.clone())),
        None => (i.min(notes.len().saturating_sub(1)), None),
    }
}

/// The agents "Mark all seen" marks seen: asks stay until answered.
fn readable(notes: Vec<Note>) -> Vec<String> {
    notes.into_iter().filter(|n| n.status != Status::NeedsYou).map(|n| n.agent).collect()
}

/// Note `i`, clamped to the list, and the terminal to focus for it.
fn select(notes: &[Note], i: usize) -> (usize, Option<String>) {
    let i = i.min(notes.len().saturating_sub(1));
    (i, notes.get(i).map(|n| n.terminal.clone()))
}

/// The note `key` moves the selection to, if it navigates.
fn step(key: &str, selected: usize) -> Option<usize> {
    match key {
        "j" | "down" => Some(selected + 1),
        "k" | "up" => Some(selected.saturating_sub(1)),
        _ => None,
    }
}

pub struct InboxState {
    pub(crate) selected: usize,
    pub(crate) focus: FocusHandle,
}

impl InboxState {
    pub fn new(cx: &mut Context<Desktop>) -> Self {
        Self { selected: 0, focus: cx.focus_handle() }
    }
}

impl Desktop {
    pub fn open_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = Screen::Inbox;
        self.select_note(0, cx);
        window.focus(&self.inbox.focus, cx);
    }

    fn select_note(&mut self, i: usize, cx: &mut Context<Self>) {
        (self.inbox.selected, self.terminal.focused) = select(&notes(&self.agents), i);
        cx.notify();
    }

    fn project_name(&self, agent: &str) -> String {
        let projects = self.projects();
        let cwd = self.cwd_of(agent).unwrap_or_default();
        basename(self.project_of(&cwd, &projects).unwrap_or(&cwd))
    }

    pub(crate) fn open_selected(&mut self, _: &OpenSession, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen != Screen::Inbox {
            return;
        }
        if let Some(n) = notes(&self.agents).into_iter().nth(self.inbox.selected) {
            self.focus_agent(&n.agent, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Note, count, first_line, heading, notes, readable, reselect, select, step};
    use crate::status::Status;
    use agents::{Agents, Item, Permission, Summary};

    fn agent(id: &str, status: &str, at: i64) -> Summary {
        Summary { id: id.into(), terminal_id: format!("t-{id}"), title: id.to_uppercase(), status: status.into(), attached: true, updated_at: at, ..Default::default() }
    }

    #[test]
    fn selection_follows_the_focused_note_else_clamps_and_drops_focus() {
        let note = |t: &str| Note { agent: t.into(), terminal: t.into(), status: Status::NeedsYou, title: String::new(), subtitle: String::new(), at: 0 };
        assert_eq!(reselect(&[note("new"), note("a"), note("b")], Some("a"), 0), (1, Some("a".to_string())));
        assert_eq!(reselect(&[note("a"), note("b")], Some("c"), 2), (1, None));
        assert_eq!(reselect(&[note("a")], None, 4), (0, None));
    }

    #[test]
    fn lists_agents_that_need_you_then_failed_then_done() {
        let mut agents = Agents::default();
        agents.list = vec![agent("done", "done", 3), Summary { failed: true, ..agent("fail", "done", 1) }, agent("ask", "needsYou", 0), agent("busy", "working", 9), Summary { attached: false, ..agent("blind", "done", 9) }];
        agents.pending = vec![Permission { agent_id: "ask".into(), tool_name: "Bash".into(), ..Default::default() }];
        agents.timelines.insert("done".into(), vec![Item { kind: "assistant".into(), text: "\nTagged v2\nmore".into(), ..Default::default() }]);
        agents.timelines.insert("fail".into(), vec![Item { kind: "result".into(), error: "exit 1".into(), ..Default::default() }]);
        let got: Vec<(String, Status, String, String)> = notes(&agents).into_iter().map(|n| (n.terminal, n.status, n.title, n.subtitle)).collect();
        let want = [("t-ask", Status::NeedsYou, "Wants to use Bash", "ASK"), ("t-fail", Status::Failed, "FAIL failed", "exit 1"), ("t-done", Status::Done, "DONE", "Tagged v2")];
        assert_eq!(got, want.map(|(t, s, a, b)| (t.to_string(), s, a.to_string(), b.to_string())));
    }

    #[test]
    fn the_bell_counts_the_agents_that_need_you_and_the_unseen_failed_and_done() {
        let agents = Agents {
            list: vec![agent("done", "done", 3), Summary { failed: true, ..agent("fail", "done", 1) }, agent("ask", "needsYou", 0), agent("busy", "working", 9), agent("idle", "idle", 9), Summary { attached: false, ..agent("blind", "done", 9) }],
            ..Default::default()
        };
        assert_eq!(count(&agents), 3);
        assert_eq!(count(&agents), notes(&agents).len());
    }

    #[test]
    fn the_inbox_lists_needs_you_failed_done_oldest_first() {
        let mut agents = Agents::default();
        let failed = |id: &str, at| Summary { failed: true, ..agent(id, "done", at) };
        agents.list = vec![agent("new", "done", 5), agent("ask2", "needsYou", 7), failed("fail", 9), agent("old", "done", 1), agent("ask", "needsYou", 2)];
        let notes = notes(&agents);
        let got: Vec<&str> = notes.iter().map(|n| n.agent.as_str()).collect();
        assert_eq!(got, vec!["ask", "ask2", "fail", "old", "new"]);
        let headings: Vec<_> = (0..notes.len()).map(|i| heading(&notes, i)).collect();
        let want = [Some(("NEEDS YOU", 2)), None, Some(("FAILED", 1)), Some(("DONE", 2)), None];
        assert_eq!(headings, want.map(|h| h.map(|(l, n)| (l.to_string(), n))));
    }

    #[test]
    fn a_needs_you_agent_without_an_open_ask_reads_its_title_and_last_text() {
        let mut agents = Agents::default();
        agents.list = vec![agent("ask", "needsYou", 0)];
        agents.timelines.insert("ask".into(), vec![Item { kind: "assistant".into(), text: "Which branch?".into(), ..Default::default() }]);
        let n = &notes(&agents)[0];
        assert_eq!((n.title.as_str(), n.subtitle.as_str()), ("ASK", "Which branch?"));
    }

    #[test]
    fn j_and_down_step_to_the_next_note_and_k_and_up_to_the_previous_stopping_at_the_top() {
        let got = [step("j", 1), step("down", 1), step("k", 1), step("up", 1), step("k", 0), step("x", 1)];
        assert_eq!(got, [Some(2), Some(2), Some(0), Some(0), Some(0), None]);
    }

    fn note(terminal: &str, status: Status) -> Note {
        Note { agent: terminal.to_uppercase(), terminal: terminal.into(), status, title: String::new(), subtitle: String::new(), at: 0 }
    }

    #[test]
    fn selecting_a_note_clamps_to_the_list_and_focuses_its_terminal() {
        let notes = [note("a", Status::NeedsYou), note("b", Status::Done)];
        assert_eq!((select(&notes, 0), select(&notes, 5), select(&[], 1)), ((0, Some("a".to_string())), (1, Some("b".to_string())), (0, None)));
    }

    #[test]
    fn mark_all_seen_marks_every_agent_but_those_that_need_you() {
        let notes = vec![note("a", Status::NeedsYou), note("b", Status::Failed), note("c", Status::Done)];
        assert_eq!(readable(notes), vec!["B", "C"]);
    }

    #[test]
    fn first_line_skips_blank_lines() {
        assert_eq!(first_line("\n  Tagged, changelog updated\nmore"), "Tagged, changelog updated");
        assert_eq!(first_line(""), "");
    }
}
