#[allow(dead_code)]
pub(crate) mod station;

use crate::board::Layout;
use crate::board::logic::Filters;
use crate::desktop::Desktop;
use agents::Team;
use agents::tickets::Ticket;
use git::github::Pr;
use gpui_kit::*;
use station::{COLUMNS, Stations, arrange};

pub(crate) struct FactoryState {
    /// Indices into the snapshot, per column of `COLUMNS`.
    pub(crate) columns: Stations,
    pub(crate) lists: Vec<ListState>,
    /// The rails drawn as full columns.
    pub(crate) open: [bool; COLUMNS.len()],
}

impl FactoryState {
    pub(crate) fn new(_: &mut Window, _: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let state = Self { columns: Default::default(), lists: COLUMNS.iter().map(|_| ListState::new(0, ListAlignment::Top, px(400.))).collect(), open: Default::default() };
        (state, Vec::new())
    }

    pub(crate) fn regroup<'a>(&mut self, tickets: &[Ticket], filters: &Filters, team: impl Fn(&Ticket) -> Option<&'a Team>, pr: impl Fn(&Ticket) -> Option<&'a Pr>) {
        self.columns = arrange(tickets, filters, team, pr);
        // A ticket's height follows its content, so the lists measure afresh whenever the snapshot changes.
        for (list, line) in self.lists.iter().zip(&self.columns) {
            let top = list.logical_scroll_top();
            list.reset(line.len());
            list.scroll_to(top);
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
