use super::tool_detail::{LINE, TEXT_SIZE};
use super::user_bubble::{grow, text_lines, tween};
use chat::{ToolCall, ToolKind, ToolStatus};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Instant;
use theme::*;

const HEAD: f32 = 34.;
/// Where the text after the tile starts: the card's padding, the tile and the gap after it.
const INSET: f32 = 36.;
const PAIR_GAP: f32 = 10.;
const BOTTOM: f32 = 10.;

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

/// Each question asked and what the user answered it, once answered.
pub(crate) fn answered(call: &ToolCall) -> Vec<(String, String)> {
    match (&call.kind, &call.output) {
        (ToolKind::Question(qs), Some(out)) if call.status == ToolStatus::Ok => chat::answers(qs, out).into_iter().map(|(q, a)| (q.question.clone(), a)).collect(),
        _ => Vec::new(),
    }
}

/// How the marker opens onto its answers: whether it shows them, when it last opened or shut, and what toggles it.
pub(crate) struct Open<F> {
    pub(crate) expanded: bool,
    pub(crate) toggled: Option<Instant>,
    pub(crate) on_click: F,
}

/// An AskUserQuestion call in the transcript; the questions themselves are asked in the composer.
/// Once answered it opens onto each question and its answer, its height easing between the two.
pub(crate) fn marker(id: &str, call: &ToolCall, width: f32, open: Open<impl Fn(&ClickEvent, &mut Window, &mut App) + 'static>, window: &Window) -> Div {
    let pairs = answered(call);
    let tile = div().size(px(20.)).flex_none().rounded(px(6.)).bg(ink(0.09)).flex().items_center().justify_center().child(icon("comment", 12., TEXT_2));
    let opens = !pairs.is_empty();
    let head = div()
        .id(SharedString::from(format!("{id}-question")))
        .h(px(HEAD))
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .px(px(8.))
        .child(tile)
        .child(div().flex_none().font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child("Question"))
        .child(div().flex_1().min_w_0().truncate().text_color(Hsla::from(TEXT).opacity(0.9)).child(value(call)))
        .when(opens, |d| {
            let chevron = icon("chevron-down", 12., TEXT_3).when(open.expanded, |i| i.with_transformation(Transformation::rotate(radians(std::f32::consts::PI))));
            d.cursor_pointer().group("question-head").child(chevron.group_hover("question-head", |s| s.text_color(TEXT))).on_click(open.on_click)
        });
    let text_width = width - INSET - 12.;
    let body_height = pairs.iter().map(|(q, a)| (text_lines(q, TEXT_SIZE, text_width, window) + text_lines(a, TEXT_SIZE, text_width, window)) as f32 * LINE).sum::<f32>()
        + PAIR_GAP * pairs.len().saturating_sub(1) as f32
        + BOTTOM;
    let moving = open.toggled.is_some_and(|at| at.elapsed() < tween(body_height));
    let body = (opens && (open.expanded || moving)).then(|| {
        let pair = |(q, a): (String, String)| div().flex().flex_col().child(div().text_color(TEXT_3).child(q)).child(div().text_color(TEXT).child(a));
        div().pl(px(INSET)).pr(px(12.)).pb(px(BOTTOM)).flex().flex_col().gap(px(PAIR_GAP)).line_height(px(LINE)).children(pairs.into_iter().map(pair))
    });
    let card = div().flex().flex_col().overflow_hidden().rounded(px(10.)).border_1().border_color(hairline(0.08)).bg(ink(0.045)).text_size(px(TEXT_SIZE)).child(head).children(body);
    let card = if moving {
        let (from, to) = if open.expanded { (HEAD, HEAD + body_height) } else { (HEAD + body_height, HEAD) };
        // The border sits outside the eased height, so the card ends exactly where its natural height starts.
        div().child(card.with_animation(SharedString::from(format!("{id}-question-{}", open.expanded)), grow(body_height), move |d, t| d.h(px(from + (to - from) * t + 2.))))
    } else {
        card
    };
    div().py(px(4.)).child(card)
}

#[cfg(test)]
mod tests {
    use super::{answered, value};
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

    #[test]
    fn only_an_answered_question_has_answers_to_open_onto() {
        let out = Some(r#"User has answered your questions: "Which  parser?"="Pest". You can now continue."#.to_string());
        assert_eq!(answered(&ToolCall { output: out.clone(), ..ask(ToolStatus::Ok, "Parser") }), [("Which  parser?".to_string(), "Pest".to_string())]);
        assert!(answered(&ToolCall { output: out, ..ask(ToolStatus::Failed, "Parser") }).is_empty());
        assert!(answered(&ask(ToolStatus::Ok, "Parser")).is_empty());
    }
}
