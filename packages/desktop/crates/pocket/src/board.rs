mod bulk_bar;
mod card;
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
mod side;
mod tabs;
mod ticket_page;
mod toast;
mod top_bar;
mod values;
mod views;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, Screen, popup_closing};
use crate::git_ui::pull_requests::glyph as pr_glyph;
use agents::tickets::{Board as Snapshot, State, Ticket, TicketDraft};
use agents::{Decision, Outbox};
pub(crate) use card_menu::PropChange;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use logic::{
    Action, COLS, Col, Columns, Escape, Field, FilterCat, Filters, Key, Opens, Order, Props, Row, Slot, Surface, Tab, arrange, board_cols, board_key,
    closed_note, column, escape, hidden_by, opens, page_ids, rows, tab_count, ticket_key,
};
use selection::{Selection, walk};
use side::{Extra, Pr, Project, Side};
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

/// What the Board asks of pocketd.
pub(crate) enum Message {
    Start(String),
    Reopen(String),
    Move { id: String, to: State, before: Option<String> },
    Limit(usize),
}

impl Message {
    fn send_to(&self, out: &Outbox) {
        match self {
            Message::Start(id) => out.board_start(id),
            Message::Reopen(id) => out.board_reopen(id),
            Message::Move { id, to, before } => out.board_move(id, *to, before.as_deref()),
            Message::Limit(limit) => out.board_limit(*limit),
        }
    }
}

pub(crate) enum BoardEvent {
    /// What the page groups by changed; `Desktop` regroups the tickets.
    Changed,
    /// A control was pressed: the other menus shut, then `show` opens this one.
    OpenMenu(Menu),
    /// As `OpenMenu`, shutting the menu instead when it is the one open.
    ToggleMenu(Menu),
    CloseMenus,
    /// The filter menu, on a field's values when it names one.
    OpenFilter(Option<FilterCat>),
    FocusRoot,
    Send(Message),
    /// The layout was picked; `Desktop` keeps it for the next launch.
    Layout(Layout),
    Open(String),
    Action {
        action: Action,
        id: String,
    },
    Close(Vec<String>),
    SetProps {
        ids: Vec<String>,
        change: PropChange,
    },
    Answer {
        request: String,
        decision: Decision,
    },
    /// The composer's Add or Start, with a ticket to make of it.
    Create {
        start: bool,
    },
    LoadBranches {
        surface: Surface,
        folder: String,
    },
    EditTicket(String),
    LeavePage,
    StartPage,
    MovePageBack,
    SavePage,
    SaveView,
    /// A filter was picked; `Desktop` regroups and aims the composer at what it filters to.
    Filtered,
    ApplyView(usize),
    RemoveView(usize),
}

/// The Board screen's page, and the ticket's page over it, over what `Desktop` hands down each time it draws them; `Desktop` draws the bar.
pub struct Board {
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
    /// The tickets as of the last regroup, which the columns index into.
    tickets: Snapshot,
    side: Side,
    _subs: Vec<Subscription>,
}

impl EventEmitter<BoardEvent> for Board {}

impl Board {
    pub fn new(window: &mut Window, cx: &mut Context<Self>) -> Self {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let filter_search = cx.new(|cx| InputState::new(window, cx).placeholder("Add filter…"));
        let (form, mut subs) = form::Form::new(window, cx);
        let (composer, composer_subs) = composer::Composer::new(window, cx);
        subs.extend(composer_subs);
        subs.push(cx.subscribe(&search, |this, s, _: &InputEvent, cx| {
            this.filters.query = s.read(cx).value().to_string();
            cx.emit(BoardEvent::Changed);
            cx.notify();
        }));
        subs.push(cx.subscribe(&filter_search, |_, _, _: &InputEvent, cx| cx.notify()));
        Self {
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
            tickets: Snapshot::default(),
            side: Side::default(),
            _subs: subs,
        }
    }

    fn set(&mut self, side: Side) {
        self.side = side;
    }

