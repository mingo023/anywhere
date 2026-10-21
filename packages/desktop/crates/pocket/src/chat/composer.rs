mod access_pill;
pub(crate) mod attachments;
mod background;
pub(crate) mod btw;
mod goal;
mod model_pill;
mod queue;
pub(crate) mod send_button;
pub(crate) mod suggest;

use super::question::Wizard;
use suggest::Prompt;
use crate::composer::model_menu::Side;
use crate::composer::{Attachment, Draft, Menu};
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use crate::explorer::toast::Notice;
use crate::status::Status;
use crate::util::now_ms;
use agents::{Decision, Summary};
use chat::{Action, Ask, Command, Kind, Local, Session};
use gpui_kit::component::input::{InputEvent, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
use std::collections::HashMap;
use theme::*;
use workspace::Place;

pub(crate) const PLACEHOLDER: &str = "Ask, build, / for commands, @ for files";
const BUSY_PLACEHOLDER: &str = "Send a follow-up into this turn…";
const STOPPING_PLACEHOLDER: &str = "Queue a follow-up…";
const MAX_WIDTH: f32 = 744.;
pub(crate) const RADIUS: f32 = 16.;
pub(crate) const INPUT_TEXT_SIZE: f32 = 14.5;
pub(crate) const INPUT_LINE_HEIGHT: f32 = 22.;
/// The rows that fit 260px; past them the input scrolls.
pub(crate) const MAX_ROWS: usize = (260. / INPUT_LINE_HEIGHT) as usize;

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
pub(super) fn hint(mode: Mode, working: bool, stopping: bool, empty: bool, can_queue: bool) -> Option<&'static str> {
    match mode {
        Mode::ViewOnly => None,
        Mode::Permission { .. } | Mode::Question { .. } => Some("Answer the request above to continue"),
        Mode::Prompt if !working => None,
        Mode::Prompt if stopping => Some("Stopping…"),
        Mode::Prompt if empty || !can_queue => Some("Working · esc to stop"),
        Mode::Prompt => Some("⏎ send into this turn"),
    }
}

/// The input's placeholder: a turn being stopped takes a follow-up only into the queue.
fn placeholder(working: bool, stopping: bool, can_queue: bool) -> &'static str {
    match (working && can_queue, stopping) {
        (false, _) => PLACEHOLDER,
        (true, false) => BUSY_PLACEHOLDER,
        (true, true) => STOPPING_PLACEHOLDER,
    }
}

/// Whether Stop stands beside Steer: while the agent can be stopped. Once stopping, a draft queues and the stop is on its way.
fn stop_beside_steer(action: Action, interrupt: bool) -> bool {
    action == Action::Steer && interrupt
}

/// The prompt input, built here since dropped files and Enter are wired at construction.
pub(crate) fn input(window: &mut Window, cx: &mut Context<Desktop>) -> Entity<TextareaState> {
    let drop = attachments::drop_images(cx.weak_entity());
    cx.new(|cx| TextareaState::new(window, cx).placeholder(PLACEHOLDER).auto_grow(1, MAX_ROWS).submit_on_enter(true).on_drop_paths(drop).decorate(super::mention::decorations))
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
    pub(crate) suggest: suggest::Suggest,
    /// Whether the background tray shows one line rather than its tasks; every chat's tray folds with it.
    pub(crate) background_folded: bool,
    /// Each chat's side question, by agent.
    pub(crate) btw: HashMap<String, btw::Btw>,
}

