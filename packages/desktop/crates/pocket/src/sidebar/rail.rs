use crate::desktop::Desktop;
use crate::desktop::pages;
use crate::desktop::chrome::{HEADER, Overlay, RAIL, Screen, drag_area};
use crate::terminal_view::context;
use agents::Level;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

pub(super) fn rail_slot(id: impl Into<ElementId>, active: bool) -> Stateful<Div> {
    ui::control(div().id(id))
        .relative()
        .size(px(32.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(8.))
        .cursor_pointer()
        .when(active, |d| d.bg(FILL_3))
        .when(!active, |d| d.hover(|s| s.bg(FILL_2)))
}

pub(super) fn rail_button(id: impl Into<ElementId>, name: &str, active: bool) -> Stateful<Div> {
    rail_slot(id, active).child(icon(name, 17., if active { TEXT } else { TEXT_2 }))
}

impl Desktop {
    /// The compact layout's rail: sidebar toggle, project switcher, search, inbox and settings.
    pub(crate) fn nav(&self, cx: &mut Context<Self>) -> Div {
        let expand = rail_button("nav-expand", "sidebar-expand", false)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_sidebar(&crate::actions::ToggleSidebar, window, cx)));
        let open = self.sidebar.picker.open;
        let p = self.project.as_deref().unwrap_or_default();
        let mark = self.project_mark(p, ui::mark_letter(&self.repo_name(p)));
        let button = rail_slot("nav-project", open)
            .child(ui::repo_tile(&mark, 24., true, None))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.toggle_project_picker(window, cx);
            }));
        let project = div().relative().child(button).when(open, |d| d.child(ui::dropdown(36., ui::menu_in("projects-menu-in", self.project_picker(cx)))));
        let search = rail_button("nav-search", "search", self.overlay == Some(Overlay::Palette))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::Palette, window, cx)));
        let unseen = crate::inbox::count(&self.agents);
        let inbox = rail_button("nav-inbox", "inbox", self.screen == Screen::Inbox)
            .when(unseen > 0, |d| d.child(ui::count_badge(unseen).top(px(-3.)).right(px(-3.))))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_screen(Screen::Inbox, window, cx)));
        let pages: Vec<_> = pages::offered(&self.agents)
            .map(|p| {
                let screen = p.screen;
                rail_button(SharedString::from(format!("nav-{}", p.name.to_lowercase())), p.icon, self.screen == screen).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.toggle_screen(screen, window, cx)))
            })
            .collect();
        let settings = rail_button("nav-settings", "settings", self.screen == Screen::Settings)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_settings(&crate::actions::OpenSettings, window, cx)));
        let bars = self.usage().into_iter().map(|(p, left, level)| {
            let fill = if level == Level::Low { provider_color(p) } else { context::glyph(level) };
            div().w(px(24.)).h(px(3.)).flex().rounded(px(2.)).bg(SEPARATOR_STRONG).child(div().w(relative(left as f32 / 100.)).rounded(px(2.)).bg(fill))
        });
        // The divider starts under the header so it doesn't cut through the traffic lights, which overhang the rail.
        let divider = div().absolute().top(px(HEADER)).bottom_0().right_0().w(px(0.5)).bg(SEPARATOR);
        drag_area(div().bg(nav_fill()))
            .relative()
            .w(px(RAIL))
            .flex_none()
            .h_full()
            .pt(px(37.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(6.))
            .child(divider)
            .child(expand)
            .child(project)
            .child(search)
            .child(inbox)
            .children(pages)
            .child(
                div()
                    .mt_auto()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.))
                    .child(settings)
                    .child(div().py(px(4.)).flex().flex_col().items_center().gap(px(3.)).children(bars)),
            )
    }
}

