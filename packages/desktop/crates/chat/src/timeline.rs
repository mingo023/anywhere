use crate::{
    Ask, Block, Choice, Diff, DiffLine, End, LineKind, Message, Question, SubAgent, Todo, TodoStatus, ToolCall, ToolKind, ToolStatus,
    Transcript, Turn, Usage,
};
use agents::{Call, Detail, FileDiff, Item, Permission};
use serde_json::Value;
use std::collections::HashMap;

/// Folds pocketd timeline items, in seq order, into turns:
/// - `user` starts a turn, and `result` ends it.
/// - `assistant` → `Block::Text`, `thinking` → `Block::Thinking`, `tasks` → `Block::Todos`, `plan` → `Block::Plan`.
/// - `compact` → `Block::Compacted`.
/// - `notice` → `Block::Notice`: its `text`, else the `error` the agent went on after.
/// - `tool` → `Block::Tool`, refreshed in place by `toolUseId`.
/// - `task` detail → `ToolKind::Agent`. `other` named WebFetch/WebSearch → Fetch/Search.
/// - AskUserQuestion → `ToolKind::Question`.
/// - Items carrying `parent_tool_use_id` fold into that tool's `SubAgent::transcript`.
pub(crate) fn fold(items: &[Item]) -> Transcript {
    let mut ordered: Vec<&Item> = items.iter().collect();
    if !items.is_sorted_by_key(|i| i.seq) {
        ordered.sort_by_key(|i| i.seq);
    }
    let mut children: HashMap<&str, Vec<&Item>> = HashMap::new();
    let mut top = Vec::new();
    for item in ordered {
        match parent(item) {
            Some(parent) => children.entry(parent).or_default().push(item),
            None => top.push(item),
        }
    }
    fold_level(&top, &mut children)
}

/// What the last fold of a timeline left, so the next refolds only the turns that can have changed.
#[derive(Default)]
pub struct Fold {
    /// The items last folded.
    items: Vec<Item>,
    /// The index in `items` of the item that began each turn.
    starts: Vec<usize>,
    /// Top-level calls by use id: (turn, block).
    tools: HashMap<String, (usize, usize)>,
    /// Sub-agent calls at any depth by use id, with the top-level turn holding them.
    agents: HashMap<String, usize>,
}

impl Fold {
    /// Brings `t`, which holds this fold's last result, up to `fold(items)`, and returns the turns it replaced: `t`'s last ones.
    pub(crate) fn refold(&mut self, items: &[Item], t: &mut Transcript) -> Vec<Turn> {
        if !items.is_sorted_by_key(|i| i.seq) {
            *self = Fold::default();
            return std::mem::replace(t, fold(items)).turns;
        }
        let same = self.items.iter().zip(items).take_while(|(a, b)| a == b).count();
        if same == items.len() && same == self.items.len() {
            return Vec::new();
        }
        let from = self.resumable(items, same).unwrap_or(0);
        let start = if from == 0 { 0 } else { self.starts[from] };
        let replaced = t.turns.split_off(from);
        self.starts.truncate(from);
        self.tools.retain(|_, (turn, _)| *turn < from);
        self.agents.retain(|_, turn| *turn < from);
        let mut children: HashMap<&str, Vec<&Item>> = HashMap::new();
        for (i, item) in items.iter().enumerate() {
            match parent(item) {
                // Kept turns already hold what unchanged items did to them.
                Some(parent) if self.agents.contains_key(parent) => {}
                Some(parent) => children.entry(parent).or_default().push(item),
                None if i >= start && !self.kept_call(item, from) => {
                    let turns = t.turns.len();
                    step(t, &mut self.tools, item);
                    if t.turns.len() > turns {
                        self.starts.push(i);
                    }
                }
                None => {}
            }
        }
        claim(&mut t.turns[from..], &mut children);
        for (ix, turn) in t.turns.iter().enumerate().skip(from) {
            record_agents(&turn.blocks, ix, &mut self.agents);
        }
        self.items.truncate(same);
        self.items.extend_from_slice(&items[same..]);
        replaced
    }

    /// The turn holding item `same`, the first that changed, unless a changed item, old or new, refreshes a call
    /// or feeds a sub-agent in an earlier turn.
    fn resumable(&self, items: &[Item], same: usize) -> Option<usize> {
        let from = self.starts.partition_point(|&s| s < same).checked_sub(1)?;
        let reaches_back = |item: &Item| match parent(item) {
            Some(parent) => self.agents.get(parent).is_some_and(|&turn| turn < from),
            None => self.kept_call(item, from),
        };
        (!items[same..].iter().chain(&self.items[same..]).any(reaches_back)).then_some(from)
    }

