use super::logic::{fallback_name, when_label};
use agents::automations::{Automation, AutomationDraft, Schedule, valid_every, valid_time};
use store::prefs::Automations as Defaults;

pub const PROVIDERS: [&str; 2] = ["claude", "codex"];
pub const WEEKDAYS: [u8; 5] = [1, 2, 3, 4, 5];
pub const EVERY_DAY: [u8; 7] = [0, 1, 2, 3, 4, 5, 6];
pub const WEEKENDS: [u8; 2] = [0, 6];
pub const TIME_ERROR: &str = "Use 24-hour HH:MM, like 09:00";
pub const EVERY_ERROR: &str = "Between 5 minutes and 7 days";

/// What the editor holds: every field, texts included, as plain values.
#[derive(Clone, PartialEq, Debug)]
pub struct Values {
    pub id: Option<String>,
    pub title: String,
    pub prompt: String,
    pub provider: &'static str,
    pub folder: String,
    /// A trigger was added; the days, time and interval below only count once it is.
    pub scheduled: bool,
    pub repeat: bool,
    /// Indexed as schedules number days: 0 is Sunday.
    pub days: [bool; 7],
    pub time: String,
    pub every: String,
    pub enabled: bool,
    /// Empty, as on automations saved before it, leaves it to the agent's own settings.
    pub access: String,
    pub new_worktree: bool,
}

impl Values {
    pub fn new(folder: String) -> Self {
        Self::from_defaults(folder, &Defaults::default())
    }

    /// A new automation as Settings › Automations sets them up.
    pub fn from_defaults(folder: String, d: &Defaults) -> Self {
        let mut days = [false; 7];
        d.days.iter().filter(|&&d| d < 7).for_each(|&d| days[d as usize] = true);
        let provider = PROVIDERS.into_iter().find(|p| *p == d.agent).unwrap_or(PROVIDERS[0]);
        Self { id: None, title: String::new(), prompt: String::new(), provider, folder, scheduled: false, repeat: false, days, time: d.time.clone(), every: d.every.to_string(), enabled: true, access: d.access.clone(), new_worktree: d.new_worktree }
    }

    pub fn add_schedule(&mut self, d: &Defaults) {
        let fresh = Self::from_defaults(String::new(), d);
        (self.scheduled, self.repeat, self.days, self.time, self.every) = (true, false, fresh.days, fresh.time, fresh.every);
    }

    pub fn of(a: &Automation) -> Self {
        let provider = PROVIDERS.into_iter().find(|p| *p == a.provider).unwrap_or(PROVIDERS[0]);
        let base = Self { id: Some(a.id.clone()), title: a.name.clone(), prompt: a.prompt.clone(), provider, folder: a.folder.clone(), scheduled: true, enabled: a.enabled, access: a.access.clone(), new_worktree: a.new_worktree, ..Self::new(String::new()) };
        match &a.schedule {
            Schedule::Days { days, time } => {
                let mut on = [false; 7];
                days.iter().for_each(|&d| on[d as usize] = true);
                Self { days: on, time: time.clone(), ..base }
            }
            Schedule::Interval { every_min } => Self { repeat: true, every: every_min.to_string(), ..base },
        }
    }

    /// The schedule as filled in, right or not; none until a trigger is added.
    fn schedule(&self) -> Option<Schedule> {
        if !self.scheduled {
            return None;
        }
        Some(if self.repeat {
            Schedule::Interval { every_min: self.every.trim().parse().unwrap_or(0) }
        } else {
            Schedule::Days { days: (0..7u8).filter(|&d| self.days[d as usize]).collect(), time: self.time.trim().into() }
        })
    }

    /// The schedule once it would pass.
    pub fn valid_schedule(&self) -> Option<Schedule> {
        self.schedule().filter(|s| match s {
            Schedule::Days { days, time } => !days.is_empty() && valid_time(time),
            Schedule::Interval { every_min } => valid_every(*every_min),
        })
    }

    /// "Weekdays at 09:00", while the schedule is right.
    pub fn label(&self) -> Option<String> {
        self.valid_schedule().as_ref().map(when_label)
    }

