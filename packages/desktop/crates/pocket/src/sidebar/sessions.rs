use crate::desktop::Desktop;
use crate::desktop::chrome::{RowMenu, empty, state};
use crate::sidebar::in_tree;
use crate::status::{Card, Kind};
use crate::util::{ago, now_ms};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashSet;
use theme::*;
use ui::{self, State};

/// The sessions whose title holds `query`, in the order given.
pub(crate) fn matching(cards: Vec<Card>, query: &str) -> Vec<Card> {
    let query = query.to_lowercase();
    cards.into_iter().filter(|c| c.title.to_lowercase().contains(&query)).collect()
}

/// The cards in `tree`, and every card started under one kept, wherever it runs: a child in its own worktree is listed with its parent.
pub(crate) fn with_children(cards: Vec<Card>, tree: Option<&str>, tree_of: impl Fn(&str) -> Option<String>) -> Vec<Card> {
    let mut kept: HashSet<String> = in_tree(cards.clone(), tree, tree_of).into_iter().map(|c| c.id).collect();
    while let Some(child) = cards.iter().find(|c| !kept.contains(&c.id) && kept.contains(&c.parent)) {
        kept.insert(child.id.clone());
    }
    cards.into_iter().filter(|c| kept.contains(&c.id)).collect()
}

/// Each card followed by the cards started under it, a level deeper, in the order given; a card whose parent isn't among them is at the top level.
pub(crate) fn nest(cards: Vec<Card>) -> Vec<(Card, usize)> {
    let ids: HashSet<String> = cards.iter().map(|c| c.id.clone()).collect();
    let (top, mut rest): (Vec<Card>, Vec<Card>) = cards.into_iter().partition(|c| !ids.contains(&c.parent));
    let mut out = Vec::new();
    for card in top {
        place(card, 0, &mut rest, &mut out);
    }
    out
}

fn place(card: Card, depth: usize, rest: &mut Vec<Card>, out: &mut Vec<(Card, usize)>) {
    let (children, others): (Vec<Card>, Vec<Card>) = std::mem::take(rest).into_iter().partition(|c| c.parent == card.id);
    *rest = others;
    out.push((card, depth));
    for child in children {
        place(child, depth + 1, rest, out);
    }
}

/// How many of the rows go in the Pinned box: those up to the first unpinned top-level row, so a pinned parent keeps its children.
pub(crate) fn pinned_len(rows: &[(Card, usize)]) -> usize {
    rows.iter().position(|(c, depth)| *depth == 0 && !c.pinned).unwrap_or(rows.len())
}

