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
    /// The local commands of the newest commit, in the order they ran. A failure ends the list.
    pub checks: Vec<Check>,
    /// Checks passed with no local command and no GitHub check.
    pub no_checks: bool,
    /// What the reviewer found in the PR's current head.
    pub findings: Vec<Finding>,
}

/// How bad the reviewer says a finding is, the worst first.
#[derive(Deserialize, Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum Severity {
    Critical,
    Minor,
    #[default]
    #[serde(other)]
    Nit,
}

/// One thing the reviewer found in a Ticket's PR.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Finding {
    pub severity: Severity,
    pub file: String,
    pub line: u32,
    pub message: String,
}

/// One local command's run in the Checks station.
#[derive(Deserialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Check {
    pub command: String,
    /// -1 when it ran too long or could not start.
    pub exit: i32,
    /// The last 64 KB.
    pub output: String,
    /// The auto-fix attempts used when it ran.
    pub attempt: u32,
    /// Unix ms.
    pub at: i64,
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

/// A profile as a list shows it.
#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ProfileInfo {
    pub id: String,
    pub name: String,
    /// claude or codex.
    pub provider: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub model: String,
    pub skills: u32,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub built_in: bool,
}

/// A whole profile: who the agent is, how it works, and its skills' files.
#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Profile {
    #[serde(flatten)]
    pub info: ProfileInfo,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub soul: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub instructions: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<ProfileFile>,
}

/// One file under `skills/`, by its path from the profile's folder. `text` is base64 when `binary`.
#[derive(Deserialize, Serialize, Clone, Debug, Default, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct ProfileFile {
    pub path: String,
    pub text: String,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub binary: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_ticket_detail_reads_its_findings_and_a_severity_orders_worst_first() {
        let raw = serde_json::json!({"ticketId": "t1", "activity": [], "findings": [
            {"severity": "critical", "file": "src/a.rs", "line": 12, "message": "Leaks the handle"},
            {"severity": "nit", "message": "Name it"}
        ]});
        let d: Detail = serde_json::from_value(raw).unwrap();
        assert_eq!(
            d.findings,
            vec![
                Finding { severity: Severity::Critical, file: "src/a.rs".into(), line: 12, message: "Leaks the handle".into() },
                Finding { severity: Severity::Nit, file: String::new(), line: 0, message: "Name it".into() },
            ]
        );
        assert!(Severity::Critical < Severity::Minor && Severity::Minor < Severity::Nit);
    }

    #[test]
    fn a_detail_without_findings_has_none() {
        let d: Detail = serde_json::from_value(serde_json::json!({"ticketId": "t1", "activity": []})).unwrap();
        assert!(d.findings.is_empty());
    }
}
