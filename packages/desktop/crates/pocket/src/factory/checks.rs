//! The Checks tab: one row per local command, the failure and its attempts, then the PR's checks on GitHub.

use super::spec::heading;
use crate::board::logic::Tone;
use crate::desktop::Desktop;
use agents::factory::Check;
use agents::tickets::{Status, Ticket};
use git::github::{Check as Run, Outcome};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// A configured command's mark in the Checks tab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    Passed,
    Failed,
    /// It failed and the lead is fixing it.
    Fixing,
    /// It has not run on this commit yet.
    Waiting,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Row {
    pub(crate) command: String,
    pub(crate) mark: Mark,
    pub(crate) exit: i32,
}

/// The end of a failed command's output, cut once when the detail arrives.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Output {
    pub(crate) command: String,
    pub(crate) exit: i32,
    pub(crate) text: String,
    /// The lines in `text`, and in the whole output.
    pub(crate) shown: usize,
    pub(crate) total: usize,
}

/// One row per configured command, marked by the check at the same place in the newest record. A record of other
/// commands, from before the settings changed, marks nothing. Without the settings, the commands that ran.
pub(crate) fn rows(commands: &[String], checks: &[Check], fixing: bool) -> Vec<Row> {
    let ran: Vec<String> = checks.iter().map(|c| c.command.clone()).collect();
    let commands = if commands.is_empty() { &ran[..] } else { commands };
    commands
        .iter()
        .enumerate()
        .map(|(i, command)| {
            let check = checks.get(i).filter(|c| &c.command == command);
            let mark = match check {
                None => Mark::Waiting,
                Some(c) if c.exit == 0 => Mark::Passed,
                Some(_) if fixing => Mark::Fixing,
                Some(_) => Mark::Failed,
            };
            Row { command: command.clone(), mark, exit: check.map_or(0, |c| c.exit) }
        })
        .collect()
}

pub(crate) fn passed(rows: &[Row]) -> usize {
    rows.iter().filter(|r| r.mark == Mark::Passed).count()
}

/// Whether the lead works on a fix note: pocketd says so in `why`.
pub(crate) fn fixing(t: &Ticket) -> bool {
    t.status == Status::Working && t.why.starts_with("Fixing · ")
}

/// "Auto-fix 1 of 2 used", or what the hint granted.
pub(crate) fn attempt_line(t: &Ticket, cap: u8) -> String {
    match () {
        _ if t.hint && fixing(t) => "One attempt granted by you".into(),
        _ if cap == 0 => "Auto-fix is off".into(),
        _ => format!("Auto-fix {} of {cap} used", t.fix.min(cap)),
    }
}

/// What failed, from a failed Ticket's `why`: the part before " · ".
pub(crate) fn what(why: &str) -> &str {
    match why.split(" · ").next() {
        Some(w) if !w.is_empty() => w,
        _ => "Checks failed",
    }
}

/// The last `lines` lines of `check`'s output.
pub(crate) fn tail(check: &Check, lines: usize) -> Output {
    let all: Vec<&str> = check.output.trim_end_matches('\n').lines().collect();
    let shown = &all[all.len().saturating_sub(lines)..];
    Output { command: check.command.clone(), exit: check.exit, text: shown.join("\n"), shown: shown.len(), total: all.len() }
}

/// The line above a cut output.
pub(crate) fn hidden(out: &Output) -> Option<String> {
    (out.shown < out.total).then(|| format!("Showing the last {} of {} lines", out.shown, out.total))
}

/// The tail of the command that failed, if one did.
pub(crate) fn failed_output(checks: &[Check]) -> Option<Output> {
    checks.iter().find(|c| c.exit != 0).map(|c| tail(c, OUTPUT_LINES))
}

/// How many lines of a failed command's output the tab shows.
pub(crate) const OUTPUT_LINES: usize = 40;

/// A card's note at Checks: the progress while it works, what failed once it failed.
pub(crate) fn checks_note(t: &Ticket) -> Option<(String, Tone)> {
    match t.status {
        Status::Failed => Some((format!("Failed · {}", what(&t.why)), Tone::Failed)),
        _ if t.why.is_empty() => None,
        _ => Some((t.why.clone(), Tone::Dim)),
    }
}

