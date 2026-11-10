use agents::tickets::{ClosedWhy, State, Status, Ticket};
use std::cmp::Reverse;
use std::collections::HashSet;

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

    /// What it says while it holds nothing.
    pub(crate) fn hint(self) -> &'static str {
        match self {
            Col::Todo => "Write a ticket below, or press N.",
            Col::Queued => "Tickets wait here until a slot frees up.",
            Col::Working => "Tickets you start show up here.",
            Col::NeedsYou => "Questions from your agents land here.",
            Col::Done => "New results land here.",
            Col::InReview => "Results you've opened wait here.",
            Col::Merged => "Merged work ends up here.",
            Col::Closed => "Closed tickets.",
        }
    }
}

/// The priorities in the order menus and sorting put them: none comes last.
pub(crate) const PRIORITIES: [u8; 5] = [1, 2, 3, 4, 0];

pub(crate) fn priority_name(p: u8) -> &'static str {
    match p {
        1 => "Urgent",
        2 => "High",
        3 => "Medium",
        4 => "Low",
        _ => "No priority",
    }
}

fn priority_rank(p: u8) -> usize {
    PRIORITIES.iter().position(|x| *x == p).unwrap_or(PRIORITIES.len())
}

pub(crate) const LABELS: [&str; 4] = ["Bug", "Feature", "Docs", "Infra"];

/// A ticket's short ID, as `ANY-12`; none before it has a number.
pub(crate) fn ticket_key(project: &str, number: u64) -> Option<String> {
    (number > 0).then(|| {
        let prefix: String = project.chars().filter(|c| c.is_alphanumeric()).take(3).collect();
        format!("{}-{number}", prefix.to_uppercase())
    })
}

const MAX_TITLE: usize = 200;

/// The composer's text as a title, its first line, and a prompt, all of it.
pub(crate) fn split_draft(text: &str) -> (String, String) {
    let prompt = text.trim();
    let title = prompt.lines().next().unwrap_or_default().trim().chars().take(MAX_TITLE).collect();
    (title, prompt.to_string())
}

/// What a card shows of a ticket's details; all of it unless switched off in Display.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) struct Props {
    pub(crate) id: bool,
    pub(crate) priority: bool,
    pub(crate) labels: bool,
    pub(crate) project: bool,
    pub(crate) agent: bool,
    pub(crate) updated: bool,
}

impl Default for Props {
    fn default() -> Self {
        Self { id: true, priority: true, labels: true, project: true, agent: true, updated: true }
    }
}

/// A set of ticket choices being made: the composer's, or the ticket page's.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Surface {
    Composer,
    Page,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Field {
    Project,
    Agent,
    Priority,
    Labels,
    Base,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum FilterCat {
    Status,
    Agent,
    Project,
    Priority,
    Label,
}

impl FilterCat {
    pub(crate) const ALL: [FilterCat; 5] = [FilterCat::Status, FilterCat::Agent, FilterCat::Project, FilterCat::Priority, FilterCat::Label];

    pub(crate) fn name(self) -> &'static str {
        match self {
            FilterCat::Status => "Status",
            FilterCat::Agent => "Agent",
            FilterCat::Project => "Project",
            FilterCat::Priority => "Priority",
            FilterCat::Label => "Labels",
        }
    }
}

/// Which part of the Board's flow the page shows.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) enum Tab {
    #[default]
    All,
    Active,
    Todo,
}

impl Tab {
    pub(crate) const ALL: [Tab; 3] = [Tab::All, Tab::Active, Tab::Todo];

    pub(crate) fn name(self) -> &'static str {
        match self {
            Tab::All => "All tickets",
            Tab::Active => "Active",
            Tab::Todo => "To do",
        }
    }

    fn shows(self, col: Col) -> bool {
        match self {
            Tab::All => true,
            Tab::Active => !col.finished() && col != Col::Todo,
            Tab::Todo => col == Col::Todo,
        }
    }
}

fn on_page(col: Col, tab: Tab, show_closed: bool) -> bool {
    tab.shows(col) && (show_closed || col != Col::Closed)
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

/// Said in the composer when a new ticket would not start right away.
pub(crate) fn wait_hint(busy: usize, limit: usize) -> Option<String> {
    (busy >= limit).then(|| format!("{limit} of {limit} working, so it will wait in Queued"))
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) enum Order {
    #[default]
    Activity,
    Created,
    Priority,
    Name,
}

/// The values a filter field is set to: a ticket with any of them stays, or goes when `not`.
#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Choice<T> {
    pub(crate) values: Vec<T>,
    pub(crate) not: bool,
}

impl<T> Default for Choice<T> {
    fn default() -> Self {
        Self { values: Vec::new(), not: false }
    }
}

impl<T: PartialEq> Choice<T> {
    fn keeps(&self, has: impl Fn(&T) -> bool) -> bool {
        self.values.is_empty() || self.values.iter().any(has) != self.not
    }

    /// Adds or takes away `value`; a field left with none says nothing, so it forgets `not`.
    pub(crate) fn toggle(&mut self, value: T) {
        match self.values.iter().position(|v| *v == value) {
            Some(i) => _ = self.values.remove(i),
            None => self.values.push(value),
        }
        self.not &= !self.values.is_empty();
    }

    /// The one value this field insists on, if it does.
    pub(crate) fn only(&self) -> Option<&T> {
        match self.values.as_slice() {
            [v] if !self.not => Some(v),
            _ => None,
        }
    }
}

