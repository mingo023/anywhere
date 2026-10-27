use super::fold::{self, Live};
use super::jump::{self, Glide};
use super::pin::{self, Pin};
use super::markdown::{self, Highlights};
use super::reveal::{self, Reveal};
use super::tool_row::Tools;
use super::working::{self, format_elapsed};
use super::{end_row, meta, phase, question_marker, shared_card, sticky, sub_agent, todos, user_bubble};
use crate::creating;
use crate::desktop::Desktop;
use crate::util::now_ms;
use chat::{Block, Command, Delivery, End, Message, Row, Session, ToolKind, ToolStatus, Transcript, Turn};
use gpui_kit::*;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use theme::*;

/// The transcript's reading width.
pub(super) const WIDTH: f32 = 720.;
/// How long a row shown from the background tray stays lit.
const FLASH: Duration = Duration::from_millis(900);
/// A sent prompt fades in where it's held, from just below.
const SENT: creating::Part = (0., 12.);

/// A reading width held between 560 and 1200, in 16px steps.
pub(super) fn content_width(width: f32) -> f32 {
    let width = if width.is_finite() { width.clamp(560., 1200.) } else { WIDTH };
    560. + ((width - 560.) / 16.).round() * 16.
}

/// One agent's chat as shown: its folded session and the list laying out its rows.
pub(crate) struct View {
    pub(crate) session: Session,
    pub(crate) list: ListState,
    pub(crate) tools: Entity<Tools>,
    _tools: Subscription,
    jump: bool,
    glide: Option<(Instant, Glide)>,
    /// The prompt just sent, held at the top while its reply grows.
    pin: Option<Pin>,
    /// When the user last sent a prompt: its row rises into the chat.
    sent: Option<Instant>,
    /// The row just shown from the background tray, and when: it flashes.
    flash: Option<(String, Instant)>,
    /// Whether the user's last scroll left the list at its end, until the next render reads it.
    scrolled: Rc<Cell<Option<bool>>>,
    /// The fold that last settled, and when: its label fades in.
    settled: Option<(String, Instant)>,
    copied: Option<(String, Task<()>)>,
    expanded: HashSet<String>,
    /// The row last shown more or less of, and when.
    toggled: Option<(String, Instant)>,
    code: Highlights,
    /// Messages still streaming onto the screen, by id.
    reveals: HashMap<String, Reveal>,
    /// The prompt row whose card over the top is opened in full.
    pub(crate) sticky_open: Option<String>,
    /// Whether the chat has shown what it opened with; text after that streams in.
    primed: bool,
}

impl View {
    fn new(agent: &str, cx: &mut Context<Desktop>) -> Self {
        let list = ListState::new(0, ListAlignment::Bottom, px(320.));
        list.set_follow_mode(FollowMode::Tail);
        let scrolled = Rc::new(Cell::new(None));
        list.set_scroll_handler({
            let scrolled = scrolled.clone();
            move |e, _, _| scrolled.set(Some(!e.is_scrolled))
        });
        let tools = cx.new(|_| Tools::default());
        let agent = agent.to_string();
        let _tools = cx.observe(&tools, move |d, _, _| d.chats.views.get(&agent).into_iter().for_each(View::remeasure_under_pin));
        Self { session: Session::default(), list, tools, _tools, jump: false, glide: None, pin: None, sent: None, flash: None, scrolled, settled: None, copied: None, expanded: HashSet::new(), toggled: None, code: Highlights::default(), reveals: HashMap::new(), sticky_open: None, primed: false }
    }

    /// How far the list sits above its end.
    fn distance(&self) -> f32 {
        f32::from(self.list.max_offset_for_scrollbar().y + self.list.scroll_px_offset_for_scrollbar().y).max(0.)
    }

    /// The row at the top of the viewport and how far into it. At its end the list keeps no top row, so it's found
    /// from the end, by the heights last laid out: render runs before the list remeasures the rows that changed.
    pub(super) fn top_row(&self) -> (usize, f32) {
        let (top, count) = (self.list.logical_scroll_top(), self.list.item_count());
        if top.item_ix < count {
            return (top.item_ix, f32::from(top.offset_in_item));
        }
        sticky::top_at_end(count, self.shown_at_end(), |ix| f32::from(self.list.laid_out_height_from(ix)))
    }

    /// How far under the viewport's top row `ix` starts; None while that's unmeasured.
    pub(super) fn row_top(&self, ix: usize) -> Option<f32> {
        if self.list.logical_scroll_top().item_ix < self.list.item_count() {
            // The list paints its rows under its top padding, which `bounds_for_item` leaves out.
            return self.list.bounds_for_item(ix).map(|b| f32::from(b.top() - self.list.viewport_bounds().top()) + pin::TOP);
        }
        Some(self.shown_at_end() - f32::from(self.list.laid_out_height_from(ix)))
    }

    /// The height the rows show in at the list's end, over the room padded under them.
    fn shown_at_end(&self) -> f32 {
        f32::from(self.list.viewport_bounds().size.height) - pin::BOTTOM - self.pin.as_ref().map_or(0., |p| p.spacer)
    }

