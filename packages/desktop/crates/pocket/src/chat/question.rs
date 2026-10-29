use super::composer::RADIUS;
use super::markdown;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use chat::{Ask, Command, Question};
use gpui_kit::base::text::TextView;
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;
use theme::*;
use ui::Variant;

/// How long a single-select pick shows before the wizard moves on.
pub(super) const AUTO_ADVANCE: Duration = Duration::from_millis(220);
pub(super) const PLACEHOLDER: &str = "Something else…";
const PLACEHOLDER_PICKED: &str = "Something else, or leave blank to use your pick";
/// Past this the question's body scrolls, so the options stay in view.
const BODY_MAX_HEIGHT: f32 = 184.;

#[derive(Debug, PartialEq)]
pub(super) enum Step {
    Stay,
    AutoAdvance,
    Done(HashMap<String, String>),
}

/// An ask's questions, a page each: single-select advances on its own, number keys pick.
pub(crate) struct Wizard {
    pub(super) request_id: String,
    questions: Vec<Question>,
    pub(super) page: usize,
    picked: Vec<Vec<usize>>,
    typed: Vec<String>,
    /// Whether the last page change went forward, so the new page slides in from that side.
    forward: bool,
    /// Set once the answers or the skip went out; the ask stays pending until the next poll, and mustn't send twice.
    sent: bool,
}

/// Small caps tracked out with hair spaces, as an eyebrow label.
fn tracked_upper(text: &str) -> String {
    text.to_uppercase().chars().map(String::from).collect::<Vec<_>>().join("\u{200A}")
}

/// A question's first line as its title, and the rest, if any, as markdown under it.
fn title_and_body(question: &str) -> (&str, Option<&str>) {
    let (title, body) = question.trim().split_once('\n').unwrap_or((question.trim(), ""));
    let body = body.trim();
    (title.trim(), (!body.is_empty()).then_some(body))
}

impl Wizard {
    pub(super) fn new(ask: &Ask) -> Self {
        let n = ask.questions.len();
        Self { request_id: ask.request_id.clone(), questions: ask.questions.clone(), page: 0, picked: vec![Vec::new(); n], typed: vec![String::new(); n], forward: true, sent: false }
    }

    fn last(&self) -> bool {
        self.page + 1 >= self.questions.len()
    }

    fn is_picked(&self, option: usize) -> bool {
        self.picked[self.page].contains(&option)
    }

    pub(super) fn page_has_pick(&self) -> bool {
        !self.picked[self.page].is_empty()
    }

    pub(super) fn can_advance(&self, typed_empty: bool) -> bool {
        self.page_has_pick() || !typed_empty
    }

    pub(super) fn primary(&self) -> &'static str {
        if self.last() { "Submit" } else { "Next" }
    }

    pub(super) fn typed(&self) -> &str {
        &self.typed[self.page]
    }

    /// A pick over typed words clears them and the picks they hid, since typed words win.
    /// The last page's pick waits for Submit, so an answer is never sent by a stray key.
    pub(super) fn select(&mut self, option: usize) -> Step {
        let q = &self.questions[self.page];
        if option >= q.options.len() {
            return Step::Stay;
        }
        if !self.typed[self.page].trim().is_empty() {
            self.typed[self.page].clear();
            self.picked[self.page].clear();
        }
        let picked = &mut self.picked[self.page];
        if !q.multi {
            *picked = vec![option];
            return if self.last() { Step::Stay } else { Step::AutoAdvance };
        }
        match picked.iter().position(|&p| p == option) {
            Some(at) => _ = picked.remove(at),
            None => picked.push(option),
        }
        Step::Stay
    }

    /// The option key `n` picks: 1–9 the first nine, of those the page has.
    pub(super) fn key_option(&self, n: usize) -> Option<usize> {
        (1..=9).contains(&n).then(|| n - 1).filter(|&ix| ix < self.questions[self.page].options.len())
    }

    pub(super) fn set_typed(&mut self, text: String) {
        self.typed[self.page] = text;
    }

    pub(super) fn advance(&mut self) -> Step {
        if self.last() {
            return Step::Done(self.answers());
        }
        self.page += 1;
        self.forward = true;
        Step::Stay
    }

    /// Steps back a page; false on the first, which has none to go back to.
    pub(super) fn back(&mut self) -> bool {
        if self.page == 0 {
            return false;
        }
        self.page -= 1;
        self.forward = false;
        true
    }

    /// Each answer by its question's text; typed words win over picks, and several picks join with ", ".
    fn answers(&self) -> HashMap<String, String> {
        let answer = |(ix, q): (usize, &Question)| {
            let typed = self.typed[ix].trim();
            let text = if typed.is_empty() { self.picked[ix].iter().map(|&p| q.options[p].label.as_str()).collect::<Vec<_>>().join(", ") } else { typed.to_string() };
            (q.question.clone(), text)
        };
        self.questions.iter().enumerate().map(answer).collect()
    }
}

