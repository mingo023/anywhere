use crate::actions::{ToggleFocus, ToggleRail, ToggleSidebar};
use crate::desktop::Desktop;
use crate::removal::Removal;
use crate::status::Status;
use crate::terminals::close::Busy;
use crate::sidebar::column::{changes_badge, changes_dot};
use gpui_kit::*;
use gpui_kit::prelude::FluentBuilder as _;
use theme::*;
use ui::{self, State, icon_button_sized};

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

/// Where content starts to clear the window's traffic lights.
pub(crate) const LIGHTS: f32 = 91.;

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
    TeardownFailed { removal: Removal, tail: String },
    Discard(Vec<String>),
    CloseSession(String),
    CloseTerminals { ids: Vec<String>, busy: Busy, worktree: String },
    Paste { pane: String, text: String },
    CloseFile(String),
    Quit(usize),
    OpenExternal(String),
    DeleteAutomation(String),
}

/// The sidebar row whose `⋯` menu is open.
#[derive(Clone, PartialEq)]
pub enum RowMenu {
    Project(String),
    Tree { project: String, tree: String },
    Session(String),
}

pub fn id(s: String) -> ElementId {
    ElementId::Name(s.into())
}

/// Armed by a press on a drag area. The window moves on the first mouse move, not the press, as a move takes the mouse up and the click with it.
struct WindowMove(bool);

impl Global for WindowMove {}

pub fn drag_area(d: Div) -> Div {
    let disarm = |_: &MouseUpEvent, _: &mut Window, cx: &mut App| cx.set_global(WindowMove(false));
    d.on_mouse_down(MouseButton::Left, |ev, window, cx| match ev.click_count {
        1 => cx.set_global(WindowMove(true)),
        2 => window.titlebar_double_click(),
        _ => {}
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

pub fn sidebar_toggled(layout: Layout) -> Layout {
    match layout {
        Layout::Sidebars => Layout::Compact,
        Layout::Compact | Layout::Focus => Layout::Sidebars,
    }
}

impl Desktop {
    pub(crate) fn toggle_rail(&mut self, _: &ToggleRail, _: &mut Window, cx: &mut Context<Self>) {
        if self.layout == Layout::Compact {
            self.sidebar.column_hidden = !self.sidebar.column_hidden;
            cx.notify();
        }
    }

    pub(crate) fn toggle_sidebar(&mut self, _: &ToggleSidebar, _: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.layout = sidebar_toggled(self.layout);
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

    /// Lets the user drag `d`'s right edge to resize `col`, or double-click it to reset.
    pub fn resizable(&self, d: Div, col: Column, cx: &mut Context<Self>) -> Div {
        // The deferred handle paints above overlays, so it would steal their clicks.
        if self.overlay.is_some() {
            return d;
        }
        let line = div().absolute().top_0().bottom_0().left(px(SEAM / 2.)).w(px(1.)).group_hover("seam", |s| s.bg(SEPARATOR_STRONG));
        // Occludes so the press starts a resize, not the drag area's window move.
        let handle = div()
            .id(("resize-column", col as usize))
            .group("seam")
            .absolute()
            .top_0()
            .bottom_0()
            .right(px(-SEAM / 2.))
            .w(px(SEAM))
            .occlude()
            .cursor_col_resize()
            .child(line)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    if e.click_count == 2 {
                        this.reset_width(col, cx);
                    }
                }),
            )
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

    fn reset_width(&mut self, col: Column, cx: &mut Context<Self>) {
        self.widths[col as usize] = None;
        self.save_soon(cx);
        cx.notify();
    }

    /// A top bar's left padding, `pad` unless Focus needs room for the traffic lights, and the sidebar toggle it starts with.
    pub(crate) fn bar_start(&self, pad: f32, cx: &mut Context<Self>) -> (f32, Option<Stateful<Div>>) {
        let toggle = |name: &str, cx: &mut Context<Self>| {
            icon_button_sized("focus-toggle", name, 28., TEXT_2).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_focus(&crate::actions::ToggleFocus, window, cx)))
        };
        match self.layout {
            // Leaves room for the window's traffic lights once the sidebars are hidden.
            Layout::Focus => (LIGHTS, Some(toggle("sidebar-expand", cx).relative().when(changes_badge(self.repo()).is_some(), |d| d.child(changes_dot())))),
            Layout::Compact | Layout::Sidebars => (pad, None),
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
    use super::{Layout, sidebar_toggled};

    #[test]
    fn cmd_b_swaps_the_projects_sidebar_for_the_rail_and_brings_it_back() {
        assert_eq!([Layout::Sidebars, Layout::Compact, Layout::Focus].map(sidebar_toggled), [Layout::Compact, Layout::Sidebars, Layout::Sidebars]);
    }
}
