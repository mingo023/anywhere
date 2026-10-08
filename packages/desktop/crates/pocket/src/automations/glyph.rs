use agents::automations::RunStatus;
use gpui_kit::*;
use theme::*;

/// The status as the Runs tab and the pill name it.
pub fn label(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Waiting => "Needs you",
        RunStatus::Running | RunStatus::Pending => "Working",
        RunStatus::Succeeded => "Done",
        RunStatus::Failed => "Failed",
        RunStatus::Skipped => "Skipped",
        RunStatus::Cancelled => "Stopped",
        RunStatus::Unknown => "",
    }
}

/// The colour of the status's mark.
pub fn tone(status: RunStatus) -> Token {
    match status {
        RunStatus::Waiting => WAITING,
        RunStatus::Running | RunStatus::Pending => ACCENT,
        RunStatus::Succeeded => SUCCESS_TEXT,
        RunStatus::Failed => FAILED,
        RunStatus::Skipped | RunStatus::Cancelled | RunStatus::Unknown => TEXT_4,
    }
}

/// The status as a `size`-px mark; `id` keeps each spinner's animation apart.
pub fn glyph(status: RunStatus, size: f32, id: impl Into<ElementId>) -> AnyElement {
    let color = tone(status);
    match status {
        RunStatus::Waiting => {
            let dot = (size * 0.57).round();
            let halo = (size >= 14.).then(|| ui::ring(WAITING_BG, 2.5));
            div().size(px(size)).flex_none().flex().items_center().justify_center().child(ui::dot(dot, color).shadow(halo.into_iter().collect::<Vec<_>>())).into_any_element()
        }
        RunStatus::Running | RunStatus::Pending => dot_spinner(id, size, color).into_any_element(),
        RunStatus::Succeeded => icon("check", size, color).into_any_element(),
        RunStatus::Failed => icon("x-bold", size, color).into_any_element(),
        RunStatus::Skipped | RunStatus::Unknown => icon("run-skipped", size, color).into_any_element(),
        RunStatus::Cancelled => icon("run-cancelled", size, color).into_any_element(),
    }
}

/// The pill under a run's header: the status's mark and name on its own tint.
pub fn status_pill(status: RunStatus, id: impl Into<ElementId>) -> Div {
    let (bg, fg) = match status {
        RunStatus::Waiting => (WAITING_BG, WAITING_TEXT),
        RunStatus::Running | RunStatus::Pending => (ACCENT_TINT, ACCENT),
        RunStatus::Succeeded => (SUCCESS_BG, SUCCESS_TEXT),
        RunStatus::Failed => (FAILED_BG, FAILED),
        RunStatus::Skipped | RunStatus::Cancelled | RunStatus::Unknown => (FILL_3, TEXT_2),
    };
    let mark = match status {
        RunStatus::Waiting => ui::dot(6., WAITING).into_any_element(),
        RunStatus::Skipped | RunStatus::Unknown => icon("minus", 11., fg).into_any_element(),
        RunStatus::Cancelled => icon("stop", 11., fg).into_any_element(),
        _ => glyph(status, 11., id),
    };
    let text = if status == RunStatus::Pending { "Starting" } else { label(status) };
    div().h(px(22.)).pl(px(8.)).pr(px(9.)).flex().flex_none().items_center().gap(px(6.)).rounded(px(11.)).bg(bg).text_color(fg).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).child(mark).child(text)
}
