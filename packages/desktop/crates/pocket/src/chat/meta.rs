use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use theme::*;

/// "15:45", in the reader's time zone.
pub(super) fn format_timestamp<Tz: chrono::TimeZone>(ms: i64, tz: &Tz) -> String
where
    Tz::Offset: std::fmt::Display,
{
    chrono::DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(tz).format("%-H:%M").to_string()).unwrap_or_default()
}

/// A round icon button with its tooltip.
pub(super) fn action(id: String, glyph: &'static str, size: f32, icon_size: f32, tip: &'static str, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Stateful<Div> {
    div()
        .id(SharedString::from(id))
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(size * 0.26))
        .cursor_pointer()
        .hover(|d| d.bg(FILL_3))
        .active(|d| d.bg(FILL_4))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
        .on_click(on_click)
        .child(icon(glyph, icon_size, TEXT_4))
}

fn dot() -> Div {
    div().size(px(3.)).mx(px(8.)).rounded(px(2.)).bg(TEXT_5)
}

/// Under an answer: copy it, run its turn again, then how long it took and when it came.
pub(super) fn meta(
    id: &str,
    copied: bool,
    copy: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    retry: Option<impl Fn(&ClickEvent, &mut Window, &mut App) + 'static>,
    worked: Option<String>,
    at: Option<i64>,
) -> Div {
    let stat = |s: String| div().text_size(px(12.5)).child(s);
    let mut stats: Vec<Div> = worked.into_iter().chain(at.map(|at| format_timestamp(at, &chrono::Local))).map(stat).collect();
    let stats = (!stats.is_empty()).then(|| {
        let first = stats.remove(0);
        div().flex().items_center().child(dot()).child(first).children(stats.into_iter().map(|s| div().flex().items_center().child(dot()).child(s)))
    });
    div()
        .pt(px(2.))
        .pb(px(10.))
        .ml(px(-6.))
        .flex()
        .items_center()
        .gap(px(2.))
        .text_color(TEXT_4)
        .child(action(format!("copy-{id}"), if copied { "check" } else { "copy" }, 30., 16., "Copy answer", copy))
        .children(retry.map(|r| action(format!("retry-{id}"), "redo", 30., 16., "Retry this turn", r)))
        .children(stats)
}

#[cfg(test)]
mod tests {
    use super::format_timestamp;
    use chrono::FixedOffset;

    #[test]
    fn timestamps_read_twenty_four_hour_time() {
        let utc = FixedOffset::east_opt(0).unwrap();
        // 2026-07-01 15:45 UTC
        assert_eq!(format_timestamp(1_782_920_700_000, &utc), "15:45");
        assert_eq!(format_timestamp(1_782_920_700_000, &FixedOffset::east_opt(-16 * 3600).unwrap()), "23:45");
    }
}
