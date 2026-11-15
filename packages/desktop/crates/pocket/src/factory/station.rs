use crate::board::logic::{FINISHED_SHOWN, Filters, Slot, Tone, activity, busy, column as status_column};
use crate::team::progress::counts as task_counts;
use crate::team::{PROPOSED, plural};
use agents::team::state::RUNNING;
use agents::tickets::{ClosedWhy, Gate, State, Station, Status, Ticket};
use agents::{Permission, Team};
use git::github::{Pr, PrState};
use std::cmp::Reverse;

/// A column of the Factory layout.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Column {
    Todo,
    Spec,
    Plan,
    Implement,
    Checks,
    Pr,
    Merged,
}

pub(crate) const COLUMNS: [Column; 7] = [Column::Todo, Column::Spec, Column::Plan, Column::Implement, Column::Checks, Column::Pr, Column::Merged];

impl Column {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Column::Todo => "To do",
            Column::Spec => "Spec",
            Column::Plan => "Plan",
            Column::Implement => "Implement",
            Column::Checks => "Checks",
            Column::Pr => "PR review",
            Column::Merged => "Merged",
        }
    }

    /// Drawn as a narrow rail until it is opened.
    pub(crate) fn rail(self) -> bool {
        matches!(self, Column::Todo | Column::Merged)
    }

    /// You decide here.
    pub(crate) fn yours(self) -> bool {
        matches!(self, Column::Plan | Column::Pr)
    }
}

/// Where the Factory layout puts a ticket: the station pocketd set, or one read from its state, its team and its PR.
pub(crate) fn column(t: &Ticket, team: Option<&Team>, pr: Option<&Pr>) -> Column {
    match t.station {
        Station::Spec => return Column::Spec,
        Station::Plan => return Column::Plan,
        Station::Implement => return Column::Implement,
        Station::Checks => return Column::Checks,
        Station::Pr => return Column::Pr,
        Station::Merged => return Column::Merged,
        Station::None => {}
    }
    match t.state {
        State::Todo | State::Queued => Column::Todo,
        State::Closed => Column::Merged,
        State::Live => match (team.map(|t| t.state.as_str()), pr.map(|p| p.state)) {
            (Some("planning" | PROPOSED), _) => Column::Plan,
            (Some(RUNNING | "review"), _) => Column::Implement,
            (_, Some(PrState::Open)) => Column::Pr,
            (_, Some(PrState::Merged)) => Column::Merged,
            _ => Column::Implement,
        },
    }
}

/// Whether the Factory layout shows the ticket. One closed without a merge left the line.
pub(crate) fn on_line(t: &Ticket) -> bool {
    t.state != State::Closed || t.closed_why == ClosedWhy::Merged
}

/// What a ticket waits on you for: the gate pocketd set, or one read from its status and its team.
pub(crate) fn gate(t: &Ticket, team: Option<&Team>) -> Gate {
    if t.state != State::Live {
        return Gate::None;
    }
    match (t.gate, t.status) {
        (Gate::None, Status::NeedsYou) if team.is_some_and(|t| t.state == PROPOSED) => Gate::Plan,
        (Gate::None, Status::NeedsYou) => Gate::Ask,
        (gate, _) => gate,
    }
}

pub(crate) fn waits(t: &Ticket) -> bool {
    gate(t, None) != Gate::None
}

pub(crate) fn failed(t: &Ticket) -> bool {
    t.state == State::Live && t.status == Status::Failed
}

/// A ticket's place in its column: what waits on you, then what failed, then the rest, each oldest first.
pub(crate) fn rank(t: &Ticket) -> (u8, i64) {
    let group = match (waits(t), failed(t)) {
        (true, _) => 0,
        (_, true) => 1,
        _ => 2,
    };
    (group, t.changed_at)
}

/// Indices into the snapshot, per column of `COLUMNS`.
pub(crate) type Stations = [Vec<usize>; COLUMNS.len()];

