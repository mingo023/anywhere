use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Wire names of the task states whose words the desktop's copy avoids.
pub mod state {
    pub const RUNNING: &str = "running";
    pub const WAITING: &str = "waiting";
}

/// A lead chat's team, as `team.list` and `team.update` report it.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Team {
    /// The lead's agent id.
    pub lead: String,
    /// planning, proposed, running, review, merging, merged, conflict or failed.
    pub state: String,
    pub error: String,
    pub tasks: Vec<TeamTask>,
    /// The lead said "Ready to merge".
    pub ready: bool,
    /// The files the failed merge conflicted in.
    pub conflict: Vec<String>,
    /// The lead's one repair turn was used.
    pub repaired: bool,
    pub updated_at: i64,
}

/// One task of a team's plan.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct TeamTask {
    pub id: String,
    pub title: String,
    /// claude or codex.
    pub agent: String,
    pub model: String,
    /// The only files it may change.
    pub files: Vec<String>,
    /// The tasks whose work it starts from.
    pub after: Vec<String>,
    /// Command prefixes its worker runs without asking.
    pub allow: Vec<String>,
    pub instructions: String,
    /// planned, waiting, running, needsYou, done, failed, stopped or merged.
    pub state: String,
    /// The worker's agent id, once it runs.
    pub worker: String,
    pub branch: String,
    pub worktree: String,
    /// The worker's last text.
    pub summary: String,
    /// `git diff --stat` against its base.
    pub stat: String,
}

/// A task kept at Start, as `team.start` takes it: its agent and model as picked, its allowlist as approved.
/// An empty model or allowlist is left off the frame, as pocketd's `omitempty` writes it.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct TaskPick {
    pub id: String,
    pub agent: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub allow: Vec<String>,
}

/// Holds `team` unless the one held for its lead is newer: a list read before an update can arrive after it.
pub(crate) fn keep(teams: &mut HashMap<String, Team>, team: Team) {
    if teams.get(&team.lead).is_none_or(|held| held.updated_at <= team.updated_at) {
        teams.insert(team.lead.clone(), team);
    }
}

/// The team request pocketd refused.
#[derive(Clone, Debug, PartialEq)]
pub enum TeamAction {
    /// A team prompt, sent under this request id.
    Plan(String),
    Start,
    Stop,
    Merge,
}
