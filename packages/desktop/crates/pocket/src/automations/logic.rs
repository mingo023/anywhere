use crate::desktop::chrome::Screen;
use crate::util::{ago, ago_long};
use agents::automations::{Automation, Run, RunStatus, Schedule};
use chrono::{DateTime, Datelike, TimeZone, Timelike};
use std::fmt::Display;

pub const DEFAULT_TIME: &str = "09:00";
pub const DEFAULT_EVERY: &str = "60";
pub const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
const DAYS_PLURAL: [&str; 7] = ["Sundays", "Mondays", "Tuesdays", "Wednesdays", "Thursdays", "Fridays", "Saturdays"];
/// Days as the schedule numbers them (0 = Sunday), in the order the week is shown: Monday first.
pub const WEEK: [u8; 7] = [1, 2, 3, 4, 5, 6, 0];
const NAME_CHARS: usize = 40;
const STRIP: usize = 12;

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Tab {
    #[default]
    Automations,
    Runs,
}

#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Filter {
    #[default]
    All,
    Active,
    Paused,
}

/// Indices of the automations `filter` keeps, in list order.
pub fn visible(autos: &[Automation], filter: Filter) -> Vec<usize> {
    autos.iter().enumerate().filter(|(_, a)| filter == Filter::All || a.enabled == (filter == Filter::Active)).map(|(i, _)| i).collect()
}

/// "3 active · 1 paused", over every automation whatever the filter.
pub fn counts_text(autos: &[Automation]) -> String {
    let active = autos.iter().filter(|a| a.enabled).count();
    format!("{active} active · {} paused", autos.len() - active)
}

/// The enabled automations due soonest, at most `n`, as indices into `autos`.
pub fn up_next(autos: &[Automation], n: usize) -> Vec<usize> {
    let mut due: Vec<usize> = (0..autos.len()).filter(|&i| autos[i].enabled && autos[i].next_run_at > 0).collect();
    due.sort_by_key(|&i| autos[i].next_run_at);
    due.truncate(n);
    due
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Section {
    NeedsYou,
    Working,
    Today,
    Yesterday,
    EarlierThisWeek,
    Older,
}

impl Section {
    const ALL: [Section; 6] = [Section::NeedsYou, Section::Working, Section::Today, Section::Yesterday, Section::EarlierThisWeek, Section::Older];

    pub fn label(self) -> &'static str {
        match self {
            Section::NeedsYou => "Needs you",
            Section::Working => "Working",
            Section::Today => "Today",
            Section::Yesterday => "Yesterday",
            Section::EarlierThisWeek => "Earlier this week",
            Section::Older => "Older",
        }
    }
}

/// A row of the Runs tab: the heading of the soonest automations and each of them, or a section's heading with its size and the run at that index.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Row {
    SoonHead,
    Soon(usize),
    Head(Section, usize),
    Run(usize),
}

fn local<Tz: TimeZone>(ms: i64, tz: &Tz) -> DateTime<Tz> {
    DateTime::from_timestamp_millis(ms).unwrap_or_default().with_timezone(tz)
}

fn days_ago<Tz: TimeZone>(ms: i64, now: i64, tz: &Tz) -> i64 {
    local(now, tz).date_naive().signed_duration_since(local(ms, tz).date_naive()).num_days()
}

fn section_of<Tz: TimeZone>(run: &Run, now: i64, tz: &Tz) -> Section {
    match run.status {
        RunStatus::Waiting => Section::NeedsYou,
        RunStatus::Running | RunStatus::Pending => Section::Working,
        _ => match days_ago(run.started_at, now, tz) {
            ..=0 => Section::Today,
            1 => Section::Yesterday,
            2..=6 => Section::EarlierThisWeek,
            _ => Section::Older,
        },
    }
}

