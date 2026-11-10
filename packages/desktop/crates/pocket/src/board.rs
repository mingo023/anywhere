mod card;
mod bulk_bar;
mod card_menu;
mod column;
mod composer;
mod display_menu;
mod filter_bar;
mod filter_notice;
mod form;
mod form_chips;
mod hidden_columns;
pub(crate) mod logic;
mod page;
mod parts;
mod row;
mod selection;
mod tabs;
mod ticket_page;
mod toast;
mod top_bar;
mod values;
mod views;

use crate::actions::OpenBoard;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, Screen};
use agents::tickets::{Board, Ticket};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use logic::{Action, COLS, Col, Columns, Escape, Field, FilterCat, Filters, Key, Landing, Opens, Order, Props, Reach, Row, Slot, Surface, Tab, arrange, board_cols, board_key, column, escape, hidden_by, landing, reorders_nothing, opens, page_ids, rows, tab_count};
use selection::{Selection, walk};
use std::collections::HashSet;
use toast::Toast;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub(crate) enum Layout {
    #[default]
    Board,
    List,
}

/// The popups the Board opens, one at a time.
#[derive(Clone, PartialEq, Debug)]
pub(crate) enum Menu {
    Card { id: String, at: Point<Pixels>, sub: Option<Field> },
    Status { id: String, at: Point<Pixels> },
    Display,
    Filter,
    FilterIn(FilterCat),
    Pick(Surface, Field),
    Bulk { action: Action, at: Point<Pixels> },
}

pub struct BoardState {
    pub(crate) layout: Layout,
    tab: Tab,
    pub(crate) order: Order,
    pub(crate) show_closed: bool,
    pub(crate) filters: Filters,
    columns: Columns,
    hidden: usize,
    /// How many tickets each of `Tab::ALL` holds.
    counts: [usize; Tab::ALL.len()],
    /// The columns drawn, and those listed as hidden.
    shown: Vec<Col>,
    away: Vec<Col>,
    /// Columns folded away by hand.
    tucked: [bool; COLS.len()],
    /// The list's sections showing only their heads.
    folded: [bool; COLS.len()],
    pub(crate) props: Props,
    pub(crate) searching: bool,
    /// The tickets the open ticket page steps through; `None` shows the Board.
    page: Option<Vec<String>>,
    lists: Vec<ListState>,
    rows: Vec<Row>,
    pub(crate) rows_list: ListState,
    columns_scroll: ScrollHandle,
    selection: Selection,
    pub(crate) menu: Option<Menu>,
    search: Entity<InputState>,
    filter_search: Entity<InputState>,
    form: form::Form,
    composer: composer::Composer,
    toast: Option<Toast>,
    toasts: u64,
}

impl BoardState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let filter_search = cx.new(|cx| InputState::new(window, cx).placeholder("Add filter…"));
        let (form, mut subs) = form::Form::new(window, cx);
        let (composer, composer_subs) = composer::Composer::new(window, cx);
        subs.extend(composer_subs);
        subs.push(cx.subscribe(&search, |this: &mut Desktop, s, _: &InputEvent, cx| {
            this.board.filters.query = s.read(cx).value().to_string();
            this.board_changed();
            cx.notify();
        }));
        subs.push(cx.subscribe(&filter_search, |_: &mut Desktop, _, _: &InputEvent, cx| cx.notify()));
        let state = Self {
            layout: Layout::default(),
            tab: Tab::default(),
            order: Order::default(),
            show_closed: false,
            filters: Filters::default(),
            columns: Default::default(),
            hidden: 0,
            counts: Default::default(),
            shown: Vec::new(),
            away: Vec::new(),
            tucked: Default::default(),
            folded: Default::default(),
            props: Props::default(),
            searching: false,
            page: None,
            lists: COLS.iter().map(|_| ListState::new(0, ListAlignment::Top, px(400.))).collect(),
            rows: Vec::new(),
            rows_list: ListState::new(0, ListAlignment::Top, px(400.)),
            columns_scroll: ScrollHandle::new(),
            selection: Selection::default(),
            menu: None,
            search,
            filter_search,
            form,
            composer,
            toast: None,
            toasts: 0,
        };
        (state, subs)
    }

    fn regroup(&mut self, board: &Board) {
        self.columns = arrange(&board.tickets, &self.filters, self.order);
        self.hidden = hidden_by(&board.tickets, &self.filters, self.tab, self.show_closed);
        self.counts = Tab::ALL.map(|tab| tab_count(&board.tickets, &self.filters, tab, self.show_closed));
        (self.shown, self.away) = board_cols(&self.columns, self.tab, self.show_closed, &self.tucked);
        // A ticket's height follows its content, so the lists measure afresh whenever the snapshot changes.
        for (list, line) in self.lists.iter().zip(&self.columns) {
            let top = list.logical_scroll_top();
            list.reset(line.len());
            list.scroll_to(top);
        }
        self.rows = rows(&self.columns, self.tab, self.show_closed, &self.folded);
        let top = self.rows_list.logical_scroll_top();
        self.rows_list.reset(self.rows.len());
        self.rows_list.scroll_to(top);
        let slots = self.walk_slots();
        let ids = |finished: bool| -> HashSet<&str> { slots.iter().filter(|s| finished || !s.col.finished()).filter_map(|s| board.tickets.get(s.index)).map(|t| t.id.as_str()).collect() };
        self.selection.keep(&ids(true), &ids(false));
    }

    /// The tickets on the page in reading order.
    fn walk_slots(&self) -> Vec<Slot> {
        walk(self.layout == Layout::List, &self.rows, &self.shown, &self.columns)
    }

    fn reveal(&self, slot: Slot) {
        if self.layout == Layout::List {
            if let Some(ix) = self.rows.iter().position(|r| *r == Row::Ticket(slot)) {
                self.rows_list.scroll_to_reveal_item(ix);
            }
        } else {
            self.lists[slot.col as usize].scroll_to_reveal_item(slot.pos);
            if let Some(ix) = self.shown.iter().position(|c| *c == slot.col) {
                self.columns_scroll.scroll_to_item(ix);
            }
        }
    }
}

