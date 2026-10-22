mod detail;
mod graph;

use super::sub_agent::{self, Mark};
use super::working::format_elapsed;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use crate::util::now_ms;
use agents::Summary;
use chat::{Block, Spawned, ToolCall, ToolStatus, Transcript};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::{Duration, Instant};
use theme::*;

/// `SURFACE` laid on `POPOVER`, opaque: a translucent card shows its rings, painted beneath it, through.
const CARD: Token = Token::new(0xffffffff, 0x373737ff);
/// Long enough for `SPRING` to settle, so the map stays drawn while it fades out.
const SHUTTING: Duration = Duration::from_millis(500);
const HEADER: f32 = 48.;
const FOOTER: f32 = 34.;
const DETAIL_HEIGHT: f32 = 460.;
const MAX_HEIGHT: f32 = 520.;
/// How far the map rises, and draws in from each side, as it opens out of the chip.
const RISE: f32 = 8.;
const INSET: f32 = 12.;

/// Where the agent map stands, apart from the focus it takes for its keys.
#[derive(Default)]
pub(crate) struct Nav {
    /// The agent whose map shows.
    open: Option<String>,
    /// A map closed a moment ago, drawn while it fades.
    closing: Option<(String, Instant)>,
    /// The node the arrow keys are on: 0 is the main agent, then `chat::spawned`'s order.
    selected: usize,
    /// The call whose details show over the graph.
    detail: Option<String>,
    /// The details last shown, kept while they slide back out.
    shown: Option<String>,
    prompt_open: bool,
}

impl Nav {
    fn is_open(&self, agent: &str) -> bool {
        self.open.as_deref() == Some(agent)
    }

    /// Whether `agent`'s map is drawn: open, or still fading out.
    fn drawn(&self, agent: &str, now: Instant) -> bool {
        self.is_open(agent) || self.closing.as_ref().is_some_and(|(a, at)| a == agent && now.duration_since(*at) < SHUTTING)
    }

    /// Opens `agent`'s map on its graph, or closes it when it's open.
    fn toggle(&mut self, agent: &str, now: Instant) {
        if self.is_open(agent) {
            return self.close(now);
        }
        *self = Self { open: Some(agent.to_string()), ..Self::default() };
    }

    fn close(&mut self, now: Instant) {
        if let Some(agent) = self.open.take() {
            self.closing = Some((agent, now));
        }
    }

    fn show(&mut self, node: usize, call: &str) {
        self.selected = node;
        self.detail = Some(call.to_string());
        self.shown = self.detail.clone();
        self.prompt_open = false;
    }

    fn back(&mut self) {
        self.detail = None;
    }

    /// Esc leaves the details for the graph, and the graph for the chat.
    fn escape(&mut self, now: Instant) {
        if self.detail.is_some() { self.back() } else { self.close(now) }
    }

    /// Moves the pick down or up the `count` nodes, stopping at either end.
    fn step(&mut self, down: bool, count: usize) {
        self.selected = if down { (self.selected + 1).min(count.saturating_sub(1)) } else { self.selected.saturating_sub(1) };
    }
}

pub(crate) struct AgentMap {
    pub(crate) nav: Nav,
    focus: FocusHandle,
}

impl AgentMap {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self { nav: Nav::default(), focus: cx.focus_handle() }
    }
}

/// How long `call` has run: until now while it runs.
fn ran_ms(call: &ToolCall, now: i64) -> i64 {
    match call.status {
        ToolStatus::Running => now - call.at,
        _ => call.duration_ms.map_or(0, |ms| ms as i64),
    }
}

/// "32.9k tokens", "640 tokens".
fn tokens(n: u64) -> String {
    if n < 1_000 { format!("{n} tokens") } else { format!("{:.1}k tokens", n as f64 / 1_000.) }
}

