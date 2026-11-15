mod activity;
pub(crate) mod bar;
pub(crate) mod checks;
pub(crate) mod import;
pub(crate) mod metrics;
pub(crate) mod page;
mod pr;
pub(crate) mod profile_sheet;
mod spec;
pub(crate) mod station;
pub(crate) mod tabs;
pub(crate) mod workers;

use crate::board::Layout;
use crate::board::logic::{Filters, Slot};
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use agents::Team;
use agents::tickets::Ticket;
use git::github::Pr;
use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;
use std::collections::HashMap;
use station::{COLUMNS, Column, Counts, Stations, arrange, counts};

pub(crate) struct FactoryState {
    /// Indices into the snapshot, per column of `COLUMNS`.
    pub(crate) columns: Stations,
    pub(crate) lists: Vec<ListState>,
    /// The rails drawn as full columns.
    pub(crate) open: [bool; COLUMNS.len()],
    pub(crate) scroll: ScrollHandle,
    /// What the chips say, over every ticket.
    pub(crate) counts: Counts,
    /// The open Factory ticket page; `None` shows the Board.
    pub(crate) page: Option<page::Page>,
    /// Where a change request to the lead is typed.
    pub(crate) changes: Entity<InputState>,
    /// Where the words of a deny are typed, for a request that takes them.
    pub(crate) deny: Entity<InputState>,
    /// Where a hint for the lead of a Ticket that failed at Checks is typed.
    pub(crate) hint: Entity<InputState>,
    /// The tail of each ticket's failed command, cut when its detail arrived.
    pub(crate) outputs: HashMap<String, checks::Output>,
    /// The merge method menu of the PR review bar is open.
    pub(crate) merge_menu: bool,
    /// A merge was sent and pocketd has not answered.
    pub(crate) merging: bool,
    /// The profile sheet, while it is open.
    pub(crate) profile: Option<profile_sheet::ProfileSheet>,
}

impl FactoryState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let changes = cx.new(|cx| InputState::new(window, cx).placeholder("What should the lead change?"));
        let deny = cx.new(|cx| InputState::new(window, cx).placeholder("Tell it why (optional)"));
        let hint = cx.new(|cx| InputState::new(window, cx).placeholder("Hint for the lead"));
        let typed = cx.subscribe(&hint, |_: &mut Desktop, _, _: &InputEvent, cx| cx.notify());
        let desktop = cx.entity().downgrade();
        // Ahead of the keymap: the change request's field would take ⌘↵ first.
        let key = cx.intercept_keystrokes(move |ev, window, cx| {
            let k = &ev.keystroke;
            if k.key == "enter" && k.modifiers.secondary(cx) && desktop.update(cx, |d, cx| d.factory_key(window, cx)).unwrap_or(false) {
                cx.stop_propagation();
            }
        });
        let state = Self { columns: Default::default(), lists: COLUMNS.iter().map(|_| ListState::new(0, ListAlignment::Top, px(400.))).collect(), open: Default::default(), scroll: ScrollHandle::new(), counts: Counts::default(), page: None, changes, deny, hint, outputs: HashMap::new(), merge_menu: false, merging: false, profile: None };
        (state, vec![key, typed])
    }

    pub(crate) fn regroup<'a>(&mut self, tickets: &[Ticket], filters: &Filters, team: impl Fn(&Ticket) -> Option<&'a Team>, pr: impl Fn(&Ticket) -> Option<&'a Pr>) {
        self.columns = arrange(tickets, filters, team, pr);
        self.counts = counts(tickets);
        // A ticket's height follows its content, so the lists measure afresh whenever the snapshot changes.
        for (list, line) in self.lists.iter().zip(&self.columns) {
            let top = list.logical_scroll_top();
            list.reset(line.len());
            list.scroll_to(top);
        }
    }

    /// How many cards the columns hold.
    pub(crate) fn shown(&self) -> usize {
        shown(&self.columns)
    }

    pub(crate) fn toggle(&mut self, column: Column) {
        toggle(&mut self.open, column);
    }

    /// Scrolls the column that holds `slot`, and the row of columns, to show it.
    pub(crate) fn reveal(&self, slot: Slot) {
        if let Some(c) = self.columns.iter().position(|line| line.get(slot.pos) == Some(&slot.index)) {
            self.lists[c].scroll_to_reveal_item(slot.pos);
            self.scroll.scroll_to_item(c);
        }
    }
}

fn shown(columns: &Stations) -> usize {
    columns.iter().map(Vec::len).sum()
}

fn toggle(open: &mut [bool; COLUMNS.len()], column: Column) {
    open[column as usize] = !open[column as usize];
}

impl Desktop {
    pub(crate) fn open_factory_ticket(&mut self, id: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.factory.page = Some(page::Page::of(id));
        self.aim_plan_edits();
        self.outbox.local.factory_ticket(id);
        self.outbox.local.profile_list();
        if let Some(t) = self.agents.board.get(id) {
            self.outbox.local.factory_config(&t.folder);
        }
        window.focus(&self.root, cx);
        cx.notify();
    }

    pub(crate) fn leave_factory_ticket(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.factory.page = None;
        window.focus(&self.root, cx);
        cx.notify();
    }

    /// A Board snapshot came in: a page whose ticket went away closes.
    pub(crate) fn drop_stale_factory_page(&mut self) {
        if self.factory.page.as_ref().is_some_and(|p| p.stale(self.agents.board.get(&p.ticket))) {
            self.factory.page = None;
        }
        self.aim_plan_edits();
    }

    /// pocketd says a ticket's detail changed: ask again when its page is the one shown.
    pub(crate) fn factory_updated(&mut self, ticket: &str) {
        if self.screen == Screen::Board && self.factory.page.as_ref().is_some_and(|p| p.shows(ticket)) {
            self.outbox.local.factory_ticket(ticket);
        }
    }

    pub(crate) fn regroup_stations(&mut self) {
        let (agents, prs) = (&self.agents, &self.prs);
        self.factory.regroup(&agents.board.tickets, &self.board.filters, |t| agents.team(&t.agent_id), |t| prs.get(&t.worktree));
    }

    /// A team or a PR changed. Only the Factory layout, and a List grouped by station, place a ticket by them.
    pub(crate) fn stations_changed(&mut self) {
        if self.board.layout == Layout::Factory || crate::board::logic::by_station(&self.agents.board.tickets) {
            self.board_changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::station::Column;
    use super::{shown, toggle};

    #[test]
    fn a_rail_opens_on_a_click_and_closes_on_the_next() {
        let mut open = Default::default();
        toggle(&mut open, Column::Merged);
        assert_eq!(open, [false, false, false, false, false, false, true]);
        toggle(&mut open, Column::Merged);
        assert_eq!(open, [false; 7]);
    }

    #[test]
    fn the_layout_shows_the_cards_of_every_column() {
        assert_eq!(shown(&[vec![0, 1], vec![], vec![2], vec![], vec![], vec![], vec![3]]), 4);
    }
}
