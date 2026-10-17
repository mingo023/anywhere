mod attachments;
mod model_pill;
mod queue;
mod send_button;

use super::question::Wizard;
use crate::composer::Draft;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use crate::status::Status;
use crate::util::now_ms;
use agents::{Decision, Summary};
use chat::{Action, Ask, Command};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
use theme::*;

const PLACEHOLDER: &str = "Ask, build, / for commands, @ for files";
const BUSY_PLACEHOLDER: &str = "Queue a follow-up, or ⌘⏎ to steer…";
const MAX_WIDTH: f32 = 744.;
pub(super) const RADIUS: f32 = 16.;
const INPUT_TEXT_SIZE: f32 = 14.5;
const INPUT_LINE_HEIGHT: f32 = 22.;
/// The rows that fit 260px; past them the input scrolls.
pub(super) const MAX_ROWS: usize = (260. / INPUT_LINE_HEIGHT) as usize;

/// What stands under the transcript.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum Mode {
    Prompt,
    ViewOnly,
    /// The agent waits on a permission; pocketd refuses prompts until it's answered.
    Permission { answerable: bool },
    Question { answerable: bool },
}

pub(super) fn composer_mode(observe_only: bool, ask: Option<&Ask>) -> Mode {
    let answerable = !observe_only;
    match ask {
        Some(a) if !a.questions.is_empty() => Mode::Question { answerable },
        Some(_) => Mode::Permission { answerable },
        None if observe_only => Mode::ViewOnly,
        None => Mode::Prompt,
    }
}

/// The footer's hint at what Enter does now.
pub(super) fn hint(mode: Mode, working: bool, empty: bool, can_queue: bool) -> Option<&'static str> {
    match mode {
        Mode::ViewOnly => None,
        Mode::Permission { .. } | Mode::Question { .. } => Some("Answer the request above to continue"),
        Mode::Prompt if !working => None,
        Mode::Prompt if empty || !can_queue => Some("Working · esc to stop"),
        Mode::Prompt => Some("⏎ queue · ⌘⏎ steer now"),
    }
}

/// The prompt input, built here since dropped files and Enter are wired at construction.
pub(crate) fn input(window: &mut Window, cx: &mut Context<Desktop>) -> Entity<TextareaState> {
    let drop = attachments::drop_images(cx.weak_entity());
    cx.new(|cx| TextareaState::new(window, cx).placeholder(PLACEHOLDER).auto_grow(1, MAX_ROWS).submit_on_enter(true).on_drop_paths(drop))
}

/// The chat composer's state besides its input (`ChatState::input`), shared by every chat pane.
pub(crate) struct ComposerState {
    pub(crate) draft: Draft,
    /// The free-text answer of a question, or the reply to a permission.
    pub(super) answer: Entity<TextareaState>,
    pub(super) wizard: Option<Wizard>,
    /// Takes keys while a permission or question stands in the pill's place.
    pub(super) ask_focus: FocusHandle,
    pub(super) advance: Option<Task<()>>,
    /// The permission request a reply is being typed for.
    pub(super) replying: Option<String>,
    /// The input's placeholder, which reads differently while the agent works.
    placeholder: Cell<&'static str>,
}

