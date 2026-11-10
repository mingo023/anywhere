use super::Menu;
use super::logic::{COLS, Choice, FilterCat, Filters, LABELS, PRIORITIES, column, priority_name};
use super::parts::{bar_button, label_dot, priority_glyph, status_glyph};
use super::views::can_save_view;
use crate::desktop::Desktop;
use agents::providers;
use agents::tickets::{Status, Ticket};
use gpui_kit::component::input::Input;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

const FIELD_H: f32 = 30.;
const FIELD_GAP: f32 = 5.;
const ROW_H: f32 = 28.;

fn cat_icon(cat: FilterCat) -> AnyElement {
    match cat {
        FilterCat::Status => icon("status-todo", 13., TEXT_3).into_any_element(),
        FilterCat::Agent => icon("bot", 13., TEXT_3).into_any_element(),
        FilterCat::Project => icon("folder", 13., TEXT_3).into_any_element(),
        FilterCat::Priority => priority_glyph(3).into_any_element(),
        FilterCat::Label => label_dot("Feature").into_any_element(),
    }
}

fn filter_pop(id: &'static str) -> Stateful<Div> {
    ui::pop(div().id(id)).w(px(240.)).p(px(5.)).flex().flex_col().occlude()
}

/// A value in the filter menu: its tick, mark, name and how many tickets have it.
fn value_row(id: impl Into<ElementId>, on: bool, lead: impl IntoElement, name: &str, tickets: usize) -> Stateful<Div> {
    let tick = div().size(px(14.)).flex().flex_none().items_center().justify_center().rounded(px(4.)).when(on, |d| d.bg(INDIGO).child(icon("check", 9., ON_INDIGO))).when(!on, |d| d.border_1().border_color(SEPARATOR_STRONG));
    div().id(id).h(px(ROW_H)).px(px(8.)).flex().flex_none().items_center().gap(px(8.)).rounded(px(6.)).cursor_pointer().text_size(px(12.5)).hover(|s| s.bg(FILL_2)).child(tick).child(div().w(px(14.)).flex().flex_none().justify_center().child(lead)).child(div().flex_1().min_w_0().truncate().child(name.to_string())).child(div().text_size(px(12.)).text_color(TEXT_4).child(tickets.to_string()))
}

impl Desktop {
    pub(super) fn toggle_board_menu(&mut self, menu: Menu, cx: &mut Context<Self>) {
        let was = self.board.menu.clone();
        self.close_menus();
        self.board.menu = (was != Some(menu.clone())).then_some(menu);
        cx.notify();
    }

    pub(super) fn open_filter_menu(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.board.menu = Some(Menu::Filter);
        self.focus_filter_search(window, cx);
    }

