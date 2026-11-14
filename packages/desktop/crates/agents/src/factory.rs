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

/// One ticket's spec and what happened to it, as `factory.ticket` answers.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Detail {
    pub ticket_id: String,
    /// The spec block's markdown; empty until the lead wrote one.
    pub spec: String,
    /// Oldest first.
    pub activity: Vec<Activity>,
}

#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Activity {
    /// Unix ms.
    pub at: i64,
    pub station: crate::tickets::Station,
    /// `you`, `lead`, `worker:<task>`, `reviewer` or `anywhere`.
    pub actor: String,
    pub kind: String,
    pub text: String,
}
