use agents::tickets::{ClosedWhy, State, Status, Ticket};
use std::cmp::Reverse;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Col {
    Todo,
    Queued,
    Working,
    NeedsYou,
    Done,
    InReview,
    Merged,
    Closed,
}

pub(crate) const COLS: [Col; 8] = [Col::Todo, Col::Queued, Col::Working, Col::NeedsYou, Col::Done, Col::InReview, Col::Merged, Col::Closed];

impl Col {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Col::Todo => "To do",
            Col::Queued => "Queued",
            Col::Working => "Working",
            Col::NeedsYou => "Needs you",
            Col::Done => "Done",
            Col::InReview => "In review",
            Col::Merged => "Merged",
            Col::Closed => "Closed",
        }
    }

    /// Only tickets without a session can be dragged: the rest move with their session.
    pub(crate) fn lifts(self) -> bool {
        matches!(self, Col::Todo | Col::Queued)
    }

    /// Whether a ticket lifted from `from` can land here.
    pub(crate) fn takes(self, from: Col) -> bool {
        matches!((from, self), (Col::Todo, Col::Queued | Col::Working) | (Col::Queued, Col::Todo))
    }

    fn state(self) -> Option<State> {
        match self {
            Col::Todo => Some(State::Todo),
            Col::Queued => Some(State::Queued),
            _ => None,
        }
    }

    /// Its tickets are over and done with.
    pub(crate) fn finished(self) -> bool {
        matches!(self, Col::Merged | Col::Closed)
    }
}

pub(crate) fn column(t: &Ticket) -> Col {
    match t.state {
        State::Todo => Col::Todo,
        State::Queued => Col::Queued,
        State::Closed if t.closed_why == ClosedWhy::Merged => Col::Merged,
        State::Closed => Col::Closed,
        State::Live => match t.status {
            Status::NeedsYou | Status::Failed | Status::Stopped => Col::NeedsYou,
            Status::Starting | Status::Working => Col::Working,
            Status::Done => Col::Done,
            Status::Idle | Status::None => Col::InReview,
        },
    }
}

/// The tickets counted against the Board's limit.
pub(crate) fn busy(tickets: &[Ticket]) -> usize {
    tickets.iter().filter(|t| t.state == State::Live && matches!(t.status, Status::Starting | Status::Working | Status::NeedsYou)).count()
}

/// Said in the dialog when a new ticket would not start right away.
pub(crate) fn wait_hint(busy: usize, limit: usize) -> Option<String> {
    (busy >= limit).then(|| format!("{limit} of {limit} working, so it will wait in Queued"))
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) enum Order {
    #[default]
    Activity,
    Created,
    Name,
}

#[derive(Clone, PartialEq, Debug, Default)]
pub(crate) struct Filters {
    pub(crate) agent: Option<String>,
    pub(crate) project: Option<String>,
    pub(crate) query: String,
}

impl Filters {
    fn keeps(&self, t: &Ticket) -> bool {
        let query = self.query.trim().to_lowercase();
        self.agent.as_ref().is_none_or(|a| *a == t.provider)
            && self.project.as_ref().is_none_or(|p| *p == t.folder)
            && (query.is_empty() || t.title.to_lowercase().contains(&query) || crate::util::basename(&t.worktree).to_lowercase().contains(&query))
    }
}

pub(crate) fn activity(t: &Ticket) -> i64 {
    if t.closed_at > 0 { t.closed_at } else { t.changed_at }
}

/// How many finished tickets a column shows, the latest ones.
const FINISHED_SHOWN: usize = 50;

/// Indices into the snapshot, per column of `COLS`.
pub(crate) type Columns = [Vec<usize>; COLS.len()];

/// To do and Queued keep the order they were put in; the others follow `order`.
pub(crate) fn arrange(tickets: &[Ticket], filters: &Filters, order: Order) -> Columns {
    let mut columns: Columns = Default::default();
    for (i, t) in tickets.iter().enumerate().filter(|(_, t)| filters.keeps(t)) {
        columns[column(t) as usize].push(i);
    }
    for (col, line) in COLS.iter().zip(columns.iter_mut()) {
        if col.finished() {
            line.sort_by_key(|&i| Reverse(activity(&tickets[i])));
            line.truncate(FINISHED_SHOWN);
        }
        match order {
            _ if col.lifts() => line.sort_by_key(|&i| tickets[i].order),
            Order::Activity => line.sort_by_key(|&i| Reverse(activity(&tickets[i]))),
            Order::Created => line.sort_by_key(|&i| Reverse(tickets[i].created_at)),
            Order::Name => line.sort_by_cached_key(|&i| tickets[i].title.to_lowercase()),
        }
    }
    columns
}