/// The runs of `only` (all when `None`) under their sections, each kept in the order given.
pub fn sections<Tz: TimeZone>(runs: &[Run], only: Option<&str>, now: i64, tz: &Tz) -> Vec<Row> {
    let shown: Vec<(usize, Section)> = runs.iter().enumerate().filter(|(_, r)| only.is_none_or(|id| r.automation_id == id)).map(|(i, r)| (i, section_of(r, now, tz))).collect();
    let mut rows = Vec::new();
    for section in Section::ALL {
        let of: Vec<usize> = shown.iter().filter(|(_, s)| *s == section).map(|(i, _)| *i).collect();
        if !of.is_empty() {
            rows.push(Row::Head(section, of.len()));
            rows.extend(of.into_iter().map(Row::Run));
        }
    }
    rows
}

/// The Runs tab: the three automations due next, unless one automation's runs are shown, then the runs.
pub fn run_rows<Tz: TimeZone>(autos: &[Automation], runs: &[Run], only: Option<&str>, now: i64, tz: &Tz) -> Vec<Row> {
    let soon = if only.is_some() { Vec::new() } else { up_next(autos, 3) };
    let head = (!soon.is_empty()).then_some(Row::SoonHead);
    head.into_iter().chain(soon.into_iter().map(Row::Soon)).chain(sections(runs, only, now, tz)).collect()
}

/// "09:41".
pub fn clock<Tz: TimeZone>(ms: i64, tz: &Tz) -> String {
    let at = local(ms, tz);
    format!("{:02}:{:02}", at.hour(), at.minute())
}

fn weekday<Tz: TimeZone>(ms: i64, tz: &Tz) -> &'static str {
    DAYS[local(ms, tz).weekday().num_days_from_sunday() as usize]
}

/// "09:41" today, "Yest.", a weekday within the week, otherwise "5 Oct".
pub fn short_time<Tz: TimeZone>(ms: i64, now: i64, tz: &Tz) -> String
where
    Tz::Offset: Display,
{
    match days_ago(ms, now, tz) {
        ..=0 => clock(ms, tz),
        1 => "Yest.".into(),
        2..=6 => weekday(ms, tz).into(),
        _ => local(ms, tz).format("%-d %b").to_string(),
    }
}

/// "Today 09:00", "Tomorrow 09:00", "Mon 09:00" within the week either way, otherwise "Mon 5 Oct 09:00".
pub fn stamp<Tz: TimeZone>(ms: i64, now: i64, tz: &Tz) -> String
where
    Tz::Offset: Display,
{
    if ms <= 0 {
        return String::new();
    }
    let clock = clock(ms, tz);
    match days_ago(ms, now, tz) {
        0 => format!("Today {clock}"),
        1 => format!("Yesterday {clock}"),
        -1 => format!("Tomorrow {clock}"),
        -6..=-2 | 2..=6 => format!("{} {clock}", weekday(ms, tz)),
        _ => format!("{} {clock}", local(ms, tz).format("%a %-d %b")),
    }
}

/// "42s", "7m" or "1h 5m" between a run's start and finish; empty while it has none.
pub fn duration_text(started_at: i64, finished_at: i64) -> String {
    if finished_at <= started_at {
        return String::new();
    }
    let secs = (finished_at - started_at) / 1000;
    match secs {
        0..=59 => format!("{secs}s"),
        60..=3599 => format!("{}m", secs / 60),
        _ => format!("{}h {}m", secs / 3600, secs % 3600 / 60),
    }
}

fn days_text(days: &[u8]) -> String {
    let mut days = days.to_vec();
    days.sort_unstable_by_key(|&d| (d + 6) % 7);
    days.dedup();
    match days[..] {
        [1, 2, 3, 4, 5, 6, 0] => "Every day".into(),
        [1, 2, 3, 4, 5] => "Weekdays".into(),
        [6, 0] => "Weekends".into(),
        [d] => DAYS_PLURAL[d as usize].into(),
        _ => days.iter().map(|&d| DAYS[d as usize]).collect::<Vec<_>>().join(", "),
    }
}