impl ComposerState {
    pub(crate) fn new(input: &Entity<TextareaState>, window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let answer = cx.new(|cx| TextareaState::new(window, cx).placeholder(super::question::PLACEHOLDER).auto_grow(1, MAX_ROWS).submit_on_enter(true));
        let subs = vec![
            cx.subscribe_in(input, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { shift: false, .. } => this.chat_enter(window, cx),
                InputEvent::Change => this.draft_changed(Prompt::Chat, cx),
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
            suggest: Default::default(),
            background_folded: false,
            btw: HashMap::new(),
        };
        (state, subs)
    }
}

/// Paints each mention's icon over its `@` in `input`, read after the input painted this frame so it sits where the text does.
pub(crate) fn mention_icons(input: Entity<TextareaState>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |_, _, window, cx| {
            let input = input.read(cx);
            let text = input.value();
            let slots: Vec<(Bounds<Pixels>, String)> =
                chat::marks(&text).mentions.into_iter().filter_map(|r| Some((input.range_to_bounds(&(r.start..r.start + 1))?, text[r.start + 1..r.end].to_string()))).collect();
            let visible = input.input_bounds();
            window.with_content_mask(Some(ContentMask { bounds: visible }), |window| {
                super::mention::paint_icons(slots.iter().map(|(slot, path)| (*slot, path.as_str())), 13., window, cx)
            });
        },
    )
    .absolute()
    .size_0()
}

/// The prompt box, ringed darker while typed into.
pub(crate) fn frame(typing: bool) -> Div {
    let (ring, drop) = if typing { (TEXT_4, 0x11111314) } else { (SEPARATOR_STRONG, 0x1111130f) };
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
}

/// The prompt box while files are dragged over it.
pub(crate) fn dragged_over(s: StyleRefinement) -> StyleRefinement {
    s.border_color(ACCENT).shadow(vec![BoxShadow { inset: true, ..ui::ring(ACCENT_RING, 2.) }])
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

/// Why `agent`'s last send went nowhere, while that shows.
pub(super) fn said_in_terminal<'a>(notice: Option<&'a Notice>, agent: &str) -> Option<&'a str> {
    match notice {
        Some(Notice::InTerminal { agent: to, text }) if to == agent => Some(text),
        _ => None,
    }
}

/// Why the chat won't send `command`: only the agent's TUI runs it, and one the chat drives has none.
fn terminal_only_said(command: &str, provider: &str, chat_only: bool) -> String {
    if chat_only {
        format!("{command} isn't available in the chat.")
    } else {
        format!("{command} runs only in {}'s terminal. Run it there.", super::fold::who(provider))
    }
}

/// On a turn, which an agent asking the user still is.
pub(super) fn working(agent: &Summary) -> bool {
    matches!(Status::of(agent), Some(Status::Working | Status::NeedsYou))
}

/// `chat::busy` for the agent's chat; one not yet opened goes by status.
fn busy(agent: &Summary, session: Option<&Session>) -> bool {
    session.map_or_else(|| working(agent), |s| chat::busy(&s.transcript, &s.outgoing, working(agent)))
}

impl Desktop {
    fn chat_busy(&self, agent: &Summary) -> bool {
        busy(agent, self.chats.views.get(&agent.id).map(|v| &v.session))
    }

    fn chat_caps(&self, provider: &str) -> chat::Capabilities {
        chat::capabilities(provider, self.agents.provider_info(provider).chat)
    }

    fn chat_stopping(&self, agent: &Summary) -> bool {
        self.chats.views.get(&agent.id).is_some_and(|v| v.session.outgoing.stopping())
    }

    /// The agent's open ask in `terminal`'s chat, and whether a deny can carry a reply.
    pub(super) fn chat_ask(&self, terminal: &str) -> Option<(Ask, bool)> {
        let a = self.summary(terminal)?;
        let p = self.agents.pending.iter().find(|p| p.agent_id == a.id)?;
        Some((chat::for_provider(&a.provider).ask(p), p.feedback))
    }