    /// Whether `item` refreshes a call in a turn before `from`.
    fn kept_call(&self, item: &Item, from: usize) -> bool {
        item.call.as_ref().filter(|_| item.kind == "tool").and_then(|c| self.tools.get(tool_id(item, c))).is_some_and(|&(turn, _)| turn < from)
    }
}

fn parent(item: &Item) -> Option<&str> {
    item.parent_tool_use_id.as_deref().filter(|p| !p.is_empty())
}

fn fold_level<'a>(items: &[&'a Item], children: &mut HashMap<&'a str, Vec<&'a Item>>) -> Transcript {
    let mut t = Transcript::default();
    let mut tools = HashMap::new();
    for &item in items {
        step(&mut t, &mut tools, item);
    }
    claim(&mut t.turns, children);
    t
}

/// Folds one item into `t`; `tools` finds each call (turn, block) by use id, so a later item refreshes it in place.
fn step(t: &mut Transcript, tools: &mut HashMap<String, (usize, usize)>, item: &Item) {
    match item.kind.as_str() {
        "user" => t.turns.push(Turn { id: item.id.clone(), prompt: Some(message(item)), ..Turn::default() }),
        "result" => open(t, item).end = Some(end(item)),
        "tool" => {
            let Some(call) = &item.call else { return };
            let id = tool_id(item, call);
            let block = Block::Tool(tool(id, item, call));
            if let Some(&(turn, ix)) = tools.get(id) {
                t.turns[turn].blocks[ix] = block;
                return;
            }
            let turn = open(t, item);
            turn.blocks.push(block);
            let ix = turn.blocks.len() - 1;
            tools.insert(id.into(), (t.turns.len() - 1, ix));
        }
        _ => {
            if let Some(block) = block(item) {
                open(t, item).blocks.push(block);
            }
        }
    }
}

fn tool_id<'a>(item: &'a Item, call: &'a Call) -> &'a str {
    if call.tool_use_id.is_empty() { &item.id } else { &call.tool_use_id }
}

/// Each sub-agent call takes its items out of `children` as it folds them, so a parent cycle can't recurse forever.
fn claim<'a>(turns: &mut [Turn], children: &mut HashMap<&'a str, Vec<&'a Item>>) {
    for turn in turns {
        for block in &mut turn.blocks {
            if let Block::Tool(ToolCall { id, kind: ToolKind::Agent(sub), .. }) = block
                && let Some(kids) = children.remove(id.as_str())
            {
                sub.transcript = fold_level(&kids, children);
            }
        }
    }
}

/// Notes each sub-agent call in `blocks`, at any depth and in the order `claim` reaches them, as held by top-level turn `turn`.
fn record_agents(blocks: &[Block], turn: usize, agents: &mut HashMap<String, usize>) {
    for block in blocks {
        if let Block::Tool(ToolCall { id, kind: ToolKind::Agent(sub), .. }) = block {
            if !agents.contains_key(id) {
                agents.insert(id.clone(), turn);
            }
            for t in &sub.transcript.turns {
                record_agents(&t.blocks, turn, agents);
            }
        }
    }
}

/// The turn still under way, or a new one the agent began itself.
fn open<'t>(t: &'t mut Transcript, item: &Item) -> &'t mut Turn {
    if t.turns.last().is_none_or(|turn| turn.end.is_some()) {
        t.turns.push(Turn { id: item.id.clone(), ..Turn::default() });
    }
    t.turns.last_mut().expect("just pushed")
}

fn message(item: &Item) -> Message {
    Message { id: item.id.clone(), at: item.ts, text: item.text.clone() }
}

/// None for an item the chat doesn't show, a blank message among them.
fn block(item: &Item) -> Option<Block> {
    let blank = item.text.trim().is_empty();
    Some(match item.kind.as_str() {
        "assistant" if !blank => Block::Text(message(item)),
        "thinking" if !blank => Block::Thinking(message(item)),
        "plan" if !blank => Block::Plan(message(item)),
        "tasks" => Block::Todos { id: item.id.clone(), todos: item.items.iter().map(|t| Todo { text: t.text.clone(), status: todo_status(&t.status) }).collect() },
        "compact" => Block::Compacted { id: item.id.clone(), auto: item.trigger == "auto" },
        "notice" if !item.error.is_empty() => Block::Notice { message: Message { text: item.error.clone(), ..message(item) }, error: true },
        "notice" if !blank => Block::Notice { message: message(item), error: false },
        _ => return None,
    })
}

