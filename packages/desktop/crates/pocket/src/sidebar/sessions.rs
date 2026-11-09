use crate::desktop::Desktop;
use crate::desktop::chrome::{RowMenu, empty, id, state};
use crate::sidebar::fold::{self, Fold};
use crate::sidebar::{in_tree, team_fold};
use crate::status::{Card, Kind};
use crate::util::{ago, now_ms};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use theme::*;
use ui::{self, State};

/// The sessions whose title holds `query`, in the order given.
pub(crate) fn matching(cards: Vec<Card>, query: &str) -> Vec<Card> {
    let query = query.to_lowercase();
    cards.into_iter().filter(|c| c.title.to_lowercase().contains(&query)).collect()
}

/// The cards in `tree`, the sessions they were started under, and every card started under one kept, wherever it runs: a team is listed whole from any of its worktrees.
pub(crate) fn with_family(cards: Vec<Card>, tree: Option<&str>, tree_of: impl Fn(&str) -> Option<String>) -> Vec<Card> {
    let mut kept: HashSet<String> = in_tree(cards.clone(), tree, tree_of).into_iter().map(|c| c.id).collect();
    let parents: HashMap<&str, &str> = cards.iter().map(|c| (c.id.as_str(), c.parent.as_str())).collect();
    for id in kept.clone() {
        let mut at = id.as_str();
        while let Some(parent) = parents.get(at).copied().filter(|p| parents.contains_key(p) && kept.insert(p.to_string())) {
            at = parent;
        }
    }
    while let Some(child) = cards.iter().find(|c| !kept.contains(&c.id) && kept.contains(&c.parent)) {
        kept.insert(child.id.clone());
    }
    cards.into_iter().filter(|c| kept.contains(&c.id)).collect()
}

/// Each card followed by the cards started under it, a level deeper, in the order given; a card whose parent isn't among them is at the top level.
pub(crate) fn nest(cards: Vec<Card>) -> Vec<(Card, usize)> {
    nest_by(cards, |c| c.id.clone(), |c| c.parent.clone())
}

/// [`nest`] over anything with an id and a parent; items that only have each other as parents stay at the top level.
pub(crate) fn nest_by<T>(items: Vec<T>, id: impl Fn(&T) -> String + Copy, parent: impl Fn(&T) -> String + Copy) -> Vec<(T, usize)> {
    let ids: HashSet<String> = items.iter().map(id).collect();
    let (top, mut rest): (Vec<T>, Vec<T>) = items.into_iter().partition(|i| !ids.contains(&parent(i)));
    let mut out = Vec::new();
    for item in top {
        place(item, 0, id, parent, &mut rest, &mut out);
    }
    out.extend(rest.into_iter().map(|i| (i, 0)));
    out
}

