use crate::desktop::Desktop;
use crate::desktop::chrome::{HEADER, Overlay, RAIL, Screen, Side, drag_area};
use crate::sidebar::column::{changes_badge, changes_dot};
use crate::terminal_view::context;
use agents::Level;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// `clicked`'s icon, with the column `shown` on `side`: the side to show, and whether to hide the column.
pub(crate) fn rail_pick(side: Side, shown: bool, clicked: Side) -> (Side, bool) {
    (clicked, shown && side == clicked)
}

fn rail_slot(id: impl Into<ElementId>, active: bool) -> Stateful<Div> {
    div()
        .id(id)
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

fn rail_button(id: impl Into<ElementId>, name: &str, active: bool) -> Stateful<Div> {
    rail_slot(id, active).child(icon(name, 17., if active { TEXT } else { TEXT_2 }))
}

impl Desktop {
    fn column_on_side(&self) -> bool {
        self.screen != Screen::Inbox && !self.sidebar.column_hidden
    }

    fn pick_side(&mut self, side: Side, cx: &mut Context<Self>) {
        (self.side, self.sidebar.column_hidden) = rail_pick(self.side, self.column_on_side(), side);
        self.screen = Screen::Sessions;
        self.refresh_graph(cx);
        cx.notify();
    }

    fn toggle_inbox(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.screen == Screen::Inbox {
            self.screen = Screen::Sessions;
            cx.notify();
        } else {
            self.open_inbox(window, cx);
        }
    }

    /// The compact layout's rail: sidebar toggle, project switcher, the column's sides, search, inbox and settings.
    pub(crate) fn nav(&self, cx: &mut Context<Self>) -> Div {
        let expand = rail_button("nav-expand", "sidebar-expand", false)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_sidebar(&crate::actions::ToggleSidebar, window, cx)));
        let open = self.sidebar.picker.open;
        let name = self.project.as_deref().map(|p| self.repo_name(p)).unwrap_or_default();
        let button = rail_slot("nav-project", open)
            .child(ui::repo_mark(&name, true, None).size(px(24.)).text_size(px(12.)))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.toggle_project_picker(window, cx);
            }));
        let project = div().relative().child(button).when(open, |d| d.child(ui::dropdown(36., ui::menu_in("projects-menu-in", self.project_picker(cx)))));
        let shown = self.column_on_side();
        let dirty = changes_badge(self.repo()).is_some();
        let sides = [(Side::Sessions, "comment"), (Side::Explorer, "file"), (Side::Changes, "branch")].into_iter().enumerate().map(|(i, (side, name))| {
            rail_button(("nav-side", i), name, shown && self.side == side)
                .when(side == Side::Changes && dirty, |d| d.child(changes_dot()))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_side(side, cx)))
        });
        let search = rail_button("nav-search", "search", self.overlay == Some(Overlay::Palette))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::Palette, window, cx)));
        let unseen = crate::inbox::count(&self.agents);
        let inbox = rail_button("nav-inbox", "inbox", self.screen == Screen::Inbox)
            .when(unseen > 0, |d| d.child(ui::count_badge(unseen).top(px(-3.)).right(px(-3.))))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_inbox(window, cx)));
        let settings = rail_button("nav-settings", "settings", false)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.project_settings(&crate::actions::ProjectSettings, window, cx)));
        let bars = self.usage().into_iter().map(|(p, left, level)| {
            let fill = if level == Level::Low { provider_color(p) } else { context::glyph(level) };
            div().w(px(24.)).h(px(3.)).flex().rounded(px(2.)).bg(SEPARATOR_STRONG).child(div().w(relative(left as f32 / 100.)).rounded(px(2.)).bg(fill))
        });
        let me = if self.initials.is_empty() { "ME".to_string() } else { self.initials.clone() };
        // The divider starts under the header so it doesn't cut through the traffic lights, which overhang the rail.
        let divider = div().absolute().top(px(HEADER)).bottom_0().right_0().w(px(0.5)).bg(SEPARATOR);
        drag_area(div().bg(SIDE))
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
            .children(sides)
            .child(search)
            .child(inbox)
            .child(
                div()
                    .mt_auto()
                    .flex()
                    .flex_col()
                    .items_center()
                    .gap(px(8.))
                    .child(settings)
                    .child(div().py(px(4.)).flex().flex_col().items_center().gap(px(3.)).children(bars))
                    .child(ui::avatar(&me, 30.).text_size(px(10.5))),
            )
    }
}

#[cfg(test)]
mod tests {
    use super::rail_pick;
    use crate::desktop::chrome::Side;

    #[test]
    fn a_side_icon_shows_its_side_and_the_shown_sides_icon_hides_the_column() {
        let got = [rail_pick(Side::Sessions, true, Side::Explorer), rail_pick(Side::Explorer, true, Side::Explorer), rail_pick(Side::Explorer, false, Side::Explorer)];
        assert_eq!(got, [(Side::Explorer, false), (Side::Explorer, true), (Side::Explorer, false)]);
    }
}