    /// Sizes the room that holds the sent prompt a quarter down and keeps the list on it, gliding it there, until its
    /// reply fills what's under it.
    fn hold_pin(&mut self, glide: bool, window: &mut Window) {
        let scrolled = self.scrolled.take();
        let Some(pin) = &mut self.pin else { return };
        if let Some(at_end) = scrolled.or_else(|| (!pin.unmeasured() && !pin.anchored && self.list.is_following_tail()).then_some(true)) {
            pin.moved(at_end);
        }
        let Some(ix) = pin.row(&self.session.rows) else {
            self.pin = None;
            return;
        };
        let viewport = f32::from(self.list.viewport_bounds().size.height);
        let laid_out = self.list.laid_out(ix).then(|| f32::from(self.list.laid_out_height_from(ix)));
        let above = f32::from(self.list.laid_out_height_from(0) - self.list.laid_out_height_from(ix));
        let now = Instant::now();
        if !pin.step(viewport, above, pin.below(self.list.height_from(ix).map(f32::from), laid_out), glide.then_some(now)) {
            // On the prompt the list follows the tail from here; scrolled away, it stays put until back at the end.
            let (anchored, top) = (pin.anchored, self.list.logical_scroll_top());
            self.pin = None;
            self.list.set_follow_mode(FollowMode::Tail);
            if !anchored {
                self.list.scroll_to(top);
            }
            return;
        }
        if pin.unmeasured() {
            window.request_animation_frame();
        } else if pin.anchored {
            // Held by the item rather than by following the end: a row growing under the prompt can't push it up for a frame
            // before the room shrinks.
            self.list.set_follow_mode(FollowMode::Normal);
            self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) });
            let (top, gliding) = pin.top(now);
            self.list.scroll_by(px(pin::TOP - top));
            if gliding {
                window.request_animation_frame();
            }
        }
    }

    /// A row changed height where the list wouldn't know: the held prompt's room can't come from what was measured.
    fn remeasure_under_pin(&self) {
        if let Some(ix) = self.pin.as_ref().and_then(|p| p.row(&self.session.rows)) {
            self.list.remeasure_items(ix..self.list.item_count());
        }
    }

    /// Holds the prompt just sent at the top once its row shows.
    pub(super) fn pin(&mut self) {
        self.pin = Some(Pin::new());
        self.sent = Some(Instant::now());
        self.list.set_follow_mode(FollowMode::Tail);
    }

    /// A steer went out: the prompt held at the top, if any, stays held, and the steer shows at the tail.
    pub(super) fn steered(&mut self) {
        self.sent = Some(Instant::now());
        if let Some(pin) = &mut self.pin {
            pin.steered();
        }
    }

    fn copied(&self, key: &str) -> bool {
        self.copied.as_ref().is_some_and(|(k, _)| k == key)
    }

    /// Refolds the session when its timeline moved and lays out what changed; returns the queued sends it let go.
    fn refresh(&mut self, agent: &agents::Summary, revision: u64, items: &[agents::Item]) -> Vec<(u64, String)> {
        let before: HashMap<String, usize> = latest_texts(&self.session.transcript).map(|m| (m.id.clone(), m.text.len())).collect();
        let tail = self.session.live_tail().map(|f| self.session.transcript.turns[f].id.clone());
        if let Some((changed, range, count)) = self.session.update(chat::for_provider(&agent.provider), revision, items, super::composer::working(agent), now_ms()) {
            if let Some(turn) = chat::settled_tail(tail.as_deref(), &self.session.transcript) {
                self.settled = Some((format!("{turn}#fold"), Instant::now()));
            }
            self.pace(&before, Instant::now());
            self.list.remeasure_items(changed.clone());
            self.list.splice(range, count);
            // Rows kept above the change can show or hide as their reply settles; the list measured them as they were.
            let rows = &self.session.rows[..changed.start.min(self.session.rows.len())];
            if let Some(fold) = rows.iter().rposition(|r| matches!(r, Row::Fold { .. })) {
                self.list.remeasure_items(fold..changed.start);
            }
            self.code.retain(|row| self.session.ids.contains(row));
        }
        self.session.outgoing.released()
    }

    /// Streams in the text this update added to the latest turns; what the chat opened with shows at once.
    fn pace(&mut self, before: &HashMap<String, usize>, now: Instant) {
        if !self.primed {
            self.primed = !self.session.transcript.turns.is_empty();
            return;
        }
        stream_in(&mut self.reveals, before, &self.session.transcript, now);
    }
}

/// Starts or extends a reveal for each latest message longer than it was `before`, and forgets finished ones.
fn stream_in(reveals: &mut HashMap<String, Reveal>, before: &HashMap<String, usize>, t: &Transcript, now: Instant) {
    reveals.retain(|_, r| r.moving(now));
    for m in latest_texts(t) {
        let was = before.get(&m.id).copied().unwrap_or(0);
        if m.text.len() > was {
            reveals.entry(m.id.clone()).or_insert_with(|| Reveal::shown(was, now)).arrive(m.text.len(), now);
        }
    }
}

/// The messages of the last two turns, where new text lands.
fn latest_texts(t: &Transcript) -> impl Iterator<Item = &Message> {
    t.turns.iter().rev().take(2).flat_map(|t| &t.blocks).filter_map(text_of)
}

fn text_of(b: &Block) -> Option<&Message> {
    match b {
        Block::Text(m) | Block::Plan(m) => Some(m),
        _ => None,
    }
}

/// Whether `turn` is the one the agent is still working on.
fn live(t: &Transcript, turn: usize) -> bool {
    turn + 1 == t.turns.len() && t.turns[turn].end.is_none()
}

pub(super) fn block_at(b: &Block) -> Option<i64> {
    match b {
        Block::Text(m) | Block::Thinking(m) | Block::Plan(m) | Block::Notice { message: m, .. } => Some(m.at),
        Block::Tool(t) => Some(t.at),
        _ => None,
    }
}

/// When the agent began the turn: its prompt, else its first timed block.
fn started(turn: &Turn) -> Option<i64> {
    turn.prompt.as_ref().map(|p| p.at).or_else(|| turn.blocks.iter().find_map(block_at))
}

/// What a live fold says: stopping once the user stopped it, waiting on the user, thinking before anything else, else
/// working for `now` less the start.
fn live_state(t: &Transcript, turns: Range<usize>, ask: Option<bool>, stopping: bool, now: i64) -> Live {
    if stopping {
        return Live::Stopping;
    }
    match ask {
        Some(true) => return Live::Answer,
        Some(false) => return Live::Approval,
        None => {}
    }
    let blocks = || t.turns[turns.clone()].iter().flat_map(|turn| &turn.blocks);
    let background = chat::in_background(&t.turns).count();
    if background == 0 && blocks().all(|b| matches!(b, Block::Thinking(_))) {
        return Live::Thinking;
    }
    let secs = started(&t.turns[turns.start]).map_or(0, |at| (now - at).max(0) / 1000);
    Live::Working { secs, background }
}

/// How long a settled reply took: its turns' own count, else from its start to its last move.
fn took(t: &Transcript, turns: Range<usize>) -> u64 {
    if let Some((ms, _)) = chat::worked(t, turns.clone()) {
        return ms;
    }
    let last = t.turns[turns.clone()].iter().flat_map(|turn| &turn.blocks).filter_map(block_at).max();
    match (started(&t.turns[turns.start]), last) {
        (Some(a), Some(b)) => (b - a).max(0) as u64,
        _ => 0,
    }
}

/// Whether a phase is its turn's newest work: the turn goes on and nothing but todos came after it.
fn phase_live(t: &Transcript, turn: usize, blocks: &Range<usize>) -> bool {
    live(t, turn) && t.turns[turn].blocks[blocks.end..].iter().all(|b| matches!(b, Block::Todos { .. }))
}