fn every(n: u32, unit: &str) -> String {
    if n == 1 { format!("Every {unit}") } else { format!("Every {n} {unit}s") }
}

/// "Weekdays at 09:00", "Fridays at 17:00", "Every 2 hours".
pub fn when_label(s: &Schedule) -> String {
    match s {
        Schedule::Days { days, time } => format!("{} at {time}", days_text(days)),
        Schedule::Interval { every_min } => match every_min {
            m if m % 1440 == 0 => every(m / 1440, "day"),
            m if m % 60 == 0 => every(m / 60, "hour"),
            m => every(*m, "minute"),
        },
    }
}

/// The first time after `now` the schedule fires, as pocketd would work it out on this clock.
pub fn next_fire<Tz: TimeZone>(s: &Schedule, now: i64, tz: &Tz) -> Option<i64> {
    match s {
        Schedule::Interval { every_min } => Some(now + i64::from(*every_min) * 60_000),
        Schedule::Days { days, time } => {
            let (h, m) = time.split_once(':')?;
            let (h, m) = (h.parse().ok()?, m.parse().ok()?);
            let today = local(now, tz).date_naive();
            (0..=7).find_map(|ahead| {
                let date = today + chrono::Days::new(ahead);
                let on = days.contains(&(date.weekday().num_days_from_sunday() as u8));
                let at = tz.from_local_datetime(&date.and_hms_opt(h, m, 0)?).earliest()?.timestamp_millis();
                (on && at > now).then_some(at)
            })
        }
    }
}

/// "Weekdays at 09:00 · next Today 09:00"; just the label while paused.
pub fn trigger_line<Tz: TimeZone>(a: &Automation, now: i64, tz: &Tz) -> String
where
    Tz::Offset: Display,
{
    let label = when_label(&a.schedule);
    if a.enabled && a.next_run_at > 0 { format!("{label} · next {}", stamp(a.next_run_at, now, tz)) } else { label }
}

/// What a run's row says on the right of the last-run mark: the status, or when it finished well.
pub fn last_label<Tz: TimeZone>(r: &Run, now: i64, tz: &Tz) -> String
where
    Tz::Offset: Display,
{
    match r.status {
        RunStatus::Waiting => "Needs you".into(),
        RunStatus::Running | RunStatus::Pending => "Working".into(),
        RunStatus::Succeeded => short_time(r.started_at, now, tz),
        RunStatus::Failed => "Failed".into(),
        RunStatus::Skipped => "Skipped".into(),
        RunStatus::Cancelled => "Stopped".into(),
        RunStatus::Unknown => String::new(),
    }
}

/// "Scheduled 09:00" or "Run now".
pub fn trigger_text<Tz: TimeZone>(r: &Run, tz: &Tz) -> String {
    if r.trigger == "manual" { "Run now".into() } else { format!("Scheduled {}", clock(r.started_at, tz)) }
}

/// "Scheduled 09:00 · Skipped: the Mac was asleep".
pub fn run_reason<Tz: TimeZone>(r: &Run, tz: &Tz) -> String {
    let trigger = trigger_text(r, tz);
    if r.why.is_empty() || r.why == trigger { trigger } else { format!("{trigger} · {}", r.why) }
}

/// How long ago a live run began, or when a finished one did.
pub fn run_when<Tz: TimeZone>(r: &Run, now: i64, tz: &Tz) -> String
where
    Tz::Offset: Display,
{
    match r.status {
        RunStatus::Waiting | RunStatus::Running | RunStatus::Pending => ago(r.started_at, now),
        _ => short_time(r.started_at, now, tz),
    }
}

