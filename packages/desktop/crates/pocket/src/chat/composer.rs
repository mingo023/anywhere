mod access_pill;
mod account_chip;
mod attachments;
mod background;
pub(crate) mod btw;
mod goal;
mod model_pill;
mod queue;
pub(crate) mod send_button;
pub(crate) mod suggest;
mod switch_agent;

use super::question::Wizard;
use suggest::Prompt;
use crate::composer::{Draft, Menu};
use crate::composer::add_menu::{self, Opens};
use crate::composer::model_menu::focus_search;
use crate::composer::{team_chip, team_offered};
use crate::desktop::Desktop;
use crate::renames;
use crate::desktop::chrome::id;
use crate::explorer::toast::Notice;
use crate::status::Status;
use crate::util::now_ms;
use agents::{Decision, Summary};
use chat::{Action, Ask, Command, Kind, Local, Session};
use gpui_kit::component::input::{InputEditsEvent, InputEvent, InputState, RopeExt as _, Textarea, TextareaState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::cell::Cell;
use std::collections::HashMap;
use theme::*;
use workspace::Place;

/// [DEBUG-focus] Where `log_focus_miss` appends.
pub(crate) static FOCUS_LOG: std::sync::OnceLock<std::path::PathBuf> = std::sync::OnceLock::new();

pub(crate) const PLACEHOLDER: &str = "Ask, build, / for commands, @ for files";
const BUSY_PLACEHOLDER: &str = "Send a follow-up into this turn…";
const STOPPING_PLACEHOLDER: &str = "Queue a follow-up…";
const MAX_WIDTH: f32 = 904.;
pub(crate) const RADIUS: f32 = 16.;
pub(crate) const INPUT_TEXT_SIZE: f32 = 14.5;
pub(crate) const INPUT_LINE_HEIGHT: f32 = 22.;
/// The rows that fit 260px; past them the input scrolls.
pub(crate) const MAX_ROWS: usize = (260. / INPUT_LINE_HEIGHT) as usize;
/// The rows the input keeps room for while empty, so the prompt leads the box.
pub(crate) const MIN_ROWS: usize = 2;

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

/// The footer's hint at what Enter does now; `team` while the Team chip is set.
pub(super) fn hint(mode: Mode, working: bool, stopping: bool, empty: bool, can_queue: bool, team: bool) -> Option<&'static str> {
    match mode {
        Mode::ViewOnly => None,
        Mode::Permission { .. } | Mode::Question { .. } => Some("Answer the request above to continue"),
        Mode::Prompt if !working => None,
        Mode::Prompt if stopping => Some("Stopping…"),
        Mode::Prompt if team => Some("Team waits for this turn to end · esc to stop"),
        Mode::Prompt if empty || !can_queue => Some("Working · esc to stop"),
        Mode::Prompt => Some("⏎ send into this turn · ⌘⏎ queue for after"),
    }
}

/// What the Team chip does to a send.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(super) enum TeamSend {
    /// The send goes as it would without the chip.
    Off,
    /// The send asks the agent for a team plan.
    On,
    /// The send stays in the box: the plan starts a turn of its own, and the queue can't carry the flag.
    Wait,
}

/// A command goes as typed, since pocketd would wrap it in the team rules.
pub(super) fn team_send(team: bool, command: bool, busy: bool) -> TeamSend {
    match (team && !command, busy) {
        (false, _) => TeamSend::Off,
        (true, true) => TeamSend::Wait,
        (true, false) => TeamSend::On,
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

/// The text `key` types, if it types any: shortcuts and keys like Enter, Tab or Escape don't.
fn typed_text(key: &Keystroke) -> Option<String> {
    let m = &key.modifiers;
    if m.platform || m.control || m.function {
        return None;
    }
    key.key_char.clone().filter(|c| !c.is_empty() && !c.chars().any(char::is_control))
}

/// Whether a `TextView` holds focus, as after a click on a reply's markdown.
fn text_view_focused(contexts: &[KeyContext]) -> bool {
    contexts.last().is_some_and(|c| c.contains("TextView"))
}

/// The prompt input, built here since dropped files and Enter are wired at construction.
pub(crate) fn input(window: &mut Window, cx: &mut Context<Desktop>) -> Entity<TextareaState> {
    let drop = crate::composer::drop_handler(cx.weak_entity(), |d| &mut d.chats.composer.draft);
    cx.new(|cx| TextareaState::new(window, cx).placeholder(PLACEHOLDER).auto_grow(MIN_ROWS, MAX_ROWS).submit_on_enter(true).on_drop_paths(drop).decorate(super::mention::decorations))
}

/// The chat composer's state besides its input (`ChatState::input`), shared by every chat pane.
pub(crate) struct ComposerState {
    pub(crate) draft: Draft,
    /// Narrows the model panel's list.
    pub(crate) search: Entity<InputState>,
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
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search models"));
        let subs = vec![
            cx.subscribe(&search, |_, _, _: &InputEvent, cx| cx.notify()),
            cx.subscribe_in(input, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { shift: false, secondary } => this.chat_enter(*secondary, window, cx),
                InputEvent::Change => {
                    this.pill_typed_command(Prompt::Chat, window, cx);
                    this.draft_changed(Prompt::Chat, cx);
                }
                _ => {}
            }),
            cx.subscribe_in(input, window, |this, _, ev: &InputEditsEvent, window, cx| this.rich_typed(Prompt::Chat, ev, window, cx)),
            cx.subscribe_in(&answer, window, |this, _, ev: &InputEvent, window, cx| match ev {
                InputEvent::PressEnter { shift: false, .. } => this.chat_answer_enter(window, cx),
                InputEvent::Change => cx.notify(),
                _ => {}
            }),
        ];
        let state = Self {
            draft: Draft::default(),
            search,
            answer,
            wizard: None,
            ask_focus: cx.focus_handle(),
            advance: None,
            replying: None,
            placeholder: Cell::new(PLACEHOLDER),
            suggest: Default::default(),
            background_folded: true,
            btw: HashMap::new(),
        };
        (state, subs)
    }
}

