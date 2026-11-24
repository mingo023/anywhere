use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, HEADER, SEAM, Side, drag_area, seam_handle_in};
use crate::explorer::Explorer;
use crate::git_ui::changes::Changes;
use crate::git_ui::pull_request::{PrSide, pr_wants_you};
use crate::git_ui::pull_requests::glyph;
use crate::region;
use crate::sidebar::column::changes_badge;
use crate::sidebar::rail::{rail_button, rail_slot};
use crate::sidebar::sessions::SessionList;
use crate::status::Status;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::icon_button_sized;
use workspace::tree::Axis;

const MIN: f32 = 280.;
const MAX: f32 = 600.;
/// The design's width, until the user drags the edge.
const OPEN: f32 = 334.;
pub(crate) const RAIL_WIDTH: f32 = 44.;

/// The payload of a drag on the panel's edge; its own type, so the left column's resize doesn't follow it.
struct PanelEdge;

/// `clicked`'s rail icon, with the panel `open` on `side`: the side to show, and whether the panel stays open.
pub(crate) fn rail_pick(side: Side, open: bool, clicked: Side) -> (Side, bool) {
    (clicked, !(open && side == clicked))
}

/// The panel's width with its edge dragged `wide` from the rail, or `None` once that's narrow enough to collapse it.
fn edge_width(wide: f32) -> Option<f32> {
    (wide >= MIN * 0.55).then(|| wide.clamp(MIN, MAX))
}

impl Desktop {
    pub(crate) fn pick_side(&mut self, side: Side, cx: &mut Context<Self>) {
        let (side, open) = rail_pick(self.side, self.sidebar.read(cx).panel_open, side);
        self.side = side;
        self.sidebar.update(cx, |s, _| s.panel_open = open);
        self.refresh_graph(cx);
        if self.side == Side::Changes && open {
            self.load_compare(cx);
        }
        cx.notify();
    }

    fn drag_panel_edge(&mut self, wide: f32, cx: &mut Context<Self>) {
        let width = edge_width(wide);
        let col = Column::Sessions as usize;
        // A drag move comes every frame, moved or not; notifying on each would redraw forever.
        if self.sidebar.read(cx).panel_open == width.is_some() && width.is_none_or(|w| self.widths[col] == Some(w)) {
            return;
        }
        self.sidebar.update(cx, |s, _| s.panel_open = width.is_some());
        if width.is_some() {
            self.widths[col] = width;
            self.save_soon(cx);
        }
        cx.notify();
    }

    /// How wide the right-hand panel is now: none once collapsed.
    pub(crate) fn workspace_panel_width(&self, cx: &App) -> f32 {
        self.sidebar.read(cx).panel.at() * self.width(Column::Sessions, OPEN)
    }

    /// The right-hand panel and its rail, sliding off the window's edge in Focus.
    pub(crate) fn workspace_side(&mut self, cx: &mut Context<Self>) -> Div {
        self.sync_panel_rail(cx);
        let panel = self.workspace_panel_width(cx);
        self.sync_panel(cx);
        let body = region::cached(&self.sidebar.read(cx).panel_view, region::column(panel));
        div()
            .flex()
            .flex_none()
            .h_full()
            .w(px((panel + RAIL_WIDTH) * self.sidebar.read(cx).unfocused()))
            .overflow_hidden()
            .child(body)
            .child(region::cached(&self.sidebar.read(cx).panel_rail, region::column(RAIL_WIDTH)))
    }

    fn panel_frame(&self, cx: &App) -> PanelFrame {
        let open = self.sidebar.read(cx).panel_open;
        PanelFrame {
            open,
            width: self.width(Column::Sessions, OPEN),
            shown: self.sidebar.read(cx).panel.at() > 0.,
            seam: open && self.overlay.is_none(),
            title: self.place(cx).map(|t| self.place_name(&t)).or_else(|| self.project.as_deref().map(|p| self.repo_name(p))).unwrap_or_default(),
        }
    }

    /// Hands the panel what it draws: its frame, and the entity of the side the rail picked.
    fn sync_panel(&mut self, cx: &mut Context<Self>) {
        let frame = self.panel_frame(cx);
        let body = frame.shown.then(|| match self.side {
            Side::Sessions => PanelBody::Sessions(self.sessions_view(cx)),
            Side::Explorer => PanelBody::Explorer(self.explorer_view(cx)),
            Side::Changes => PanelBody::Changes(self.changes_list(cx)),
            Side::PullRequest => PanelBody::PullRequest(self.pr.clone()),
        });
        self.sidebar.read(cx).panel_view.clone().update(cx, |p, cx| p.set(frame, body, cx));
    }