/// The header's line under a run's status: "3m ago", "Started 09:00 · 41m", "Today 07:41 · 4m".
pub fn run_age_text<Tz: TimeZone>(r: &Run, now: i64, tz: &Tz) -> String
where
    Tz::Offset: Display,
{
    match r.status {
        RunStatus::Waiting => ago_long(r.started_at, now),
        RunStatus::Running | RunStatus::Pending => format!("Started {} · {}", clock(r.started_at, tz), ago(r.started_at, now)),
        _ => [stamp(r.started_at, now, tz), duration_text(r.started_at, r.finished_at)].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · "),
    }
}

/// The statuses of the newest `STRIP` runs, oldest first.
pub fn strip<'a>(newest_first: impl Iterator<Item = &'a Run>) -> Vec<RunStatus> {
    let mut recent: Vec<RunStatus> = newest_first.take(STRIP).map(|r| r.status).collect();
    recent.reverse();
    recent
}

fn first_line(prompt: &str) -> &str {
    prompt.lines().map(str::trim).find(|l| !l.is_empty()).unwrap_or_default()
}

/// A title for an automation left untitled: its prompt's first line.
pub fn fallback_name(prompt: &str) -> String {
    first_line(prompt).chars().take(NAME_CHARS).collect()
}

/// How a run's permission mode reads; empty for a run from before runs kept it.
pub fn access_label(access: &str) -> &'static str {
    match access {
        "ask" => "Ask",
        "edits" => "Edits",
        "auto" => "Auto",
        "full" => "Full",
        "settings" => "Agent's settings",
        _ => "",
    }
}

/// Keeps `selected` while it is listed; if it is gone or unset, the first automation, or none.
pub fn reselect(selected: Option<&str>, autos: &[Automation]) -> Option<String> {
    let kept = selected.and_then(|id| autos.iter().find(|a| a.id == id)).or(autos.first());
    kept.map(|a| a.id.clone())
}

/// The selection a freshly opened screen starts from: the one kept, else the first automation.
pub fn opening_selection(selected: Option<&str>, autos: &[Automation]) -> Option<String> {
    selected.map(str::to_string).or_else(|| autos.first().map(|a| a.id.clone()))
}

/// `only` while that automation is still listed.
pub fn keep_only(only: Option<String>, autos: &[Automation]) -> Option<String> {
    only.filter(|id| autos.iter().any(|a| &a.id == id))
}

/// The run the Runs tab shows, among those of `only` when set: the one selected while it is listed, else the first that needs the user, else the newest.
pub fn shown_run<'a>(selected: Option<&str>, only: Option<&str>, runs: &'a [Run]) -> Option<&'a Run> {
    let mut listed = runs.iter().filter(|r| only.is_none_or(|id| r.automation_id == id));
    let first = listed.clone().next();
    selected.and_then(|id| listed.clone().find(|r| r.id == id)).or_else(|| listed.find(|r| r.status == RunStatus::Waiting)).or(first)
}

/// Where Esc leaves the screen and the editor: the editor closes first, then the screen.
pub fn after_escape(screen: Screen, editing: bool) -> (Screen, bool) {
    match (screen, editing) {
        (Screen::Automations, true) => (Screen::Automations, false),
        (Screen::Automations, false) => (Screen::Sessions, false),
        other => other,
    }
}

