use super::logic::Col;
use agents::tickets::Status;
use crate::desktop::chrome;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use theme::*;
use ui::Variant;

pub(super) const CARD_RADIUS: f32 = 8.;

/// A column's mark, or a ticket's when `status` says more than its column.
pub(super) fn status_glyph(col: Col, status: Status) -> impl IntoElement {
    let (name, color) = match (col, status) {
        (Col::Working, Status::Starting) => ("status-queued", WORKING),
        (Col::Working, _) => ("status-half", WORKING),
        (Col::NeedsYou, Status::Failed) => ("status-closed", FAILED),
        (Col::NeedsYou, Status::Stopped) => ("status-stopped", TEXT_3),
        (Col::NeedsYou, _) => ("status-needs", WAITING_DOT),
        (Col::Done, _) => ("status-three-quarters", SUCCESS),
        (Col::InReview, _) => ("status-full", GOAL_TEXT),
        (Col::Merged, _) => ("status-merged", INDIGO),
        (Col::Closed, _) => ("status-closed", TEXT_4),
        (Col::Todo, _) => ("status-todo", TEXT_4),
        (Col::Queued, _) => ("status-queued", TEXT_3),
    };
    icon(name, 14., color)
}

pub(super) fn pill() -> Div {
    div().h(px(20.)).px(px(7.)).flex().flex_none().items_center().gap(px(4.)).rounded(px(10.)).border_1().border_color(SEPARATOR).text_size(px(11.5)).text_color(TEXT_2)
}

/// A column's mark, name and count.
pub(super) fn col_heading(col: Col, count: String) -> Div {
    div().flex().items_center().gap(px(8.)).text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(status_glyph(col, Status::None)).child(col.name()).child(div().font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(count))
}

/// The agent on a ticket, where a person's avatar would go.
pub(super) fn agent_avatar(id: impl Into<ElementId>, provider: &str, name: String) -> Stateful<Div> {
    let name = SharedString::from(name);
    div().id(id).size(px(18.)).flex().flex_none().items_center().justify_center().rounded(px(9.)).bg(FILL_2).child(theme::provider_icon(provider, 11., TEXT_2)).tooltip(move |window, cx| Tooltip::new(name.clone()).build(window, cx))
}

/// One of a few choices, as the page's tabs and the Display menu show them.
pub(super) fn choice(id: impl Into<ElementId>, label: &'static str, on: bool) -> Stateful<Div> {
    ui::chip(id, on).h(px(24.)).px(px(9.)).rounded(px(12.)).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(label)
}

/// The filled action of the page or dialog.
pub(super) fn action_button(id: impl Into<ElementId>, label: &str) -> Stateful<Div> {
    chrome::pill(id, Variant::Ghost, None, label).bg(INDIGO).text_color(ON_INDIGO).hover(|s| s.opacity(0.9)).active(|s| s.opacity(0.85))
}
