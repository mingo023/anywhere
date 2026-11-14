use super::catalog::Setting;
use super::{field_box, row};
use crate::desktop::Desktop;
use agents::Config;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::Duration;
use theme::*;

/// The most check commands a project can have, and the longest one.
const MAX_CHECKS: usize = 16;
const MAX_CHECK: usize = 400;
const MAX_SLOTS: u8 = 8;
const MAX_FIX: u8 = 2;

/// How long a change waits for the next before it is sent: each save that keeps the Factory off asks gh.
const SAVE_AFTER: Duration = Duration::from_millis(400);

pub(super) const ROWS: &[Setting] = &[
    Setting::custom("factory-project", "Project", "Project", |d, s, cx| d.factory_project_row(s, cx)),
    Setting::custom("factory-enabled", "Pipeline", "Factory", |d, s, cx| d.factory_enabled_row(s, cx)).hint("Off: this project's tickets behave as they do today"),
    Setting::custom("factory-stations", "Pipeline", "Stations", |d, s, _| d.factory_stations_row(s)),
    Setting::custom("factory-spec", "Pipeline", "Spec station", |d, s, cx| d.factory_switch_row(s, |c| c.spec, |c, on| c.spec = on, cx)).hint("The lead writes a brief and acceptance criteria before it plans"),
    Setting::custom("factory-plan-gate", "Pipeline", "Plan gate", |_, s, _| always_on(s)).hint("No worker starts until you approve the lead's plan"),
    Setting::custom("factory-pr-gate", "Pipeline", "PR gate", |_, s, _| always_on(s)).hint("You review the PR and merge it. Agents never merge"),
    Setting::custom("factory-reviewer", "Pipeline", "Reviewer", |d, s, cx| d.factory_switch_row(s, |c| c.reviewer, |c, on| c.reviewer = on, cx)).hint("A separate read-only agent comments before the PR gate"),
    Setting::custom("factory-slots", "Limits", "Slot limit", |d, s, cx| d.factory_slots_row(s, cx)).hint("Tickets an agent works on at once. A ticket that needs you holds no slot"),
    Setting::custom("factory-fix", "Limits", "Auto-fix attempts", |d, s, cx| d.factory_fix_row(s, cx)).hint("Tries by the lead before failed checks come to you"),
    Setting::custom("factory-checks", "Check commands", "Check commands", |d, _, cx| d.factory_checks_rows(cx)),
];

/// The Factory section's state: the project shown, your change until pocketd answers, and why each project can't turn the Factory on.
pub(crate) struct FactorySettings {
    pub(crate) project: Option<String>,
    pub(crate) draft: Draft,
    /// The sentence from pocketd's last `factory.config`, by project; empty when the Factory can be turned on.
    problems: HashMap<String, String>,
    /// The project menu is open.
    pub(crate) menu: bool,
    /// Where a new check command is typed.
    command: Entity<InputState>,
    /// The save that waits for `SAVE_AFTER`; a newer change drops it.
    timer: Option<Task<()>>,
}

/// A project's settings as you changed them, until pocketd answers the save.
#[derive(Default, Debug)]
pub(crate) struct Draft {
    edit: Option<(String, Config)>,
    sent: bool,
}

impl Draft {
    /// What the page shows for `project`: your change while it waits, else what pocketd keeps.
    pub(crate) fn shown<'a>(&'a self, project: &str, saved: Option<&'a Config>) -> Option<&'a Config> {
        match &self.edit {
            Some((p, c)) if p == project => Some(c),
            _ => saved,
        }
    }

    pub(crate) fn change(&mut self, project: &str, config: Config) {
        (self.edit, self.sent) = (Some((project.into(), config)), false);
    }

    /// The change to send, once.
    pub(crate) fn unsent(&mut self) -> Option<(String, Config)> {
        if self.sent {
            return None;
        }
        self.sent = true;
        self.edit.clone()
    }

    /// pocketd answered with `project`'s settings: a sent change gives way to them.
    pub(crate) fn answered(&mut self, project: &str) {
        if self.sent && self.edit.as_ref().is_some_and(|(p, _)| p == project) {
            self.edit = None;
        }
    }

