use crate::actions::{ResetZoom, ToggleFocus, ToggleRail, ToggleSidebar, ZoomIn, ZoomOut};
use crate::desktop::Desktop;
use crate::removal::Removal;
use crate::settings::Section;
use crate::status::Status;
use crate::terminals::close::Busy;
use crate::sidebar::column::{changes_dot, has_changes};
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use theme::*;
use ui::{self, State, icon_button_sized};
use workspace::tree::Axis;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Screen {
    Sessions,
    Inbox,
    Automations,
    Settings,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Side {
    Sessions,
    Explorer,
    Changes,
}

pub use store::Layout;

pub(crate) const SEAM: f32 = 20.;

/// Where content starts to clear the window's traffic lights, at 100%.
pub(crate) const LIGHTS: f32 = 91.;

/// The traffic lights' offset from the window's corner, at 100%.
pub(crate) const LIGHTS_AT: f32 = 14.;

pub(crate) const RAIL: f32 = 56.;

/// A sidebar column's title bar.
pub(crate) const HEADER: f32 = 42.;

/// A sidebar column whose right edge the user drags.
#[derive(Clone, Copy, PartialEq)]
pub enum Column {
    Projects,
    Sessions,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Overlay {
    Palette,
    NewSession,
    AddRepo,
    More,
    Confirm,
    PairPhone,
    PhoneAccess,
}

#[derive(Clone)]
pub enum Confirm {
    RemoveProject(String),
    DeleteWorktree { removal: Removal, dirty: usize, lost: usize },
    DeleteLocal(String),
    TeardownFailed { removal: Removal, tail: String },
    Discard(Vec<String>),
    CloseSession(String),
    CloseTerminals { ids: Vec<String>, busy: Busy, worktree: String },
    Paste { pane: String, text: String },
    CloseFile(String),
    Quit(usize),
    OpenExternal(String),
    DeleteAutomation(String),
    ResetSection(Section),
    RevokeDevice { id: String, name: String },
}

/// The sidebar row whose `⋯` menu is open.
#[derive(Clone, PartialEq)]
pub enum RowMenu {
    Project(String),
    Tree { project: String, tree: String },
    /// The project's own Local, by its main worktree's path, or one the user added, by its id.
    Local(String),
    Session(String),
}

pub fn id(s: String) -> ElementId {
    ElementId::Name(s.into())
}

/// Armed by a press on a drag area. The window moves on the first mouse move, not the press, as a move takes the mouse up and the click with it.
struct WindowMove(bool);

impl Global for WindowMove {}

#[derive(Debug, PartialEq)]
enum BarPress {
    Arm,
    Zoom,
}

/// What a press on a drag area does; one on a control in it is the control's, so a quick second click doesn't zoom the window.
fn bar_press(click_count: usize, on_control: bool) -> Option<BarPress> {
    match click_count {
        _ if on_control => None,
        1 => Some(BarPress::Arm),
        2 => Some(BarPress::Zoom),
        _ => None,
    }
}

pub fn drag_area(d: Div) -> Div {
    let disarm = |_: &MouseUpEvent, _: &mut Window, cx: &mut App| cx.set_global(WindowMove(false));
    // Cleared on the way down, so only a control under this press, whose handler runs before ours on the way up, sets it.
    d.capture_any_mouse_down(|_, _, cx| cx.set_global(ui::ControlPress(false)))
        .on_mouse_down(MouseButton::Left, |ev, window, cx| match bar_press(ev.click_count, cx.global::<ui::ControlPress>().0) {
            Some(BarPress::Arm) => cx.set_global(WindowMove(true)),
            Some(BarPress::Zoom) => window.titlebar_double_click(),
            None => {}
        })
        .on_mouse_up(MouseButton::Left, disarm)
        .on_mouse_up_out(MouseButton::Left, disarm)
        .on_mouse_move(|_, window, cx| {
            if cx.try_global::<WindowMove>().is_some_and(|m| m.0) {
                cx.set_global(WindowMove(false));
                window.start_window_move();
            }
        })
}

pub fn state(status: Status, added: usize, removed: usize) -> State {
    match status {
        Status::NeedsYou => State::NeedsYou,
        Status::Failed => State::Failed,
        Status::Done => State::Done(added, removed),
        Status::Working => State::Working,
        Status::Idle => State::Idle(added, removed),
    }
}

pub fn column() -> Div {
    ui::side(div().w(px(348.)).flex_none().h_full().flex().flex_col().overflow_hidden())
}

/// A doc tab's bar over its content: breadcrumb and meta on the left, `right` on the far side.
pub fn doc_bar(crumbs: Vec<String>, meta: Vec<AnyElement>, right: impl IntoElement) -> Div {
    ui::page_bar()
        .border_t(px(0.5))
        .border_color(SEPARATOR)
        .bg(PAGE)
        .child(ui::breadcrumb(crumbs))
        .child(ui::meta_row(meta))
        .child(div().ml_auto().flex().flex_none().items_center().gap(px(8.)).child(right))
}

pub fn observe_banner() -> Div {
    div()
        .h(px(28.))
        .px(px(24.))
        .flex()
        .flex_none()
        .items_center()
        .text_size(px(12.5))
        .text_color(WAITING_TEXT)
        .bg(WAITING_BG)
        .child("Observe only — pocketd is managed elsewhere")
}

pub fn empty(text: impl Into<SharedString>) -> Div {
    div().p(px(16.)).text_size(px(13.5)).text_color(TEXT_2).child(text.into())
}

/// Where content starts to clear the traffic lights at `zoom`: their offset zooms, but AppKit keeps the buttons their own size.
pub(crate) fn lights(zoom: f32) -> f32 {
    LIGHTS_AT + (LIGHTS - LIGHTS_AT) / zoom
}

/// The left padding of a bar beside the compact rail: `pad`, or past the traffic lights where they overhang the rail.
pub(crate) fn past_lights(zoom: f32, pad: f32) -> f32 {
    (lights(zoom) - RAIL).max(pad)
}

/// A top bar's left padding outside Focus: past the traffic lights where they overhang the compact rail.
fn compact_pad(layout: Layout, screen: Screen, pad: f32, zoom: f32) -> f32 {
    // Only the rail is left of the Sessions page; other screens put a column between.
    if layout == Layout::Compact && screen == Screen::Sessions { past_lights(zoom, pad) } else { pad }
}

/// Zooms the window, moving the traffic lights so they stay in the zoomed title bar.
pub(crate) fn apply_zoom(zoom: f32, window: &mut Window) {
    window.set_zoom(zoom);
    window.set_traffic_light_position(point(px(LIGHTS_AT * zoom), px(LIGHTS_AT * zoom)));
}

pub fn sidebar_toggled(layout: Layout) -> Layout {
    match layout {
        Layout::Sidebars => Layout::Compact,
        Layout::Compact | Layout::Focus => Layout::Sidebars,
    }
}

impl Desktop {
    pub(crate) fn toggle_rail(&mut self, _: &ToggleRail, _: &mut Window, cx: &mut Context<Self>) {
        // The panel shows only on the Sessions screen; flipping it elsewhere would surprise on the way back.
        if self.layout != Layout::Focus && self.screen == Screen::Sessions {
            self.sidebar.panel_open = !self.sidebar.panel_open;
            cx.notify();
        }
    }

    pub(crate) fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.layout = sidebar_toggled(self.layout);
        self.save_soon(cx);
        cx.notify();
    }

    pub(crate) fn zoom_in(&mut self, _: &ZoomIn, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(self.store.appearance.zoomed_in(), window, cx);
    }

    pub(crate) fn zoom_out(&mut self, _: &ZoomOut, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(self.store.appearance.zoomed_out(), window, cx);
    }

    pub(crate) fn reset_zoom(&mut self, _: &ResetZoom, window: &mut Window, cx: &mut Context<Self>) {
        self.set_zoom(100, window, cx);
    }

    fn set_zoom(&mut self, percent: u32, window: &mut Window, cx: &mut Context<Self>) {
        self.store.appearance.zoom = Some(percent);
        apply_zoom(self.store.appearance.zoom_factor(), window);
        self.save_soon(cx);
        cx.notify();
    }

    pub(crate) fn toggle_focus(&mut self, _: &ToggleFocus, _: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.layout = match self.layout {
            Layout::Sidebars => Layout::Compact,
            Layout::Compact => Layout::Focus,
            Layout::Focus => Layout::Sidebars,
        };
        self.save_soon(cx);
        cx.notify();
    }

    /// The user's width for `col`, the design's `fallback` until they drag its edge.
    pub fn width(&self, col: Column, fallback: f32) -> f32 {
        self.widths[col as usize].unwrap_or(fallback)
    }

    /// A `SEAM`-thick handle for the caller to place across an edge between cells laid out along `axis`, its line lit on
    /// hover; double-clicking it calls `reset`. `None` while an overlay is open.
    pub(crate) fn seam(&self, id: impl Into<ElementId>, axis: Axis, reset: impl Fn(&mut Self, &mut Context<Self>) + 'static, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        // The deferred handle paints above overlays, so it would steal their clicks.
        if self.overlay.is_some() {
            return None;
        }
        let line = div().absolute().group_hover("seam", |s| s.bg(SEPARATOR_STRONG));
        // Occludes so the press starts a resize, not the drag area's window move.
        let handle = div().id(id).group("seam").absolute().occlude();
        let (line, handle) = match axis {
            Axis::Row => (line.top_0().bottom_0().left(px(SEAM / 2.)).w(px(1.)), handle.top_0().bottom_0().w(px(SEAM)).cursor_col_resize()),
            Axis::Column => (line.left_0().right_0().top(px(SEAM / 2.)).h(px(1.)), handle.left_0().right_0().h(px(SEAM)).cursor_row_resize()),
        };
        Some(handle.child(line).on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                if e.click_count == 2 {
                    reset(this, cx);
                }
            }),
        ))
    }

    /// Lets the user drag `d`'s right edge to resize `col`, or double-click it to reset.
    pub fn resizable(&self, d: Div, col: Column, cx: &mut Context<Self>) -> Div {
        let Some(handle) = self.seam(("resize-column", col as usize), Axis::Row, move |this, cx| this.reset_width(col, cx), cx) else {
            return d;
        };
        let handle = handle
            .right(px(-SEAM / 2.))
            .on_drag(col, |_, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| EmptyView)
            });
        // Deferred so the handle straddles the edge: `column()` clips its children.
        d.relative().child(deferred(handle)).on_drag_move(cx.listener(move |this, e: &DragMoveEvent<Column>, _, cx| {
            if *e.drag(cx) == col {
                this.drag_edge(col, f32::from(e.event.position.x - e.bounds.left()), cx);
            }
        }))
    }

    fn drag_edge(&mut self, col: Column, width: f32, cx: &mut Context<Self>) {
        let (min, max) = match col {
            Column::Projects => (200., 420.),
            Column::Sessions => (280., 600.),
        };
        self.widths[col as usize] = Some(width.clamp(min, max));
        self.save_soon(cx);
        cx.notify();
    }

    pub(crate) fn reset_width(&mut self, col: Column, cx: &mut Context<Self>) {
        self.widths[col as usize] = None;
        self.save_soon(cx);
        cx.notify();
    }

    /// A top bar's left padding, `pad` unless the traffic lights overhang it, and the sidebar toggle it starts with.
    pub(crate) fn bar_start(&self, pad: f32, cx: &mut Context<Self>) -> (f32, Option<Stateful<Div>>) {
        let toggle = |name: &str, cx: &mut Context<Self>| {
            icon_button_sized("focus-toggle", name, 28., TEXT_2).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_focus(&crate::actions::ToggleFocus, window, cx)))
        };
        match self.layout {
            // Leaves room for the window's traffic lights once the sidebars are hidden.
            Layout::Focus => (lights(self.store.appearance.zoom_factor()), Some(toggle("sidebar-expand", cx).relative().when(has_changes(self.repo()), |d| d.child(changes_dot())))),
            Layout::Compact | Layout::Sidebars => (compact_pad(self.layout, self.screen, pad, self.store.appearance.zoom_factor()), None),
        }
    }

    /// The page's top bar: sidebar toggle, breadcrumb and meta on the left, `right` on the far side.
    pub fn page_bar(&self, crumbs: Vec<String>, meta: Vec<AnyElement>, right: impl IntoElement, cx: &mut Context<Self>) -> Div {
        let (pad, toggle) = self.bar_start(12., cx);
        drag_area(ui::page_bar())
            .pl(px(pad))
            .children(toggle)
            .child(ui::breadcrumb(crumbs))
            .child(ui::meta_row(meta))
            .child(div().ml_auto().flex().flex_none().items_center().gap(px(8.)).child(right))
    }
}