/// When what follows a phase began: its next timed block, else the next turn's start.
fn next_at(t: &Transcript, turn: usize, end: usize) -> Option<i64> {
    t.turns[turn].blocks[end..].iter().find_map(block_at).or_else(|| t.turns.get(turn + 1).and_then(started))
}

/// A reply's answer: the text each of its turns ends on, after its work.
fn answer(t: &Transcript, turns: Range<usize>) -> String {
    let tail = |turn: &Turn| turn.blocks.iter().rposition(chat::is_work).map_or(0, |b| b + 1);
    let texts = t.turns[turns].iter().flat_map(|turn| turn.blocks[tail(turn)..].iter().filter_map(|b| if let Block::Text(m) | Block::Plan(m) = b { Some(m.text.as_str()) } else { None }));
    texts.collect::<Vec<_>>().join("\n\n")
}

/// Whether `row` hangs off its fold's rail: a phase, or the text between phases while the fold is open. The live tail's
/// fold shows no header, its line having moved to the bottom, so nothing hangs off it.
fn in_fold(t: &Transcript, live_tail: Option<usize>, row: &Row, fold_open: bool) -> bool {
    let headed = row.turn().and_then(|turn| chat::fold_of(t, turn)).is_some_and(|fold| live_tail != Some(fold));
    headed && (matches!(row, Row::Phase { .. }) || (fold_open && narration(t, row)))
}

fn narration(t: &Transcript, row: &Row) -> bool {
    matches!(row, Row::Text { turn, block, .. } if chat::narrates(&t.turns[*turn], *block))
}

/// Whether `a` and `b` are markdown blocks of one message.
fn same_message(a: &Row, b: &Row) -> bool {
    matches!((a, b), (Row::Text { turn: x, block: i, .. }, Row::Text { turn: y, block: j, .. }) if (x, i) == (y, j))
}

/// Whether a row under a fold shows: the work does while the fold is open; a phase holding a failed sub-agent always does.
fn shown(t: &Transcript, row: &Row, fold_open: bool) -> bool {
    match row {
        Row::Phase { turn, blocks } => fold_open || t.turns[*turn].blocks[blocks.clone()].iter().any(failed_agent),
        Row::Tasks { .. } => fold_open,
        _ => true,
    }
}

fn failed_agent(b: &Block) -> bool {
    matches!(b, Block::Tool(c) if c.status == ToolStatus::Failed && matches!(c.kind, ToolKind::Agent(_)))
}

/// Where the call with use id `id` sits: its turn and block.
fn find_call(t: &Transcript, id: &str) -> Option<(usize, usize)> {
    t.turns.iter().enumerate().find_map(|(ix, turn)| Some((ix, turn.blocks.iter().position(|b| matches!(b, Block::Tool(c) if c.id == id))?)))
}

/// "Audit deps is done", "Background command `cargo test` failed", "3 subagents are done"; backticks mark code.
fn notice_label(calls: &[&chat::ToolCall]) -> String {
    match calls {
        [chat::ToolCall { kind: ToolKind::Agent(a), .. }] => format!("{} is done", sub_agent::name(a)),
        [c @ chat::ToolCall { kind: ToolKind::Shell { command, .. }, .. }] => {
            let command = command.lines().next().unwrap_or_default();
            match (c.name.as_str(), c.status) {
                ("Monitor", _) => format!("Monitor `{command}` ended"),
                (_, ToolStatus::Failed) => format!("Background command `{command}` failed"),
                _ => format!("Background command `{command}` is done"),
            }
        }
        _ if all_agents(calls) => format!("{} subagents are done", calls.len()),
        _ => format!("{} background tasks are done", calls.len()),
    }
}

/// A label's text before, inside and after its code span; the outer backticks bound it, so a command may hold its own.
fn code_span(label: &str) -> (&str, Option<&str>, &str) {
    match label.split_once('`').and_then(|(before, rest)| Some((before, rest.rsplit_once('`')?))) {
        Some((before, (code, after))) => (before, Some(code), after),
        None => (label, None, ""),
    }
}

fn all_agents(calls: &[&chat::ToolCall]) -> bool {
    calls.iter().all(|c| matches!(c.kind, ToolKind::Agent(_)))
}

fn message(t: &Transcript, turn: usize, block: usize) -> Option<&Message> {
    t.turns.get(turn).and_then(|t| t.blocks.get(block)).and_then(text_of)
}

fn copy(agent: &str, key: String, text: String, cx: &mut Context<Desktop>) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let agent = agent.to_string();
    cx.listener(move |this, _, _, cx| this.chat_copy(&agent, key.clone(), text.clone(), cx))
}

/// A prompt's bubble with its copy and time, or `under` in their place. A send shows as its echo will, so the echo
/// replacing it doesn't show.
#[allow(clippy::too_many_arguments)]
fn prompt_bubble(view: &View, agent: &str, id: &str, text: &str, at: i64, dim: bool, under: Option<Div>, width: f32, window: &mut Window, cx: &mut Context<Desktop>) -> Div {
    let lines = user_bubble::lines(text, width, window);
    let toggle = user_bubble::needs_collapse(lines).then(|| {
        let toggled = view.toggled.as_ref().filter(|(row, _)| row == id).map(|(_, at)| *at);
        let (agent, row) = (agent.to_string(), id.to_string());
        (user_bubble::Fold { lines, toggled }, cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_toggle_expanded(&agent, &row, cx)))
    });
    let bubble = user_bubble::bubble(id, text.to_string(), width, dim, view.expanded.contains(id), toggle);
    let under = under.unwrap_or_else(|| {
        let key = format!("prompt:{id}");
        user_bubble::actions(id, view.copied(&key), copy(agent, key, text.to_string(), cx), Some(at))
    });
    div().flex().flex_col().items_end().gap(px(4.)).child(bubble).child(under)
}

fn toggle(agent: &str, id: &str, default: bool, cx: &mut Context<Desktop>) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let (agent, id) = (agent.to_string(), id.to_string());
    cx.listener(move |this, _, _, cx| this.chat_toggle(&agent, &id, default, cx))
}

