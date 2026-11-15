mod detail;
mod list;
mod ticket;

use crate::actions::OpenSession;
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use crate::factory::page::Page;
use crate::team::plural;
use crate::status::Status;
use crate::util::{basename, first_line};
use agents::tickets::{self, Gate, Station, Ticket};
use agents::{Agents, Permission, Summary};
use gpui_kit::*;
use std::cmp::Reverse;
use std::collections::HashMap;
use store::prefs::sidebar::{InboxFilter, InboxSort};

pub(crate) const CONTEXT: &str = "Inbox";

/// The decision a Factory row asks for.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Plan,
    Ask,
    Pr,
    Failed,
}

/// What makes a row a Factory row: its Ticket and the decision.
#[derive(Clone, Debug, PartialEq)]
pub struct FactoryRow {
    pub ticket: String,
    pub kind: Kind,
    pub station: Station,
    /// The Ask's request id, empty when the row is no Ask or the agent asks in its chat.
    pub request: String,
}

impl FactoryRow {
    pub fn label(&self) -> String {
        let station = match self.station {
            Station::Spec => "Spec",
            Station::Plan => "Plan",
            Station::Implement => "Implement",
            Station::Checks => "Checks",
            Station::Pr => "PR review",
            Station::Merged | Station::None => "Ticket",
        };
        match self.kind {
            Kind::Plan => "Plan approval".into(),
            Kind::Ask => "Command ask".into(),
            Kind::Pr => "PR review".into(),
            Kind::Failed => format!("{station} failed"),
        }
    }
}

#[derive(Clone)]
pub struct Note {
    pub agent: String,
    pub terminal: String,
    pub status: Status,
    pub title: String,
    pub subtitle: String,
    pub at: i64,
    /// Set when the row is a Factory ticket's decision, not a session's.
    pub factory: Option<FactoryRow>,
}

impl Note {
    /// What the selection follows: the terminal for a session's row, the Ticket and the decision for a Factory row.
    pub fn key(&self) -> String {
        match &self.factory {
            None => self.terminal.clone(),
            Some(f) if f.kind == Kind::Ask => format!("factory:{}:{}", f.ticket, self.agent),
            Some(f) => format!("factory:{}:{:?}", f.ticket, f.kind),
        }
    }
}

pub(crate) fn noted(a: &Summary) -> Option<Status> {
    Status::of(a).filter(Status::alerting)
}