/// The tickets the filters keep, per column and in `rank` order. Merged keeps its latest ones, newest first.
pub(crate) fn arrange<'a>(tickets: &[Ticket], filters: &Filters, team: impl Fn(&Ticket) -> Option<&'a Team>, pr: impl Fn(&Ticket) -> Option<&'a Pr>) -> Stations {
    let mut columns: Stations = Default::default();
    let keeps = filters.keeper();
    for (i, t) in tickets.iter().enumerate().filter(|(_, t)| on_line(t) && keeps(t)) {
        columns[column(t, team(t), pr(t)) as usize].push(i);
    }
    for (col, line) in COLUMNS.iter().zip(columns.iter_mut()) {
        if *col == Column::Merged {
            line.sort_by_key(|&i| Reverse(activity(&tickets[i])));
            line.truncate(FINISHED_SHOWN);
        } else {
            line.sort_by_key(|&i| rank(&tickets[i]));
        }
    }
    columns
}

/// The cards on the page in reading order: each column top to bottom, without a rail that is closed.
pub(crate) fn walk(columns: &Stations, open: &[bool; COLUMNS.len()], tickets: &[Ticket]) -> Vec<Slot> {
    let mut slots = Vec::new();
    for (column, line) in COLUMNS.iter().zip(columns) {
        if column.rail() && !open[*column as usize] {
            continue;
        }
        for (pos, &index) in line.iter().enumerate() {
            if let Some(t) = tickets.get(index) {
                slots.push(Slot { col: status_column(t), pos, index });
            }
        }
    }
    slots
}

/// The PR's number: what pocketd wrote on the ticket, else the PR the desktop found for its worktree. 0 when there is none.
pub(crate) fn pr_number(t: &Ticket, pr: Option<&Pr>) -> u32 {
    if t.pr > 0 { t.pr } else { pr.map_or(0, |p| p.number) }
}

/// The one line a card says in the Factory layout: what waits on you, or that it failed. `ask` is the pending command.
pub(crate) fn note(t: &Ticket, team: Option<&Team>, pr: Option<&Pr>, ask: Option<String>) -> Option<(String, Tone)> {
    let text = match gate(t, team) {
        Gate::Plan => team.map_or_else(|| "Plan ready".to_string(), |team| format!("Plan ready · {}", plural(team.tasks.len(), "task", "tasks"))),
        Gate::Ask => ask.map_or_else(|| "Ask".to_string(), |command| format!("Ask · {command}")),
        Gate::Pr => match pr_number(t, pr) {
            0 => "Ready to merge".to_string(),
            n => format!("Ready to merge · #{n}"),
        },
        Gate::None if t.station == Station::Checks => return super::checks::checks_note(t),
        Gate::None if failed(t) && t.why.is_empty() => return Some(("Failed".to_string(), Tone::Failed)),
        Gate::None if failed(t) => return Some((format!("Failed · {}", t.why), Tone::Failed)),
        Gate::None => return None,
    };
    Some((text, Tone::Asking))
}

/// The Ask that holds the ticket: its lead's, or one of its team's workers'.
pub(crate) fn asker<'a>(t: &Ticket, team: Option<&Team>, pending: &'a [Permission]) -> Option<&'a Permission> {
    pending.iter().find(|p| p.agent_id == t.agent_id || team.is_some_and(|team| team.tasks.iter().any(|task| task.worker == p.agent_id)))
}

/// The tasks done out of the plan's, as `2/3`, once the team runs.
pub(crate) fn progress(team: &Team) -> Option<String> {
    let started = !team.tasks.is_empty() && !matches!(team.state.as_str(), "planning" | PROPOSED);
    started.then(|| format!("{}/{}", task_counts(&team.tasks).done, team.tasks.len()))
}

pub(crate) const RAIL_W: f32 = 44.;
pub(crate) const RAIL_OPEN: f32 = 232.;
pub(crate) const COL_MIN: f32 = 196.;
pub(crate) const COL_MAX: f32 = 248.;
pub(crate) const GAP: f32 = 12.;
/// The space on each side of the columns' row.
pub(crate) const PAD: f32 = 12.;