pub(crate) fn shown_cols(show_closed: bool) -> impl Iterator<Item = Col> {
    COLS.into_iter().filter(move |c| show_closed || *c != Col::Closed)
}

/// Where a ticket is shown: its column, its place there, and its index in the snapshot.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Slot {
    pub(crate) col: Col,
    pub(crate) pos: usize,
    pub(crate) index: usize,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Row {
    Head(Col),
    Ticket(Slot),
}

/// The same buckets as the board, as sections; an empty one is left out.
pub(crate) fn rows(columns: &Columns, show_closed: bool) -> Vec<Row> {
    let mut out = Vec::new();
    for col in shown_cols(show_closed) {
        let line = &columns[col as usize];
        if !line.is_empty() {
            out.push(Row::Head(col));
            out.extend(line.iter().enumerate().map(|(pos, &index)| Row::Ticket(Slot { col, pos, index })));
        }
    }
    out
}

/// A queued ticket's place in line, from 0.
pub(crate) fn in_line(pos: usize) -> String {
    let n = pos + 1;
    match n {
        1 => "Next".into(),
        2 => "2nd in line".into(),
        3 => "3rd in line".into(),
        _ => format!("{n}th in line"),
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Action {
    Start,
    Edit,
    Delete,
    MoveBack,
    Open,
    StartAgain,
    CopyBranch,
    Close,
    Restore,
    DeleteWorktree,
    Separator,
}

fn ended(t: &Ticket) -> bool {
    t.status == Status::Stopped || t.status == Status::Failed && t.agent_id.is_empty()
}

/// What of a ticket's session and Worktree is still there.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) struct Reach {
    pub(crate) session: bool,
    pub(crate) tree: bool,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Opens {
    Edit,
    Session,
    Worktree,
}

/// Where clicking a ticket goes: its dialog before it starts, then its session, else its Worktree.
pub(crate) fn opens(t: &Ticket, reach: Reach) -> Option<Opens> {
    match column(t) {
        Col::Todo | Col::Queued => Some(Opens::Edit),
        Col::Closed => None,
        _ if reach.session && t.status != Status::Stopped => Some(Opens::Session),
        _ if reach.tree => Some(Opens::Worktree),
        _ => None,
    }
}

/// What a ticket's menu offers.
pub(crate) fn actions(t: &Ticket, reach: Reach) -> Vec<Action> {
    use Action::*;
    let open = opens(t, reach).map(|_| Open);
    let copy = reach.tree.then_some(CopyBranch);
    let delete_tree = reach.tree.then_some([Separator, DeleteWorktree]).into_iter().flatten();
    match column(t) {
        Col::Todo => vec![Start, Edit, Separator, Delete],
        Col::Queued => vec![MoveBack, Edit, Separator, Delete],
        Col::Working => open.into_iter().chain(copy).collect(),
        Col::NeedsYou if ended(t) => open.into_iter().chain([StartAgain]).chain(copy).chain([Separator, Close]).collect(),
        Col::NeedsYou | Col::Done | Col::InReview => open.into_iter().chain(copy).chain([Separator, Close]).collect(),
        Col::Merged => copy.into_iter().chain(delete_tree).collect(),
        Col::Closed => {
            let restorable = t.closed_why == ClosedWhy::Closed && (reach.tree || t.worktree.is_empty());
            restorable.then_some(Restore).into_iter().chain(copy).chain(delete_tree).collect()
        }
    }
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Landing {
    Start,
    Move { column: State, before: Option<String> },
}

/// What dropping the ticket `id` lifted from `from` on `to` does, before the ticket `before` when it landed on one.
pub(crate) fn landing(id: &str, from: Col, to: Col, before: Option<&str>) -> Option<Landing> {
    if before == Some(id) {
        return None;
    }
    if from == to {
        return to.state().filter(|_| from.lifts() && before.is_some()).map(|column| Landing::Move { column, before: before.map(str::to_string) });
    }
    if !to.takes(from) {
        return None;
    }
    match to.state() {
        Some(column) if before.is_some() || to == Col::Todo => Some(Landing::Move { column, before: before.map(str::to_string) }),
        _ => Some(Landing::Start),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ticket(id: &str, state: State, status: Status) -> Ticket {
        Ticket {
            id: id.into(),
            title: id.into(),
            prompt: "do".into(),
            provider: "claude".into(),
            folder: "/repo".into(),
            base: String::new(),
            state,
            order: 0,
            status,
            why: String::new(),
            agent_id: "a".into(),
            terminal_id: String::new(),
            worktree: String::new(),
            created_at: 0,
            changed_at: 0,
            closed_at: 0,
            closed_why: ClosedWhy::default(),
        }
    }

    fn live(status: Status) -> Ticket {
        ticket("t", State::Live, status)
    }

    #[test]
    fn a_ticket_sits_in_the_column_of_its_session() {
        let cases = [
            (Status::Starting, Col::Working),
            (Status::Working, Col::Working),
            (Status::NeedsYou, Col::NeedsYou),
            (Status::Failed, Col::NeedsYou),
            (Status::Stopped, Col::NeedsYou),
            (Status::Done, Col::Done),
            (Status::Idle, Col::InReview),
        ];
        for (status, want) in cases {
            assert_eq!(column(&live(status)), want, "{status:?}");
        }
    }

    #[test]
    fn a_ticket_closed_by_its_merge_goes_to_merged_and_any_other_to_closed() {
        let mut t = ticket("t", State::Closed, Status::Idle);
        t.closed_why = ClosedWhy::Merged;
        assert_eq!(column(&t), Col::Merged);
        t.closed_why = ClosedWhy::Closed;
        assert_eq!(column(&t), Col::Closed);
    }

    #[test]
    fn only_live_tickets_with_work_in_hand_count_against_the_limit() {
        let tickets = [live(Status::Working), live(Status::NeedsYou), live(Status::Starting), live(Status::Done), live(Status::Idle), ticket("q", State::Queued, Status::None), ticket("c", State::Closed, Status::Working)];
        assert_eq!(busy(&tickets), 3);
    }

    #[test]
    fn the_dialog_warns_only_when_every_slot_is_taken() {
        assert_eq!(wait_hint(3, 4), None);
        assert_eq!(wait_hint(4, 4).as_deref(), Some("4 of 4 working, so it will wait in Queued"));
    }

    #[test]
    fn to_do_keeps_its_own_order_whatever_the_ordering() {
        let mut a = ticket("a", State::Todo, Status::None);
        let mut b = ticket("b", State::Todo, Status::None);
        (a.order, b.order, a.changed_at, b.changed_at) = (2, 1, 9, 1);
        assert_eq!(arrange(&[a, b], &Filters::default(), Order::Activity)[Col::Todo as usize], vec![1, 0]);
    }

    #[test]
    fn other_columns_follow_the_ordering() {
        let mut a = live(Status::Done);
        let mut b = live(Status::Done);
        (a.title, b.title, a.changed_at, b.changed_at, a.created_at, b.created_at) = ("Zed".into(), "ant".into(), 1, 9, 9, 1);
        let tickets = [a, b];
        let done = |o| arrange(&tickets, &Filters::default(), o)[Col::Done as usize].clone();
        assert_eq!(done(Order::Activity), vec![1, 0]);
        assert_eq!(done(Order::Created), vec![0, 1]);
        assert_eq!(done(Order::Name), vec![1, 0]);
    }

    #[test]
    fn filters_narrow_by_agent_project_and_text() {
        let mut a = ticket("Fix the login", State::Todo, Status::None);
        let mut b = ticket("Other", State::Todo, Status::None);
        (a.provider, b.folder, b.worktree) = ("codex".into(), "/other".into(), "/worktrees/login-page".into());
        let tickets = [a, b];
        let got = |f: Filters| arrange(&tickets, &f, Order::Activity)[Col::Todo as usize].clone();
        assert_eq!(got(Filters { agent: Some("codex".into()), ..Default::default() }), vec![0]);
        assert_eq!(got(Filters { project: Some("/other".into()), ..Default::default() }), vec![1]);
        assert_eq!(got(Filters { query: " LOGIN ".into(), ..Default::default() }), vec![0, 1]);
        assert_eq!(got(Filters { query: "worktrees".into(), ..Default::default() }), Vec::<usize>::new());
    }

    #[test]
    fn a_finished_column_keeps_only_the_latest_fifty() {
        let tickets: Vec<_> = (0..60).map(|n| {
            let mut t = ticket("c", State::Closed, Status::Idle);
            t.closed_at = n + 1;
            t
        }).collect();
        let closed = arrange(&tickets, &Filters::default(), Order::Name)[Col::Closed as usize].clone();
        assert_eq!(closed.len(), 50);
        assert!(closed.iter().all(|&i| i >= 10));
    }

    #[test]
    fn the_list_skips_empty_sections_and_closed_unless_shown() {
        let tickets = [ticket("a", State::Todo, Status::None), ticket("c", State::Closed, Status::Idle)];
        let columns = arrange(&tickets, &Filters::default(), Order::Activity);
        assert_eq!(rows(&columns, false), vec![Row::Head(Col::Todo), Row::Ticket(Slot { col: Col::Todo, pos: 0, index: 0 })]);
        assert_eq!(rows(&columns, true).len(), 4);
    }

    #[test]
    fn a_queued_ticket_says_its_place() {
        assert_eq!([in_line(0), in_line(1), in_line(2), in_line(3)], ["Next", "2nd in line", "3rd in line", "4th in line"]);
    }

    #[test]
    fn only_unstarted_tickets_lift_and_only_to_where_they_may_go() {
        assert!(Col::Queued.takes(Col::Todo) && Col::Working.takes(Col::Todo) && Col::Todo.takes(Col::Queued));
        assert!(!Col::Done.takes(Col::Todo) && !Col::Working.takes(Col::Queued) && !Col::Todo.takes(Col::Working));
        assert!(!Col::Working.lifts() && Col::Todo.lifts());
    }

    #[test]
    fn dropping_starts_reorders_or_moves_back() {
        let before = Landing::Move { column: State::Todo, before: Some("x".into()) };
        assert_eq!(landing("t", Col::Todo, Col::Working, None), Some(Landing::Start));
        assert_eq!(landing("t", Col::Todo, Col::Queued, None), Some(Landing::Start));
        assert_eq!(landing("t", Col::Todo, Col::Todo, Some("x")), Some(before));
        assert_eq!(landing("t", Col::Queued, Col::Todo, None), Some(Landing::Move { column: State::Todo, before: None }));
        assert_eq!(landing("t", Col::Todo, Col::Queued, Some("x")), Some(Landing::Move { column: State::Queued, before: Some("x".into()) }));
        assert_eq!(landing("t", Col::Todo, Col::Todo, None), None);
        assert_eq!(landing("t", Col::Queued, Col::Working, None), None);
    }

    #[test]
    fn dropping_a_ticket_on_itself_does_nothing() {
        assert_eq!(landing("t", Col::Todo, Col::Todo, Some("t")), None);
    }

    const BOTH: Reach = Reach { session: true, tree: true };
    const TREE: Reach = Reach { session: false, tree: true };
    const NOTHING: Reach = Reach { session: false, tree: false };

    #[test]
    fn a_session_that_ended_offers_start_again_and_a_failed_live_one_does_not() {
        use Action::*;
        let stopped = live(Status::Stopped);
        assert_eq!(actions(&stopped, TREE), vec![Open, StartAgain, CopyBranch, Separator, Close]);
        assert_eq!(actions(&live(Status::Failed), BOTH), vec![Open, CopyBranch, Separator, Close]);
        let mut unlaunched = live(Status::Failed);
        unlaunched.agent_id.clear();
        assert_eq!(actions(&unlaunched, NOTHING), vec![StartAgain, Separator, Close]);
    }

    #[test]
    fn a_merged_ticket_without_its_worktree_offers_nothing() {
        let mut t = ticket("t", State::Closed, Status::Idle);
        t.closed_why = ClosedWhy::Merged;
        assert!(actions(&t, NOTHING).is_empty());
        assert_eq!(actions(&t, Reach { session: false, tree: false }), vec![]);
        assert_eq!(actions(&t, TREE), vec![Action::CopyBranch, Action::Separator, Action::DeleteWorktree]);
    }

    #[test]
    fn a_ticket_whose_worktree_is_gone_cannot_be_restored() {
        let mut t = ticket("t", State::Closed, Status::Idle);
        t.closed_why = ClosedWhy::Deleted;
        assert!(actions(&t, NOTHING).is_empty());
        t.closed_why = ClosedWhy::Closed;
        t.worktree = "/wt/gone".into();
        assert!(actions(&t, NOTHING).is_empty());
        t.worktree.clear();
        assert_eq!(actions(&t, NOTHING), vec![Action::Restore]);
    }

    #[test]
    fn a_click_goes_to_the_dialog_the_session_or_the_worktree() {
        assert_eq!(opens(&ticket("t", State::Todo, Status::None), NOTHING), Some(Opens::Edit));
        assert_eq!(opens(&live(Status::Working), BOTH), Some(Opens::Session));
        assert_eq!(opens(&live(Status::Stopped), BOTH), Some(Opens::Worktree));
        assert_eq!(opens(&live(Status::Done), TREE), Some(Opens::Worktree));
        assert_eq!(opens(&live(Status::Failed), NOTHING), None);
    }

    #[test]
    fn closed_tickets_are_not_clickable_but_merged_ones_are() {
        let mut t = ticket("t", State::Closed, Status::Idle);
        assert_eq!(opens(&t, BOTH), None);
        t.closed_why = ClosedWhy::Merged;
        assert_eq!(opens(&t, BOTH), Some(Opens::Session));
    }
}
