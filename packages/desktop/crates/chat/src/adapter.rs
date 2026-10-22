mod claude;
mod codex;
mod other;

use crate::commands::{Builtin, Kind};
use crate::{Action, Ask, Fold, Transcript, Turn};

/// What an agent's chat can do; the view hides what it can't.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Capabilities {
    pub interrupt: bool,
    pub compact: bool,
    /// A prompt sent mid-turn waits in the agent's own queue instead of being lost.
    pub send_while_working: bool,
    /// Its spawn tool calls carry a nested transcript.
    pub sub_agents: bool,
    /// It can take what was sent into its turn before its current step ends. Only pocketd says so: an older one can't.
    pub send_now: bool,
}

impl From<agents::providers::ChatCaps> for Capabilities {
    fn from(c: agents::providers::ChatCaps) -> Self {
        Self { interrupt: c.interrupt, compact: c.compact, send_while_working: c.send_while_working, sub_agents: c.sub_agents, send_now: c.send_now }
    }
}

impl Capabilities {
    /// Whether the composer's button may do `action` for this agent.
    pub fn allows(&self, action: Action) -> bool {
        match action {
            Action::Stop => self.interrupt,
            Action::Steer | Action::Queue => self.send_while_working,
            Action::Send => true,
            Action::Stopping => false,
        }
    }
}

/// Turns one agent's raw history into the shared transcript.
pub trait Backend: Sync {
    type Item;
    type Ask;
    fn capabilities(&self) -> Capabilities;
    fn transcript(&self, items: &[Self::Item]) -> Transcript;
    /// Brings `t` up to `transcript(items)`, given `fold` holds what refolding `t` last left, and returns the turns it replaced:
    /// `t`'s last ones. A backend that keeps no `Fold` replaces them all.
    fn refold(&self, items: &[Self::Item], t: &mut Transcript, _fold: &mut Fold) -> Vec<Turn> {
        std::mem::replace(t, self.transcript(items)).turns
    }
    fn ask(&self, ask: &Self::Ask) -> Ask;
    /// What the chat does with command `name` given `arg`. One that opens a panel in the agent's terminal is
    /// unsupported: the chat can't show it, and the next send would land in its keys.
    fn command(&self, _name: &str, _arg: &str) -> Kind {
        Kind::Send
    }
    /// The commands the `/` menu offers before the agent's CLI lists its own.
    fn builtins(&self) -> &'static [Builtin] {
        &[]
    }
    /// Whether `/goal pause` holds the agent's goal without dropping it.
    fn pauses_goal(&self) -> bool {
        false
    }
}

/// The adapter for a pocketd agent, by its `Summary::provider`; a generic one for a provider this build doesn't know.
pub fn for_provider(provider: &str) -> &'static dyn Backend<Item = agents::Item, Ask = agents::Permission> {
    match provider {
        "claude" => &claude::Claude,
        "codex" => &codex::Codex,
        _ => &other::Other,
    }
}

