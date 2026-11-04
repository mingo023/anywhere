use serde::{Deserialize, Serialize};

/// What a worktree's Changes panel shows: its uncommitted changes, or with `compare` everything it changed against `against`.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default)]
pub struct Scope {
    pub compare: bool,
    pub against: Against,
    /// Diffs the two tips instead of from where the worktree left the ref.
    pub direct: bool,
    /// Takes the working tree, uncommitted files included, as the new side instead of HEAD.
    pub working: bool,
}

/// The ref a Compare is against.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum Against {
    /// The branch the worktree was created from, as `git::read` finds it.
    #[default]
    Base,
    /// Another worktree of the project, by its path, at its branch.
    Worktree(String),
    /// A branch, remote branch, tag or commit.
    Ref(String),
}