/// A code span's rounded shade, inset from the line it sits on; none for a span wrapped onto another line.
pub(super) fn code_shade(span: Bounds<Pixels>) -> Option<PaintQuad> {
    let one_line = span.size.width > px(0.) && span.size.height <= px(INPUT_LINE_HEIGHT);
    one_line.then(|| quad(Bounds::new(span.origin + point(px(0.), px(2.)), size(span.size.width, span.size.height - px(4.))), px(5.), Hsla::from(FILL_3), px(1.), HAIRLINE, BorderStyle::Solid))
}

/// A fenced block's shade, from its opening fence's line down to its closing one's and as wide as `within`.
pub(super) fn block_shade(top: Pixels, bottom: Pixels, within: Bounds<Pixels>) -> PaintQuad {
    quad(Bounds::from_corners(point(within.left(), top), point(within.right(), bottom)), px(8.), Hsla::from(FILL_2), px(1.), HAIRLINE, BorderStyle::Solid)
}

/// Paints over `input` once it painted this frame, so each sits where the text does: a rounded shade over each code
/// span, translucent so its text reads through, and each mention's icon over its `@`.
pub(crate) fn marks_overlay(input: Entity<TextareaState>) -> impl IntoElement {
    canvas(
        |_, _, _| (),
        move |_, _, window, cx| {
            let input = input.read(cx);
            let text = input.value();
            let (marks, code) = chat::marks_outside_code(&text);
            let visible = input.input_bounds();
            let shades: Vec<PaintQuad> = code
                .iter()
                .filter_map(|c| match c {
                    chat::Code::Inline(r) => input.range_to_bounds(r).and_then(code_shade),
                    chat::Code::Block { open, close } => Some(block_shade(input.range_to_bounds(open)?.top(), input.range_to_bounds(close)?.bottom(), visible)),
                })
                .collect();
            let slots: Vec<(Bounds<Pixels>, String)> = marks.mentions.into_iter().filter_map(|r| Some((input.range_to_bounds(&(r.start..r.start + 1))?, text[r.start + 1..r.end].to_string()))).collect();
            window.with_content_mask(Some(ContentMask { bounds: visible }), |window| {
                shades.into_iter().for_each(|q| window.paint_quad(q));
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

/// Laid over a whole conversation so files dropped anywhere on it reach its prompt; dashed in while they hover.
pub(crate) fn drop_zone(on_drop: impl Fn(&ExternalPaths, &mut Window, &mut App) + 'static) -> Div {
    div()
        .absolute()
        .top(px(4.))
        .bottom(px(4.))
        .left(px(4.))
        .right(px(4.))
        .rounded(px(RADIUS))
        .drag_over::<ExternalPaths>(|s, _, _, _| s.border_1().border_dashed().border_color(ACCENT).bg(Hsla::from(ACCENT).opacity(0.06)))
        .on_drop(on_drop)
}

/// Moves focus to `to` when one of `from` holds it, so focus follows the pill into the ask that replaces it and back.
pub(crate) fn hand_focus(from: Vec<FocusHandle>, to: FocusHandle) -> impl IntoElement {
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

fn close_menu(this: &mut Desktop, cx: &mut Context<Desktop>) {
    this.chats.composer.draft.menu = None;
    cx.notify();
}

/// A chip's menu of `rows` under `title`, opening upward from the chip's left edge as the composer sits at the bottom;
/// a press outside closes it.
fn chip_menu(id: &'static str, title: &'static str, width: f32, rows: impl IntoIterator<Item = impl IntoElement>, cx: &mut Context<Desktop>) -> Div {
    hang(id, crate::composer::menu_list(id, title, width, rows), cx)
}

/// `menu` opening upward from the chip's left edge; a press outside it closes it.
fn hang(id: &'static str, menu: impl IntoElement, cx: &mut Context<Desktop>) -> Div {
    let menu = div()
        .id(SharedString::from(format!("{id}-out")))
        // Menus hang over the transcript; without this a click on one also lands on what it covers.
        .occlude()
        .child(menu)
        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| close_menu(this, cx)));
    let menu = ui::menu_in(SharedString::from(format!("{id}-in")), menu);
    div().absolute().top(px(-6.)).left_0().child(deferred(anchored().anchor(Anchor::BottomLeft).snap_to_window_with_margin(px(8.)).child(menu)).with_priority(1))
}

fn fade_in<E: Styled + IntoElement + 'static>(id: ElementId, e: E) -> AnimationElement<E> {
    e.with_animation(id, Animation::new(MENU_IN).with_easing(ease_out_quint()), |e, t| e.opacity(t))
}

/// What the chat said to `agent`'s footer, while that shows.
pub(super) fn said_in_chat<'a>(notice: Option<&'a Notice>, agent: &str) -> Option<(&'static str, &'a str)> {
    match notice {
        Some(Notice::InChat { agent: to, icon, text }) if to == agent => Some((icon, text)),
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

/// Whether `/remote-control <arg>` turns it on or off; none for an argument it doesn't take.
fn remote_control_on(arg: &str) -> Option<bool> {
    match arg {
        "" | "on" => Some(true),
        "off" => Some(false),
        _ => None,
    }
}

/// What the footer says once Remote Control answered; a link it gave is on the clipboard.
fn remote_control_said(url: &str) -> &'static str {
    if url.is_empty() { "Remote Control is off." } else { "Remote Control is on. Its link is copied." }
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

    pub(super) fn chat_caps(&self, provider: &str) -> chat::Capabilities {
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
            Command::Send(text) => self.chat_prompt(agent, text, false),
            Command::Interrupt => {
                self.outbox.interrupt(agent);
                if let Some(v) = self.chats.views.get_mut(agent) {
                    v.session.outgoing.stop(now_ms());
                }
            }
            Command::SendNow => {
                if self.chats.views.get_mut(agent).is_some_and(|v| v.session.outgoing.send_now(now_ms())) {
                    self.outbox.send_now(agent);
                }
            }
            Command::Compact => {
                self.outbox.compact(agent);
                let busy = self.agents.get(agent).is_some_and(|a| self.chat_busy(a));
                if let Some(v) = self.chats.views.get_mut(agent) {
                    v.session.outgoing.compact(busy, now_ms());
                    if busy { v.steered() } else { v.pin() }
                }
            }
            Command::Allow(r) => self.outbox.resolve(&r, Decision::Allow),
            Command::Deny(r) => self.outbox.resolve(&r, Decision::Deny),
            Command::Reply { request, text } => self.outbox.reply(&request, &text),
            Command::Answer { request, answers } => self.outbox.answer(&request, &answers),
        }
        cx.notify();
    }

    /// Sends `text`, or holds it in the queue while the agent works; `team` asks for a team plan.
    fn chat_prompt(&mut self, agent: &str, text: String, team: bool) {
        let working = self.agents.get(agent).is_some_and(|a| self.chat_busy(a));
        let view = self.chats.views.get_mut(agent);
        let held = working && view.is_some();
        let request = if held { None } else { self.outbox.prompt(agent, &text, team) };
        if let Some(v) = view {
            let send = v.session.outgoing.push(text, working, now_ms());
            if let (Some(send), true) = (send, team) {
                v.session.outgoing.asks_team(send);
            }
            if let Some((send, request)) = send.zip(request) {
                v.session.outgoing.sent_as(send, request);
            }
            if !working {
                v.pin();
            }
        }
    }

    fn focused_chat(&self) -> Option<String> {
        self.terminal.focused.clone().filter(|t| self.chat_shown(t))
    }

    fn chat_enter(&mut self, queue: bool, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(t) = self.focused_chat() {
            self.chat_send(&t, queue, window, cx);
        }
    }

    fn chat_mode(&self, terminal: &str) -> Mode {
        composer_mode(self.agents.observe_only(), self.chat_ask(terminal).as_ref().map(|(a, _)| a))
    }

    /// Text typed while nothing holds the keys goes to the focused pane's chat, as in Discord. Returns whether it did.
    pub(crate) fn type_into_chat(&mut self, key: &Keystroke, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let Some(text) = typed_text(key) else { return false };
        if !self.keys_free(window) || !self.panes_on_screen() {
            return false;
        }
        let Some(t) = self.focused_chat().filter(|t| self.chat_mode(t) == Mode::Prompt) else { return false };
        let Some(agent) = self.summary(&t).map(|a| a.id.clone()) else { return false };
        self.chat_focus_input(&agent, window, cx);
        self.chats.input.update(cx, |s, cx| s.replace_text_in_range(None, &text, window, cx));
        true
    }

    /// Whether nothing that takes typing holds the keys.
    fn keys_free(&self, window: &Window) -> bool {
        self.root.is_focused(window) || text_view_focused(&window.context_stack())
    }

    /// [DEBUG-focus] Logs text typed while a chat is focused that neither its input nor `type_into_chat` takes.
    pub(crate) fn log_focus_miss(&self, key: &Keystroke, window: &Window, cx: &App) {
        let (Some(_), Some(t), Some(path)) = (typed_text(key), self.focused_chat(), FOCUS_LOG.get()) else { return };
        if self.chats.input.read(cx).focus_handle(cx).is_focused(window) {
            return;
        }
        let mode = self.chat_mode(&t);
        if self.keys_free(window) && self.panes_on_screen() && mode == Mode::Prompt {
            return;
        }
        let c = &self.chats.composer;
        let named = [
            ("root", self.root.clone()),
            ("terminal", self.terminal.focus.clone()),
            ("ask", c.ask_focus.clone()),
            ("answer", c.answer.read(cx).focus_handle(cx)),
            ("map", self.chats.map.focus.clone()),
            ("browser", self.browsers.focus.clone()),
            ("inbox", self.inbox.focus.clone()),
            ("sidebar-filter", self.sidebar.filter.input.read(cx).focus_handle(cx)),
        ];
        let focus = match window.focused(cx) {
            None => "none",
            Some(f) => named.iter().find(|(_, h)| *h == f).map_or("other", |(n, _)| n),
        };
        let menus = [
            ("panels.menu", self.panels.menu.is_some()),
            ("panels.actions", self.panels.actions.is_some()),
            ("panels.top_actions", self.panels.top_actions.is_some()),
            ("row_menu", self.row_menu.is_some()),
            ("changes.commit_menu", self.changes.commit_menu),
            ("changes.menu", self.changes.menu),
            ("pr.ship_menu", self.pr.ship_menu),
            ("pr.merge_menu", self.pr.merge_menu),
            ("pr.create_menu", self.pr.create_menu),
            ("sidebar.picker", self.sidebar.picker.open),
            ("sidebar.add_menu", self.sidebar.add_menu),
            ("sidebar.organize", self.sidebar.organize.menu),
            ("automations.menu", self.automations.menu.is_some()),
            ("explorer.menu", self.explorer.menu.is_some()),
            ("terminal.menu", self.terminal.menu.is_some()),
            ("accounts.menu", self.accounts.menu.is_some()),
            ("clone_form.group_menu", self.clone_form.group_menu),
            ("chats.files", self.chats.files.is_some()),
        ];
        let open: Vec<&str> = menus.iter().filter(|(_, on)| *on).map(|(n, _)| *n).collect();
        let line = format!(
            "{} key={:?} focus={focus} active={} screen={:?} tree={} create={} overlay={} menus={open:?} mode={mode:?} replying={}\n",
            now_ms(),
            key.key,
            window.is_window_active(),
            self.screen,
            self.session_tree().is_some(),
            self.shown_create().is_some(),
            self.overlay.is_some(),
            c.replying.is_some(),
        );
        if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
            let _ = std::io::Write::write_all(&mut f, line.as_bytes());
        }
    }

    /// Sends the draft unless the agent can't take it now; Enter never stops.
    /// While the agent works a send goes into the running turn, as the CLI's does; behind a stop, or to `queue`, it waits for the turn to end.
    fn chat_send(&mut self, terminal: &str, queue: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(a) = self.summary(terminal) else { return };
        if self.chat_mode(terminal) != Mode::Prompt {
            return;
        }
        let (busy, caps) = (self.chat_busy(a), self.chat_caps(&a.provider));
        if busy && !caps.send_while_working {
            return;
        }
        let steer = busy && !self.chat_stopping(a) && !queue;
        let (agent, provider, chat_only) = (a.id.clone(), a.provider.clone(), a.chat_only);
        self.chat_take_input(&agent, window, cx);
        let typed = self.chats.input.read(cx).value().to_string();
        let text = self.chats.composer.draft.prompt(&typed);
        if text.is_empty() {
            return;
        }
        // From what was typed: the prompt adds the attached images, which no command's argument holds.
        let slash = chat::slash(&typed).map(|(name, arg)| (caps.runs(chat::for_provider(&provider).command(name, arg)), name, arg.to_string()));
        if let Some((Kind::Unsupported, name, _)) = slash {
            return self.show_notice(Notice::InChat { agent, icon: "terminal", text: terminal_only_said(&format!("/{name}"), &provider, chat_only) }, cx);
        }
        let team = team_send(self.chats.composer.draft.team && team_offered(self.agents.teams_offered(), chat_only, &provider), slash.is_some(), busy);
        if team == TeamSend::Wait {
            return cx.notify();
        }
        self.chats.composer.draft.clear();
        self.chats.input.update(cx, |s, cx| s.set_value("", window, cx));
        if let Some((Kind::Local(local), _, arg)) = slash {
            return self.chat_local(&agent, local, &arg, window, cx);
        }
        // A command's name must lead what it sends.
        let text = if slash.is_some() { text } else { self.chats.composer.suggest.with_mcp_context(&agent, &provider, &text) };
        if chat::skips_queue(&typed) {
            self.outbox.prompt(&agent, &text, false);
            return cx.notify();
        }
        if !steer {
            self.chat_prompt(&agent, text, team == TeamSend::On);
            return cx.notify();
        }
        let request = self.outbox.prompt(&agent, &text, false);
        if let Some(v) = self.chats.views.get_mut(&agent) {
            let send = v.session.outgoing.steer_text(text, now_ms());
            if let Some(request) = request {
                v.session.outgoing.sent_as(send, request);
            }
            v.steered();
        }
        cx.notify();
    }

    /// Remote Control's answer for `agent`'s chat: its link goes to the clipboard and the footer says so.
    pub(crate) fn chat_remote_controlled(&mut self, agent: String, url: Result<String, String>, cx: &mut Context<Self>) {
        match url {
            Ok(url) => {
                if !url.is_empty() {
                    cx.write_to_clipboard(ClipboardItem::new_string(url.clone()));
                }
                self.show_notice(Notice::InChat { agent, icon: "check", text: remote_control_said(&url).into() }, cx);
            }
            Err(reason) => {
                self.error = Some(reason);
                cx.notify();
            }
        }
    }

    /// Runs a command the desktop does itself: a picker without its argument opens the model pill on its side, and
    /// `/mcp` lists the servers to tag, its argument as the query.
    fn chat_local(&mut self, agent: &str, local: Local, arg: &str, window: &mut Window, cx: &mut Context<Self>) {
        match (local, arg) {
            (Local::Model | Local::Effort, "") => {
                if self.chats.composer.draft.menu != Some(Menu::Model) && self.chats.composer.draft.toggle(Menu::Model) {
                    let search = self.chats.composer.search.clone();
                    focus_search(&search, window, cx);
                    self.chat_models_opened(agent, cx);
                }
            }
            (Local::Model, model) => self.outbox.settings(agent, model, ""),
            (Local::Effort, effort) => self.outbox.settings(agent, "", effort),
            (Local::Compact, _) => self.chat_command(agent, Command::Compact, cx),
            (Local::New, _) => self.new_chat_tab(Place::Pane(None), cx),
            (Local::Btw, "") => {}
            (Local::Btw, question) => self.chat_btw_ask(agent, question),
            (Local::Mcp, query) => self.type_text(Prompt::Chat, &format!("{}{query}", chat::mcp::TAG), window, cx),
            (Local::RemoteControl, arg) => match remote_control_on(arg) {
                Some(on) => self.outbox.remote_control(agent, on),
                None => self.show_notice(Notice::InChat { agent: agent.into(), icon: "warning", text: "/remote-control takes on or off.".into() }, cx),
            },
        }
        cx.notify();
    }

    fn chat_press(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(a) = self.summary(terminal) else { return };
        let empty = self.chats.input.read(cx).value().trim().is_empty() && !self.chats.composer.draft.has_attachments();
        if chat::action(self.chat_busy(a), self.chat_stopping(a), empty) == Action::Stop {
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
    pub(crate) fn chat_draft(&self, agent: &str, cx: &App) -> (String, Option<&Draft>) {
        if self.chats.drafts.owner() == Some(agent) {
            return (self.chats.input.read(cx).value().to_string(), Some(&self.chats.composer.draft));
        }
        self.chats.drafts.parked(agent).map_or((String::new(), None), |(text, draft)| (text.clone(), Some(draft)))
    }

    /// Whether `agent`'s draft has the Team chip set, held in the input or parked.
    fn chat_team(&self, agent: &str) -> bool {
        if self.chats.drafts.owner() == Some(agent) {
            return self.chats.composer.draft.team;
        }
        self.chats.drafts.parked(agent).is_some_and(|(_, draft)| draft.team)
    }

    /// Adds `text` to the end of `agent`'s draft, in the input when its chat holds it.
    pub(crate) fn chat_add_text(&mut self, agent: &str, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        if self.chats.drafts.owner() != Some(agent) {
            let (draft, _) = self.chats.drafts.parked_mut(agent);
            *draft = added(draft, text);
            return;
        }
        let value = added(&self.chats.input.read(cx).value(), text);
        self.chats.input.update(cx, |s, cx| {
            let end = value.len();
            s.set_value(crate::composer::pills::kept_pills(&s.content(), value, None), window, cx);
            let end = s.text().offset_to_position(end);
            s.set_cursor_position(end, window, cx);
        });
        cx.notify();
    }

    /// `agent`'s draft to attach to: the input's while its chat holds it, else the one parked for it.
    pub(crate) fn chat_draft_mut(&mut self, agent: &str) -> &mut Draft {
        if self.chats.drafts.owner() == Some(agent) {
            return &mut self.chats.composer.draft;
        }
        &mut self.chats.drafts.parked_mut(agent).1
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
            Mode::Prompt => {
                let mut from = vec![c.ask_focus.clone(), c.answer.read(cx).focus_handle(cx)];
                // The search takes the keys while the model panel is open; closing it would leave them nowhere.
                if c.draft.menu != Some(Menu::Model) {
                    from.push(c.search.read(cx).focus_handle(cx));
                }
                Some(hand_focus(from, input))
            }
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
        let interrupt = self.chat_caps(&agent.provider).interrupt;
        let stop = |tasks: &[&chat::ToolCall]| mode == Mode::Prompt && background::stops(agent.chat_only, interrupt, busy, tasks);
        let background = matches!(mode, Mode::Prompt | Mode::ViewOnly).then(|| view.and_then(|v| self.background_tray(&agent.id, &v.session.transcript, stop, cx))).flatten();
        let background = background.map(|t| fade_in(id(format!("chat-background-in:{terminal}")), t));
        let btw = matches!(mode, Mode::Prompt).then(|| self.btw_card(&agent.id, cx)).flatten();
        let (typed, draft) = self.chat_draft(&agent.id, cx);
        let empty = typed.trim().is_empty() && !draft.is_some_and(Draft::has_attachments);
        let asking = matches!(mode, Mode::Permission { .. } | Mode::Question { .. });
        let said = said_in_chat(self.explorer.notice.as_ref().map(|(n, _)| n), &agent.id);
        let account = (mode != Mode::ViewOnly).then(|| self.chat_account_chip(agent, cx)).flatten();
        let team = team_offered(self.agents.teams_offered(), agent.chat_only, &agent.provider) && self.chat_team(&agent.id);
        let footer = self.composer_footer(agent, account, hint(mode, busy, stopping, empty, can_queue, team), said, asking, cx.reduce_motion());
        let map = view.and_then(|v| self.chat_agent_map(agent, &v.session.transcript, cx));
        let island = self.team_island(agent, cx);
        let column = div().w_full().max_w(px(MAX_WIDTH)).flex().flex_col().children(island).children(map).children(btw).children(background).children(tray).child(body).child(footer).children(handoff);
        div().flex_none().flex().justify_center().px(px(32.)).pb(px(16.)).child(column)
    }

    /// The agent's branch and `account`, then `hint`, amber while the agent asks; `said` stands in for the hint while it shows.
    fn composer_footer(&self, agent: &Summary, account: Option<Div>, hint: Option<&'static str>, said: Option<(&'static str, &str)>, asking: bool, reduce_motion: bool) -> Div {
        let branch = self.worktree_of(&agent.cwd).filter(|t| !t.branch.is_empty()).map(|t| {
            div()
                .min_w_0()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(icon("worktree", 13., TEXT_3))
                .child(div().min_w_0().truncate().font_family(code_font()).text_size(px(11.5)).text_color(TEXT_2).child(renames::branch_label(&self.renames, "composer-branch", &t.path, &t.branch, TEXT_2, reduce_motion)))
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
            .children(account)
            .child(div().flex_1())
            .children(match said {
                Some((glyph, said)) => Some(div().min_w_0().flex().items_center().gap(px(6.)).text_color(TEXT_2).child(icon(glyph, 13., TEXT_3)).child(div().min_w_0().truncate().child(said.to_string()))),
                None => hint.map(|h| div().flex_none().whitespace_nowrap().text_color(if asking { WAITING_TEXT } else { TEXT_4 }).child(h)),
            })
    }

    /// Where files dropped on `terminal`'s chat go to its input, while its prompt box is the one shown and takes them.
    pub(crate) fn chat_drop_zone(&self, terminal: &str, cx: &mut Context<Self>) -> Option<Div> {
        let agent = self.summary(terminal)?.id.clone();
        if composer_mode(self.agents.observe_only(), self.chat_ask(terminal).as_ref().map(|(a, _)| a)) != Mode::Prompt {
            return None;
        }
        let (desktop, input) = (cx.entity().downgrade(), self.chats.input.clone());
        // Not a `cx.listener`: the input's drop handler updates `Desktop`.
        Some(drop_zone(move |paths, window, cx| {
            desktop.update(cx, |d, cx| d.chat_take_input(&agent, window, cx)).ok();
            input.update(cx, |s, cx| s.drop_paths(paths, window, cx));
        }))
    }

    fn prompt_box(&self, terminal: &str, agent: &Summary, view_only: bool, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let caps = self.chat_caps(&agent.provider);
        let held = self.chats.drafts.owner() == Some(agent.id.as_str());
        let (typed, draft) = self.chat_draft(&agent.id, cx);
        let empty = typed.trim().is_empty() && !draft.is_some_and(Draft::has_attachments);
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
        let team = team_offered(self.agents.teams_offered(), agent.chat_only, &agent.provider).then(|| self.chat_team(&agent.id));
        let add = (!view_only).then(|| {
            let (agent, open) = (agent.id.clone(), held && self.chats.composer.draft.menu == Some(Menu::Add));
            let toggle = move |this: &mut Desktop, window: &mut Window, cx: &mut Context<Desktop>| {
                this.chat_take_input(&agent, window, cx);
                this.chats.composer.draft.toggle(Menu::Add);
                cx.notify();
            };
            add_menu::add_button(id(format!("chat-add:{terminal}")), open, team, Opens::Up, |d| &mut d.chats.composer.draft, toggle, cx)
        });
        let chip = (!view_only && team == Some(true)).then(|| {
            let agent = agent.id.clone();
            team_chip::team_chip(id(format!("chat-team-remove:{terminal}")), move |this, window, cx| {
                this.chat_take_input(&agent, window, cx);
                this.chats.composer.draft.team = false;
            }, cx)
        });
        let strip = draft.and_then(|d| attachments::strip(d, cx));
        let input = if view_only {
            div().px(px(10.)).py(px(8.)).text_color(TEXT_3).child("View only").into_any_element()
        } else if !held {
            // Another chat's draft is in the input; this one's shows until a press here takes the input.
            let shown = if typed.is_empty() { (self.chats.composer.placeholder.get().to_string(), TEXT_3) } else { (typed, TEXT) };
            div().px(px(10.)).py(px(8.)).min_h(px(MIN_ROWS as f32 * INPUT_LINE_HEIGHT + 16.)).line_height(px(INPUT_LINE_HEIGHT)).text_color(shown.1).child(shown.0).into_any_element()
        } else {
            let (desktop, input) = (cx.entity().downgrade(), self.chats.input.clone());
            let textarea = crate::composer::pills::with_pills(Textarea::new(&self.chats.input), &self.chats.input)
                .appearance(false)
                .text_size(px(INPUT_TEXT_SIZE))
                .line_height(px(INPUT_LINE_HEIGHT))
                .on_paste(move |item, window, cx| {
                    let unsaved = |d: &mut Desktop, (title, detail): (String, String)| d.error = Some(format!("{title}: {detail}"));
                    crate::composer::paste(&desktop, &input, |d| &mut d.chats.composer.draft, unsaved, item, window, cx)
                });
            div().child(textarea).child(marks_overlay(self.chats.input.clone())).into_any_element()
        };
        let (t, a) = (terminal.to_string(), agent.id.clone());
        let text = crate::composer::prompt_text().child(input);
        let bar = crate::composer::prompt_bar()
            .children(self.chat_goal_pill(agent, view_only, cx))
            .child(self.chat_model_pill(agent, view_only, cx))
            .children(self.chat_access_pill(agent, view_only, cx))
            .children(self.chats.views.get(&agent.id).and_then(|v| self.chat_agents_chip(agent, &v.session.transcript, cx)))
            .children(chip)
            .child(div().flex_1())
            .children(add)
            .children(ghost)
            .children(send);
        let suggestions = (!view_only && held).then(|| self.suggest_list(Prompt::Chat, cx)).flatten();
        let keyed = self.suggest_keys(Prompt::Chat, frame(typing), cx);
        self.rich_keys(Prompt::Chat, keyed, cx)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| this.chat_focus_input(&a, window, cx)),
            )
            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    if this.escape_composer_menu() {
                        cx.stop_propagation();
                        return cx.notify();
                    }
                    this.chat_escape(&t, cx);
                }
            }))
            .children(suggestions)
            .children(strip)
            .child(text)
            .child(bar)
    }
}

