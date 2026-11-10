use serde::{Deserialize, Serialize};

pub const DEFAULT_LIMIT: usize = 4;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum State {
    Todo,
    Queued,
    Live,
    Closed,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "camelCase")]
pub enum Status {
    Starting,
    Working,
    NeedsYou,
    Done,
    Failed,
    Stopped,
    Idle,
    /// No session yet, or a status a newer pocketd added.
    #[default]
    #[serde(other)]
    None,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ClosedWhy {
    Merged,
    Deleted,
    /// By the user, or a reason a newer pocketd added.
    #[default]
    #[serde(other)]
    Closed,
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Ticket {
    pub id: String,
    pub title: String,
    pub prompt: String,
    pub provider: String,
    pub folder: String,
    #[serde(default)]
    pub base: String,
    pub state: State,
    #[serde(default)]
    pub order: i64,
    #[serde(default)]
    pub status: Status,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub terminal_id: String,
    #[serde(default)]
    pub worktree: String,
    #[serde(default)]
    pub created_at: i64,
    #[serde(default)]
    pub changed_at: i64,
    #[serde(default)]
    pub closed_at: i64,
    #[serde(default)]
    pub closed_why: ClosedWhy,
}

/// What `board.save` carries; no `id` creates.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct TicketDraft {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub title: String,
    pub prompt: String,
    pub provider: String,
    pub folder: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub base: String,
}

#[derive(Debug, PartialEq)]
pub struct Board {
    pub tickets: Vec<Ticket>,
    pub limit: usize,
}

impl Default for Board {
    fn default() -> Self {
        Self { tickets: Vec::new(), limit: DEFAULT_LIMIT }
    }
}

impl Board {
    pub fn get(&self, id: &str) -> Option<&Ticket> {
        self.tickets.iter().find(|t| t.id == id)
    }
}
