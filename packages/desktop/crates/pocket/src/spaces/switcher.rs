use super::{Reach, Space, flash_key, waiting};
use crate::desktop::Desktop;
use crate::settings::ssh_hosts::link_color;
use crate::sidebar::rail::{rail_button, rail_slot};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Instant;
use theme::*;

/// One space's button as `Desktop` hands it over each frame.
pub(crate) struct Button {
    space: Space,
    glyph: &'static str,
    /// A host's link dot colour.
    dot: Option<Token>,
    count: usize,
    tint: Option<u32>,
    tip: String,
    flashed: bool,
    on: bool,
}

pub(crate) enum SwitcherEvent {
    Switch(Space),
}

/// A row or column of space buttons; the aside's foot and the compact rail each have their own.
pub(crate) struct Switcher {
    /// Keeps the aside's ids and the rail's apart.
    prefix: &'static str,
    column: bool,
    buttons: Vec<Button>,
}

impl EventEmitter<SwitcherEvent> for Switcher {}

impl Switcher {
    pub(crate) fn new(prefix: &'static str, column: bool) -> Self {
        Self { prefix, column, buttons: Vec::new() }
    }

    pub(crate) fn set(&mut self, buttons: Vec<Button>) {
        self.buttons = buttons;
    }
}

impl Render for Switcher {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let prefix = self.prefix;
        let buttons = self.buttons.iter().enumerate().map(|(i, b)| {
            let (on, to, tip) = (b.on, b.space.clone(), b.tip.clone());
            rail_slot(SharedString::from(format!("{prefix}-space-{i}")), on)
                .child(icon(b.glyph, 16., if on { TEXT } else { TEXT_2 }))
                .when_some(b.dot, |d, dot| {
                    d.child(div().absolute().right(px(3.)).bottom(px(4.)).p(px(1.5)).rounded_full().bg(nav_fill()).child(ui::dot(6., dot)))
                })
                .when_some(b.tint.filter(|_| on), |d, c| {
                    d.child(div().absolute().left(px(10.)).bottom(px(1.)).w(px(12.)).h(px(2.5)).rounded(px(2.)).bg(rgb(c)))
                })
                .when(b.count > 0 && !on, |d| d.child(ui::count_badge(b.count).top(px(-1.)).right(px(-1.))))
                .when(b.flashed, |d| d.bg(ACCENT_BG))
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SwitcherEvent::Switch(to.clone()))))
        });
        div().flex().items_center().gap(px(if self.column { 6. } else { 2. })).when(self.column, |d| d.flex_col()).children(buttons)
    }
}

impl Desktop {
    /// Hands `switcher` one button per space: its icon, a host's link dot, its colour under the one on screen and the count of sessions waiting on you in the others.
    fn hand_spaces(&self, switcher: &Entity<Switcher>, order: &[Space], current: &Space, cx: &mut Context<Self>) {
        let waiting_in = self.waiting_projects(cx);
        let now = Instant::now();
        let buttons = order
            .iter()
            .enumerate()
            .map(|(i, space)| {
                let link = space.host().and_then(|id| self.fleet.hosts.links.get(id));
                let keys = if i < 9 { format!(" \u{b7} \u{2303}\u{2318}{}", i + 1) } else { String::new() };
                let off = if self.space_reach(space) == Reach::Offline { " (offline)" } else { "" };
                Button {
                    space: space.clone(),
                    glyph: self.space_glyph(space),
                    dot: space.host().map(|_| link_color(link)),
                    count: waiting(waiting_in.iter().map(String::as_str), space),
                    tint: self.space_colour(space, order),
                    tip: format!("{}{off}{keys}", self.space_name(space)),
                    flashed: self.spaces.flashed(&flash_key(space), now),
                    on: space == current,
                }
            })
            .collect();
        switcher.update(cx, |s, _| s.set(buttons));
    }

    pub(crate) fn on_switcher_event(&mut self, _: &Entity<Switcher>, ev: &SwitcherEvent, window: &mut Window, cx: &mut Context<Self>) {
        let SwitcherEvent::Switch(to) = ev;
        self.switch_space(to.clone(), window, cx);
    }

    /// The compact rail's space buttons.
    pub(crate) fn rail_spaces(&self, order: &[Space], current: &Space, cx: &mut Context<Self>) -> Entity<Switcher> {
        let spaces = self.sidebar.read(cx).rail_spaces.clone();
        self.hand_spaces(&spaces, order, current, cx);
        spaces
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
            .child(
                // With many hosts the buttons scroll, so Add and Settings stay in reach.
                div().id("aside-spaces").flex().items_center().gap(px(2.)).min_w_0().p(px(1.)).overflow_x_scroll().child({
                    let spaces = self.sidebar.read(cx).aside_spaces.clone();
                    self.hand_spaces(&spaces, order, current, cx);
                    spaces
                }),
            )
            .child(add)
            .child(div().flex_1())
            .child(settings)
    }

    /// Settings › Hosts with the add form open.
    pub(crate) fn add_host_from_sidebar(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus(cx);
        self.show_hosts(window, cx);
        self.open_add_host(None, window, cx);
    }
}
