use super::machine::Blocker;
use crate::composer::Chips;
use crate::modals::clone_project::Machine;
use std::path::PathBuf;

pub(crate) struct Project {
    pub(crate) key: String,
    pub(crate) name: String,
    pub(crate) mark: ui::Mark,
}

pub(crate) struct Side {
    pub(crate) machine: Machine,
    pub(crate) machines: Vec<Machine>,
    pub(crate) host: Option<String>,
    pub(crate) blocker: Option<Blocker>,
    pub(crate) project: Option<Project>,
    pub(crate) projects: Vec<Project>,
    pub(crate) opens: bool,
    pub(crate) idle: bool,
    pub(crate) background: Option<String>,
    pub(crate) chips: Chips,
    pub(crate) attachments: PathBuf,
    pub(crate) suggest_key: Option<String>,
}
