use super::Menu;
use super::logic::Filters;
use crate::desktop::Desktop;
use agents::providers;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn toggle_board_menu(&mut self, menu: Menu, cx: &mut Context<Self>) {
        let was = self.board.menu.clone();
        self.close_menus();
        self.board.menu = (was != Some(menu.clone())).then_some(menu);
        cx.notify();
    }

    fn active_filter(&self, id: &'static str, label: String, clear: fn(&mut Filters), cx: &mut Context<Self>) -> Div {
        let remove = ui::icon_button_sized(id, "x", 16., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            clear(&mut this.board.filters);
            this.board_changed();
            cx.notify();
        }));
        div().h(px(24.)).pl(px(8.)).pr(px(4.)).flex().items_center().gap(px(4.)).rounded(px(7.)).bg(FILL_2).text_size(px(12.)).text_color(TEXT_2).child(label).child(remove)
    }

    pub(super) fn bar_toggle(&self, id: &'static str, glyph: &'static str, label: &'static str, menu: Menu, popup: Option<Div>, cx: &mut Context<Self>) -> Div {
        let open = self.board.menu.as_ref() == Some(&menu);
        let chip = ui::chip(id, open).child(icon(glyph, 12., TEXT_3)).child(label).capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            cx.stop_propagation();
            this.toggle_board_menu(menu.clone(), cx);
        }));
        div().relative().child(chip).children(popup.filter(|_| open))
    }

    /// `row`, setting the filter to `value` when clicked.
    fn filter_row(&self, row: Stateful<Div>, value: Option<String>, set: fn(&mut Filters, Option<String>), cx: &mut Context<Self>) -> Stateful<Div> {
        row.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            set(&mut this.board.filters, value.clone());
            this.close_menus();
            this.board_changed();
            cx.notify();
        }))
    }

    fn filter_menu(&self, cx: &mut Context<Self>) -> Div {
        let filters = &self.board.filters;
        let agents: Vec<_> = providers::IDS
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let on = filters.agent.as_deref() == Some(*p);
                self.filter_row(ui::menu_pick(("board-filter-agent-row", i), theme::provider_icon(p, 13., TEXT_2), &self.agent_name(p), on), (!on).then(|| p.to_string()), |f, v| f.agent = v, cx)
            })
            .collect();
        let projects: Vec<_> = self
            .projects()
            .into_iter()
            .enumerate()
            .map(|(i, p)| {
                let on = filters.project.as_deref() == Some(p.as_str());
                self.filter_row(ui::menu_pick(("board-filter-project-row", i), self.repo_tile_for(&p, 14.), &self.repo_name(&p), on), (!on).then(|| p.clone()), |f, v| f.project = v, cx)
            })
            .collect();
        ui::dropdown(30., ui::menu_in("board-filter-in", self.popup("board-filter-menu", 240., cx).child(ui::menu_label("Agent")).children(agents).child(ui::menu_divider()).child(ui::menu_label("Project")).children(projects)))
    }

    pub(super) fn filter_bar(&self, cx: &mut Context<Self>) -> Div {
        let filters = &self.board.filters;
        let agent = filters.agent.as_ref().map(|a| self.active_filter("board-filter-agent", format!("Agent is {}", self.agent_name(a)), |f| f.agent = None, cx));
        let project = filters.project.as_ref().map(|p| self.active_filter("board-filter-project", format!("Project is {}", self.repo_name(p)), |f| f.project = None, cx));
        let menu = (self.board.menu == Some(Menu::Filter)).then(|| self.filter_menu(cx));
        let add = self.bar_toggle("board-add-filter", "filter", "Filter", Menu::Filter, menu, cx);
        let menu = (self.board.menu == Some(Menu::Display)).then(|| self.display_menu(cx));
        let display = self.bar_toggle("board-display", "sliders", "Display", Menu::Display, menu, cx);
        div().h(px(36.)).flex_none().px(px(24.)).flex().items_center().gap(px(6.)).children(agent).children(project).child(add).child(div().flex_1()).child(display)
    }
}
