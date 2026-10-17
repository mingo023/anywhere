use super::tool_detail::TEXT_SIZE;
use chat::{ToolCall, ToolKind, ToolStatus};
use gpui_kit::*;
use theme::*;

/// What the marker reads: the first question's header once answered, else that it waits.
pub(crate) fn value(call: &ToolCall) -> String {
    let first = match &call.kind {
        ToolKind::Question(qs) => qs.first(),
        _ => None,
    };
    match first {
        _ if call.status == ToolStatus::Running => "Awaiting your answer…".into(),
        Some(q) if !q.header.trim().is_empty() => q.header.split_whitespace().collect::<Vec<_>>().join(" "),
        Some(q) => q.question.split_whitespace().collect::<Vec<_>>().join(" "),
        None => String::new(),
    }
}

/// An AskUserQuestion call in the transcript; the questions themselves are asked in the composer.
pub(crate) fn marker(call: &ToolCall) -> Div {
    let tile = div().size(px(20.)).flex_none().rounded(px(6.)).bg(ink(0.09)).flex().items_center().justify_center().child(icon("comment", 12., TEXT_2));
    div().py(px(4.)).child(
        div()
            .h(px(34.))
            .flex()
            .items_center()
            .gap(px(8.))
            .px(px(8.))
            .rounded(px(10.))
            .border_1()
            .border_color(hairline(0.08))
            .bg(ink(0.045))
            .text_size(px(TEXT_SIZE))
            .child(tile)
            .child(div().flex_none().font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child("Question"))
            .child(div().min_w_0().truncate().text_color(Hsla::from(TEXT).opacity(0.9)).child(value(call))),
    )
}

#[cfg(test)]
mod tests {
    use super::value;
    use chat::{Question, ToolCall, ToolKind, ToolStatus};

    fn ask(status: ToolStatus, header: &str) -> ToolCall {
        let q = Question { header: header.into(), question: "Which  parser?".into(), ..Default::default() };
        ToolCall { id: "t".into(), name: "AskUserQuestion".into(), kind: ToolKind::Question(vec![q]), status, output: None, duration_ms: None, at: 0 }
    }

    #[test]
    fn a_question_waits_until_answered_then_shows_its_header() {
        assert_eq!(value(&ask(ToolStatus::Running, "Parser")), "Awaiting your answer…");
        assert_eq!(value(&ask(ToolStatus::Ok, "Parser")), "Parser");
        assert_eq!(value(&ask(ToolStatus::Ok, " ")), "Which parser?");
    }
}
