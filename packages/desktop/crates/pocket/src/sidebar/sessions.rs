use crate::desktop::Desktop;
use crate::desktop::chrome::{RowMenu, empty, state};
use crate::status::{Card, Kind};
use crate::util::{ago, now_ms};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, State};

/// The sessions whose title holds `query`, in the order given.
pub(crate) fn matching(cards: Vec<Card>, query: &str) -> Vec<Card> {
    let query = query.to_lowercase();
    cards.into_iter().filter(|c| c.title.to_lowercase().contains(&query)).collect()
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
        let (pinned, rest): (Vec<_>, Vec<_>) = self.visible_sessions(cx).into_iter().enumerate().partition(|(_, c)| c.pinned);
        let pinned: Vec<_> = pinned.into_iter().map(|(i, c)| self.card(i, c, chips, cx)).collect();
        let rest: Vec<_> = rest.into_iter().map(|(i, c)| self.card(i, c, chips, cx)).collect();
        let cards = div().p(px(8.)).flex().flex_col().gap(px(2.)).when(!pinned.is_empty(), |d| d.child(ui::pinned_group(pinned))).children(rest);
        wrap.child(list.child(cards))
    }

    fn card(&self, i: usize, c: Card, chips: bool, cx: &mut Context<Self>) -> Stateful<Div> {
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
            let model = if details.model { c.model } else { String::new() };
            ui::session_row(("card", i), selected, ui::agent_label(&c.provider, model), when, c.title, c.notice, branch, Some(pill), details.diff)
        };
        row.relative()
            .child(more)
            .on_mouse_down(MouseButton::Right, Self::open_row_menu(menu, cx))
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
    fn a_status_change_keeps_the_session_order() {
        let before = vec![card("a", "working"), card("b", "idle"), card("c", "working")];
        let after = vec![card("a", "done"), card("b", "needsYou"), card("c", "idle")];
        assert_eq!(titles(matching(before, "")), vec!["a", "b", "c"]);
        assert_eq!(titles(matching(after, "")), vec!["a", "b", "c"]);
    }
}