    pub(crate) fn chat_command(&mut self, agent: &str, c: Command, cx: &mut Context<Self>) {
        match c {
            Command::Send(text) => {
                let working = self.agents.get(agent).is_some_and(|a| self.chat_busy(a));
                let view = self.chats.views.get_mut(agent);
                let held = working && view.is_some();
                let request = if held { None } else { self.outbox.prompt(agent, &text) };
                if let Some(v) = view {
                    let send = v.session.outgoing.push(text, working, now_ms());
                    if let Some((send, request)) = send.zip(request) {
                        v.session.outgoing.sent_as(send, request);
                    }
                    if !working {
                        v.pin();
                    }
                }
            }
            Command::Interrupt => {
                self.outbox.interrupt(agent);
                if let Some(v) = self.chats.views.get_mut(agent) {
                    v.session.outgoing.stop(now_ms());
                }
            }
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

    fn chat_enter(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(t) = self.focused_chat() {
            self.chat_send(&t, window, cx);
        }
    }

    /// Sends the draft unless the agent can't take it now; Enter never stops.
    /// While the agent works a send goes into the running turn, as the CLI's does; behind a stop it waits for the turn to end.
    fn chat_send(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(a) = self.summary(terminal) else { return };
        if composer_mode(self.agents.observe_only(), self.chat_ask(terminal).as_ref().map(|(a, _)| a)) != Mode::Prompt {
            return;
        }
        let busy = self.chat_busy(a);
        if busy && !self.chat_caps(&a.provider).send_while_working {
            return;
        }
        let steer = busy && !self.chat_stopping(a);
        let (agent, provider, chat_only) = (a.id.clone(), a.provider.clone(), a.chat_only);
        self.chat_take_input(&agent, window, cx);
        let typed = self.chats.input.read(cx).value().to_string();
        let text = self.chats.composer.draft.prompt(&typed);
        if text.is_empty() {
            return;
        }
        // From what was typed: the prompt adds the attached images, which no command's argument holds.
        let slash = chat::slash(&typed).map(|(name, arg)| (chat::for_provider(&provider).command(name, arg), name, arg.to_string()));
        if let Some((Kind::Unsupported, name, _)) = slash {
            return self.show_notice(Notice::InTerminal { agent, text: terminal_only_said(&format!("/{name}"), &provider, chat_only) }, cx);
        }
        self.chats.composer.draft.clear();
        self.chats.input.update(cx, |s, cx| s.set_value("", window, cx));
        if let Some((Kind::Local(local), _, arg)) = slash {
            return self.chat_local(&agent, local, &arg, cx);
        }
        if chat::skips_queue(&typed) {
            self.outbox.prompt(&agent, &text);
            return cx.notify();
        }
        if !steer {
            return self.chat_command(&agent, Command::Send(text), cx);
        }
        let request = self.outbox.prompt(&agent, &text);
        if let Some(v) = self.chats.views.get_mut(&agent) {
            let send = v.session.outgoing.steer_text(text, now_ms());
            if let Some(request) = request {
                v.session.outgoing.sent_as(send, request);
            }
            v.steered();
        }
        cx.notify();
    }

    /// Runs a command the desktop does itself: a picker without its argument opens the model pill on its side.
    fn chat_local(&mut self, agent: &str, local: Local, arg: &str, cx: &mut Context<Self>) {
        match (local, arg) {
            (Local::Model | Local::Effort, "") => {
                self.load_models(cx);
                let draft = &mut self.chats.composer.draft;
                draft.menu = Some(Menu::Model);
                draft.side = Some(if local == Local::Model { Side::Model } else { Side::Effort });
            }
            (Local::Model, model) => self.outbox.settings(agent, model, ""),
            (Local::Effort, effort) => self.outbox.settings(agent, "", effort),
            (Local::Compact, _) => self.chat_command(agent, Command::Compact, cx),
            (Local::New, _) => self.new_chat_tab(Place::Pane(None), cx),
            (Local::Btw, "") => {}
            (Local::Btw, question) => self.chat_btw_ask(agent, question),
        }
        cx.notify();
    }

    fn chat_press(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(a) = self.summary(terminal) else { return };
        let empty = self.chats.input.read(cx).value().trim().is_empty() && self.chats.composer.draft.images.is_empty();
        if chat::action(self.chat_busy(a), self.chat_stopping(a), empty) == Action::Stop {
            let agent = a.id.clone();
            self.chat_command(&agent, Command::Interrupt, cx);
        } else {
            self.chat_send(terminal, window, cx);
        }
    }

    fn chat_stop(&mut self, terminal: &str, cx: &mut Context<Self>) {
        if let Some(a) = self.summary(terminal) {
            let agent = a.id.clone();
            self.chat_command(&agent, Command::Interrupt, cx);
        }
    }

    /// Esc stops a working agent, but only from an empty input, where the agent can be interrupted and isn't being already.
    fn chat_escape(&mut self, terminal: &str, cx: &mut Context<Self>) {
        let Some(a) = self.summary(terminal) else { return };
        let action = chat::action(self.chat_busy(a), self.chat_stopping(a), self.chats.input.read(cx).value().is_empty());
        if action != Action::Stop || !self.chat_caps(&a.provider).interrupt {
            return;
        }
        let agent = a.id.clone();
        self.chat_command(&agent, Command::Interrupt, cx);
    }

    /// What `agent`'s chat has typed and attached: the input's while it holds that chat, else what waits for it.
    pub(super) fn chat_draft(&self, agent: &str, cx: &App) -> (String, &[Attachment]) {
        if self.chats.drafts.owner() == Some(agent) {
            return (self.chats.input.read(cx).value().to_string(), &self.chats.composer.draft.images);
        }
        self.chats.drafts.parked(agent).map_or((String::new(), &[][..]), |(text, draft)| (text.clone(), &draft.images))
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
        let can_queue = self.chat_caps(&agent.provider).send_while_working;
        let (busy, stopping) = (self.chat_busy(agent), self.chat_stopping(agent));
        let placeholder = placeholder(busy, stopping, can_queue);
        if c.placeholder.replace(placeholder) != placeholder {
            self.chats.input.update(cx, |s, cx| s.set_placeholder(placeholder, window, cx));
        }
        let body = match (mode, ask) {
            (Mode::Permission { answerable }, Some((ask, feedback))) => fade_in(id(format!("chat-ask:{}", ask.request_id)), self.permission_card(terminal, &ask, feedback, answerable, cx)).into_any_element(),
            (Mode::Question { answerable }, Some((ask, _))) => fade_in(id(format!("chat-ask:{}", ask.request_id)), self.question_panel(terminal, &ask, answerable, cx)).into_any_element(),
            _ => self.prompt_box(terminal, agent, mode == Mode::ViewOnly, window, cx).into_any_element(),
        };
        let view = self.chats.views.get(&agent.id);
        let pending = view.map_or(&[][..], |v| v.session.outgoing.list());
        let paused = view.is_some_and(|v| v.session.outgoing.paused(&v.session.transcript));
        let tray = (mode == Mode::Prompt).then(|| self.queue_tray(&agent.id, pending, paused, cx)).flatten();
        let tray = tray.map(|t| fade_in(id(format!("chat-queue:{terminal}")), t));
        let goal = (mode == Mode::Prompt).then(|| self.goal_strip(agent, cx)).flatten();
        let background = matches!(mode, Mode::Prompt | Mode::ViewOnly).then(|| view.and_then(|v| self.background_tray(&agent.id, &v.session.transcript, cx))).flatten();
        let background = background.map(|t| fade_in(id(format!("chat-background-in:{terminal}")), t));
        let btw = matches!(mode, Mode::Prompt).then(|| self.btw_card(&agent.id, cx)).flatten();
        let (typed, images) = self.chat_draft(&agent.id, cx);
        let empty = typed.trim().is_empty() && images.is_empty();
        let asking = matches!(mode, Mode::Permission { .. } | Mode::Question { .. });
        let said = said_in_terminal(self.explorer.notice.as_ref().map(|(n, _)| n), &agent.id);
        let footer = self.composer_footer(agent, hint(mode, busy, stopping, empty, can_queue), said, asking);
        let column = div().w_full().max_w(px(MAX_WIDTH)).flex().flex_col().children(goal).children(btw).children(background).children(tray).child(body).child(footer).children(handoff);
        div().flex_none().flex().justify_center().px(px(32.)).pb(px(16.)).child(column)
    }

    /// The agent's branch, then `hint`, amber while the agent asks; `said` stands in for the hint while it shows.
    fn composer_footer(&self, agent: &Summary, hint: Option<&'static str>, said: Option<&str>, asking: bool) -> Div {
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
            .children(match said {
                Some(said) => Some(div().min_w_0().flex().items_center().gap(px(6.)).text_color(TEXT_2).child(icon("terminal", 13., TEXT_3)).child(div().min_w_0().truncate().child(said.to_string()))),
                None => hint.map(|h| div().flex_none().whitespace_nowrap().text_color(if asking { WAITING_TEXT } else { TEXT_4 }).child(h)),
            })
    }

    fn prompt_box(&self, terminal: &str, agent: &Summary, view_only: bool, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let caps = self.chat_caps(&agent.provider);
        let held = self.chats.drafts.owner() == Some(agent.id.as_str());
        let (typed, images) = self.chat_draft(&agent.id, cx);
        let empty = typed.trim().is_empty() && images.is_empty();
        let typing = !empty || (held && self.chats.input.read(cx).focus_handle(cx).is_focused(window));
        let stopping = self.chat_stopping(agent);
        let action = chat::action(self.chat_busy(agent), stopping, empty);
        let blocked = !caps.allows(action);
        let press = |t: &str| {
            let t = t.to_string();
            cx.listener(move |this, _: &ClickEvent, window, cx| this.chat_press(&t, window, cx))
        };
        let ghost = (!view_only && stop_beside_steer(action, caps.interrupt)).then(|| {
            let t = terminal.to_string();
            send_button::ghost_stop(id(format!("chat-stop:{terminal}"))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_stop(&t, cx)))
        });
        let send = (!view_only).then(|| send_button::send_button(id(format!("chat-send:{terminal}")), action, empty, blocked).when(!blocked && !(action == Action::Send && empty), |b| b.on_click(press(terminal))));
        let attach = (!view_only).then(|| attachments::attach_button(id(format!("chat-attach:{terminal}")), |d| &mut d.chats.composer.draft, cx));
        let strip = attachments::strip(images, cx);
        let input = if view_only {
            div().px(px(10.)).py(px(8.)).text_color(TEXT_3).child("View only").into_any_element()
        } else if !held {
            // Another chat's draft is in the input; this one's shows until a press here takes the input.
            let shown = if typed.is_empty() { (self.chats.composer.placeholder.get().to_string(), TEXT_3) } else { (typed, TEXT) };
            div().px(px(10.)).py(px(8.)).line_height(px(INPUT_LINE_HEIGHT)).text_color(shown.1).child(shown.0).into_any_element()
        } else {
            let (desktop, input) = (cx.entity().downgrade(), self.chats.input.clone());
            let textarea = Textarea::new(&self.chats.input)
                .appearance(false)
                .text_size(px(INPUT_TEXT_SIZE))
                .line_height(px(INPUT_LINE_HEIGHT))
                .on_paste(move |item, window, cx| {
                    let unsaved = |d: &mut Desktop, (title, detail): (String, String)| d.error = Some(format!("{title}: {detail}"));
                    crate::composer::paste(&desktop, &input, |d| &mut d.chats.composer.draft, unsaved, item, window, cx)
                });
            div().child(textarea).child(mention_icons(self.chats.input.clone())).into_any_element()
        };
        let (t, a) = (terminal.to_string(), agent.id.clone());
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
            .children(self.chat_access_pill(agent, view_only, cx))
            .child(div().flex_1())
            .child(self.chat_model_pill(agent, view_only, cx))
            .children(ghost)
            .children(send);
        let suggestions = (!view_only && held).then(|| self.suggest_list(Prompt::Chat, cx)).flatten();
        let (desktop, input, drop_agent) = (cx.entity().downgrade(), self.chats.input.clone(), agent.id.clone());
        // Files dropped anywhere on the box go to the input, as a press here takes it. Not a `cx.listener`: the input's drop handler updates `Desktop`.
        let framed = frame(typing).when(!view_only, |f| {
            f.drag_over::<ExternalPaths>(|s, _, _, _| dragged_over(s)).on_drop(move |paths: &ExternalPaths, window, cx| {
                desktop.update(cx, |d, cx| d.chat_take_input(&drop_agent, window, cx)).ok();
                input.update(cx, |s, cx| s.drop_paths(paths, window, cx));
            })
        });
        self.suggest_keys(Prompt::Chat, framed, cx)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    this.chat_take_input(&a, window, cx);
                    this.chats.input.update(cx, |s, cx| s.focus(window, cx));
                }),
            )
            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    this.chat_escape(&t, cx);
                }
            }))
            .children(suggestions)
            .children(strip)
            .child(text)
            .child(bar)
    }
}

