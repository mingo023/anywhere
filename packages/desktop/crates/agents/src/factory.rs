use serde::{Deserialize, Serialize};

/// One project's Factory settings, as `factory.config` carries them and `factory.config.save` sends them whole.
#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Config {
    pub enabled: bool,
    pub spec: bool,
    pub reviewer: bool,
    /// The lead's profile id.
    pub lead: String,
    /// The profile id each task's worker gets unless the plan names another.
    pub workers: String,
    /// The reviewer's profile id.
    pub review: String,
    /// The project's own slot limit; 0 is the Board's.
    pub slots: u8,
    /// Auto-fix attempts a ticket gets.
    pub fix: u8,
    /// The local commands the Checks station runs, in order.
    pub checks: Vec<String>,
}
