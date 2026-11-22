use crate::composer::AccessChip;
use crate::composer::model_menu::Launch;
use agents::Permission;
use std::collections::HashMap;
use store::prefs::Automations as Defaults;

pub(crate) struct Project {
    pub(crate) name: String,
    pub(crate) mark: ui::Mark,
}

/// What `Desktop` hands the Automations screen before it draws.
#[derive(Default)]
pub(crate) struct Side {
    pub(crate) labels: HashMap<String, String>,
    pub(crate) projects: HashMap<String, Project>,
    pub(crate) folders: Vec<String>,
    /// The sessions the automations were created by or post into, by id.
    pub(crate) titles: HashMap<String, String>,
    /// The requests waiting runs' agents ask, by agent.
    pub(crate) asks: HashMap<String, Permission>,
    pub(crate) defaults: Defaults,
    /// Where a new automation starts: the project on screen, else the first listed.
    pub(crate) folder: String,
    /// Left padding of the list's header, past the window's lights.
    pub(crate) lead: f32,
}

/// What the editor's model pill and access chip draw from.
pub(crate) struct Chips {
    pub(crate) launch: Launch,
    pub(crate) access: AccessChip,
}
