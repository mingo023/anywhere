use super::logic::values;
use super::{Activity, ActivityEvent, Menu};
use crate::desktop::chrome::popup_closing;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::value_row;

impl Activity {
    pub(crate) fn filters(&self, cx: &mut Context<Self>) -> Div {
        let search = div()
            .w(px(220.))
            .h(px(26.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(7.))
            .bg(FILL_1)
            .child(icon("search", 12., TEXT_3))
            .child(div().flex_1().min_w_0().text_size(px(12.5)).child(Input::new(&self.search).appearance(false).p_0()));
        div()
            .flex()
            .items_center()
            .gap(px(6.))
            .child(search)
            .child(self.activity_filter_chip(Menu::Project, cx))
            .child(self.activity_filter_chip(Menu::Agent, cx))
    }

    fn activity_filter_chip(&self, menu: Menu, cx: &mut Context<Self>) -> Div {
        let open = self.menu == Some(menu);
        let picked = self.filters.picks(menu).len();
        let text = if picked == 0 { menu.label().to_string() } else { format!("{} · {picked}", menu.label()) };
        let chip = ui::popover_trigger(
            div()
                .id(("activity-filter-chip", menu as usize))
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
                .child(icon("chevron-down", 11., TEXT_3)),
            open,
            cx.listener(move |_, _: &MouseDownEvent, _, cx| cx.emit(ActivityEvent::OpenMenu(menu))),
        );
        div().relative().child(chip).when(open, |d| d.child(self.activity_filter_menu(menu, cx)))
    }

    fn activity_filter_menu(&self, menu: Menu, cx: &mut Context<Self>) -> Div {
        let rows: Vec<_> = values(&self.grouped.items, menu)
            .into_iter()
            .enumerate()
            .map(|(i, (value, count))| {
                let on = self.filters.picks(menu).contains(&value);
                let (lead, name) = match menu {
                    Menu::Project => (icon("folder", 13., TEXT_3).into_any_element(), self.name_of(&value)),
                    Menu::Agent => (theme::provider_icon(&value, 12., TEXT_2).into_any_element(), self.name_of(&value)),
                };
                value_row(("activity-filter", i), on, lead, &name, count).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.filters.toggle(menu, &value);
                    cx.emit(ActivityEvent::Changed);
                    cx.notify();
                }))
            })
            .collect();
        let popup = popup_closing("activity-filter-menu", 260., cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(ActivityEvent::CloseMenus)));
        ui::dropdown_right(30., ui::menu_in("activity-filter-in", popup.children(rows)))
    }

    /// The name `Desktop` handed down for a menu's value.
    fn name_of(&self, value: &str) -> String {
        self.side.names.get(value).cloned().unwrap_or_else(|| value.to_string())
    }
}
