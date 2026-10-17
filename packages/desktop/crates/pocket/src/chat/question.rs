use super::composer::RADIUS;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use chat::{Ask, Command, Question};
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::Duration;
use theme::*;
use ui::Variant;

/// How long a single-select pick shows before the wizard moves on.
pub(super) const AUTO_ADVANCE: Duration = Duration::from_millis(220);
pub(super) const PLACEHOLDER: &str = "Type your own answer, or pick an option above";
const PLACEHOLDER_PICKED: &str = "Type your own answer, or leave this blank to use the selected option";

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
}

/// Small caps tracked out with hair spaces, as Zeron's eyebrow labels.
fn tracked_upper(text: &str) -> String {
    text.to_uppercase().chars().map(String::from).collect::<Vec<_>>().join("\u{200A}")
}

impl Wizard {
    pub(super) fn new(ask: &Ask) -> Self {
        let n = ask.questions.len();
        Self { request_id: ask.request_id.clone(), questions: ask.questions.clone(), page: 0, picked: vec![Vec::new(); n], typed: vec![String::new(); n] }
    }

    fn counter(&self) -> String {
        format!("{}/{}", self.page + 1, self.questions.len())
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
        if self.page + 1 >= self.questions.len() { "Submit" } else { "Next" }
    }

    pub(super) fn typed(&self) -> &str {
        &self.typed[self.page]
    }

    pub(super) fn select(&mut self, option: usize) -> Step {
        let q = &self.questions[self.page];
        if option >= q.options.len() {
            return Step::Stay;
        }
        let picked = &mut self.picked[self.page];
        if !q.multi {
            *picked = vec![option];
            return Step::AutoAdvance;
        }
        match picked.iter().position(|&p| p == option) {
            Some(at) => _ = picked.remove(at),
            None => picked.push(option),
        }
        Step::Stay
    }

    /// Keys 1–9 pick the first nine options.
    pub(super) fn press_number(&mut self, n: usize) -> Step {
        if !(1..=9).contains(&n) {
            return Step::Stay;
        }
        self.select(n - 1)
    }

    pub(super) fn set_typed(&mut self, text: String) {
        self.typed[self.page] = text;
    }

    pub(super) fn advance(&mut self) -> Step {
        if self.page + 1 < self.questions.len() {
            self.page += 1;
            Step::Stay
        } else {
            Step::Done(self.answers())
        }
    }