#[cfg(test)]
mod tests {
    use super::{Mode, busy, composer_mode, hint, placeholder, said_in_terminal, stop_beside_steer, terminal_only_said, working};
    use crate::explorer::toast::Notice;
    use agents::Summary;
    use chat::{Action, Ask, Block, End, Message, Question, Session, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};

    fn ask(questions: Vec<Question>) -> Ask {
        Ask { request_id: "r".into(), tool: ToolKind::Shell { command: "ls".into() }, tool_name: "Bash".into(), questions }
    }

    fn session(turns: Vec<Turn>) -> Session {
        let mut s = Session::default();
        s.transcript = Transcript { turns };
        s
    }

    fn turn(end: Option<End>) -> Turn {
        Turn { id: "u1".into(), prompt: Some(Message { id: "u1".into(), at: 0, text: "Fix it".into() }), blocks: Vec::new(), end }
    }

    fn agent(status: &str) -> Summary {
        Summary { status: status.into(), attached: true, ..Default::default() }
    }

    #[test]
    fn a_send_right_after_a_stop_goes_out_even_if_status_still_says_working() {
        let stopped = session(vec![turn(Some(End::Failed { error: "Interrupted".into() }))]);
        assert!(!busy(&agent("working"), Some(&stopped)));
        assert!(busy(&agent("idle"), Some(&session(vec![turn(None)]))));
        assert!(busy(&agent("working"), None), "a chat not yet opened goes by status");
    }

