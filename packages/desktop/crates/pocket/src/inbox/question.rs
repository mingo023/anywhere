use super::card::{Btn, btn, in_session, reply_field};
use super::{Inbox, Replying};
use crate::chat::markdown;
use crate::chat::question::title_and_body;
use crate::desktop::chrome::id;
use agents::{Permission, Question};
use gpui_kit::base::text::TextView;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::sync::Arc;
use theme::*;

/// Past this the question and its options scroll, so the buttons stay put.
const SCROLLS_PAST: f32 = 320.;

/// An AskUserQuestion's pages, as the notch answers them: a single-select pick answers and moves on, even on the last page.
pub(crate) struct Wizard {
    page: usize,
    picked: Vec<Vec<usize>>,
    typed: Vec<String>,
}

impl Wizard {
    pub(super) fn new(questions: usize) -> Self {
        Self { page: 0, picked: vec![Vec::new(); questions], typed: vec![String::new(); questions] }
    }

    pub(super) fn page(&self) -> usize {
        self.page
    }

    fn is_picked(&self, option: usize) -> bool {
        self.picked[self.page].contains(&option)
    }

    fn has_pick(&self) -> bool {
        !self.picked[self.page].is_empty()
    }

    /// Picks `option` on the page, or lets a multi-select pick go; true when the pick moves on by itself.
    pub(super) fn pick(&mut self, questions: &[Question], option: usize) -> bool {
        let Some(q) = questions.get(self.page).filter(|q| option < q.options.len()) else { return false };
        self.typed[self.page].clear();
        let picked = &mut self.picked[self.page];
        if !q.multi_select {
            *picked = vec![option];
            return true;
        }
        match picked.iter().position(|&p| p == option) {
            Some(at) => _ = picked.remove(at),
            None => picked.push(option),
        }
        false
    }

    pub(super) fn still_picked(&self, page: usize, option: usize) -> bool {
        self.page == page && self.picked[page] == [option]
    }

    pub(super) fn type_answer(&mut self, text: String) {
        self.typed[self.page] = text;
    }

    pub(super) fn next(&mut self, questions: &[Question]) -> Option<(HashMap<String, String>, String)> {
        if self.page + 1 < questions.len() {
            self.page += 1;
            return None;
        }
        let answers = self.answers(questions);
        let line = match questions {
            [q] => format!("Answered “{}”", answers.get(&q.question).map(String::as_str).unwrap_or_default()),
            _ => format!("Sent {} answers", questions.len()),
        };
        Some((answers, line))
    }

    pub(super) fn back(&mut self) {
        self.page = self.page.saturating_sub(1);
    }

    /// Each answer by its question's text: typed words, else the picks in the options' order joined with ", ", as the notch sends them.
    fn answers(&self, questions: &[Question]) -> HashMap<String, String> {
        let answer = |(i, q): (usize, &Question)| {
            let typed = self.typed[i].trim();
            let text = if typed.is_empty() {
                q.options.iter().enumerate().filter(|(o, _)| self.picked[i].contains(o)).map(|(_, c)| c.label.as_str()).collect::<Vec<_>>().join(", ")
            } else {
                typed.to_string()
            };
            (q.question.clone(), text)
        };
        questions.iter().enumerate().map(answer).collect()
    }
}

fn glyph(multi: bool, on: bool) -> Div {
    let mark = div().size(px(13.)).flex().items_center().justify_center().rounded(px(if multi { 3.5 } else { 6.5 }));
    let mark = match (multi, on) {
        (true, true) => mark.bg(TEXT).child(icon("check", 10., ON_TEXT)),
        (false, true) => mark.border(px(1.5)).border_color(TEXT).child(div().size(px(6.5)).rounded(px(3.25)).bg(TEXT)),
        _ => mark.border(px(1.5)).border_color(TEXT_3),
    };
    div().h(px(18.)).flex().flex_none().items_center().child(mark)
}