/// `draft` with `text` after it, a space between unless the draft ends in one or is empty.
fn added(draft: &str, text: &str) -> String {
    if draft.is_empty() || draft.ends_with(char::is_whitespace) { format!("{draft}{text}") } else { format!("{draft} {text}") }
}

#[cfg(test)]
mod tests {
    use super::{Mode, TeamSend, added, busy, composer_mode, hint, placeholder, remote_control_on, remote_control_said, said_in_chat, stop_beside_steer, team_send, terminal_only_said, text_view_focused, typed_text, working};
    use gpui_kit::{KeyContext, Keystroke, Modifiers};
    use crate::explorer::toast::Notice;
    use agents::Summary;
    use chat::{Action, Ask, Block, End, Message, Question, Session, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};

    #[test]
    fn added_text_follows_the_draft_after_a_space() {
        assert_eq!(added("", "@a.rs "), "@a.rs ");
        assert_eq!(added("look at", "@a.rs "), "look at @a.rs ");
        assert_eq!(added("look at ", "@a.rs "), "look at @a.rs ");
    }

    fn ask(questions: Vec<Question>) -> Ask {
        Ask { request_id: "r".into(), tool: ToolKind::Shell { command: "ls".into(), description: String::new() }, tool_name: "Bash".into(), questions }
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
        let stopped = session(vec![turn(Some(End::Failed { error: "Interrupted".into(), duration_ms: 0 }))]);
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
        assert_eq!(hint(Mode::Prompt, busy, false, true, true, false), None, "no esc to stop");
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
        assert_eq!(hint(Mode::Prompt, false, false, true, true, false), None);
        assert_eq!(hint(Mode::Prompt, true, false, true, true, false), Some("Working · esc to stop"));
        assert_eq!(hint(Mode::Prompt, true, false, false, true, false), Some("⏎ send into this turn · ⌘⏎ queue for after"));
        assert_eq!(hint(Mode::Prompt, true, false, false, false, false), Some("Working · esc to stop"), "an agent that can't take a send mid-turn has nothing to send");
        assert_eq!(hint(Mode::Permission { answerable: true }, true, false, true, true, false), Some("Answer the request above to continue"));
        assert_eq!(hint(Mode::ViewOnly, false, false, true, true, false), None);
    }