#[derive(Clone, PartialEq, Debug, Default)]
pub(crate) struct Filters {
    pub(crate) status: Choice<Col>,
    pub(crate) agent: Choice<String>,
    pub(crate) project: Choice<String>,
    pub(crate) priority: Choice<u8>,
    pub(crate) label: Choice<String>,
    pub(crate) query: String,
}

impl Filters {
    /// Whether a filter is set; the search has its own field.
    pub(crate) fn any(&self) -> bool {
        FilterCat::ALL.into_iter().any(|cat| self.count(cat) > 0)
    }

    pub(crate) fn count(&self, cat: FilterCat) -> usize {
        match cat {
            FilterCat::Status => self.status.values.len(),
            FilterCat::Agent => self.agent.values.len(),
            FilterCat::Project => self.project.values.len(),
            FilterCat::Priority => self.priority.values.len(),
            FilterCat::Label => self.label.values.len(),
        }
    }

    pub(crate) fn is_not(&self, cat: FilterCat) -> bool {
        match cat {
            FilterCat::Status => self.status.not,
            FilterCat::Agent => self.agent.not,
            FilterCat::Project => self.project.not,
            FilterCat::Priority => self.priority.not,
            FilterCat::Label => self.label.not,
        }
    }

    pub(crate) fn set_not(&mut self, cat: FilterCat, not: bool) {
        match cat {
            FilterCat::Status => self.status.not = not,
            FilterCat::Agent => self.agent.not = not,
            FilterCat::Project => self.project.not = not,
            FilterCat::Priority => self.priority.not = not,
            FilterCat::Label => self.label.not = not,
        }
    }

    pub(crate) fn clear(&mut self, cat: FilterCat) {
        match cat {
            FilterCat::Status => self.status = Choice::default(),
            FilterCat::Agent => self.agent = Choice::default(),
            FilterCat::Project => self.project = Choice::default(),
            FilterCat::Priority => self.priority = Choice::default(),
            FilterCat::Label => self.label = Choice::default(),
        }
    }

    fn keeper(&self) -> impl Fn(&Ticket) -> bool + '_ {
        let query = self.query.trim().to_lowercase();
        move |t| self.status.keeps(|c| *c == column(t))
            && self.agent.keeps(|a| *a == t.provider)
            && self.project.keeps(|p| *p == t.folder)
            && self.priority.keeps(|p| *p == t.priority)
            && self.label.keeps(|l| t.labels.contains(l))
            && (query.is_empty() || t.title.to_lowercase().contains(&query) || crate::util::basename(&t.worktree).to_lowercase().contains(&query))
    }
}

/// How many tickets the page would show but for the filters and the search.
pub(crate) fn hidden_by(tickets: &[Ticket], filters: &Filters, tab: Tab, show_closed: bool) -> usize {
    let keeps = filters.keeper();
    tickets.iter().filter(|t| on_page(column(t), tab, show_closed) && !keeps(t)).count()
}

/// How many tickets a tab holds, after the filters.
pub(crate) fn tab_count(tickets: &[Ticket], filters: &Filters, tab: Tab, show_closed: bool) -> usize {
    let keeps = filters.keeper();
    tickets.iter().filter(|t| on_page(column(t), tab, show_closed) && keeps(t)).count()
}

/// The columns the Board draws, then those it lists as hidden: what was folded away, Closed unless shown, and Queued and Merged while empty. Only All tickets lists them.
pub(crate) fn board_cols(columns: &Columns, tab: Tab, show_closed: bool, tucked: &[bool; COLS.len()]) -> (Vec<Col>, Vec<Col>) {
    let vacant = |c: Col| matches!(c, Col::Queued | Col::Merged) && columns[c as usize].is_empty();
    let (shown, away): (Vec<Col>, Vec<Col>) = COLS.into_iter().filter(|c| tab.shows(*c)).partition(|c| (show_closed || *c != Col::Closed) && !tucked[*c as usize] && !vacant(*c));
    (shown, if tab == Tab::All { away } else { away.into_iter().filter(|c| tucked[*c as usize]).collect() })
}

/// What the page's main area shows.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Body {
    Hero,
    Columns,
    List,
    Filtered,
}

pub(crate) fn body(list: bool, tickets: usize, rows: usize, hidden: usize) -> Body {
    match (list, rows, hidden) {
        _ if tickets == 0 => Body::Hero,
        (_, 0, 1..) => Body::Filtered,
        (false, ..) => Body::Columns,
        (true, 0, _) => Body::Hero,
        (true, ..) => Body::List,
    }
}

/// What a key does on the Board while no field has the keyboard.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Key {
    Compose,
    Search,
    Filter,
    Next,
    Previous,
    Start,
    Open,
    Pick,
    PickAll,
}

pub(crate) fn board_key(key: &str, command: bool, on_page: bool) -> Option<Key> {
    match (key, command, on_page) {
        ("enter", true, true) => Some(Key::Start),
        ("a", true, false) => Some(Key::PickAll),
        ("n" | "c", false, false) => Some(Key::Compose),
        ("/", false, false) => Some(Key::Search),
        ("f", false, false) => Some(Key::Filter),
        ("j", false, _) | ("down", false, false) => Some(Key::Next),
        ("k", false, _) | ("up", false, false) => Some(Key::Previous),
        ("enter", false, false) => Some(Key::Open),
        ("x", false, false) => Some(Key::Pick),
        _ => None,
    }
}

