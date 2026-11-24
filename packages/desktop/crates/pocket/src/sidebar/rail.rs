use crate::desktop::Desktop;
use crate::desktop::chrome::{HEADER, Overlay, RAIL, Screen, drag_area};
use crate::desktop::pages;
use crate::sidebar::project_picker::ProjectPicker;
use crate::spaces::switcher::Switcher;
use crate::terminal_view::context;
use agents::Level;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

pub(crate) fn rail_slot(id: impl Into<ElementId>, active: bool) -> Stateful<Div> {
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

pub(crate) fn rail_button(id: impl Into<ElementId>, name: &str, active: bool) -> Stateful<Div> {
    rail_slot(id, active).child(icon(name, 17., if active { TEXT } else { TEXT_2 }))
}

/// What the rail asks `Desktop` for.
pub(crate) enum RailEvent {
    Expand,
    Projects,
    Palette,
    Page(Screen),
    Settings,
}

impl EventEmitter<RailEvent> for Rail {}

/// What `Desktop` hands the rail each frame: what it draws that it can't read for itself.
#[derive(PartialEq)]
pub(crate) struct RailFrame {
    pub(crate) mark: ui::Mark,
    pub(crate) spaces: Option<Entity<Switcher>>,
    pub(crate) palette: bool,
    pub(crate) unseen: usize,
    pub(crate) screen: Screen,
    pub(crate) pages: Vec<(&'static str, &'static str, Screen)>,
    pub(crate) usage: Vec<(&'static str, u64, Level)>,
}

/// The compact layout's rail: sidebar toggle, project switcher, search, inbox and settings.
pub(crate) struct Rail {
    picker: Entity<ProjectPicker>,
    frame: Option<RailFrame>,
}

impl Rail {
    pub(crate) fn new(picker: Entity<ProjectPicker>) -> Self {
        Self { picker, frame: None }
    }

    pub(crate) fn set(&mut self, frame: RailFrame, cx: &mut Context<Self>) {
        if self.frame.as_ref() != Some(&frame) {
            self.frame = Some(frame);
            cx.notify();
        }
    }
}

impl Render for Rail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div().size_full().flex().flex_col();
        let Some(f) = self.frame.as_ref() else { return root };
        let expand = rail_button("nav-expand", "sidebar-expand", false).on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(RailEvent::Expand)));
        let open = self.picker.read(cx).open;
        let button = ui::popover_trigger(
            rail_slot("nav-project", open).child(ui::repo_tile(&f.mark, 24., true, None)),
            open,
            cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(RailEvent::Projects)),
        );
        let project =
            div().relative().child(button).when(open, |d| d.child(ui::dropdown(36., ui::menu_in("projects-menu-in", div().child(self.picker.clone())))));
        let search = rail_button("nav-search", "search", f.palette).on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(RailEvent::Palette)));
        let inbox = rail_button("nav-inbox", "inbox", f.screen == Screen::Inbox)
            .when(f.unseen > 0, |d| d.child(ui::count_badge(f.unseen).top(px(-3.)).right(px(-3.))))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(RailEvent::Page(Screen::Inbox))));
        let pages: Vec<_> = f
            .pages
            .iter()
            .map(|&(name, icon, screen)| {
                rail_button(SharedString::from(format!("nav-{}", name.to_lowercase())), icon, f.screen == screen)
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(RailEvent::Page(screen))))
            })
            .collect();
        let settings = rail_button("nav-settings", "settings", f.screen == Screen::Settings)
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(RailEvent::Settings)));
        let bars = f.usage.iter().map(|&(p, left, level)| {
            let fill = if level == Level::Low { provider_color(p) } else { context::glyph(level) };
            div().w(px(24.)).h(px(3.)).flex().rounded(px(2.)).bg(SEPARATOR_STRONG).child(div().w(relative(left as f32 / 100.)).rounded(px(2.)).bg(fill))
        });
        // The divider starts under the header so it doesn't cut through the traffic lights, which overhang the rail.
        let divider = div().absolute().top(px(HEADER)).bottom_0().right_0().w(px(0.5)).bg(SEPARATOR);
        root.child(
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
                .children(f.spaces.clone())
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
                ),
        )
    }
}

impl Desktop {
    /// Hands the rail what it draws from `Desktop`'s state.
    pub(crate) fn sync_rail(&self, cx: &mut Context<Self>) {
        self.sync_picker(cx);
        let p = self.project.as_deref().unwrap_or_default();
        let (order, current) = (self.space_order(), self.space());
        let frame = RailFrame {
            mark: self.project_mark(p, ui::mark_letter(&self.repo_name(p))),
            spaces: (order.len() > 1).then(|| self.rail_spaces(&order, &current, cx)),
            palette: self.overlay == Some(Overlay::Palette),
            unseen: crate::inbox::count(&self.agents),
            screen: self.screen,
            pages: pages::offered(&self.agents).map(|p| (p.name, p.icon, p.screen)).collect(),
            usage: self.usage(),
        };
        self.sidebar.read(cx).rail.clone().update(cx, |r, cx| r.set(frame, cx));
    }

    pub(crate) fn on_rail_event(&mut self, _: &Entity<Rail>, ev: &RailEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            RailEvent::Expand => self.toggle_sidebar(&crate::actions::ToggleSidebar, window, cx),
            RailEvent::Projects => self.toggle_project_picker(window, cx),
            RailEvent::Palette => self.open(Overlay::Palette, window, cx),
            RailEvent::Page(screen) => self.toggle_screen(*screen, window, cx),
            RailEvent::Settings => self.open_settings(&crate::actions::OpenSettings, window, cx),
        }
    }
}
