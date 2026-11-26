use super::fold::{self, Live};
use super::jump::{self, Glide, Seek};
use super::markdown::{self, Docs, Highlights};
use super::pin::{self, Pin};
use super::reveal::{self, Reveal};
use super::tool_row::Tools;
use super::working::{self, format_elapsed};
use super::{end_row, meta, phase, pr_link, question_marker, sticky, sub_agent, timeline, todos, user_bubble};
use crate::creating::{self, Creates};
use crate::explorer::mermaid::{self, Diagrams, Resized};
use crate::util::now_ms;
use agents::delegations::Delegation;
use chat::{Block, Command, Delivered, Delivery, End, Message, Row, Session, ToolKind, ToolStatus, Transcript, Turn};
use gpui_kit::base::text::TextView;
use gpui_kit::*;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::ops::Range;
use std::rc::Rc;
use std::sync::Arc;
use std::time::{Duration, Instant};
use theme::*;

mod sends;

/// The transcript's reading width.
pub(super) const WIDTH: f32 = 880.;
/// How long a row shown from the background tray, or a prompt landed on from the timeline, stays lit.
pub(super) const FLASH: Duration = Duration::from_millis(900);
/// A sent prompt fades in where it's held, from just below.
const SENT: creating::Part = (0., 12.);

/// A reading width held between 560 and 1200, in 16px steps.
pub(super) fn content_width(width: f32) -> f32 {
    let width = if width.is_finite() { width.clamp(560., 1200.) } else { WIDTH };
    560. + ((width - 560.) / 16.).round() * 16.
}

#[derive(Default)]
pub(crate) struct Side {
    pub(crate) agent: agents::Summary,
    pub(crate) inset: f32,
    pub(crate) ask: Option<bool>,
    pub(crate) send_now: bool,
    pub(crate) delegated_by: Option<(String, String)>,
    pub(crate) here: std::path::PathBuf,
    pub(crate) pick_open: Option<String>,
    pub(crate) delegations: Vec<Delegation>,
    pub(crate) hosts: Option<(Vec<hosts::Host>, crate::fleet::Links)>,
    pub(crate) capturing: bool,
    pub(crate) prs: Option<Arc<[pr_link::Peek]>>,
}

pub(crate) enum ViewEvent {
    Open(String, markdown::Press),
    Command(Command),
    TakeBack(String),
    Pick(String),
    OpenSession(String),
    LinkHost(String),
    ShowHosts,
    ShowPr,
    SendFix(Vec<String>, Vec<String>),
    Scrolled(bool),
    ReachedTop,
}

impl EventEmitter<ViewEvent> for View {}

/// One agent's chat as shown: its folded session and the list laying out its rows.
pub(crate) struct View {
    agent: String,
    pub(super) side: Rc<Side>,
    creates: Entity<Creates>,
    diagrams: Entity<Diagrams>,
    _subs: Vec<Subscription>,
    pub(super) session: Session,
    pub(super) list: ListState,
    pub(super) tools: Entity<Tools>,
    jump: bool,
    glide: Option<(Instant, Glide)>,
    seek: Option<(Instant, Seek)>,
    /// How far a delegated session's breadcrumb covers the top of the list, which pads its rows to start below it.
    pub(super) inset: f32,
    /// The prompt just sent, held at the top while its reply grows.
    pin: Option<Pin>,
    /// When the user last sent a prompt: its row rises into the chat.
    sent: Option<Instant>,
    /// The row just shown from the background tray or the prompt timeline, and when: it flashes, or a prompt's bubble
    /// lights.
    flash: Option<(String, Instant)>,
    /// Whether the user's last scroll left the list at its end, until the next render reads it.
    scrolled: Rc<Cell<Option<bool>>>,
    /// The fold that last settled, and when: its label fades in.
    settled: Option<(String, Instant)>,
    copied: Option<(String, Task<()>)>,
    expanded: HashSet<String>,
    /// The row last shown more or less of, and when.
    toggled: Option<(String, Instant)>,
    /// When the user last opened or shut a row, until it has eased to its height: the chat holds still meanwhile.
    held: Option<Instant>,
    code: Highlights,
    docs: Docs,
    /// Messages still streaming onto the screen, by id.
    reveals: HashMap<String, Reveal>,
    /// The prompt row whose card over the top is opened in full.
    pub(crate) sticky_open: Option<String>,
    /// Where the pointer is on the prompt timeline; its moves redraw only the transcript.
    pub(super) pointer: Entity<timeline::Pointer>,
    /// Where the prompt timeline's long tick is sliding.
    pub(super) lit: Option<timeline::Lit>,
    /// Whether the chat has shown what it opened with; text after that streams in.
    primed: bool,
    at_top: bool,
    /// Which deliveries arrived after it did.
    arrivals: crate::delegated::Arrivals,
    links_pr: bool,
    has_host_link: bool,
}

