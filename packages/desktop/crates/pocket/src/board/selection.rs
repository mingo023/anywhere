use super::Menu;
use super::logic::{Action, Col, Columns, Row, Slot, action_field, change_reaches, column};
use crate::desktop::Desktop;
use gpui_kit::*;
use std::cell::Cell;
use std::collections::HashSet;
use std::rc::Rc;

/// Where the keyboard is on the Board, and the tickets picked for a change to all of them.
#[derive(Default, Debug)]
pub(super) struct Selection {
    pub(super) cursor: Option<String>,
    pub(super) picked: HashSet<String>,
    /// Under the cursor's ticket as last drawn, where a key opens its menu.
    pub(super) cursor_at: Rc<Cell<Point<Pixels>>>,
}

impl Selection {
    /// Moves the cursor `delta` places along `ids`, stopping at either end; from nowhere it lands on the first. Returns where it landed.
    fn step(&mut self, ids: &[&str], delta: isize) -> Option<usize> {
        let last = ids.len().checked_sub(1)?;
        let at = self.cursor.as_deref().and_then(|c| ids.iter().position(|id| *id == c));
        let next = at.map_or(0, |i| i.saturating_add_signed(delta).min(last));
        self.cursor = Some(ids[next].to_string());
        Some(next)
    }

    pub(super) fn toggle(&mut self, id: &str) {
        if !self.picked.remove(id) {
            self.picked.insert(id.to_string());
        }
    }

    /// Forgets what the page no longer shows, so a change never reaches a ticket out of sight, and what can no longer be picked.
    pub(super) fn keep(&mut self, shown: &HashSet<&str>, pickable: &HashSet<&str>) {
        self.picked.retain(|id| pickable.contains(id.as_str()));
        self.cursor = self.cursor.take().filter(|c| shown.contains(c.as_str()));
    }
}

/// The tickets on the page in reading order: the list's rows, or each column top to bottom.
pub(super) fn walk(list: bool, rows: &[Row], shown: &[Col], columns: &Columns) -> Vec<Slot> {
    if list {
        return rows.iter().filter_map(|r| if let Row::Ticket(slot) = r { Some(*slot) } else { None }).collect();
    }
    shown.iter().flat_map(|&col| columns[col as usize].iter().enumerate().map(move |(pos, &index)| Slot { col, pos, index })).collect()
}

impl Desktop {
    pub(super) fn step_board(&mut self, delta: isize, window: &mut Window, cx: &mut Context<Self>) {
        if self.board.page.is_some() {
            return self.step_ticket_page(delta, window, cx);
        }
        let slots = self.board.walk_slots();
        let tickets = &self.agents.board.tickets;
        let ids: Vec<&str> = slots.iter().map(|s| tickets.get(s.index).map_or("", |t| t.id.as_str())).collect();
        if let Some(at) = self.board.selection.step(&ids, delta) {
            self.board.reveal(slots[at]);
            cx.notify();
        }
    }

    pub(super) fn open_cursor_ticket(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(id) = self.board.selection.cursor.clone() {
            self.open_ticket(&id, window, cx);
        }
    }

    pub(super) fn pick_cursor_ticket(&mut self, cx: &mut Context<Self>) {
        let Some(id) = self.board.selection.cursor.clone() else { return };
        if self.ticket(&id).is_some_and(|t| !column(t).finished()) {
            self.board.selection.toggle(&id);
            cx.notify();
        }
    }

    pub(super) fn open_cursor_menu(&mut self, action: Action, cx: &mut Context<Self>) {
        let Some(id) = self.board.selection.cursor.clone().filter(|id| self.ticket(id).is_some_and(|t| change_reaches(column(t), action))) else { return };
        let at = self.board.selection.cursor_at.get();
        match action_field(action) {
            Some(field) => self.open_card_menu(&id, at, Some(field), cx),
            None => {
                self.close_menus();
                self.board.menu = Some(Menu::Status { id, at });
                cx.notify();
            }
        }
    }

    pub(super) fn pick_all_tickets(&mut self, cx: &mut Context<Self>) {
        let tickets = &self.agents.board.tickets;
        let ids = self.board.walk_slots().into_iter().filter(|s| !s.col.finished()).filter_map(|s| tickets.get(s.index)).map(|t| t.id.clone());
        self.board.selection.picked.extend(ids);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Selection, walk};
    use crate::board::logic::{COLS, Col, Columns, Row, Slot};
    use std::collections::HashSet;

    #[test]
    fn the_cursor_starts_on_the_first_ticket_and_stops_at_either_end() {
        let mut s = Selection::default();
        assert_eq!(s.step(&["a", "b"], -1), Some(0));
        assert_eq!(s.step(&["a", "b"], 1), Some(1));
        assert_eq!(s.step(&["a", "b"], 1), Some(1));
        assert_eq!(s.step(&["a", "b"], -1), Some(0));
        assert_eq!(s.step(&["a", "b"], -1), Some(0));
    }

    #[test]
    fn the_cursor_has_nowhere_to_go_on_an_empty_page() {
        assert_eq!(Selection::default().step(&[], 1), None);
    }

    #[test]
    fn a_cursor_on_a_ticket_that_went_away_starts_over() {
        let mut s = Selection { cursor: Some("gone".into()), ..Default::default() };
        assert_eq!(s.step(&["a", "b"], 1), Some(0));
    }

    #[test]
    fn picking_twice_lets_go() {
        let mut s = Selection::default();
        s.toggle("a");
        assert!(s.picked.contains("a"));
        s.toggle("a");
        assert!(s.picked.is_empty());
    }

    #[test]
    fn a_selection_forgets_what_the_page_stopped_showing() {
        let mut s = Selection { cursor: Some("b".into()), picked: ["a".to_string(), "b".to_string()].into(), ..Default::default() };
        s.keep(&["a"].into(), &["a"].into());
        assert_eq!(s.picked, HashSet::from(["a".to_string()]));
        assert_eq!(s.cursor, None);
    }

    #[test]
    fn a_picked_ticket_that_finishes_is_let_go_while_the_cursor_can_stay_on_it() {
        let mut s = Selection { cursor: Some("a".into()), picked: ["a".to_string()].into(), ..Default::default() };
        s.keep(&["a"].into(), &HashSet::new());
        assert!(s.picked.is_empty());
        assert_eq!(s.cursor.as_deref(), Some("a"));
    }

    #[test]
    fn the_walk_reads_columns_top_to_bottom_and_the_list_by_its_rows() {
        let mut columns: Columns = Default::default();
        columns[Col::Todo as usize] = vec![4, 2];
        columns[Col::Done as usize] = vec![7];
        columns[Col::Merged as usize] = vec![9];
        let got = walk(false, &[], &[Col::Todo, Col::Done], &columns);
        assert_eq!(got.iter().map(|s| s.index).collect::<Vec<_>>(), vec![4, 2, 7]);
        assert_eq!(got[1], Slot { col: Col::Todo, pos: 1, index: 2 });

        let slot = Slot { col: COLS[0], pos: 0, index: 3 };
        assert_eq!(walk(true, &[Row::Head(COLS[0]), Row::Ticket(slot)], &[], &columns), vec![slot]);
    }
}
