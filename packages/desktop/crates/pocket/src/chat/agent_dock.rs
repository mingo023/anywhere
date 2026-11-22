use super::agent_detail::{self, Detail};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Screen, id};
use crate::glide::Glide;
use crate::util::now_ms;
use chat::{Block, SubAgent, ToolCall, ToolKind, Transcript};
use gpui_kit::*;
use std::time::Instant;
use theme::*;

const WIDTH: f32 = 400.;

/// The sub-agent calls in a chat, in the order its rows show them.
fn agent_calls(t: &Transcript) -> Vec<(&ToolCall, &SubAgent)> {
    t.turns
        .iter()
        .flat_map(|turn| &turn.blocks)
        .filter_map(|b| match b {
            Block::Tool(c @ ToolCall { kind: ToolKind::Agent(a), .. }) => Some((c, a)),
            _ => None,
        })
        .collect()
}

/// A sub-agent the dock shows: the chat it ran in and its call.
#[derive(Clone, Debug, PartialEq)]
struct Shown {
    agent: String,
    call: String,
}

/// Which sub-agent the dock shows, apart from its motion and focus.
#[derive(Default)]
pub(crate) struct Pick {
    open: Option<Shown>,
    /// The sub-agent last shown, drawn while the dock slides out.
    shown: Option<Shown>,
    all_steps: bool,
    brief_open: bool,
}

impl Pick {
    /// The call the dock shows for `agent`'s chat.
    pub(crate) fn open_in(&self, agent: &str) -> Option<&str> {
        self.open.as_ref().filter(|s| s.agent == agent).map(|s| s.call.as_str())
    }

    /// Shows `call`, or closes the dock when it already shows it.
    fn toggle(&mut self, agent: &str, call: &str) {
        if self.open_in(agent) == Some(call) {
            return self.close();
        }
        self.show(agent, call);
    }

    fn show(&mut self, agent: &str, call: &str) {
        self.open = Some(Shown { agent: agent.into(), call: call.into() });
        self.shown = self.open.clone();
        self.all_steps = false;
        self.brief_open = false;
    }

    fn close(&mut self) {
        self.open = None;
    }

    /// Moves to the next or previous of `calls`, the open chat's sub-agents, stopping at either end; false when it
    /// didn't move.
    fn step(&mut self, down: bool, calls: &[&str]) -> bool {
        let Some(Shown { agent, call }) = self.open.clone() else { return false };
        let Some(at) = calls.iter().position(|c| *c == call) else { return false };
        let to = if down { at + 1 } else { at.wrapping_sub(1) };
        let Some(next) = calls.get(to) else { return false };
        self.show(&agent, next);
        true
    }

    /// Closes the dock when the sub-agent it shows is gone from its chat.
    fn keep_if(&mut self, holds: impl Fn(&str, &str) -> bool) {
        if self.open.as_ref().is_some_and(|s| !holds(&s.agent, &s.call)) {
            self.close();
        }
    }
}

/// Esc shuts the dock only where it shows, on the session on screen, and leaves an open agent map and a pending ask
/// their own Esc.
fn escape_closes(dock_in: Option<&str>, session: Option<&str>, map_open: bool, asking: bool) -> bool {
    dock_in.is_some() && dock_in == session && !map_open && !asking
}

/// The panel at a chat's right edge with one sub-agent's details.
pub(crate) struct AgentDock {
    pub(crate) pick: Pick,
    glide: Glide,
    focus: FocusHandle,
}

impl AgentDock {
    pub(crate) fn new(cx: &mut App) -> Self {
        Self { pick: Pick::default(), glide: Glide::from(0.), focus: cx.focus_handle() }
    }

    /// Whether it holds the keys; it takes only the arrows, so typing still goes to the chat.
    pub(crate) fn focused(&self, window: &Window) -> bool {
        self.focus.is_focused(window)
    }

    /// Slides the dock in or out; true while it still moves.
    pub(crate) fn tick(&mut self, now: Instant, reduce_motion: bool) -> bool {
        let moving = self.glide.tick(if self.pick.open.is_some() { 1. } else { 0. }, now, reduce_motion);
        if !moving && self.pick.open.is_none() {
            self.pick.shown = None;
        }
        moving
    }
}