    pub(crate) fn on_panel_view_event(&mut self, _: &Entity<Panel>, ev: &PanelEvent, window: &mut Window, cx: &mut Context<Self>) {
        match *ev {
            PanelEvent::Drag(wide) => self.drag_panel_edge(wide, cx),
            PanelEvent::Reset => self.reset_width(Column::Sessions, cx),
            PanelEvent::NewSession => self.new_session(None, false, window, cx),
        }
    }

    /// Hands the rail at the window's right edge what it draws from `Desktop`'s state.
    pub(crate) fn sync_panel_rail(&self, cx: &mut Context<Self>) {
        let pr = self.shown_pr(cx).map(|(_, pr)| pr);
        let frame = PanelRailFrame {
            open: self.sidebar.read(cx).panel_open,
            side: self.side,
            waiting: self.sessions_matching("", cx).iter().any(|c| c.status == Status::NeedsYou),
            changed: self.repo(cx).map_or(0, |r| r.files.len()),
            pr: pr.map(|pr| (glyph(pr).1, pr_wants_you(pr))),
        };
        self.sidebar.read(cx).panel_rail.clone().update(cx, |r, cx| r.set(frame, cx));
    }

    pub(crate) fn on_panel_rail_event(&mut self, _: &Entity<PanelRail>, PanelRailEvent::Pick(side): &PanelRailEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.pick_side(*side, cx);
    }
}

/// What the panel draws around its body.
#[derive(PartialEq)]
pub(crate) struct PanelFrame {
    pub(crate) open: bool,
    pub(crate) width: f32,
    /// The glide has opened it far enough to draw its body.
    pub(crate) shown: bool,
    pub(crate) seam: bool,
    pub(crate) title: String,
}

/// The side the rail picked, as the entity that draws it.
#[derive(PartialEq)]
pub(crate) enum PanelBody {
    Sessions(Entity<SessionList>),
    Explorer(Entity<Explorer>),
    Changes(Entity<Changes>),
    PullRequest(Entity<PrSide>),
}

/// What the panel asks `Desktop` for.
#[derive(Clone, Copy)]
pub(crate) enum PanelEvent {
    /// The edge dragged this far from the rail.
    Drag(f32),
    Reset,
    NewSession,
}

impl EventEmitter<PanelEvent> for Panel {}

/// The right-hand panel: its title and add button over `body`, with the edge to drag.
fn panel_chrome(f: &PanelFrame, body: Option<AnyElement>, cx: &mut Context<Panel>) -> Div {
    let seam = f.seam.then(|| {
        seam_handle_in("panel-seam", Axis::Row, |_: &mut Panel, _, cx| cx.emit(PanelEvent::Reset), cx).left(px(-SEAM / 2.)).on_drag(PanelEdge, |_, _, _, cx| {
            cx.stop_propagation();
            cx.new(|_| EmptyView)
        })
    });
    let add = icon_button_sized("column-add", "plus", 28., TEXT_2).on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(PanelEvent::NewSession)));
    let header = div()
        .pl(px(18.))
        .pr(px(12.))
        .flex()
        .items_center()
        .gap(px(4.))
        .child(div().flex_1().text_size(px(16.)).font_weight(FontWeight::BOLD).truncate().child(f.title.clone()));
    // At its full width while the panel slides, so its rows don't reflow.
    let body = body.map(|body| {
        div()
            .bg(SIDE)
            .h_full()
            .w(px(f.width))
            .flex_none()
            .flex()
            .flex_col()
            .overflow_hidden()
            .border_l(px(0.5))
            .border_color(SEPARATOR)
            .child(drag_area(header.child(add)).h(px(HEADER)).flex_none().border_b(px(0.5)).border_color(SEPARATOR))
            .child(body)
    });
    div()
        .relative()
        .flex()
        .flex_none()
        .h_full()
        .overflow_hidden()
        .children(body)
        // Deferred so the handle paints over the page it straddles.
        .children(seam.map(deferred))
        .on_drag_move(cx.listener(|_, e: &DragMoveEvent<PanelEdge>, _, cx| cx.emit(PanelEvent::Drag(f32::from(e.bounds.right() - e.event.position.x)))))
}

