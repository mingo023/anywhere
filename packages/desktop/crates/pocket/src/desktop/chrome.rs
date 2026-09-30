use crate::actions::{ToggleFocus, ToggleRail};
use crate::desktop::Desktop;
use crate::status::Status;
use gpui_kit::*;
use theme::*;
use ui::{self, State, icon_button_sized};

#[derive(Clone, Copy, PartialEq)]
pub enum Screen {
    Sessions,
    Inbox,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Side {
    Sessions,
    Explorer,
    Changes,
}

#[derive(Clone, Copy, PartialEq)]
pub enum Layout {
    Sidebars,
    Compact,
    Focus,
}

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
}

#[derive(Clone)]
pub enum Confirm {
    RemoveProject(String),
    DeleteWorktree { project: String, tree: String, branch: String, dirty: usize },
    Discard(Vec<String>),
}

/// The sidebar row whose `⋯` menu is open.
#[derive(Clone, PartialEq)]
pub enum RowMenu {
    Project(String),
    Tree { project: String, tree: String },
}

pub fn id(s: String) -> ElementId {
    ElementId::Name(s.into())
}

pub fn drag_area(d: Div) -> Div {
    d.on_mouse_down(MouseButton::Left, |ev, window, _| {
        if ev.click_count == 1 {
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
        .border_color(rgba(SEPARATOR))
        .bg(rgba(PAGE))
        .child(ui::breadcrumb(crumbs))
        .child(ui::meta_row(meta))
        .child(div().ml_auto().flex().flex_none().items_center().gap(px(8.)).child(right))
}

pub fn empty(text: impl Into<SharedString>) -> Div {
    div().p(px(16.)).text_size(px(13.5)).text_color(rgba(TEXT_3)).child(text.into())
}

impl Desktop {
    pub(crate) fn toggle_rail(&mut self, _: &ToggleRail, _: &mut Window, cx: &mut Context<Self>) {
        if self.layout == Layout::Compact {
            self.panel = !self.panel;
            cx.notify();
        }
    }

    pub(crate) fn toggle_focus(&mut self, _: &ToggleFocus, _: &mut Window, cx: &mut Context<Self>) {
        if self.panel {
            self.panel = false;
        } else {
            self.layout = match self.layout {
                Layout::Sidebars => Layout::Compact,
                Layout::Compact => Layout::Focus,
                Layout::Focus => Layout::Sidebars,
            };
        }
        cx.notify();
    }

    /// The user's width for `col`, the design's `fallback` until they drag its edge.
    pub fn width(&self, col: Column, fallback: f32) -> f32 {
        self.widths[col as usize].unwrap_or(fallback)
    }

    /// Lets the user drag `d`'s right edge to resize `col`.
    pub fn resizable(&self, d: Div, col: Column, cx: &mut Context<Self>) -> Div {
        // Occludes so the press starts a resize, not the drag area's window move.
        let handle = div()
            .id(("resize-column", col as usize))
            .absolute()
            .top_0()
            .bottom_0()
            .right_0()
            .w(px(5.))
            .occlude()
            .cursor_col_resize()
            .on_drag(col, |_, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| EmptyView)
            });
        d.relative().child(handle).on_drag_move(cx.listener(move |this, e: &DragMoveEvent<Column>, _, cx| {
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
        cx.notify();
    }

    /// A top bar's left padding and the sidebar toggle it starts with.
    pub(crate) fn bar_start(&self, cx: &mut Context<Self>) -> (f32, Option<Stateful<Div>>) {
        let toggle = |name: &str, cx: &mut Context<Self>| {
            icon_button_sized("focus-toggle", name, 28., TEXT_2).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_focus(&crate::actions::ToggleFocus, window, cx)))
        };
        match self.layout {
            // Leaves room for the window's traffic lights once the sidebars are hidden.
            Layout::Focus => (91., Some(toggle("sidebar-expand", cx))),
            Layout::Compact | Layout::Sidebars => (24., None),
        }
    }

    /// The page's top bar: sidebar toggle, breadcrumb and meta on the left, `right` on the far side.
    pub fn page_bar(&self, crumbs: Vec<String>, meta: Vec<AnyElement>, right: impl IntoElement, cx: &mut Context<Self>) -> Div {
        let (pad, toggle) = self.bar_start(cx);
        drag_area(ui::page_bar())
            .pl(px(pad))
            .children(toggle)
            .child(ui::breadcrumb(crumbs))
            .child(ui::meta_row(meta))
            .child(div().ml_auto().flex().flex_none().items_center().gap(px(8.)).child(right))
    }
}