    /// Takes the tickets as they are now and groups them for the page.
    fn regroup(&mut self, board: Snapshot) {
        self.tickets = board;
        let board = &self.tickets;
        self.columns = arrange(&board.tickets, &self.filters, self.order);
        self.hidden = hidden_by(&board.tickets, &self.filters, self.tab, self.show_closed);
        self.counts = Tab::ALL.map(|tab| tab_count(&board.tickets, tab, self.show_closed));
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
        let board = &self.tickets;
        let ids = |finished: bool| -> HashSet<&str> {
            slots.iter().filter(|s| finished || !s.col.finished()).filter_map(|s| board.tickets.get(s.index)).map(|t| t.id.as_str()).collect()
        };
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

    fn ticket(&self, id: &str) -> Option<&Ticket> {
        self.tickets.get(id)
    }

    /// The ticket a column or list row shows, if the snapshot still has it there.
    fn shown_ticket(&self, col: Col, index: usize) -> Option<&Ticket> {
        self.tickets.tickets.get(index).filter(|t| column(t) == col)
    }

    /// A floating menu a click outside closes.
    pub(super) fn popup(&self, id: &'static str, width: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        popup_closing(id, width, cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(BoardEvent::CloseMenus)))
    }

    pub(super) fn agent_name(&self, provider: &str) -> String {
        self.side.labels.get(provider).cloned().unwrap_or_else(|| theme::provider_name(provider).into())
    }

    pub(super) fn repo_name(&self, folder: &str) -> String {
        self.side.projects.get(folder).map(|p| p.name.clone()).unwrap_or_default()
    }

    pub(super) fn repo_tile_for(&self, folder: &str, size: f32) -> Div {
        let mark = self.side.projects.get(folder).map_or_else(|| ui::Mark::Letters(String::new()), |p| p.mark.clone());
        ui::repo_tile(&mark, size, false, None)
    }

    /// `row` for picking `provider`; one pocketd didn't find is dimmed and says how to install it.
    pub(super) fn agent_row(&self, provider: &str, row: Stateful<Div>) -> Stateful<Div> {
        if self.side.installed.contains(provider) {
            return row;
        }
        let why = crate::models::not_found(provider);
        row.opacity(0.4).tooltip(move |window, cx| gpui_kit::component::tooltip::Tooltip::new(why.clone()).build(window, cx))
    }

    fn extra(&self, t: &Ticket) -> &Extra {
        let index = self.tickets.tickets.iter().position(|x| x.id == t.id);
        index.and_then(|i| self.side.extras.get(&i)).unwrap_or(&side::NO_EXTRA)
    }

    /// The places in the snapshot the page needs `Desktop`'s word on: the tickets it draws, the menu's and the page's.
    fn wanted(&self) -> Vec<usize> {
        let drawn: Vec<usize> = if self.layout == Layout::List {
            self.rows.iter().filter_map(|r| if let Row::Ticket(slot) = r { Some(slot.index) } else { None }).collect()
        } else {
            self.shown.iter().flat_map(|&col| self.columns[col as usize].iter().copied()).collect()
        };
        let menu = match &self.menu {
            Some(Menu::Card { id, .. } | Menu::Status { id, .. }) => Some(id.as_str()),
            _ => None,
        };
        let page = self.page.as_ref().and(self.form.picks.values.id.as_deref());
        let named = menu.into_iter().chain(page).filter_map(|id| self.tickets.tickets.iter().position(|t| t.id == id));
        let mut wanted: Vec<usize> = drawn.into_iter().chain(named).collect();
        wanted.sort_unstable();
        wanted.dedup();
        wanted
    }

    /// The folders the page will name.
    fn named(&self, wanted: &[usize]) -> HashSet<String> {
        let tickets = wanted.iter().filter_map(|&i| self.tickets.tickets.get(i)).map(|t| t.folder.clone());
        tickets
            .chain(self.filters.project.values.iter().cloned())
            .chain([self.composer.picks.values.folder.clone(), self.form.picks.values.folder.clone()])
            .collect()
    }

    pub(crate) fn menu_open(&self) -> bool {
        self.menu.is_some()
    }

    /// Esc lets go of a focused field first, then a page, a pick, the search, and last the Board; `Desktop` acts on it.
    pub(crate) fn escape(&self, window: &Window, cx: &App) -> Escape {
        let fields = [self.composer.text.focus_handle(cx), self.form.title.focus_handle(cx), self.form.prompt.focus_handle(cx), self.search.focus_handle(cx)];
        let typing = fields.iter().any(|field| field.is_focused(window));
        escape(self.page.is_some(), self.searching, typing, !self.selection.picked.is_empty())
    }

    /// Whether the keyboard is free for the Board's keys: `root` has it, or a field a closed menu or page left it in.
    fn keys_free(&self, root_focused: bool, window: &Window, cx: &App) -> bool {
        // A field keeps the keyboard's focus after what held it closes: the filter menu's search, a ticket page that went away.
        let stranded = self.page.is_none() && (self.form.title.focus_handle(cx).is_focused(window) || self.form.prompt.focus_handle(cx).is_focused(window));
        stranded || root_focused || self.filter_search.focus_handle(cx).is_focused(window)
    }

    pub(crate) fn show(&mut self, menu: Menu, cx: &mut Context<Self>) {
        self.menu = Some(menu);
        cx.notify();
    }

    fn clear_picks(&mut self, cx: &mut Context<Self>) {
        self.selection.picked.clear();
        cx.notify();
    }

    /// Opens the page of a ticket that has not started; its folder, whose branches the page lists.
    fn open_page(&mut self, t: &Ticket, window: &mut Window, cx: &mut Context<Self>) -> String {
        self.page = Some(page_ids(&self.tickets.tickets, &self.columns));
        let f = &mut self.form;
        f.picks = values::Picks::new(values::Values::of(t));
        f.title.update(cx, |s, cx| s.set_value(t.title.clone(), window, cx));
        f.prompt.update(cx, |s, cx| s.set_value(t.prompt.clone(), window, cx));
        t.folder.clone()
    }

    /// Keeps the open page's steps current. When its ticket started or went away, there is no page left to show: the page closes, and the edits it never saved come back to send.
    fn keep_page(&mut self) -> Option<Option<TicketDraft>> {
        self.page.as_ref()?;
        let v = &self.form.picks.values;
        let t = v.id.as_deref().and_then(|id| self.tickets.get(id));
        if t.is_some_and(|t| column(t).lifts()) {
            self.page = Some(page_ids(&self.tickets.tickets, &self.columns));
            return None;
        }
        let draft = t.and_then(|t| values::late_save(v, t));
        self.page = None;
        Some(draft)
    }
}

impl Desktop {
    pub(crate) fn board_projects(&self) -> Vec<String> {
        self.store.without_scratch(self.projects())
    }