impl Desktop {
    /// Refolds every chat as its timeline moves, on screen or not, and sends each one's next queued send once its agent goes
    /// idle. A chat away from the screen streams its text in as it arrives, so it isn't let out again on return.
    pub(crate) fn refresh_chats(&mut self) {
        let mut released = Vec::new();
        for (id, view) in self.chats.views.iter_mut() {
            let Some(agent) = self.agents.get(id) else { continue };
            let items = self.agents.timelines.get(id).map_or(&[][..], Vec::as_slice);
            released.extend(view.refresh(agent, self.agents.revision(id), items).into_iter().map(|(send, text)| (id.clone(), send, text)));
        }
        for (id, send, text) in released {
            if let (Some(request), Some(v)) = (self.outbox.prompt(&id, &text), self.chats.views.get_mut(&id)) {
                v.session.outgoing.sent_as(send, request);
            }
        }
    }

    /// `terminal`'s agent's transcript, refolded when its timeline moved.
    pub(super) fn transcript(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let Some(agent) = self.summary(terminal) else { return div().flex_1() };
        let (id, agent) = (agent.id.clone(), agent.clone());
        let items = self.agents.timelines.get(&id).map_or(&[][..], Vec::as_slice);
        let creates = &self.creates;
        let view = self.chats.views.entry(id.clone()).or_insert_with(|| {
            let mut v = View::new(&id, cx);
            // A chat opened from its create shows the prompt pocketd sent before the transcript holds it.
            if let Some(prompt) = creates.unsent(&id, terminal, Instant::now()) {
                v.session.outgoing.push(prompt.to_string(), false, now_ms());
            }
            v
        });
        for (send, text) in view.refresh(&agent, self.agents.revision(&id), items) {
            if let Some(request) = self.outbox.prompt(&id, &text) {
                view.session.outgoing.sent_as(send, request);
            }
        }
        if view.list.item_count() == 0 {
            return div().flex_1().flex().items_center().justify_center().text_size(px(14.)).text_color(TEXT_3).child("Send a message to start.");
        }
        view.hold_pin(!cx.reduce_motion(), window);
        view.jump = view.glide.is_none() && jump::jump_visibility(view.jump, view.distance());
        let spacer = view.pin.as_ref().map_or(0., |p| p.spacer);
        let rows = {
            let id = id.clone();
            list(view.list.clone(), cx.processor(move |this, ix, window, cx| this.chat_row(&id, ix, window, cx))).size_full().pt(px(pin::TOP)).pb(px(pin::BOTTOM + spacer))
        };
        let pill = view.jump.then(|| {
            let id = id.clone();
            jump::pill(cx.listener(move |this, _, window, cx| this.chat_jump_to_bottom(&id, window, cx)))
        });
        let sticky = self.chat_sticky(&id, window, cx);
        let cover = sticky.as_ref().map_or(0., |(_, cover)| *cover);
        div().relative().flex_1().min_h_0().child(sticky::below(cover, rows)).children(sticky.map(|(card, _)| card)).children(pill)
    }

    /// Opens a page in the browser the settings pick, or a file in the preview.
    pub(super) fn chat_open(&mut self, agent: &str, link: &str, cwd: &str, window: &mut Window, cx: &mut Context<Self>) {
        match markdown::target(link, cwd) {
            markdown::Target::Web(url) if self.store.terminal.system_browser => cx.open_url(&url),
            markdown::Target::Web(url) => self.open_browser(Some(url), window, cx),
            markdown::Target::File(path) => self.chat_open_file(agent, path, cwd, window.mouse_position(), cx),
        }
    }

    /// Opens or closes a fold, phase or sub-agent, and has the list measure again what that shows or hides.
    pub(crate) fn chat_toggle(&mut self, agent: &str, id: &str, default: bool, cx: &mut Context<Self>) {
        let Some(view) = self.chats.views.get_mut(agent) else { return };
        let s = &mut view.session;
        s.toggle(id, default);
        if let Some(ix) = s.ids.iter().position(|i| i == id) {
            let end = match s.rows[ix] {
                Row::Fold { .. } => (ix + 1..s.rows.len()).find(|&i| matches!(s.rows[i], Row::Prompt { .. } | Row::Notice { .. } | Row::Fold { .. } | Row::Live { .. } | Row::Pending(_) | Row::Working)).unwrap_or(s.rows.len()),
                _ => ix + 1,
            };
            view.list.remeasure_items(ix..end);
        }
        cx.notify();
    }