#[cfg(test)]
mod tests {
    use super::{BarPress, Layout, Screen, bar_press, compact_pad, lights, past_lights, sidebar_toggled};

    #[test]
    fn a_double_click_on_a_bar_zooms_the_window_but_not_on_a_control_in_it() {
        assert_eq!([bar_press(1, false), bar_press(2, false), bar_press(3, false)], [Some(BarPress::Arm), Some(BarPress::Zoom), None]);
        assert_eq!([bar_press(1, true), bar_press(2, true)], [None, None]);
    }

    #[test]
    fn cmd_b_swaps_the_projects_sidebar_for_the_rail_and_brings_it_back() {
        assert_eq!([Layout::Sidebars, Layout::Compact, Layout::Focus].map(sidebar_toggled), [Layout::Compact, Layout::Sidebars, Layout::Sidebars]);
    }

    #[test]
    fn compact_bars_clear_the_traffic_lights_only_where_nothing_sits_between_them_and_the_rail() {
        let got = [(Layout::Compact, Screen::Sessions), (Layout::Compact, Screen::Inbox), (Layout::Sidebars, Screen::Sessions)].map(|(l, s)| compact_pad(l, s, 10., 1.));
        assert_eq!(got, [35., 10., 10.]);
    }

    #[test]
    fn the_traffic_lights_take_less_of_a_zoomed_in_window_and_more_of_a_zoomed_out_one() {
        assert_eq!([lights(1.), lights(2.), lights(0.5)], [91., 52.5, 168.]);
        assert_eq!([past_lights(1., 10.), past_lights(2., 10.), past_lights(0.5, 10.)], [35., 10., 112.]);
    }
}
