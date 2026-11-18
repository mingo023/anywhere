use super::header::glyph;
use super::{Reach, Space, flash_key, waiting};
use crate::desktop::Desktop;
use crate::settings::ssh_hosts::link_color;
use crate::sidebar::rail::{rail_button, rail_slot};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Instant;
use theme::*;

impl Desktop {
    /// One button per space: its icon, a host's link dot, its colour under the one on screen and the count of sessions waiting on you in the others. `prefix` keeps the aside's and the rail's ids apart.
    pub(crate) fn space_buttons(&self, prefix: &str, order: &[Space], current: &Space, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let waiting_in = self.waiting_projects();
        order
            .iter()
            .enumerate()
            .map(|(i, space)| {
                let on = space == current;
                let link = space.host().and_then(|id| self.fleet.hosts.links.get(id));
                let count = waiting(waiting_in.iter().map(String::as_str), space);
                let tint = self.space_colour(space, order);
                let tip = {
                    let keys = if i < 9 { format!(" \u{b7} \u{2303}\u{2318}{}", i + 1) } else { String::new() };
                    let off = if self.space_reach(space) == Reach::Offline { " (offline)" } else { "" };
                    format!("{}{off}{keys}", self.space_name(space))
                };
                let to = space.clone();
                rail_slot(SharedString::from(format!("{prefix}-space-{i}")), on)
                    .child(icon(glyph(space), 16., if on { TEXT } else { TEXT_2 }))
                    .when(space.host().is_some(), |d| d.child(div().absolute().right(px(3.)).bottom(px(4.)).p(px(1.5)).rounded_full().bg(nav_fill()).child(ui::dot(6., link_color(link)))))
                    .when_some(tint.filter(|_| on), |d, c| d.child(div().absolute().left(px(10.)).bottom(px(1.)).w(px(12.)).h(px(2.5)).rounded(px(2.)).bg(rgb(c))))
                    .when(count > 0 && !on, |d| d.child(ui::count_badge(count).top(px(-1.)).right(px(-1.))))
                    .when(self.spaces.flashed(&flash_key(space), Instant::now()), |d| d.bg(ACCENT_BG))
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.switch_space(to.clone(), window, cx)))
                    .into_any_element()
            })
            .collect()
    }

    /// The aside's foot: the space buttons, add a host, and Settings at the right.
    pub(crate) fn space_switcher(&self, order: &[Space], current: &Space, cx: &mut Context<Self>) -> Div {
        let add = rail_button("aside-add-host", "plus", false)
            .tooltip(|window, cx| Tooltip::new("Add a host").build(window, cx))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.add_host_from_sidebar(window, cx)));
        let settings = rail_button("aside-settings", "settings", false)
            .tooltip(|window, cx| Tooltip::new("Settings").build(window, cx))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_settings(&crate::actions::OpenSettings, window, cx)));
        div()
            .mt(px(6.))
            .mx(px(-8.))
            .px(px(8.))
            .pt(px(6.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(2.))
            .border_t(px(0.5))
            .border_color(SEPARATOR)
            .children(self.space_buttons("aside", order, current, cx))
            .child(add)
            .child(div().flex_1())
            .child(settings)
    }

    /// Settings › Hosts with the add form open.
    pub(crate) fn add_host_from_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.show_hosts(window, cx);
        self.open_add_host(None, window, cx);
    }
}