    /// What to save, or the first thing in the way.
    pub fn draft(&self) -> Result<AutomationDraft, &'static str> {
        let prompt = self.prompt.trim();
        if prompt.is_empty() {
            return Err("Write instructions");
        }
        let schedule = self.schedule().ok_or("Add a trigger")?;
        let title = self.title.trim();
        let name = if title.is_empty() { fallback_name(prompt) } else { title.to_string() };
        let draft = AutomationDraft { id: self.id.clone(), name, prompt: prompt.into(), provider: self.provider.into(), folder: self.folder.clone(), schedule, enabled: self.enabled, access: self.access.clone(), new_worktree: self.new_worktree };
        draft.problem().map_or(Ok(draft), Err)
    }

    /// Set under the trigger once its field has text that can't be right; an empty field is the header's to point out.
    pub fn field_error(&self) -> Option<&'static str> {
        if self.repeat {
            let n = self.every.trim();
            (!n.is_empty() && !n.parse().is_ok_and(valid_every)).then_some(EVERY_ERROR)
        } else {
            let t = self.time.trim();
            (!t.is_empty() && !valid_time(t)).then_some(TIME_ERROR)
        }
    }

    pub fn toggle_day(&mut self, day: u8) {
        let on = &mut self.days[day as usize];
        *on = !*on;
    }

    pub fn set_days(&mut self, days: &[u8]) {
        self.days = [false; 7];
        days.iter().for_each(|&d| self.days[d as usize] = true);
    }

    pub fn has_days(&self, days: &[u8]) -> bool {
        (0..7u8).all(|d| self.days[d as usize] == days.contains(&d))
    }

    pub fn fill(&mut self, t: &Template) {
        self.title = t.title.into();
        self.prompt = t.prompt.into();
        self.scheduled = true;
        match t.when {
            When::Days(days, time) => {
                (self.repeat, self.time) = (false, time.into());
                self.set_days(days);
            }
            When::Every(minutes) => (self.repeat, self.every) = (true, minutes.to_string()),
        }
    }
}

#[derive(Clone, Copy)]
pub enum When {
    Days(&'static [u8], &'static str),
    Every(u32),
}

/// A starting point for an empty editor.
pub struct Template {
    pub icon: &'static str,
    pub title: &'static str,
    pub prompt: &'static str,
    pub when: When,
}