/// What `provider`'s chat can do: what pocketd `said`, else its adapter's own, for a pocketd older than `chat`.
pub fn capabilities(provider: &str, said: Option<agents::providers::ChatCaps>) -> Capabilities {
    said.map_or_else(|| for_provider(provider).capabilities(), Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{Block, End, Message, Row, Session, ToolCall, ToolKind, ToolStatus, Turn};

    #[test]
    fn codex_and_unknown_providers_get_an_adapter() {
        for provider in ["claude", "codex", "gemini"] {
            let can = for_provider(provider).capabilities();
            assert!(can.interrupt && can.compact && can.send_while_working, "{provider}");
            assert_eq!(can.sub_agents, provider != "codex", "{provider}");
        }
    }

    #[test]
    fn pocketd_s_chat_caps_win_over_the_adapter_s() {
        let said = agents::providers::ChatCaps { interrupt: true, ..Default::default() };
        assert_eq!(capabilities("claude", Some(said)), Capabilities { interrupt: true, ..Default::default() });
        assert_eq!(capabilities("codex", None), for_provider("codex").capabilities());
    }

    #[test]
    fn the_composer_blocks_only_what_the_agent_cant_do() {
        let all = for_provider("codex").capabilities();
        assert!([Action::Send, Action::Steer, Action::Queue, Action::Stop].into_iter().all(|a| all.allows(a)));
        let none = Capabilities::default();
        assert_eq!([Action::Send, Action::Steer, Action::Queue, Action::Stop].map(|a| none.allows(a)), [true, false, false, false]);
        assert!(Gemini.capabilities().allows(Action::Stop) && !Gemini.capabilities().allows(Action::Steer));
        assert!(!all.allows(Action::Stopping), "a stop under way has nothing to press");
    }

    /// A third agent, written against the trait alone: its history is plain lines, not pocketd items.
    struct Gemini;

    enum Line {
        User(&'static str),
        Model(&'static str),
        Shell(&'static str, Option<bool>),
        Done,
    }

    struct Confirm(&'static str, &'static str);

    impl Backend for Gemini {
        type Item = Line;
        type Ask = Confirm;

        fn capabilities(&self) -> Capabilities {
            Capabilities { interrupt: true, ..Default::default() }
        }

        fn transcript(&self, items: &[Line]) -> Transcript {
            let mut t = Transcript::default();
            for (i, line) in items.iter().enumerate() {
                let (id, at) = (format!("g{i}"), i as i64);
                let message = |text: &str| Message { id: id.clone(), at, text: text.into() };
                if let Line::User(text) = line {
                    t.turns.push(Turn { id: id.clone(), prompt: Some(message(text)), ..Default::default() });
                    continue;
                }
                let turn = t.turns.last_mut().unwrap();
                match line {
                    Line::Model(text) => turn.blocks.push(Block::Text(message(text))),
                    Line::Shell(command, ok) => {
                        let status = match ok {
                            None => ToolStatus::Running,
                            Some(true) => ToolStatus::Ok,
                            Some(false) => ToolStatus::Failed,
                        };
                        let kind = ToolKind::Shell { command: command.to_string() };
                        turn.blocks.push(Block::Tool(ToolCall { id, name: "run_shell_command".into(), kind, status, output: None, duration_ms: None, at }));
                    }
                    Line::Done => turn.end = Some(End::Done { duration_ms: 0, usage: None }),
                    Line::User(_) => unreachable!(),
                }
            }
            t
        }

        fn ask(&self, c: &Confirm) -> Ask {
            Ask { request_id: c.0.into(), tool: ToolKind::Shell { command: c.1.into() }, tool_name: "run_shell_command".into(), questions: vec![] }
        }
    }

    #[test]
    fn a_third_backend_drives_the_chat_through_the_trait_alone() {
        let backend: &dyn Backend<Item = Line, Ask = Confirm> = &Gemini;
        let mut s = Session::default();
        let mut items = vec![Line::User("fix the build")];
        assert_eq!(s.update(backend, 1, &items, false, 0), Some((0..0, 0..0, 3)));
        assert_eq!(s.ids, ["g0", "g0#fold", "g0#live"]);

        items.extend([Line::Model("Running make."), Line::Shell("make", None)]);
        s.update(backend, 2, &items, false, 0);
        assert_eq!(s.ids, ["g0", "g0#fold", "g0#0.0", "g0#p1", "g0#live"]);
        assert_eq!(s.rows[3], Row::Phase { turn: 0, blocks: 1..2 });

        items[2] = Line::Shell("make", Some(true));
        items.push(Line::Done);
        assert_eq!(s.update(backend, 3, &items, false, 0), Some((3..4, 4..5, 1)), "the finished phase is measured afresh and the end takes the live line's place");
        assert_eq!(s.ids, ["g0", "g0#fold", "g0#0.0", "g0#p1", "g0#end"]);
        assert!(matches!(&s.transcript.turns[0].blocks[1], Block::Tool(c) if c.status == ToolStatus::Ok));

        s.outgoing.push("again".into(), false, 4);
        s.update(backend, 3, &items, false, 4);
        assert_eq!(s.ids[5..], ["pending-0", "working"]);
        items.push(Line::User("again"));
        s.update(backend, 4, &items, false, 5);
        assert_eq!(s.ids[5..], ["g4", "g4#fold", "g4#live"], "the echoed prompt settles the send");
        assert!(s.outgoing.list().is_empty());

        let ask = backend.ask(&Confirm("r1", "rm -rf target"));
        assert_eq!((ask.request_id.as_str(), ask.tool), ("r1", ToolKind::Shell { command: "rm -rf target".into() }));
    }
}