/// What Esc does on the Board, nearest thing first.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Escape {
    LeavePage,
    CloseSearch,
    LeaveField,
    ClearPicks,
    LeaveBoard,
}

pub(crate) fn escape(on_page: bool, searching: bool, typing: bool, picking: bool) -> Escape {
    match (on_page, searching, typing) {
        (true, _, true) | (false, false, true) => Escape::LeaveField,
        (true, ..) => Escape::LeavePage,
        (_, _, false) if picking => Escape::ClearPicks,
        (_, true, _) => Escape::CloseSearch,
        _ => Escape::LeaveBoard,
    }
}

/// The tab that shows a ticket just made, so it doesn't land out of sight.
pub(crate) fn tab_for_new(tab: Tab, start: bool) -> Tab {
    match (tab, start) {
        (Tab::Active, false) | (Tab::Todo, true) => Tab::All,
        _ => tab,
    }
}

/// The section the list's top row is in, once that section's own head is off the top.
pub(crate) fn stuck_head(rows: &[Row], top: usize, past_start: bool) -> Option<Col> {
    if !past_start && matches!(rows.get(top), Some(Row::Head(_))) {
        return None;
    }
    rows.iter().take(top + 1).rev().find_map(|r| if let Row::Head(col) = r { Some(*col) } else { None })
}

/// Folds column `col` away by hand or brings it back; Closed has its own switch.
pub(crate) fn set_away(col: Col, away: bool, tucked: &mut [bool; COLS.len()], show_closed: &mut bool) {
    if col == Col::Closed {
        *show_closed = !away;
    } else {
        tucked[col as usize] = away;
    }
}

/// The picked tickets `action` reaches, in the order `slots` shows them.
pub(crate) fn bulk_targets<'a>(slots: &[Slot], tickets: &'a [Ticket], picked: &HashSet<String>, action: Action) -> Vec<&'a Ticket> {
    slots.iter().filter(|s| bulk_applies(s.col, action)).filter_map(|s| tickets.get(s.index)).filter(|t| picked.contains(&t.id)).collect()
}

/// The tickets the ticket page steps through: those with no session, as the columns show them.
pub(crate) fn page_ids(tickets: &[Ticket], columns: &Columns) -> Vec<String> {
    [Col::Todo, Col::Queued].iter().flat_map(|c| &columns[*c as usize]).filter_map(|&i| tickets.get(i)).map(|t| t.id.clone()).collect()
}

/// The ticket `delta` places from `at`; none past either end.
pub(crate) fn stepped<'a>(ids: &'a [String], at: &str, delta: isize) -> Option<&'a str> {
    let i = ids.iter().position(|id| id == at)?.checked_add_signed(delta)?;
    ids.get(i).map(String::as_str)
}

pub(crate) fn activity(t: &Ticket) -> i64 {
    if t.closed_at > 0 { t.closed_at } else { t.changed_at }
}

/// How many finished tickets a column shows, the latest ones.
const FINISHED_SHOWN: usize = 50;

/// Indices into the snapshot, per column of `COLS`.
pub(crate) type Columns = [Vec<usize>; COLS.len()];

/// Queued keeps the order it runs in and To do keeps its own unless sorted on purpose; the others follow `order`.
pub(crate) fn arrange(tickets: &[Ticket], filters: &Filters, order: Order) -> Columns {
    let mut columns: Columns = Default::default();
    let keeps = filters.keeper();
    for (i, t) in tickets.iter().enumerate().filter(|(_, t)| keeps(t)) {
        columns[column(t) as usize].push(i);
    }
    for (col, line) in COLS.iter().zip(columns.iter_mut()) {
        if col.finished() {
            line.sort_by_key(|&i| Reverse(activity(&tickets[i])));
            line.truncate(FINISHED_SHOWN);
        }
        match order {
            _ if *col == Col::Queued => line.sort_by_key(|&i| tickets[i].order),
            Order::Activity if col.lifts() => line.sort_by_key(|&i| tickets[i].order),
            Order::Activity => line.sort_by_key(|&i| Reverse(activity(&tickets[i]))),
            Order::Created => line.sort_by_key(|&i| Reverse(tickets[i].created_at)),
            Order::Priority => line.sort_by_key(|&i| priority_rank(tickets[i].priority)),
            Order::Name => line.sort_by_cached_key(|&i| tickets[i].title.to_lowercase()),
        }
    }
    columns
}