    /// pocketd refused the save: the saved settings show again.
    pub(crate) fn refused(&mut self) {
        if self.sent {
            self.edit = None;
        }
    }
}

/// "2 gates · auto-fix 2× · 4 slots". `board` is the Board's limit, which a project limit of 0 follows.
pub(crate) fn summary(c: &Config, board: usize) -> String {
    let fix = if c.fix == 0 { "auto-fix off".to_string() } else { format!("auto-fix {}×", c.fix) };
    let slots = if c.slots == 0 { board } else { c.slots as usize };
    let mut line = format!("2 gates · {fix} · {}", crate::team::plural(slots, "slot", "slots"));
    if !c.enabled {
        line.push_str(" · off for this project");
    }
    line
}

pub(crate) fn pipeline(spec: bool) -> String {
    let stations: &[&str] = if spec { &["To do", "Spec", "Plan approval", "Implement", "Checks", "PR review", "Merged"] } else { &["To do", "Plan approval", "Implement", "Checks", "PR review", "Merged"] };
    stations.join(" · ")
}

pub(crate) fn step_slots(n: u8, by: i32) -> u8 {
    (n as i32 + by).clamp(0, MAX_SLOTS as i32) as u8
}

pub(crate) fn slots_label(n: u8) -> String {
    if n == 0 { "Board limit".into() } else { n.to_string() }
}

pub(crate) fn step_fix(n: u8, by: i32) -> u8 {
    (n as i32 + by).clamp(0, MAX_FIX as i32) as u8
}

pub(crate) fn fix_label(n: u8) -> String {
    if n == 0 { "Off".into() } else { n.to_string() }
}

/// Adds `typed` to the end of the checks; false when it is blank, too long, there already, or one too many.
pub(crate) fn add_check(c: &mut Config, typed: &str) -> bool {
    let command = typed.trim();
    let fits = !command.is_empty() && command.chars().count() <= MAX_CHECK && c.checks.len() < MAX_CHECKS;
    if !fits || c.checks.iter().any(|x| x == command) {
        return false;
    }
    c.checks.push(command.into());
    true
}

pub(crate) fn remove_check(c: &mut Config, i: usize) {
    if i < c.checks.len() {
        c.checks.remove(i);
    }
}

impl FactorySettings {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let command = cx.new(|cx| InputState::new(window, cx).placeholder("Add a command"));
        let typed = cx.subscribe_in(&command, window, |this, _, ev: &InputEvent, window, cx| {
            if let InputEvent::PressEnter { .. } = ev {
                this.factory_add_check(window, cx);
            }
        });
        (Self { project: None, draft: Draft::default(), problems: HashMap::new(), menu: false, command, timer: None }, vec![typed])
    }

    pub(crate) fn answered(&mut self, project: &str, problem: &str) {
        self.draft.answered(project);
        self.problems.insert(project.into(), problem.into());
    }
}

fn always_on(s: &'static Setting) -> Div {
    row(s.label, s.hint, div().flex().items_center().gap(px(5.)).text_size(px(12.5)).text_color(TEXT_3).child(icon("shield", 12., TEXT_3)).child("Always on"))
}

/// What a stepper's button does.
type Act = Box<dyn Fn(&mut Desktop, &mut Context<Desktop>)>;

/// A − n + control, as `Control::Stepper` draws it. A side with no `Act` is dimmed.
fn stepper(id: &'static str, label: String, less: Option<Act>, more: Option<Act>, cx: &mut Context<Desktop>) -> Div {
    let button = |side: &'static str, glyph: &'static str, act: Option<Act>| {
        let enabled = act.is_some();
        div()
            .id(SharedString::from(format!("{id}-{side}")))
            .size(px(22.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .text_size(px(14.))
            .text_color(if enabled { TEXT_2 } else { TEXT_4 })
            .when(enabled, |d| d.cursor_pointer().hover(|d| d.bg(FILL_3)))
            .child(glyph)
            .when_some(act, |d, act| d.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| act(this, cx))))
    };
    let less = button("less", "\u{2212}", less);
    let more = button("more", "+", more);
    div()
        .h(px(26.))
        .px(px(2.))
        .flex()
        .items_center()
        .gap(px(2.))
        .rounded(px(7.))
        .bg(FILL_2)
        .child(less)
        .child(div().min_w(px(76.)).flex().justify_center().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(label))
        .child(more)
}

