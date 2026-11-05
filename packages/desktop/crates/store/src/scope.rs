use serde::{Deserialize, Serialize};

/// What a worktree's Changes panel shows: its uncommitted changes, or with `compare` everything it changed against `against`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Scope {
    pub compare: bool,
    pub against: Against,
}

/// The ref a Compare is against.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Against {
    /// The branch the worktree was created from, as `git::read` finds it.
    #[default]
    Base,
    /// Another branch, local or on origin.
    Ref(String),
}
