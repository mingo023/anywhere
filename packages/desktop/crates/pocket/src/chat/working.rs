use super::shimmer::shimmer;
use agents::PromptState;
use gpui_kit::*;
use theme::*;

pub(super) fn format_elapsed(secs: i64) -> String {
    let secs = secs.max(0);
    match secs {
        0..60 => format!("{secs}s"),
        60..3_600 => format!("{}m {}s", secs / 60, secs % 60),
        3_600..86_400 => format!("{}h {}m", secs / 3_600, secs % 3_600 / 60),
        _ => format!("{}d {}h", secs / 86_400, secs % 86_400 / 3_600),
    }
}

/// "Sending…", light sweeping across it, until the send lands; what else is known of it reads still.
pub(super) fn sending(state: PromptState) -> Div {
    let row = div().py(px(4.)).text_size(px(14.)).line_height(px(20.));
    match (state, chat::trail_text(state)) {
        (PromptState::Sending, _) | (_, None) => row.child(shimmer("chat-sending", "Sending…", TEXT_3)),
        (_, Some(text)) => row.text_color(TEXT_3).child(text),
    }
}

pub(super) fn compacting() -> Div {
    div().py(px(4.)).text_size(px(14.)).line_height(px(20.)).child(shimmer("chat-compacting", "Compacting…", TEXT_3))
}

pub(super) fn not_delivered(id: impl Into<ElementId>, text: String) -> Stateful<Div> {
    div().id(id).flex().items_center().gap(px(8.)).py(px(4.)).text_size(px(13.)).text_color(FAILED).cursor_pointer().child(text)
}

#[cfg(test)]
mod tests {
    use super::format_elapsed;

    #[test]
    fn elapsed_reads_seconds_then_minutes_then_hours_then_days() {
        assert_eq!(format_elapsed(-5), "0s");
        assert_eq!(format_elapsed(12), "12s");
        assert_eq!(format_elapsed(184), "3m 4s");
        assert_eq!(format_elapsed(3_720), "1h 2m");
        assert_eq!(format_elapsed(97_200), "1d 3h");
    }
}