impl Desktop {
    /// Shows `project`, else the one shown, else the first project, and asks pocketd for its settings.
    pub(crate) fn open_factory_project(&mut self, project: Option<String>, cx: &mut Context<Self>) {
        let projects = self.projects();
        let pick = project.or_else(|| self.settings.factory.project.clone().filter(|p| projects.contains(p))).or_else(|| projects.first().cloned());
        self.settings.factory.menu = false;
        self.settings.factory.project = pick.clone();
        if let Some(p) = pick.filter(|_| self.agents.factory_offered()) {
            self.outbox.local.factory_config(&p);
        }
        cx.notify();
    }

    /// The shown project and its settings as the page shows them.
    fn factory_shown(&self) -> Option<(String, Config)> {
        let project = self.settings.factory.project.clone()?;
        let saved = self.agents.factory_config(&project);
        let config = self.settings.factory.draft.shown(&project, saved).cloned().unwrap_or_default();
        Some((project, config))
    }

    fn factory_set(&mut self, config: Config, cx: &mut Context<Self>) {
        let Some(project) = self.settings.factory.project.clone().filter(|_| self.agents.factory_offered()) else { return };
        self.settings.factory.draft.change(&project, config);
        self.settings.factory.timer = Some(cx.spawn(async move |this, cx| {
            cx.background_executor().timer(SAVE_AFTER).await;
            let _ = this.update(cx, |d, _| d.factory_send());
        }));
        cx.notify();
    }

    fn factory_change(&mut self, change: impl FnOnce(&mut Config), cx: &mut Context<Self>) {
        if let Some((_, mut config)) = self.factory_shown() {
            change(&mut config);
            self.factory_set(config, cx);
        }
    }

    fn factory_send(&mut self) {
        self.settings.factory.timer = None;
        if let Some((project, config)) = self.settings.factory.draft.unsent() {
            self.outbox.local.factory_save(&project, &config);
        }
    }

    fn factory_add_check(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let typed = self.settings.factory.command.read(cx).value().to_string();
        let Some((_, mut config)) = self.factory_shown() else { return };
        if add_check(&mut config, &typed) {
            self.factory_set(config, cx);
            self.settings.factory.command.update(cx, |s, cx| s.set_value("", window, cx));
        }
    }