impl Desktop {
    /// The wizard for `terminal`'s open question, started afresh when the ask is new.
    fn wizard(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) -> Option<(String, &mut Wizard)> {
        let (ask, _) = self.chat_ask(terminal)?;
        let agent = self.summary(terminal)?.id.clone();
        let c = &mut self.chats.composer;
        if c.wizard.as_ref().is_none_or(|w| w.request_id != ask.request_id) {
            c.wizard = Some(Wizard::new(&ask));
            c.advance = None;
            c.answer.update(cx, |s, cx| {
                s.set_value("", window, cx);
                s.set_placeholder(PLACEHOLDER, window, cx);
            });
        }
        c.wizard.as_mut().filter(|w| !w.sent).map(|w| (agent, w))
    }

    /// Ends the wizard with `answer`: the answers, or a deny that skips the ask and stops the agent's turn.
    fn wizard_close(&mut self, terminal: &str, answer: impl FnOnce(String) -> Command, window: &mut Window, cx: &mut Context<Self>) {
        let Some((agent, w)) = self.wizard(terminal, window, cx) else { return };
        let command = answer(w.request_id.clone());
        w.sent = true;
        let c = &mut self.chats.composer;
        c.advance = None;
        c.answer.update(cx, |s, cx| s.set_value("", window, cx));
        self.chat_command(&agent, command, cx);
    }

    fn wizard_step(&mut self, terminal: &str, step: Step, window: &mut Window, cx: &mut Context<Self>) {
        match step {
            Step::AutoAdvance => {
                let t = terminal.to_string();
                self.chats.composer.advance = Some(cx.spawn_in(window, async move |this, cx| {
                    cx.background_executor().timer(AUTO_ADVANCE).await;
                    this.update_in(cx, |d, window, cx| d.wizard_advance(&t, window, cx)).ok();
                }));
            }
            Step::Done(answers) => self.wizard_close(terminal, |request| Command::Answer { request, answers }, window, cx),
            Step::Stay => {}
        }
        cx.notify();
    }

    fn wizard_select(&mut self, terminal: &str, option: usize, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.chats.composer.answer.read(cx).value().to_string();
        let Some((_, w)) = self.wizard(terminal, window, cx) else { return };
        w.set_typed(typed.clone());
        let step = w.select(option);
        let cleared = w.typed() != typed;
        let placeholder = if w.page_has_pick() { PLACEHOLDER_PICKED } else { PLACEHOLDER };
        self.chats.composer.answer.update(cx, |s, cx| {
            if cleared {
                s.set_value("", window, cx);
            }
            s.set_placeholder(placeholder, window, cx);
        });
        self.wizard_step(terminal, step, window, cx);
    }

    pub(super) fn wizard_advance(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.chats.composer.answer.read(cx).value().to_string();
        let Some((_, w)) = self.wizard(terminal, window, cx) else { return };
        if !w.can_advance(typed.trim().is_empty()) {
            return;
        }
        w.set_typed(typed);
        let step = w.advance();
        let next = w.typed().to_string();
        let placeholder = if w.page_has_pick() { PLACEHOLDER_PICKED } else { PLACEHOLDER };
        self.chats.composer.advance = None;
        if step == Step::Stay {
            self.chats.composer.answer.update(cx, |s, cx| {
                s.set_value(next, window, cx);
                s.set_placeholder(placeholder, window, cx);
            });
        }
        self.wizard_step(terminal, step, window, cx);
    }

