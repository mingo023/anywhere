use super::logic::Col;
use agents::tickets::Status;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use theme::*;
use ui::Variant;

pub(super) const CARD_RADIUS: f32 = 9.;

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

pub(super) fn priority_glyph(priority: u8) -> impl IntoElement {
    let (name, color) = match priority {
        1 => ("priority-urgent", WORKING),
        2 => ("priority-high", TEXT_3),
        3 => ("priority-medium", TEXT_3),
        4 => ("priority-low", TEXT_3),
        _ => ("priority-none", TEXT_4),
    };
    icon(name, 14., color)
}

pub(super) fn label_dot(label: &str) -> Div {
    let color = match label {
        "Bug" => FAILED,
        "Docs" => WAITING_DOT,
        "Infra" => GOAL_TEXT,
        _ => MERGED,
    };
    div().size(px(8.)).flex_none().rounded(px(4.)).bg(color)
}

/// `field` with a multi-line input's built-in padding, which its style can't reach, pulled back out.
pub(super) fn flush_textarea(field: Div) -> Div {
    field.mx(px(-10.)).my(px(-8.))
}

pub(super) fn pill() -> Div {
    div().h(px(20.)).px(px(7.)).flex().flex_none().items_center().gap(px(4.)).rounded(px(10.)).border_1().border_color(SEPARATOR).text_size(px(11.5)).text_color(TEXT_2)
}

pub(super) fn prop_chip() -> Div {
    div().h(px(22.)).px(px(6.)).flex().flex_none().items_center().gap(px(5.)).max_w(px(170.)).rounded(px(11.)).border_1().border_color(SEPARATOR).text_size(px(12.)).text_color(TEXT_2).whitespace_nowrap()
}

/// A column's mark, name and count.
pub(super) fn col_heading(col: Col, count: String) -> Div {
    div().flex().items_center().gap(px(8.)).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).child(status_glyph(col, Status::None)).child(col.name()).child(div().font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(count))
}

/// The agent on a ticket, where a person's avatar would go.
pub(super) fn agent_avatar(id: impl Into<ElementId>, provider: &str, name: String) -> Stateful<Div> {
    let name = SharedString::from(name);
    div().id(id).size(px(18.)).flex().flex_none().items_center().justify_center().rounded(px(9.)).bg(FILL_2).child(theme::provider_icon(provider, 11., TEXT_2)).tooltip(move |window, cx| Tooltip::new(name.clone()).build(window, cx))
}

/// One of a few choices, as the Display menu shows them.
pub(super) fn choice(id: impl Into<ElementId>, label: &'static str, on: bool) -> Stateful<Div> {
    ui::chip(id, on).h(px(24.)).px(px(9.)).rounded(px(12.)).text_size(px(12.)).child(label)
}

pub(super) fn quick_button(id: impl Into<ElementId>, lead: Option<&'static str>, label: &'static str) -> Stateful<Div> {
    div().id(id).h(px(20.)).px(px(7.)).flex().flex_none().items_center().gap(px(4.)).rounded(px(6.)).cursor_pointer().bg(FILL_2).hover(|s| s.bg(FILL_3)).text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(TEXT).whitespace_nowrap().children(lead.map(|n| icon(n, 10., TEXT))).child(label)
}

pub(super) fn key_chip(keys: &str, fill: impl Into<Fill>) -> Div {
    div().px(px(4.)).py(px(3.)).rounded(px(4.)).bg(fill).opacity(0.85).text_size(px(10.5)).line_height(px(11.)).font_weight(FontWeight::MEDIUM).child(ui::os_keys(keys))
}

pub(super) fn start_kbd() -> Div {
    key_chip("⌘↵", rgba(0x0000001a))
}

pub(super) fn bar_button(id: impl Into<ElementId>, v: Variant, lead: Option<&str>, label: impl IntoElement) -> Stateful<Div> {
    ui::button(id, v, lead, label).h(px(26.)).px(px(10.)).rounded(px(7.)).text_size(px(12.5)).font_weight(FontWeight::MEDIUM)
}