/// One row before its text is built, so the bell can count rows without building any.
enum Found<'a> {
    Session(&'a Summary, Status),
    Ask(&'a Summary, &'a Ticket, Option<&'a Permission>),
    Gate(&'a Ticket, Kind),
}

fn in_factory(t: &Ticket) -> bool {
    t.station != Station::None && t.state == tickets::State::Live
}

/// Sessions outside the Factory that alert, then one row per decision a Factory ticket waits on.
/// A session of a Factory ticket only gets a row for its Ask: the Ticket's own row covers the rest.
fn found(agents: &Agents) -> Vec<Found<'_>> {
    let leads: HashMap<&str, &Ticket> = agents.board.tickets.iter().filter(|t| in_factory(t) && !t.agent_id.is_empty()).map(|t| (t.agent_id.as_str(), t)).collect();
    let mut out = Vec::new();
    for a in &agents.list {
        let Some(status) = noted(a) else { continue };
        let Some(t) = leads.get(a.id.as_str()).or_else(|| leads.get(a.parent.as_str())).copied() else {
            out.push(Found::Session(a, status));
            continue;
        };
        if status != Status::NeedsYou {
            continue;
        }
        let ask = agents.pending.iter().find(|p| p.agent_id == a.id);
        if ask.is_some() || !matches!(t.gate, Gate::Plan | Gate::Pr) {
            out.push(Found::Ask(a, t, ask));
        }
    }
    for t in agents.board.tickets.iter().filter(|t| in_factory(t)) {
        match (t.status == tickets::Status::Failed, t.gate) {
            (true, _) => out.push(Found::Gate(t, Kind::Failed)),
            (_, Gate::Plan) => out.push(Found::Gate(t, Kind::Plan)),
            (_, Gate::Pr) => out.push(Found::Gate(t, Kind::Pr)),
            _ => {}
        }
    }
    out
}

/// What a Ticket's row says after its decision: the plan's size, the PR's number, or why it failed. The project when none is known.
fn gate_detail(t: &Ticket, kind: Kind, team: Option<&agents::Team>) -> String {
    let detail = match kind {
        Kind::Plan => team.map(|tm| plural(tm.tasks.len(), "task", "tasks")),
        Kind::Pr => Some(t.pr).filter(|n| *n > 0).map(|n| format!("#{n}")),
        Kind::Failed => Some(first_line(&t.why)).filter(|w| !w.is_empty()),
        Kind::Ask => None,
    };
    detail.unwrap_or_else(|| basename(&t.folder))
}

/// What needs you, then what failed, then the done sessions nobody has seen yet, each oldest first.
pub fn notes(agents: &Agents) -> Vec<Note> {
    let row = |t: &Ticket, kind, request: String| Some(FactoryRow { ticket: t.id.clone(), kind, station: t.station, request });
    let mut out: Vec<Note> = found(agents)
        .into_iter()
        .map(|f| match f {
            Found::Session(a, status) => {
                let (title, subtitle) = match (status, agents.pending.iter().find(|p| p.agent_id == a.id)) {
                    (Status::NeedsYou, Some(p)) => (p.ask(), a.title.clone()),
                    (Status::Failed, _) => (format!("{} failed", a.title), first_line(agents.last_result(&a.id).map_or("", |r| r.error.as_str()))),
                    _ => (a.title.clone(), first_line(agents.last_text(&a.id).unwrap_or_default())),
                };
                Note { agent: a.id.clone(), terminal: a.terminal_id.clone(), status, title, subtitle, at: a.updated_at, factory: None }
            }
            Found::Ask(a, t, ask) => Note {
                agent: a.id.clone(),
                terminal: a.terminal_id.clone(),
                status: Status::NeedsYou,
                title: t.title.clone(),
                subtitle: ask.map_or_else(|| basename(&t.folder), |p| p.parts().1),
                at: a.updated_at,
                factory: row(t, Kind::Ask, ask.map(|p| p.request_id.clone()).unwrap_or_default()),
            },
            Found::Gate(t, kind) => Note {
                agent: t.agent_id.clone(),
                terminal: t.terminal_id.clone(),
                status: if kind == Kind::Failed { Status::Failed } else { Status::NeedsYou },
                title: t.title.clone(),
                subtitle: gate_detail(t, kind, agents.team(&t.agent_id)),
                at: t.changed_at,
                factory: row(t, kind, String::new()),
            },
        })
        .collect();
    out.sort_by_key(|n| (n.status, n.at));
    out
}

pub(crate) const FILTERS: [(InboxFilter, &str); 4] = [(InboxFilter::All, "All"), (InboxFilter::NeedsYou, "Needs you"), (InboxFilter::Failed, "Failed"), (InboxFilter::Done, "Done")];

/// The notes `filter` keeps, in `sort`'s order; Most urgent first is the order `notes` gives.
pub fn arrange(mut notes: Vec<Note>, sort: InboxSort, filter: InboxFilter) -> Vec<Note> {
    notes.retain(|n| match filter {
        InboxFilter::All => true,
        InboxFilter::NeedsYou => n.status == Status::NeedsYou,
        InboxFilter::Failed => n.status == Status::Failed,
        InboxFilter::Done => n.status == Status::Done,
    });
    match sort {
        InboxSort::Urgent => {}
        InboxSort::Newest => notes.sort_by_key(|n| Reverse(n.at)),
        InboxSort::Oldest => notes.sort_by_key(|n| n.at),
    }
    notes
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
    found(agents).len()
}

/// The rows that need an answer, for the Dock badge that counts only those.
pub fn needing(agents: &Agents) -> usize {
    found(agents).iter().filter(|f| !matches!(f, Found::Session(_, status) if *status != Status::NeedsYou) && !matches!(f, Found::Gate(_, Kind::Failed))).count()
}

/// Keeps the selection on the row with `key` as the list changes, and says whether that row is still there.
/// If it left, the row that took its place is selected.
pub fn reselect(notes: &[Note], key: Option<&str>, i: usize) -> (usize, bool) {
    match notes.iter().position(|n| key == Some(n.key().as_str())) {
        Some(j) => (j, true),
        None => (i.min(notes.len().saturating_sub(1)), false),
    }
}

/// Asks stay until answered, so they are never marked seen.
fn asks(note: &Note) -> bool {
    note.status == Status::NeedsYou
}

/// The agents "Mark all seen" marks seen. A Factory row stays until its decision is made.
fn readable(notes: Vec<Note>) -> Vec<String> {
    notes.into_iter().filter(|n| !asks(n) && n.factory.is_none()).map(|n| n.agent).collect()
}

/// Note `i`, clamped to the list, and the terminal to focus for it. A Factory row focuses none.
fn select(notes: &[Note], i: usize) -> (usize, Option<String>) {
    let i = i.min(notes.len().saturating_sub(1));
    (i, notes.get(i).filter(|n| n.factory.is_none()).map(|n| n.terminal.clone()))
}

/// The Factory ticket page a row opens in the pane.
fn page_of(n: &Note) -> Option<Page> {
    n.factory.as_ref().map(|f| Page { ask: f.request.clone(), ..Page::of(&f.ticket) })
}

/// What the virtualized list draws: a heading, or note `i`.
#[derive(Clone, Debug, PartialEq)]
pub enum Entry {
    Head(String, usize),
    Note(usize),
}

/// The list's rows. Headings only show with Sort inbox by Most urgent.
pub fn entries(notes: &[Note], urgent: bool) -> Vec<Entry> {
    let mut out = Vec::with_capacity(notes.len() + 3);
    for i in 0..notes.len() {
        if let Some((label, count)) = heading(notes, i).filter(|_| urgent) {
            out.push(Entry::Head(label, count));
        }
        out.push(Entry::Note(i));
    }
    out
}

/// Where note `i` is in the list.
pub fn entry_of(entries: &[Entry], i: usize) -> Option<usize> {
    entries.iter().position(|e| *e == Entry::Note(i))
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
    /// The selected row's key.
    pub(crate) key: Option<String>,
    /// The rows shown. Rebuilt when sessions or the Board change, never in render.
    pub(crate) notes: Vec<Note>,
    pub(crate) entries: Vec<Entry>,
    pub(crate) list: ListState,
    /// The Factory ticket page of the selected row, which the Factory bar acts on.
    pub(crate) page: Option<Page>,
    /// The sort `notes` was built with.
    pub(crate) sort: InboxSort,
    pub(crate) focus: FocusHandle,
    pub(crate) filter: InboxFilter,
    pub(crate) filter_open: bool,
}

impl InboxState {
    pub fn new(cx: &mut Context<Desktop>) -> Self {
        Self { selected: 0, key: None, notes: Vec::new(), entries: Vec::new(), list: ListState::new(0, ListAlignment::Top, px(400.)), page: None, sort: InboxSort::Urgent, focus: cx.focus_handle(), filter: InboxFilter::All, filter_open: false }
    }

    /// Takes the rows to show with row `i` selected. The list keeps its place.
    fn show(&mut self, notes: Vec<Note>, i: usize, sort: InboxSort) {
        let entries = entries(&notes, sort == InboxSort::Urgent);
        if entries != self.entries {
            let top = self.list.logical_scroll_top();
            self.list.reset(entries.len());
            self.list.scroll_to(top);
            self.entries = entries;
        }
        let key = notes.get(i).map(Note::key);
        if key != self.key {
            self.page = notes.get(i).and_then(page_of);
        }
        (self.selected, self.key, self.sort) = (i, key, sort);
        self.notes = notes;
    }
}

impl Desktop {
    pub fn open_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.screen = Screen::Inbox;
        (self.inbox.filter, self.inbox.filter_open) = (self.store.sidebar.inbox_filter, false);
        self.select_note(0, cx);
        window.focus(&self.inbox.focus, cx);
    }

    fn arranged(&self) -> Vec<Note> {
        arrange(notes(&self.agents), self.store.sidebar.inbox_sort, self.inbox.filter)
    }

    pub(crate) fn select_note(&mut self, i: usize, cx: &mut Context<Self>) {
        let notes = self.arranged();
        let (i, terminal) = select(&notes, i);
        self.terminal.focused = terminal;
        self.inbox.show(notes, i, self.store.sidebar.inbox_sort);
        if let Some(at) = entry_of(&self.inbox.entries, i) {
            self.inbox.list.scroll_to_reveal_item(at);
        }
        self.inbox_page_changed();
        cx.notify();
    }

    /// The Inbox follows the sessions and the Board. If the selected row left, the focus drops
    /// so keystrokes never reach a terminal the user didn't pick.
    pub(crate) fn inbox_changed(&mut self) {
        let notes = self.arranged();
        let (i, kept) = reselect(&notes, self.inbox.key.as_deref(), self.inbox.selected);
        if !kept {
            self.terminal.focused = None;
        }
        self.inbox.show(notes, i, self.store.sidebar.inbox_sort);
        if !kept {
            self.inbox_page_changed();
        }
    }

    /// The pane shows another Factory row: point the plan edits at it and ask pocketd for its detail.
    fn inbox_page_changed(&mut self) {
        self.aim_plan_edits();
        if let Some(page) = &self.inbox.page {
            self.outbox.local.factory_ticket(&page.ticket);
        }
    }

    /// The selected Factory row's detail is stale: ask for it again.
    pub(crate) fn inbox_ticket_updated(&self, ticket: &str) {
        if self.screen == Screen::Inbox && self.inbox.page.as_ref().is_some_and(|p| p.shows(ticket)) {
            self.outbox.local.factory_ticket(ticket);
        }
    }

    fn project_name(&self, agent: &str) -> String {
        basename(&self.project_path(agent))
    }

    /// The project the agent works in, else its own folder.
    fn project_path(&self, agent: &str) -> String {
        let projects = self.projects();
        let cwd = self.cwd_of(agent).unwrap_or_default();
        self.project_of(&cwd, &projects).cloned().unwrap_or(cwd)
    }

    pub(crate) fn open_selected(&mut self, _: &OpenSession, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen != Screen::Inbox {
            return;
        }
        let Some(n) = self.inbox.notes.get(self.inbox.selected).cloned() else { return };
        if n.factory.is_some() {
            self.factory_primary(window, cx);
        } else {
            self.focus_agent(&n.agent, window, cx);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Entry, FactoryRow, Kind, Note, arrange, count, gate_detail, heading, entry_of, entries, notes, readable, page_of, reselect, select, step};
    use crate::status::Status;
    use agents::tickets::{Station, Ticket};
    use agents::{Agents, Item, Permission, Summary};
    use serde_json::json;
    use store::prefs::sidebar::{InboxFilter, InboxSort};

    fn agent(id: &str, status: &str, at: i64) -> Summary {
        Summary { id: id.into(), terminal_id: format!("t-{id}"), title: id.to_uppercase(), status: status.into(), attached: true, updated_at: at, ..Default::default() }
    }

    #[test]
    fn the_selection_stays_on_its_row_and_takes_the_next_row_when_that_row_leaves() {
        let rows = |ids: &[&str]| ids.iter().map(|t| note(t, Status::NeedsYou)).collect::<Vec<_>>();
        assert_eq!(reselect(&rows(&["new", "a", "b"]), Some("a"), 0), (1, true));
        assert_eq!(reselect(&rows(&["a", "c"]), Some("b"), 1), (1, false), "b left and c took its place");
        assert_eq!(reselect(&rows(&["a"]), Some("c"), 2), (0, false));
        assert_eq!(reselect(&[], None, 4), (0, false));
    }

    #[test]
    fn a_factory_row_has_its_own_key_and_focuses_no_terminal() {
        let gate = Note { factory: row("T1", Kind::Plan, Station::Plan, ""), ..note("lead", Status::NeedsYou) };
        let ask = Note { factory: row("T1", Kind::Ask, Station::Implement, "r1"), ..note("w1", Status::NeedsYou) };
        let plain = note("a", Status::Done);
        assert_eq!((gate.key(), ask.key(), plain.key()), ("factory:T1:Plan".to_string(), "factory:T1:W1".to_string(), "a".to_string()));
        let notes = [gate, plain];
        assert_eq!((select(&notes, 0), select(&notes, 1)), ((0, None), (1, Some("a".to_string()))));
    }

    #[test]
    fn a_factory_row_opens_the_page_of_its_ticket_and_the_ask_it_names() {
        let ask = Note { factory: row("T1", Kind::Ask, Station::Implement, "r1"), ..note("w1", Status::NeedsYou) };
        let page = page_of(&ask).unwrap();
        assert_eq!((page.ticket.as_str(), page.ask.as_str(), page.tab, page.changes), ("T1", "r1", None, false));
        assert!(page_of(&note("a", Status::Done)).is_none());
    }

    #[test]
    fn a_gate_row_says_the_plans_size_the_prs_number_or_why_it_failed() {
        let t = |more| ticket("T1", "lead", more);
        let team = agents::Team { tasks: vec![Default::default(), Default::default()], ..Default::default() };
        assert_eq!(gate_detail(&t(json!({})), Kind::Plan, Some(&team)), "2 tasks");
        assert_eq!(gate_detail(&t(json!({})), Kind::Plan, None), "api");
        assert_eq!(gate_detail(&t(json!({"pr": 203})), Kind::Pr, None), "#203");
        assert_eq!(gate_detail(&t(json!({})), Kind::Pr, None), "api");
        assert_eq!(gate_detail(&t(json!({"why": "Checks · npm test\nmore"})), Kind::Failed, None), "Checks · npm test");
    }

    #[test]
    fn the_list_puts_a_heading_before_each_status_when_sorted_by_urgency() {
        let notes = [note("a", Status::NeedsYou), note("b", Status::NeedsYou), note("c", Status::Failed)];
        let urgent = entries(&notes, true);
        assert_eq!(urgent, vec![Entry::Head("NEEDS YOU".into(), 2), Entry::Note(0), Entry::Note(1), Entry::Head("FAILED".into(), 1), Entry::Note(2)]);
        assert_eq!(entries(&notes, false), vec![Entry::Note(0), Entry::Note(1), Entry::Note(2)]);
        assert_eq!((entry_of(&urgent, 2), entry_of(&urgent, 9)), (Some(4), None));
    }

    #[test]
    fn lists_agents_that_need_you_then_failed_then_done() {
        let mut agents = Agents {
            list: vec![agent("done", "done", 3), Summary { failed: true, ..agent("fail", "done", 1) }, agent("ask", "needsYou", 0), agent("busy", "working", 9), Summary { attached: false, ..agent("blind", "done", 9) }],
            pending: vec![Permission { agent_id: "ask".into(), tool_name: "Bash".into(), ..Default::default() }],
            ..Default::default()
        };
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
        let mut agents = Agents { list: vec![agent("ask", "needsYou", 0)], ..Default::default() };
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
        Note { agent: terminal.to_uppercase(), terminal: terminal.into(), status, title: String::new(), subtitle: String::new(), at: 0, factory: None }
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
    fn the_inbox_shows_one_status_or_all_newest_or_oldest_first() {
        let at = |t: &str, status, at| Note { at, ..note(t, status) };
        let all = || vec![at("a", Status::NeedsYou, 3), at("b", Status::Failed, 1), at("c", Status::Done, 2)];
        let terminals = |notes: Vec<Note>| notes.into_iter().map(|n| n.terminal).collect::<Vec<_>>();
        assert_eq!(terminals(arrange(all(), InboxSort::Urgent, InboxFilter::All)), ["a", "b", "c"]);
        assert_eq!(terminals(arrange(all(), InboxSort::Newest, InboxFilter::All)), ["a", "c", "b"]);
        assert_eq!(terminals(arrange(all(), InboxSort::Oldest, InboxFilter::All)), ["b", "c", "a"]);
        assert_eq!(terminals(arrange(all(), InboxSort::Urgent, InboxFilter::Failed)), ["b"]);
    }

    /// A Factory ticket led by `lead`. `more` overrides fields, in the wire's names.
    fn ticket(id: &str, lead: &str, more: serde_json::Value) -> Ticket {
        let mut raw = json!({"id": id, "title": format!("Ticket {id}"), "prompt": "", "provider": "claude", "folder": "/code/api", "state": "live", "agentId": lead, "terminalId": format!("t-{lead}"), "station": "plan", "changedAt": 5});
        raw.as_object_mut().unwrap().extend(more.as_object().unwrap().clone());
        serde_json::from_value(raw).unwrap()
    }

    fn worker(id: &str, lead: &str, status: &str, at: i64) -> Summary {
        Summary { parent: lead.into(), ..agent(id, status, at) }
    }

    fn row(ticket: &str, kind: Kind, station: Station, request: &str) -> Option<FactoryRow> {
        Some(FactoryRow { ticket: ticket.into(), kind, station, request: request.into() })
    }

    #[test]
    fn a_ticket_at_the_plan_gate_is_one_row_that_carries_the_ticket() {
        let mut agents = Agents { list: vec![agent("lead", "needsYou", 9)], ..Default::default() };
        agents.board.tickets = vec![ticket("T1", "lead", json!({"gate": "plan", "status": "needsYou", "changedAt": 4}))];
        let got = notes(&agents);
        assert_eq!(got.len(), 1);
        let n = &got[0];
        assert_eq!((n.agent.as_str(), n.status, n.title.as_str(), n.subtitle.as_str(), n.at), ("lead", Status::NeedsYou, "Ticket T1", "api", 4));
        assert_eq!(n.factory, row("T1", Kind::Plan, Station::Plan, ""));
        assert_eq!(n.factory.as_ref().unwrap().label(), "Plan approval");
    }

    #[test]
    fn a_worker_that_asks_is_a_row_with_the_command() {
        let mut agents = Agents { list: vec![agent("lead", "working", 1), worker("w1", "lead", "needsYou", 7)], ..Default::default() };
        let mut ask = Permission { request_id: "r1".into(), agent_id: "w1".into(), tool_name: "Bash".into(), ..Default::default() };
        (ask.detail.kind, ask.detail.command) = ("shell".into(), "npm install".into());
        agents.pending = vec![ask];
        agents.board.tickets = vec![ticket("T1", "lead", json!({"station": "implement", "gate": "ask", "status": "needsYou"}))];
        let got = notes(&agents);
        assert_eq!(got.len(), 1);
        let n = &got[0];
        assert_eq!((n.agent.as_str(), n.terminal.as_str(), n.title.as_str(), n.subtitle.as_str(), n.at), ("w1", "t-w1", "Ticket T1", "npm install", 7));
        assert_eq!(n.factory, row("T1", Kind::Ask, Station::Implement, "r1"));
        assert_eq!(n.factory.as_ref().unwrap().label(), "Command ask");
    }

    #[test]
    fn a_ticket_at_the_pr_gate_is_a_row_even_when_no_session_needs_you() {
        let mut agents = Agents { list: vec![agent("lead", "done", 9)], ..Default::default() };
        agents.board.tickets = vec![ticket("T1", "lead", json!({"station": "pr", "gate": "pr", "status": "done", "changedAt": 6}))];
        let got = notes(&agents);
        assert_eq!(got.len(), 1, "the lead's done row is left out");
        assert_eq!((got[0].status, got[0].at), (Status::NeedsYou, 6));
        assert_eq!(got[0].factory.as_ref().unwrap().label(), "PR review");
    }

    #[test]
    fn a_factory_ticket_that_failed_is_one_failed_row() {
        let mut agents = Agents { list: vec![Summary { failed: true, ..agent("lead", "done", 9) }], ..Default::default() };
        agents.board.tickets = vec![ticket("T1", "lead", json!({"station": "checks", "status": "failed", "changedAt": 3}))];
        let got = notes(&agents);
        assert_eq!(got.len(), 1);
        assert_eq!((got[0].status, got[0].at), (Status::Failed, 3));
        assert_eq!(got[0].factory, row("T1", Kind::Failed, Station::Checks, ""));
        assert_eq!(got[0].factory.as_ref().unwrap().label(), "Checks failed");
    }

    #[test]
    fn a_factory_ticket_that_failed_with_its_lead_gone_is_still_a_row() {
        let mut agents = Agents::default();
        agents.board.tickets = vec![ticket("T1", "", json!({"station": "spec", "status": "failed", "terminalId": "", "changedAt": 3}))];
        let got = notes(&agents);
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].factory, row("T1", Kind::Failed, Station::Spec, ""));
        assert_eq!(count(&agents), 1);
    }

    #[test]
    fn factory_rows_sort_with_the_rest_and_the_bell_counts_one_per_row() {
        let mut agents = Agents { list: vec![agent("solo", "needsYou", 5), agent("lead", "needsYou", 8), Summary { failed: true, ..agent("lead2", "done", 9) }, agent("old", "done", 1)], ..Default::default() };
        agents.board.tickets = vec![ticket("T1", "lead", json!({"gate": "plan", "status": "needsYou", "changedAt": 2})), ticket("T2", "lead2", json!({"station": "checks", "status": "failed", "changedAt": 3}))];
        let got: Vec<(String, Status)> = notes(&agents).into_iter().map(|n| (n.factory.map_or(n.agent, |f| f.ticket), n.status)).collect();
        let want = [("T1", Status::NeedsYou), ("solo", Status::NeedsYou), ("T2", Status::Failed), ("old", Status::Done)];
        assert_eq!(got, want.map(|(id, s)| (id.to_string(), s)));
        assert_eq!(count(&agents), 4);
    }

    #[test]
    fn mark_all_seen_leaves_factory_rows() {
        let failed = Note { factory: row("T1", Kind::Failed, Station::Checks, ""), ..note("a", Status::Failed) };
        assert_eq!(readable(vec![failed, note("b", Status::Done)]), vec!["B"]);
    }
}