fn place<T>(item: T, depth: usize, id: impl Fn(&T) -> String + Copy, parent: impl Fn(&T) -> String + Copy, rest: &mut Vec<T>, out: &mut Vec<(T, usize)>) {
    let key = id(&item);
    let (children, others): (Vec<T>, Vec<T>) = std::mem::take(rest).into_iter().partition(|c| parent(c) == key);
    *rest = others;
    out.push((item, depth));
    for child in children {
        place(child, depth + 1, id, parent, rest, out);
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
        let query = self.sidebar.search.read(cx).value();
        let sessions = self.sessions_matching(&query);
        let (pinned, rest) = sessions.split_at(pinned_len(&sessions));
        let pinned_count = pinned.len();
        let mut n = 0;
        let pinned = self.card_rows(pinned, !query.is_empty(), &mut n, chips, cx);
        let rest = self.card_rows(rest, !query.is_empty(), &mut n, chips, cx);
        let cards = div().p(px(8.)).flex().flex_col().gap(px(2.)).when(!pinned.is_empty(), |d| d.child(ui::pinned_group(pinned, pinned_count))).children(rest);
        wrap.child(list.child(cards))
    }

    /// `rows`' cards, each lead's workers folding under it. `held` keeps every team open, as a search does; `n` counts the cards ⌃1–⌃9 reach.
    fn card_rows(&self, rows: &[(Card, usize)], held: bool, n: &mut usize, chips: bool, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut out = Vec::new();
        for b in self.sidebar.team_folds.branches(rows, held, self.session()) {
            out.push(self.card(*n, b.card.clone(), b.hides_session, chips, cx).into_any_element());
            *n += 1;
            if b.kids.is_empty() {
                continue;
            }
            out.push(Self::team_disclosure(&b, held, cx).into_any_element());
            let lead = &b.card.id;
            let inner = if !self.sidebar.closing.drawn(&Fold::Team(lead.clone()), !b.folded) {
                vec![]
            } else if b.folded {
                // Still shutting, so off the ⌃1–⌃9 count.
                self.card_rows(b.kids, held, &mut 0, false, cx)
            } else {
                self.card_rows(b.kids, held, n, chips, cx)
            };
            let body = div().relative().ml(px(16.)).pt(px(2.)).flex().flex_col().gap(px(2.)).when(!inner.is_empty(), |d| d.child(team_fold::rail(-7., 8.))).children(inner);
            out.push(fold::springing(id(format!("team-fold:{lead}")), !b.folded, b.kids_height(), body).into_any_element());
        }
        out
    }

    /// `hides_session`: the card is selected for a folded worker's sake.
    fn card(&self, i: usize, c: Card, hides_session: bool, chips: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = hides_session || self.session() == Some(c.id.as_str());
        let key = id(format!("card:{}", c.id));
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
            ui::pinned_row(key, selected, when, c.title, c.notice, branch, Some(pill), details.diff)
        } else {
            let model = details.model.then_some(c.model).filter(|m| !m.is_empty());
            let label = model.into_iter().chain(self.accounts.session_label(&c.id, &c.provider, &c.account)).collect::<Vec<_>>().join(" \u{b7} ");
            ui::session_row(key, selected, ui::agent_label(&c.provider, label), when, c.title, c.notice, branch, Some(pill), details.diff)
        };
        row.relative()
            .child(more)
            .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.focus_agent(&id, window, cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::{matching, nest, pinned_len, with_family};
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
    fn cards_that_only_have_each_other_as_parents_stay_at_the_top_level() {
        let cards = vec![child("a", "b"), child("b", "a")];
        assert_eq!(rows(nest(cards)), vec![("a".to_string(), 0), ("b".into(), 0)]);
    }

    #[test]
    fn a_child_in_its_own_worktree_is_listed_with_its_parent_from_either_tree() {
        let tree_of = |cwd: &str| Some(cwd.to_string());
        let cards = vec![at(card("lead", "idle"), "/p"), at(child("w1", "lead"), "/wt"), at(card("x", "idle"), "/wt")];
        assert_eq!(titles(with_family(cards.clone(), Some("/p"), tree_of)), vec!["lead", "w1"]);
        assert_eq!(titles(with_family(cards, Some("/wt"), tree_of)), vec!["lead", "w1", "x"]);
    }

    #[test]
    fn from_one_workers_tree_the_whole_team_is_listed_and_no_one_else() {
        let tree_of = |cwd: &str| Some(cwd.to_string());
        let cards = vec![at(card("lead", "idle"), "/p"), at(child("w1", "lead"), "/w1"), at(child("w2", "lead"), "/w2"), at(card("other", "idle"), "/p")];
        assert_eq!(titles(with_family(cards, Some("/w1"), tree_of)), vec!["lead", "w1", "w2"]);
    }

    #[test]
    fn the_pinned_box_holds_a_pinned_parent_with_its_children() {
        let listed = vec![(Card { pinned: true, ..card("lead", "idle") }, 0), (child("w1", "lead"), 1), (card("other", "idle"), 0)];
        assert_eq!(pinned_len(&listed), 2);
    }
}