    #[test]
    fn sub_agents_left_in_the_background_leave_the_composer_free() {
        let running = Block::Tool(ToolCall { id: "a".into(), name: "Task".into(), kind: ToolKind::Agent(SubAgent::default()), status: ToolStatus::Running, output: None, duration_ms: None, at: 0 });
        let ended = session(vec![Turn { blocks: vec![running], ..turn(Some(End::Done { duration_ms: 5, usage: None })) }]);
        let busy = busy(&agent("idle"), Some(&ended));
        assert!(!busy, "a send starts a new turn");
        assert_eq!(placeholder(busy, false, true), super::PLACEHOLDER);
        assert_eq!(hint(Mode::Prompt, busy, false, true, true), None, "no esc to stop");
        assert_eq!(chat::action(busy, false, true), Action::Send, "no Stop");
    }

    #[test]
    fn a_chat_with_no_turns_yet_goes_by_status() {
        assert!(busy(&agent("working"), Some(&Session::default())));
        assert!(!busy(&agent("idle"), Some(&Session::default())));
    }

    #[test]
    fn a_queued_send_waits_while_the_agent_asks_and_its_transcript_has_no_turns() {
        let mut s = Session::default();
        s.outgoing.push("Next".into(), true, 0);
        s.outgoing.settle(&s.transcript, working(&agent("needsYou")), 1_000);
        assert!(s.outgoing.released().is_empty());
        assert!(busy(&agent("needsYou"), Some(&s)));
    }