/// A sub-agent's second line: how long it ran and the context it used, or how long it ran before it failed.
fn meta(call: &ToolCall, tokens_used: u64, now: i64) -> String {
    let ran = format_elapsed(ran_ms(call, now) / 1000);
    match call.status {
        ToolStatus::Failed => format!("Failed after {ran}"),
        _ if tokens_used == 0 => ran,
        _ => format!("{ran} · {}", tokens(tokens_used)),
    }
}

/// The tool calls a sub-agent made itself.
fn calls(agent: &chat::SubAgent) -> usize {
    agent.transcript.turns.iter().flat_map(|t| &t.blocks).filter(|b| matches!(b, Block::Tool(_))).count()
}

/// What the main agent is on: the sub-agents its turn ends handing off to, its newest step, or done but for those still at work.
fn main_line(t: &Transcript, map: &[Spawned]) -> String {
    let still = map.iter().any(|s| s.call.status == ToolStatus::Running);
    let Some(turn) = t.turns.last().filter(|_| chat::at_work(t)) else {
        return if still { "Turn ended · agents still working".into() } else { "Done".into() };
    };
    if turn.blocks.last().is_some_and(chat::running_agent) {
        let n = turn.blocks.iter().filter(|b| chat::running_agent(b)).count();
        return format!("Handed off to {n} agent{}", if n == 1 { "" } else { "s" });
    }
    turn.blocks.iter().rev().find_map(|b| if let Block::Tool(c) = b { Some(sub_agent::step(c)) } else { None }).unwrap_or_else(|| "Working".into())
}

/// The chip's words: how many of the agents still work, or how many there were.
fn chip_label(map: &[Spawned]) -> (bool, String) {
    let running = map.iter().filter(|s| s.call.status == ToolStatus::Running).count();
    let n = map.len();
    let agents = if n == 1 { "agent" } else { "agents" };
    match running {
        0 => (false, format!("{n} {agents}")),
        r if r == n => (true, format!("{n} {agents}")),
        r => (true, format!("{r} of {n} {agents}")),
    }
}

/// The turn and block of the main agent's call that `map[ix]` was spawned under, to show it in the chat.
fn place(t: &Transcript, map: &[Spawned], mut ix: usize) -> Option<(usize, usize)> {
    while let Some(parent) = map[ix].parent {
        ix = parent;
    }
    let call = &map[ix].call.id;
    t.turns.iter().enumerate().find_map(|(ti, turn)| turn.blocks.iter().position(|b| matches!(b, Block::Tool(c) if &c.id == call)).map(|bi| (ti, bi)))
}

