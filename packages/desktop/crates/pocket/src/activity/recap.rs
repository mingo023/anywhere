use agents::Item;
use std::collections::BTreeSet;

/// What the drawer tells of a session's last turn.
#[derive(Default, Debug, PartialEq)]
pub(crate) struct Recap {
    pub asked: String,
    pub read: usize,
    pub edited: usize,
    pub said: String,
}

/// The last prompt, the files read and edited since, and the last thing the agent said; sub-agents' work is left to their own calls.
pub(crate) fn recap(items: &[Item]) -> Recap {
    let own: Vec<&Item> = items.iter().filter(|i| i.parent_tool_use_id.is_none()).collect();
    let from = own.iter().rposition(|i| i.kind == "user").unwrap_or(0);
    let turn = &own[from.min(own.len())..];
    let paths = |kinds: &[&str]| turn.iter().filter_map(|i| i.call.as_ref()).filter(|c| kinds.contains(&c.detail.kind.as_str())).map(|c| c.detail.path.as_str()).collect::<BTreeSet<_>>().len();
    Recap {
        asked: turn.first().filter(|i| i.kind == "user").map(|i| i.text.clone()).unwrap_or_default(),
        read: paths(&["read"]),
        edited: paths(&["edit", "write"]),
        said: turn.iter().rev().find(|i| i.kind == "assistant").map(|i| i.text.clone()).unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::{Recap, recap};
    use agents::{Call, Detail, Item};

    fn said(kind: &str, text: &str) -> Item {
        Item { kind: kind.into(), text: text.into(), ..Item::default() }
    }

    fn tool(kind: &str, path: &str) -> Item {
        Item { kind: "tool".into(), call: Some(Call { detail: Detail { kind: kind.into(), path: path.into(), ..Detail::default() }, ..Call::default() }), ..Item::default() }
    }

    #[test]
    fn a_recap_tells_only_the_last_turn() {
        let items = [said("user", "old"), tool("read", "a.rs"), said("assistant", "old answer"), said("user", "fix it"), tool("read", "b.rs"), tool("read", "b.rs"), tool("read", "c.rs"), tool("edit", "b.rs"), tool("write", "d.rs"), said("assistant", "thinking"), said("assistant", "Fixed.")];
        assert_eq!(recap(&items), Recap { asked: "fix it".into(), read: 2, edited: 2, said: "Fixed.".into() });
    }

    #[test]
    fn a_sub_agents_work_is_not_the_sessions() {
        let sub = |mut i: Item| {
            i.parent_tool_use_id = Some("task".into());
            i
        };
        let items = [said("user", "go"), sub(tool("read", "x.rs")), sub(said("assistant", "sub says")), said("assistant", "done")];
        assert_eq!(recap(&items), Recap { asked: "go".into(), read: 0, edited: 0, said: "done".into() });
    }

    #[test]
    fn no_timeline_tells_nothing() {
        assert_eq!(recap(&[]), Recap::default());
    }
}
