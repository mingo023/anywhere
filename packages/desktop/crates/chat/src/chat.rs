//! An agent's conversation as a chat shows it: the shared transcript every agent's history folds into, and the rows it lays out as.

mod adapter;
mod commands;
mod marks;
pub mod mcp;
mod outgoing;
mod rows;
mod session;
mod spawned;
mod timeline;

pub use adapter::{capabilities, for_provider, Backend, Capabilities};
pub use commands::{menu, skips_queue, slash, Builtin, Kind, Local, Slash};
pub use marks::{marks, mention_name, Marks};
pub use outgoing::{action, busy, Action, Delivery, Outgoing, Pending};
pub use rows::{at_work, at_work_on, fold_of, folded, in_background, is_work, live_tail, markdown_blocks, narrates, phase_title, reply, reported, row_of, rows, running_agent, settled_tail, splice, step_prompt, top_gap, worked, Row};
pub use session::{Folds, Session};
pub use spawned::{Spawned, spawned};
pub use timeline::Fold;

/// One agent's conversation, oldest turn first.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Transcript {
    pub turns: Vec<Turn>,
}

/// A prompt and everything the agent did for it, up to its end.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Turn {
    /// The id of the turn's first item.
    pub id: String,
    /// None for a turn the agent began itself (history before the first fetched prompt, an auto compact).
    pub prompt: Option<Message>,
    pub blocks: Vec<Block>,
    /// None while the agent is still on it.
    pub end: Option<End>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Message {
    pub id: String,
    /// Unix ms.
    pub at: i64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Block {
    Text(Message),
    Thinking(Message),
    Tool(ToolCall),
    Todos { id: String, todos: Vec<Todo> },
    /// A plan-mode plan, markdown.
    Plan(Message),
    Compacted { id: String, auto: bool },
    /// Why the agent went on without a prompt, as "Stop hook: tests fail", or an `error` it went on after.
    Notice { message: Message, error: bool },
}

#[derive(Clone, Debug, PartialEq)]
pub struct ToolCall {
    /// The agent's tool-use id; sub-agent items point at it.
    pub id: String,
    pub name: String,
    pub kind: ToolKind,
    pub status: ToolStatus,
    pub output: Option<String>,
    pub duration_ms: Option<u64>,
    pub at: i64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ToolStatus {
    Running,
    Ok,
    Failed,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ToolKind {
    Shell { command: String },
    Read { path: String },
    Edit { path: String, diff: Option<Diff> },
    Write { path: String, diff: Option<Diff> },
    Search { query: String },
    Fetch { url: String },
    Question(Vec<Question>),
    Agent(SubAgent),
    /// Any other tool; `input` is its JSON input, pretty-printed, and `description` what it is about in words, when known.
    Other { input: String, description: String },
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct SubAgent {
    pub description: String,
    /// Empty until the backend reports sub-agent items (`Capabilities::sub_agents`).
    pub transcript: Transcript,
    /// The model it runs on, an alias until it first replies; empty when unknown.
    pub model: String,
    /// The context it last answered from; 0 until it first replies.
    pub tokens: u64,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Diff {
    pub lines: Vec<DiffLine>,
    pub additions: u32,
    pub deletions: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct DiffLine {
    pub number: Option<u32>,
    pub kind: LineKind,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    Add,
    Del,
    Context,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Todo {
    pub text: String,
    pub status: TodoStatus,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TodoStatus {
    Pending,
    Active,
    Done,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Question {
    pub header: String,
    pub question: String,
    pub options: Vec<Choice>,
    pub multi: bool,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Choice {
    pub label: String,
    pub description: String,
}

#[derive(Clone, Debug, PartialEq)]
pub enum End {
    Done { duration_ms: u64, usage: Option<Usage> },
    Failed { error: String, duration_ms: u64 },
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Usage {
    pub input: u64,
    pub output: u64,
    pub cache_read: u64,
}

/// A pending ask for the user: a tool waiting for permission, or questions to answer.
#[derive(Clone, Debug, PartialEq)]
pub struct Ask {
    pub request_id: String,
    pub tool: ToolKind,
    pub tool_name: String,
    pub questions: Vec<Question>,
}

/// What the chat view asks of the agent; `Desktop` routes each to its backend's transport.
#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Send(String),
    Interrupt,
    /// Has the agent take what waits for its next step now.
    SendNow,
    Compact,
    Allow(String),
    Deny(String),
    Reply { request: String, text: String },
    Answer { request: String, answers: std::collections::HashMap<String, String> },
}