impl Desktop {
    /// The composer's chip that opens the agent map, while an agent works or the map is open.
    pub(super) fn chat_agents_chip(&self, agent: &Summary, t: &Transcript, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let map = chat::spawned(t);
        let (live, label) = chip_label(&map);
        let open = self.chats.map.nav.is_open(&agent.id);
        if map.is_empty() || !(live || open) {
            return None;
        }
        let a = agent.id.clone();
        let chip = super::super::composer::pill()
            .id(id(format!("chat-agents:{}", agent.id)))
            .cursor_pointer()
            .text_color(if open { ACCENT } else { TEXT_2 })
            .when(open, |d| d.bg(ACCENT_BG))
            .hover(|s| s.bg(FILL_2))
            .map(|d| if live { d.child(ui::dot(6., SUCCESS)) } else { d.child(icon("agent-map", 13., if open { ACCENT } else { TEXT_3 })) })
            .child(div().font_weight(FontWeight::MEDIUM).child(label))
            // As the model pill: in capture, so the map's own press-outside doesn't shut what this opens.
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                cx.stop_propagation();
                this.chat_toggle_map(&a, window, cx);
            }));
        Some(chip)
    }

    pub(crate) fn chat_toggle_map(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.chats.map.nav.toggle(agent, Instant::now());
        if self.chats.map.nav.is_open(agent) {
            self.chats.map.focus.focus(window, cx);
        } else {
            self.chats.input.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    fn chat_map_escape(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.chats.map.nav.escape(Instant::now());
        if self.chats.map.nav.open.is_none() {
            self.chats.input.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    pub(crate) fn chat_map_show(&mut self, node: usize, call: &str, cx: &mut Context<Self>) {
        self.chats.map.nav.show(node, call);
        cx.notify();
    }

    /// Shows in the chat the call `map[ix]` was spawned under, closing the map.
    fn chat_map_reveal(&mut self, agent: &str, ix: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.chats.views.get(agent) else { return };
        let t = &view.session.transcript;
        let at = place(t, &chat::spawned(t), ix);
        self.chats.map.nav.close(Instant::now());
        self.chats.input.update(cx, |s, cx| s.focus(window, cx));
        if let Some((turn, block)) = at {
            self.chat_show_block(agent, turn, block, cx);
        }
        cx.notify();
    }

    /// `agent`'s map over its composer column while it's open or fading out: the graph of who spawned whom, or one's details.
    pub(super) fn chat_agent_map(&self, agent: &Summary, t: &Transcript, cx: &mut Context<Self>) -> Option<AnyElement> {
        let nav = &self.chats.map.nav;
        if !nav.drawn(&agent.id, Instant::now()) {
            return None;
        }
        let open = nav.is_open(&agent.id);
        let map = chat::spawned(t);
        let now = now_ms();
        let (graph, graph_height) = self.chat_map_graph(agent, t, &map, open && nav.detail.is_none(), now, cx);
        let detail = nav.shown.as_ref().and_then(|call| map.iter().position(|s| &s.call.id == call)).map(|ix| self.chat_map_detail(&agent.id, &map, ix, open && nav.detail.is_some(), now, cx));
        let on_detail = nav.detail.is_some() && detail.is_some();
        let height = if on_detail { DETAIL_HEIGHT } else { HEADER + graph_height + FOOTER }.min(MAX_HEIGHT);

        let a = agent.id.clone();
        let header = div()
            .h(px(HEADER))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(10.))
            .pl(px(18.))
            .pr(px(10.))
            .child(div().text_size(px(15.)).font_weight(FontWeight::BOLD).text_color(TEXT).child("Agents"))
            .child(div().min_w_0().truncate().text_size(px(12.5)).text_color(TEXT_3).child(summary(&map)))
            .child(div().flex_1())
            .child(close_button(format!("chat-map-close:{}", agent.id)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.chats.map.nav.close(Instant::now());
                this.chats.input.update(cx, |s, cx| s.focus(window, cx));
                cx.notify();
            })));
        let list = div()
            .absolute()
            .top_0()
            .bottom_0()
            .w_full()
            .flex()
            .flex_col()
            .child(header)
            .child(graph)
            .child(div().h(px(FOOTER)).flex_none().px(px(18.)).text_size(px(11.5)).text_color(TEXT_4).child("Click an agent for its prompt, steps and report"));
        // One spring slides the graph out to the left as the details come in from the right, and back.
        let pages = div().id(id(format!("chat-map-pages:{}", agent.id))).relative().flex_1().min_h_0().overflow_hidden().with_spring(
            id(format!("chat-map-page:{}", agent.id)),
            SpringAnimation::new(SPRING).to(on_detail),
            move |d, p| {
                let p = p.0;
                d.child(list.left(relative(-0.18 * p)).opacity(1. - p)).children(detail.map(|page| page.left(relative(1. - p)).opacity(p)))
            },
        );
        let panel = ui::pop(div().id(id(format!("chat-map:{}", agent.id))))
            .w_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .track_focus(&self.chats.map.focus)
            .when(open, |d| {
                d.occlude()
                    .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        this.chats.map.nav.close(Instant::now());
                        cx.notify();
                    }))
                    .on_key_down(cx.listener(move |this, e: &KeyDownEvent, window, cx| this.chat_map_key(&a, e, window, cx)))
            })
            .child(pages)
            .with_spring(id(format!("chat-map-height:{}", agent.id)), SpringAnimation::new(SPRING).to(px(height)), |d, h| d.h(h));
        // Opens out of the chip: it rises and widens to its place as it fades in, and goes back the same way.
        let shown = div().absolute().bottom_full().left_0().right_0().child(panel).with_spring(
            id(format!("chat-map-open:{}", agent.id)),
            SpringAnimation::new(SPRING).to(open).from(false),
            |d, t| {
                let rest = 1. - t.0.clamp(0., 1.);
                d.opacity(t.0.clamp(0., 1.)).mb(px(RISE * t.0)).pl(px(INSET * rest)).pr(px(INSET * rest))
            },
        );
        Some(div().relative().w_full().h_0().child(shown).into_any_element())
    }

    fn chat_map_key(&mut self, agent: &str, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some(view) = self.chats.views.get(agent) else { return };
        let map = chat::spawned(&view.session.transcript);
        let nav = &mut self.chats.map.nav;
        match e.keystroke.key.as_str() {
            "escape" => return self.chat_map_escape(window, cx),
            "down" if nav.detail.is_none() => nav.step(true, map.len() + 1),
            "up" if nav.detail.is_none() => nav.step(false, map.len() + 1),
            "enter" | "right" if nav.detail.is_none() && nav.selected > 0 => {
                let (node, call) = (nav.selected, map[nav.selected - 1].call.id.clone());
                nav.show(node, &call);
            }
            "left" if nav.detail.is_some() => nav.back(),
            _ => return,
        }
        cx.stop_propagation();
        cx.notify();
    }
}

