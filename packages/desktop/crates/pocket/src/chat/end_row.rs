use gpui_kit::*;
use theme::*;

/// Whether a failed turn was the user stopping it rather than an error.
pub(super) fn interrupted(error: &str) -> bool {
    error.to_ascii_lowercase().contains("interrupt")
}

/// "Interrupted · What should Claude do instead?"
pub(super) fn interrupted_notice(who: &str) -> Div {
    div().py(px(4.)).text_size(px(14.)).line_height(px(20.)).text_color(TEXT_3).child(format!("Interrupted · What should {who} do instead?"))
}

pub(super) fn error(id: &str, message: String, copied: bool, on_copy: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Div {
    let failed = Hsla::from(FAILED);
    let muted = Hsla::from(FAILED_TEXT).opacity(0.8);
    let tile = div().size(px(20.)).flex_none().flex().items_center().justify_center().rounded(px(6.)).bg(failed.opacity(0.12)).child(icon("warning", 12., muted));
    let copy = div()
        .id(SharedString::from(format!("copy-{id}")))
        .size(px(20.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|d| d.bg(failed.opacity(0.12)))
        .on_click(on_copy)
        .child(icon(if copied { "check" } else { "copy" }, 12., muted));
    let header = div().flex().items_center().gap(px(8.)).child(tile).child(div().font_weight(FontWeight::MEDIUM).text_color(muted).child("Error")).child(div().flex_1()).child(copy);
    let frame = div()
        .w_full()
        .overflow_hidden()
        .flex()
        .flex_col()
        .gap(px(6.))
        .rounded(px(10.))
        .border_1()
        .border_color(failed.opacity(0.16))
        .bg(failed.opacity(0.05))
        .px(px(10.))
        .py(px(8.))
        .text_size(px(12.))
        .child(header)
        .child(div().text_color(Hsla::from(TEXT).opacity(0.8)).child(message));
    div().py(px(4.)).w_full().child(frame)
}

/// A rule either side of "Context compacted".
pub(super) fn compacted(auto: bool) -> Div {
    let rule = || div().flex_1().h(px(1.)).bg(SEPARATOR_STRONG);
    let label = if auto { "Context compacted automatically" } else { "Context compacted" };
    div().py(px(14.)).flex().items_center().gap(px(10.)).child(rule()).child(div().text_size(px(12.)).text_color(Hsla::from(TEXT_2).opacity(0.7)).child(label)).child(rule())
}

#[cfg(test)]
mod tests {
    use super::interrupted;

    #[test]
    fn a_turn_the_user_stopped_reads_as_interrupted_not_failed() {
        assert!(interrupted("Interrupted by user"));
        assert!(interrupted("[Request interrupted by user]"));
        assert!(!interrupted("API Error: 529 overloaded"));
    }
}