    #[test]
    fn a_send_on_its_way_holds_the_next_one() {
        let mut s = session(vec![turn(Some(End::Done { duration_ms: 5, usage: None }))]);
        s.outgoing.push("Next".into(), false, 0);
        assert!(busy(&agent("idle"), Some(&s)));
        s.transcript = Transcript::default();
        assert!(busy(&agent("idle"), Some(&s)));
    }

    #[test]
    fn the_footer_hints_at_what_enter_does_now() {
        assert_eq!(hint(Mode::Prompt, false, false, true, true), None);
        assert_eq!(hint(Mode::Prompt, true, false, true, true), Some("Working · esc to stop"));
        assert_eq!(hint(Mode::Prompt, true, false, false, true), Some("⏎ send into this turn"));
        assert_eq!(hint(Mode::Prompt, true, false, false, false), Some("Working · esc to stop"), "an agent that can't take a send mid-turn has nothing to send");
        assert_eq!(hint(Mode::Permission { answerable: true }, true, false, true, true), Some("Answer the request above to continue"));
        assert_eq!(hint(Mode::ViewOnly, false, false, true, true), None);
    }

    #[test]
    fn a_turn_being_stopped_reads_stopping_and_takes_sends_only_into_the_queue() {
        assert_eq!(hint(Mode::Prompt, true, true, true, true), Some("Stopping…"));
        assert_eq!(hint(Mode::Prompt, true, true, false, true), Some("Stopping…"), "Enter queues instead of steering");
        assert_eq!(hint(Mode::Prompt, false, true, true, true), None, "the turn ended");
        assert_eq!(placeholder(true, true, true), "Queue a follow-up…");
        assert_eq!(placeholder(true, false, true), "Send a follow-up into this turn…");
        assert!(stop_beside_steer(Action::Steer, true));
        assert!(!stop_beside_steer(Action::Queue, true), "the stop is already on its way");
        assert!(!stop_beside_steer(Action::Steer, false));
    }

