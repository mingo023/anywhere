//! The sub-agents a conversation spawned, as its agent map shows them.

use crate::{Block, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript};

/// A sub-agent on the map and the call that spawned it.
#[derive(Debug, PartialEq)]
pub struct Spawned<'a> {
    pub call: &'a ToolCall,
    pub agent: &'a SubAgent,
    /// The index of the sub-agent that spawned it; None when the main agent did.
    pub parent: Option<usize>,
}

/// The sub-agents of the latest turn that spawned any, and those of earlier turns still at work, each followed by the
/// ones it spawned.
pub fn spawned(t: &Transcript) -> Vec<Spawned<'_>> {
    let latest = t.turns.iter().rposition(|turn| turn.blocks.iter().any(|b| agent(b).is_some()));
    let mut map = Vec::new();
    for (ix, turn) in t.turns.iter().enumerate() {
        for (call, a) in turn.blocks.iter().filter_map(agent) {
            if Some(ix) == latest || call.status == ToolStatus::Running {
                push(&mut map, call, a, None);
            }
        }
    }
    map
}

fn agent(b: &Block) -> Option<(&ToolCall, &SubAgent)> {
    match b {
        Block::Tool(call) => match &call.kind {
            ToolKind::Agent(a) => Some((call, a)),
            _ => None,
        },
        _ => None,
    }
}

fn push<'a>(map: &mut Vec<Spawned<'a>>, call: &'a ToolCall, agent: &'a SubAgent, parent: Option<usize>) {
    let me = map.len();
    map.push(Spawned { call, agent, parent });
    for (c, a) in agent.transcript.turns.iter().flat_map(|t| &t.blocks).filter_map(self::agent) {
        push(map, c, a, Some(me));
    }
}

#[cfg(test)]
mod tests {
    use super::{Spawned, spawned};
    use crate::{Block, End, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};

    fn agent(id: &str, status: ToolStatus, spawns: Vec<Block>) -> Block {
        let transcript = Transcript { turns: vec![Turn { id: format!("{id}-t"), blocks: spawns, ..Default::default() }] };
        let kind = ToolKind::Agent(SubAgent { description: id.into(), transcript, ..Default::default() });
        Block::Tool(ToolCall { id: id.into(), name: "Agent".into(), kind, status, output: None, duration_ms: None, at: 0 })
    }

    fn shell(id: &str) -> Block {
        Block::Tool(ToolCall { id: id.into(), name: "Bash".into(), kind: ToolKind::Shell { command: "ls".into(), description: String::new() }, status: ToolStatus::Ok, output: None, duration_ms: None, at: 0 })
    }

    fn turn(blocks: Vec<Block>, ended: bool) -> Turn {
        Turn { id: "t".into(), prompt: None, blocks, end: ended.then_some(End::Done { duration_ms: 1, usage: None }) }
    }

    fn ids<'a>(map: &'a [Spawned<'a>]) -> Vec<(&'a str, Option<usize>)> {
        map.iter().map(|s| (s.call.id.as_str(), s.parent)).collect()
    }

    #[test]
    fn each_sub_agent_follows_the_one_that_spawned_it() {
        let t = Transcript { turns: vec![turn(vec![agent("a", ToolStatus::Ok, vec![shell("s"), agent("a1", ToolStatus::Ok, vec![])]), shell("x"), agent("b", ToolStatus::Running, vec![])], false)] };
        assert_eq!(ids(&spawned(&t)), [("a", None), ("a1", Some(0)), ("b", None)]);
    }

    #[test]
    fn the_map_holds_the_latest_turn_that_spawned_and_earlier_agents_still_at_work() {
        let t = Transcript {
            turns: vec![
                turn(vec![agent("old", ToolStatus::Ok, vec![]), agent("bg", ToolStatus::Running, vec![])], true),
                turn(vec![agent("new", ToolStatus::Failed, vec![])], true),
                turn(vec![shell("s")], false),
            ],
        };
        assert_eq!(ids(&spawned(&t)), [("bg", None), ("new", None)]);
    }

    #[test]
    fn a_conversation_that_spawned_nothing_has_an_empty_map() {
        assert!(spawned(&Transcript { turns: vec![turn(vec![shell("s")], true)] }).is_empty());
    }
}