pub(crate) fn shown_cols(tab: Tab, show_closed: bool) -> impl Iterator<Item = Col> {
    COLS.into_iter().filter(move |c| on_page(*c, tab, show_closed))
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
pub(crate) fn rows(columns: &Columns, tab: Tab, show_closed: bool, folded: &[bool; COLS.len()]) -> Vec<Row> {
    let mut out = Vec::new();
    for col in shown_cols(tab, show_closed) {
        let line = &columns[col as usize];
        if !line.is_empty() {
            out.push(Row::Head(col));
            if !folded[col as usize] {
                out.extend(line.iter().enumerate().map(|(pos, &index)| Row::Ticket(Slot { col, pos, index })));
            }
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
    Stop,
    Queue,
    Priority,
    Labels,
    Agent,
    Project,
    Select,
    Deselect,
    /// The selection's own menu of statuses; a ticket's offers Queue and MoveBack directly.
    SetStatus,
    CreatePr,
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

/// Where clicking a ticket goes: its page before it starts, then its session, else its Worktree.
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
        Col::Working => open.into_iter().chain(copy).chain(reach.session.then_some([Separator, Stop]).into_iter().flatten()).collect(),
        Col::NeedsYou if ended(t) => open.into_iter().chain([StartAgain]).chain(copy).chain([Separator, Close]).collect(),
        Col::NeedsYou | Col::Done | Col::InReview => open.into_iter().chain(copy).chain([Separator, Close]).collect(),
        Col::Merged => copy.into_iter().chain(delete_tree).collect(),
        Col::Closed => {
            let restorable = t.closed_why == ClosedWhy::Closed && (reach.tree || t.worktree.is_empty());
            restorable.then_some(Restore).into_iter().chain(copy).chain(delete_tree).collect()
        }
    }
}

/// `items` with `extra` offered ahead of what the separator sets apart.
fn ahead_of_separator(mut items: Vec<Action>, extra: impl IntoIterator<Item = Action>) -> Vec<Action> {
    let at = items.iter().position(|a| *a == Action::Separator).unwrap_or(items.len());
    items.splice(at..at, extra);
    items
}

/// `items` with a ticket's properties, its agent and project only until a session has them.
pub(crate) fn with_properties(items: Vec<Action>, lifts: bool) -> Vec<Action> {
    let unstarted = lifts.then_some([Action::Agent, Action::Project]).into_iter().flatten();
    ahead_of_separator(items, [Action::Priority].into_iter().chain(unstarted).chain([Action::Labels]))
}

/// Whether a change made to a whole selection reaches a ticket in `col`: what its own menu offers, so Close passes over running tickets.
pub(crate) fn bulk_applies(col: Col, action: Action) -> bool {
    match action {
        Action::Start => col == Col::Todo,
        Action::Close => matches!(col, Col::NeedsYou | Col::Done | Col::InReview),
        Action::Priority | Action::Labels => !col.finished(),
        Action::Agent | Action::SetStatus => col.lifts(),
        Action::Queue => col == Col::Todo,
        Action::MoveBack => col == Col::Queued,
        _ => false,
    }
}

/// The one thing a card offers without opening a menu.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Quick {
    Start,
    Answer,
    StartAgain,
    CreatePr,
}

pub(crate) fn quick(t: &Ticket, col: Col, asking: bool, reach: Reach, has_pr: bool) -> Option<Quick> {
    match col {
        Col::Todo => Some(Quick::Start),
        Col::NeedsYou if asking => Some(Quick::Answer),
        Col::NeedsYou if ended(t) => Some(Quick::StartAgain),
        Col::InReview if reach.tree && !has_pr => Some(Quick::CreatePr),
        _ => None,
    }
}

/// What a ticket's status mark offers: a status set by hand, only before there is a session.
pub(crate) fn status_actions(col: Col) -> Vec<Action> {
    match col {
        Col::Todo => vec![Action::Start, Action::Queue],
        Col::Queued => vec![Action::MoveBack],
        _ => Vec::new(),
    }
}

/// A ticket's menu: its actions, and its properties until it is over.
pub(crate) fn menu_items(t: &Ticket, reach: Reach, picked: bool) -> Vec<Action> {
    let items = actions(t, reach);
    let col = column(t);
    if col.finished() {
        return items;
    }
    ahead_of_separator(with_properties(items, col.lifts()), [if picked { Action::Deselect } else { Action::Select }])
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Tone {
    Dim,
    Asking,
    Failed,
    Quiet,
}

/// The line a ticket says for itself: what its agent is at, its place in line, or what stopped it.
pub(crate) fn note(t: &Ticket, slot: Slot, activity: Option<&str>, ask: Option<String>) -> Option<(String, Tone)> {
    let why = || (!t.why.is_empty()).then(|| t.why.clone());
    match (slot.col, t.status) {
        (Col::Working, Status::Starting) => Some(("Starting…".to_string(), Tone::Dim)),
        (Col::Working, _) => activity.filter(|a| !a.is_empty()).map(|a| (a.to_string(), Tone::Dim)),
        (Col::Queued, _) => Some((in_line(slot.pos), Tone::Dim)),
        (Col::NeedsYou, Status::NeedsYou) => ask.map(|a| (a, Tone::Asking)),
        (Col::NeedsYou, Status::Failed) => why().map(|w| (w, Tone::Failed)),
        (Col::NeedsYou, Status::Stopped) => why().map(|w| (w, Tone::Quiet)),
        _ => None,
    }
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Landing {
    Start,
    Move { column: State, before: Option<String> },
}

/// Whether a drop would rewrite an order the current sort hides, so the card would only snap back.
pub(crate) fn reorders_nothing(from: Col, to: Col, order: Order) -> bool {
    from == Col::Todo && to == Col::Todo && order != Order::Activity
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
            number: 0,
            priority: 0,
            labels: Vec::new(),
        }
    }

    const FOLDED: [bool; COLS.len()] = [false; COLS.len()];

    #[test]
    fn the_cursor_keys_work_off_the_page_and_arrows_stay_off_it() {
        assert_eq!(board_key("down", false, false), Some(Key::Next));
        assert_eq!(board_key("up", false, false), Some(Key::Previous));
        assert_eq!(board_key("enter", false, false), Some(Key::Open));
        assert_eq!(board_key("enter", false, true), None);
        assert_eq!(board_key("x", false, false), Some(Key::Pick));
        assert_eq!(board_key("a", true, false), Some(Key::PickAll));
        assert_eq!(board_key("a", true, true), None);
    }

    #[test]
    fn esc_lets_go_of_a_selection_before_it_leaves_but_not_while_typing() {
        assert_eq!(escape(false, false, false, true), Escape::ClearPicks);
        assert_eq!(escape(false, true, false, true), Escape::ClearPicks);
        assert_eq!(escape(false, true, true, true), Escape::CloseSearch);
        assert_eq!(escape(false, false, true, true), Escape::LeaveField);
        assert_eq!(escape(true, false, false, true), Escape::LeavePage);
    }

    #[test]
    fn a_tickets_agent_and_project_are_offered_only_until_it_has_a_session() {
        use Action::*;
        let reach = Reach::default();
        let todo = menu_items(&ticket("a", State::Todo, Status::None), reach, false);
        assert_eq!(todo, vec![Start, Edit, Priority, Agent, Project, Labels, Select, Separator, Delete]);
        let working = menu_items(&live(Status::Working), Reach { session: true, tree: false }, false);
        assert!(working.contains(&Priority) && !working.contains(&Agent) && !working.contains(&Project));
    }

    #[test]
    fn a_picked_ticket_offers_to_be_let_go_and_a_finished_one_offers_neither() {
        let reach = Reach::default();
        assert!(menu_items(&ticket("a", State::Todo, Status::None), reach, true).contains(&Action::Deselect));
        let closed = menu_items(&ticket("a", State::Closed, Status::None), reach, false);
        assert!(!closed.contains(&Action::Select) && !closed.contains(&Action::Deselect));
    }

    #[test]
    fn a_bulk_change_takes_the_picked_tickets_it_applies_to_in_page_order() {
        let tickets = [ticket("a", State::Todo, Status::None), ticket("b", State::Todo, Status::None), ticket("c", State::Queued, Status::None)];
        let slot = |pos, index, col| Slot { col, pos, index };
        let slots = [slot(0, 1, Col::Todo), slot(1, 0, Col::Todo), slot(0, 2, Col::Queued)];
        let picked: HashSet<String> = ["a".to_string(), "b".to_string(), "c".to_string()].into();
        let ids = |action| bulk_targets(&slots, &tickets, &picked, action).iter().map(|t| t.id.clone()).collect::<Vec<_>>();
        assert_eq!(ids(Action::Start), vec!["b", "a"]);
        assert_eq!(ids(Action::Priority), vec!["b", "a", "c"]);
        assert!(bulk_targets(&slots, &tickets, &HashSet::new(), Action::Start).is_empty());
    }

    #[test]
    fn a_bulk_change_reaches_only_tickets_their_own_menu_would_let_it() {
        assert!(bulk_applies(Col::Todo, Action::Start) && !bulk_applies(Col::Queued, Action::Start));
        assert!(!bulk_applies(Col::Working, Action::Close) && bulk_applies(Col::Done, Action::Close));
        assert!(!bulk_applies(Col::Merged, Action::Priority) && bulk_applies(Col::Working, Action::Labels));
        assert!(bulk_applies(Col::Queued, Action::Agent) && !bulk_applies(Col::Working, Action::Agent));
        assert!(bulk_applies(Col::Todo, Action::Queue) && !bulk_applies(Col::Queued, Action::Queue));
        assert!(bulk_applies(Col::Queued, Action::MoveBack) && !bulk_applies(Col::Todo, Action::MoveBack));
        assert!(bulk_applies(Col::Todo, Action::SetStatus) && !bulk_applies(Col::Done, Action::SetStatus));
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
    fn the_composer_warns_only_when_every_slot_is_taken() {
        assert_eq!(wait_hint(3, 4), None);
        assert_eq!(wait_hint(4, 4).as_deref(), Some("4 of 4 working, so it will wait in Queued"));
    }

    #[test]
    fn esc_goes_back_one_step_at_a_time() {
        assert_eq!(escape(true, true, false, false), Escape::LeavePage);
        assert_eq!(escape(false, true, true, false), Escape::CloseSearch);
        assert_eq!(escape(false, false, true, false), Escape::LeaveField);
        assert_eq!(escape(true, false, true, false), Escape::LeaveField);
        assert_eq!(escape(false, false, false, false), Escape::LeaveBoard);
    }

    #[test]
    fn a_new_ticket_switches_only_from_a_tab_that_would_hide_it() {
        assert_eq!(tab_for_new(Tab::Active, false), Tab::All);
        assert_eq!(tab_for_new(Tab::Active, true), Tab::Active);
        assert_eq!(tab_for_new(Tab::Todo, true), Tab::All);
        assert_eq!(tab_for_new(Tab::Todo, false), Tab::Todo);
    }

    #[test]
    fn a_section_head_sticks_once_it_reaches_the_top_of_the_list() {
        let slot = Slot { col: Col::Todo, pos: 0, index: 0 };
        let rows = [Row::Head(Col::Todo), Row::Ticket(slot), Row::Head(Col::Done), Row::Ticket(slot)];
        assert_eq!(stuck_head(&rows, 0, false), None);
        assert_eq!(stuck_head(&rows, 0, true), Some(Col::Todo));
        assert_eq!(stuck_head(&rows, 1, false), Some(Col::Todo));
        assert_eq!(stuck_head(&rows, 2, false), None);
        assert_eq!(stuck_head(&rows, 3, false), Some(Col::Done));
    }

    #[test]
    fn closed_folds_away_by_its_own_switch_and_the_rest_by_hand() {
        let (mut tucked, mut show_closed) = ([false; COLS.len()], true);
        set_away(Col::Closed, true, &mut tucked, &mut show_closed);
        assert!(!show_closed && tucked == [false; COLS.len()]);
        set_away(Col::Queued, true, &mut tucked, &mut show_closed);
        assert!(tucked[Col::Queued as usize]);
        set_away(Col::Queued, false, &mut tucked, &mut show_closed);
        assert!(!tucked[Col::Queued as usize]);
    }

    #[test]
    fn only_tickets_without_a_session_take_a_status_by_hand() {
        assert_eq!(status_actions(Col::Todo), [Action::Start, Action::Queue]);
        assert_eq!(status_actions(Col::Queued), [Action::MoveBack]);
        assert!(status_actions(Col::Working).is_empty());
        assert!(status_actions(Col::Merged).is_empty());
    }

    #[test]
    fn a_ticket_says_what_its_agent_is_at_its_place_in_line_or_what_stopped_it() {
        let slot = |col, pos| Slot { col, pos, index: 0 };
        assert_eq!(note(&live(Status::Starting), slot(Col::Working, 0), None, None), Some(("Starting…".to_string(), Tone::Dim)));
        assert_eq!(note(&live(Status::Working), slot(Col::Working, 0), Some("Reading a file"), None), Some(("Reading a file".to_string(), Tone::Dim)));
        assert_eq!(note(&live(Status::Working), slot(Col::Working, 0), Some(""), None), None);
        assert_eq!(note(&ticket("q", State::Queued, Status::None), slot(Col::Queued, 1), None, None), Some(("2nd in line".to_string(), Tone::Dim)));
        assert_eq!(note(&live(Status::NeedsYou), slot(Col::NeedsYou, 0), None, Some("Wants to run ls".into())), Some(("Wants to run ls".to_string(), Tone::Asking)));
        let failed = Ticket { why: "Tests failed".into(), ..live(Status::Failed) };
        assert_eq!(note(&failed, slot(Col::NeedsYou, 0), None, None), Some(("Tests failed".to_string(), Tone::Failed)));
        let stopped = Ticket { why: "Session closed".into(), ..live(Status::Stopped) };
        assert_eq!(note(&stopped, slot(Col::NeedsYou, 0), None, None), Some(("Session closed".to_string(), Tone::Quiet)));
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

    fn any_of<T>(value: T) -> Choice<T> {
        Choice { values: vec![value], not: false }
    }

    #[test]
    fn a_field_keeps_a_ticket_with_any_of_its_values_or_rules_them_out() {
        let [a, b, c] = ["a", "b", "c"].map(|t| ticket(t, State::Todo, Status::None));
        let (mut a, mut b, c) = (a, b, c);
        (a.priority, b.priority) = (1, 2);
        let tickets = [a, b, c];
        let got = |f: Filters| arrange(&tickets, &f, Order::Activity)[Col::Todo as usize].clone();
        assert_eq!(got(Filters { priority: Choice { values: vec![1, 2], not: false }, ..Default::default() }), vec![0, 1]);
        assert_eq!(got(Filters { priority: Choice { values: vec![1, 2], not: true }, ..Default::default() }), vec![2]);
    }

    #[test]
    fn a_ticket_with_one_of_several_labels_counts_as_having_it() {
        let mut a = ticket("a", State::Todo, Status::None);
        a.labels = vec!["Bug".into(), "Docs".into()];
        let tickets = [a];
        let got = |f: Filters| arrange(&tickets, &f, Order::Activity)[Col::Todo as usize].clone();
        assert_eq!(got(Filters { label: Choice { values: vec!["Infra".into(), "Docs".into()], not: false }, ..Default::default() }), vec![0]);
        assert!(got(Filters { label: Choice { values: vec!["Docs".into()], not: true }, ..Default::default() }).is_empty());
    }

    #[test]
    fn taking_the_last_value_away_forgets_not() {
        let mut c = Choice { values: vec![1, 2], not: true };
        c.toggle(1);
        assert!(c.not);
        c.toggle(2);
        assert_eq!(c, Choice::default());
    }

    #[test]
    fn only_a_single_plain_value_names_one_project() {
        assert_eq!(any_of(1).only(), Some(&1));
        assert_eq!(Choice { values: vec![1], not: true }.only(), None);
        assert_eq!(Choice { values: vec![1, 2], not: false }.only(), None);
    }

    #[test]
    fn filters_narrow_by_agent_project_and_text() {
        let mut a = ticket("Fix the login", State::Todo, Status::None);
        let mut b = ticket("Other", State::Todo, Status::None);
        (a.provider, b.folder, b.worktree) = ("codex".into(), "/other".into(), "/worktrees/login-page".into());
        let tickets = [a, b];
        let got = |f: Filters| arrange(&tickets, &f, Order::Activity)[Col::Todo as usize].clone();
        assert_eq!(got(Filters { agent: any_of("codex".to_string()), ..Default::default() }), vec![0]);
        assert_eq!(got(Filters { project: any_of("/other".to_string()), ..Default::default() }), vec![1]);
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
        assert_eq!(rows(&columns, Tab::All, false, &FOLDED), vec![Row::Head(Col::Todo), Row::Ticket(Slot { col: Col::Todo, pos: 0, index: 0 })]);
        assert_eq!(rows(&columns, Tab::All, true, &FOLDED).len(), 4);
        assert!(rows(&columns, Tab::Active, true, &FOLDED).is_empty());
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
    fn a_tab_shows_its_part_of_the_flow() {
        let shown = |tab| shown_cols(tab, true).collect::<Vec<_>>();
        assert_eq!(shown(Tab::All), COLS);
        assert_eq!(shown(Tab::Active), vec![Col::Queued, Col::Working, Col::NeedsYou, Col::Done, Col::InReview]);
        assert_eq!(shown(Tab::Todo), vec![Col::Todo]);
    }

    #[test]
    fn closed_stays_hidden_on_every_tab_until_it_is_asked_for() {
        assert!(Tab::ALL.iter().all(|tab| !shown_cols(*tab, false).any(|c| c == Col::Closed)));
        assert!(shown_cols(Tab::All, true).any(|c| c == Col::Closed));
    }

    #[test]
    fn queued_and_merged_tuck_away_while_empty_and_closed_until_shown() {
        let columns = Columns::default();
        let (shown, away) = board_cols(&columns, Tab::All, false, &FOLDED);
        assert_eq!(shown, vec![Col::Todo, Col::Working, Col::NeedsYou, Col::Done, Col::InReview]);
        assert_eq!(away, vec![Col::Queued, Col::Merged, Col::Closed]);
        let mut held = Columns::default();
        held[Col::Queued as usize].push(0);
        assert!(board_cols(&held, Tab::All, true, &FOLDED).0.contains(&Col::Queued));
        assert!(board_cols(&columns, Tab::All, true, &FOLDED).0.contains(&Col::Closed));
    }

    #[test]
    fn a_column_folded_away_is_listed_on_every_tab_but_only_all_lists_the_automatic_ones() {
        let mut tucked = FOLDED;
        tucked[Col::Working as usize] = true;
        let (shown, away) = board_cols(&Columns::default(), Tab::All, false, &tucked);
        assert!(!shown.contains(&Col::Working) && away.contains(&Col::Working));
        assert_eq!(board_cols(&Columns::default(), Tab::Active, false, &tucked).1, vec![Col::Working]);
    }

    #[test]
    fn dragging_within_a_sorted_to_do_changes_nothing_but_leaving_it_still_works() {
        assert!(reorders_nothing(Col::Todo, Col::Todo, Order::Priority));
        assert!(!reorders_nothing(Col::Todo, Col::Todo, Order::Activity));
        assert!(!reorders_nothing(Col::Todo, Col::Queued, Order::Priority));
    }

    #[test]
    fn to_do_sorts_when_asked_and_keeps_its_own_order_by_default() {
        let mut a = ticket("a", State::Todo, Status::None);
        let mut b = ticket("b", State::Todo, Status::None);
        (a.order, a.priority, b.order, b.priority) = (1, 4, 2, 1);
        assert_eq!(arrange(&[a.clone(), b.clone()], &Filters::default(), Order::Priority)[Col::Todo as usize], vec![1, 0]);
        assert_eq!(arrange(&[a, b], &Filters::default(), Order::Activity)[Col::Todo as usize], vec![0, 1]);
    }

    #[test]
    fn a_folded_section_keeps_its_head_and_drops_its_tickets() {
        let tickets = [ticket("a", State::Todo, Status::None)];
        let columns = arrange(&tickets, &Filters::default(), Order::Activity);
        let mut folded = FOLDED;
        folded[Col::Todo as usize] = true;
        assert_eq!(rows(&columns, Tab::All, false, &folded), vec![Row::Head(Col::Todo)]);
    }

    #[test]
    fn the_first_line_titles_a_draft_and_all_of_it_is_the_prompt() {
        assert_eq!(split_draft("  Fix login\n\nIt flakes on CI \n"), ("Fix login".to_string(), "Fix login\n\nIt flakes on CI".to_string()));
        assert_eq!(split_draft(&"a".repeat(300)).0.len(), MAX_TITLE);
        assert_eq!(split_draft("  \n "), (String::new(), String::new()));
    }

    #[test]
    fn a_short_id_takes_its_prefix_from_the_project() {
        assert_eq!(ticket_key("anywhere", 12).as_deref(), Some("ANY-12"));
        assert_eq!(ticket_key("my-room", 3).as_deref(), Some("MYR-3"));
        assert_eq!(ticket_key("anywhere", 0), None);
    }

    #[test]
    fn priority_sorts_live_columns_with_none_last_but_never_the_line() {
        let mut a = live(Status::Done);
        let mut b = live(Status::Done);
        let mut c = live(Status::Done);
        (a.priority, b.priority, c.priority) = (0, 3, 1);
        assert_eq!(arrange(&[a, b, c], &Filters::default(), Order::Priority)[Col::Done as usize], vec![2, 1, 0]);
        let mut q = ticket("q", State::Queued, Status::None);
        let mut r = ticket("r", State::Queued, Status::None);
        (q.order, q.priority, r.order, r.priority) = (1, 4, 2, 1);
        assert_eq!(arrange(&[q, r], &Filters::default(), Order::Priority)[Col::Queued as usize], vec![0, 1]);
    }

    #[test]
    fn filters_narrow_by_status_priority_and_label() {
        let mut a = live(Status::Working);
        (a.priority, a.labels) = (2, vec!["Bug".into()]);
        let b = ticket("b", State::Todo, Status::None);
        let tickets = [a, b];
        let count = |f: Filters| tab_count(&tickets, &f, Tab::All, false);
        assert_eq!(count(Filters::default()), 2);
        assert_eq!(count(Filters { status: any_of(Col::Todo), ..Default::default() }), 1);
        assert_eq!(count(Filters { priority: any_of(2), ..Default::default() }), 1);
        assert_eq!(count(Filters { label: any_of("Docs".to_string()), ..Default::default() }), 0);
        assert!(!Filters { query: "x".into(), ..Default::default() }.any());
        assert!(Filters { label: any_of("Bug".to_string()), ..Default::default() }.any());
    }

    #[test]
    fn the_page_shows_the_hero_until_there_is_something_to_list() {
        assert_eq!(body(false, 0, 0, 0), Body::Hero);
        assert_eq!(body(false, 3, 0, 3), Body::Filtered);
        assert_eq!(body(false, 3, 0, 0), Body::Columns);
        assert_eq!(body(true, 3, 0, 0), Body::Hero);
        assert_eq!(body(true, 3, 0, 2), Body::Filtered);
        assert_eq!(body(true, 3, 4, 0), Body::List);
    }

    #[test]
    fn the_page_steps_through_the_tickets_with_no_session() {
        let (mut a, mut q) = (ticket("a", State::Todo, Status::None), ticket("q", State::Queued, Status::None));
        (a.order, q.order) = (1, 2);
        let tickets = [live(Status::Working), a, q];
        let ids = page_ids(&tickets, &arrange(&tickets, &Filters::default(), Order::Activity));
        assert_eq!(ids, vec!["a".to_string(), "q".to_string()]);
        assert_eq!(stepped(&ids, "a", 1), Some("q"));
        assert_eq!(stepped(&ids, "a", -1), None);
        assert_eq!(stepped(&ids, "q", 1), None);
    }

    #[test]
    fn keys_compose_on_the_board_and_step_on_the_page() {
        assert_eq!(board_key("n", false, false), Some(Key::Compose));
        assert_eq!(board_key("c", false, false), Some(Key::Compose));
        assert_eq!(board_key("/", false, false), Some(Key::Search));
        assert_eq!(board_key("f", false, false), Some(Key::Filter));
        assert_eq!(board_key("j", false, false), Some(Key::Next));
        assert_eq!(board_key("down", false, true), None);
        assert_eq!(board_key("j", false, true), Some(Key::Next));
        assert_eq!(board_key("k", false, true), Some(Key::Previous));
        assert_eq!(board_key("n", false, true), None);
        assert_eq!(board_key("enter", true, true), Some(Key::Start));
        assert_eq!(board_key("enter", true, false), None);
        assert_eq!(board_key("n", true, false), None);
    }

    #[test]
    fn a_working_ticket_can_be_stopped_only_while_its_session_is_there() {
        let reach = Reach { session: true, tree: true };
        assert_eq!(actions(&live(Status::Working), reach), vec![Action::Open, Action::CopyBranch, Action::Separator, Action::Stop]);
        assert!(!actions(&live(Status::Working), Reach { session: false, tree: true }).contains(&Action::Stop));
    }

    #[test]
    fn priority_and_labels_come_before_what_the_separator_sets_apart() {
        use Action::*;
        assert_eq!(with_properties(vec![Open, Separator, Close], false), vec![Open, Priority, Labels, Separator, Close]);
        assert_eq!(with_properties(vec![Open], false), vec![Open, Priority, Labels]);
        assert_eq!(with_properties(vec![Open], true), vec![Open, Priority, Agent, Project, Labels]);
    }

    #[test]
    fn a_card_offers_what_its_state_needs_next() {
        let reach = Reach { session: true, tree: true };
        let todo = ticket("t", State::Todo, Status::None);
        let mut ended = live(Status::Stopped);
        ended.agent_id.clear();
        assert_eq!(quick(&todo, Col::Todo, false, reach, false), Some(Quick::Start));
        assert_eq!(quick(&live(Status::NeedsYou), Col::NeedsYou, true, reach, false), Some(Quick::Answer));
        assert_eq!(quick(&live(Status::NeedsYou), Col::NeedsYou, false, reach, false), None);
        assert_eq!(quick(&ended, Col::NeedsYou, false, reach, false), Some(Quick::StartAgain));
        assert_eq!(quick(&live(Status::Idle), Col::InReview, false, reach, false), Some(Quick::CreatePr));
        assert_eq!(quick(&live(Status::Idle), Col::InReview, false, reach, true), None);
        assert_eq!(quick(&live(Status::Idle), Col::InReview, false, Reach { session: true, tree: false }, false), None);
        assert_eq!(quick(&live(Status::Working), Col::Working, false, reach, false), None);
    }

    #[test]
    fn the_hidden_count_is_what_the_filters_take_off_this_page() {
        let (a, mut b, c) = (ticket("login", State::Todo, Status::None), ticket("other", State::Todo, Status::None), ticket("gone", State::Closed, Status::Idle));
        b.provider = "codex".into();
        let tickets = [a, b, c];
        let codex = Filters { agent: any_of("codex".to_string()), ..Default::default() };
        assert_eq!(hidden_by(&tickets, &Filters::default(), Tab::All, false), 0);
        assert_eq!(hidden_by(&tickets, &codex, Tab::All, false), 1);
        assert_eq!(hidden_by(&tickets, &codex, Tab::All, true), 2);
        assert_eq!(hidden_by(&tickets, &codex, Tab::Active, true), 0);
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
    fn a_click_goes_to_the_page_the_session_or_the_worktree() {
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
