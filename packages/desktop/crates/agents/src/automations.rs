use serde::{Deserialize, Deserializer, Serialize};

const MAX_NAME: usize = 80;
const MAX_PROMPT: usize = 64 << 10;
const EVERY_MIN: std::ops::RangeInclusive<u32> = 5..=10080;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(tag = "kind", rename_all = "lowercase")]
pub enum Schedule {
    /// `days` are 0 = Sunday to 6 = Saturday; `time` is "HH:MM" on pocketd's clock.
    Days { days: Vec<u8>, time: String },
    Interval {
        #[serde(rename = "everyMin")]
        every_min: u32,
    },
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Automation {
    pub id: String,
    pub name: String,
    pub prompt: String,
    pub provider: String,
    pub folder: String,
    pub schedule: Schedule,
    pub enabled: bool,
    /// Unix ms; 0 while disabled.
    #[serde(default)]
    pub next_run_at: i64,
}

#[derive(Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum RunStatus {
    Pending,
    Running,
    Waiting,
    Succeeded,
    Failed,
    Skipped,
    Cancelled,
    /// A status a newer pocketd added.
    #[serde(other)]
    Unknown,
}

/// Reads the automations of a snapshot one by one: one this client can't read (a schedule kind
/// from a newer pocketd, a day outside 0..=6) is left out, so the rest still show.
pub fn known<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<Automation>, D::Error> {
    let raw = Vec::<serde_json::Value>::deserialize(d)?;
    let readable = |a: &Automation| !matches!(&a.schedule, Schedule::Days { days, .. } if days.iter().any(|&d| d > 6));
    Ok(raw.into_iter().filter_map(|v| serde_json::from_value(v).ok()).filter(readable).collect())
}

#[derive(Deserialize, Clone, Debug, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct Run {
    pub id: String,
    pub automation_id: String,
    pub status: RunStatus,
    /// "schedule" or "manual".
    pub trigger: String,
    #[serde(default)]
    pub why: String,
    #[serde(default)]
    pub summary: String,
    /// Unix ms; a skipped run carries the time it was due.
    pub started_at: i64,
    #[serde(default)]
    pub finished_at: i64,
    #[serde(default)]
    pub agent_id: String,
    #[serde(default)]
    pub terminal_id: String,
}

/// What `automation.save` carries; no `id` creates.
#[derive(Serialize, Clone, Debug, PartialEq)]
pub struct AutomationDraft {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub name: String,
    pub prompt: String,
    pub provider: String,
    pub folder: String,
    pub schedule: Schedule,
    pub enabled: bool,
}

impl AutomationDraft {
    /// Why pocketd would refuse this draft, mirroring its validation.
    pub fn problem(&self) -> Option<&'static str> {
        let name = self.name.trim().chars().count();
        if name == 0 || name > MAX_NAME {
            return Some("Give it a name of up to 80 characters");
        }
        if self.prompt.is_empty() {
            return Some("Write a prompt");
        }
        if self.prompt.len() > MAX_PROMPT {
            return Some("Shorten the prompt to 64 KB");
        }
        if self.provider != "claude" && self.provider != "codex" {
            return Some("Pick an agent");
        }
        if self.folder.is_empty() {
            return Some("Pick a folder");
        }
        match &self.schedule {
            Schedule::Days { days, time } => {
                let unique = days.iter().enumerate().all(|(i, d)| !days[..i].contains(d));
                if days.is_empty() || days.iter().any(|&d| d > 6) || !unique {
                    return Some("Pick at least one day");
                }
                (!valid_time(time)).then_some("Set a time as HH:MM")
            }
            Schedule::Interval { every_min } => (!valid_every(*every_min)).then_some("Repeat every 5 minutes to 7 days"),
        }
    }
}

pub fn valid_every(minutes: u32) -> bool {
    EVERY_MIN.contains(&minutes)
}

pub fn valid_time(time: &str) -> bool {
    let Some((h, m)) = time.split_once(':') else { return false };
    let part = |s: &str, max: u32| s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit()) && s.parse::<u32>().is_ok_and(|n| n < max);
    part(h, 24) && part(m, 60)
}