fn todo_status(status: &str) -> TodoStatus {
    match status {
        "in_progress" => TodoStatus::Active,
        "completed" => TodoStatus::Done,
        _ => TodoStatus::Pending,
    }
}

fn end(item: &Item) -> End {
    if !item.ok {
        return End::Failed { error: item.error.clone(), duration_ms: item.duration_ms.max(0) as u64 };
    }
    let usage = item.usage.as_ref().map(|u| Usage { input: u.input_tokens, output: u.output_tokens, cache_read: u.cache_read_tokens });
    End::Done { duration_ms: item.duration_ms.max(0) as u64, usage }
}

fn tool(id: &str, item: &Item, call: &Call) -> ToolCall {
    let name = if call.name.is_empty() { &call.detail.name } else { &call.name };
    let status = match call.status.as_str() {
        "ok" => ToolStatus::Ok,
        "error" => ToolStatus::Failed,
        _ => ToolStatus::Running,
    };
    ToolCall { id: id.into(), name: name.clone(), kind: kind(name, &call.detail), status, output: call.output.clone(), duration_ms: call.duration_ms, at: item.ts }
}

fn kind(name: &str, d: &Detail) -> ToolKind {
    match d.kind.as_str() {
        "shell" => ToolKind::Shell { command: d.command.clone(), description: d.description.clone() },
        "read" => ToolKind::Read { path: d.path.clone() },
        "edit" => ToolKind::Edit { path: d.path.clone(), diff: d.diff.as_ref().map(diff) },
        "write" => ToolKind::Write { path: d.path.clone(), diff: d.diff.as_ref().map(diff) },
        "search" if d.path.is_empty() => ToolKind::Search { query: d.query.clone() },
        "search" => ToolKind::Search { query: format!("{} in {}", d.query, d.path) },
        "task" => ToolKind::Agent(SubAgent { description: d.description.clone(), model: d.model.clone(), tokens: d.tokens, ..Default::default() }),
        _ if name == "AskUserQuestion" => ToolKind::Question(questions(d.questions())),
        _ if name == "WebFetch" => ToolKind::Fetch { url: input_text(&d.input, "url") },
        _ if name == "WebSearch" => ToolKind::Search { query: input_text(&d.input, "query") },
        _ if d.input.is_null() => ToolKind::Other { input: String::new(), description: d.description.clone() },
        _ => ToolKind::Other { input: serde_json::to_string_pretty(&d.input).unwrap_or_default(), description: d.description.clone() },
    }
}

fn input_text(input: &Value, key: &str) -> String {
    input.get(key).and_then(Value::as_str).unwrap_or_default().into()
}

fn diff(d: &FileDiff) -> Diff {
    let line = |l: &agents::DiffLine| DiffLine {
        number: l.number,
        kind: match l.kind.as_str() {
            "add" => LineKind::Add,
            "del" => LineKind::Del,
            _ => LineKind::Context,
        },
        text: l.text.clone(),
    };
    Diff { lines: d.lines.iter().map(line).collect(), additions: d.additions, deletions: d.deletions }
}

fn questions(qs: Vec<agents::Question>) -> Vec<Question> {
    qs.into_iter()
        .map(|q| Question {
            header: q.header,
            question: q.question,
            options: q.options.into_iter().map(|c| Choice { label: c.label, description: c.description }).collect(),
            multi: q.multi_select,
        })
        .collect()
}

