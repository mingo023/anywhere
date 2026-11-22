use super::machine::Blocker;
use crate::composer::Chips;
use crate::modals::clone_project::Machine;
use std::path::PathBuf;

/// A project the menu lists, or the one the draft is on.
pub(crate) struct Project {
    pub(crate) key: String,
    pub(crate) name: String,
    pub(crate) mark: ui::Mark,
}

/// What `Desktop` hands the new-session page before it draws.
pub(crate) struct Side {
    /// The machine on screen.
    pub(crate) machine: Machine,
    /// Every machine, while its menu is open.
    pub(crate) machines: Vec<Machine>,
    /// The host's name for what opens a folder there; none on This Mac.
    pub(crate) host: Option<String>,
    pub(crate) blocker: Option<Blocker>,
    /// The project the draft is on.
    pub(crate) project: Option<Project>,
    /// The machine's projects, while their menu is open.
    pub(crate) projects: Vec<Project>,
    /// pocketd can open a worktree on a branch or a pull request.
    pub(crate) opens: bool,
    /// Whether the page stands in for an empty main view, so Esc doesn't leave it.
    pub(crate) idle: bool,
    /// The window's background image, as a path.
    pub(crate) background: Option<String>,
    pub(crate) chips: Chips,
    pub(crate) attachments: PathBuf,
    /// The agent, account and project the list of suggestions is for.
    pub(crate) suggest_key: Option<String>,
}
