use crate::desktop::Desktop;
use crate::desktop::chrome::{empty, state};
use crate::sidebar::in_tree;
use crate::status::{Card, Kind, Status};
use crate::util::{ago, now_ms};
use gpui_kit::component::input::Input;
use gpui_kit::*;
use theme::*;
use ui::{self, State};

/// The sessions whose title holds `query`, those that need you first.
fn matching(cards: Vec<Card>, query: &str) -> Vec<Card> {
    let query = query.to_lowercase();
    let mut cards: Vec<Card> = cards.into_iter().filter(|c| c.title.to_lowercase().contains(&query)).collect();
    cards.sort_by_key(|c| c.status != Status::NeedsYou);
    cards
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
            .border_color(rgba(SEPARATOR))
            .child(icon("search", 14., TEXT_3))
            .child(div().flex_1().text_size(px(13.)).child(Input::new(&self.sidebar.search).appearance(false).p_0().text_size(px(13.))));
        let list = div().id("cards").flex_1().min_h_0().overflow_y_scroll();
        let wrap = div().flex_1().min_h_0().flex().flex_col().child(search);
        let Some(project) = self.project.clone() else {
            return wrap.child(list.child(empty("Add a project with + to start.")));
        };
        let tree = self.cwd();
        let cards = in_tree(self.cards(&project), tree.as_deref(), |cwd| self.tree_of(cwd));
        let cards = matching(cards, &self.sidebar.search.read(cx).value());
        let cards: Vec<_> = cards.into_iter().enumerate().map(|(i, c)| self.card(i, c, cx)).collect();
        wrap.child(list.child(div().p(px(8.)).flex().flex_col().gap(px(2.)).children(cards)))
    }

    fn card(&self, i: usize, c: Card, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.session.as_ref() == Some(&c.id);
        let (added, removed) = self.repos.get(&c.cwd).map(|r| r.totals()).unwrap_or_default();
        let id = c.id.clone();
        let pill = match c.kind {
            Kind::NotAttached => State::NotAttached,
            _ => state(c.status, added, removed),
        };
        let lead = ui::provider_label(&c.provider, c.kind == Kind::Ended);
        let branch = self.repos.get(&c.cwd).map(|r| r.branch.clone());
        ui::session_row(("card", i), selected, lead, ago(c.at, now_ms()), c.title, branch, Some(pill))
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.focus_agent(&id, window, cx)))
    }
}

#[cfg(test)]
mod tests {
    use super::matching;
    use crate::status::{self, Card};
    use agents::Summary;

    fn card(title: &str, status: &str) -> Card {
        status::card(&Summary { id: title.into(), title: title.into(), status: status.into(), attached: true, ..Default::default() }, "/p")
    }

    fn titles(cards: Vec<Card>) -> Vec<String> {
        cards.into_iter().map(|c| c.title).collect()
    }

    #[test]
    fn search_keeps_the_sessions_whose_title_matches_in_any_case() {
        let cards = vec![card("Fix CI", "idle"), card("Docs", "working"), card("fix login", "done")];
        assert_eq!(titles(matching(cards, "FIX")), vec!["Fix CI", "fix login"]);
    }

    #[test]
    fn search_lifts_sessions_that_need_you_and_keeps_the_rest_in_order() {
        let cards = vec![card("a", "done"), card("b", "needsYou"), card("c", "working"), card("d", "needsYou"), card("e", "idle")];
        assert_eq!(titles(matching(cards, "")), vec!["b", "d", "a", "c", "e"]);
    }
}