    /// Back a page, or from the first, skips the ask.
    fn wizard_back(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.chats.composer.answer.read(cx).value().to_string();
        let Some((_, w)) = self.wizard(terminal, window, cx) else { return };
        w.set_typed(typed);
        if !w.back() {
            return self.wizard_close(terminal, Command::Deny, window, cx);
        }
        let prev = w.typed().to_string();
        let placeholder = if w.page_has_pick() { PLACEHOLDER_PICKED } else { PLACEHOLDER };
        self.chats.composer.advance = None;
        self.chats.composer.answer.update(cx, |s, cx| {
            s.set_value(prev, window, cx);
            s.set_placeholder(placeholder, window, cx);
        });
        cx.notify();
    }

    /// Digits pick unless they're typed into the answer, Enter advances from outside it, Esc goes back unless it clears.
    fn wizard_key(&mut self, terminal: &str, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let input = self.chats.composer.answer.read(cx);
        let focused = input.focus_handle(cx).is_focused(window);
        let empty = input.value().trim().is_empty();
        let key = e.keystroke.key.as_str();
        let n = key.parse::<usize>().ok().filter(|_| !e.keystroke.modifiers.modified() && (!focused || empty));
        if let Some(option) = n.and_then(|n| self.wizard(terminal, window, cx)?.1.key_option(n)) {
            self.wizard_select(terminal, option, window, cx);
            return cx.stop_propagation();
        }
        match key {
            "enter" if !focused => {
                self.wizard_advance(terminal, window, cx);
                cx.stop_propagation();
            }
            "escape" => {
                if !focused || empty {
                    self.wizard_back(terminal, window, cx);
                }
                cx.stop_propagation();
            }
            _ => {}
        }
    }