    fn factory_project_row(&mut self, _: &'static Setting, cx: &mut Context<Self>) -> Div {
        let shown = self.factory_shown();
        let name = shown.as_ref().map_or_else(|| "No project".to_string(), |(p, _)| self.repo_name(p));
        let hint = match &shown {
            _ if !self.agents.factory_offered() => "This Mac's pocketd doesn't run the Factory".to_string(),
            Some((_, c)) => summary(c, self.agents.board.limit),
            None => "Add a project to set up its Factory".to_string(),
        };
        let button = div()
            .id("factory-project-pick")
            .h(px(26.))
            .px(px(10.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(7.))
            .bg(FILL_2)
            .cursor_pointer()
            .hover(|d| d.bg(FILL_3))
            .text_size(px(12.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(TEXT)
            .child(name)
            .child(icon("chevron-down", 12., TEXT_3))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.settings.factory.menu = !this.settings.factory.menu;
                cx.notify();
            }));
        let menu = self.settings.factory.menu.then(|| {
            let rows = self.projects().into_iter().enumerate().map(|(i, p)| {
                let label = self.repo_name(&p);
                div().id(("factory-project", i)).h(px(28.)).px(px(10.)).flex().items_center().rounded(px(6.)).cursor_pointer().hover(|d| d.bg(FILL_2)).text_size(px(13.)).child(label).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_factory_project(Some(p.clone()), cx)))
            });
            ui::pop(div()).absolute().top(px(30.)).right_0().min_w(px(200.)).p(px(4.)).flex().flex_col().children(rows)
        });
        row("Project", Some(&hint), div().relative().child(button).children(menu))
    }

    fn factory_enabled_row(&mut self, s: &'static Setting, cx: &mut Context<Self>) -> Div {
        let (project, config) = self.factory_shown().unwrap_or_default();
        let problem = self.settings.factory.problems.get(&project).filter(|p| !p.is_empty()).cloned();
        // A problem only blocks turning it on: a Factory that is on can always be turned off.
        let blocked = problem.is_some() && !config.enabled;
        let switch = div()
            .id("factory-enabled")
            .when(blocked, |d| d.opacity(0.5))
            .when(!blocked, |d| d.cursor_pointer().on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.factory_change(|c| c.enabled = !c.enabled, cx))))
            .child(ui::toggle("factory-enabled-switch", config.enabled));
        row(s.label, problem.as_deref().filter(|_| blocked).or(s.hint), switch)
    }

    fn factory_stations_row(&mut self, s: &'static Setting) -> Div {
        let spec = self.factory_shown().is_none_or(|(_, c)| c.spec);
        row(s.label, Some(&pipeline(spec)), div())
    }

    fn factory_switch_row(&mut self, s: &'static Setting, get: fn(&Config) -> bool, set: fn(&mut Config, bool), cx: &mut Context<Self>) -> Div {
        let on = self.factory_shown().is_some_and(|(_, c)| get(&c));
        let switch = div().id(SharedString::from(format!("{}-switch", s.id))).cursor_pointer().child(ui::toggle(s.id, on)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.factory_change(|c| set(c, !get(c)), cx)));
        row(s.label, s.hint, switch)
    }

    fn factory_slots_row(&mut self, s: &'static Setting, cx: &mut Context<Self>) -> Div {
        let n = self.factory_shown().map_or(0, |(_, c)| c.slots);
        let less = (n > 0).then(|| Box::new(|d: &mut Desktop, cx: &mut Context<Desktop>| d.factory_change(|c| c.slots = step_slots(c.slots, -1), cx)) as Act);
        let more = (n < MAX_SLOTS).then(|| Box::new(|d: &mut Desktop, cx: &mut Context<Desktop>| d.factory_change(|c| c.slots = step_slots(c.slots, 1), cx)) as Act);
        row(s.label, s.hint, stepper(s.id, slots_label(n), less, more, cx))
    }

    fn factory_fix_row(&mut self, s: &'static Setting, cx: &mut Context<Self>) -> Div {
        let n = self.factory_shown().map_or(0, |(_, c)| c.fix);
        let less = (n > 0).then(|| Box::new(|d: &mut Desktop, cx: &mut Context<Desktop>| d.factory_change(|c| c.fix = step_fix(c.fix, -1), cx)) as Act);
        let more = (n < MAX_FIX).then(|| Box::new(|d: &mut Desktop, cx: &mut Context<Desktop>| d.factory_change(|c| c.fix = step_fix(c.fix, 1), cx)) as Act);
        row(s.label, s.hint, stepper(s.id, fix_label(n), less, more, cx))
    }

    fn factory_checks_rows(&mut self, cx: &mut Context<Self>) -> Div {
        let checks = self.factory_shown().map(|(_, c)| c.checks).unwrap_or_default();
        let line = |i: usize, command: String, cx: &mut Context<Self>| {
            let remove = ui::icon_button_sized(("factory-check-remove", i), "x", 20., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.factory_change(|c| remove_check(c, i), cx)));
            row(div().font_family(MONO).text_size(px(12.)).font_weight(FontWeight::NORMAL).child(command), None, remove).border_b(px(0.5)).border_color(SEPARATOR)
        };
        let lines: Vec<Div> = checks.into_iter().enumerate().map(|(i, command)| line(i, command, cx)).collect();
        let field = field_box().w_full().child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(&self.settings.factory.command).appearance(false).p_0().text_size(px(12.))));
        div().flex().flex_col().children(lines).child(div().px(px(14.)).py(px(10.)).child(field))
    }
}

#[cfg(test)]
mod tests {
    use super::{add_check, fix_label, pipeline, remove_check, slots_label, step_fix, step_slots, summary, Draft, MAX_CHECK, MAX_CHECKS, ROWS};
    use agents::Config;

    fn on() -> Config {
        Config { enabled: true, spec: true, reviewer: true, slots: 4, fix: 2, ..Config::default() }
    }

