use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, Overlay, Screen, Side, column, drag_area, empty, state};
use crate::status::{Card, Kind, Status};
use crate::util::{ago, now_ms};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, State, icon_button_sized};

impl Desktop {
    pub(crate) fn column_view(&mut self, cx: &mut Context<Self>) -> Div {
        let body = match (self.screen, self.side) {
            (Screen::Inbox, _) => {
                let list = self.inbox_list(cx).w(px(self.width(Column::Sessions, 348.)));
                return self.resizable(list, Column::Sessions, cx);
            }
            (_, Side::Sessions) => self.session_list(cx).into_any_element(),
            (_, Side::Explorer) => self.explorer(cx).into_any_element(),
            (_, Side::Changes) => self.changes_list(cx).into_any_element(),
        };
        let totals = self.repo().filter(|r| !r.files.is_empty()).map(|r| r.totals());
        let tabs = [(Side::Sessions, "Sessions"), (Side::Explorer, "Explorer"), (Side::Changes, "Changes")].into_iter().enumerate().map(|(i, (side, label))| {
            let selected = self.side == side;
            div()
                .id(("column-tab", i))
                .flex_1()
                .h(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .cursor_pointer()
                .text_size(px(13.))
                .when(selected, |d| d.bg(rgba(FILL_4)).text_color(rgba(TEXT)).font_weight(FontWeight::SEMIBOLD))
                .when(!selected, |d| d.text_color(rgba(TEXT_2)).font_weight(FontWeight::MEDIUM).hover(|s| s.bg(rgba(FILL_2))))
                .map(|d| match (side, totals) {
                    (Side::Changes, Some((added, removed))) => d.child(ui::meta_diff(added, removed, 12.5)),
                    _ => d.child(label),
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.side = side;
                    cx.notify();
                }))
        });
        let tabs = div().p(px(8.)).flex().flex_none().gap(px(4.)).border_b(px(0.5)).border_color(rgba(SEPARATOR)).children(tabs);
        let column = column()
            .w(px(self.width(Column::Sessions, 334.)))
            .child(drag_area(self.column_header(cx)).h(px(42.)).flex_none().border_b(px(0.5)).border_color(rgba(SEPARATOR)))
            .child(tabs)
            .child(body);
        self.resizable(column, Column::Sessions, cx)
    }

    fn column_header(&self, cx: &mut Context<Self>) -> Div {
        let add = icon_button_sized("column-add", "plus", 28., TEXT_2)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx)));
        div()
            .pl(px(18.))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().text_size(px(16.)).font_weight(FontWeight::BOLD).child("Workspace"))
            .child(add)
    }

    fn session_list(&mut self, cx: &mut Context<Self>) -> Div {
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
            .child(div().flex_1().text_size(px(13.)).child(Input::new(&self.session_search).appearance(false).p_0().text_size(px(13.))));
        let list = div().id("cards").flex_1().min_h_0().overflow_y_scroll();
        let wrap = div().flex_1().min_h_0().flex().flex_col().child(search);
        let Some(project) = self.project.clone() else {
            return wrap.child(list.child(empty("Add a project with + to start.")));
        };
        let tree = self.cwd();
        let query = self.session_search.read(cx).value().to_lowercase();
        let mut cards: Vec<Card> = self
            .cards(&project)
            .into_iter()
            .filter(|c| self.tree_of(&c.cwd) == tree && c.title.to_lowercase().contains(&query))
            .collect();
        cards.sort_by_key(|c| c.status != Status::NeedsYou);
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
