use super::logic::Col;
use agents::tickets::Status;
use gpui_kit::*;
use theme::*;

/// A column's mark, or a ticket's when `status` says more than its column.
pub(super) fn status_glyph(id: impl Into<ElementId>, col: Col, status: Status) -> AnyElement {
    match (col, status) {
        (Col::Working, Status::Starting) => spinner(id, 13., TEXT_3).into_any_element(),
        (Col::Working, _) => spinner(id, 13., WORKING).into_any_element(),
        (Col::NeedsYou, Status::Failed) => icon("x", 13., FAILED).into_any_element(),
        (Col::NeedsYou, Status::Stopped) => icon("stop", 13., TEXT_3).into_any_element(),
        (Col::NeedsYou, _) => ui::dot(8., WAITING_DOT).into_any_element(),
        (Col::Done, _) => ui::dot(8., SUCCESS).into_any_element(),
        (Col::InReview, _) => icon("pull-request", 13., GOAL_TEXT).into_any_element(),
        (Col::Merged, _) => icon("merge", 13., MERGED).into_any_element(),
        (Col::Closed, _) => icon("run-cancelled", 13., TEXT_4).into_any_element(),
        (Col::Todo, _) => icon("agent-ring", 13., TEXT_4).into_any_element(),
        (Col::Queued, _) => icon("clock", 13., TEXT_3).into_any_element(),
    }
}

pub(super) fn pill() -> Div {
    div().h(px(20.)).px(px(6.)).flex().flex_none().items_center().gap(px(4.)).rounded(px(6.)).bg(FILL_2).text_size(px(11.5)).text_color(TEXT_2)
}
