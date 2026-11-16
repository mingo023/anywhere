use crate::desktop::Desktop;
use chat::{Child, Delivered, Outcome};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// What pocketd told the chat of the sessions it delegated to, one quiet line each: their logo and title, then how it went. A click opens the session.
pub(crate) fn delivered(id: &str, d: &Delivered, cx: &mut Context<Desktop>) -> Div {
    div().flex().flex_col().items_start().gap(px(1.)).ml(px(-8.)).children(d.children.iter().enumerate().map(|(i, child)| line(&format!("{id}-{i}"), child, d, cx)))
}

fn verb(d: &Delivered) -> (String, Token) {
    match d.outcome {
        Outcome::Done => ("done".into(), TEXT_3),
        Outcome::Failed => ("failed".into(), FAILED_TEXT),
        Outcome::Asks => (format!("asks \u{201c}{}\u{201d}", d.question.split_whitespace().collect::<Vec<_>>().join(" ")), TEXT_2),
    }
}

fn line(key: &str, child: &Child, d: &Delivered, cx: &mut Context<Desktop>) -> Stateful<Div> {
    let (said, colour) = verb(d);
    let open = child.id.clone();
    let title = if child.title.is_empty() { "Session".to_string() } else { child.title.clone() };
    div()
        .id(SharedString::from(format!("delivery-{key}")))
        .group("delivery")
        .max_w(relative(1.))
        .h(px(28.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(8.))
        .text_size(px(14.))
        .text_color(TEXT_3)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(|s| s.bg(FILL_2))
        .active(|s| s.bg(FILL_3))
        .on_click(cx.listener(move |this, _, window, cx| this.delegated_open(&open, window, cx)))
        .child(provider_icon(&child.agent, 13., TEXT_3))
        .child(div().min_w_0().truncate().font_weight(FontWeight::MEDIUM).text_color(TEXT_BODY).child(title))
        .child(div().min_w_0().truncate().text_color(colour).when(d.outcome == Outcome::Asks, |s| s.max_w(px(340.))).child(said))
        .child(div().flex_none().text_color(TEXT_4).invisible().group_hover("delivery", |s| s.visible()).child(icon("chevron-right", 12., TEXT_4)))
}

#[cfg(test)]
mod tests {
    use super::verb;
    use chat::{Delivered, Outcome};

    fn said(outcome: Outcome, question: &str) -> String {
        verb(&Delivered { id: "d".into(), outcome, question: question.into(), children: Vec::new() }).0
    }

    #[test]
    fn a_delivery_says_done_failed_or_what_the_session_asks_on_one_line() {
        assert_eq!([said(Outcome::Done, ""), said(Outcome::Failed, "")], ["done", "failed"]);
        assert_eq!(said(Outcome::Asks, "Which\ntest   runner?"), "asks \u{201c}Which test runner?\u{201d}");
    }
}