/// The screen to show: Automations is left once pocketd doesn't offer it, since it would never fill.
pub fn reachable(screen: Screen, offered: bool) -> Screen {
    if screen == Screen::Automations && !offered { Screen::Sessions } else { screen }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::FixedOffset;

    const MIN: i64 = 60_000;
    const HOUR: i64 = 60 * MIN;
    const DAY_MS: i64 = 24 * HOUR;

    fn auto(id: &str, enabled: bool, next: i64) -> Automation {
        Automation {
            id: id.into(),
            name: id.into(),
            prompt: "p".into(),
            provider: "claude".into(),
            folder: "/code/app".into(),
            schedule: Schedule::Interval { every_min: 60 },
            enabled,
            access: String::new(),
            new_worktree: false,
            next_run_at: next,
        }
    }

    fn run(id: &str, automation: &str, status: RunStatus, started_at: i64) -> Run {
        Run {
            id: id.into(),
            automation_id: automation.into(),
            status,
            trigger: "schedule".into(),
            why: String::new(),
            summary: String::new(),
            started_at,
            finished_at: 0,
            agent_id: String::new(),
            terminal_id: String::new(),
            access: String::new(),
            worktree: String::new(),
        }
    }

    fn utc() -> FixedOffset {
        FixedOffset::east_opt(0).unwrap()
    }

    /// Wednesday 7 Oct 2026, 12:00 UTC.
    const NOW: i64 = 1_791_374_400_000;

    #[test]
    fn a_filter_keeps_the_active_or_the_paused_automations() {
        let autos = [auto("a", true, 1), auto("b", false, 0), auto("c", true, 2)];
        assert_eq!(visible(&autos, Filter::All), [0, 1, 2]);
        assert_eq!(visible(&autos, Filter::Active), [0, 2]);
        assert_eq!(visible(&autos, Filter::Paused), [1]);
    }

    #[test]
    fn the_counts_cover_every_automation_whatever_the_filter() {
        assert_eq!(counts_text(&[auto("a", true, 1), auto("b", false, 0), auto("c", true, 2)]), "2 active · 1 paused");
        assert_eq!(counts_text(&[]), "0 active · 0 paused");
    }

    #[test]
    fn up_next_lists_the_enabled_automations_due_soonest_and_skips_paused_ones() {
        let autos = [auto("late", true, 30), auto("off", false, 0), auto("soon", true, 10), auto("mid", true, 20), auto("last", true, 40)];
        let ids: Vec<_> = up_next(&autos, 3).iter().map(|&i| autos[i].id.as_str()).collect();
        assert_eq!(ids, ["soon", "mid", "late"]);
    }

    #[test]
    fn runs_group_under_sections_in_order_and_empty_sections_are_left_out() {
        let runs = [
            run("today", "a", RunStatus::Succeeded, NOW - HOUR),
            run("wait", "a", RunStatus::Waiting, NOW - 2 * HOUR),
            run("week", "a", RunStatus::Failed, NOW - 3 * DAY_MS),
            run("go", "b", RunStatus::Pending, NOW),
            run("yest", "a", RunStatus::Skipped, NOW - 13 * HOUR),
            run("old", "a", RunStatus::Succeeded, NOW - 9 * DAY_MS),
        ];
        assert_eq!(
            sections(&runs, None, NOW, &utc()),
            [
                Row::Head(Section::NeedsYou, 1),
                Row::Run(1),
                Row::Head(Section::Working, 1),
                Row::Run(3),
                Row::Head(Section::Today, 1),
                Row::Run(0),
                Row::Head(Section::Yesterday, 1),
                Row::Run(4),
                Row::Head(Section::EarlierThisWeek, 1),
                Row::Run(2),
                Row::Head(Section::Older, 1),
                Row::Run(5),
            ]
        );
    }

    #[test]
    fn a_filter_keeps_only_that_automations_runs_and_no_runs_leaves_no_rows() {
        let runs = [run("1", "a", RunStatus::Succeeded, NOW), run("2", "b", RunStatus::Succeeded, NOW)];
        assert_eq!(sections(&runs, Some("b"), NOW, &utc()), [Row::Head(Section::Today, 1), Row::Run(1)]);
        assert_eq!(sections(&runs, Some("c"), NOW, &utc()), []);
    }

    #[test]
    fn the_runs_tab_opens_on_the_soonest_automations_unless_one_automations_runs_are_shown() {
        let autos = [auto("a", true, 20), auto("b", true, 10), auto("c", false, 0)];
        let runs = [run("1", "a", RunStatus::Succeeded, NOW)];
        assert_eq!(run_rows(&autos, &runs, None, NOW, &utc()), [Row::SoonHead, Row::Soon(1), Row::Soon(0), Row::Head(Section::Today, 1), Row::Run(0)]);
        assert_eq!(run_rows(&autos, &runs, Some("a"), NOW, &utc()), [Row::Head(Section::Today, 1), Row::Run(0)]);
        assert_eq!(run_rows(&[auto("c", false, 0)], &[], None, NOW, &utc()), []);
    }

    #[test]
    fn the_day_a_run_belongs_to_follows_the_zone() {
        let runs = [run("1", "a", RunStatus::Succeeded, NOW - 10 * HOUR)];
        let ahead = FixedOffset::east_opt(13 * 3600).unwrap();
        assert_eq!(sections(&runs, None, NOW, &utc())[0], Row::Head(Section::Today, 1));
        assert_eq!(sections(&runs, None, NOW, &ahead)[0], Row::Head(Section::Yesterday, 1));
    }

    #[test]
    fn a_short_time_reads_as_a_clock_yesterday_a_weekday_or_a_date() {
        let at = |ms| short_time(ms, NOW, &utc());
        assert_eq!([at(NOW - 3 * HOUR), at(NOW - 15 * HOUR), at(NOW - 3 * DAY_MS), at(NOW - 9 * DAY_MS)], ["09:00", "Yest.", "Sun", "28 Sep"]);
    }

    #[test]
    fn a_stamp_reads_as_today_tomorrow_a_weekday_or_a_date() {
        let at = |ms| stamp(ms, NOW, &utc());
        assert_eq!(at(NOW - 3 * HOUR), "Today 09:00");
        assert_eq!(at(NOW - 15 * HOUR), "Yesterday 21:00");
        assert_eq!(at(NOW + 21 * HOUR), "Tomorrow 09:00");
        assert_eq!(at(NOW + DAY_MS + 21 * HOUR), "Fri 09:00");
        assert_eq!(at(NOW - 2 * DAY_MS), "Mon 12:00");
        assert_eq!(at(NOW - 9 * DAY_MS), "Mon 28 Sep 12:00");
        assert_eq!(at(0), "");
    }

    #[test]
    fn a_schedule_reads_as_days_at_a_time_or_as_an_interval() {
        let days = |d: &[u8]| when_label(&Schedule::Days { days: d.to_vec(), time: "09:00".into() });
        assert_eq!(days(&[0, 1, 2, 3, 4, 5, 6]), "Every day at 09:00");
        assert_eq!(days(&[5, 4, 3, 2, 1]), "Weekdays at 09:00");
        assert_eq!(days(&[6, 0]), "Weekends at 09:00");
        assert_eq!(days(&[5]), "Fridays at 09:00");
        assert_eq!(days(&[0, 5, 1]), "Mon, Fri, Sun at 09:00");
        let every = |m| when_label(&Schedule::Interval { every_min: m });
        assert_eq!([every(30), every(60), every(120), every(1440), every(2880), every(90)], ["Every 30 minutes", "Every hour", "Every 2 hours", "Every day", "Every 2 days", "Every 90 minutes"]);
    }

    #[test]
    fn the_next_fire_of_days_is_the_next_listed_day_at_that_time_and_of_an_interval_its_length_away() {
        let days = |d: &[u8], time: &str| next_fire(&Schedule::Days { days: d.to_vec(), time: time.into() }, NOW, &utc()).map(|ms| stamp(ms, NOW, &utc()));
        assert_eq!(days(&[1, 2, 3, 4, 5], "13:00").as_deref(), Some("Today 13:00"));
        assert_eq!(days(&[1, 2, 3, 4, 5], "09:00").as_deref(), Some("Tomorrow 09:00"));
        assert_eq!(days(&[1], "09:00").as_deref(), Some("Mon 09:00"));
        assert_eq!(days(&[3], "11:00").as_deref(), Some("Wed 14 Oct 11:00"));
        assert_eq!(days(&[], "09:00"), None);
        assert_eq!(days(&[1], "soon"), None);
        assert_eq!(next_fire(&Schedule::Interval { every_min: 90 }, NOW, &utc()), Some(NOW + 90 * MIN));
    }

    #[test]
    fn a_trigger_line_adds_the_next_run_only_while_enabled() {
        let weekdays = Schedule::Days { days: vec![1, 2, 3, 4, 5], time: "09:00".into() };
        let on = Automation { schedule: weekdays.clone(), ..auto("a", true, NOW + 21 * HOUR) };
        assert_eq!(trigger_line(&on, NOW, &utc()), "Weekdays at 09:00 · next Tomorrow 09:00");
        let off = Automation { schedule: weekdays, ..auto("a", false, 0) };
        assert_eq!(trigger_line(&off, NOW, &utc()), "Weekdays at 09:00");
    }

    #[test]
    fn the_last_run_says_what_needs_attention_and_when_a_good_one_ran() {
        let label = |status, at| last_label(&run("1", "a", status, at), NOW, &utc());
        let got = [label(RunStatus::Waiting, NOW), label(RunStatus::Running, NOW), label(RunStatus::Succeeded, NOW - 3 * HOUR), label(RunStatus::Failed, NOW), label(RunStatus::Skipped, NOW), label(RunStatus::Cancelled, NOW)];
        assert_eq!(got, ["Needs you", "Working", "09:00", "Failed", "Skipped", "Stopped"]);
    }

    #[test]
    fn a_runs_reason_names_what_started_it_and_why_it_did_not_finish() {
        let scheduled = Run { why: "Mac was asleep".into(), ..run("1", "a", RunStatus::Skipped, NOW - 3 * HOUR) };
        assert_eq!(run_reason(&scheduled, &utc()), "Scheduled 09:00 · Mac was asleep");
        let manual = Run { trigger: "manual".into(), ..run("2", "a", RunStatus::Succeeded, NOW) };
        assert_eq!(run_reason(&manual, &utc()), "Run now");
    }

    #[test]
    fn a_live_run_shows_its_age_and_a_finished_one_its_time() {
        let when = |status, at| run_when(&run("1", "a", status, at), NOW, &utc());
        assert_eq!([when(RunStatus::Waiting, NOW - 3 * MIN), when(RunStatus::Running, NOW - 41 * MIN), when(RunStatus::Failed, NOW - 3 * HOUR), when(RunStatus::Failed, NOW - DAY_MS)], ["3m", "41m", "09:00", "Yest."]);
    }

    #[test]
    fn a_runs_header_age_tells_how_long_ago_it_began_or_when_it_ran_and_for_how_long() {
        let age = |status, at: i64, took: i64| run_age_text(&Run { finished_at: if took > 0 { at + took } else { 0 }, ..run("1", "a", status, at) }, NOW, &utc());
        assert_eq!(age(RunStatus::Waiting, NOW - 3 * MIN, 0), "3m ago");
        assert_eq!(age(RunStatus::Running, NOW - 41 * MIN, 0), "Started 11:19 · 41m");
        assert_eq!(age(RunStatus::Succeeded, NOW - 5 * HOUR, 4 * MIN), "Today 07:00 · 4m");
        assert_eq!(age(RunStatus::Skipped, NOW - 20 * HOUR, 0), "Yesterday 16:00");
    }

    #[test]
    fn the_strip_holds_the_newest_twelve_runs_oldest_first() {
        let runs: Vec<Run> = (0..15).map(|i| run(&i.to_string(), "a", if i == 0 { RunStatus::Failed } else { RunStatus::Succeeded }, NOW - i)).collect();
        let got = strip(runs.iter());
        assert_eq!((got.len(), got.first(), got.last()), (12, Some(&RunStatus::Succeeded), Some(&RunStatus::Failed)));
    }

    #[test]
    fn an_untitled_automation_takes_its_prompts_first_line_up_to_forty_characters() {
        assert_eq!(fallback_name("\n Check CI\nmore"), "Check CI");
        assert_eq!(fallback_name(&"y".repeat(50)).len(), 40);
        assert_eq!(fallback_name("  "), "");
    }

    #[test]
    fn a_selection_stays_while_listed_moves_to_the_first_when_gone_and_none_stays_none() {
        let autos = [auto("a", true, 1), auto("b", true, 2)];
        assert_eq!(reselect(Some("b"), &autos).as_deref(), Some("b"));
        assert_eq!(reselect(Some("gone"), &autos).as_deref(), Some("a"));
        assert_eq!(reselect(Some("gone"), &[]), None);
        assert_eq!(reselect(None, &autos).as_deref(), Some("a"));
    }

    #[test]
    fn a_screen_opens_on_the_kept_selection_or_the_first_automation() {
        let autos = [auto("a", true, 1), auto("b", true, 2)];
        assert_eq!(opening_selection(Some("b"), &autos).as_deref(), Some("b"));
        assert_eq!(opening_selection(None, &autos).as_deref(), Some("a"));
        assert_eq!(opening_selection(None, &[]), None);
    }

    #[test]
    fn a_run_filter_stays_only_while_its_automation_is_listed() {
        let autos = [auto("a", true, 1)];
        assert_eq!(keep_only(Some("a".into()), &autos).as_deref(), Some("a"));
        assert_eq!(keep_only(Some("gone".into()), &autos), None);
        assert_eq!(keep_only(None, &autos), None);
    }

    #[test]
    fn the_runs_tab_shows_the_selected_run_else_the_first_that_needs_you_else_the_newest() {
        let runs = [run("new", "a", RunStatus::Succeeded, 3), run("wait", "a", RunStatus::Waiting, 2), run("old", "a", RunStatus::Failed, 1)];
        let id = |sel| shown_run(sel, None, &runs).map(|r| r.id.as_str());
        assert_eq!([id(Some("old")), id(Some("gone")), id(None)], [Some("old"), Some("wait"), Some("wait")]);
        assert_eq!(shown_run(None, None, &runs[..1]).map(|r| r.id.as_str()), Some("new"));
        assert!(shown_run(None, None, &[]).is_none());
    }

    #[test]
    fn the_runs_tab_shows_only_runs_of_the_automation_it_is_filtered_to() {
        let runs = [run("x", "a", RunStatus::Waiting, 3), run("y", "b", RunStatus::Succeeded, 2)];
        assert_eq!(shown_run(Some("x"), Some("b"), &runs).map(|r| r.id.as_str()), Some("y"));
        assert!(shown_run(None, Some("c"), &runs).is_none());
    }

    #[test]
    fn escape_closes_the_editor_then_leaves_the_screen() {
        assert_eq!(after_escape(Screen::Automations, true), (Screen::Automations, false));
        assert_eq!(after_escape(Screen::Automations, false), (Screen::Sessions, false));
    }

    #[test]
    fn automations_is_left_only_when_pocketd_stops_offering_it() {
        assert_eq!(reachable(Screen::Automations, false), Screen::Sessions);
        assert_eq!(reachable(Screen::Automations, true), Screen::Automations);
        assert_eq!(reachable(Screen::Inbox, false), Screen::Inbox);
    }

    #[test]
    fn duration_reads_in_seconds_minutes_or_hours_and_is_empty_until_finished() {
        let got = [duration_text(0, 0), duration_text(1000, 43_000), duration_text(0, 7 * 60_000 + 59_000), duration_text(0, HOUR + 5 * 60_000)];
        assert_eq!(got, ["", "42s", "7m", "1h 5m"]);
    }

    #[test]
    fn a_run_names_its_mode_and_an_older_run_names_none() {
        assert_eq!([access_label("edits"), access_label("settings"), access_label("")], ["Edits", "Agent's settings", ""]);
    }
}