    #[test]
    fn a_turn_being_stopped_reads_stopping_and_takes_sends_only_into_the_queue() {
        assert_eq!(hint(Mode::Prompt, true, true, true, true, false), Some("Stopping…"));
        assert_eq!(hint(Mode::Prompt, true, true, false, true, false), Some("Stopping…"), "Enter queues instead of steering");
        assert_eq!(hint(Mode::Prompt, false, true, true, true, false), None, "the turn ended");
        assert_eq!(placeholder(true, true, true), "Queue a follow-up…");
        assert_eq!(placeholder(true, false, true), "Send a follow-up into this turn…");
        assert!(stop_beside_steer(Action::Steer, true));
        assert!(!stop_beside_steer(Action::Queue, true), "the stop is already on its way");
        assert!(!stop_beside_steer(Action::Steer, false));
    }

    #[test]
    fn a_team_prompt_asks_for_a_plan_only_from_an_idle_agent_and_never_for_a_command() {
        assert_eq!(team_send(true, false, false), TeamSend::On);
        assert_eq!(team_send(true, false, true), TeamSend::Wait, "the plan starts a turn of its own");
        assert_eq!(team_send(true, true, false), TeamSend::Off, "a command goes as typed");
        assert_eq!(team_send(false, false, true), TeamSend::Off);
    }