impl View {
    pub(crate) fn new(agent: &str, creates: Entity<Creates>, diagrams: Entity<Diagrams>, cx: &mut Context<Self>) -> Self {
        let list = ListState::new(0, ListAlignment::Bottom, px(320.));
        list.set_follow_mode(FollowMode::Tail);
        let scrolled = Rc::new(Cell::new(None));
        list.set_scroll_handler({
            let scrolled = scrolled.clone();
            let this = cx.weak_entity();
            move |e, _, cx| {
                scrolled.set(Some(!e.is_scrolled));
                let (this, away, top) = (this.clone(), e.is_scrolled, e.visible_range.start == 0);
                cx.defer(move |cx| {
                    _ = this.update(cx, |v, cx| {
                        cx.emit(ViewEvent::Scrolled(away));
                        if reaches_top(&mut v.at_top, top) {
                            cx.emit(ViewEvent::ReachedTop);
                        }
                    })
                });
            }
        });
        let tools = cx.new(|_| Tools::default());
        let pointer = cx.new(|_| timeline::Pointer::default());
        let subs = vec![
            cx.observe(&tools, |view, tools, cx| {
                // It notifies once more when a diff's colours land, long after the row opened.
                let toggled = tools.read(cx).toggled.as_ref().map(|(_, at)| *at).filter(|at| easing(*at, Instant::now()));
                view.held = view.held.max(toggled);
                view.remeasure_under_pin();
            }),
            cx.subscribe(&diagrams, |view, _, _: &Resized, _| view.remeasure_under_pin()),
        ];
        Self {
            agent: agent.to_string(),
            side: Rc::default(),
            creates,
            diagrams,
            _subs: subs,
            session: Session::default(),
            list,
            tools,
            jump: false,
            glide: None,
            seek: None,
            inset: 0.,
            pin: None,
            sent: None,
            flash: None,
            scrolled,
            settled: None,
            copied: None,
            expanded: HashSet::new(),
            toggled: None,
            held: None,
            code: Highlights::default(),
            docs: Docs::default(),
            reveals: HashMap::new(),
            sticky_open: None,
            pointer,
            lit: None,
            primed: false,
            at_top: false,
            arrivals: Default::default(),
            links_pr: false,
            has_host_link: false,
        }
    }

    pub(crate) fn links_pr(&self) -> bool {
        self.links_pr
    }

    pub(crate) fn has_host_link(&self) -> bool {
        self.has_host_link
    }

    pub(crate) fn set_side(&mut self, side: Side) {
        self.inset = side.inset;
        self.side = Rc::new(side);
    }