impl Desktop {
    fn board_projects(&self) -> Vec<String> {
        self.store.without_scratch(self.projects())
    }

    pub(crate) fn open_board(&mut self, _: &OpenBoard, window: &mut Window, cx: &mut Context<Self>) {
        if !self.agents.board_offered() {
            return;
        }
        self.close_menus();
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        self.screen = Screen::Board;
        self.board.layout = if self.store.board_list { Layout::List } else { Layout::Board };
        self.aim_board_composer(cx);
        if self.board.page.is_some() {
            self.leave_ticket_page(window, cx);
        }
        self.board_changed();
        window.focus(&self.root, cx);
        cx.notify();
    }

    /// Off the screen nothing reads the columns; opening it regroups.
    pub(crate) fn board_changed(&mut self) {
        if self.screen == Screen::Board {
            self.board.regroup(&self.agents.board);
            self.drop_stale_ticket_page();
        }
    }

    /// Keeps the open page's steps current; a ticket that started or went away has no page left to show.
    fn drop_stale_ticket_page(&mut self) {
        if self.board.page.is_none() {
            return;
        }
        let v = &self.board.form.picks.values;
        let t = v.id.as_deref().and_then(|id| self.ticket(id));
        if t.is_some_and(|t| column(t).lifts()) {
            self.board.page = Some(page_ids(&self.agents.board.tickets, &self.board.columns));
            return;
        }
        if let Some(draft) = t.and_then(|t| values::late_save(v, t)) {
            self.outbox.local.board_save(draft, false);
        }
        self.close_menus();
        self.board.page = None;
    }

    pub(crate) fn leave_unoffered_board(&mut self) {
        if self.screen == Screen::Board && !self.agents.board_offered() {
            self.screen = Screen::Sessions;
        }
    }