/// The automations and runs from pocketd's last snapshot; runs are newest first.
#[derive(Default, Debug)]
pub struct Automations {
    pub items: Vec<Automation>,
    pub runs: Vec<Run>,
    /// A snapshot has arrived, so an empty list means none rather than not yet known.
    pub loaded: bool,
}

impl Automations {
    pub fn apply(&mut self, items: Vec<Automation>, runs: Vec<Run>) {
        self.items = items;
        self.runs = runs;
        self.loaded = true;
    }

    /// Runs whose agent needs the user, newest first.
    pub fn waiting(&self) -> impl Iterator<Item = &Run> {
        self.runs.iter().filter(|r| r.status == RunStatus::Waiting)
    }

    pub fn last_run<'a>(&'a self, automation: &str) -> Option<&'a Run> {
        self.runs.iter().find(|r| r.automation_id == automation)
    }

    pub fn runs_of<'a>(&'a self, automation: &'a str) -> impl Iterator<Item = &'a Run> {
        self.runs.iter().filter(move |r| r.automation_id == automation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn draft() -> AutomationDraft {
        AutomationDraft {
            id: None,
            name: "Morning review".into(),
            prompt: "Review new PRs".into(),
            provider: "claude".into(),
            folder: "/Users/me/app".into(),
            schedule: Schedule::Days { days: vec![1, 2, 3, 4, 5], time: "09:00".into() },
            enabled: true,
        }
    }

    fn run(id: &str, automation: &str, status: RunStatus) -> Run {
        Run {
            id: id.into(),
            automation_id: automation.into(),
            status,
            trigger: "schedule".into(),
            why: String::new(),
            summary: String::new(),
            started_at: 0,
            finished_at: 0,
            agent_id: String::new(),
            terminal_id: String::new(),
        }
    }

    #[test]
    fn a_snapshot_replaces_the_automations() {
        let mut a = Automations::default();
        assert!(!a.loaded);
        let snapshot: crate::Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/automations.json")).unwrap();
        a.apply(snapshot.automations, snapshot.runs);
        assert!(a.loaded);
        assert_eq!((a.items.len(), a.runs.len()), (1, 2));
        a.apply(vec![], vec![]);
        assert!(a.loaded && a.items.is_empty() && a.runs.is_empty());
    }

    #[test]
    fn the_golden_snapshot_decodes() {
        let f: crate::Frame = serde_json::from_str(include_str!("../../../../pocketd/internal/proto/testdata/golden/server/automations.json")).unwrap();
        let au = &f.automations[0];
        assert_eq!(au.schedule, Schedule::Days { days: vec![1, 2, 3, 4, 5], time: "09:00".into() });
        assert_eq!((au.provider.as_str(), au.enabled, au.next_run_at), ("claude", true, 1791450000000));
        let [waiting, done] = &f.runs[..] else { panic!("want two runs") };
        assert_eq!((waiting.status, waiting.trigger.as_str(), waiting.agent_id.as_str(), waiting.finished_at), (RunStatus::Waiting, "manual", "a1", 0));
        assert_eq!((done.status, done.summary.as_str(), done.finished_at), (RunStatus::Succeeded, "Reviewed 3 PRs.", 1791277500000));
    }

    #[test]
    fn a_disabled_automation_decodes_without_a_next_run() {
        let au: Automation = serde_json::from_str(
            r#"{"id":"au2","name":"n","prompt":"p","provider":"codex","folder":"/f","schedule":{"kind":"interval","everyMin":60},"enabled":false}"#,
        )
        .unwrap();
        assert_eq!((au.schedule, au.next_run_at), (Schedule::Interval { every_min: 60 }, 0));
    }

    #[test]
    fn the_newest_run_and_the_waiting_runs_are_found_by_automation() {
        let a = Automations {
            runs: vec![run("r3", "b", RunStatus::Waiting), run("r2", "a", RunStatus::Failed), run("r1", "a", RunStatus::Running)],
            ..Default::default()
        };
        assert_eq!(a.last_run("a").map(|r| r.id.as_str()), Some("r2"));
        assert_eq!(a.runs_of("a").count(), 2);
        assert_eq!(a.waiting().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["r3"]);
        assert!(a.last_run("c").is_none());
    }

    #[test]
    fn a_status_from_a_newer_pocketd_decodes_as_unknown_and_keeps_the_snapshot() {
        let raw = r#"{"type":"automations","automations":[],"runs":[{"id":"r1","automationId":"a","status":"paused","trigger":"schedule","startedAt":1}]}"#;
        let f: crate::Frame = serde_json::from_str(raw).unwrap();
        assert_eq!(f.runs[0].status, RunStatus::Unknown);
    }

    #[test]
    fn an_automation_with_a_schedule_from_a_newer_pocketd_is_dropped_alone() {
        let auto = |id: &str, schedule: &str| format!(r#"{{"id":"{id}","name":"n","prompt":"p","provider":"claude","folder":"/f","schedule":{schedule},"enabled":true}}"#);
        let raw = format!(
            r#"{{"type":"automations","automations":[{},{},{},{}],"runs":[]}}"#,
            auto("a", r#"{"kind":"interval","everyMin":60}"#),
            auto("b", r#"{"kind":"cron","expr":"* * * * *"}"#),
            auto("c", r#"{"kind":"days","days":[7],"time":"09:00"}"#),
            auto("d", r#"{"kind":"days","days":[1],"time":"09:00"}"#),
        );
        let f: crate::Frame = serde_json::from_str(&raw).unwrap();
        assert_eq!(f.automations.iter().map(|a| a.id.as_str()).collect::<Vec<_>>(), ["a", "d"]);
    }

    #[test]
    fn a_prompt_too_long_is_not_told_to_be_written() {
        let long = AutomationDraft { prompt: "x".repeat(MAX_PROMPT + 1), ..draft() };
        let empty = AutomationDraft { prompt: String::new(), ..draft() };
        assert_eq!((empty.problem(), long.problem()), (Some("Write a prompt"), Some("Shorten the prompt to 64 KB")));
    }

    #[test]
    fn a_complete_draft_has_no_problem() {
        assert_eq!(draft().problem(), None);
        let every = |every_min| AutomationDraft { schedule: Schedule::Interval { every_min }, ..draft() };
        assert_eq!((every(5).problem(), every(10080).problem()), (None, None));
    }

    #[test]
    fn a_draft_pocketd_would_refuse_has_a_problem() {
        let with = |f: fn(&mut AutomationDraft)| {
            let mut d = draft();
            f(&mut d);
            d.problem().is_some()
        };
        assert!(with(|d| d.name = "  ".into()));
        assert!(with(|d| d.name = "x".repeat(81)));
        assert!(with(|d| d.prompt.clear()));
        assert!(with(|d| d.prompt = "x".repeat(MAX_PROMPT + 1)));
        assert!(with(|d| d.provider = "gemini".into()));
        assert!(with(|d| d.folder.clear()));
        assert!(with(|d| d.schedule = Schedule::Days { days: vec![], time: "09:00".into() }));
        assert!(with(|d| d.schedule = Schedule::Days { days: vec![7], time: "09:00".into() }));
        assert!(with(|d| d.schedule = Schedule::Days { days: vec![1, 1], time: "09:00".into() }));
        assert!(with(|d| d.schedule = Schedule::Days { days: vec![1], time: "9:00".into() }));
        assert!(with(|d| d.schedule = Schedule::Days { days: vec![1], time: "24:00".into() }));
        assert!(with(|d| d.schedule = Schedule::Interval { every_min: 4 }));
        assert!(with(|d| d.schedule = Schedule::Interval { every_min: 10081 }));
    }

    #[test]
    fn a_draft_serializes_as_pocketd_reads_it() {
        let mut d = draft();
        let want = include_str!("../../../../pocketd/internal/proto/testdata/golden/client/automation_save.json");
        let want: serde_json::Value = serde_json::from_str(want).unwrap();
        d.id = Some("au1".into());
        assert_eq!(serde_json::to_value(&d).unwrap(), want["automation"]);
        let every = AutomationDraft { schedule: Schedule::Interval { every_min: 60 }, provider: "codex".into(), ..draft() };
        let want = include_str!("../../../../pocketd/internal/proto/testdata/golden/client/automation_save_interval.json");
        let want: serde_json::Value = serde_json::from_str(want).unwrap();
        assert_eq!(serde_json::to_value(&every).unwrap(), want["automation"]);
    }
}