    pub(crate) fn open_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.agents.board_offered() {
            return;
        }
        self.close_menus(cx);
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        self.screen = Screen::Board;
        let layout = if self.store.board_list { Layout::List } else { Layout::Board };
        self.board.update(cx, |b, _| b.layout = layout);
        self.aim_board_composer(cx);
        if self.board.read(cx).page.is_some() {
            self.leave_ticket_page(window, cx);
        }
        self.board_changed(cx);
        window.focus(&self.root, cx);
        cx.notify();
    }

    /// Off the screen nothing reads the columns; opening it regroups.
    pub(crate) fn board_changed(&mut self, cx: &mut App) {
        if self.screen == Screen::Board {
            let tickets = self.agents.board.clone();
            self.board.update(cx, |b, _| b.regroup(tickets));
            self.drop_stale_ticket_page(cx);
        }
    }

    fn drop_stale_ticket_page(&mut self, cx: &mut App) {
        let Some(late) = self.board.update(cx, |b, _| b.keep_page()) else { return };
        if let Some(draft) = late {
            self.outbox.local.board_save(draft, false);
        }
        self.close_menus(cx);
    }

    pub(crate) fn leave_unoffered_board(&mut self) {
        if self.screen == Screen::Board && !self.agents.board_offered() {
            self.screen = Screen::Sessions;
        }
    }