pub(crate) fn ask(p: &Permission) -> Ask {
    Ask { request_id: p.request_id.clone(), tool: kind(&p.tool_name, &p.detail), tool_name: p.tool_name.clone(), questions: questions(p.questions()) }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    /// Items in the order given, each numbered by its place unless it says otherwise.
    fn items(v: Value) -> Vec<Item> {
        let Value::Array(list) = v else { panic!("an array") };
        list.into_iter()
            .enumerate()
            .map(|(i, mut v)| {
                let n = i as i64 + 1;
                v.as_object_mut().unwrap().entry("seq").or_insert(json!(n));
                v.as_object_mut().unwrap().entry("id").or_insert(json!(format!("i{n}")));
                v["ts"] = json!(n * 1000);
                serde_json::from_value(v).unwrap()
            })
            .collect()
    }

    fn tool(id: &str, name: &str, detail: Value, status: &str) -> Value {
        json!({"kind": "tool", "call": {"toolUseId": id, "name": name, "detail": detail, "status": status}})
    }

    fn only_tool(t: &Transcript) -> &ToolCall {
        match t.turns[0].blocks.as_slice() {
            [Block::Tool(call)] => call,
            other => panic!("one tool, got {other:?}"),
        }
    }

    #[test]
    fn a_user_item_starts_a_turn_and_a_result_ends_it() {
        let t = fold(&items(json!([
            {"kind": "user", "text": "hi"},
            {"kind": "assistant", "text": "hello"},
            {"kind": "result", "ok": true, "durationMs": 5, "usage": {"inputTokens": 1, "outputTokens": 2, "cacheReadTokens": 3}},
            {"kind": "user", "text": "again"},
        ])));
        assert_eq!(t.turns.len(), 2);
        let first = &t.turns[0];
        assert_eq!(first.id, "i1");
        assert_eq!(first.prompt, Some(Message { id: "i1".into(), at: 1000, text: "hi".into() }));
        assert_eq!(first.blocks, vec![Block::Text(Message { id: "i2".into(), at: 2000, text: "hello".into() })]);
        assert_eq!(first.end, Some(End::Done { duration_ms: 5, usage: Some(Usage { input: 1, output: 2, cache_read: 3 }) }));
        assert_eq!(t.turns[1].prompt.as_ref().map(|m| m.text.as_str()), Some("again"));
        assert_eq!(t.turns[1].end, None);
    }

    #[test]
    fn items_before_the_first_prompt_form_a_turn_without_one() {
        let t = fold(&items(json!([
            {"kind": "assistant", "text": "picking up"},
            {"kind": "result", "ok": true},
            {"kind": "compact", "trigger": "auto"},
            {"kind": "user", "text": "next"},
        ])));
        assert_eq!(t.turns.len(), 3);
        assert_eq!((t.turns[0].id.as_str(), &t.turns[0].prompt), ("i1", &None));
        assert!(t.turns[0].end.is_some());
        assert_eq!((t.turns[1].id.as_str(), &t.turns[1].prompt), ("i3", &None));
        assert_eq!(t.turns[1].blocks, vec![Block::Compacted { id: "i3".into(), auto: true }]);
        assert!(t.turns[2].prompt.is_some());
    }

    #[test]
    fn a_tool_update_replaces_the_running_call_with_the_same_use_id() {
        let t = fold(&items(json!([
            {"kind": "user", "text": "go"},
            tool("tu1", "Bash", json!({"kind": "shell", "command": "ls"}), "running"),
            {"kind": "assistant", "text": "meanwhile"},
            {"kind": "tool", "call": {"toolUseId": "tu1", "name": "Bash", "detail": {"kind": "shell", "command": "ls"}, "status": "ok", "output": "a\nb", "durationMs": 7}},
        ])));
        let blocks = &t.turns[0].blocks;
        assert_eq!(blocks.len(), 2);
        let Block::Tool(call) = &blocks[0] else { panic!("the tool keeps its place") };
        assert_eq!((call.id.as_str(), call.status, call.output.as_deref(), call.duration_ms), ("tu1", ToolStatus::Ok, Some("a\nb"), Some(7)));
        assert_eq!(call.kind, ToolKind::Shell { command: "ls".into(), description: String::new() });
    }

    #[test]
    fn a_command_keeps_what_the_agent_said_it_does() {
        let t = fold(&items(json!([tool("tu1", "Bash", json!({"kind": "shell", "command": "go test ./...", "description": "Run the Go tests"}), "ok")])));
        let Block::Tool(call) = &t.turns[0].blocks[0] else { panic!("a tool call") };
        assert_eq!(call.kind, ToolKind::Shell { command: "go test ./...".into(), description: "Run the Go tests".into() });
    }

    #[test]
    fn a_tool_end_after_a_new_prompt_updates_the_call_in_its_own_turn() {
        let t = fold(&items(json!([
            {"kind": "user", "text": "one"},
            tool("tu1", "Bash", json!({"kind": "shell", "command": "make"}), "running"),
            {"kind": "user", "text": "two"},
            tool("tu1", "Bash", json!({"kind": "shell", "command": "make"}), "error"),
        ])));
        let Block::Tool(call) = &t.turns[0].blocks[0] else { panic!("a tool") };
        assert_eq!(call.status, ToolStatus::Failed);
        assert!(t.turns[1].blocks.is_empty());
    }

    #[test]
    fn tools_without_a_use_id_never_replace_each_other() {
        let t = fold(&items(json!([
            tool("", "Bash", json!({"kind": "shell", "command": "a"}), "ok"),
            tool("", "Bash", json!({"kind": "shell", "command": "b"}), "ok"),
        ])));
        assert_eq!(t.turns[0].blocks.len(), 2);
    }

    #[test]
    fn a_failed_result_ends_the_turn_with_its_error() {
        let t = fold(&items(json!([
            {"kind": "user", "text": "deploy"},
            {"kind": "result", "ok": false, "durationMs": 9000, "error": "API Error: 529 Overloaded"},
        ])));
        assert_eq!(t.turns[0].end, Some(End::Failed { error: "API Error: 529 Overloaded".into(), duration_ms: 9000 }));
    }

    #[test]
    fn a_result_with_no_prompt_ends_a_turn_of_its_own() {
        let t = fold(&items(json!([
            {"kind": "user", "text": "stop"},
            {"kind": "result", "ok": true},
            {"kind": "result", "ok": false, "error": "interrupted"},
        ])));
        assert_eq!(t.turns.len(), 2);
        assert_eq!(t.turns[1].prompt, None);
        assert_eq!(t.turns[1].end, Some(End::Failed { error: "interrupted".into(), duration_ms: 0 }));
    }

    #[test]
    fn items_out_of_seq_order_fold_by_seq() {
        let t = fold(&items(json!([
            {"kind": "assistant", "text": "reply", "seq": 2},
            {"kind": "user", "text": "ask", "seq": 1},
        ])));
        assert_eq!(t.turns.len(), 1);
        assert_eq!(t.turns[0].prompt.as_ref().map(|m| m.text.as_str()), Some("ask"));
        assert_eq!(t.turns[0].blocks.len(), 1);
    }

    #[test]
    fn blank_text_and_thinking_are_left_out() {
        let t = fold(&items(json!([
            {"kind": "user", "text": "hi"},
            {"kind": "thinking", "text": "  "},
            {"kind": "assistant", "text": "\n"},
        ])));
        assert!(t.turns[0].blocks.is_empty());
    }

    #[test]
    fn webfetch_and_websearch_become_fetch_and_search() {
        let t = fold(&items(json!([
            tool("f", "WebFetch", json!({"kind": "other", "name": "WebFetch", "input": {"url": "https://x.dev", "prompt": "why"}}), "ok"),
            tool("s", "WebSearch", json!({"kind": "other", "name": "WebSearch", "input": {"query": "gpui list"}}), "ok"),
            tool("g", "Grep", json!({"kind": "search", "query": "fn group", "path": "src"}), "ok"),
            tool("m", "mcp__linear__create", json!({"kind": "other", "name": "mcp__linear__create", "input": {"team": "Desktop"}}), "ok"),
        ])));
        let kinds: Vec<_> = t.turns[0]
            .blocks
            .iter()
            .map(|b| match b {
                Block::Tool(c) => c.kind.clone(),
                other => panic!("{other:?}"),
            })
            .collect();
        assert_eq!(kinds[0], ToolKind::Fetch { url: "https://x.dev".into() });
        assert_eq!(kinds[1], ToolKind::Search { query: "gpui list".into() });
        assert_eq!(kinds[2], ToolKind::Search { query: "fn group in src".into() });
        assert_eq!(kinds[3], ToolKind::Other { input: "{\n  \"team\": \"Desktop\"\n}".into(), description: String::new() });
    }

    #[test]
    fn another_tool_keeps_what_pocketd_says_it_is_about() {
        let t = fold(&items(json!([tool("s", "TaskStop", json!({"kind": "other", "name": "TaskStop", "input": {"task_id": "a1"}, "description": "Audit"}), "ok")])));
        let Block::Tool(c) = &t.turns[0].blocks[0] else { panic!() };
        assert_eq!(c.kind, ToolKind::Other { input: "{\n  \"task_id\": \"a1\"\n}".into(), description: "Audit".into() });
    }

    #[test]
    fn ask_user_question_becomes_a_question_tool() {
        let input = json!({"questions": [{"question": "Which parser?", "header": "Parser", "multiSelect": true,
            "options": [{"label": "comrak", "description": "GFM"}]}]});
        let t = fold(&items(json!([tool("q", "AskUserQuestion", json!({"kind": "other", "name": "AskUserQuestion", "input": input}), "running")])));
        let question = Question {
            header: "Parser".into(),
            question: "Which parser?".into(),
            options: vec![Choice { label: "comrak".into(), description: "GFM".into() }],
            multi: true,
        };
        assert_eq!(only_tool(&t).kind, ToolKind::Question(vec![question]));
    }

    #[test]
    fn a_task_call_becomes_a_sub_agent() {
        let t = fold(&items(json!([tool("tu-t", "Task", json!({"kind": "task", "description": "Audit packages/app"}), "running")])));
        let call = only_tool(&t);
        assert_eq!(call.kind, ToolKind::Agent(SubAgent { description: "Audit packages/app".into(), ..Default::default() }));
        assert_eq!(call.status, ToolStatus::Running);
    }

    #[test]
    fn a_task_call_carries_the_context_its_sub_agent_answered_from() {
        let t = fold(&items(json!([tool("tu-t", "Task", json!({"kind": "task", "description": "Look", "model": "claude-haiku-5-5", "tokens": 32900}), "running")])));
        let ToolKind::Agent(agent) = &only_tool(&t).kind else { panic!("not an agent") };
        assert_eq!((agent.model.as_str(), agent.tokens), ("claude-haiku-5-5", 32900));
    }

    fn sub_agent(call: &ToolCall) -> &Transcript {
        match &call.kind {
            ToolKind::Agent(sub) => &sub.transcript,
            other => panic!("a sub-agent, got {other:?}"),
        }
    }

    #[test]
    fn items_with_a_parent_fold_into_their_sub_agents_transcript() {
        let t = fold(&items(json!([
            {"kind": "user", "text": "audit"},
            tool("tu-t", "Task", json!({"kind": "task", "description": "Audit"}), "running"),
            {"kind": "tool", "parentToolUseId": "tu-t", "call": {"toolUseId": "tu-c", "name": "Read", "detail": {"kind": "read", "path": "Cargo.toml"}, "status": "ok"}},
            {"kind": "assistant", "text": "back in the parent"},
            {"kind": "assistant", "text": "unused crates", "parentToolUseId": "tu-t"},
        ])));
        let blocks = &t.turns[0].blocks;
        assert_eq!(blocks.len(), 2);
        let Block::Tool(spawn) = &blocks[0] else { panic!("the spawn") };
        let sub = sub_agent(spawn);
        assert_eq!(sub.turns.len(), 1);
        assert_eq!(sub.turns[0].prompt, None);
        assert!(matches!(&sub.turns[0].blocks[..], [Block::Tool(c), Block::Text(m)] if c.id == "tu-c" && m.text == "unused crates"));
    }

    #[test]
    fn a_sub_agents_own_sub_agent_nests_two_deep() {
        let t = fold(&items(json!([
            tool("a", "Task", json!({"kind": "task", "description": "outer"}), "running"),
            {"kind": "tool", "parentToolUseId": "a", "call": {"toolUseId": "b", "name": "Task", "detail": {"kind": "task", "description": "inner"}, "status": "running"}},
            {"kind": "assistant", "text": "deep", "parentToolUseId": "b"},
        ])));
        let Block::Tool(inner) = &sub_agent(only_tool(&t)).turns[0].blocks[0] else { panic!("the inner spawn") };
        assert!(matches!(&sub_agent(inner).turns[0].blocks[..], [Block::Text(m)] if m.text == "deep"));
    }

    #[test]
    fn a_claude_session_pocketd_paged_nests_each_sub_agent_under_its_own_agent_call() {
        let golden: Value = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/timeline_subagents.json")).unwrap();
        let t = fold(&serde_json::from_value::<Vec<Item>>(golden["items"].clone()).unwrap());
        assert_eq!(t.turns.len(), 1);
        let [Block::Tool(list), Block::Tool(count), Block::Text(answer)] = &t.turns[0].blocks[..] else { panic!("{:?}", t.turns[0].blocks) };
        assert!(answer.text.starts_with("The repo holds"));
        assert!(matches!(&list.kind, ToolKind::Agent(a) if a.model == "claude-haiku-4-5-20251001"), "the model it replied as");
        let list = sub_agent(list);
        assert_eq!(list.turns[0].prompt.as_ref().map(|m| m.text.as_str()), Some("Run ls in /w and report what is there"));
        assert!(matches!(&list.turns[0].blocks[..], [Block::Tool(ToolCall { kind: ToolKind::Shell { command, .. }, status: ToolStatus::Ok, .. }), Block::Text(m)]
            if command == "ls /w" && m.text == "README.md and src/"));
        let count = sub_agent(count);
        assert!(matches!(&count.turns[0].blocks[..], [Block::Text(m)] if m.text == "1, 2, 3"));
    }

    #[test]
    fn items_whose_parent_is_no_sub_agent_are_left_out() {
        let t = fold(&items(json!([
            tool("r", "Read", json!({"kind": "read", "path": "a"}), "ok"),
            {"kind": "assistant", "text": "orphan", "parentToolUseId": "r"},
            {"kind": "assistant", "text": "lost", "parentToolUseId": "gone"},
        ])));
        assert_eq!(t.turns.len(), 1);
        assert_eq!(t.turns[0].blocks.len(), 1);
    }

    #[test]
    fn a_tasks_item_becomes_todos_with_mapped_statuses() {
        let t = fold(&items(json!([{"kind": "tasks", "items": [
            {"text": "read", "status": "completed"},
            {"text": "group", "status": "in_progress"},
            {"text": "test", "status": "pending"},
        ]}])));
        let todo = |text: &str, status| Todo { text: text.into(), status };
        assert_eq!(
            t.turns[0].blocks,
            vec![Block::Todos { id: "i1".into(), todos: vec![todo("read", TodoStatus::Done), todo("group", TodoStatus::Active), todo("test", TodoStatus::Pending)] }]
        );
    }

    #[test]
    fn a_compact_item_becomes_a_compacted_marker() {
        let t = fold(&items(json!([{"kind": "user", "text": "go"}, {"kind": "compact", "trigger": "manual"}])));
        assert_eq!(t.turns[0].blocks, vec![Block::Compacted { id: "i2".into(), auto: false }]);
    }

    #[test]
    fn a_notice_after_an_ended_turn_heads_a_turn_the_agent_began_itself() {
        let t = fold(&items(json!([
            {"kind": "user", "text": "go"},
            {"kind": "result", "ok": true, "durationMs": 5},
            {"kind": "notice", "text": "Stop hook: tests fail"},
            {"kind": "assistant", "text": "Fixing them."},
        ])));
        assert_eq!(t.turns.len(), 2);
        assert_eq!(t.turns[1].prompt, None);
        assert_eq!(t.turns[1].blocks[0], Block::Notice { message: Message { id: "i3".into(), at: 3000, text: "Stop hook: tests fail".into() }, error: false });
    }

    #[test]
    fn an_error_the_agent_went_on_after_is_a_notice_in_its_turn() {
        let golden: Value = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/stream_notice_error.json")).unwrap();
        let mut notice: Item = serde_json::from_value(golden["item"].clone()).unwrap();
        notice.seq = 2;
        let t = fold(&[items(json!([{"kind": "user", "text": "go"}])).remove(0), notice]);
        assert_eq!(t.turns.len(), 1);
        assert_eq!(t.turns[0].end, None);
        assert!(matches!(&t.turns[0].blocks[..], [Block::Notice { message, error: true }] if message.text == "API Error: 500"));
    }

    #[test]
    fn an_edit_keeps_its_diff() {
        let golden: Value = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/stream_tool_done.json")).unwrap();
        let item: Item = serde_json::from_value(golden["item"].clone()).unwrap();
        let line = |kind, text: &str| DiffLine { number: Some(1), kind, text: text.into() };
        let diff = Diff { lines: vec![line(LineKind::Del, "a"), line(LineKind::Add, "b")], additions: 1, deletions: 1 };
        assert_eq!(only_tool(&fold(&[item])).kind, ToolKind::Edit { path: "a.go".into(), diff: Some(diff) });
    }

    #[test]
    fn a_permission_maps_to_an_ask_with_its_tool() {
        let golden: Value = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/permission_request.json")).unwrap();
        let p: Permission = serde_json::from_value(golden["request"].clone()).unwrap();
        assert_eq!(ask(&p), Ask { request_id: "r1".into(), tool: ToolKind::Shell { command: "rm x".into(), description: "remove".into() }, tool_name: "Bash".into(), questions: vec![] });
    }
}