pub(super) fn list(rows: impl Iterator<Item = Div>) -> Div {
    let rows = rows.enumerate().map(|(i, row)| row.when(i > 0, |r| r.border_t(px(0.5)).border_color(SEPARATOR)));
    div().flex().flex_col().rounded(px(9.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR, 0.5)]).overflow_hidden().children(rows)
}

fn line(mark: AnyElement, name: Div, end: Div) -> Div {
    div().px(px(12.)).py(px(9.)).flex().items_center().gap(px(10.)).text_size(px(13.)).child(div().w(px(14.)).flex_none().flex().justify_center().child(mark)).child(name).child(end)
}

fn quiet(text: &'static str) -> Div {
    div().text_size(px(13.5)).line_height(px(21.)).text_color(TEXT_3).child(text)
}

impl Desktop {
    /// The Checks tab of `t`'s page: each local command, the failure with its attempts, then the PR's checks.
    pub(super) fn factory_checks_tab(&self, t: &Ticket) -> Div {
        let detail = self.agents.detail(&t.id);
        let config = self.agents.factory_config(&t.folder);
        let commands = config.map(|c| c.checks.as_slice()).unwrap_or_default();
        let cap = config.map_or(t.fix, |c| c.fix);
        let checks = detail.map(|d| d.checks.as_slice()).unwrap_or_default();
        let fixing = fixing(t);
        let rows = rows(commands, checks, fixing);
        let running = t.status == Status::Working && t.why.starts_with("Checks · ");
        let mut doc = div().flex().flex_col().child(heading("Checks", Some(format!("{}/{} passed", passed(&rows), rows.len())), None).mt(px(18.)));
        doc = match (rows.is_empty(), detail.is_some_and(|d| d.no_checks)) {
            (true, true) => doc.child(quiet("No checks ran: this project has no local commands and GitHub reported none")),
            (true, false) => doc.child(quiet("This project runs no local commands")),
            _ => {
                let first_waiting = rows.iter().position(|r| r.mark == Mark::Waiting);
                doc.child(list(rows.iter().enumerate().map(|(i, row)| check_row(i, row, running && first_waiting == Some(i)))))
            }
        };
        if t.status == Status::Failed || fixing {
            doc = doc.child(self.checks_callout(t, cap, fixing));
        }
        if let Some(pr) = self.prs.get(&t.worktree).filter(|pr| !pr.runs.is_empty()) {
            let done = pr.runs.iter().filter(|r| matches!(r.outcome, Outcome::Passed | Outcome::Skipped)).count();
            doc = doc.child(heading("GitHub checks", Some(format!("{done}/{} passed", pr.runs.len())), None)).child(list(pr.runs.iter().enumerate().map(|(i, run)| run_row(i, run))));
        }
        doc
    }

    fn checks_callout(&self, t: &Ticket, cap: u8, fixing: bool) -> Div {
        let (title, title_color, line_color, bg) = if fixing { ("Fixing".to_string(), TEXT, TEXT_3, FILL_1) } else { (what(&t.why).to_string(), FAILED_TEXT, FAILED_TEXT, FAILED_BG) };
        let head = div()
            .flex()
            .items_center()
            .gap(px(10.))
            .text_size(px(13.))
            .child(div().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD).text_color(title_color).child(title))
            .child(div().flex_1().flex().justify_end().text_size(px(12.)).text_color(line_color).child(attempt_line(t, cap)));
        let output = self.factory.outputs.get(&t.id).map(|out| {
            div()
                .mt(px(8.))
                .flex()
                .flex_col()
                .gap(px(4.))
                .children(hidden(out).map(|note| div().text_size(px(11.5)).text_color(TEXT_3).child(note)))
                .child(div().font_family(MONO).text_size(px(11.5)).line_height(px(17.)).text_color(TEXT_2).whitespace_nowrap().overflow_hidden().child(out.text.clone()))
        });
        div()
            .mt(px(10.))
            .px(px(14.))
            .py(px(12.))
            .rounded(px(9.))
            .bg(bg)
            .when(!fixing, |d| d.border_1().border_color(Hsla::from(FAILED).opacity(0.2)))
            .flex()
            .flex_col()
            .child(head)
            .children(output)
    }
}