    /// How far the list sits above its end.
    pub(super) fn distance(&self) -> f32 {
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

    /// How far the list would scroll to bring row `ix` to the top, by the heights laid out so far.
    fn offset_to(&self, ix: usize) -> f32 {
        let (top, offset) = self.top_row();
        f32::from(self.list.laid_out_height_from(top) - self.list.laid_out_height_from(ix)) - offset
    }

    /// Row `ix`'s height as last laid out.
    fn row_height(&self, ix: usize) -> f32 {
        f32::from(self.list.laid_out_height_from(ix) - self.list.laid_out_height_from(ix + 1))
    }

    /// Whether row `id` is lit just now.
    fn flashing(&self, id: &str) -> bool {
        self.flash.as_ref().is_some_and(|(row, at)| row == id && at.elapsed() < FLASH)
    }

    /// Lights row `ix`.
    fn flash_row(&mut self, ix: usize) {
        self.flash = self.session.ids.get(ix).map(|id| (id.clone(), Instant::now()));
    }

    /// How far under the viewport's top row `ix` starts; None while that's unmeasured.
    pub(super) fn row_top(&self, ix: usize) -> Option<f32> {
        if self.list.logical_scroll_top().item_ix < self.list.item_count() {
            // The list paints its rows under its top padding, which `bounds_for_item` leaves out.
            return self.list.bounds_for_item(ix).map(|b| f32::from(b.top() - self.list.viewport_bounds().top()) + pin::TOP);
        }
        Some(self.shown_at_end() - f32::from(self.list.laid_out_height_from(ix)))
    }

    /// How far under the viewport's top row `ix` ends, by the heights last laid out; None once it's above the top.
    pub(super) fn row_end(&self, ix: usize) -> Option<f32> {
        (ix >= self.top_row().0).then(|| self.offset_to(ix + 1) + pin::TOP)
    }

    /// The height the rows show in at the list's end, over the room padded under them.
    fn shown_at_end(&self) -> f32 {
        f32::from(self.list.viewport_bounds().size.height) - self.inset - pin::BOTTOM - self.pin.as_ref().map_or(0., |p| p.spacer)
    }

    /// Keeps a row the user opened or shut where it shows while it eases to its height. Following its end, the list
    /// would push that row and all above it up instead. Under a held prompt the room, sized from the heights last
    /// measured, would run a frame behind a row shrinking, and the list, too short, would scroll up to the turn before.
    fn hold_row(&mut self, window: &mut Window) {
        if !self.held.is_some_and(|at| easing(at, Instant::now())) {
            return;
        }
        window.request_animation_frame();
        if self.pin.as_ref().is_some_and(|p| p.anchored || p.unmeasured()) {
            self.remeasure_under_pin();
        } else if self.list.is_following_tail() {
            let ix = self.top_row().0;
            if let Some(top) = self.row_top(ix) {
                self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) });
                self.list.scroll_by(px(pin::TOP - top));
            }
        }
    }

    /// Sizes the room that holds the sent prompt a quarter down and keeps the list on it, gliding it there, until its
    /// reply fills what's under it.
    fn hold_pin(&mut self, glide: bool, window: &mut Window) {
        let scrolled = self.scrolled.take();
        let opened = self.held.is_some();
        let Some(pin) = &mut self.pin else { return };
        if let Some(at_end) = scrolled.or_else(|| (!pin.unmeasured() && !pin.anchored && self.list.is_following_tail()).then_some(true)) {
            pin.moved(at_end);
        }
        let Some(ix) = pin.row(&self.session.rows) else {
            self.pin = None;
            return;
        };
        let viewport = f32::from(self.list.viewport_bounds().size.height) - self.inset;
        let laid_out = self.list.laid_out(ix).then(|| f32::from(self.list.laid_out_height_from(ix)));
        let above = f32::from(self.list.laid_out_height_from(0) - self.list.laid_out_height_from(ix));
        let now = Instant::now();
        if !pin.step(viewport, above, pin.below(self.list.height_from(ix).map(f32::from), laid_out), glide.then_some(now)) {
            // On the prompt the list follows the tail from here; scrolled away, or filled by a row the user opened, it
            // stays put until back at the end.
            let (anchored, top) = (pin.anchored, self.list.logical_scroll_top());
            self.pin = None;
            self.list.set_follow_mode(FollowMode::Tail);
            if !anchored || opened {
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
    pub(super) fn remeasure_under_pin(&self) {
        if let Some(ix) = self.pin.as_ref().and_then(|p| p.row(&self.session.rows)) {
            self.list.remeasure_items(ix..self.list.item_count());
        }
    }

    /// Holds the prompt just sent at the top once its row shows.
    pub(super) fn pin(&mut self) {
        self.pin = Some(Pin::new());
        self.seek = None;
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

    /// Refolds the session when its timeline moved, or the sessions it delegated to did, and lays out what changed;
    /// returns the queued sends it let go.
    pub(crate) fn refresh(
        &mut self,
        agent: &agents::Summary,
        revision: u64,
        items: &[agents::Item],
        delegations: &[agents::delegations::Delegation],
    ) -> Vec<(u64, String)> {
        self.session.delegated(&crate::delegated::spans(delegations));
        let before: HashMap<String, usize> = latest_texts(&self.session.transcript).map(|m| (m.id.clone(), m.text.len())).collect();
        let tail = self.session.live_tail().map(|f| self.session.transcript.turns[f].id.clone());
        self.session.outgoing.set_compacting(agent.compacting);
        if let Some((changed, range, count)) =
            self.session.update(chat::for_provider(&agent.provider), revision, items, super::composer::working(agent), now_ms())
        {
            if let Some(turn) = chat::settled_tail(tail.as_deref(), &self.session.transcript) {
                self.settled = Some((format!("{turn}#fold"), Instant::now()));
            }
            let delivered = self.session.transcript.turns.iter().rev().take(2).flat_map(|t| &t.blocks).filter_map(|b| match b {
                Block::Delivered(d) => Some(d.id.as_str()),
                _ => None,
            });
            self.arrivals.observe(delivered, self.primed, Instant::now());
            let links = |m: &Message| m.text.contains("/pull/");
            self.links_pr = self.links_pr
                || if self.primed {
                    latest_texts(&self.session.transcript).any(links)
                } else {
                    self.session.transcript.turns.iter().flat_map(|t| &t.blocks).filter_map(text_of).any(links)
                };
            self.has_host_link =
                self.has_host_link || self.session.rows.get(changed.clone()).is_some_and(|rows| rows.iter().any(|r| matches!(r, Row::HostLink { .. })));
            self.pace(&before, Instant::now());
            // Rows added above leave the list short of its top again.
            self.at_top = false;
            self.list.remeasure_items(changed.clone());
            self.list.splice(range, count);
            // Rows kept above the change can show or hide as their reply settles; the list measured them as they were.
            let rows = &self.session.rows[..changed.start.min(self.session.rows.len())];
            if let Some(fold) = rows.iter().rposition(|r| matches!(r, Row::Fold { .. })) {
                self.list.remeasure_items(fold..changed.start);
            }
            let ids: HashSet<&str> = self.session.ids.iter().map(String::as_str).collect();
            self.code.retain(|row| ids.contains(row.as_str()));
            // A group of reports keeps a document per report, keyed under its row's id.
            self.docs.retain(|key| ids.contains(key.split('|').next().unwrap_or(key)));
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

fn reaches_top(was: &mut bool, now: bool) -> bool {
    !std::mem::replace(was, now) && now
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

fn delivered_in(turns: &[Turn]) -> impl Iterator<Item = &Delivered> {
    turns.iter().flat_map(|turn| &turn.blocks).filter_map(|b| if let Block::Delivered(d) = b { Some(d) } else { None })
}

/// Whether the agent is taking in a report of group `turns`, which its line shows in place of the live line.
fn taking_in(t: &Transcript, turns: &Range<usize>, closes: bool, ask: Option<bool>) -> bool {
    !closes && ask.is_none() && chat::at_work_on(t, turns.clone())
}

/// A reply's answer: the text each of its turns ends on, after its work.
fn answer(t: &Transcript, turns: Range<usize>) -> String {
    let tail = |turn: &Turn| turn.blocks.iter().rposition(chat::is_work).map_or(0, |b| b + 1);
    let texts = t.turns[turns]
        .iter()
        .flat_map(|turn| turn.blocks[tail(turn)..].iter().filter_map(|b| if let Block::Text(m) | Block::Plan(m) = b { Some(m.text.as_str()) } else { None }));
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
        [chat::ToolCall { kind: ToolKind::Workflow(w), .. }] => format!("{} is done", w.label()),
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

fn copy(key: String, text: String, cx: &mut Context<View>) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    cx.listener(move |this, _, _, cx| this.copy_text(key.clone(), text.clone(), cx))
}

/// A prompt's bubble with its copy and time, or `under` in their place. A send shows as its echo will, so the echo
/// replacing it doesn't show.
#[expect(clippy::too_many_arguments, reason = "a view helper takes each part it draws")]
fn prompt_bubble(
    view: &View,
    id: &str,
    text: &str,
    at: i64,
    dim: bool,
    under: Option<Div>,
    width: f32,
    here: &std::path::Path,
    quote_colours: Option<markdown::Styles>,
    window: &mut Window,
    cx: &mut Context<View>,
) -> Div {
    let lines = user_bubble::lines(text, width, window);
    let toggle = user_bubble::needs_collapse(lines).then(|| {
        let toggled = view.toggled.as_ref().filter(|(row, _)| row == id).map(|(_, at)| *at);
        let row = id.to_string();
        (user_bubble::Fold { lines, toggled }, cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_expanded(&row, cx)))
    });
    let open = {
        let this = cx.weak_entity();
        move |at: &str, _: &mut Window, cx: &mut App| _ = this.update(cx, |_, cx| cx.emit(ViewEvent::Open(at.to_string(), markdown::Press::Follow)))
    };
    let bubble =
        user_bubble::bubble(id, text.to_string(), width, dim, view.expanded.contains(id), view.flashing(id), here, toggle, open, quote_colours, window, cx);
    let under = under.unwrap_or_else(|| {
        let key = format!("prompt:{id}");
        user_bubble::actions(id, view.copied(&key), copy(key, text.to_string(), cx), Some(at))
    });
    div().flex().flex_col().items_end().gap(px(4.)).child(bubble).child(under)
}

/// The colours of the code a prompt quotes, once worked out; the first ask for them is left in `highlight`.
fn quote_colours(code: &mut Highlights, id: &str, prompt: &str, highlight: &mut Option<(String, String)>) -> Option<markdown::Styles> {
    let (quote, _) = crate::add_to_chat::split_quote(prompt)?;
    let (lang, text) = super::quote_card::colour_source(&quote)?;
    let (colours, start) = code.get(id, &text, is_dark());
    if start {
        *highlight = Some((lang.to_string(), text));
    }
    colours.map(|c| c.styles().clone())
}

/// Whether a row opened or shut at `toggled` can still be easing to its height at `now`.
fn easing(toggled: Instant, now: Instant) -> bool {
    now.saturating_duration_since(toggled) < user_bubble::TWEEN_MAX
}

fn toggle(id: &str, default: bool, cx: &mut Context<View>) -> impl Fn(&ClickEvent, &mut Window, &mut App) + 'static {
    let id = id.to_string();
    cx.listener(move |this, _, _, cx| this.toggle_row(&id, default, cx))
}

impl Render for View {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().size_full().flex().flex_col().child(self.body(window, cx))
    }
}

impl View {
    fn body(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if self.list.item_count() == 0 {
            return div().flex_1().flex().items_center().justify_center().text_size(px(14.)).text_color(TEXT_3).child("Send a message to start.");
        }
        if self.scrolled.get().is_some() {
            self.seek = None;
        }
        self.hold_row(window);
        self.hold_pin(!cx.reduce_motion(), window);
        self.held = self.held.filter(|at| easing(*at, Instant::now()));
        let busy = chat::busy(&self.session.transcript, &self.session.outgoing, super::composer::working(&self.side.agent));
        self.jump = self.glide.is_none() && jump::jump_visibility(self.jump, self.distance(), busy);
        let spacer = self.pin.as_ref().map_or(0., |p| p.spacer);
        let inset = self.inset;
        let rows = list(self.list.clone(), cx.processor(|this, ix, window, cx| this.row(ix, window, cx)))
            .size_full()
            .pt(px(pin::TOP + inset))
            .pb(px(pin::BOTTOM + spacer));
        let pill = self.jump.then(|| jump::pill(cx.listener(|this, _, window, cx| this.jump_to_bottom(window, cx))));
        let sticky = self.sticky(window, cx);
        let cover = sticky.as_ref().map_or(0., |(_, cover)| *cover);
        let timeline = self.timeline(window, cx);
        // A see-through page has no fill for the breadcrumb to fade the rows into, so nothing paints under it.
        let clip = if see_through() { inset } else { 0. };
        div()
            .relative()
            .flex_1()
            .min_h_0()
            .child(sticky::below(cover.max(clip), rows))
            .children(sticky.map(|(card, _)| sticky::below(clip, card)))
            .children(timeline)
            .children(pill)
    }

    /// Opens or closes a fold, phase or sub-agent, and has the list measure again what that shows or hides.
    pub(crate) fn toggle_row(&mut self, id: &str, default: bool, cx: &mut Context<Self>) {
        self.held = Some(Instant::now());
        self.toggled = Some((id.to_string(), Instant::now()));
        let s = &mut self.session;
        s.toggle(id, default);
        if let Some(ix) = s.ids.iter().position(|i| i == id) {
            let end = match s.rows[ix] {
                Row::Fold { .. } => (ix + 1..s.rows.len())
                    .find(|&i| {
                        matches!(s.rows[i], Row::Prompt { .. } | Row::Notice { .. } | Row::Fold { .. } | Row::Live { .. } | Row::Pending(_) | Row::Working)
                    })
                    .unwrap_or(s.rows.len()),
                _ => ix + 1,
            };
            self.list.remeasure_items(ix..end);
        }
        cx.notify();
    }

    pub(crate) fn reveal(&mut self, ix: usize, cx: &mut Context<Self>) {
        self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) });
        if let Some(f) = self.session.shut_fold_over(ix) {
            self.toggle_row(&f, false, cx);
        }
    }

    pub(crate) fn show_block(&mut self, turn: usize, block: usize, cx: &mut Context<Self>) {
        let s = &self.session;
        let Some(ix) = chat::row_of(&s.rows, turn, block) else { return };
        let id = s.ids[ix].clone();
        let shut = s.shut_phase(ix);
        // A prompt held at the top, or a glide to the end, would scroll the list off the row.
        (self.pin, self.glide, self.seek) = (None, None, None);
        self.flash = Some((id.clone(), Instant::now()));
        self.reveal(ix, cx);
        if shut {
            self.toggle_row(&id, false, cx);
        }
        cx.notify();
    }

    pub(super) fn jump_to_prompt(&mut self, ix: usize, cx: &mut Context<Self>) {
        // A prompt held at the top, or a glide to the end, would scroll the list off the prompt.
        (self.pin, self.glide, self.seek) = (None, None, None);
        self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(0.) });
        cx.notify();
    }

    pub(super) fn seek_prompt(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        if cx.reduce_motion() {
            return self.jump_to_prompt(ix, cx);
        }
        let (top, offset) = self.top_row();
        let viewport = f32::from(self.list.viewport_bounds().size.height);
        let start = jump::lead_in(top, ix, viewport, |row| self.list.laid_out(row).then(|| self.row_height(row)));
        if start == top && self.offset_to(ix).abs() < 0.5 {
            // Already there: letting go lights it at once.
            return self.jump_to_prompt(ix, cx);
        }
        (self.pin, self.glide) = (None, None);
        let offset = if start == top { offset } else { 0. };
        self.list.scroll_to(ListOffset { item_ix: start, offset_in_item: px(offset) });
        // A seek under way already has a frame coming that steps this one.
        if self.seek.replace((Instant::now(), Seek::new(ix))).is_none() {
            self.seek_step(window, cx);
        }
    }

    /// One frame of a glide to a prompt; once there it lights the prompt if it was let go on.
    fn seek_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = self.seek.as_ref().map(|(_, seek)| seek.row) else { return };
        let remaining = self.offset_to(row);
        let Some((start, seek)) = &mut self.seek else { return };
        let (by, done) = seek.step(start.elapsed(), remaining);
        if done {
            let lights = seek.lights;
            self.seek = None;
            self.list.scroll_to(ListOffset { item_ix: row, offset_in_item: px(0.) });
            if lights {
                self.flash_row(row);
            }
        } else {
            self.list.scroll_by(px(by));
            cx.on_next_frame(window, |this, window, cx| this.seek_step(window, cx));
        }
        cx.notify();
    }

    pub(super) fn land_on_prompt(&mut self, ix: usize, cx: &mut Context<Self>) {
        if self.seek.as_mut().is_some_and(|(_, seek)| seek.land(ix)) {
            return;
        }
        self.flash_row(ix);
        self.jump_to_prompt(ix, cx);
    }

    pub(crate) fn step_prompt(&mut self, down: bool, cx: &mut Context<Self>) {
        if let Some(ix) = chat::step_prompt(&self.session.rows, self.top_row().0, down) {
            self.jump_to_prompt(ix, cx);
        }
    }

    /// Scrolls to `rows` rows before the end; the list knows only measured heights, so a pixel scroll stops short.
    pub(crate) fn scroll_back(&mut self, rows: usize, cx: &mut Context<Self>) {
        self.list.scroll_to(ListOffset { item_ix: self.list.item_count().saturating_sub(rows), offset_in_item: px(0.) });
        cx.notify();
    }

    pub(crate) fn jump_to_bottom(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.jump = false;
        self.seek = None;
        match Glide::new(self.distance(), f32::from(self.list.viewport_bounds().size.height)) {
            Some(glide) => {
                self.glide = Some((Instant::now(), glide));
                self.glide_step(window, cx);
            }
            None => self.list.set_follow_mode(FollowMode::Tail),
        }
        cx.notify();
    }

    /// One frame of a glide to the end; it follows the tail once there.
    fn glide_step(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some((start, glide)) = &mut self.glide else { return };
        let (by, done) = glide.step(start.elapsed());
        self.list.scroll_by(px(by));
        if done {
            self.glide = None;
            self.list.set_follow_mode(FollowMode::Tail);
        } else {
            cx.on_next_frame(window, |this, window, cx| this.glide_step(window, cx));
        }
        cx.notify();
    }

    fn copy_text(&mut self, key: String, text: String, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(text));
        let clear = {
            let key = key.clone();
            cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_millis(1200)).await;
                this.update(cx, |view, cx| {
                    if view.copied(&key) {
                        view.copied = None;
                        cx.notify();
                    }
                })
                .ok();
            })
        };
        self.copied = Some((key, clear));
        cx.notify();
    }

    pub(crate) fn toggle_expanded(&mut self, row: &str, cx: &mut Context<Self>) {
        if !self.expanded.remove(row) {
            self.expanded.insert(row.to_string());
        }
        self.toggled = Some((row.to_string(), Instant::now()));
        self.held = Some(Instant::now());
        self.remeasure_under_pin();
        cx.notify();
    }

    fn retry(&mut self, send: u64, cx: &mut Context<Self>) {
        if let Some(text) = self.session.outgoing.retry(send) {
            cx.emit(ViewEvent::TakeBack(text));
        }
    }

    fn highlight(&mut self, row: &str, lang: &str, code: &str, cx: &mut Context<Self>) {
        let (row, lang, code, dark) = (row.to_string(), lang.to_string(), code.to_string(), is_dark());
        cx.spawn(async move |this, cx| {
            let len = code.len();
            let styles = cx.background_spawn(async move { markdown::highlight(&lang, &code, dark) }).await;
            this.update(cx, |view, cx| {
                view.code.set(&row, len, dark, styles);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    fn row(&mut self, ix: usize, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let side = self.side.clone();
        let agent = self.agent.clone();
        let agent = agent.as_str();
        let (provider, model) = (side.agent.provider.clone(), crate::status::model_or_agent(&side.agent));
        let ask = side.ask;
        let send_now = side.send_now;
        let delegated_by = side.delegated_by.clone();
        let here = side.here.clone();
        let open: markdown::Open = {
            let this = cx.weak_entity();
            Arc::new(move |link, press, _, cx| _ = this.update(cx, |_, cx| cx.emit(ViewEvent::Open(link.to_string(), press))))
        };
        let picks = sub_agent::Picks {
            open: side.pick_open.clone(),
            on_pick: {
                let this = cx.weak_entity();
                Rc::new(move |call: &str, _: &mut Window, cx: &mut App| _ = this.update(cx, |_, cx| cx.emit(ViewEvent::Pick(call.to_string()))))
            },
        };
        let creates = self.creates.clone();
        let this = cx.weak_entity();
        let diagrams = self.diagrams.clone();
        let view = &mut *self;
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
        let last_on_rail =
            rail && !s.rows[ix + 1..].iter().find(|next| !same_message(&row, next)).is_some_and(|next| in_fold(t, s.live_tail(), next, fold_open));
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
                Some(p) => {
                    let colours = quote_colours(&mut view.code, &id, &p.text, &mut highlight);
                    let bubble = prompt_bubble(view, &id, &p.text, p.at, false, None, width, &here, colours, window, cx);
                    match delegated_by.filter(|_| *turn == 0) {
                        Some((by, label)) => div()
                            .flex()
                            .flex_col()
                            .items_end()
                            .gap(px(6.))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.))
                                    .text_size(px(12.))
                                    .text_color(TEXT_4)
                                    .child(provider_icon(&by, 11., TEXT_4))
                                    .child(label),
                            )
                            .child(bubble),
                        None => bubble,
                    }
                }
                None => div(),
            },
            Row::Fold { turn } => {
                let turns = chat::folded(t, *turn);
                if s.live_tail() == Some(*turn) {
                    div()
                } else {
                    let settled = fold::settled(&id, &provider, fold::settled_label(who, took(t, turns)), fold_open, toggle(&id, false, cx));
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
                let on_toggle = toggle(&id, false, cx);
                phase::phase(&id, blocks, next_at(t, *turn, range.end), phase_live(t, *turn, range), s.open(&id, false), on_toggle, &picks, &view.tools, cx)
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
                    Some((turn, block)) => div().child(
                        line.cursor_pointer().hover(|s| s.text_color(TEXT_2)).on_click(cx.listener(move |this, _, _, cx| this.show_block(turn, block, cx))),
                    ),
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
                    // Drawn once whole: a fence still streaming is half a diagram, which wouldn't parse.
                    Some(("mermaid", code)) if markdown::closed(text) => {
                        let (key, place) = (mermaid::key(&code, is_dark()), mermaid::place((agent, &id)));
                        let heading = || mermaid::heading_above(full, range.start);
                        let block = div().child(mermaid::diagram(&diagrams, key, place, &SharedString::from(code.to_string()), heading, window, cx));
                        if text.starts_with(' ') { div().pl(px(markdown::LIST_INDENT)).child(block) } else { block }
                    }
                    Some((lang, code)) => {
                        let (colours, start) = view.code.get(&id, &code, is_dark());
                        if start {
                            highlight = Some((lang.to_string(), code.to_string()));
                        }
                        let block = div().child(markdown::code_block(&view.docs.code(&id, lang, &code, cx), colours));
                        let block = match reveal {
                            Some(_) => div()
                                .child(block.with_animation(SharedString::from(format!("code-in-{id}")), Animation::new(reveal::FADE), |b, t| b.opacity(t))),
                            None => block,
                        };
                        // An indented fence belongs to the list item above it.
                        if text.starts_with(' ') { div().pl(px(markdown::LIST_INDENT)).child(block) } else { block }
                    }
                    None => {
                        let peeks = match &side.prs {
                            // Only a message linking a PR copies the PRs it names.
                            Some(all) if text.contains("/pull/") => pr_link::within(all, text),
                            _ => Default::default(),
                        };
                        let doc = view.docs.get(&id, text, reveal.is_some() || live(t, *turn), cx);
                        div().child(markdown::block(TextView::new(&doc), open.clone()).plugin(pr_link::PrLinks {
                            peeks,
                            open: open.clone(),
                            view: this.clone(),
                        }))
                    }
                };
                let plan = range.start == 0
                    && !text.trim_start().starts_with('#')
                    && matches!(t.turns.get(*turn).and_then(|t| t.blocks.get(*block)), Some(Block::Plan(_)));
                div().flex().flex_col().gap(px(8.)).children(plan.then(todos::plan_header)).child(body)
            }
            Row::Question { turn, block } => match t.turns.get(*turn).and_then(|t| t.blocks.get(*block)) {
                Some(Block::Tool(call)) => {
                    let toggled = view.toggled.as_ref().filter(|(row, _)| *row == id).map(|(_, at)| *at);
                    let row = id.clone();
                    let on_click = cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_expanded(&row, cx));
                    question_marker::marker(&id, call, width, question_marker::Open { expanded: view.expanded.contains(&id), toggled, on_click }, window)
                }
                _ => div(),
            },
            Row::Reports { turns, closes } => {
                // Sessions yet to report belong to a group still waiting, until a prompt starts another wave.
                let waiting = !closes && !s.rows[ix + 1..].iter().any(|r| matches!(r, Row::Prompt { .. } | Row::Reports { .. }));
                let sessions = crate::delegated::standing(delivered_in(&t.turns[turns.clone()]), &side.delegations, waiting);
                let live = taking_in(t, turns, *closes, ask).then(|| crate::delegated::reading(delivered_in(&t.turns[turns.end - 1..turns.end]).last()));
                let sums_up = closes.then(|| turns.end - 1);
                let (shown, toggled) = (s.open(&id, false), view.toggled.as_ref().filter(|(row, _)| *row == id).map(|(_, at)| *at));
                let drawn = crate::delegated::drawn(shown, toggled);
                let reports = turns
                    .clone()
                    .map(|at| {
                        let said = if drawn && sums_up != Some(at) { answer(t, at..at + 1) } else { String::new() };
                        let answer = (!said.is_empty()).then(|| {
                            let doc = view.docs.get(&format!("{id}|{at}"), &said, false, cx);
                            markdown::block(TextView::new(&doc), open.clone()).text_size(px(13.)).line_height(px(20.)).text_color(TEXT_2).into_any_element()
                        });
                        crate::delegated::Report { key: format!("{id}-{at}"), deliveries: delivered_in(&t.turns[at..at + 1]).collect(), answer }
                    })
                    .collect();
                let (arrivals, now) = (&view.arrivals, Instant::now());
                let age = move |d: &str| arrivals.age(d, now);
                let group = crate::delegated::Group { id: &id, sessions, live, reports, open: shown, toggled, age: &age };
                crate::delegated::group(group, toggle(&id, false, cx), window, cx)
            }
            Row::Delivery { turn, block } => match t.turns.get(*turn).and_then(|t| t.blocks.get(*block)) {
                Some(Block::Delivered(d)) => crate::delegated::delivered(&id, d, view.arrivals.age(&d.id, Instant::now()), window, cx),
                _ => div(),
            },
            Row::HostLink { turn, block } => match t.turns.get(*turn).and_then(|t| t.blocks.get(*block)) {
                Some(Block::HostLink { hint, reason, .. }) => {
                    let linked = match &side.hosts {
                        Some((list, links)) => super::host_link::linked(hint, list, links),
                        None => super::host_link::Linked::No,
                    };
                    super::host_link::card(&id, hint, reason, linked, cx)
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
                    end_row::error(&id, error.clone(), view.copied(&key), copy(key.clone(), error.clone(), cx))
                }
                Some(End::Done { .. }) => match chat::reply(t, *turn).filter(|turns| !chat::at_work_on(t, turns.clone())) {
                    Some(turns) => {
                        let key = format!("reply:{id}");
                        let worked = chat::worked(t, turns.clone()).map(|(ms, _)| format_elapsed((ms / 1000) as i64));
                        let at = t.turns[turns.clone()].iter().flat_map(|turn| &turn.blocks).filter_map(block_at).max();
                        let retry = t.turns[turns.clone()].iter().find_map(|turn| turn.prompt.as_ref()).map(|p| {
                            let text = p.text.clone();
                            cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(ViewEvent::Command(Command::Send(text.clone()))))
                        });
                        meta::meta(&id, view.copied(&key), copy(key.clone(), answer(t, turns), cx), retry, worked, at)
                    }
                    None => return div().id(SharedString::from(id)).into_any_element(),
                },
                None => div(),
            },
            Row::Live { turn }
                if s.rows
                    .iter()
                    .rev()
                    .find_map(|r| if let Row::Reports { turns, closes } = r { Some((turns, *closes)) } else { None })
                    .is_some_and(|(turns, closes)| turns.contains(turn) && taking_in(t, turns, closes, ask)) =>
            {
                div()
            }
            Row::Live { turn } => {
                fold::live(&id, &provider, fold::live_label(who, &model, &live_state(t, chat::folded(t, *turn), ask, s.outgoing.stopping(), now_ms())))
            }
            Row::Working if s.outgoing.compacting() => working::compacting(),
            Row::Working if s.outgoing.trail() == Some(agents::PromptState::Resending) => working::resending(),
            Row::Working => {
                let live = if s.outgoing.stopping() { Live::Stopping } else { Live::Thinking };
                fold::live(&id, &provider, fold::live_label(who, &model, &live))
            }
            Row::Pending(i) => match s.outgoing.echoed().nth(*i) {
                Some((_, p)) => {
                    let retry = p.not_delivered().map(|text| {
                        let send = p.id;
                        working::not_delivered(SharedString::from(format!("retry-{id}")), text)
                            .on_click(cx.listener(move |this, _, _, cx| this.retry(send, cx)))
                    });
                    // What waits for the agent's next step goes all at once, so only the last of it says so.
                    let waiting: Vec<u64> = s.outgoing.waiting().map(|w| w.id).collect();
                    let under = waiting.contains(&p.id).then(|| {
                        if waiting.last() != Some(&p.id) {
                            return div();
                        }
                        let action = (send_now && !s.outgoing.sending_now()).then(|| {
                            user_bubble::send_now(&id)
                                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(ViewEvent::Command(Command::SendNow))))
                                .into_any_element()
                        });
                        user_bubble::waiting(who, waiting.len(), action)
                    });
                    let under = under.or_else(|| s.outgoing.after_turn().any(|c| c.id == p.id).then(|| user_bubble::after_turn(who)));
                    let dim = p.delivery == Delivery::Failed || under.is_some();
                    let colours = quote_colours(&mut view.code, &id, &p.text, &mut highlight);
                    prompt_bubble(view, &id, &p.text, p.sent_at, dim, under, width, &here, colours, window, cx).children(retry)
                }
                None => div(),
            },
        };
        let inner = if rail { phase::in_fold(last_on_rail, elbow, inner) } else { inner };
        let prompt = |r: &Row| matches!(r, Row::Prompt { .. } | Row::Pending(_));
        let rising =
            view.sent.and_then(|at| creating::rise(at.elapsed(), SENT, cx.reduce_motion())).filter(|_| prompt(&row) && !s.rows[ix + 1..].iter().any(prompt));
        let inner = match rising {
            Some(r) => {
                window.request_animation_frame();
                r.on(inner)
            }
            None => inner,
        };
        let inner = match &view.flash {
            // A prompt lights its bubble instead.
            Some(_) if view.flashing(&id) && !prompt(&row) => {
                let flash = inner.rounded(px(8.)).with_animation(
                    SharedString::from(format!("flash-{id}")),
                    Animation::new(FLASH).with_easing(ease_out_quint()),
                    |d, t| d.bg(Hsla::from(FILL_4).opacity(1. - t)),
                );
                div().child(flash)
            }
            _ => inner,
        };
        let inner = match part {
            Some(part) => creating::arriving(&creates, agent, part, inner, window, cx),
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
            self.highlight(&id, &lang, &code, cx);
        }
        element
    }
}