    fn focus_filter_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.board.filter_search.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    fn bar_toggle(&self, id: &'static str, glyph: &'static str, tip: &'static str, menu: Menu, open: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        div().id(id).size(px(26.)).flex().items_center().justify_center().rounded(px(13.)).border_1().border_color(SEPARATOR).cursor_pointer().hover(|s| s.bg(FILL_2)).when(open, |d| d.bg(FILL_3)).child(icon(glyph, 13., TEXT_2)).when(!open, |d| d.tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            if open {
                this.close_menus();
                cx.notify();
            } else if menu == Menu::Filter {
                this.open_filter_menu(window, cx);
            } else {
                this.toggle_board_menu(menu.clone(), cx);
            }
        }))
    }

    /// `row`, adding `value` to its filter or taking it away when clicked. The menu stays open for the next one.
    fn toggles_filter<T: PartialEq + Clone + 'static>(&self, row: Stateful<Div>, value: T, field: fn(&mut Filters) -> &mut Choice<T>, cx: &mut Context<Self>) -> Stateful<Div> {
        row.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            field(&mut this.board.filters).toggle(value.clone());
            this.board_changed();
            this.aim_board_composer(cx);
            cx.notify();
        }))
    }

    /// The values `cat` offers, each with the name a search matches.
    fn filter_values(&self, cat: FilterCat, cx: &mut Context<Self>) -> Vec<(String, Stateful<Div>)> {
        let tickets = &self.agents.board.tickets;
        let filters = &self.board.filters;
        let count = |has: &dyn Fn(&Ticket) -> bool| tickets.iter().filter(|t| has(t)).count();
        match cat {
            FilterCat::Status => COLS
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let row = value_row(("board-filter-status", i), filters.status.values.contains(c), status_glyph(*c, Status::None), c.name(), count(&|t| column(t) == *c));
                    (c.name().to_string(), self.toggles_filter(row, *c, |f| &mut f.status, cx))
                })
                .collect(),
            FilterCat::Agent => providers::IDS
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let name = self.agent_name(p);
                    let row = value_row(("board-filter-agent", i), filters.agent.values.iter().any(|a| a == p), theme::provider_icon(p, 13., TEXT_2), &name, count(&|t| t.provider == *p));
                    (name, self.toggles_filter(row, p.to_string(), |f| &mut f.agent, cx))
                })
                .collect(),
            FilterCat::Project => self
                .projects()
                .into_iter()
                .enumerate()
                .map(|(i, p)| {
                    let name = self.repo_name(&p);
                    let row = value_row(("board-filter-project", i), filters.project.values.contains(&p), self.repo_tile_for(&p, 14.), &name, count(&|t| t.folder == p));
                    (name, self.toggles_filter(row, p, |f| &mut f.project, cx))
                })
                .collect(),
            FilterCat::Priority => PRIORITIES
                .iter()
                .enumerate()
                .map(|(i, p)| {
                    let row = value_row(("board-filter-priority", i), filters.priority.values.contains(p), priority_glyph(*p), priority_name(*p), count(&|t| t.priority == *p));
                    (priority_name(*p).to_string(), self.toggles_filter(row, *p, |f| &mut f.priority, cx))
                })
                .collect(),
            FilterCat::Label => LABELS
                .iter()
                .enumerate()
                .map(|(i, l)| {
                    let row = value_row(("board-filter-label", i), filters.label.values.iter().any(|v| v == l), label_dot(l), l, count(&|t| t.labels.iter().any(|v| v == l)));
                    (l.to_string(), self.toggles_filter(row, l.to_string(), |f| &mut f.label, cx))
                })
                .collect(),
        }
    }

    fn filter_menu(&self, cx: &mut Context<Self>) -> Div {
        let query = self.board.filter_search.read(cx).value().trim().to_lowercase();
        let field = div().h(px(FIELD_H)).pl(px(7.)).pr(px(6.)).mb(px(FIELD_GAP)).flex().flex_none().items_center().gap(px(6.)).border_b_1().border_color(SEPARATOR).text_size(px(12.5)).child(div().flex_1().min_w_0().child(Input::new(&self.board.filter_search).appearance(false).p_0())).child(ui::kbd("F"));
        let rows: Vec<Stateful<Div>> = if query.is_empty() { self.filter_cats(cx) } else { FilterCat::ALL.into_iter().flat_map(|cat| self.filter_values(cat, cx)).filter(|(name, _)| name.to_lowercase().contains(&query)).map(|(_, row)| row).collect() };
        let none = rows.is_empty().then(|| div().px(px(8.)).py(px(6.)).text_size(px(12.)).text_color(TEXT_3).child("No matching filters"));
        let main = filter_pop("board-filter-menu").max_h(px(380.)).overflow_y_scroll().child(field).children(rows).children(none);
        let side = match self.board.menu {
            Some(Menu::FilterIn(cat)) if query.is_empty() => {
                let at = FilterCat::ALL.iter().position(|c| *c == cat).unwrap_or_default();
                Some(div().mt(px(FIELD_H + FIELD_GAP + at as f32 * ROW_H)).child(filter_pop("board-filter-values").max_h(px(340.)).overflow_y_scroll().children(self.filter_values(cat, cx).into_iter().map(|(_, row)| row))))
            }
            _ => None,
        };
        // The panes share one outside: a click on the other is not a click away.
        let close = cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.close_menus();
            cx.notify();
        });
        ui::dropdown_right(30., ui::menu_in("board-filter-in", div().flex().items_start().gap(px(4.)).children(side).child(main).on_mouse_down_out(close)))
    }

    fn filter_cats(&self, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let open = if let Some(Menu::FilterIn(cat)) = self.board.menu { Some(cat) } else { None };
        FilterCat::ALL
            .into_iter()
            .enumerate()
            .map(|(i, cat)| {
                ui::menu_pick(("board-filter-cat", i), cat_icon(cat), cat.name(), false)
                    .h(px(ROW_H))
                    .text_size(px(12.5))
                    .when(open == Some(cat), |d| d.bg(FILL_2))
                    .child(icon("chevron-right", 11., TEXT_4))
                    .on_hover(cx.listener(move |this, hovered: &bool, _, cx| {
                        if *hovered && this.board.menu != Some(Menu::FilterIn(cat)) {
                            this.board.menu = Some(Menu::FilterIn(cat));
                            cx.notify();
                        }
                    }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.board.menu = Some(Menu::FilterIn(cat));
                        cx.notify();
                    }))
            })
            .collect()
    }

    pub(super) fn board_tools(&self, cx: &mut Context<Self>) -> Div {
        let open = matches!(self.board.menu, Some(Menu::Filter | Menu::FilterIn(_)));
        let menu = open.then(|| self.filter_menu(cx));
        let mark = self.board.filters.any().then(|| div().absolute().top(px(2.)).right(px(2.)).size(px(6.)).rounded(px(3.)).bg(INDIGO));
        let filter = div().relative().child(self.bar_toggle("board-add-filter", "filter", "Filter", Menu::Filter, open, cx)).children(mark).children(menu);
        let open = self.board.menu == Some(Menu::Display);
        let menu = open.then(|| self.display_menu(cx));
        let display = div().relative().child(self.bar_toggle("board-display", "sliders", "Display", Menu::Display, open, cx)).children(menu);
        div().flex().items_center().gap(px(6.)).child(filter).child(display)
    }

    /// A field's token's value: the one chosen, or how many.
    fn filter_token_value(&self, cat: FilterCat) -> Div {
        let f = &self.board.filters;
        let one: Option<(AnyElement, String)> = match cat {
            FilterCat::Status => f.status.values.first().filter(|_| f.count(cat) == 1).map(|c| (status_glyph(*c, Status::None).into_any_element(), c.name().to_string())),
            FilterCat::Agent => f.agent.values.first().filter(|_| f.count(cat) == 1).map(|a| (theme::provider_icon(a, 13., TEXT_2).into_any_element(), self.agent_name(a))),
            FilterCat::Project => f.project.values.first().filter(|_| f.count(cat) == 1).map(|p| (self.repo_tile_for(p, 14.).into_any_element(), self.repo_name(p))),
            FilterCat::Priority => f.priority.values.first().filter(|_| f.count(cat) == 1).map(|p| (priority_glyph(*p).into_any_element(), priority_name(*p).to_string())),
            FilterCat::Label => f.label.values.first().filter(|_| f.count(cat) == 1).map(|l| (label_dot(l).into_any_element(), l.clone())),
        };
        let body = div().flex().items_center().gap(px(5.));
        match one {
            Some((lead, name)) => body.child(lead).child(name),
            None => body.child(format!("{} {}", f.count(cat), cat.name().to_lowercase())),
        }
    }

    fn filter_token(&self, cat: FilterCat, i: usize, cx: &mut Context<Self>) -> Div {
        let f = &self.board.filters;
        let cell = |id: &'static str| div().id((id, i)).h_full().px(px(6.)).flex().items_center().border_l_1().border_color(SEPARATOR).cursor_pointer().text_color(TEXT_3).hover(|s| s.bg(FILL_2).text_color(TEXT));
        let verb = if f.is_not(cat) { "is not" } else if f.count(cat) > 1 { "is any of" } else { "is" };
        let flip = cell("board-token-verb").child(verb).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            let not = this.board.filters.is_not(cat);
            this.board.filters.set_not(cat, !not);
            this.board_changed();
            cx.notify();
        }));
        let value = cell("board-token-value").text_color(TEXT).font_weight(FontWeight::MEDIUM).child(self.filter_token_value(cat)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.open_filter_menu(window, cx);
            this.board.menu = Some(Menu::FilterIn(cat));
        }));
        let remove = cell("board-token-x").child(icon("x", 11., TEXT_3)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.board.filters.clear(cat);
            this.board_changed();
            cx.notify();
        }));
        div().h(px(26.)).pl(px(8.)).flex().flex_none().items_center().gap(px(6.)).overflow_hidden().rounded(px(7.)).border_1().border_color(SEPARATOR).text_size(px(12.)).text_color(TEXT_2).whitespace_nowrap().child(cat_icon(cat)).child(cat.name()).child(flip).child(value).child(remove)
    }

    /// The filters in force, on their own row only while there are any.
    pub(super) fn active_filters(&self, cx: &mut Context<Self>) -> Option<Div> {
        let f = &self.board.filters;
        let tokens: Vec<Div> = FilterCat::ALL.into_iter().enumerate().filter(|(_, cat)| f.count(*cat) > 0).map(|(i, cat)| self.filter_token(cat, i, cx)).collect();
        (!tokens.is_empty()).then(|| {
            let add = ui::icon_button_sized("board-token-add", "plus", 22., TEXT_3).capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                if matches!(this.board.menu, Some(Menu::Filter | Menu::FilterIn(_))) {
                    this.close_menus();
                    cx.notify();
                } else {
                    this.open_filter_menu(window, cx);
                }
            }));
            let clear = bar_button("board-filters-clear", Variant::Ghost, None, "Clear").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.clear_board_filters(window, cx)));
            let save = can_save_view(&self.store.board_views, f).then(|| bar_button("board-view-save", Variant::Secondary, None, "Save view").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.save_board_view(cx))));
            div().mx(px(8.)).mb(px(6.)).min_h(px(40.)).flex_none().px(px(8.)).py(px(6.)).flex().flex_wrap().items_center().gap(px(6.)).rounded(px(8.)).bg(FILL_1).children(tokens).child(add).child(div().flex_1()).child(clear).children(save)
        })
    }

    pub(super) fn clear_board_filters(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.board.filters = Filters::default();
        self.board.search.update(cx, |s, cx| s.set_value("", window, cx));
        self.board_changed();
        cx.notify();
    }
}