    #[test]
    fn the_summary_counts_the_gates_the_fix_attempts_and_the_slots() {
        assert_eq!(summary(&on(), 4), "2 gates · auto-fix 2× · 4 slots");
        assert_eq!(summary(&Config { fix: 0, slots: 0, ..on() }, 3), "2 gates · auto-fix off · 3 slots");
        assert_eq!(summary(&Config { enabled: false, slots: 1, ..on() }, 4), "2 gates · auto-fix 2× · 1 slot · off for this project");
    }

    #[test]
    fn the_pipeline_leaves_out_spec_when_the_station_is_off() {
        assert_eq!(pipeline(true), "To do · Spec · Plan approval · Implement · Checks · PR review · Merged");
        assert_eq!(pipeline(false), "To do · Plan approval · Implement · Checks · PR review · Merged");
    }

    #[test]
    fn the_slot_stepper_goes_from_the_board_limit_to_eight() {
        assert_eq!((step_slots(0, 1), step_slots(1, -1), step_slots(8, 1), step_slots(0, -1)), (1, 0, 8, 0));
        assert_eq!((slots_label(0), slots_label(3)), ("Board limit".to_string(), "3".to_string()));
    }

    #[test]
    fn auto_fix_takes_zero_to_two_attempts() {
        assert_eq!((step_fix(2, 1), step_fix(0, -1), step_fix(1, 1)), (2, 0, 2));
        assert_eq!((fix_label(0), fix_label(2)), ("Off".to_string(), "2".to_string()));
    }

    #[test]
    fn a_check_command_is_added_once_trimmed_and_never_blank() {
        let mut c = on();
        assert!(add_check(&mut c, "  cargo test \n"));
        assert!(!add_check(&mut c, "cargo test"));
        assert!(!add_check(&mut c, "   "));
        assert!(!add_check(&mut c, &"x".repeat(MAX_CHECK + 1)));
        assert_eq!(c.checks, ["cargo test"]);
        for i in 1..MAX_CHECKS {
            assert!(add_check(&mut c, &format!("make t{i}")));
        }
        assert!(!add_check(&mut c, "one too many"));
        remove_check(&mut c, 0);
        remove_check(&mut c, 99);
        assert_eq!(c.checks.len(), MAX_CHECKS - 1);
        assert_eq!(c.checks[0], "make t1");
    }

    #[test]
    fn a_draft_shows_until_pocketd_answers_its_save() {
        let saved = on();
        let mut d = Draft::default();
        assert_eq!(d.shown("/app", Some(&saved)), Some(&saved));
        d.change("/app", Config { fix: 0, ..on() });
        assert_eq!(d.shown("/app", Some(&saved)).map(|c| c.fix), Some(0));
        assert_eq!(d.shown("/other", None), None);
        assert_eq!(d.unsent().map(|(p, c)| (p, c.fix)), Some(("/app".to_string(), 0)));
        assert!(d.unsent().is_none());
        d.answered("/other");
        assert_eq!(d.shown("/app", Some(&saved)).map(|c| c.fix), Some(0));
        d.answered("/app");
        assert_eq!(d.shown("/app", Some(&saved)), Some(&saved));
    }

    #[test]
    fn a_change_after_the_send_outlives_the_answer_to_the_older_save() {
        let mut d = Draft::default();
        d.change("/app", Config { fix: 0, ..on() });
        d.unsent();
        d.change("/app", Config { fix: 1, ..on() });
        d.answered("/app");
        assert_eq!(d.shown("/app", None).map(|c| c.fix), Some(1));
        assert!(d.unsent().is_some());
    }

    #[test]
    fn a_refused_save_shows_the_saved_settings_again() {
        let mut d = Draft::default();
        d.change("/app", Config { enabled: true, ..Config::default() });
        d.unsent();
        d.refused();
        assert_eq!(d.shown("/app", Some(&Config::default())).map(|c| c.enabled), Some(false));
    }

    use crate::settings::catalog::Control;

    #[test]
    fn every_factory_row_is_drawn_by_the_section_so_none_has_a_reset() {
        assert!(ROWS.iter().all(|r| matches!(r.control, Control::Custom(_)) && r.resets.is_none()));
        let groups = crate::settings::catalog::groups(ROWS);
        assert_eq!(groups, ["Project", "Pipeline", "Limits", "Check commands"]);
    }
}