    /// The questions panel in the pill's place; read-only when this client can't answer.
    pub(super) fn question_panel(&self, terminal: &str, ask: &Ask, answerable: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let c = &self.chats.composer;
        let fresh;
        let w = match &c.wizard {
            Some(w) if w.request_id == ask.request_id => w,
            _ => {
                fresh = Wizard::new(ask);
                &fresh
            }
        };
        let q = &w.questions[w.page];
        let (title, body) = title_and_body(&q.question);
        let typed_empty = c.answer.read(cx).value().trim().is_empty();
        let options = q.options.iter().enumerate().map(|(ix, o)| {
            let picked = w.is_picked(ix) && typed_empty;
            let t = terminal.to_string();
            let mark = div()
                .flex_none()
                .size(px(16.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(if q.multi { 5. } else { 8. }))
                .when(picked, |d| d.bg(TEXT))
                .when(!picked, |d| d.border_1().border_color(TEXT_5))
                .when(picked && q.multi, |d| d.child(icon("check", 11., ON_TEXT)))
                .when(picked && !q.multi, |d| d.child(div().size(px(6.)).rounded(px(3.)).bg(ON_TEXT)));
            let text = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.5)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(o.label.clone()))
                .when(!o.description.is_empty(), |d| d.child(div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(o.description.clone())));
            div()
                .id(("chat-question-option", ix))
                .flex()
                .items_center()
                .gap(px(12.))
                .pl(px(12.))
                .pr(px(10.))
                .py(px(9.))
                .rounded(px(11.))
                .border_1()
                .border_color(if picked { Hsla::from(SEPARATOR_STRONG) } else { transparent_black() })
                .bg(if picked { FILL_3 } else { FILL_1 })
                .when(!typed_empty, |el| el.opacity(0.55))
                .child(mark)
                .child(text)
                .when(ix < 9, |el| {
                    el.child(
                        div()
                            .flex_none()
                            .size(px(20.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .bg(if picked { FILL_4 } else { FILL_2 })
                            .text_size(px(11.))
                            .text_color(if picked { TEXT } else { TEXT_3 })
                            .child((ix + 1).to_string()),
                    )
                })
                .when(answerable, |el| {
                    el.cursor_pointer()
                        .hover(|s| if picked { s } else { s.bg(FILL_2) })
                        .active(|s| s.bg(FILL_3))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.wizard_select(&t, ix, window, cx)))
                })
        });
        let n = w.questions.len();
        let progress = (n > 1).then(|| {
            div()
                .flex()
                .items_center()
                .gap(px(3.))
                .children((0..n).map(|i| div().w(px(14.)).h(px(3.)).rounded(px(2.)).bg(if i <= w.page { TEXT_2 } else { FILL_3 })))
                .child(div().ml(px(6.)).text_size(px(11.)).text_color(TEXT_4).child(format!("{} of {n}", w.page + 1)))
        });
        let head = div()
            .h(px(16.))
            .flex()
            .items_center()
            .child(div().flex_1().text_size(px(10.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(tracked_upper(&q.header)))
            .children(progress);
        let body = body.map(|text| {
            let open: markdown::Open = {
                let (this, (agent, cwd)) = (cx.weak_entity(), self.summary(terminal).map(|a| (a.id.clone(), a.cwd.clone())).unwrap_or_default());
                Arc::new(move |link, window, cx| this.update(cx, |d, cx| d.chat_open(&agent, link, &cwd, window, cx)).unwrap_or_default())
            };
            div()
                .id(id(format!("chat-question-body:{terminal}")))
                .mt(px(6.))
                .max_h(px(BODY_MAX_HEIGHT))
                .overflow_y_scroll()
                .child(markdown::block(TextView::markdown(id(format!("chat-question-md:{}:{}", ask.request_id, w.page)), SharedString::from(text.to_string())), open).text_size(px(13.5)).line_height(px(21.)))
        });
        let other = answerable.then(|| {
            // The textarea pads itself 8 × 10, so the row adds no vertical padding and the pencil drops to its first line.
            div()
                .mt(px(4.))
                .flex()
                .items_start()
                .gap(px(2.))
                .pl(px(12.))
                .rounded(px(11.))
                .border_1()
                .border_color(hairline(0.08))
                .child(div().flex_none().pt(px(11.)).child(icon("pencil", 14., TEXT_4)))
                .child(div().flex_1().min_w_0().text_size(px(13.5)).child(Textarea::new(&c.answer).appearance(false)))
        });
        let slide = if w.forward { 14. } else { -14. };
        let page = div()
            .flex()
            .flex_col()
            .child(head)
            .child(div().mt(px(8.)).text_size(px(15.)).line_height(px(21.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title.to_string()))
            .children(body)
            .when(q.multi, |el| el.child(div().mt(px(4.)).text_size(px(12.)).text_color(TEXT_3).child("Select one or more.")))
            .child(div().mt(px(12.)).flex().flex_col().gap(px(4.)).children(options))
            .with_animation(id(format!("chat-question-page:{}:{}", ask.request_id, w.page)), Animation::new(Duration::from_millis(220)).with_easing(ease_out_quint()), move |e, t| {
                e.opacity(t).ml(px(slide * (1. - t))).mr(px(-slide * (1. - t)))
            });
        let (skip, back, next, keys) = (terminal.to_string(), terminal.to_string(), terminal.to_string(), terminal.to_string());
        let footer = answerable.then(|| {
            div()
                .mt(px(12.))
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    ui::button(id(format!("chat-question-skip:{terminal}")), Variant::Ghost, None, "Skip")
                        .h(px(30.))
                        .px(px(10.))
                        .when(w.page == 0, |el| el.child(ui::button_kbd("esc")))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.wizard_close(&skip, Command::Deny, window, cx))),
                )
                .child(div().flex_1())
                .when(w.page > 0, |el| {
                    el.child(
                        ui::button(id(format!("chat-question-back:{terminal}")), Variant::Ghost, None, "Back")
                            .h(px(30.))
                            .px(px(10.))
                            .child(ui::button_kbd("esc"))
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.wizard_back(&back, window, cx))),
                    )
                })
                .child(
                    ui::button(id(format!("chat-question-next:{terminal}")), Variant::Primary, None, w.primary())
                        .h(px(30.))
                        .px(px(14.))
                        .child(ui::button_kbd("⏎"))
                        .when(!w.can_advance(typed_empty), |el| el.opacity(0.35).cursor_default().active(|s| s))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.wizard_advance(&next, window, cx))),
                )
        });
        div()
            .id(id(format!("chat-question:{terminal}")))
            .track_focus(&c.ask_focus)
            .when(answerable, |el| el.on_key_down(cx.listener(move |this, e: &KeyDownEvent, window, cx| this.wizard_key(&keys, e, window, cx))))
            .flex()
            .flex_col()
            .pt(px(14.))
            .px(px(14.))
            .pb(px(12.))
            .rounded(px(RADIUS))
            .border_1()
            .border_color(hairline(0.1))
            .bg(SURFACE)
            .shadow(vec![ui::shadow(SHADOW, 8., 24.)])
            .overflow_hidden()
            .child(page)
            .children(other)
            .children(footer)
    }
}