/// Each column's width on a page `available` wide. The middle columns share what the rails, the gaps and the padding leave.
pub(crate) fn widths(available: f32, open: &[bool; COLUMNS.len()]) -> [f32; COLUMNS.len()] {
    let rail = |c: Column| if open[c as usize] { RAIL_OPEN } else { RAIL_W };
    let rails: f32 = COLUMNS.into_iter().filter(|c| c.rail()).map(rail).sum();
    let middle = COLUMNS.iter().filter(|c| !c.rail()).count() as f32;
    let each = ((available - 2. * PAD - (COLUMNS.len() - 1) as f32 * GAP - rails) / middle).floor().clamp(COL_MIN, COL_MAX);
    COLUMNS.map(|c| if c.rail() { rail(c) } else { each })
}

/// What a filter chip of the Factory layout keeps.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Needs {
    You,
    Ask,
    Failed,
}

impl Needs {
    pub(crate) const ALL: [Needs; 3] = [Needs::You, Needs::Ask, Needs::Failed];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Needs::You => "needs you",
            Needs::Ask => "ask",
            Needs::Failed => "failed",
        }
    }

    pub(crate) fn keeps(self, t: &Ticket) -> bool {
        match self {
            Needs::You => waits(t),
            Needs::Ask => gate(t, None) == Gate::Ask,
            Needs::Failed => failed(t),
        }
    }

    /// The chip that is on after a click on `clicked`: a second click on the same chip clears it.
    pub(crate) fn toggled(current: Option<Needs>, clicked: Needs) -> Option<Needs> {
        (current != Some(clicked)).then_some(clicked)
    }
}

/// What the chips say, over every ticket whatever the filters hide.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) struct Counts {
    pub(crate) you: usize,
    pub(crate) ask: usize,
    pub(crate) failed: usize,
    /// The tickets that hold a slot.
    pub(crate) slots: usize,
}

impl Counts {
    pub(crate) fn of(&self, needs: Needs) -> usize {
        match needs {
            Needs::You => self.you,
            Needs::Ask => self.ask,
            Needs::Failed => self.failed,
        }
    }
}

pub(crate) fn counts(tickets: &[Ticket]) -> Counts {
    let n = |needs: Needs| tickets.iter().filter(|t| needs.keeps(t)).count();
    Counts { you: n(Needs::You), ask: n(Needs::Ask), failed: n(Needs::Failed), slots: busy(tickets) }
}