impl ComposerState {
    pub(crate) fn new(input: &Entity<TextareaState>, window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let answer = cx.new(|cx| TextareaState::new(window, cx).placeholder(super::question::PLACEHOLDER).auto_grow(1, MAX_ROWS).submit_on_enter(true));
        let subs = vec![
            cx.subscribe_in(input, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { shift: false, secondary } => this.chat_enter(*secondary, window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
            cx.subscribe_in(&answer, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { shift: false, .. } => this.chat_answer_enter(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
        ];
        let state = Self {
            draft: Draft::default(),
            answer,
            wizard: None,
            ask_focus: cx.focus_handle(),
            advance: None,
            replying: None,
            placeholder: Cell::new(PLACEHOLDER),
        };
        (state, subs)
    }
}

/// Moves focus to `to` when one of `from` holds it, so focus follows the pill into the ask that replaces it and back.
fn hand_focus(from: Vec<FocusHandle>, to: FocusHandle) -> impl IntoElement {
    canvas(
        move |_, window, cx| {
            if from.iter().any(|f| f.is_focused(window)) {
                window.defer(cx, move |window, cx| to.focus(window, cx));
            }
        },
        |_, _, _, _| {},
    )
    .absolute()
}

fn fade_in<E: Styled + IntoElement + 'static>(id: ElementId, e: E) -> AnimationElement<E> {
    e.with_animation(id, Animation::new(MENU_IN).with_easing(ease_out_quint()), |e, t| e.opacity(t))
}

fn working(agent: &Summary) -> bool {
    Status::of(agent) == Some(Status::Working)
}

impl Desktop {
    /// The agent's open ask in `terminal`'s chat, and whether a deny can carry a reply.
    pub(super) fn chat_ask(&self, terminal: &str) -> Option<(Ask, bool)> {
        let a = self.summary(terminal)?;
        let p = self.agents.pending.iter().find(|p| p.agent_id == a.id)?;
        Some((chat::for_provider(&a.provider).ask(p), p.feedback))
    }

    pub(crate) fn chat_command(&mut self, agent: &str, c: Command, cx: &mut Context<Self>) {
        match c {
            Command::Send(text) => {
                let working = self.agents.get(agent).is_some_and(working);
                let view = self.chats.views.get_mut(agent);
                let held = working && view.is_some();
                if let Some(v) = view {
                    v.session.outgoing.push(text.clone(), working, now_ms());
                }
                if !held {
                    self.outbox.prompt(agent, &text);
                }
            }
            Command::Interrupt => self.outbox.interrupt(agent),
            Command::Compact => self.outbox.compact(agent),
            Command::Allow(r) => self.outbox.resolve(&r, Decision::Allow),
            Command::Deny(r) => self.outbox.resolve(&r, Decision::Deny),
            Command::Reply { request, text } => self.outbox.reply(&request, &text),
            Command::Answer { request, answers } => self.outbox.answer(&request, &answers),
        }
        cx.notify();
    }

    fn focused_chat(&self) -> Option<String> {
        self.terminal.focused.clone().filter(|t| self.chat_shown(t))
    }

    fn chat_enter(&mut self, steer: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(t) = self.focused_chat() {
            self.chat_send(&t, steer, window, cx);
        }
    }

    /// Sends the draft unless the agent can't take it now; Enter never stops.
    /// While the agent works a send waits its turn, unless `steer` sends it into the running turn.
    fn chat_send(&mut self, terminal: &str, steer: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(a) = self.summary(terminal) else { return };
        if composer_mode(self.agents.observe_only(), self.chat_ask(terminal).as_ref().map(|(a, _)| a)) != Mode::Prompt {
            return;
        }
        let busy = working(a);
        if busy && !chat::for_provider(&a.provider).capabilities().send_while_working {
            return;
        }
        let agent = a.id.clone();
        let text = self.chats.composer.draft.prompt(&self.chats.input.read(cx).value());
        if text.is_empty() {
            return;
        }
        self.chats.composer.draft.clear();
        self.chats.input.update(cx, |s, cx| s.set_value("", window, cx));
        if !(steer && busy) {
            return self.chat_command(&agent, Command::Send(text), cx);
        }
        if let Some(v) = self.chats.views.get_mut(&agent) {
            v.session.outgoing.push(text.clone(), false, now_ms());
        }
        self.outbox.prompt(&agent, &text);
        cx.notify();
    }

    fn chat_press(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(a) = self.summary(terminal) else { return };
        let empty = self.chats.input.read(cx).value().trim().is_empty() && self.chats.composer.draft.images.is_empty();
        if chat::action(working(a), empty) == Action::Stop {
            let agent = a.id.clone();
            self.chat_command(&agent, Command::Interrupt, cx);
        } else {
            self.chat_send(terminal, false, window, cx);
        }
    }

    fn chat_stop(&mut self, terminal: &str, cx: &mut Context<Self>) {
        if let Some(a) = self.summary(terminal) {
            let agent = a.id.clone();
            self.chat_command(&agent, Command::Interrupt, cx);
        }
    }

    /// Esc stops a working agent, but only from an empty input and where the agent can be interrupted.
    fn chat_escape(&mut self, terminal: &str, cx: &mut Context<Self>) {
        let Some(a) = self.summary(terminal) else { return };
        if !self.chats.input.read(cx).value().is_empty() || !working(a) || !chat::for_provider(&a.provider).capabilities().interrupt {
            return;
        }
        let agent = a.id.clone();
        self.chat_command(&agent, Command::Interrupt, cx);
    }

    /// `set_value` emits no change, so the composer redraws here.
    pub(crate) fn set_chat_text(&mut self, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.chats.input.update(cx, |s, cx| s.set_value(text.to_string(), window, cx));
        cx.notify();
    }

    fn chat_answer_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(t) = self.focused_chat() else { return };
        if self.chats.composer.replying.is_some() {
            self.chat_reply(&t, window, cx);
        } else {
            self.wizard_advance(&t, window, cx);
        }
    }

    /// `terminal`'s chat composer: the prompt box, or the agent's open ask in its place, over where the agent works and what Enter does.
    pub(crate) fn chat_composer(&self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let Some(agent) = self.summary(terminal) else { return div() };
        let ask = self.chat_ask(terminal);
        let mode = composer_mode(self.agents.observe_only(), ask.as_ref().map(|(a, _)| a));
        let c = &self.chats.composer;
        let input = self.chats.input.read(cx).focus_handle(cx);
        let handoff = match mode {
            _ if self.focused_chat().as_deref() != Some(terminal) => None,
            Mode::Prompt => Some(hand_focus(vec![c.ask_focus.clone(), c.answer.read(cx).focus_handle(cx)], input)),
            Mode::Permission { answerable: true } | Mode::Question { answerable: true } => Some(hand_focus(vec![input], c.ask_focus.clone())),
            _ => None,
        };
        let can_queue = chat::for_provider(&agent.provider).capabilities().send_while_working;
        let placeholder = if working(agent) && can_queue { BUSY_PLACEHOLDER } else { PLACEHOLDER };
        if c.placeholder.replace(placeholder) != placeholder {
            self.chats.input.update(cx, |s, cx| s.set_placeholder(placeholder, window, cx));
        }
        let body = match (mode, ask) {
            (Mode::Permission { answerable }, Some((ask, feedback))) => fade_in(id(format!("chat-ask:{}", ask.request_id)), self.permission_card(terminal, &ask, feedback, answerable, cx)).into_any_element(),
            (Mode::Question { answerable }, Some((ask, _))) => fade_in(id(format!("chat-ask:{}", ask.request_id)), self.question_panel(terminal, &ask, answerable, cx)).into_any_element(),
            _ => self.prompt_box(terminal, agent, mode == Mode::ViewOnly, window, cx).into_any_element(),
        };
        let pending = self.chats.views.get(&agent.id).map_or(&[][..], |v| v.session.outgoing.list());
        let tray = (mode == Mode::Prompt).then(|| self.queue_tray(&agent.id, pending, cx)).flatten();
        let tray = tray.map(|t| fade_in(id(format!("chat-queue:{terminal}")), t));
        let empty = self.chats.input.read(cx).value().trim().is_empty() && c.draft.images.is_empty();
        let asking = matches!(mode, Mode::Permission { .. } | Mode::Question { .. });
        let footer = self.composer_footer(agent, hint(mode, working(agent), empty, can_queue), asking);
        let column = div().w_full().max_w(px(MAX_WIDTH)).flex().flex_col().children(tray).child(body).child(footer).children(handoff);
        div().flex_none().flex().justify_center().px(px(32.)).pb(px(16.)).child(column)
    }

    /// The agent's branch, then `hint`, amber while the agent asks.
    fn composer_footer(&self, agent: &Summary, hint: Option<&'static str>, asking: bool) -> Div {
        let branch = self.worktree_of(&agent.cwd).filter(|t| !t.branch.is_empty()).map(|t| {
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(icon("worktree", 13., TEXT_3))
                .child(div().min_w_0().truncate().font_family(code_font()).text_size(px(11.5)).text_color(TEXT_2).child(t.branch.clone()))
        });
        div()
            .min_w_0()
            .pt(px(8.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(10.))
            .text_size(px(12.))
            .text_color(TEXT_3)
            .children(branch)
            .child(div().flex_1())
            .children(hint.map(|h| div().flex_none().whitespace_nowrap().text_color(if asking { WAITING_TEXT } else { TEXT_4 }).child(h)))
    }

    fn prompt_box(&self, terminal: &str, agent: &Summary, view_only: bool, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let c = &self.chats.composer;
        let caps = chat::for_provider(&agent.provider).capabilities();
        let state = self.chats.input.read(cx);
        let empty = state.value().trim().is_empty() && c.draft.images.is_empty();
        let typing = !empty || state.focus_handle(cx).is_focused(window);
        let action = chat::action(working(agent), empty);
        let blocked = !caps.allows(action);
        let press = |t: &str| {
            let t = t.to_string();
            cx.listener(move |this, _: &ClickEvent, window, cx| this.chat_press(&t, window, cx))
        };
        let ghost = (!view_only && action == Action::Queue && caps.interrupt).then(|| {
            let t = terminal.to_string();
            send_button::ghost_stop(id(format!("chat-stop:{terminal}"))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_stop(&t, cx)))
        });
        let send = (!view_only).then(|| send_button::send_button(id(format!("chat-send:{terminal}")), action, empty, blocked).when(!blocked && !(action == Action::Send && empty), |b| b.on_click(press(terminal))));
        let attach = (!view_only).then(|| attachments::attach_button(terminal, cx));
        let strip = attachments::strip(&c.draft.images, cx);
        let input = if view_only {
            div().px(px(10.)).py(px(8.)).text_color(TEXT_3).child("View only").into_any_element()
        } else {
            let desktop = cx.entity().downgrade();
            Textarea::new(&self.chats.input)
                .appearance(false)
                .text_size(px(INPUT_TEXT_SIZE))
                .line_height(px(INPUT_LINE_HEIGHT))
                .on_paste(move |item, window, cx| {
                    let unsaved = |d: &mut Desktop, (title, detail): (String, String)| d.error = Some(format!("{title}: {detail}"));
                    crate::composer::paste_image(&desktop, |d| &mut d.chats.composer.draft, unsaved, item, window, cx)
                })
                .into_any_element()
        };
        let (ring, drop) = if typing { (TEXT_4, 0x11111314) } else { (SEPARATOR_STRONG, 0x1111130f) };
        let t = terminal.to_string();
        // The textarea pads itself 8 × 10; with these and the border its text sits at the design's 14 × 16 and the box starts 44 tall.
        let text = div().pt(px(5.)).px(px(5.)).child(input);
        let bar = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .pt(px(4.))
            .px(px(7.))
            .pb(px(7.))
            .children(attach)
            .child(div().flex_1())
            .child(self.chat_model_pill(agent, view_only, cx))
            .children(ghost)
            .children(send);
        div()
            .relative()
            .flex()
            .flex_col()
            .rounded(px(RADIUS))
            .bg(SURFACE)
            // A border, not a ring: a shadow fills beneath the box, and dark SURFACE is translucent.
            .border_1()
            .border_color(ring)
            .shadow(vec![ui::shadow(rgba(drop), 8., 28.)])
            .text_size(px(INPUT_TEXT_SIZE))
            .text_color(TEXT)
            .on_mouse_down(MouseButton::Left, cx.listener(|this, _: &MouseDownEvent, window, cx| this.chats.input.update(cx, |s, cx| s.focus(window, cx))))
            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    this.chat_escape(&t, cx);
                }
            }))
            .children(strip)
            .child(text)
            .child(bar)
    }
}