#[cfg(test)]
mod tests {
    use super::{Live, answer, code_span, content_width, easing, in_fold, live_state, notice_label, phase_live, reaches_top, shown, stream_in, took};
    use chat::{Block, End, Message, Row, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};
    use std::collections::HashMap;
    use std::time::{Duration, Instant};

    #[test]
    fn the_top_is_reached_once_however_long_the_list_stays_there() {
        let mut was = false;
        let got: Vec<bool> = [false, true, true, false, true].into_iter().map(|top| reaches_top(&mut was, top)).collect();
        assert_eq!(got, [false, true, false, false, true]);
    }

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
        let workflow = ToolKind::Workflow(chat::workflow::Workflow { name: "measure".into(), ..Default::default() });
        assert_eq!(notice_label(&[&call("Workflow", workflow, ToolStatus::Ok)]), "Workflow measure is done");
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
    fn a_row_the_user_opened_holds_the_chat_no_longer_than_a_height_eases() {
        let t0 = Instant::now();
        assert!(easing(t0, t0) && easing(t0, t0 + Duration::from_millis(849)));
        assert!(!easing(t0, t0 + Duration::from_millis(850)));
    }

    #[test]
    fn content_width_clamps_between_560_and_1200_in_steps_of_16() {
        assert_eq!(content_width(300.), 560.);
        assert_eq!(content_width(5000.), 1200.);
        assert_eq!(content_width(745.), 752.);
        assert_eq!(content_width(720.), 720.);
        assert_eq!(content_width(f32::NAN), 880.);
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
}