impl Desktop {
    pub(super) fn session_list(&mut self, cx: &mut Context<Self>) -> Div {
        let search = div()
            .h(px(44.))
            .flex_none()
            .pl(px(18.))
            .pr(px(14.))
            .flex()
            .items_center()
            .gap(px(9.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(icon("search", 14., TEXT_3))
            .child(div().flex_1().text_size(px(13.)).child(Input::new(&self.sidebar.search).appearance(false).p_0().text_size(px(13.))));
        let list = div().id("cards").flex_1().min_h_0().overflow_y_scroll();
        let wrap = div().flex_1().min_h_0().flex().flex_col().child(search);
        if self.project.is_none() {
            return wrap.child(list.child(empty("Add a project with + to start.")));
        }
        let chips = self.chips.shown && self.overlay.is_none();
        let sessions = self.visible_sessions(cx);
        let pinned = pinned_len(&sessions);
        let mut rows: Vec<_> = sessions.into_iter().enumerate().map(|(i, (c, depth))| self.card(i, c, depth, chips, cx)).collect();
        let rest = rows.split_off(pinned);
        let cards = div().p(px(8.)).flex().flex_col().gap(px(2.)).when(!rows.is_empty(), |d| d.child(ui::pinned_group(rows))).children(rest);
        wrap.child(list.child(cards))
    }

    fn card(&self, i: usize, c: Card, depth: usize, chips: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.session() == Some(c.id.as_str());
        let (added, removed) = self.repos.get(&c.cwd).map(|r| r.totals()).unwrap_or_default();
        let id = c.id.clone();
        let pill = match c.kind {
            Kind::NotAttached => State::NotAttached,
            _ => state(c.status, added, removed),
        };
        let details = self.store.sidebar.details;
        let branch = self.repos.get(&c.cwd).map(|r| r.branch.clone()).filter(|_| details.branch);
        let menu = RowMenu::Session(c.id.clone());
        let open = self.row_menu.as_ref() == Some(&menu);
        let when = if open {
            String::new().into_any_element()
        } else if chips && i < 9 {
            ui::jump_chip(i + 1).into_any_element()
        } else if details.time {
            ago(c.at, now_ms()).into_any_element()
        } else {
            String::new().into_any_element()
        };
        let more = div()
            .absolute()
            .top(px(7.))
            .right(px(6.))
            .when(!open, |d| d.invisible().group_hover(ui::SESSION_ROW, |s| s.visible()))
            .child(self.row_menu_button(&c.id, menu.clone(), cx));
        let row = if c.pinned {
            ui::pinned_row(("card", i), selected, when, c.title, c.notice, branch, Some(pill), details.diff)
        } else {
            let model = details.model.then_some(c.model).filter(|m| !m.is_empty());
            let label = model.into_iter().chain(self.accounts.session_label(&c.id, &c.provider, &c.account)).collect::<Vec<_>>().join(" \u{b7} ");
            ui::session_row(("card", i), selected, ui::agent_label(&c.provider, label), when, c.title, c.notice, branch, Some(pill), details.diff)
        };
        row.relative()
            .when(depth > 0, |r| r.ml(px(16. * depth as f32)))
            .child(more)
            .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.focus_agent(&id, window, cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::{matching, nest, pinned_len, with_children};
    use crate::status::{self, Card};
    use agents::Summary;

    fn card(title: &str, status: &str) -> Card {
        status::card(&Summary { id: title.into(), title: title.into(), status: status.into(), attached: true, ..Default::default() }, "/p")
    }

    fn child(id: &str, parent: &str) -> Card {
        Card { parent: parent.into(), ..card(id, "idle") }
    }

    fn at(c: Card, cwd: &str) -> Card {
        Card { cwd: cwd.into(), ..c }
    }

    fn titles(cards: Vec<Card>) -> Vec<String> {
        cards.into_iter().map(|c| c.title).collect()
    }

    fn rows(rows: Vec<(Card, usize)>) -> Vec<(String, usize)> {
        rows.into_iter().map(|(c, depth)| (c.id, depth)).collect()
    }

    #[test]
    fn search_keeps_the_sessions_whose_title_matches_in_any_case() {
        let cards = vec![card("Fix CI", "idle"), card("Docs", "working"), card("fix login", "done")];
        assert_eq!(titles(matching(cards, "FIX")), vec!["Fix CI", "fix login"]);
    }

    #[test]
    fn a_status_change_keeps_the_session_order() {
        let before = vec![card("a", "working"), card("b", "idle"), card("c", "working")];
        let after = vec![card("a", "done"), card("b", "needsYou"), card("c", "idle")];
        assert_eq!(titles(matching(before, "")), vec!["a", "b", "c"]);
        assert_eq!(titles(matching(after, "")), vec!["a", "b", "c"]);
    }

    #[test]
    fn children_are_listed_under_their_parent_one_level_in_in_the_order_given() {
        let cards = vec![child("w2", "lead"), card("lead", "idle"), card("other", "idle"), child("w1", "lead")];
        assert_eq!(rows(nest(cards)), vec![("lead".to_string(), 0), ("w2".into(), 1), ("w1".into(), 1), ("other".into(), 0)]);
    }

    #[test]
    fn a_child_whose_parent_isnt_listed_stays_at_the_top_level() {
        let cards = vec![child("w1", "gone"), card("other", "idle")];
        assert_eq!(rows(nest(cards)), vec![("w1".to_string(), 0), ("other".into(), 0)]);
    }

    #[test]
    fn a_child_in_its_own_worktree_is_listed_with_its_parent_and_in_its_own_tree() {
        let tree_of = |cwd: &str| Some(cwd.to_string());
        let cards = vec![at(card("lead", "idle"), "/p"), at(child("w1", "lead"), "/wt"), at(card("x", "idle"), "/wt")];
        assert_eq!(titles(with_children(cards.clone(), Some("/p"), tree_of)), vec!["lead", "w1"]);
        assert_eq!(titles(with_children(cards, Some("/wt"), tree_of)), vec!["w1", "x"]);
    }

    #[test]
    fn the_pinned_box_holds_a_pinned_parent_with_its_children() {
        let listed = vec![(Card { pinned: true, ..card("lead", "idle") }, 0), (child("w1", "lead"), 1), (card("other", "idle"), 0)];
        assert_eq!(pinned_len(&listed), 2);
    }
}
