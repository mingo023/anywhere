pub(crate) mod station;

use crate::board::Layout;
use crate::board::logic::{Filters, Slot};
use crate::desktop::Desktop;
use agents::Team;
use agents::tickets::Ticket;
use git::github::Pr;
use gpui_kit::*;
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
}

impl FactoryState {
    pub(crate) fn new(_: &mut Window, _: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let state = Self { columns: Default::default(), lists: COLUMNS.iter().map(|_| ListState::new(0, ListAlignment::Top, px(400.))).collect(), open: Default::default(), scroll: ScrollHandle::new(), counts: Counts::default() };
        (state, Vec::new())
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
        self.columns.iter().map(Vec::len).sum()
    }

    pub(crate) fn toggle(&mut self, column: Column) {
        self.open[column as usize] = !self.open[column as usize];
    }

    /// Scrolls the column that holds `slot`, and the row of columns, to show it.
    pub(crate) fn reveal(&self, slot: Slot) {
        if let Some(c) = self.columns.iter().position(|line| line.get(slot.pos) == Some(&slot.index)) {
            self.lists[c].scroll_to_reveal_item(slot.pos);
            self.scroll.scroll_to_item(c);
        }
    }
}

impl Desktop {
    pub(crate) fn regroup_stations(&mut self) {
        let (agents, prs) = (&self.agents, &self.prs);
        self.factory.regroup(&agents.board.tickets, &self.board.filters, |t| agents.team(&t.agent_id), |t| prs.get(&t.worktree));
    }

    /// A team or a PR changed. Only the Factory layout places a ticket by them.
    pub(crate) fn stations_changed(&mut self) {
        if self.board.layout == Layout::Factory {
            self.board_changed();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FactoryState;
    use super::station::Column;

    #[test]
    fn a_rail_opens_on_a_click_and_closes_on_the_next() {
        let mut state = FactoryState { columns: Default::default(), lists: Vec::new(), open: Default::default(), scroll: gpui_kit::ScrollHandle::new(), counts: Default::default() };
        state.toggle(Column::Merged);
        assert_eq!(state.open, [false, false, false, false, false, false, true]);
        state.toggle(Column::Merged);
        assert_eq!(state.open, [false; 7]);
    }

    #[test]
    fn the_layout_shows_the_cards_of_every_column() {
        let state = FactoryState { columns: [vec![0, 1], vec![], vec![2], vec![], vec![], vec![], vec![3]], lists: Vec::new(), open: Default::default(), scroll: gpui_kit::ScrollHandle::new(), counts: Default::default() };
        assert_eq!(state.shown(), 4);
    }
}