    pub(super) fn back(&mut self) {
        self.page = self.page.saturating_sub(1);
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
        Some((agent, c.wizard.as_mut()?))
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
            Step::Done(answers) => {
                let Some((agent, w)) = self.wizard(terminal, window, cx) else { return };
                let request = w.request_id.clone();
                self.chats.composer.wizard = None;
                self.chats.composer.advance = None;
                self.chats.composer.answer.update(cx, |s, cx| s.set_value("", window, cx));
                self.chat_command(&agent, Command::Answer { request, answers }, cx);
            }
            Step::Stay => {}
        }
        cx.notify();
    }

    fn wizard_select(&mut self, terminal: &str, option: usize, number: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some((_, w)) = self.wizard(terminal, window, cx) else { return };
        let step = if number { w.press_number(option) } else { w.select(option) };
        let placeholder = if w.page_has_pick() { PLACEHOLDER_PICKED } else { PLACEHOLDER };
        self.chats.composer.answer.update(cx, |s, cx| s.set_placeholder(placeholder, window, cx));
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

    fn wizard_back(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.chats.composer.answer.read(cx).value().to_string();
        let Some((_, w)) = self.wizard(terminal, window, cx) else { return };
        w.set_typed(typed);
        w.back();
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
        let empty = input.value().is_empty();
        let key = e.keystroke.key.as_str();
        match key.parse::<usize>() {
            Ok(n) if (1..=9).contains(&n) && !e.keystroke.modifiers.modified() => {
                if !focused || empty {
                    self.wizard_select(terminal, n, true, window, cx);
                    cx.stop_propagation();
                }
            }
            _ if key == "enter" && !focused => {
                self.wizard_advance(terminal, window, cx);
                cx.stop_propagation();
            }
            _ if key == "escape" => {
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
        let typed_empty = c.answer.read(cx).value().trim().is_empty();
        let options = q.options.iter().enumerate().map(|(ix, o)| {
            let picked = w.is_picked(ix) && typed_empty;
            let t = terminal.to_string();
            div()
                .id(("chat-question-option", ix))
                .flex()
                .items_center()
                .gap(px(12.))
                .px(px(14.))
                .py(px(10.))
                .rounded(px(12.))
                .border_1()
                .border_color(if picked { ink(0.16) } else { transparent_black() })
                .bg(if picked { ink(0.09) } else { ink(0.025) })
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .text_size(px(13.5))
                        .font_weight(FontWeight::MEDIUM)
                        .text_color(if picked { Hsla::from(TEXT) } else { Hsla::from(TEXT).opacity(0.9) })
                        .child(o.label.clone()),
                )
                .when(ix < 9, |el| {
                    el.child(
                        div()
                            .flex_none()
                            .size(px(22.))
                            .flex()
                            .items_center()
                            .justify_center()
                            .rounded(px(6.))
                            .bg(if picked { ink(0.16) } else { ink(0.05) })
                            .text_size(px(11.))
                            .text_color(if picked { Hsla::from(TEXT) } else { Hsla::from(TEXT_2).opacity(0.6) })
                            .child((ix + 1).to_string()),
                    )
                })
                .when(answerable, |el| {
                    el.cursor_pointer()
                        .hover(|s| if picked { s } else { s.bg(ink(0.06)) })
                        .active(|s| s.bg(ink(0.09)))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.wizard_select(&t, ix, false, window, cx)))
                })
        });
        let header = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .text_size(px(10.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(Hsla::from(TEXT_2).opacity(0.6))
            .child(tracked_upper(&q.header))
            .when(w.questions.len() > 1, |el| el.child(div().h(px(20.)).px(px(6.)).flex().items_center().rounded(px(6.)).bg(ink(0.06)).text_size(px(10.)).child(w.counter())));
        let body = div()
            .px(px(16.))
            .pt(px(16.))
            .pb(px(if answerable { 0. } else { 16. }))
            .flex()
            .flex_col()
            .child(header)
            .child(div().mt(px(6.)).text_size(px(15.)).line_height(px(20.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(q.question.clone()))
            .when(q.multi, |el| el.child(div().mt(px(4.)).text_size(px(12.)).text_color(Hsla::from(TEXT_2).opacity(0.65)).child("Select one or more options.")))
            .child(div().mt(px(12.)).flex().flex_col().gap(px(4.)).children(options))
            .when(answerable, |el| {
                // The textarea pads itself 8 × 10; this frames it to Zeron's pt 12, px 4.
                el.child(div().mt(px(12.)).border_t_1().border_color(hairline(0.06)).pt(px(4.)).pb(px(4.)).child(div().mx(px(-6.)).text_size(px(14.)).child(Textarea::new(&c.answer).appearance(false))))
            });
        let (back, next, keys) = (terminal.to_string(), terminal.to_string(), terminal.to_string());
        let footer = answerable.then(|| {
            div()
                .flex()
                .justify_between()
                .items_center()
                .px(px(16.))
                .pb(px(16.))
                .pt(px(4.))
                .child(if w.page > 0 {
                    ui::button(id(format!("chat-question-back:{terminal}")), Variant::Ghost, None, "Back")
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.wizard_back(&back, window, cx)))
                        .into_any_element()
                } else {
                    div().into_any_element()
                })
                .child(
                    ui::button(id(format!("chat-question-next:{terminal}")), Variant::Primary, None, w.primary())
                        .px(px(16.))
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
            .rounded(px(RADIUS))
            .border_1()
            .border_color(hairline(0.1))
            .bg(SURFACE)
            .shadow(vec![ui::shadow(SHADOW, 8., 24.)])
            .child(body)
            .children(footer)
    }
}

#[cfg(test)]
mod tests {
    use super::{AUTO_ADVANCE, Step, Wizard, tracked_upper};
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
    fn a_single_select_pick_advances_after_220ms() {
        assert_eq!(AUTO_ADVANCE, Duration::from_millis(220));
        let mut w = wizard(vec![question("Which?", 2, false), question("Many?", 2, true)]);
        assert_eq!(w.select(1), Step::AutoAdvance);
        w.advance();
        assert_eq!(w.select(0), Step::Stay, "a multi-select waits for Next");
    }

    #[test]
    fn the_last_page_offers_submit() {
        let mut w = wizard(vec![question("A?", 2, false), question("B?", 2, false)]);
        assert_eq!(w.primary(), "Next");
        assert_eq!(w.advance(), Step::Stay);
        assert_eq!(w.primary(), "Submit");
        assert!(matches!(w.advance(), Step::Done(_)));
    }

    #[test]
    fn number_keys_pick_the_first_nine_options() {
        let mut w = wizard(vec![question("Which?", 10, true)]);
        w.press_number(1);
        w.press_number(9);
        assert_eq!(w.press_number(10), Step::Stay);
        assert_eq!(w.press_number(0), Step::Stay);
        assert!(w.is_picked(0) && w.is_picked(8) && !w.is_picked(9));
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