impl Inbox {
    pub(super) fn question_box(&self, agent: &str, p: &Permission, cx: &mut Context<Self>) -> Div {
        let questions = p.questions();
        let w = self.answers.wizard(&p.request_id);
        let page = w.map_or(0, Wizard::page).min(questions.len().saturating_sub(1));
        let Some(q) = questions.get(page) else { return div() };
        let n = questions.len();
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().flex_1().min_w_0().truncate().text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(q.header.clone()))
            .when(n > 1, |d| {
                d.child(div().flex_none().text_size(px(11.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_3).child(format!("Question {} of {n}", page + 1)))
            });
        let (title, body) = title_and_body(&q.question);
        let title = div().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title.to_string());
        let body = body.map(|text| {
            let open: markdown::Open = Arc::new(|link, _, _, cx| cx.open_url(link));
            let view = TextView::markdown(id(format!("inbox-question-md:{}:{page}", p.request_id)), SharedString::from(text.to_string()));
            div().mt(px(4.)).child(markdown::block(view, open).text_size(px(12.5)).line_height(px(18.)).text_color(TEXT_2))
        });
        let request = p.request_id.clone();
        if self.observe_only {
            return div().flex().flex_col().gap(px(6.)).child(head).child(title).children(body).child(in_session(agent, "Open session", cx));
        }
        if self.replying == Some(Replying::Other(request.clone())) {
            let back =
                btn(id(format!("inbox-q-cancel:{request}")), Btn::Ghost, "Back").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.cancel_reply(cx)));
            return div().flex().flex_col().gap(px(6.)).child(head).child(title).child(reply_field(&self.reply, cx)).child(div().flex().child(back));
        }
        let options = q.options.iter().enumerate().map(|(ix, o)| {
            let on = w.is_some_and(|w| w.is_picked(ix));
            let request = request.clone();
            div()
                .id(id(format!("inbox-q-opt:{}:{ix}", p.request_id)))
                .flex()
                .items_start()
                .gap(px(9.))
                .px(px(10.))
                .py(px(7.))
                .rounded(px(8.))
                .cursor_pointer()
                .bg(if on { FILL_4 } else { FILL_2 })
                .hover(|s| s.bg(FILL_4))
                .active(|s| s.opacity(0.85))
                .child(glyph(q.multi_select, on).text_color(if on { TEXT } else { TEXT_3 }))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(div().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(o.label.clone()))
                        .when(!o.description.is_empty(), |d| {
                            d.child(div().text_size(px(11.5)).line_height(px(16.)).text_color(TEXT_2).child(o.description.clone()))
                        }),
                )
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick(&request, ix, cx)))
        });
        let scroll = div()
            .id(id(format!("inbox-q-scroll:{}", p.request_id)))
            .max_h(px(SCROLLS_PAST))
            .overflow_y_scroll()
            .child(title)
            .children(body)
            .child(div().mt(px(10.)).flex().flex_col().gap(px(4.)).children(options));
        let other = p.feedback.then(|| {
            let request = request.clone();
            btn(id(format!("inbox-q-other:{request}")), Btn::Ghost, "Other…").on_click(
                cx.listener(move |this, _: &ClickEvent, window, cx| this.start_reply(Replying::Other(request.clone()), "Your answer".into(), window, cx)),
            )
        });
        let back = (page > 0).then(|| {
            let request = request.clone();
            btn(id(format!("inbox-q-back:{request}")), Btn::Secondary, "Back").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.back(&request, cx)))
        });
        let next = q.multi_select.then(|| {
            let ready = w.is_some_and(Wizard::has_pick);
            let request = request.clone();
            btn(id(format!("inbox-q-next:{request}")), Btn::Primary, if page + 1 < n { "Next" } else { "Submit" })
                .when(!ready, |b| b.opacity(0.35))
                .when(ready, |b| b.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.next_page(&request, cx))))
        });
        div()
            .flex()
            .flex_col()
            .child(head)
            .child(div().mt(px(6.)).child(scroll))
            .child(div().mt(px(10.)).flex().items_center().gap(px(6.)).children(other).child(div().flex_1()).children(back).children(next))
    }
}

#[cfg(test)]
mod tests {
    use super::Wizard;
    use agents::{Choice, Question};

    fn question(text: &str, options: &[&str], multi_select: bool) -> Question {
        let options = options.iter().map(|l| Choice { label: l.to_string(), description: String::new() }).collect();
        Question { question: text.into(), header: "Pick".into(), options, multi_select }
    }

    #[test]
    fn a_single_select_pick_moves_on_by_itself_even_on_the_last_page() {
        let qs = [question("Which?", &["a", "b"], false)];
        let mut w = Wizard::new(1);
        assert!(w.pick(&qs, 1));
        assert_eq!(w.next(&qs).map(|(a, line)| (a["Which?"].clone(), line)), Some(("b".into(), "Answered “b”".into())));
    }

    #[test]
    fn a_multi_select_waits_and_sends_its_picks_in_the_options_order() {
        let qs = [question("Which?", &["a", "b", "c"], true), question("More?", &["x"], false)];
        let mut w = Wizard::new(2);
        assert!(!w.pick(&qs, 2));
        assert!(!w.pick(&qs, 0));
        assert!(!w.pick(&qs, 1));
        assert!(!w.pick(&qs, 1), "a second pick lets it go");
        assert_eq!(w.next(&qs), None);
        w.type_answer("y".into());
        let (answers, line) = w.next(&qs).unwrap();
        assert_eq!((answers["Which?"].as_str(), answers["More?"].as_str(), line.as_str()), ("a, c", "y", "Sent 2 answers"));
    }

    #[test]
    fn a_pick_that_changed_before_the_pause_ended_does_not_move_on() {
        let qs = [question("A?", &["a", "b"], false), question("B?", &["x"], false)];
        let mut w = Wizard::new(2);
        w.pick(&qs, 0);
        w.pick(&qs, 1);
        assert!(!w.still_picked(0, 0));
        assert!(w.still_picked(0, 1));
        w.next(&qs);
        assert!(!w.still_picked(0, 1), "the page moved on already");
    }

    #[test]
    fn back_stops_at_the_first_page_and_an_out_of_range_pick_does_nothing() {
        let qs = [question("A?", &["a"], false)];
        let mut w = Wizard::new(1);
        w.back();
        assert_eq!(w.page(), 0);
        assert!(!w.pick(&qs, 5));
    }
}
