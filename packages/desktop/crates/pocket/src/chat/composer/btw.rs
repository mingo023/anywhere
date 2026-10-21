use super::queue::{control, surface};
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

/// A side question and, once it came, its answer or why there's none.
pub(crate) struct Btw {
    request: String,
    question: String,
    answer: Option<Result<String, String>>,
}

impl Desktop {
    pub(super) fn chat_btw_ask(&mut self, agent: &str, question: &str) {
        let Some(request) = self.outbox.btw(agent, question) else { return };
        self.chats.composer.btw.insert(agent.to_string(), Btw { request, question: question.to_string(), answer: None });
    }

    /// The answer to side question `request`, if its card still shows; a newer question replaced an older one's.
    pub(crate) fn chat_btw_answered(&mut self, request: &str, answer: Result<String, String>, cx: &mut Context<Self>) {
        if let Some(b) = self.chats.composer.btw.values_mut().find(|b| b.request == request) {
            b.answer = Some(answer);
            cx.notify();
        }
    }

    /// The side question over the box with its answer, until closed.
    pub(super) fn btw_card(&self, agent: &str, cx: &mut Context<Self>) -> Option<Div> {
        let b = self.chats.composer.btw.get(agent)?;
        let close = {
            let agent = agent.to_string();
            control(format!("chat-btw-close:{agent}"), "Close").child(icon("x", 13., TEXT_3)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.chats.composer.btw.remove(&agent);
                cx.notify();
            }))
        };
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .child(div().flex_none().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("/btw"))
            .child(div().flex_1().min_w_0().truncate().text_color(TEXT_3).child(b.question.clone()))
            .child(close);
        let (text, color) = match &b.answer {
            None => ("Thinking…".to_string(), TEXT_4),
            Some(Ok(answer)) => (answer.clone(), TEXT_BODY),
            Some(Err(why)) => (why.clone(), TEXT_3),
        };
        let body = div().id(SharedString::from(format!("chat-btw:{agent}"))).mt(px(6.)).max_h(px(220.)).overflow_y_scroll().text_size(px(13.)).line_height(px(19.)).text_color(color).child(text);
        Some(surface().child(head).child(body))
    }
}