#[cfg(test)]
mod tests {
    use super::{AUTO_ADVANCE, Step, Wizard, title_and_body, tracked_upper};
    use chat::{Ask, Choice, Question, ToolKind};
    use std::time::Duration;

    fn question(text: &str, options: usize, multi: bool) -> Question {
        let options = (1..=options).map(|i| Choice { label: format!("o{i}"), description: String::new() }).collect();
        Question { header: "Pick".into(), question: text.into(), options, multi }
    }

    fn wizard(questions: Vec<Question>) -> Wizard {
        Wizard::new(&Ask { request_id: "r".into(), tool: ToolKind::Question(questions.clone()), tool_name: "AskUserQuestion".into(), questions })
    }

    #[test]
    fn eyebrow_labels_are_capitals_spaced_by_hair_spaces() {
        assert_eq!(tracked_upper("Db"), "D\u{200A}B");
    }

    #[test]
    fn a_question_titles_with_its_first_line_and_shows_the_rest_below() {
        assert_eq!(title_and_body("Which?"), ("Which?", None));
        assert_eq!(title_and_body("Ship this plan?\n\n- one\n- two\n"), ("Ship this plan?", Some("- one\n- two")));
    }

    #[test]
    fn a_single_select_pick_advances_after_220ms() {
        assert_eq!(AUTO_ADVANCE, Duration::from_millis(220));
        let mut w = wizard(vec![question("Which?", 2, false), question("Many?", 2, true)]);
        assert_eq!(w.select(1), Step::AutoAdvance);
        w.advance();
        assert_eq!(w.select(0), Step::Stay, "a multi-select waits for Next");
    }

    #[test]
    fn the_last_page_offers_submit_and_waits_for_it() {
        let mut w = wizard(vec![question("A?", 2, false), question("B?", 2, false)]);
        assert_eq!(w.primary(), "Next");
        assert_eq!(w.advance(), Step::Stay);
        assert_eq!(w.primary(), "Submit");
        assert_eq!(w.select(0), Step::Stay, "a last-page pick doesn't send on its own");
        assert!(matches!(w.advance(), Step::Done(_)));
    }

    #[test]
    fn going_back_from_the_first_page_has_nowhere_to_go() {
        let mut w = wizard(vec![question("A?", 2, false), question("B?", 2, false)]);
        assert!(!w.back(), "the first page skips instead");
        w.advance();
        assert!(w.forward);
        assert!(w.back());
        assert!(!w.forward, "the page slides in from the side it came from");
        assert_eq!(w.page, 0);
    }

    #[test]
    fn number_keys_pick_the_first_nine_options() {
        let w = wizard(vec![question("Which?", 10, true)]);
        assert_eq!(w.key_option(1), Some(0));
        assert_eq!(w.key_option(9), Some(8));
        assert_eq!(w.key_option(10), None);
        assert_eq!(w.key_option(0), None);
        let w = wizard(vec![question("Which?", 3, true)]);
        assert_eq!(w.key_option(5), None, "a key past the options is left to type");
    }

    #[test]
    fn a_pick_over_typed_words_clears_them_and_the_picks_they_hid() {
        let mut w = wizard(vec![question("Which?", 3, true)]);
        w.select(0);
        w.set_typed("foo".into());
        w.select(0);
        assert_eq!(w.typed(), "");
        assert!(w.is_picked(0), "the pick lands rather than toggling off");
        w.set_typed("bar".into());
        w.select(1);
        assert!(w.is_picked(1) && !w.is_picked(0), "a pick the words hid doesn't come back");
    }

    #[test]
    fn answers_are_keyed_by_question_text() {
        let mut w = wizard(vec![question("Which files?", 3, true), question("Why?", 2, false)]);
        w.select(0);
        w.select(2);
        w.advance();
        w.select(0);
        w.set_typed("  because  ".into());
        let Step::Done(answers) = w.advance() else { panic!("the last page submits") };
        assert_eq!(answers["Which files?"], "o1, o3");
        assert_eq!(answers["Why?"], "because", "typed words win over a pick");
    }
}