    #[test]
    fn the_footer_says_a_team_prompt_waits_for_the_turn_to_end() {
        assert_eq!(hint(Mode::Prompt, true, false, false, true, true), Some("Team waits for this turn to end · esc to stop"));
        assert_eq!(hint(Mode::Prompt, false, false, false, true, true), None, "an idle agent takes it now");
        assert_eq!(hint(Mode::Prompt, true, true, false, true, true), Some("Stopping…"));
    }

    #[test]
    fn a_terminal_only_command_points_at_the_terminal_only_when_the_agent_has_its_tui() {
        assert_eq!(terminal_only_said("/cost", "claude", false), "/cost runs only in Claude's terminal. Run it there.");
        assert_eq!(terminal_only_said("/cost", "claude", true), "/cost isn't available in the chat.");
    }

    #[test]
    fn the_footer_of_the_chat_that_sent_a_terminal_command_says_where_to_run_it() {
        let said = Notice::InChat { agent: "a1".into(), icon: "terminal", text: "/cost opens a panel in Claude's terminal. Run it there.".into() };
        assert_eq!(said_in_chat(Some(&said), "a1"), Some(("terminal", "/cost opens a panel in Claude's terminal. Run it there.")));
        assert_eq!(said_in_chat(Some(&said), "a2"), None, "another chat's footer keeps its hint");
        assert_eq!(said_in_chat(Some(&Notice::Copied("a.rs".into())), "a1"), None);
        assert_eq!(said_in_chat(None, "a1"), None);
    }

