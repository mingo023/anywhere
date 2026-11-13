use super::Menu;
use super::logic::values;
use crate::board::filter_bar::value_row;
use crate::desktop::Desktop;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn activity_filters(&self, cx: &mut Context<Self>) -> Div {
        let search = div().w(px(220.)).h(px(26.)).px(px(8.)).flex().items_center().gap(px(6.)).rounded(px(7.)).bg(FILL_1).child(icon("search", 12., TEXT_3)).child(div().flex_1().min_w_0().text_size(px(12.5)).child(Input::new(&self.activity.search).appearance(false).p_0()));
        let project = self.activity_filter_chip("activity-project", "Project", self.activity.filters.projects.len(), Menu::Project, cx);
        let agent = self.activity_filter_chip("activity-agent", "Agent", self.activity.filters.providers.len(), Menu::Agent, cx);
        div().flex().items_center().gap(px(6.)).child(search).child(project).child(agent)
    }

    fn activity_filter_chip(&self, id: &'static str, label: &'static str, picked: usize, menu: Menu, cx: &mut Context<Self>) -> Div {
        let open = self.activity.menu == Some(menu);
        let text = if picked == 0 { label.to_string() } else { format!("{label} · {picked}") };
        let chip = div()
            .id(id)
            .h(px(26.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(5.))
            .rounded(px(13.))
            .border_1()
            .border_color(SEPARATOR)
            .cursor_pointer()
            .hover(|s| s.bg(FILL_2))
            .when(open || picked > 0, |d| d.bg(FILL_3))
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .child(text)
            .child(icon("chevron-down", 11., TEXT_3))
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                let was = this.activity.menu;
                this.close_menus();
                this.activity.menu = (was != Some(menu)).then_some(menu);
                cx.notify();
            }));
        div().relative().child(chip).when(open, |d| d.child(self.activity_filter_menu(menu, cx)))
    }

    fn activity_filter_menu(&self, menu: Menu, cx: &mut Context<Self>) -> Div {
        let field: fn(&super::logic::Item) -> &String = match menu {
            Menu::Project => |i| &i.project,
            Menu::Agent => |i| &i.provider,
        };
        let rows: Vec<_> = values(&self.activity.items, field).into_iter().enumerate().map(|(i, (value, count))| {
            let on = match menu {
                Menu::Project => self.activity.filters.projects.contains(&value),
                Menu::Agent => self.activity.filters.providers.contains(&value),
            };
            let (lead, name) = match menu {
                Menu::Project => (icon("folder", 13., TEXT_3).into_any_element(), self.repo_name(&value)),
                Menu::Agent => (theme::provider_icon(&value, 12., TEXT_2).into_any_element(), self.agent_name(&value)),
            };
            value_row(("activity-filter", i), on, lead, &name, count).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                let set = match menu {
                    Menu::Project => &mut this.activity.filters.projects,
                    Menu::Agent => &mut this.activity.filters.providers,
                };
                if !set.remove(&value) {
                    set.insert(value.clone());
                }
                cx.notify();
            }))
        })
        .collect();
        ui::dropdown_right(30., ui::menu_in("activity-filter-in", self.popup("activity-filter-menu", 260., cx).children(rows)))
    }
}