fn check_row(i: usize, row: &Row, running: bool) -> Div {
    let (mark, end): (AnyElement, &str) = match row.mark {
        Mark::Passed => (icon("check", 13., SUCCESS).into_any_element(), ""),
        Mark::Failed => (icon("x", 13., FAILED).into_any_element(), "Failed"),
        Mark::Fixing => (spinner(("check-fix", i), 11., WAITING_DOT).into_any_element(), "Fixing"),
        Mark::Waiting if running => (spinner(("check-run", i), 11., TEXT_3).into_any_element(), "In progress"),
        Mark::Waiting => (ui::dot(6., TEXT_4).into_any_element(), ""),
    };
    let exit = (row.mark == Mark::Failed && row.exit != 0).then(|| format!("exit {}", row.exit));
    let name = div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(12.)).text_color(TEXT_2).child(row.command.clone());
    let end = div().flex_none().flex().gap(px(8.)).text_size(px(12.)).text_color(if row.mark == Mark::Failed { FAILED_TEXT } else { TEXT_3 }).children(exit).child(end);
    line(mark, name, end)
}

fn run_row(i: usize, run: &Run) -> Div {
    let (mark, end): (AnyElement, &str) = match run.outcome {
        Outcome::Passed => (icon("check", 13., SUCCESS).into_any_element(), ""),
        Outcome::Skipped => (ui::dot(6., TEXT_4).into_any_element(), "skipped"),
        Outcome::Pending => (spinner(("gh-run", i), 11., WAITING_DOT).into_any_element(), "in progress"),
        Outcome::Failed => (icon("x", 13., FAILED).into_any_element(), "Failed"),
    };
    let name = div().flex_1().min_w_0().truncate().text_color(TEXT).child(run.name.clone());
    line(mark, name, div().flex_none().text_size(px(12.)).text_color(if run.outcome == Outcome::Failed { FAILED_TEXT } else { TEXT_3 }).child(end))
}

#[cfg(test)]
mod tests {
    // A glob would bring in gpui's `test` attribute, which shadows the std one.
    use super::{Mark, Row, Tone, attempt_line, checks_note, failed_output, fixing, hidden, passed, rows, tail, what};
    use agents::factory::Check;
    use agents::tickets::{State, Station, Status, Ticket};

    fn check(command: &str, exit: i32) -> Check {
        Check { command: command.into(), exit, output: format!("{command} said something\n"), attempt: 1, at: 1 }
    }

    fn at_checks(status: Status, why: &str, fix: u8) -> Ticket {
        Ticket { state: State::Live, station: Station::Checks, status, why: why.into(), fix, ..Ticket::sample() }
    }

    fn commands() -> Vec<String> {
        vec!["cargo fmt --check".into(), "cargo clippy".into(), "cargo test".into()]
    }

    #[test]
    fn each_configured_command_shows_how_it_ran_on_the_newest_commit() {
        let rows = rows(&commands(), &[check("cargo fmt --check", 0), check("cargo clippy", 101)], false);
        assert_eq!(rows.iter().map(|r| r.mark).collect::<Vec<_>>(), [Mark::Passed, Mark::Failed, Mark::Waiting]);
        assert_eq!(rows[1].exit, 101);
        assert_eq!(passed(&rows), 1);
    }

    #[test]
    fn while_the_lead_fixes_the_failed_command_reads_fixing() {
        let rows = rows(&commands(), &[check("cargo fmt --check", 0), check("cargo clippy", 101)], true);
        assert_eq!(rows[1].mark, Mark::Fixing);
    }

    #[test]
    fn a_command_added_since_the_last_run_waits() {
        let rows = rows(&commands(), &[check("make lint", 0)], false);
        assert!(rows.iter().all(|r| r.mark == Mark::Waiting));
    }