    /// Scrolls `agent`'s chat to its row `ix`, opening the fold over it if it shows shut.
    pub(crate) fn chat_reveal(&mut self, agent: &str, ix: usize, cx: &mut Context<Self>) {
        let Some(view) = self.chats.views.get(agent) else { return };
        view.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) });
        if let Some(f) = view.session.shut_fold_over(ix) {
            self.chat_toggle(agent, &f, false, cx);
        }
    }

    /// Scrolls `agent`'s chat to the row showing block `block` of turn `turn`, opened, and flashes it.
    pub(crate) fn chat_show_block(&mut self, agent: &str, turn: usize, block: usize, cx: &mut Context<Self>) {
        let Some(view) = self.chats.views.get_mut(agent) else { return };
        let s = &view.session;
        let Some(ix) = chat::row_of(&s.rows, turn, block) else { return };
        let id = s.ids[ix].clone();
        let shut = s.shut_phase(ix);
        // A prompt held at the top, or a glide to the end, would scroll the list off the row.
        (view.pin, view.glide) = (None, None);
        view.flash = Some((id.clone(), Instant::now()));
        self.chat_reveal(agent, ix, cx);
        if shut {
            self.chat_toggle(agent, &id, false, cx);
        }
        cx.notify();
    }

    /// Scrolls `agent`'s chat to its prompt row `ix`, as a click on the prompt stuck over the top does.
    pub(super) fn chat_jump_to_prompt(&mut self, agent: &str, ix: usize, cx: &mut Context<Self>) {
        let Some(view) = self.chats.views.get_mut(agent) else { return };
        // A prompt held at the top, or a glide to the end, would scroll the list off the prompt.
        (view.pin, view.glide) = (None, None);
        view.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) });
        cx.notify();
    }

    /// Scrolls the shown chat a prompt up or down from the exchange at its top.
    pub(crate) fn chat_step_prompt(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some(agent) = self.session().map(str::to_string) else { return };
        let Some(view) = self.chats.views.get(&agent) else { return };
        if let Some(ix) = chat::step_prompt(&view.session.rows, view.top_row().0, down) {
            self.chat_jump_to_prompt(&agent, ix, cx);
        }
    }

    /// Scrolls the shown chat to `rows` rows before its end; the list knows only measured heights, so a pixel scroll stops short.
    pub(crate) fn scroll_chat_back(&mut self, rows: usize, cx: &mut Context<Self>) {
        if let Some(view) = self.session().and_then(|id| self.chats.views.get(id)) {
            view.list.scroll_to(ListOffset { item_ix: view.list.item_count().saturating_sub(rows), offset_in_item: px(0.) });
            cx.notify();
        }
    }

    pub(crate) fn chat_jump_to_bottom(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.chats.views.get_mut(agent) else { return };
        view.jump = false;
        match Glide::new(view.distance(), f32::from(view.list.viewport_bounds().size.height)) {
            Some(glide) => {
                view.glide = Some((Instant::now(), glide));
                self.chat_glide(agent.to_string(), window, cx);
            }
            None => view.list.set_follow_mode(FollowMode::Tail),
        }
        cx.notify();
    }

    /// One frame of a glide to the end; it follows the tail once there.
    fn chat_glide(&mut self, agent: String, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.chats.views.get_mut(&agent) else { return };
        let Some((start, glide)) = &mut view.glide else { return };
        let (by, done) = glide.step(start.elapsed());
        view.list.scroll_by(px(by));
        if done {
            view.glide = None;
            view.list.set_follow_mode(FollowMode::Tail);
        } else {
            cx.on_next_frame(window, move |this, window, cx| this.chat_glide(agent, window, cx));
        }
        cx.notify();
    }

    pub(crate) fn chat_copy(&mut self, agent: &str, key: String, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        let clear = {
            let (agent, key) = (agent.to_string(), key.clone());
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(1200)).await;
                this.update(cx, |d, cx| {
                    if let Some(view) = d.chats.views.get_mut(&agent).filter(|v| v.copied(&key)) {
                        view.copied = None;
                        cx.notify();
                    }
                })
                .ok();
            })
        };
        if let Some(view) = self.chats.views.get_mut(agent) {
            view.copied = Some((key, clear));
        }
        cx.notify();
    }

    pub(crate) fn chat_toggle_expanded(&mut self, agent: &str, row: &str, cx: &mut Context<Self>) {
        let Some(view) = self.chats.views.get_mut(agent) else { return };
        if !view.expanded.remove(row) {
            view.expanded.insert(row.to_string());
        }
        view.toggled = Some((row.to_string(), Instant::now()));
        view.remeasure_under_pin();
        cx.notify();
    }

    fn chat_retry(&mut self, agent: &str, send: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.chats.views.get_mut(agent).and_then(|v| v.session.outgoing.retry(send)) else { return };
        self.chat_take_back(agent, &text, window, cx);
    }

    fn chat_highlight(&mut self, agent: &str, row: &str, lang: &str, code: &str, cx: &mut Context<Self>) {
        let (agent, row, lang, code, dark) = (agent.to_string(), row.to_string(), lang.to_string(), code.to_string(), is_dark());
        cx.spawn(async move |this, cx| {
            let len = code.len();
            let styles = cx.background_spawn(async move { markdown::highlight(&lang, &code, dark) }).await;
            this.update(cx, |d, cx| {
                if let Some(view) = d.chats.views.get_mut(&agent) {
                    view.code.set(&row, len, dark, styles);
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    fn chat_row(&mut self, agent: &str, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (provider, model) = self.agents.get(agent).map(|a| (a.provider.clone(), agents::model_label(a))).unwrap_or_default();
        let cwd = self.agents.get(agent).map(|a| a.cwd.clone()).unwrap_or_default();
        let ask = self.agents.pending.iter().find(|p| p.agent_id == agent).map(|p| !chat::for_provider(&provider).ask(p).questions.is_empty());
        let send_now = self.chat_caps(&provider).send_now;
        let open: markdown::Open = {
            let (this, id, cwd) = (cx.weak_entity(), agent.to_string(), cwd.clone());
            Arc::new(move |link, window, cx| this.update(cx, |d, cx| d.chat_open(&id, link, &cwd, window, cx)).unwrap_or_default())
        };
        let Some(view) = self.chats.views.get_mut(agent) else { return div().into_any_element() };
        // Field by field, so the code highlights stay free to borrow mutably.
        let s = &view.session;
        let (Some(row), Some(id)) = (s.rows.get(ix).cloned(), s.ids.get(ix).cloned()) else { return div().into_any_element() };
        let t = &s.transcript;
        let who = fold::who(&provider);
        let fold = row.turn().and_then(|turn| chat::fold_of(t, turn));
        let fold_id = fold.map(|f| format!("{}#fold", t.turns[f].id));
        let fold_live = fold.is_some_and(|f| chat::at_work_on(t, chat::folded(t, f)));
        let fold_open = fold_id.as_ref().is_none_or(|f| fold_live || s.open(f, false));
        if !shown(t, &row, fold_open) {
            return div().id(SharedString::from(id)).into_any_element();
        }
        let rail = in_fold(t, s.live_tail(), &row, fold_open);
        let last_on_rail = rail && !s.rows[ix + 1..].iter().find(|next| !same_message(&row, next)).is_some_and(|next| in_fold(t, s.live_tail(), next, fold_open));
        let elbow = !ix.checked_sub(1).and_then(|i| s.rows.get(i)).is_some_and(|prev| same_message(prev, &row));
        let gap = if rail { 0. } else { chat::top_gap(ix.checked_sub(1).and_then(|i| s.rows.get(i)), &row) };
        let width = content_width(WIDTH);

        let first = s.rows.iter().position(|r| matches!(r, Row::Prompt { .. } | Row::Pending(_))) == Some(ix);
        let part = match row {
            Row::Prompt { .. } | Row::Pending(_) if first => Some(creating::BUBBLE),
            Row::Working => Some(creating::THINKING),
            _ => None,
        };

        let mut highlight = None;
        let inner = match &row {
            Row::Prompt { turn } => match t.turns.get(*turn).and_then(|t| t.prompt.as_ref()) {
                Some(p) => prompt_bubble(view, agent, &id, &p.text, p.at, false, None, width, window, cx),
                None => div(),
            },
            Row::Fold { turn } => {
                let turns = chat::folded(t, *turn);
                if s.live_tail() == Some(*turn) {
                    div()
                } else {
                    let settled = fold::settled(&id, &provider, fold::settled_label(who, took(t, turns)), fold_open, toggle(agent, &id, false, cx));
                    match &view.settled {
                        Some((fold, at)) if *fold == id && at.elapsed() < reveal::FADE => {
                            div().child(settled.with_animation(SharedString::from(format!("settle-{id}")), Animation::new(reveal::FADE), |d, t| d.opacity(t)))
                        }
                        _ => settled,
                    }
                }
            }
            Row::Phase { turn, blocks: range } => {
                let blocks = &t.turns[*turn].blocks[range.clone()];
                let on_toggle = toggle(agent, &id, false, cx);
                phase::phase(&id, blocks, next_at(t, *turn, range.end), phase_live(t, *turn, range), s.open(&id, false), on_toggle, &open, &view.tools, cx)
            }
            Row::Notice { turn } => {
                let calls = chat::reported(t, *turn);
                let at = calls.first().and_then(|c| find_call(t, &c.id));
                let line = div()
                    .id(SharedString::from(format!("notice-{id}")))
                    .py(px(4.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .text_size(px(13.))
                    .text_color(TEXT_3)
                    .whitespace_nowrap()
                    .child(icon(if all_agents(&calls) { "agent-done" } else { "terminal" }, 14., TEXT_4))
                    .child({
                        let label = notice_label(&calls);
                        let (before, code, after) = code_span(&label);
                        div()
                            .min_w_0()
                            .flex()
                            .items_center()
                            .child(div().flex_none().child(before.to_string()))
                            .children(code.map(|c| div().min_w_0().truncate().font_family(code_font()).text_size(px(12.)).child(c.to_string())))
                            .child(div().flex_none().child(after.to_string()))
                    });
                match at {
                    Some((turn, block)) => {
                        let agent = agent.to_string();
                        div().child(line.cursor_pointer().hover(|s| s.text_color(TEXT_2)).on_click(cx.listener(move |this, _, _, cx| this.chat_show_block(&agent, turn, block, cx))))
                    }
                    None => div().child(line),
                }
            }
            Row::Note { turn, block } => match t.turns.get(*turn).and_then(|t| t.blocks.get(*block)) {
                Some(Block::Notice { message, error }) => end_row::note(&message.text, *error),
                _ => div(),
            },
            Row::Tasks { turn, block } => match t.turns.get(*turn).and_then(|t| t.blocks.get(*block)) {
                Some(Block::Todos { todos, .. }) => todos::card(&id, todos),
                _ => div(),
            },
            Row::Text { turn, block, range } => {
                let m = message(t, *turn, *block);
                let full = m.map_or("", |m| m.text.as_str());
                let now = Instant::now();
                let reveal = m.and_then(|m| view.reveals.get(&m.id)).filter(|r| r.moving(now));
                let whole = full.get(range.clone()).unwrap_or_default();
                // A code block shows whole once reached, so it's highlighted once rather than per word.
                let text = match reveal {
                    Some(_) if markdown::fence(whole).is_some() => whole,
                    Some(r) => full.get(range.start..range.end.min(r.cut(full, now))).unwrap_or_default(),
                    None => whole,
                };
                if reveal.is_some() {
                    window.request_animation_frame();
                    if text.is_empty() {
                        return div().id(SharedString::from(id)).into_any_element();
                    }
                }
                let body = match markdown::fence(text) {
                    Some((lang, code)) => {
                        let (styles, start) = view.code.get(&id, &code, is_dark());
                        if start {
                            highlight = Some((lang.to_string(), code.to_string()));
                        }
                        let block = div().child(markdown::code_block(SharedString::from(format!("code-{agent}-{id}")), lang, &code, styles));
                        let block = match reveal {
                            Some(_) => div().child(block.with_animation(SharedString::from(format!("code-in-{id}")), Animation::new(reveal::FADE), |b, t| b.opacity(t))),
                            None => block,
                        };
                        // An indented fence belongs to the list item above it.
                        if text.starts_with(' ') { div().pl(px(markdown::LIST_INDENT)).child(block) } else { block }
                    }
                    None => div().child(markdown::block(SharedString::from(format!("md-{agent}-{id}")), SharedString::from(text.to_string()), reveal.is_some() || live(t, *turn), open.clone())),
                };
                let plan = range.start == 0 && !text.trim_start().starts_with('#') && matches!(t.turns.get(*turn).and_then(|t| t.blocks.get(*block)), Some(Block::Plan(_)));
                div().flex().flex_col().gap(px(8.)).children(plan.then(todos::plan_header)).child(body)
            }
            Row::Shared { turn, block } => match t.turns.get(*turn).and_then(|t| t.blocks.get(*block)) {
                Some(Block::Tool(call)) => {
                    let (how, target) = shared_card::opening(call, &cwd);
                    let card = |on_click| shared_card::card(&id, call, how, target.as_deref(), width, on_click);
                    let opens = target.clone();
                    card(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        if let Some(target) = &opens {
                            this.chat_open_shared(target, how, cx);
                        }
                    }))
                }
                _ => div(),
            },
            Row::Question { turn, block } => match t.turns.get(*turn).and_then(|t| t.blocks.get(*block)) {
                Some(Block::Tool(call)) => {
                    let toggled = view.toggled.as_ref().filter(|(row, _)| *row == id).map(|(_, at)| *at);
                    let (agent, row) = (agent.to_string(), id.clone());
                    let on_click = cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_toggle_expanded(&agent, &row, cx));
                    question_marker::marker(&id, call, width, question_marker::Open { expanded: view.expanded.contains(&id), toggled, on_click }, window)
                }
                _ => div(),
            },
            Row::Compacted { turn, block } => match t.turns.get(*turn).and_then(|t| t.blocks.get(*block)) {
                Some(Block::Compacted { auto, .. }) => end_row::compacted(*auto),
                _ => div(),
            },
            Row::End { turn } => match t.turns.get(*turn).and_then(|t| t.end.as_ref()) {
                Some(End::Failed { error, .. }) if end_row::interrupted(error) => end_row::interrupted_notice(who),
                Some(End::Failed { error, .. }) => {
                    let key = format!("error:{id}");
                    end_row::error(&id, error.clone(), view.copied(&key), copy(agent, key.clone(), error.clone(), cx))
                }
                Some(End::Done { .. }) => match chat::reply(t, *turn).filter(|turns| !chat::at_work_on(t, turns.clone())) {
                    Some(turns) => {
                        let key = format!("reply:{id}");
                        let worked = chat::worked(t, turns.clone()).map(|(ms, _)| format_elapsed((ms / 1000) as i64));
                        let at = t.turns[turns.clone()].iter().flat_map(|turn| &turn.blocks).filter_map(block_at).max();
                        let retry = t.turns[turns.clone()].iter().find_map(|turn| turn.prompt.as_ref()).map(|p| {
                            let (agent, text) = (agent.to_string(), p.text.clone());
                            cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_command(&agent, Command::Send(text.clone()), cx))
                        });
                        meta::meta(&id, view.copied(&key), copy(agent, key.clone(), answer(t, turns), cx), retry, worked, at)
                    }
                    None => return div().id(SharedString::from(id)).into_any_element(),
                },
                None => div(),
            },
            Row::Live { turn } => fold::live(&id, &provider, fold::live_label(who, &model, &live_state(t, chat::folded(t, *turn), ask, s.outgoing.stopping(), now_ms()))),
            Row::Working => working::sending(),
            Row::Pending(i) => match s.outgoing.echoed().nth(*i) {
                Some((_, p)) => {
                    let retry = (p.delivery == Delivery::Failed).then(|| {
                        let (agent, send) = (agent.to_string(), p.id);
                        working::not_delivered(SharedString::from(format!("retry-{id}"))).on_click(cx.listener(move |this, _, window, cx| this.chat_retry(&agent, send, window, cx)))
                    });
                    // What waits for the agent's next step goes all at once, so only the last of it says so.
                    let waiting: Vec<u64> = s.outgoing.waiting().map(|w| w.id).collect();
                    let under = waiting.contains(&p.id).then(|| {
                        if waiting.last() != Some(&p.id) {
                            return div();
                        }
                        let action = if s.outgoing.sending_now() {
                            Some(div().child("Sending…").into_any_element())
                        } else {
                            send_now.then(|| {
                                let agent = agent.to_string();
                                user_bubble::send_now(&id).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_command(&agent, Command::SendNow, cx))).into_any_element()
                            })
                        };
                        user_bubble::waiting(who, waiting.len(), action)
                    });
                    let under = under.or_else(|| s.outgoing.after_turn().any(|c| c.id == p.id).then(|| user_bubble::after_turn(who)));
                    let dim = p.delivery == Delivery::Failed || under.is_some();
                    prompt_bubble(view, agent, &id, &p.text, p.sent_at, dim, under, width, window, cx).children(retry)
                }
                None => div(),
            },
        };
        let inner = if rail { phase::in_fold(last_on_rail, elbow, inner) } else { inner };
        let prompt = |r: &Row| matches!(r, Row::Prompt { .. } | Row::Pending(_));
        let rising = view.sent.and_then(|at| creating::rise(at.elapsed(), SENT, cx.reduce_motion())).filter(|_| prompt(&row) && !s.rows[ix + 1..].iter().any(prompt));
        let inner = match rising {
            Some(r) => {
                window.request_animation_frame();
                r.on(inner)
            }
            None => inner,
        };
        let inner = match &view.flash {
            Some((row, at)) if *row == id && at.elapsed() < FLASH => {
                let flash = inner.rounded(px(8.)).with_animation(SharedString::from(format!("flash-{id}")), Animation::new(FLASH).with_easing(ease_out_quint()), |d, t| d.bg(Hsla::from(FILL_4).opacity(1. - t)));
                div().child(flash)
            }
            _ => inner,
        };
        let inner = match part {
            Some(part) => self.arriving(agent, part, inner, window, cx),
            None => inner,
        };

        let element = div()
            .id(SharedString::from(id.clone()))
            .w_full()
            .flex()
            .justify_center()
            .pt(px(gap))
            .px(px(32.))
            .child(div().w_full().max_w(px(width)).min_w_0().child(inner))
            .into_any_element();
        if let Some((lang, code)) = highlight {
            self.chat_highlight(agent, &id, &lang, &code, cx);
        }
        element
    }
}

#[cfg(test)]
mod tests {
    use super::{Live, answer, code_span, content_width, in_fold, live_state, notice_label, phase_live, shown, stream_in, took};
    use chat::{Block, End, Message, Row, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    fn msg(text: &str, at: i64) -> Message {
        Message { id: text.into(), at, text: text.into() }
    }

    fn tool(kind: ToolKind, status: ToolStatus, at: i64) -> Block {
        Block::Tool(ToolCall { id: format!("t{at}"), name: "x".into(), kind, status, output: None, duration_ms: None, at })
    }

    fn shell(at: i64) -> Block {
        tool(ToolKind::Shell { command: "ls".into(), description: String::new() }, ToolStatus::Ok, at)
    }

    fn turn(blocks: Vec<Block>, end: Option<End>) -> Transcript {
        Transcript { turns: vec![Turn { id: "a".into(), prompt: Some(msg("go", 1_000)), blocks, end }] }
    }

    #[test]
    fn a_notice_names_the_command_that_finished_in_the_background_and_how() {
        let call = |name: &str, kind, status| ToolCall { id: "t".into(), name: name.into(), kind, status, output: None, duration_ms: None, at: 0 };
        let cmd = || ToolKind::Shell { command: "cargo test\n--workspace".into(), description: String::new() };
        assert_eq!(notice_label(&[&call("Bash", cmd(), ToolStatus::Failed)]), "Background command `cargo test` failed");
        assert_eq!(notice_label(&[&call("Bash", cmd(), ToolStatus::Ok)]), "Background command `cargo test` is done");
        assert_eq!(notice_label(&[&call("Monitor", cmd(), ToolStatus::Ok)]), "Monitor `cargo test` ended");
        let agent = call("Agent", ToolKind::Agent(SubAgent::default()), ToolStatus::Ok);
        assert_eq!(notice_label(&[&agent, &agent]), "2 subagents are done");
        assert_eq!(notice_label(&[&agent, &call("Bash", cmd(), ToolStatus::Ok)]), "2 background tasks are done");
    }

    #[test]
    fn a_notice_keeps_a_command_with_backticks_in_one_code_span() {
        assert_eq!(code_span("Background command `echo `date`` failed"), ("Background command ", Some("echo `date`"), " failed"));
        assert_eq!(code_span("2 subagents are done"), ("2 subagents are done", None, ""));
    }

    #[test]
    fn only_text_that_grew_since_the_last_update_streams_in() {
        let text = |id: &str, s: &str| Block::Text(Message { id: id.into(), at: 0, text: s.into() });
        let t = turn(vec![text("same", "word word"), text("grew", "word word word"), text("new", "word word")], None);
        let before = HashMap::from([("same".to_string(), 9), ("grew".to_string(), 4)]);
        let (mut reveals, now) = (HashMap::new(), Instant::now());
        stream_in(&mut reveals, &before, &t, now);
        assert!(!reveals.contains_key("same"));
        assert_eq!(reveals["grew"].cut("word word word", now), 4, "carries on from what showed");
        assert_eq!(reveals["new"].cut("word word", now), 0);
        let after = HashMap::from([("same".to_string(), 9), ("grew".to_string(), 14), ("new".to_string(), 9)]);
        stream_in(&mut reveals, &after, &t, now + Duration::from_secs(2));
        assert!(reveals.is_empty(), "finished reveals are forgotten");
    }

    #[test]
    fn content_width_clamps_between_560_and_1200_in_steps_of_16() {
        assert_eq!(content_width(300.), 560.);
        assert_eq!(content_width(5000.), 1200.);
        assert_eq!(content_width(745.), 752.);
        assert_eq!(content_width(720.), 720.);
        assert_eq!(content_width(f32::NAN), 720.);
    }

    #[test]
    fn a_live_fold_thinks_until_the_agent_acts_and_waits_on_the_user_first() {
        let thinking = turn(vec![Block::Thinking(msg("hm", 2_000))], None);
        assert_eq!(live_state(&thinking, 0..1, None, false, 9_000), Live::Thinking);
        let working = turn(vec![Block::Thinking(msg("hm", 2_000)), shell(3_000)], None);
        assert_eq!(live_state(&working, 0..1, None, false, 8_500), Live::Working { secs: 7, background: 0 });
        assert_eq!(live_state(&working, 0..1, Some(false), false, 8_500), Live::Approval);
        assert_eq!(live_state(&working, 0..1, Some(true), false, 8_500), Live::Answer);
    }

    #[test]
    fn a_live_fold_reads_stopping_from_the_stop_on() {
        let thinking = turn(vec![Block::Thinking(msg("hm", 2_000))], None);
        assert_eq!(live_state(&thinking, 0..1, None, true, 9_000), Live::Stopping);
        let working = turn(vec![shell(3_000)], None);
        assert_eq!(live_state(&working, 0..1, Some(false), true, 8_500), Live::Stopping);
    }

    #[test]
    fn a_settled_fold_counts_the_turns_time_else_its_first_to_last_move() {
        let done = turn(vec![shell(3_000)], Some(End::Done { duration_ms: 64_000, usage: None }));
        assert_eq!(took(&done, 0..1), 64_000);
        let failed = turn(vec![shell(3_000), shell(9_000)], Some(End::Failed { error: "interrupted".into(), duration_ms: 0 }));
        assert_eq!(took(&failed, 0..1), 8_000);
    }

    #[test]
    fn a_phase_is_live_until_its_turn_writes_or_works_on_past_it() {
        let open = turn(vec![shell(1), Block::Todos { id: "d".into(), todos: vec![] }], None);
        assert!(phase_live(&open, 0, &(0..1)));
        let answering = turn(vec![shell(1), Block::Text(msg("Done.", 2))], None);
        assert!(!phase_live(&answering, 0, &(0..1)));
        let ended = turn(vec![shell(1)], Some(End::Done { duration_ms: 1, usage: None }));
        assert!(!phase_live(&ended, 0, &(0..1)));
    }

    #[test]
    fn a_live_fold_counts_the_sub_agents_working_on_in_the_background() {
        let agent = tool(ToolKind::Agent(SubAgent::default()), ToolStatus::Running, 2_000);
        let t = turn(vec![agent], Some(End::Done { duration_ms: 1, usage: None }));
        assert_eq!(live_state(&t, 0..1, None, false, 5_000), Live::Working { secs: 4, background: 1 });
    }

    #[test]
    fn a_phase_hangs_off_its_fold_only_once_the_live_line_leaves_it() {
        let agent = |status| tool(ToolKind::Agent(SubAgent::default()), status, 1);
        let phase = Row::Phase { turn: 0, blocks: 0..1 };
        let live = turn(vec![agent(ToolStatus::Running)], Some(End::Done { duration_ms: 1, usage: None }));
        assert!(!in_fold(&live, chat::live_tail(&live), &phase, true), "the live tail's fold has no header to hang from");
        let settled = turn(vec![agent(ToolStatus::Ok)], Some(End::Done { duration_ms: 1, usage: None }));
        assert!(in_fold(&settled, chat::live_tail(&settled), &phase, true));
    }

    #[test]
    fn a_closed_fold_hides_its_work_but_not_a_failed_sub_agent() {
        let agent = |status| tool(ToolKind::Agent(SubAgent::default()), status, 1);
        let t = turn(vec![shell(1), agent(ToolStatus::Ok), agent(ToolStatus::Failed)], None);
        let phase = |blocks| Row::Phase { turn: 0, blocks };
        assert!(!shown(&t, &phase(0..2), false));
        assert!(shown(&t, &phase(2..3), false));
        assert!(shown(&t, &phase(0..2), true));
    }

    #[test]
    fn a_closed_fold_still_shows_the_text_between_its_work() {
        let t = turn(vec![Block::Text(msg("Let me look.", 1)), shell(2), Block::Text(msg("All good.", 3))], Some(End::Done { duration_ms: 1, usage: None }));
        let text = |block| Row::Text { turn: 0, block, range: 0..1 };
        assert!(shown(&t, &text(0), false) && shown(&t, &text(2), false));
        assert!(in_fold(&t, None, &text(0), true) && !in_fold(&t, None, &text(0), false), "it hangs off the rail only while the work shows");
        assert!(!in_fold(&t, None, &text(2), true));
    }

    #[test]
    fn a_replys_answer_leaves_out_the_text_between_its_work() {
        let t = turn(vec![Block::Text(msg("Let me look.", 1)), shell(2), Block::Text(msg("All good.", 3))], Some(End::Done { duration_ms: 1, usage: None }));
        assert_eq!(answer(&t, 0..1), "All good.");
    }

    #[test]
    fn a_shared_file_shows_under_a_shut_fold() {
        let call = ToolCall {
            id: "s".into(),
            name: chat::shared::TOOL.into(),
            kind: ToolKind::Shared { path: "chart.png".into(), caption: String::new() },
            status: ToolStatus::Ok,
            output: Some("/h/attachments/0a1b-chart.png".into()),
            duration_ms: None,
            at: 0,
        };
        let t = Transcript { turns: vec![Turn { id: "u1".into(), blocks: vec![Block::Tool(call)], ..Turn::default() }] };
        assert!(shown(&t, &Row::Shared { turn: 0, block: 0 }, false));
    }
}
