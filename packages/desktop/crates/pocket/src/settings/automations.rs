use super::catalog::{Look, Setting};
use super::{action, card, field_box, row};
use crate::automations::logic::{stamp, up_next};
use crate::desktop::Desktop;
use crate::util::now_ms;
use agents::automations::Automation;
use chrono::Local;
use std::time::{Duration, Instant};
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::LaunchPick;
use store::prefs::automations::ACCESSES;
use theme::*;

const DEFAULTS: &str = "Defaults for new automations";
/// The grace options in hours; 0, catching up off, never runs a missed run late.
const GRACE: [u32; 4] = [1, 6, 12, 24];
/// The grace catching up turns back on with.
const CATCH_UP: u32 = 12;
/// How long Start service shows it's starting before the card says off again.
const STARTING: Duration = Duration::from_secs(20);
/// Monday first, as 0-is-Sunday days.
const WEEK: [(u8, &str); 7] = [(1, "M"), (2, "T"), (3, "W"), (4, "T"), (5, "F"), (6, "S"), (0, "S")];

pub(super) const ROWS: &[Setting] = &[
    Setting::choice("automation-agent", DEFAULTS, "Agent", &["Claude Code", "Codex"], Look::Dropdown, |s| {
        LaunchPick::PROVIDERS.iter().position(|p| *p == s.automations.agent).unwrap_or(0)
    }, |s, i| s.automations.agent = LaunchPick::PROVIDERS[i].into())
    .hint("The agent a new automation starts with")
    .offers(super::agents::enabled_provider),
    Setting::custom("automation-schedule", DEFAULTS, "Schedule", |d, s, cx| d.schedule_row(s, cx))
        .resets(|s| format!("{}, {} days", s.automations.time, s.automations.days.len()), |s, d| (s.automations.time, s.automations.days) = (d.automations.time.clone(), d.automations.days.clone())).hint("For automations that run at a time of day"),
    Setting::stepper("automation-every", DEFAULTS, "Repeat interval", (5, 10080, 5), " min", |s| s.automations.every as i32, |s, n| s.automations.every = n as u32)
        .hint("For automations that repeat through the day"),
    Setting::choice("automation-access", DEFAULTS, "Permission mode", &["Ask", "Edits", "Auto"], Look::Segmented, |s| {
        ACCESSES.iter().position(|a| *a == s.automations.access).unwrap_or(1)
    }, |s, i| s.automations.access = ACCESSES[i].into())
    .hint("Ask pauses the run until you approve it from the Mac or phone"),
    Setting::switch("automation-worktree", DEFAULTS, "Run in a new worktree", |s| s.automations.new_worktree, |s, on| s.automations.new_worktree = on)
        .hint("Each run gets its own worktree and branch"),
    Setting::switch("catch-up", "Missed runs", "Catch up after sleep", |s| s.automations.grace_hours > 0, |s, on| s.automations.grace_hours = if on { CATCH_UP } else { 0 })
        .hint("Runs a missed schedule once when the Mac wakes")
        .effect(set_grace),
    Setting::choice("missed-grace", "Missed runs", "Run late if missed by less than", &["1 hour", "6 hours", "12 hours", "24 hours"], Look::Dropdown, |s| {
        grace_index(s.automations.grace_hours)
    }, |s, i| s.automations.grace_hours = GRACE[i])
    .hint("Older missed runs are skipped, as when the Mac was asleep")
    .under(|s| s.automations.grace_hours > 0)
    .effect(set_grace),
    Setting::stepper("run-history", "History", "Keep run history", (10, 500, 10), " runs", |s| s.automations.history as i32, |s, n| s.automations.history = n as u32)
        .hint("Per automation; older runs leave the list")
        .advanced()
        .effect(|d, cx| d.set_host("automations.history", d.store.automations.history.to_string(), cx)),
];

fn grace_index(hours: u32) -> usize {
    GRACE.iter().position(|h| *h == hours).unwrap_or(2)
}

fn set_grace(d: &mut Desktop, cx: &mut Context<Desktop>) {
    d.set_host("automations.graceHours", d.store.automations.grace_hours.to_string(), cx)
}

/// "3 automations · next run Thu 09:00".
fn service_line(count: usize, next: Option<String>) -> String {
    let count = match count {
        0 => "No automations yet".to_string(),
        1 => "1 automation".to_string(),
        n => format!("{n} automations"),
    };
    next.map_or(count.clone(), |at| format!("{count} · next run {at}"))
}

/// Automations whose run came due while pocketd was off.
fn missed(autos: &[Automation], now: i64) -> usize {
    autos.iter().filter(|a| a.enabled && a.next_run_at > 0 && a.next_run_at <= now).count()
}

/// Why nothing runs while pocketd is off, and what happens to what it missed.
fn off_line(missed: usize, catch_up: bool) -> String {
    let lead = "No automation runs until it\u{2019}s on.";
    match (missed, catch_up) {
        (0, _) => lead.into(),
        (1, true) => format!("{lead} 1 run missed so far will catch up if it\u{2019}s within the grace window."),
        (n, true) => format!("{lead} {n} runs missed so far will catch up if they are within the grace window."),
        (1, false) => format!("{lead} 1 run missed so far will be skipped."),
        (n, false) => format!("{lead} {n} runs missed so far will be skipped."),
    }
}

impl Desktop {
    fn start_service(&mut self, cx: &mut Context<Self>) {
        self.settings.starting = Some(Instant::now());
        self.restart_service(cx);
        cx.notify();
    }