impl Desktop {
    /// Shows `call` of `agent`'s chat in the dock, taking its keys, or closes the dock when it shows it.
    pub(crate) fn chat_pick_agent(&mut self, agent: &str, call: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.chats.dock.pick.toggle(agent, call);
        if self.chats.dock.pick.open.is_some() {
            self.chats.dock.focus.focus(window, cx);
        } else {
            self.chats.input.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    fn chat_close_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.chats.dock.pick.close();
        self.chats.input.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    fn chat_step_dock(&mut self, down: bool, cx: &mut Context<Self>) -> bool {
        let Some(view) = self.chats.dock.pick.open.as_ref().and_then(|s| self.chats.views.get(&s.agent)) else { return false };
        let calls: Vec<&str> = agent_calls(&view.session.transcript).iter().map(|(c, _)| c.id.as_str()).collect();
        let moved = self.chats.dock.pick.step(down, &calls);
        if moved {
            cx.notify();
        }
        moved
    }

    /// Esc on the session on screen closes its dock; true when it did.
    pub(crate) fn chat_escape_dock(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let session = self.session().filter(|_| self.screen == Screen::Sessions && self.overlay.is_none() && !self.new_form.read(cx).shown);
        let dock_in = self.chats.dock.pick.open.as_ref().map(|s| s.agent.as_str());
        let map_open = dock_in.is_some_and(|a| self.chats.map.nav.is_open(a));
        let asking = dock_in.is_some_and(|a| self.agents.pending.iter().any(|p| p.agent_id == a));
        if !escape_closes(dock_in, session, map_open, asking) {
            return false;
        }
        self.chat_close_dock(window, cx);
        true
    }

    /// Closes the dock once its sub-agent is gone from its chat, or its chat is.
    pub(crate) fn chat_prune_dock(&mut self) {
        let views = &self.chats.views;
        self.chats.dock.pick.keep_if(|agent, call| views.get(agent).is_some_and(|v| agent_calls(&v.session.transcript).iter().any(|(c, _)| c.id == call)));
    }

    /// `agent`'s dock while it shows or slides out: as wide as it has slid in, so the chat beside it narrows.
    pub(crate) fn chat_dock(&self, agent: &str, cx: &mut Context<Self>) -> Option<Stateful<Div>> {
        let dock = &self.chats.dock;
        let shown = dock.pick.shown.as_ref().filter(|s| s.agent == agent)?;
        let t = dock.glide.at().clamp(0., 1.);
        if t < 0.01 {
            return None;
        }
        let view = self.chats.views.get(agent)?;
        let (call, sub) = agent_calls(&view.session.transcript).into_iter().find(|(c, _)| c.id == shown.call)?;
        let key = format!("dock-{agent}");
        let actions = vec![
            agent_detail::action(format!("{key}-up"), "chevron-up", cx.listener(|this, _: &ClickEvent, _, cx| _ = this.chat_step_dock(false, cx))),
            agent_detail::action(format!("{key}-down"), "chevron-down", cx.listener(|this, _: &ClickEvent, _, cx| _ = this.chat_step_dock(true, cx))),
            agent_detail::action(format!("{key}-close"), "x", cx.listener(|this, _: &ClickEvent, window, cx| this.chat_close_dock(window, cx))),
        ];
        let on_brief = cx.listener(|this, _: &ClickEvent, _, cx| {
            this.chats.dock.pick.brief_open ^= true;
            cx.notify();
        });
        let on_more = cx.listener(|this, _: &ClickEvent, _, cx| {
            this.chats.dock.pick.all_steps ^= true;
            cx.notify();
        });
        let links = self.chat_links(agent, cx);
        let detail = Detail { key: key.clone(), call, agent: sub, now: now_ms(), all_steps: dock.pick.all_steps, brief_open: dock.pick.brief_open };
        let body = agent_detail::view(detail, None, actions, on_brief, on_more, &view.tools, &links, cx);
        // Laid out at its full width as it slides, so its text doesn't rewrap every frame.
        let inner = div().absolute().top_0().bottom_0().left_0().w(relative(1. / t)).opacity(t).child(body);
        let focus = dock.focus.clone();
        Some(
            div()
                .id(id(key))
                .relative()
                .flex_none()
                .h_full()
                .w(px(WIDTH * t))
                .max_w(relative(0.5))
                .overflow_hidden()
                .border_l(px(0.5))
                .border_color(SEPARATOR)
                .track_focus(&dock.focus)
                .on_mouse_down(MouseButton::Left, move |_, window, cx| focus.focus(window, cx))
                .on_key_down(cx.listener(|this, e: &KeyDownEvent, _, cx| {
                    let k = &e.keystroke;
                    if (k.key == "down" || k.key == "up") && !k.modifiers.modified() && this.chat_step_dock(k.key == "down", cx) {
                        cx.stop_propagation();
                    }
                }))
                .child(inner),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{Pick, agent_calls, escape_closes};
    use chat::{Block, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};

    fn picked(p: &Pick) -> Option<(&str, &str)> {
        p.open.as_ref().map(|s| (s.agent.as_str(), s.call.as_str()))
    }

    #[test]
    fn a_second_click_on_the_row_shown_closes_the_dock_and_another_row_switches_it() {
        let mut p = Pick::default();
        p.toggle("a", "c1");
        (p.all_steps, p.brief_open) = (true, true);
        p.toggle("a", "c2");
        assert_eq!((picked(&p), p.all_steps, p.brief_open), (Some(("a", "c2")), false, false), "a new sub-agent starts on its newest steps and a short brief");
        p.toggle("a", "c2");
        assert_eq!(picked(&p), None);
        assert_eq!(p.shown.as_ref().map(|s| s.call.as_str()), Some("c2"), "kept while it slides out");
    }

    #[test]
    fn the_dock_shows_only_in_the_chat_it_was_opened_from() {
        let mut p = Pick::default();
        p.toggle("a", "c1");
        assert_eq!((p.open_in("a"), p.open_in("b")), (Some("c1"), None));
        p.toggle("b", "c1");
        assert_eq!((p.open_in("a"), p.open_in("b")), (None, Some("c1")));
    }

    #[test]
    fn the_arrows_walk_the_chats_sub_agents_and_stop_at_either_end() {
        let mut p = Pick::default();
        assert!(!p.step(true, &["c1"]), "nothing open, nothing to move");
        p.toggle("a", "c1");
        assert!(!p.step(false, &["c1", "c2"]));
        assert!(p.step(true, &["c1", "c2"]));
        assert_eq!(picked(&p), Some(("a", "c2")));
        assert!(!p.step(true, &["c1", "c2"]));
        assert!(!p.step(true, &["c3"]), "a sub-agent no longer listed moves nowhere");
    }

    #[test]
    fn the_dock_closes_once_its_sub_agent_leaves_the_chat() {
        let mut p = Pick::default();
        p.toggle("a", "c1");
        p.keep_if(|agent, call| (agent, call) == ("a", "c1"));
        assert!(picked(&p).is_some());
        p.keep_if(|_, _| false);
        assert_eq!(picked(&p), None);
    }

    #[test]
    fn esc_closes_the_dock_only_on_its_own_session_and_yields_to_the_map_and_asks() {
        assert!(escape_closes(Some("a"), Some("a"), false, false));
        assert!(!escape_closes(Some("a"), Some("b"), false, false), "a dock off screen keeps Esc from swallowing it");
        assert!(!escape_closes(None, Some("a"), false, false));
        assert!(!escape_closes(Some("a"), Some("a"), true, false));
        assert!(!escape_closes(Some("a"), Some("a"), false, true));
    }

    #[test]
    fn a_chats_sub_agents_are_its_agent_calls_across_every_turn() {
        let call =
            |id: &str, kind| Block::Tool(ToolCall { id: id.into(), name: "x".into(), kind, status: ToolStatus::Ok, output: None, duration_ms: None, at: 0 });
        let turn = |blocks| Turn { id: "t".into(), prompt: None, blocks, end: None };
        let t = Transcript {
            turns: vec![
                turn(vec![call("a1", ToolKind::Agent(SubAgent::default())), call("s", ToolKind::Shell { command: "ls".into(), description: String::new() })]),
                turn(vec![call("a2", ToolKind::Agent(SubAgent::default()))]),
            ],
        };
        assert_eq!(agent_calls(&t).iter().map(|(c, _)| c.id.as_str()).collect::<Vec<_>>(), ["a1", "a2"]);
    }
}
