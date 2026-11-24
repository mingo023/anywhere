use agents::Permission;
use std::collections::HashMap;
use store::prefs::Automations as Defaults;

pub(crate) struct Project {
    pub(crate) name: String,
    pub(crate) mark: ui::Mark,
}

#[derive(Default)]
pub(crate) struct Side {
    pub(crate) labels: HashMap<String, String>,
    pub(crate) projects: HashMap<String, Project>,
    pub(crate) folders: Vec<String>,
    pub(crate) titles: HashMap<String, String>,
    pub(crate) asks: HashMap<String, Permission>,
    pub(crate) defaults: Defaults,
    pub(crate) folder: String,
    pub(crate) lead: f32,
}