    /// pocketd is off: what that means for runs, and a button to start it.
    fn service_off(&mut self, cx: &mut Context<Self>) -> Div {
        let starting = self.settings.starting.is_some_and(|t| t.elapsed() < STARTING);
        let (title, line) = if starting {
            ("Starting Anywhere\u{2019}s background service\u{2026}".to_string(), "This usually takes a few seconds".to_string())
        } else {
            let missed = missed(&self.agents.automations.items, now_ms());
            ("Anywhere\u{2019}s background service is off".to_string(), off_line(missed, self.store.automations.grace_hours > 0))
        };
        let start = (!starting).then(|| action("settings-start-service", "Start service").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.start_service(cx))));
        let text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(title))
            .child(div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_2).child(line));
        let mark = if starting { ui::dot(8., WAITING_DOT).into_any_element() } else { icon("warning", 15., FAILED).into_any_element() };
        div()
            .px(px(14.))
            .py(px(12.))
            .flex()
            .items_center()
            .gap(px(12.))
            .rounded(px(12.))
            .bg(if starting { FILL_1 } else { FAILED_BG })
            .shadow(vec![ui::ring(SEPARATOR, 0.5)])
            .child(div().w(px(16.)).flex().flex_none().justify_center().child(mark))
            .child(text)
            .children(start)
    }

    pub(crate) fn automations_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let service = if self.terminals.link.is_down() { self.service_off(cx) } else { card(vec![self.runs_row(cx)]) };
        div().flex().flex_col().gap(px(22.)).child(service).children(self.setting_groups(super::Section::Automations, cx))
    }

    fn runs_row(&mut self, cx: &mut Context<Self>) -> Div {
        let items = &self.agents.automations.items;
        let next = up_next(items, 1).first().map(|&i| stamp(items[i].next_run_at, now_ms(), &Local));
        let title = div().flex().items_center().gap(px(8.)).child(ui::dot(8., SUCCESS)).child("Anywhere's background service is on");
        let open = self.agents.automations_offered().then(|| {
            action("settings-open-automations", "Open Automations")
                .gap(px(6.))
                .child(icon("external", 12., TEXT_3))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_automations(&crate::actions::OpenAutomations, window, cx)))
        });
        let line = service_line(items.len(), next);
        row(title, Some(&line), div().children(open))
    }

    fn schedule_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let time = field_box().w(px(64.)).child(Input::new(&self.settings.host.time).appearance(false).p_0().font_family(MONO).text_size(px(12.)));
        let days = WEEK.map(|(day, letter)| {
            let on = self.store.automations.days.contains(&day);
            div()
                .id(("schedule-day", day as usize))
                .size(px(22.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .text_size(px(11.))
                .font_weight(FontWeight::SEMIBOLD)
                .when(on, |d| d.bg(TEXT).text_color(ON_TEXT))
                .when(!on, |d| d.bg(FILL_3).text_color(TEXT_3))
                .child(letter)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.store.automations.toggle_day(day);
                    this.save_soon(cx);
                    cx.notify();
                }))
        });
        row(setting.label, setting.hint, div().flex().items_center().gap(px(10.)).child(time).child(div().flex().gap(px(4.)).children(days)))
    }
}

#[cfg(test)]
mod tests {
    use super::super::catalog::{Control, Setting};
    use super::{ROWS, grace_index, missed, off_line, service_line};
    use agents::automations::{Automation, Schedule};
    use store::Store;

    fn row(id: &str) -> &'static Setting {
        ROWS.iter().find(|r| r.id == id).unwrap()
    }

    #[test]
    fn catching_up_off_never_runs_late_and_on_runs_late_by_twelve_hours() {
        let Control::Switch { get, set } = row("catch-up").control else { panic!() };
        let mut store = Store::default();
        store.automations.grace_hours = 24;
        set(&mut store, false);
        assert_eq!((store.automations.grace_hours, get(&store), row("missed-grace").allowed(&store)), (0, false, false));
        set(&mut store, true);
        assert_eq!((store.automations.grace_hours, get(&store), row("missed-grace").allowed(&store)), (12, true, true));
    }

    #[test]
    fn the_permission_mode_reads_back_what_it_writes_and_an_unknown_one_reads_as_edits() {
        let Control::Choice { get, set, .. } = row("automation-access").control else { panic!() };
        let mut store = Store::default();
        assert_eq!((get(&store), store.automations.access.as_str()), (1, "edits"));
        set(&mut store, 0);
        assert_eq!((get(&store), store.automations.access.as_str()), (0, "ask"));
        store.automations.access = "full".into();
        assert_eq!(get(&store), 1);
    }

    #[test]
    fn odd_hours_show_the_default_grace() {
        assert_eq!((grace_index(1), grace_index(24), grace_index(7)), (0, 3, 2));
    }

    #[test]
    fn the_service_line_counts_automations_and_names_the_next_run() {
        assert_eq!(service_line(0, None), "No automations yet");
        assert_eq!(service_line(3, Some("Thu 09:00".into())), "3 automations · next run Thu 09:00");
    }

    #[test]
    fn runs_that_came_due_while_pocketd_was_off_count_as_missed() {
        let auto = |enabled, next_run_at| Automation {
            id: "a".into(),
            name: "a".into(),
            prompt: String::new(),
            provider: "claude".into(),
            folder: "/".into(),
            schedule: Schedule::Interval { every_min: 60 },
            enabled,
            access: String::new(),
            new_worktree: false,
            next_run_at,
        };
        assert_eq!(missed(&[auto(true, 100), auto(true, 300), auto(false, 100), auto(true, 0)], 200), 1);
        assert_eq!(off_line(2, true), "No automation runs until it\u{2019}s on. 2 runs missed so far will catch up if they are within the grace window.");
        assert_eq!(off_line(1, false), "No automation runs until it\u{2019}s on. 1 run missed so far will be skipped.");
    }
}
