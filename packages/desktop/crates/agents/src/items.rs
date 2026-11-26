use crate::Question;
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Item {
    pub id: String,
    pub seq: i64,
    pub ts: i64,
    pub kind: String,
    pub text: String,
    pub error: String,
    pub duration_ms: i64,
    pub usage: Option<Usage>,
    pub call: Option<Call>,
    /// A result's: whether the turn succeeded.
    pub ok: bool,
    /// A tasks item's todos.
    pub items: Vec<TaskItem>,
    /// A compact's: "manual" or "auto".
    pub trigger: String,
    /// A delivery's: the sessions, titles and agents pocketd told the chat of, side by side.
    pub child_ids: Vec<String>,
    pub titles: Vec<String>,
    pub agents: Vec<String>,
    /// A delivery's: "done", "failed" or "asks".
    pub outcome: String,
    /// A delivery's: what the child asks, when `outcome` is "asks".
    pub question: String,
    /// A host_link's: what to fill the add-host form with.
    pub hint: String,
    /// A host_link's: why the agent wants the host.
    pub reason: String,
    /// The tool-use id of the sub-agent call this item belongs to.
    pub parent_tool_use_id: Option<String>,
    /// Who wrote a user item pocketd sent itself: "wake", "question" or "context". Empty on the user's own.
    pub origin: String,
}

impl Item {
    /// Whether this is a prompt to show: a wake is told by the delivery before it instead.
    pub fn is_prompt(&self) -> bool {
        self.kind == "user" && self.origin != "wake"
    }
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Call {
    pub tool_use_id: String,
    pub name: String,
    /// "running", "ok" or "error".
    pub status: String,
    pub output: Option<String>,
    pub duration_ms: Option<u64>,
    pub detail: Detail,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Usage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cache_read_tokens: u64,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct TaskItem {
    pub text: String,
    /// "pending", "in_progress" or "completed".
    pub status: String,
}

/// A preview of an edit or write: pocketd keeps at most 24 lines, but counts the whole diff.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct FileDiff {
    pub lines: Vec<DiffLine>,
    pub additions: u32,
    pub deletions: u32,
    /// A write with no prior contents, so every line reads as added.
    pub content_only: bool,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct DiffLine {
    pub number: Option<u32>,
    /// "add", "del" or "context".
    pub kind: String,
    pub text: String,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Detail {
    pub kind: String,
    pub command: String,
    pub path: String,
    pub query: String,
    pub description: String,
    /// A `task`'s model: the alias it was called with, then the full id once it replies.
    pub model: String,
    /// A `task`'s: the context its sub-agent last answered from.
    pub tokens: u64,
    /// A workflow agent's `task`: the phase of its workflow it runs in.
    pub phase: String,
    pub name: String,
    pub diff: Option<FileDiff>,
    /// An `other` tool's whole input.
    pub input: Value,
}

impl Detail {
    /// AskUserQuestion's, from the tool's input; an input shaped otherwise has none.
    pub fn questions(&self) -> Vec<Question> {
        self.input.get("questions").and_then(|q| Vec::deserialize(q).ok()).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Frame;
    use serde_json::json;

    fn streamed(golden: Value) -> Item {
        serde_json::from_value::<Frame>(golden).unwrap().item.unwrap()
    }

    #[test]
    fn sub_agent_items_decode_with_the_call_that_spawned_them() {
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/stream_subagent.json")).unwrap();
        assert_eq!(f.item.unwrap().parent_tool_use_id.as_deref(), Some("t4"));
        let f: Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/timeline_subagents.json")).unwrap();
        let parents: Vec<_> = f.items.iter().map(|i| i.parent_tool_use_id.as_deref()).collect();
        assert_eq!(
            parents,
            [None, None, None, Some("toolu_list"), Some("toolu_list"), Some("toolu_list"), Some("toolu_count"), Some("toolu_count"), None, None]
        );
    }

    #[test]
    fn a_tool_input_shaped_unlike_questions_still_reads() {
        let d: Detail = serde_json::from_value(json!({"kind": "other", "name": "mcp", "input": {"questions": "free text"}})).unwrap();
        assert_eq!((d.name.as_str(), d.questions().len()), ("mcp", 0));
        let d: Detail = serde_json::from_value(json!({"kind": "other", "input": null})).unwrap();
        assert!(d.questions().is_empty());
    }

    #[test]
    fn a_finished_tool_keeps_its_use_id_status_output_and_duration() {
        let call = streamed(golden!("server/stream_tool_done.json")).call.unwrap();
        assert_eq!((call.tool_use_id.as_str(), call.name.as_str(), call.status.as_str()), ("t2", "Edit", "ok"));
        assert_eq!((call.output.as_deref(), call.duration_ms), (Some("done"), Some(5)));
    }

    #[test]
    fn an_edit_keeps_its_diff_lines_and_counts() {
        let diff = streamed(golden!("server/stream_tool_done.json")).call.unwrap().detail.diff.unwrap();
        let line = |kind: &str, text: &str| DiffLine { number: Some(1), kind: kind.into(), text: text.into() };
        assert_eq!(diff, FileDiff { lines: vec![line("del", "a"), line("add", "b")], additions: 1, deletions: 1, content_only: false });
    }

    #[test]
    fn a_tasks_item_keeps_its_todos() {
        assert_eq!(streamed(golden!("server/stream_tasks.json")).items, vec![TaskItem { text: "x".into(), status: "in_progress".into() }]);
    }

    #[test]
    fn a_result_keeps_whether_it_succeeded_and_its_output_tokens() {
        let failed = streamed(golden!("server/stream_result.json"));
        assert_eq!((failed.ok, failed.error.as_str()), (false, "interrupted"));
        let f: Frame = serde_json::from_value(golden!("server/timeline.json")).unwrap();
        let done = &f.items[1];
        assert_eq!((done.ok, done.usage.as_ref().map(|u| u.output_tokens)), (true, Some(2)));
    }

    #[test]
    fn a_compact_keeps_its_trigger() {
        assert_eq!(streamed(golden!("server/stream_compact.json")).trigger, "manual");
    }

    #[test]
    fn an_ask_user_question_still_reads_its_questions_from_input() {
        let item: Item = serde_json::from_value(json!({"kind": "tool", "id": "i1", "call": {"toolUseId": "t1", "name": "AskUserQuestion", "status": "running",
            "detail": {"kind": "other", "name": "AskUserQuestion", "input": {"questions": [{"question": "Which color?", "header": "Color", "options": [{"label": "Red"}]}]}}}}))
        .unwrap();
        let detail = item.call.unwrap().detail;
        assert_eq!(detail.questions()[0].question, "Which color?");
        assert_eq!(detail.input["questions"][0]["header"], "Color");
    }
}