    pub(crate) fn escape_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let (composer, title, prompt, search) = (self.board.composer.text.focus_handle(cx), self.board.form.title.focus_handle(cx), self.board.form.prompt.focus_handle(cx), self.board.search.focus_handle(cx));
        let typing = [composer, title, prompt, search].iter().any(|field| field.is_focused(window));
        match escape(self.board.page.is_some(), self.board.searching, typing, !self.board.selection.picked.is_empty()) {
            Escape::LeavePage => self.leave_ticket_page(window, cx),
            Escape::CloseSearch => self.set_board_search(false, window, cx),
            Escape::LeaveField => {
                window.focus(&self.root, cx);
                cx.notify();
            }
            Escape::ClearPicks => {
                self.board.selection.picked.clear();
                cx.notify();
            }
            Escape::LeaveBoard => self.leave_board(window, cx),
        }
    }

    pub(crate) fn leave_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.board.page.is_some() {
            self.leave_ticket_page(window, cx);
        }
        self.screen = Screen::Sessions;
        cx.notify();
    }

    /// The Board's keys: single ones unless a field has the keyboard, ⌘↵ anywhere on a ticket's page.
    pub(crate) fn board_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let command = key.modifiers == Modifiers::command();
        // The filter menu's search keeps the keyboard's focus after the menu closes.
        let free = self.root.is_focused(window) || self.board.filter_search.focus_handle(cx).is_focused(window);
        let start_key = command && key.key == "enter";
        if self.screen != Screen::Board || self.overlay.is_some() || self.board.menu.is_some() || (!command && key.modifiers.modified()) || !(free || start_key) {
            return false;
        }
        match board_key(&key.key, command, self.board.page.is_some()) {
            Some(Key::Compose) => self.focus_board_composer(window, cx),
            Some(Key::Search) => self.set_board_search(true, window, cx),
            Some(Key::Filter) => self.open_filter_menu(window, cx),
            Some(Key::Next) => self.step_board(1, window, cx),
            Some(Key::Previous) => self.step_board(-1, window, cx),
            Some(Key::Start) => self.start_from_page(window, cx),
            Some(Key::Open) => self.open_cursor_ticket(window, cx),
            Some(Key::Pick) => self.pick_cursor_ticket(cx),
            Some(Key::PickAll) => self.pick_all_tickets(cx),
            None => return false,
        }
        true
    }

    fn ticket(&self, id: &str) -> Option<&Ticket> {
        self.agents.board.get(id)
    }

    /// The ticket's Worktree as git lists it; `None` once it is gone.
    fn board_tree(&self, t: &Ticket) -> Option<&git::Worktree> {
        self.worktrees.get(&t.folder)?.iter().find(|w| !t.worktree.is_empty() && w.path == t.worktree)
    }

    fn ticket_reach(&self, t: &Ticket) -> Reach {
        Reach { session: self.agents.get(&t.agent_id).is_some(), tree: self.board_tree(t).is_some() }
    }

    fn open_ticket(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.ticket(id).cloned() else { return };
        match opens(&t, self.ticket_reach(&t)) {
            Some(Opens::Edit) => self.edit_ticket(id, window, cx),
            Some(Opens::Session) => self.focus_agent(&t.agent_id, window, cx),
            Some(Opens::Worktree) => self.select_tree(t.folder, Some(t.worktree), cx),
            None => {}
        }
    }

    fn close_tickets(&mut self, ids: Vec<String>, cx: &mut Context<Self>) {
        let message = match ids.as_slice() {
            [id] => format!("Closed “{}”. Its worktree stays.", self.delete_ticket_title(id)),
            _ => format!("Closed {} tickets. Their worktrees stay.", ids.len()),
        };
        for id in &ids {
            self.outbox.local.board_close(id);
        }
        self.show_board_toast(message, ids, cx);
    }

    fn run_action(&mut self, action: Action, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        if let (Some(field), Some(Menu::Card { sub, .. })) = (Self::action_field(action), self.board.menu.as_mut()) {
            *sub = Some(field);
            cx.notify();
            return;
        }
        self.close_menus();
        let Some(t) = self.ticket(id).cloned() else { return };
        match action {
            Action::Start | Action::StartAgain => self.outbox.local.board_start(id),
            Action::Edit => self.edit_ticket(id, window, cx),
            Action::Delete => self.ask_delete_ticket(id.to_string(), window, cx),
            Action::MoveBack => self.outbox.local.board_move(id, agents::tickets::State::Todo, None),
            Action::Open => self.open_ticket(id, window, cx),
            Action::CopyBranch => {
                if let Some(branch) = self.board_tree(&t).map(|w| w.branch.clone()) {
                    cx.write_to_clipboard(ClipboardItem::new_string(branch.clone()));
                    self.show_board_toast(format!("Copied {branch}"), Vec::new(), cx);
                }
            }
            Action::Close => self.close_tickets(vec![id.to_string()], cx),
            Action::Restore => self.outbox.local.board_reopen(id),
            Action::DeleteWorktree => self.ask_delete_worktree(t.folder, t.worktree, cx),
            Action::Stop => {
                if let Some(terminal) = self.agents.get(&t.agent_id).map(|a| a.terminal_id.clone()) {
                    self.close_pane(&terminal, cx);
                }
            }
            Action::Queue => self.outbox.local.board_move(id, agents::tickets::State::Queued, None),
            Action::CreatePr => {
                self.open_ticket(id, window, cx);
                self.open_pr_composer(window, cx);
            }
            Action::Select | Action::Deselect => self.board.selection.toggle(id),
            Action::Priority | Action::Labels | Action::Agent | Action::Project | Action::SetStatus | Action::Separator => {}
        }
        cx.notify();
    }

    pub(super) fn action_field(action: Action) -> Option<Field> {
        match action {
            Action::Priority => Some(Field::Priority),
            Action::Labels => Some(Field::Labels),
            Action::Agent => Some(Field::Agent),
            Action::Project => Some(Field::Project),
            _ => None,
        }
    }

    fn drop_ticket(&mut self, id: &str, from: Col, to: Col, before: Option<&str>, cx: &mut Context<Self>) {
        if reorders_nothing(from, to, self.board.order) {
            return;
        }
        match landing(id, from, to, before) {
            Some(Landing::Start) => self.outbox.local.board_start(id),
            Some(Landing::Move { column, before }) => self.outbox.local.board_move(id, column, before.as_deref()),
            None => {}
        }
        cx.notify();
    }

    fn ask_delete_ticket(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        self.confirm = Some(Confirm::DeleteTicket(id));
        self.open(Overlay::Confirm, window, cx);
    }

    pub(crate) fn delete_ticket(&mut self, id: &str) {
        self.outbox.local.board_delete(id);
    }

    pub(crate) fn delete_ticket_title(&self, id: &str) -> String {
        self.ticket(id).map_or_else(|| "this ticket".to_string(), |t| t.title.clone())
    }

    /// The ticket a column or list row shows, if the snapshot still has it there.
    fn shown_ticket(&self, col: Col, index: usize) -> Option<&Ticket> {
        self.agents.board.tickets.get(index).filter(|t| column(t) == col)
    }
}