    #[test]
    fn without_the_projects_settings_the_rows_are_the_commands_that_ran() {
        let rows = rows(&[], &[check("cargo test", 0)], false);
        assert_eq!(rows, [Row { command: "cargo test".into(), mark: Mark::Passed, exit: 0 }]);
    }

    #[test]
    fn the_ticket_says_whether_the_lead_is_fixing() {
        assert!(fixing(&at_checks(Status::Working, "Fixing · attempt 1", 1)));
        assert!(fixing(&at_checks(Status::Working, "Fixing · your hint", 2)));
        assert!(!fixing(&at_checks(Status::Working, "Checks · 1/3", 0)));
        assert!(!fixing(&at_checks(Status::Failed, "cargo test · Auto-fix 2 of 2 used", 2)));
    }

    #[test]
    fn the_attempt_line_counts_the_attempts_used_of_the_cap() {
        assert_eq!(attempt_line(&at_checks(Status::Working, "Fixing · attempt 1", 1), 2), "Auto-fix 1 of 2 used");
        assert_eq!(attempt_line(&at_checks(Status::Failed, "cargo test · Auto-fix 2 of 2 used", 2), 2), "Auto-fix 2 of 2 used");
        assert_eq!(attempt_line(&at_checks(Status::Failed, "cargo test · Auto-fix is off", 0), 0), "Auto-fix is off");
        assert_eq!(attempt_line(&at_checks(Status::Failed, "x", 2), 1), "Auto-fix 1 of 1 used");
        let hinted = Ticket { hint: true, ..at_checks(Status::Working, "Fixing · your hint", 2) };
        assert_eq!(attempt_line(&hinted, 2), "One attempt granted by you");
        let after = Ticket { hint: true, ..at_checks(Status::Failed, "cargo test · Auto-fix 2 of 2 used", 2) };
        assert_eq!(attempt_line(&after, 2), "Auto-fix 2 of 2 used");
    }

    #[test]
    fn the_callout_is_titled_with_what_failed() {
        assert_eq!(what("cargo test · Auto-fix 2 of 2 used"), "cargo test");
        assert_eq!(what("Conflict in a.rs, b.rs"), "Conflict in a.rs, b.rs");
        assert_eq!(what(""), "Checks failed");
    }

    #[test]
    fn a_short_output_is_shown_whole() {
        let out = tail(&check("cargo test", 101), 40);
        assert_eq!((out.text.as_str(), out.shown, out.total), ("cargo test said something", 1, 1));
        assert_eq!(hidden(&out), None);
    }

    #[test]
    fn a_long_output_shows_its_last_lines_and_says_how_many_are_left_out() {
        let long = Check { command: "cargo test".into(), exit: 101, output: (1..=500).map(|i| format!("line {i}\n")).collect(), ..Check::default() };
        let out = tail(&long, 40);
        assert_eq!((out.shown, out.total), (40, 500));
        assert!(out.text.starts_with("line 461\n") && out.text.ends_with("line 500"));
        assert_eq!(hidden(&out).as_deref(), Some("Showing the last 40 of 500 lines"));
    }

    #[test]
    fn the_output_shown_is_the_failed_commands() {
        assert_eq!(failed_output(&[check("cargo fmt --check", 0), check("cargo test", 101)]).map(|o| o.command), Some("cargo test".to_string()));
        assert_eq!(failed_output(&[check("cargo fmt --check", 0)]), None);
    }

    #[test]
    fn a_ticket_at_checks_notes_its_progress_and_what_failed() {
        assert_eq!(checks_note(&at_checks(Status::Working, "Checks · 2/3", 0)), Some(("Checks · 2/3".into(), Tone::Dim)));
        assert_eq!(checks_note(&at_checks(Status::Working, "Fixing · attempt 1", 1)), Some(("Fixing · attempt 1".into(), Tone::Dim)));
        assert_eq!(checks_note(&at_checks(Status::Failed, "cargo test · Auto-fix 2 of 2 used", 2)), Some(("Failed · cargo test".into(), Tone::Failed)));
        assert_eq!(checks_note(&at_checks(Status::Working, "", 0)), None);
    }
}