    #[test]
    fn a_terminal_only_command_points_at_the_terminal_only_when_the_agent_has_its_tui() {
        assert_eq!(terminal_only_said("/cost", "claude", false), "/cost runs only in Claude's terminal. Run it there.");
        assert_eq!(terminal_only_said("/cost", "claude", true), "/cost isn't available in the chat.");
    }

    #[test]
    fn the_footer_of_the_chat_that_sent_a_terminal_command_says_where_to_run_it() {
        let said = Notice::InTerminal { agent: "a1".into(), text: "/cost opens a panel in Claude's terminal. Run it there.".into() };
        assert_eq!(said_in_terminal(Some(&said), "a1"), Some("/cost opens a panel in Claude's terminal. Run it there."));
        assert_eq!(said_in_terminal(Some(&said), "a2"), None, "another chat's footer keeps its hint");
        assert_eq!(said_in_terminal(Some(&Notice::Copied("a.rs".into())), "a1"), None);
        assert_eq!(said_in_terminal(None, "a1"), None);
    }

    #[test]
    fn an_observe_only_agent_gets_no_composer() {
        assert_eq!(composer_mode(true, None), Mode::ViewOnly);
        assert_eq!(composer_mode(false, None), Mode::Prompt);
        assert_eq!(composer_mode(true, Some(&ask(vec![]))), Mode::Permission { answerable: false });
        assert_eq!(composer_mode(false, Some(&ask(vec![Question::default()]))), Mode::Question { answerable: true });
    }
}