pub const TEMPLATES: [Template; 4] = [
    Template {
        icon: "merge",
        title: "Review draft PRs",
        prompt: "List the open draft pull requests, read each diff, and leave review comments on anything risky. Do not push commits.",
        when: When::Days(&WEEKDAYS, "10:00"),
    },
    Template {
        icon: "shield",
        title: "Watch failing checks",
        prompt: "Check the latest CI runs on the default branch. For each failure, find the cause and summarise it. Fix flaky tests only if the change is small.",
        when: When::Every(60),
    },
    Template {
        icon: "settings",
        title: "Environment doctor",
        prompt: "Verify the toolchain builds from a clean checkout: install dependencies, build, run the test suite. Report what broke and why.",
        when: When::Days(&[1], "08:00"),
    },
    Template {
        icon: "flow",
        title: "Repo health check",
        prompt: "Look for stale branches, unused dependencies, and TODOs older than 90 days. Summarise what to clean up first.",
        when: When::Days(&[5], "16:00"),
    },
];

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> Values {
        Values { prompt: "Review PRs\nthen stop".into(), scheduled: true, ..Values::new("/code/app".into()) }
    }

    fn automation(schedule: Schedule) -> Automation {
        Automation { id: "a1".into(), name: "Review".into(), prompt: "Look".into(), provider: "codex".into(), folder: "/code/app".into(), schedule, enabled: false, access: String::new(), new_worktree: false, next_run_at: 0 }
    }

    #[test]
    fn a_new_automation_asks_for_instructions_then_a_trigger() {
        let new = Values::new("/code/app".into());
        assert_eq!(new.draft().unwrap_err(), "Write instructions");
        assert_eq!(Values { prompt: "  \n".into(), ..new.clone() }.draft().unwrap_err(), "Write instructions");
        assert_eq!(Values { prompt: "Look".into(), ..new }.draft().unwrap_err(), "Add a trigger");
    }

    #[test]
    fn a_ready_form_runs_weekdays_with_claude_in_a_new_worktree_and_is_named_by_its_first_prompt_line() {
        let d = ready().draft().unwrap();
        assert_eq!(
            d,
            AutomationDraft {
                id: None,
                name: "Review PRs".into(),
                prompt: "Review PRs\nthen stop".into(),
                provider: "claude".into(),
                folder: "/code/app".into(),
                schedule: Schedule::Days { days: vec![1, 2, 3, 4, 5], time: "09:00".into() },
                enabled: true,
                access: "edits".into(),
                new_worktree: true,
            }
        );
        assert_eq!(Values { title: " Mine ".into(), ..ready() }.draft().unwrap().name, "Mine");
    }

    #[test]
    fn a_trigger_with_no_day_a_bad_time_or_a_bad_interval_says_so() {
        let none = Values { days: [false; 7], ..ready() };
        assert_eq!(none.draft().unwrap_err(), "Pick at least one day");
        assert_eq!(Values { time: "9am".into(), ..ready() }.draft().unwrap_err(), "Set a time as HH:MM");
        let every = |n: &str| Values { repeat: true, every: n.into(), ..ready() };
        assert_eq!(every("soon").draft().unwrap_err(), "Repeat every 5 minutes to 7 days");
        assert_eq!(every("4").draft().unwrap_err(), "Repeat every 5 minutes to 7 days");
        assert_eq!(every("30").draft().unwrap().schedule, Schedule::Interval { every_min: 30 });
    }

    #[test]
    fn a_form_without_a_folder_is_refused() {
        assert_eq!(Values { folder: String::new(), ..ready() }.draft().unwrap_err(), "Pick a folder");
    }

    #[test]
    fn an_edit_keeps_the_id_the_provider_and_whether_it_is_enabled() {
        let v = Values::of(&automation(Schedule::Days { days: vec![5], time: "17:30".into() }));
        let d = v.draft().unwrap();
        assert_eq!((d.id.as_deref(), d.enabled, d.provider.as_str(), d.name.as_str()), (Some("a1"), false, "codex", "Review"));
        assert_eq!(d.schedule, Schedule::Days { days: vec![5], time: "17:30".into() });
    }

    #[test]
    fn an_edit_keeps_the_access_and_worktree_and_one_saved_without_them_stays_on_the_agents_settings() {
        let set = Automation { access: "auto".into(), new_worktree: true, ..automation(Schedule::Interval { every_min: 60 }) };
        let d = Values::of(&set).draft().unwrap();
        assert_eq!((d.access.as_str(), d.new_worktree), ("auto", true));
        let d = Values::of(&automation(Schedule::Interval { every_min: 60 })).draft().unwrap();
        assert_eq!((d.access.as_str(), d.new_worktree), ("", false));
    }

    #[test]
    fn an_interval_automation_opens_on_repeat_and_a_days_one_on_its_days() {
        let v = Values::of(&automation(Schedule::Interval { every_min: 90 }));
        assert_eq!((v.repeat, v.every.as_str(), v.draft().unwrap().schedule), (true, "90", Schedule::Interval { every_min: 90 }));
        let v = Values::of(&automation(Schedule::Days { days: vec![0, 6], time: "08:15".into() }));
        assert_eq!((v.repeat, v.has_days(&WEEKENDS), v.time.as_str(), v.every.as_str()), (false, true, "08:15", "60"));
    }

    #[test]
    fn the_trigger_names_itself_only_while_it_is_right() {
        assert_eq!(ready().label().as_deref(), Some("Weekdays at 09:00"));
        assert_eq!(Values { time: "x".into(), ..ready() }.label(), None);
        assert_eq!(Values::new(String::new()).label(), None);
    }

    #[test]
    fn a_field_error_shows_only_for_text_that_cannot_be_right() {
        assert_eq!(ready().field_error(), None);
        assert_eq!(Values { time: "25:00".into(), ..ready() }.field_error(), Some(TIME_ERROR));
        assert_eq!(Values { time: String::new(), ..ready() }.field_error(), None);
        let every = |n: &str| Values { repeat: true, every: n.into(), ..ready() };
        assert_eq!([every("2").field_error(), every("").field_error(), every("20000").field_error(), every("60").field_error()], [Some(EVERY_ERROR), None, Some(EVERY_ERROR), None]);
    }

    #[test]
    fn days_toggle_and_presets_match_by_set() {
        let mut v = ready();
        assert!(v.has_days(&WEEKDAYS) && !v.has_days(&EVERY_DAY));
        v.toggle_day(6);
        assert!(!v.has_days(&WEEKDAYS));
        v.toggle_day(6);
        v.set_days(&WEEKENDS);
        assert!(v.has_days(&[6, 0]));
    }

    #[test]
    fn adding_a_schedule_starts_from_the_weekday_morning_default() {
        let mut v = Values { repeat: true, time: "17:30".into(), every: "30".into(), ..Values::new("/code/app".into()) };
        v.add_schedule(&Defaults::default());
        let fresh = Values::new(String::new());
        assert_eq!((v.scheduled, v.repeat, v.days, v.time, v.every), (true, false, fresh.days, fresh.time.clone(), fresh.every.clone()));
        assert!(fresh.has_days(&WEEKDAYS) && fresh.time == "09:00" && fresh.every == "60");
    }

    #[test]
    fn a_new_automation_starts_from_the_settings_defaults() {
        let d = Defaults { agent: "codex".into(), time: "07:15".into(), days: vec![0, 6], every: 30, access: "ask".into(), new_worktree: true, ..Defaults::default() };
        let mut v = Values::from_defaults("/f".into(), &d);
        v.add_schedule(&d);
        assert_eq!((v.provider, v.time.as_str(), v.every.as_str(), v.has_days(&WEEKENDS), v.has_days(&[1])), ("codex", "07:15", "30", true, false));
        assert_eq!((v.access.as_str(), v.new_worktree), ("ask", true));
    }

    #[test]
    fn a_template_fills_the_title_prompt_and_schedule_and_keeps_the_rest() {
        let mut v = Values { provider: "codex", ..Values::new("/code/app".into()) };
        v.fill(&TEMPLATES[1]);
        assert_eq!((v.title.as_str(), v.scheduled, v.repeat, v.every.as_str(), v.provider), ("Watch failing checks", true, true, "60", "codex"));
        v.fill(&TEMPLATES[2]);
        assert_eq!((v.repeat, v.time.as_str(), v.has_days(&[1])), (false, "08:00", true));
        for t in &TEMPLATES {
            let mut v = Values::new("/f".into());
            v.fill(t);
            assert!(v.draft().is_ok(), "{}", t.title);
        }
    }
}
