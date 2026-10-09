use serde::{Deserialize, Serialize};

/// The permission modes an automation's runs can start with.
pub const ACCESSES: [&str; 3] = ["ask", "edits", "auto"];

/// Defaults for new automations, and pocketd's missed-run settings as it last reported them.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Automations {
    pub agent: String,
    /// "HH:MM".
    pub time: String,
    /// 0 is Sunday.
    pub days: Vec<u8>,
    /// Minutes between runs.
    pub every: u32,
    /// One of `ACCESSES`.
    pub access: String,
    pub new_worktree: bool,
    /// 0 never runs a missed run late.
    pub grace_hours: u32,
    pub history: u32,
}

impl Default for Automations {
    fn default() -> Self {
        Self { agent: "claude".into(), time: "09:00".into(), days: vec![1, 2, 3, 4, 5], every: 60, access: "edits".into(), new_worktree: true, grace_hours: 12, history: 50 }
    }
}

impl Automations {
    /// Turns a schedule day on or off, keeping at least one.
    pub fn toggle_day(&mut self, day: u8) {
        if let Some(i) = self.days.iter().position(|d| *d == day) {
            if self.days.len() > 1 {
                self.days.remove(i);
            }
        } else if day < 7 {
            self.days.push(day);
            self.days.sort_unstable();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Automations;

    #[test]
    fn a_schedule_keeps_at_least_one_day() {
        let mut a = Automations { days: vec![1], ..Automations::default() };
        a.toggle_day(1);
        a.toggle_day(0);
        assert_eq!(a.days, [0, 1]);
        a.toggle_day(1);
        assert_eq!(a.days, [0]);
    }

    #[test]
    fn automations_saved_before_the_worktree_setting_run_in_a_new_worktree() {
        let a: Automations = serde_json::from_str(r#"{"agent":"codex","access":"ask"}"#).unwrap();
        assert_eq!((a.agent.as_str(), a.access.as_str(), a.new_worktree), ("codex", "ask", true));
    }
}