/// What a card names the ticket's work by when the desktop holds no PR for it: the PR number pocketd wrote, else the branch.
pub(crate) fn tag(t: &Ticket, branch: Option<&str>) -> Option<String> {
    if t.pr > 0 { Some(format!("#{}", t.pr)) } else { branch.map(str::to_string) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::board::logic::Col;
    use crate::desktop::chrome::RAIL;
    use agents::TeamTask;

    fn live(status: Status) -> Ticket {
        Ticket { state: State::Live, status, agent_id: "lead".into(), ..Ticket::sample() }
    }

    fn team(state: &str) -> Team {
        Team { lead: "lead".into(), state: state.into(), ..Team::default() }
    }

    fn pr(state: PrState) -> Pr {
        Pr { number: 203, state, ..Pr::default() }
    }

    #[test]
    fn a_station_set_by_pocketd_names_the_column() {
        let stations = [(Station::Spec, Column::Spec), (Station::Plan, Column::Plan), (Station::Implement, Column::Implement), (Station::Checks, Column::Checks), (Station::Pr, Column::Pr), (Station::Merged, Column::Merged)];
        for (station, want) in stations {
            let t = Ticket { station, ..live(Status::Working) };
            assert_eq!(column(&t, Some(&team("running")), Some(&pr(PrState::Open))), want);
        }
    }

    #[test]
    fn a_ticket_without_a_station_takes_its_column_from_its_team_and_its_pr() {
        let t = live(Status::Working);
        assert_eq!(column(&t, Some(&team("planning")), None), Column::Plan);
        assert_eq!(column(&t, Some(&team("proposed")), None), Column::Plan);
        assert_eq!(column(&t, Some(&team("running")), None), Column::Implement);
        assert_eq!(column(&t, Some(&team("review")), Some(&pr(PrState::Open))), Column::Implement);
        assert_eq!(column(&t, None, Some(&pr(PrState::Open))), Column::Pr);
        assert_eq!(column(&t, Some(&team("merged")), Some(&pr(PrState::Open))), Column::Pr);
        assert_eq!(column(&t, None, Some(&pr(PrState::Merged))), Column::Merged);
        assert_eq!(column(&t, None, Some(&pr(PrState::Closed))), Column::Implement);
        assert_eq!(column(&t, None, None), Column::Implement);
    }

    #[test]
    fn a_queued_ticket_sits_in_to_do() {
        let queued = Ticket { state: State::Queued, ..Ticket::sample() };
        assert_eq!(column(&queued, None, None), Column::Todo);
        assert_eq!(column(&Ticket::sample(), None, None), Column::Todo);
    }

    #[test]
    fn a_ticket_closed_as_merged_sits_in_merged_and_any_other_closed_ticket_has_no_column() {
        let merged = Ticket { state: State::Closed, closed_why: ClosedWhy::Merged, ..Ticket::sample() };
        let closed = Ticket { state: State::Closed, closed_why: ClosedWhy::Closed, station: Station::Checks, ..Ticket::sample() };
        assert_eq!((on_line(&merged), column(&merged, None, None)), (true, Column::Merged));
        assert!(!on_line(&closed));
        assert!(on_line(&live(Status::Working)) && on_line(&Ticket::sample()));
    }

    #[test]
    fn the_columns_keep_their_order_with_a_rail_at_each_end_and_you_on_the_two_gates() {
        assert_eq!(COLUMNS.map(Column::name), ["To do", "Spec", "Plan", "Implement", "Checks", "PR review", "Merged"]);
        assert_eq!(COLUMNS.map(Column::rail), [true, false, false, false, false, false, true]);
        assert_eq!(COLUMNS.map(Column::yours), [false, false, true, false, false, true, false]);
    }

    #[test]
    fn a_gate_set_by_pocketd_is_the_gate() {
        for g in [Gate::Plan, Gate::Pr, Gate::Ask] {
            assert_eq!(gate(&Ticket { gate: g, ..live(Status::NeedsYou) }, None), g);
        }
        assert_eq!(gate(&Ticket { gate: Gate::Pr, ..Ticket::sample() }, None), Gate::None, "only a live ticket waits");
    }

    #[test]
    fn a_ticket_without_a_gate_that_needs_you_waits_on_its_plan_or_on_an_ask() {
        let t = live(Status::NeedsYou);
        assert_eq!(gate(&t, Some(&team("proposed"))), Gate::Plan);
        assert_eq!(gate(&t, Some(&team("running"))), Gate::Ask);
        assert_eq!(gate(&t, None), Gate::Ask);
        assert_eq!(gate(&live(Status::Working), Some(&team("proposed"))), Gate::None);
        assert!(waits(&t) && !waits(&live(Status::Failed)));
        assert!(failed(&live(Status::Failed)) && !failed(&Ticket { status: Status::Failed, ..Ticket::sample() }));
    }

    fn at(t: Ticket, changed_at: i64) -> Ticket {
        Ticket { changed_at, ..t }
    }

    #[test]
    fn rank_puts_a_gate_before_a_failure_before_the_rest_and_the_oldest_first() {
        assert!(rank(&at(live(Status::NeedsYou), 9)) < rank(&at(live(Status::Failed), 1)));
        assert!(rank(&at(live(Status::Failed), 9)) < rank(&at(live(Status::Working), 1)));
        assert!(rank(&at(live(Status::Working), 1)) < rank(&at(live(Status::Working), 2)));
    }

    #[test]
    fn a_column_lists_what_waits_on_you_then_what_failed_then_the_oldest_first() {
        let tickets = [at(live(Status::Working), 30), at(live(Status::Failed), 40), at(live(Status::NeedsYou), 50), at(live(Status::Working), 10), at(live(Status::NeedsYou), 20)];
        let columns = arrange(&tickets, &Filters::default(), |_| None, |_| None);
        assert_eq!(columns[Column::Implement as usize], [4, 2, 1, 3, 0]);
    }

    #[test]
    fn every_ticket_on_the_line_lands_in_exactly_one_column() {
        let tickets = [
            Ticket::sample(),
            Ticket { state: State::Queued, ..Ticket::sample() },
            Ticket { station: Station::Spec, ..live(Status::Working) },
            Ticket { station: Station::Checks, ..live(Status::Failed) },
            live(Status::Working),
            Ticket { state: State::Closed, closed_why: ClosedWhy::Merged, ..Ticket::sample() },
            Ticket { state: State::Closed, ..Ticket::sample() },
        ];
        let columns = arrange(&tickets, &Filters::default(), |_| None, |_| None);
        assert_eq!(columns, [vec![0, 1], vec![2], vec![], vec![4], vec![3], vec![], vec![5]]);
    }

    #[test]
    fn a_ticket_is_placed_by_the_team_and_the_pr_that_the_lookups_find() {
        let (proposed, open) = (team("proposed"), pr(PrState::Open));
        let tickets = [live(Status::NeedsYou), Ticket { agent_id: "solo".into(), worktree: "/w/pr".into(), ..live(Status::Working) }];
        let columns = arrange(&tickets, &Filters::default(), |t| (t.agent_id == "lead").then_some(&proposed), |t| (t.worktree == "/w/pr").then_some(&open));
        assert_eq!(columns[Column::Plan as usize], [0]);
        assert_eq!(columns[Column::Pr as usize], [1]);
    }

    #[test]
    fn merged_shows_its_latest_fifty_newest_first() {
        let tickets: Vec<Ticket> = (1..=60).map(|n| Ticket { state: State::Closed, closed_why: ClosedWhy::Merged, closed_at: n, ..Ticket::sample() }).collect();
        let columns = arrange(&tickets, &Filters::default(), |_| None, |_| None);
        let merged = &columns[Column::Merged as usize];
        assert_eq!((merged.len(), merged[0], merged[49]), (50, 59, 10));
    }

    #[test]
    fn the_search_hides_a_ticket_from_its_column() {
        let tickets = [Ticket { title: "Fix scroll".into(), ..live(Status::Working) }, Ticket { title: "Add search".into(), ..live(Status::Working) }];
        let found = Filters { query: "scroll".into(), ..Filters::default() };
        assert_eq!(arrange(&tickets, &found, |_| None, |_| None)[Column::Implement as usize], [0]);
    }

    #[test]
    fn the_walk_reads_each_column_top_to_bottom_and_skips_a_closed_rail() {
        let tickets = [Ticket::sample(), Ticket { station: Station::Spec, ..live(Status::Working) }, live(Status::NeedsYou), Ticket { state: State::Closed, closed_why: ClosedWhy::Merged, ..Ticket::sample() }];
        let columns = arrange(&tickets, &Filters::default(), |_| None, |_| None);
        assert_eq!(walk(&columns, &[false; COLUMNS.len()], &tickets), [Slot { col: Col::Working, pos: 0, index: 1 }, Slot { col: Col::NeedsYou, pos: 0, index: 2 }]);
        let mut open = [false; COLUMNS.len()];
        open[Column::Todo as usize] = true;
        assert_eq!(walk(&columns, &open, &tickets).first(), Some(&Slot { col: Col::Todo, pos: 0, index: 0 }));
    }

    fn planned(n: usize, state: &str) -> Team {
        Team { tasks: (0..n).map(|i| TeamTask { id: format!("t{i}"), state: "planned".into(), ..TeamTask::default() }).collect(), ..team(state) }
    }

    fn ask(agent: &str, command: &str) -> Permission {
        Permission { agent_id: agent.into(), detail: agents::Detail { kind: "shell".into(), command: command.into(), ..Default::default() }, ..Default::default() }
    }

    #[test]
    fn a_ticket_at_the_plan_gate_reads_plan_ready_with_its_task_count() {
        let t = Ticket { station: Station::Plan, gate: Gate::Plan, ..live(Status::NeedsYou) };
        assert_eq!(note(&t, Some(&planned(3, "proposed")), None, None), Some(("Plan ready · 3 tasks".into(), Tone::Asking)));
        assert_eq!(note(&t, Some(&planned(1, "proposed")), None, None), Some(("Plan ready · 1 task".into(), Tone::Asking)));
        assert_eq!(note(&t, None, None, None), Some(("Plan ready".into(), Tone::Asking)));
    }

    #[test]
    fn a_ticket_with_an_ask_reads_ask_with_the_command() {
        let t = Ticket { station: Station::Implement, gate: Gate::Ask, ..live(Status::NeedsYou) };
        assert_eq!(note(&t, None, None, Some("cargo test -p daemon".into())), Some(("Ask · cargo test -p daemon".into(), Tone::Asking)));
        assert_eq!(note(&t, None, None, None), Some(("Ask".into(), Tone::Asking)));
    }

    #[test]
    fn a_ticket_at_the_pr_gate_reads_ready_to_merge_with_the_pr_number() {
        let t = Ticket { station: Station::Pr, gate: Gate::Pr, pr: 203, ..live(Status::NeedsYou) };
        assert_eq!(note(&t, None, None, None), Some(("Ready to merge · #203".into(), Tone::Asking)));
        let unnumbered = Ticket { pr: 0, ..t };
        assert_eq!(note(&unnumbered, None, Some(&pr(PrState::Open)), None), Some(("Ready to merge · #203".into(), Tone::Asking)));
        assert_eq!(note(&unnumbered, None, None, None), Some(("Ready to merge".into(), Tone::Asking)));
    }

    #[test]
    fn a_failed_ticket_reads_failed_with_the_reason() {
        let t = Ticket { station: Station::Checks, why: "cargo test failed twice".into(), ..live(Status::Failed) };
        assert_eq!(note(&t, None, None, None), Some(("Failed · cargo test failed twice".into(), Tone::Failed)));
        assert_eq!(note(&live(Status::Failed), None, None, None), Some(("Failed".into(), Tone::Failed)));
    }

    #[test]
    fn a_card_at_checks_says_its_progress_then_only_what_failed() {
        let t = Ticket { station: Station::Checks, why: "Checks · 2/3".into(), ..live(Status::Working) };
        assert_eq!(note(&t, None, None, None), Some(("Checks · 2/3".into(), Tone::Dim)));
        let failed = Ticket { station: Station::Checks, why: "cargo test · Auto-fix 2 of 2 used".into(), ..live(Status::Failed) };
        assert_eq!(note(&failed, None, None, None), Some(("Failed · cargo test".into(), Tone::Failed)));
    }

    #[test]
    fn a_card_says_one_thing_and_a_working_ticket_says_nothing() {
        let both = Ticket { gate: Gate::Ask, why: "boom".into(), ..live(Status::Failed) };
        assert_eq!(note(&both, None, None, None), Some(("Ask".into(), Tone::Asking)));
        assert_eq!(note(&live(Status::Working), Some(&planned(3, "running")), Some(&pr(PrState::Open)), None), None);
        assert_eq!(note(&Ticket::sample(), None, None, None), None);
    }

    #[test]
    fn the_ask_of_a_ticket_is_its_leads_or_one_of_its_workers() {
        let pending = [ask("other", "ls"), ask("w-t1", "cargo test -p daemon"), ask("lead", "git status")];
        let mut running = planned(2, "running");
        running.tasks[1].worker = "w-t1".into();
        let t = live(Status::NeedsYou);
        assert_eq!(asker(&t, Some(&running), &pending).map(|p| p.parts().1), Some("cargo test -p daemon".to_string()));
        assert_eq!(asker(&t, None, &pending).map(|p| p.agent_id.as_str()), Some("lead"));
        assert!(asker(&Ticket { agent_id: "solo".into(), ..t }, None, &pending).is_none());
    }

    #[test]
    fn the_sub_task_count_shows_once_the_team_runs() {
        let mut running = planned(3, "running");
        (running.tasks[0].state, running.tasks[1].state) = ("done".into(), "merged".into());
        assert_eq!(progress(&running), Some("2/3".to_string()));
        assert_eq!(progress(&planned(3, "proposed")), None);
        assert_eq!(progress(&planned(0, "running")), None);
    }

    const CLOSED: [bool; COLUMNS.len()] = [false; COLUMNS.len()];

    /// The width the columns take with their gaps and the row's padding.
    fn taken(widths: [f32; COLUMNS.len()]) -> f32 {
        widths.iter().sum::<f32>() + (COLUMNS.len() - 1) as f32 * GAP + 2. * PAD
    }

    #[test]
    fn all_seven_columns_fit_a_1280px_window_beside_the_collapsed_sidebar() {
        let page = 1280. - RAIL;
        let widths = widths(page, &CLOSED);
        assert_eq!(widths, [44., 208., 208., 208., 208., 208., 44.]);
        assert!(taken(widths) <= page);
    }

    #[test]
    fn a_middle_column_is_never_narrower_than_196px_so_a_small_page_scrolls_sideways() {
        let page = 1280. - 272.;
        let widths = widths(page, &CLOSED);
        assert_eq!(widths[Column::Implement as usize], COL_MIN);
        assert!(taken(widths) > page);
        assert_eq!(taken(widths), 1164., "the narrowest page that needs no scroll");
    }

    #[test]
    fn a_middle_column_is_never_wider_than_248px() {
        assert_eq!(widths(1424., &CLOSED)[Column::Spec as usize], COL_MAX);
        assert_eq!(widths(2560., &CLOSED)[Column::Spec as usize], COL_MAX);
    }

    #[test]
    fn an_open_rail_takes_232px_from_the_middle_columns() {
        let mut open = CLOSED;
        open[Column::Todo as usize] = true;
        let widths = widths(1384., &open);
        assert_eq!((widths[Column::Todo as usize], widths[Column::Plan as usize], widths[Column::Merged as usize]), (RAIL_OPEN, 202., RAIL_W));
        assert!(taken(widths) <= 1384.);
    }

    #[test]
    fn the_chips_count_what_waits_on_you_what_asks_and_what_failed() {
        let tickets = [
            Ticket { gate: Gate::Plan, ..live(Status::NeedsYou) },
            Ticket { gate: Gate::Ask, ..live(Status::NeedsYou) },
            Ticket { gate: Gate::Pr, ..live(Status::NeedsYou) },
            live(Status::NeedsYou),
            live(Status::Failed),
            live(Status::Working),
            live(Status::Starting),
            Ticket::sample(),
        ];
        let c = counts(&tickets);
        assert_eq!(c, Counts { you: 4, ask: 2, failed: 1, slots: 2 });
        assert_eq!(Needs::ALL.map(|n| c.of(n)), [4, 2, 1]);
        assert_eq!(Needs::ALL.map(Needs::name), ["needs you", "ask", "failed"]);
    }

    #[test]
    fn a_second_click_on_a_chip_clears_it_and_another_chip_replaces_it() {
        assert_eq!(Needs::toggled(None, Needs::You), Some(Needs::You));
        assert_eq!(Needs::toggled(Some(Needs::You), Needs::You), None);
        assert_eq!(Needs::toggled(Some(Needs::You), Needs::Failed), Some(Needs::Failed));
    }

    #[test]
    fn a_chip_keeps_only_its_tickets_in_the_columns() {
        let tickets = [Ticket { station: Station::Plan, gate: Gate::Plan, ..live(Status::NeedsYou) }, Ticket { station: Station::Plan, ..live(Status::Working) }, Ticket { station: Station::Checks, ..live(Status::Failed) }];
        let only = |needs| arrange(&tickets, &Filters { needs: Some(needs), ..Filters::default() }, |_| None, |_| None);
        assert_eq!(only(Needs::You), [vec![], vec![], vec![0], vec![], vec![], vec![], vec![]]);
        assert_eq!(only(Needs::Ask), Stations::default());
        assert_eq!(only(Needs::Failed)[Column::Checks as usize], [2]);
    }

    #[test]
    fn a_card_names_its_pr_and_else_its_branch() {
        assert_eq!(tag(&Ticket { pr: 203, ..live(Status::Working) }, Some("feat/x")), Some("#203".to_string()));
        assert_eq!(tag(&live(Status::Working), Some("feat/x")), Some("feat/x".to_string()));
        assert_eq!(tag(&live(Status::Working), None), None);
    }
}