    pub(crate) fn escape_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match self.board.read(cx).escape(window, cx) {
            Escape::LeavePage => self.leave_ticket_page(window, cx),
            Escape::CloseSearch => self.board.update(cx, |b, cx| b.set_search(false, window, cx)),
            Escape::LeaveField => {
                window.focus(&self.root, cx);
                cx.notify();
            }
            Escape::ClearPicks => self.board.update(cx, |b, cx| b.clear_picks(cx)),
            Escape::LeaveBoard => self.leave_board(window, cx),
        }
    }

    pub(crate) fn leave_board(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.board.read(cx).page.is_some() {
            self.leave_ticket_page(window, cx);
        }
        self.screen = Screen::Sessions;
        cx.notify();
    }

    /// The Board's keys, unless a field has the keyboard.
    pub(crate) fn board_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let command = key.modifiers == Modifiers::command();
        let b = self.board.read(cx);
        let free = b.keys_free(self.root.is_focused(window), window, cx);
        if self.screen != Screen::Board || self.overlay.is_some() || b.menu.is_some() || (!command && key.modifiers.modified()) || !free {
            return false;
        }
        let on_page = b.page.is_some();
        match board_key(&key.key, command, on_page) {
            Some(Key::Compose) => self.board.update(cx, |b, cx| b.focus_composer(window, cx)),
            Some(Key::Search) => self.board.update(cx, |b, cx| b.set_search(true, window, cx)),
            Some(Key::Filter) => self.open_filter_menu(None, window, cx),
            Some(Key::Next) => self.step_board(1, window, cx),
            Some(Key::Previous) => self.step_board(-1, window, cx),
            Some(Key::Open) => {
                if let Some(id) = self.board.read(cx).selection.cursor.clone() {
                    self.open_ticket(&id, window, cx);
                }
            }
            Some(Key::Pick) => self.board.update(cx, |b, cx| b.pick_cursor(cx)),
            Some(Key::PickAll) => self.board.update(cx, |b, cx| b.pick_all(cx)),
            Some(Key::Menu(action)) => self.board.update(cx, |b, cx| b.cursor_menu(action, cx)),
            None => return false,
        }
        true
    }

    /// The Filter menu, with its search emptied and focused.
    fn open_filter_menu(&mut self, cat: Option<FilterCat>, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus(cx);
        self.board.update(cx, |b, cx| b.open_filter(cat, window, cx));
    }

    fn step_board(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.board.read(cx).page.is_some() {
            if let Some(next) = self.board.read(cx).stepped_page(delta) {
                self.edit_ticket(&next, window, cx);
            }
            return;
        }
        self.board.update(cx, |b, cx| b.step(delta, cx));
    }

    pub(crate) fn ticket(&self, id: &str) -> Option<&Ticket> {
        self.agents.board.get(id)
    }

    /// The ticket's Worktree as git lists it; `None` once it is gone.
    fn board_tree(&self, t: &Ticket) -> Option<&git::Worktree> {
        self.worktrees.get(&t.folder)?.iter().find(|w| !t.worktree.is_empty() && w.path == t.worktree)
    }

    fn ticket_reach(&self, t: &Ticket) -> logic::Reach {
        logic::Reach { session: self.agents.get(&t.agent_id).is_some(), tree: self.board_tree(t).is_some() }
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
        let message = closed_note(&ids.first().map_or_else(String::new, |id| self.delete_ticket_title(id)), ids.len());
        for id in &ids {
            self.outbox.local.board_close(id);
        }
        self.board.update(cx, |b, cx| b.toast(message, ids, cx));
    }

    fn run_action(&mut self, action: Action, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus(cx);
        let Some(t) = self.ticket(id).cloned() else { return };
        match action {
            Action::Start | Action::StartAgain => self.outbox.local.board_start(id),
            Action::Edit => self.edit_ticket(id, window, cx),
            Action::Delete => self.ask_delete_ticket(id.to_string(), window, cx),
            Action::MoveBack => self.outbox.local.board_move(id, State::Todo, None),
            Action::Open => self.open_ticket(id, window, cx),
            Action::CopyBranch => {
                if let Some(branch) = self.board_tree(&t).map(|w| w.branch.clone()) {
                    cx.write_to_clipboard(ClipboardItem::new_string(branch.clone()));
                    self.board.update(cx, |b, cx| b.toast(format!("Copied {branch}"), Vec::new(), cx));
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
            Action::Queue => self.outbox.local.board_move(id, State::Queued, None),
            Action::CreatePr => {
                self.open_ticket(id, window, cx);
                self.open_pr_composer(window, cx);
            }
            Action::Select | Action::Deselect => self.board.update(cx, |b, _| b.selection.toggle(id)),
            Action::Priority | Action::Labels | Action::Agent | Action::Project | Action::SetStatus | Action::Separator => {}
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

    pub(crate) fn on_board_event(&mut self, _: &Entity<Board>, ev: &BoardEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            BoardEvent::Changed => self.board_changed(cx),
            BoardEvent::Filtered => {
                self.board_changed(cx);
                self.aim_board_composer(cx);
            }
            BoardEvent::OpenMenu(menu) => {
                self.close_menus(cx);
                self.board.update(cx, |b, cx| b.show(menu.clone(), cx));
            }
            BoardEvent::ToggleMenu(menu) => {
                let was = self.board.read(cx).menu.clone();
                self.close_menus(cx);
                self.board.update(cx, |b, _| b.menu = (was != Some(menu.clone())).then(|| menu.clone()));
            }
            BoardEvent::CloseMenus => _ = self.close_menus(cx),
            BoardEvent::OpenFilter(cat) => self.open_filter_menu(*cat, window, cx),
            BoardEvent::FocusRoot => window.focus(&self.root, cx),
            BoardEvent::Send(message) => message.send_to(&self.outbox.local),
            BoardEvent::Layout(layout) => {
                self.store.board_list = *layout == Layout::List;
                self.store.save();
                self.board_changed(cx);
            }
            BoardEvent::Open(id) => self.open_ticket(id, window, cx),
            BoardEvent::Action { action, id } => self.run_action(*action, id, window, cx),
            BoardEvent::Close(ids) => self.close_tickets(ids.clone(), cx),
            BoardEvent::SetProps { ids, change } => self.set_tickets_props(ids, change, cx),
            BoardEvent::Answer { request, decision } => self.answer_pending(request, *decision, cx),
            BoardEvent::Create { start } => self.create_ticket(*start, window, cx),
            BoardEvent::LoadBranches { surface, folder } => self.load_ticket_branches(*surface, folder.clone(), cx),
            BoardEvent::EditTicket(id) => self.edit_ticket(id, window, cx),
            BoardEvent::LeavePage => self.leave_ticket_page(window, cx),
            BoardEvent::StartPage => self.start_from_page(window, cx),
            BoardEvent::MovePageBack => self.move_page_ticket_back(cx),
            BoardEvent::SavePage => self.save_ticket_page(cx),
            BoardEvent::SaveView => self.save_board_view(cx),
            BoardEvent::ApplyView(i) => self.apply_board_view(*i, cx),
            BoardEvent::RemoveView(i) => self.remove_board_view(*i, cx),
        }
        cx.notify();
    }

    /// What the page reads of `Desktop` to draw: each ticket's PR, diff, ask and reach, the projects' and agents' names, and the saved views.
    pub(crate) fn board_side(&self, cx: &App) -> Side {
        let b = self.board.read(cx);
        let wanted = b.wanted();
        let folders = b.named(&wanted);
        let extras = wanted
            .iter()
            .filter_map(|&i| b.tickets.tickets.get(i).map(|t| (i, t)))
            .map(|(i, t)| {
                let ask = self.agents.pending.iter().find(|p| p.agent_id == t.agent_id);
                let extra = Extra {
                    reach: self.ticket_reach(t),
                    key: ticket_key(&self.repo_name(&t.folder), t.number),
                    activity: self.agents.get(&t.agent_id).map(|a| a.activity.clone()),
                    asking: ask.map(|p| p.ask()),
                    answer: self.card_ask(&t.agent_id).map(|p| p.request_id.clone()),
                    pr: self.prs.get(&t.worktree).map(|pr| Pr {
                        glyph: pr_glyph(pr),
                        number: pr.number,
                        green: pr.checks.failed == 0 && pr.checks.pending == 0 && pr.checks.passed > 0,
                    }),
                    diff: self.repos.get(&t.worktree).filter(|r| !r.files.is_empty()).map(|r| r.totals()),
                };
                (i, extra)
            })
            .collect();
        let pickable = self.board_projects();
        let projects = folders
            .into_iter()
            .chain(pickable.iter().cloned())
            .map(|folder| {
                let name = self.repo_name(&folder);
                let mark = self.project_mark(&folder, crate::util::initials(&name));
                (folder, Project { name, mark })
            })
            .collect();
        let views = &self.store.board_views;
        Side {
            extras,
            pickable,
            projects,
            labels: self.store.agents.providers.keys().filter_map(|p| Some((p.clone(), self.store.agents.label(p)?))).collect(),
            installed: agents::providers::IDS.into_iter().filter(|p| self.installed(p)).collect(),
            offered: self.offered(),
            views: views.iter().map(|v| v.name.clone()).collect(),
            view_on: views.iter().position(|v| views::view_is_on(v, &b.filters, b.tab)),
            can_save_view: views::can_save_view(views, &b.filters),
            composer_repo: self.folder_is_repo(&b.composer.picks.values.folder),
        }
    }

    pub(crate) fn board_start_hook(cx: &mut Context<Self>) -> Subscription {
        let desktop = cx.entity().downgrade();
        // Ahead of the keymap: a textarea's ⌘↵ would put a newline in the prompt first.
        cx.intercept_keystrokes(move |ev, window, cx| {
            let k = &ev.keystroke;
            if k.key == "enter" && k.modifiers.secondary(cx) && desktop.update(cx, |d, cx| d.start_key(window, cx)).unwrap_or(false) {
                cx.stop_propagation();
            }
        })
    }

    /// ⌘↵ on a ticket's page, whichever field has the keyboard.
    fn start_key(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let b = self.board.read(cx);
        if self.screen != Screen::Board || b.page.is_none() || self.overlay.is_some() || b.menu.is_some() {
            return false;
        }
        self.start_from_page(window, cx);
        true
    }
}
