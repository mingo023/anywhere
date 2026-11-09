mod card;
mod card_menu;
mod column;
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
mod tabs;
mod toast;
mod top_bar;
mod values;

use crate::actions::OpenBoard;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, Screen};
use agents::tickets::{Board, Ticket};
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use logic::{Action, COLS, Col, Columns, Filters, Landing, Opens, Order, Reach, Row, Tab, arrange, column, hidden_by, landing, opens, rows};
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
    Card { id: String, at: Point<Pixels> },
    Display,
    Filter,
    FormProject,
    FormAgent,
    FormBase,
}

pub struct BoardState {
    pub(crate) layout: Layout,
    tab: Tab,
    pub(crate) order: Order,
    pub(crate) show_closed: bool,
    pub(crate) filters: Filters,
    columns: Columns,
    hidden: usize,
    lists: Vec<ListState>,
    rows: Vec<Row>,
    rows_list: ListState,
    pub(crate) menu: Option<Menu>,
    search: Entity<InputState>,
    form: form::Form,
    toast: Option<Toast>,
    toasts: u64,
}

impl BoardState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let (form, mut subs) = form::Form::new(window, cx);
        subs.push(cx.subscribe(&search, |this: &mut Desktop, s, _: &InputEvent, cx| {
            this.board.filters.query = s.read(cx).value().to_string();
            this.board_changed();
            cx.notify();
        }));
        let state = Self {
            layout: Layout::default(),
            tab: Tab::default(),
            order: Order::default(),
            show_closed: false,
            filters: Filters::default(),
            columns: Default::default(),
            hidden: 0,
            lists: COLS.iter().map(|_| ListState::new(0, ListAlignment::Top, px(400.))).collect(),
            rows: Vec::new(),
            rows_list: ListState::new(0, ListAlignment::Top, px(400.)),
            menu: None,
            search,
            form,
            toast: None,
            toasts: 0,
        };
        (state, subs)
    }

    fn regroup(&mut self, board: &Board) {
        self.columns = arrange(&board.tickets, &self.filters, self.order);
        self.hidden = hidden_by(&board.tickets, &self.filters, self.tab, self.show_closed);
        // A ticket's height follows its content, so the lists measure afresh whenever the snapshot changes.
        for (list, line) in self.lists.iter().zip(&self.columns) {
            let top = list.logical_scroll_top();
            list.reset(line.len());
            list.scroll_to(top);
        }
        self.rows = rows(&self.columns, self.tab, self.show_closed);
        let top = self.rows_list.logical_scroll_top();
        self.rows_list.reset(self.rows.len());
        self.rows_list.scroll_to(top);
    }
}

impl Desktop {
    pub(crate) fn open_board(&mut self, _: &OpenBoard, window: &mut Window, cx: &mut Context<Self>) {
        if !self.agents.board_offered() {
            return;
        }
        self.close_menus();
        if self.overlay.is_some() {
            self.close_overlay(window, cx);
        }
        self.screen = Screen::Board;
        self.board_changed();
        window.focus(&self.root, cx);
        cx.notify();
    }

    /// Off the screen nothing reads the columns; opening it regroups.
    pub(crate) fn board_changed(&mut self) {
        if self.screen == Screen::Board {
            self.board.regroup(&self.agents.board);
        }
    }

    pub(crate) fn leave_unoffered_board(&mut self) {
        if self.screen == Screen::Board && !self.agents.board_offered() {
            self.screen = Screen::Sessions;
        }
    }

    pub(crate) fn escape_board(&mut self, cx: &mut Context<Self>) {
        self.screen = Screen::Sessions;
        cx.notify();
    }

    /// `N` on the Board, unless a field has the keyboard.
    pub(crate) fn board_new_ticket_key(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let pressed = self.screen == Screen::Board && self.overlay.is_none() && key.key == "n" && !key.modifiers.modified() && self.root.is_focused(window);
        if pressed {
            self.edit_ticket(None, window, cx);
        }
        pressed
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
            Some(Opens::Edit) => self.edit_ticket(Some(id), window, cx),
            Some(Opens::Session) => self.focus_agent(&t.agent_id, window, cx),
            Some(Opens::Worktree) => self.select_tree(t.folder, Some(t.worktree), cx),
            None => {}
        }
    }

    fn close_ticket(&mut self, id: &str, cx: &mut Context<Self>) {
        let Some(title) = self.ticket(id).map(|t| t.title.clone()) else { return };
        self.outbox.local.board_close(id);
        self.show_board_toast(format!("Closed “{title}”. Its worktree stays."), Some(id.to_string()), cx);
    }

    fn run_action(&mut self, action: Action, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        let Some(t) = self.ticket(id).cloned() else { return };
        match action {
            Action::Start | Action::StartAgain => self.outbox.local.board_start(id),
            Action::Edit => self.edit_ticket(Some(id), window, cx),
            Action::Delete => self.ask_delete_ticket(id.to_string(), window, cx),
            Action::MoveBack => self.outbox.local.board_move(id, agents::tickets::State::Todo, None),
            Action::Open => self.open_ticket(id, window, cx),
            Action::CopyBranch => {
                if let Some(branch) = self.board_tree(&t).map(|w| w.branch.clone()) {
                    cx.write_to_clipboard(ClipboardItem::new_string(branch.clone()));
                    self.show_board_toast(format!("Copied {branch}"), None, cx);
                }
            }
            Action::Close => self.close_ticket(id, cx),
            Action::Restore => self.outbox.local.board_reopen(id),
            Action::DeleteWorktree => self.ask_delete_worktree(t.folder, t.worktree, cx),
            Action::Separator => {}
        }
        cx.notify();
    }

    fn drop_ticket(&mut self, id: &str, from: Col, to: Col, before: Option<&str>, cx: &mut Context<Self>) {
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
