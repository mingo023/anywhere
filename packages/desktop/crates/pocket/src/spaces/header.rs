use super::{Reach, Space, colour, space_of};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use crate::settings::ssh_hosts::link_color;
use crate::util::LOCAL_ICON;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use workspace::Place;

pub(crate) fn glyph(space: &Space) -> &'static str {
    if *space == Space::Local { LOCAL_ICON } else { "server" }
}

/// An icon on a tile of a space's colour; This Mac's is plain.
pub(crate) fn tile(glyph: &str, tint: Option<u32>, size: f32) -> Div {
    let tile = div().size(px(size)).flex().flex_none().items_center().justify_center().rounded(px(size * 0.28));
    match tint {
        Some(c) => tile.bg(rgb(c)).child(icon(glyph, size * 0.6, WHITE)),
        None => tile.bg(FILL_4).child(icon(glyph, size * 0.6, TEXT_2)),
    }
}

impl Desktop {
    /// `space`'s colour: the one chosen for it, else the palette's by its place in the switcher.
    pub(crate) fn space_colour(&self, space: &Space, order: &[Space]) -> Option<u32> {
        colour(space, &self.store.sidebar.space_colours, order.iter().position(|s| s == space).unwrap_or_default())
    }

    /// Above Projects: the space on screen, its link and address, opening the space menu.
    pub(crate) fn space_header(&self, space: &Space, order: &[Space], cx: &mut Context<Self>) -> Div {
        let open = self.spaces.menu;
        let host = space.host().and_then(|id| self.fleet.hosts.list.iter().find(|h| h.id == id));
        let link = space.host().and_then(|id| self.fleet.hosts.links.get(id));
        let row = div()
            .id("aside-space")
            .h(px(34.))
            .mt(px(6.))
            .px(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_2))
            .when(open, |d| d.bg(FILL_2))
            .child(tile(glyph(space), self.space_colour(space, order), 22.))
            .child(div().min_w_0().max_w(px(120.)).truncate().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(self.space_name(space)))
            .children(host.map(|_| ui::dot(6., link_color(link))))
            .child(div().flex_1().min_w_0().truncate().text_size(px(12.)).text_color(TEXT_3).child(host.map(|h| h.address()).unwrap_or_default()))
            .child(icon("chevron-down", 12., TEXT_4))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.close_menus();
                this.spaces.menu = !open;
                cx.notify();
            }));
        div().relative().flex_none().child(row).when(open, |d| d.child(ui::dropdown(40., ui::menu_in("aside-space-menu-in", self.space_menu(space, order, cx)))))
    }

    fn space_menu(&self, space: &Space, order: &[Space], cx: &mut Context<Self>) -> Stateful<Div> {
        let name = self.space_name(space);
        let usable = self.space_terminal_target(space).is_some() && self.space_reach(space) == Reach::Online;
        let label = if *space == Space::Local { "Open terminal".to_string() } else { format!("Open terminal on {name}") };
        let terminal = ui::menu_row("space-terminal", "terminal", &label, None).when(!usable, |d| d.opacity(0.5).cursor_default()).when(usable, |d| {
            d.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_space_terminal(cx)))
        });
        let settings = ui::menu_row("space-settings", "settings", if *space == Space::Local { "Hosts settings\u{2026}" } else { "Host settings\u{2026}" }, None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.spaces.menu = false;
            this.show_hosts(window, cx);
        }));
        let menu = ui::pop(div().id("aside-space-menu"))
            .w(px(250.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.spaces.menu = false;
                cx.notify();
            }))
            .child(terminal);
        let Some(id) = space.host().map(str::to_string) else { return menu.child(settings) };
        let reconnect = (self.space_reach(space) == Reach::Offline).then(|| {
            let id = id.clone();
            ui::menu_row("space-reconnect", "reload", "Reconnect", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.spaces.menu = false;
                this.retry_host(&id);
                cx.notify();
            }))
        });
        let chosen = self.space_colour(space, order);
        let swatches = ACCENT_SWATCHES.map(|c| {
            let id = id.clone();
            div()
                .id(("space-colour", c as usize))
                .size(px(20.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .when(chosen == Some(c), |d| d.border_1().border_color(TEXT_3))
                .child(div().size(px(16.)).rounded_full().bg(rgb(c)))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.store.sidebar.space_colours.insert(id.clone(), c);
                    this.save_soon(cx);
                    cx.notify();
                }))
        });
        let remove = ui::danger_row("space-remove", "trash", "Remove host\u{2026}").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.spaces.menu = false;
            (this.confirm, this.overlay) = (Some(Confirm::RemoveHost { id: id.clone(), name: name.clone() }), Some(Overlay::Confirm));
            cx.notify();
        }));
        menu.children(reconnect)
            .child(settings)
            .child(ui::menu_divider())
            .child(ui::menu_label("Color"))
            .child(div().flex().gap(px(4.)).px(px(8.)).pb(px(6.)).children(swatches))
            .child(ui::menu_divider())
            .child(remove)
    }

    /// The project a terminal for `space` opens in: the one on screen if it is there, else the space's first.
    fn space_terminal_target(&self, space: &Space) -> Option<String> {
        self.project.clone().filter(|p| space_of(p) == *space).or_else(|| self.space_projects(space).into_iter().next())
    }

    /// A shell in the space's project; a machine's terminals live in its projects.
    pub(crate) fn open_space_terminal(&mut self, cx: &mut Context<Self>) {
        self.spaces.menu = false;
        let Some(project) = self.space_terminal_target(&self.space()) else { return };
        let tree = self.worktree.clone().filter(|_| self.project.as_deref() == Some(project.as_str()));
        self.select_tree(project, tree, cx);
        self.new_shell(Place::Pane(None), cx);
    }
}