#[cfg(test)]
mod tests {
    use super::{Mode, composer_mode, hint};
    use chat::{Ask, Question, ToolKind};

    fn ask(questions: Vec<Question>) -> Ask {
        Ask { request_id: "r".into(), tool: ToolKind::Shell { command: "ls".into() }, tool_name: "Bash".into(), questions }
    }

    #[test]
    fn the_footer_hints_at_what_enter_does_now() {
        assert_eq!(hint(Mode::Prompt, false, true, true), None);
        assert_eq!(hint(Mode::Prompt, true, true, true), Some("Working · esc to stop"));
        assert_eq!(hint(Mode::Prompt, true, false, true), Some("⏎ queue · ⌘⏎ steer now"));
        assert_eq!(hint(Mode::Prompt, true, false, false), Some("Working · esc to stop"), "an agent that can't take a send mid-turn has nothing to queue");
        assert_eq!(hint(Mode::Permission { answerable: true }, true, true, true), Some("Answer the request above to continue"));
        assert_eq!(hint(Mode::ViewOnly, false, true, true), None);
    }

    #[test]
    fn an_observe_only_agent_gets_no_composer() {
        assert_eq!(composer_mode(true, None), Mode::ViewOnly);
        assert_eq!(composer_mode(false, None), Mode::Prompt);
        assert_eq!(composer_mode(true, Some(&ask(vec![]))), Mode::Permission { answerable: false });
        assert_eq!(composer_mode(false, Some(&ask(vec![Question::default()]))), Mode::Question { answerable: true });
    }
}