/// "4 agents · 2 working · 1 failed".
fn summary(map: &[Spawned]) -> String {
    let count = |status| map.iter().filter(|s| s.call.status == status).count();
    let (working, failed) = (count(ToolStatus::Running), count(ToolStatus::Failed));
    let all = format!("{} agent{}", map.len(), if map.len() == 1 { "" } else { "s" });
    [Some(all), (working > 0).then(|| format!("{working} working")), (failed > 0).then(|| format!("{failed} failed"))].into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

fn close_button(key: String) -> Stateful<Div> {
    div()
        .id(id(key))
        .size(px(26.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.))
        .cursor_pointer()
        .hover(|s| s.bg(FILL_3))
        .child(icon("x", 12., TEXT_3))
}

/// The mark a sub-agent's node shows.
fn mark(s: &Spawned) -> Mark {
    sub_agent::mark(s.call, s.agent)
}

/// A sub-agent's name on its node.
fn name(s: &Spawned) -> String {
    sub_agent::name(s.agent)
}

#[cfg(test)]
mod tests {
    use super::{Nav, SHUTTING, chip_label, main_line, meta, place, summary, tokens};
    use chat::{Block, End, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};
    use std::time::Instant;

    fn agent(id: &str, status: ToolStatus) -> Block {
        Block::Tool(ToolCall { id: id.into(), name: "Agent".into(), kind: ToolKind::Agent(SubAgent::default()), status, output: None, duration_ms: Some(27_000), at: 1_000 })
    }

    fn shell(id: &str) -> Block {
        Block::Tool(ToolCall { id: id.into(), name: "Bash".into(), kind: ToolKind::Shell { command: "cargo test".into() }, status: ToolStatus::Ok, output: None, duration_ms: None, at: 0 })
    }

    fn turn(blocks: Vec<Block>, ended: bool) -> Turn {
        Turn { id: "t".into(), prompt: None, blocks, end: ended.then_some(End::Done { duration_ms: 1, usage: None }) }
    }

    fn call(b: &Block) -> &ToolCall {
        let Block::Tool(c) = b else { unreachable!() };
        c
    }

    #[test]
    fn escape_leaves_the_details_then_closes_the_map_which_stays_drawn_while_it_fades() {
        let (mut nav, now) = (Nav::default(), Instant::now());
        nav.toggle("a", now);
        nav.show(2, "c2");
        nav.escape(now);
        assert!(nav.is_open("a") && nav.detail.is_none());
        assert_eq!(nav.shown.as_deref(), Some("c2"), "the details stay drawn while they slide out");
        nav.escape(now);
        assert!(!nav.is_open("a") && nav.drawn("a", now));
        assert!(!nav.drawn("a", now + SHUTTING));
    }

    #[test]
    fn reopening_the_map_starts_on_the_graph_with_the_main_agent_picked() {
        let (mut nav, now) = (Nav::default(), Instant::now());
        nav.toggle("a", now);
        nav.show(3, "c3");
        nav.toggle("a", now);
        nav.toggle("a", now);
        assert_eq!((nav.selected, nav.detail.as_deref(), nav.shown.as_deref()), (0, None, None));
    }

    #[test]
    fn the_arrow_keys_stop_at_the_first_and_last_node() {
        let mut nav = Nav::default();
        nav.step(false, 3);
        assert_eq!(nav.selected, 0);
        (0..5).for_each(|_| nav.step(true, 3));
        assert_eq!(nav.selected, 2);
    }

    #[test]
    fn a_node_says_how_long_its_agent_ran_and_the_context_it_used() {
        let mut c = call(&agent("a", ToolStatus::Ok)).clone();
        assert_eq!(meta(&c, 32_900, 0), "27s · 32.9k tokens");
        assert_eq!(meta(&c, 0, 0), "27s");
        c.status = ToolStatus::Running;
        assert_eq!(meta(&c, 640, 16_000), "15s · 640 tokens");
        c.status = ToolStatus::Failed;
        assert_eq!(meta(&c, 9, 0), "Failed after 27s");
        assert_eq!(tokens(1_000), "1.0k tokens");
    }

    #[test]
    fn the_chip_counts_the_agents_still_working_out_of_all() {
        let t = Transcript { turns: vec![turn(vec![agent("a", ToolStatus::Running), agent("b", ToolStatus::Ok), agent("c", ToolStatus::Running)], true)] };
        assert_eq!(chip_label(&chat::spawned(&t)), (true, "2 of 3 agents".into()));
        let done = Transcript { turns: vec![turn(vec![agent("a", ToolStatus::Ok)], true)] };
        assert_eq!(chip_label(&chat::spawned(&done)), (false, "1 agent".into()));
        let all = Transcript { turns: vec![turn(vec![agent("a", ToolStatus::Running), agent("b", ToolStatus::Running)], false)] };
        assert_eq!(chip_label(&chat::spawned(&all)), (true, "2 agents".into()));
        assert_eq!(summary(&chat::spawned(&t)), "3 agents · 2 working");
    }

    #[test]
    fn the_main_agent_hands_off_to_the_agents_its_turn_ends_with() {
        let waiting = Transcript { turns: vec![turn(vec![shell("s"), agent("a", ToolStatus::Running), agent("b", ToolStatus::Running)], false)] };
        assert_eq!(main_line(&waiting, &chat::spawned(&waiting)), "Handed off to 2 agents");
        let on = Transcript { turns: vec![turn(vec![agent("a", ToolStatus::Ok), shell("s")], false)] };
        assert_eq!(main_line(&on, &chat::spawned(&on)), "cargo test");
        let left = Transcript { turns: vec![turn(vec![agent("a", ToolStatus::Running)], true)] };
        assert_eq!(main_line(&left, &chat::spawned(&left)), "Turn ended · agents still working");
        let done = Transcript { turns: vec![turn(vec![agent("a", ToolStatus::Ok)], true)] };
        assert_eq!(main_line(&done, &chat::spawned(&done)), "Done");
    }

    #[test]
    fn a_nested_agent_shows_in_the_chat_at_the_call_its_ancestor_was_spawned_under() {
        let inner = Transcript { turns: vec![turn(vec![agent("inner", ToolStatus::Ok)], true)] };
        let outer = Block::Tool(ToolCall { kind: ToolKind::Agent(SubAgent { transcript: inner, ..Default::default() }), ..call(&agent("outer", ToolStatus::Ok)).clone() });
        let t = Transcript { turns: vec![turn(vec![shell("s")], true), turn(vec![shell("x"), outer], true)] };
        let map = chat::spawned(&t);
        assert_eq!(place(&t, &map, 1), Some((1, 1)));
    }
}
