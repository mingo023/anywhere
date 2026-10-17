use super::shimmer::shimmer;
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

/// "Sending…", light sweeping across it, until the send lands.
pub(super) fn sending() -> Div {
    div().py(px(4.)).text_size(px(14.)).line_height(px(20.)).child(shimmer("chat-sending", "Sending…", TEXT_3))
}

pub(super) fn not_delivered(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).flex().items_center().gap(px(8.)).py(px(4.)).text_size(px(13.)).text_color(FAILED).cursor_pointer().child("Not delivered — click to retry")
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