    #[test]
    fn remote_control_says_its_link_is_copied_or_that_it_is_off() {
        assert_eq!(remote_control_said("https://claude.ai/code/session_1"), "Remote Control is on. Its link is copied.");
        assert_eq!(remote_control_said(""), "Remote Control is off.");
    }

    #[test]
    fn remote_control_turns_on_bare_or_with_on_off_with_off_and_takes_nothing_else() {
        assert_eq!([remote_control_on(""), remote_control_on("on"), remote_control_on("off")], [Some(true), Some(true), Some(false)]);
        assert_eq!(remote_control_on("of"), None, "a typo turns nothing on");
    }

    #[test]
    fn an_observe_only_agent_gets_no_composer() {
        assert_eq!(composer_mode(true, None), Mode::ViewOnly);
        assert_eq!(composer_mode(false, None), Mode::Prompt);
        assert_eq!(composer_mode(true, Some(&ask(vec![]))), Mode::Permission { answerable: false });
        assert_eq!(composer_mode(false, Some(&ask(vec![Question::default()]))), Mode::Question { answerable: true });
    }

    #[test]
    fn shortcuts_and_control_keys_type_no_text() {
        let key = |name: &str, key_char: Option<&str>, modifiers: Modifiers| typed_text(&Keystroke { modifiers, key: name.into(), key_char: key_char.map(Into::into) });
        assert_eq!(key("a", Some("a"), Modifiers::default()).as_deref(), Some("a"));
        assert_eq!(key("a", Some("A"), Modifiers::shift()).as_deref(), Some("A"));
        assert_eq!(key("s", Some("ß"), Modifiers::alt()).as_deref(), Some("ß"));
        assert_eq!(key("v", None, Modifiers::command()), None);
        assert_eq!(key("c", Some("c"), Modifiers::control()), None);
        assert_eq!(key("enter", Some("\n"), Modifiers::default()), None);
        assert_eq!(key("tab", Some("\t"), Modifiers::default()), None);
        assert_eq!(key("escape", None, Modifiers::default()), None);
    }

    #[test]
    fn a_focused_reply_leaves_typing_free_but_an_input_holds_it() {
        let stack = |names: &[&str]| names.iter().map(|n| KeyContext::parse(n).unwrap()).collect::<Vec<_>>();
        assert!(text_view_focused(&stack(&["Desktop", "TextView"])));
        assert!(!text_view_focused(&stack(&["Desktop", "Input mode=editor"])));
        assert!(!text_view_focused(&stack(&["TextView", "Terminal"])));
        assert!(!text_view_focused(&[]));
    }
}