/// The right-hand panel, empty once collapsed; a drag that collapses it can still bring it back.
#[derive(Default)]
pub(crate) struct Panel {
    drawn: Option<(PanelFrame, Option<PanelBody>)>,
}

impl Panel {
    pub(crate) fn set(&mut self, frame: PanelFrame, body: Option<PanelBody>, cx: &mut Context<Self>) {
        if self.drawn.as_ref().is_none_or(|(f, b)| *f != frame || *b != body) {
            self.drawn = Some((frame, body));
            cx.notify();
        }
    }
}

impl Render for Panel {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div().size_full().flex().flex_col();
        let Some((frame, body)) = self.drawn.as_ref() else { return root };
        let body = body.as_ref().map(|b| match b {
            PanelBody::Sessions(e) => e.clone().into_any_element(),
            PanelBody::Explorer(e) => e.clone().into_any_element(),
            PanelBody::Changes(e) => e.clone().into_any_element(),
            PanelBody::PullRequest(e) => e.clone().into_any_element(),
        });
        // `PanelFrame`'s seam and title are read by the chrome only.
        let chrome = panel_chrome(frame, body, cx);
        root.child(chrome)
    }
}

/// What the rail at the window's right edge asks `Desktop` for.
pub(crate) enum PanelRailEvent {
    Pick(Side),
}

impl EventEmitter<PanelRailEvent> for PanelRail {}

/// What `Desktop` hands the panel's rail each frame.
#[derive(PartialEq)]
pub(crate) struct PanelRailFrame {
    open: bool,
    side: Side,
    waiting: bool,
    changed: usize,
    /// The pull request's glyph colour, and whether it wants the user.
    pr: Option<(Token, bool)>,
}

/// The rail at the window's right edge: an icon per side, each opening the panel on it or closing it when shown.
#[derive(Default)]
pub(crate) struct PanelRail {
    frame: Option<PanelRailFrame>,
}

impl PanelRail {
    pub(crate) fn set(&mut self, frame: PanelRailFrame, cx: &mut Context<Self>) {
        if self.frame.as_ref() != Some(&frame) {
            self.frame = Some(frame);
            cx.notify();
        }
    }
}

impl Render for PanelRail {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let root = div().size_full().flex().flex_col();
        let Some(f) = self.frame.as_ref() else { return root };
        let sides = [(Side::Sessions, "sessions"), (Side::Explorer, "explorer"), (Side::Changes, "branch"), (Side::PullRequest, "github")];
        let sides = sides.into_iter().enumerate().map(|(i, (side, name))| {
            let active = f.open && f.side == side;
            let button = match f.pr.map(|(color, _)| color).filter(|_| side == Side::PullRequest) {
                Some(color) => rail_slot(("panel-side", i), active).child(icon(name, 17., color)),
                None => rail_button(("panel-side", i), name, active),
            };
            button
                .when(side == Side::Sessions && f.waiting, |d| d.child(ui::dot(6., WAITING_DOT).absolute().top(px(5.)).right(px(5.))))
                .when(side == Side::Changes && f.changed > 0, |d| d.child(changes_badge(f.changed)))
                .when(side == Side::PullRequest && f.pr.is_some_and(|(_, wants)| wants), |d| d.child(ui::dot(6., FAILED).absolute().top(px(5.)).right(px(5.))))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(PanelRailEvent::Pick(side))))
        });
        root.child(
            drag_area(div().bg(SIDE))
                .w(px(RAIL_WIDTH))
                .flex_none()
                .h_full()
                .pt(px((HEADER - 32.) / 2.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(6.))
                .border_l(px(0.5))
                .border_color(SEPARATOR)
                .children(sides),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX, MIN, edge_width, rail_pick};
    use crate::desktop::chrome::Side;

    #[test]
    fn the_dragged_edge_holds_between_the_narrowest_and_widest_and_collapses_the_panel_well_short_of_the_narrowest() {
        assert_eq!([100., 160., 384., 900.].map(edge_width), [None, Some(MIN), Some(384.), Some(MAX)]);
    }

    #[test]
    fn a_rail_icon_opens_its_side_and_the_shown_sides_icon_closes_the_panel() {
        let got = [
            rail_pick(Side::Sessions, true, Side::Explorer),
            rail_pick(Side::Explorer, true, Side::Explorer),
            rail_pick(Side::Explorer, false, Side::Explorer),
        ];
        assert_eq!(got, [(Side::Explorer, true), (Side::Explorer, false), (Side::Explorer, true)]);
    }
}
