//! The Workers tab: one row per task of the Ticket's Team.

use super::spec::heading;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use agents::factory::ProfileInfo;
use agents::team::state::{RUNNING, WAITING};
use agents::tickets::Ticket;
use agents::{Team, TeamTask};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::agent_avatar;

/// One task as its row shows it.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct WorkerRow {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) state: String,
    /// It waits on you, so its state is drawn in the waiting colour.
    pub(crate) you: bool,
    /// Its profile's name, or its agent when it has no profile.
    pub(crate) profile: String,
    pub(crate) provider: String,
    pub(crate) branch: String,
    pub(crate) added: usize,
    pub(crate) removed: usize,
    /// The first line of its worker's last text, while it works.
    pub(crate) note: String,
}

pub(crate) fn rows(team: &Team, profiles: &[ProfileInfo]) -> Vec<WorkerRow> {
    team.tasks
        .iter()
        .map(|x| {
            let known = profiles.iter().find(|p| !x.profile.is_empty() && p.id == x.profile);
            let (added, removed) = diff_stat(&x.stat);
            let note = if x.state == RUNNING { x.summary.lines().next().unwrap_or_default().to_string() } else { String::new() };
            WorkerRow {
                id: x.id.clone(),
                title: x.title.clone(),
                state: state_label(x),
                you: x.state == "needsYou",
                profile: known.map_or_else(|| if x.profile.is_empty() { x.agent.clone() } else { x.profile.clone() }, |p| p.name.clone()),
                provider: known.map_or_else(|| x.agent.clone(), |p| p.provider.clone()),
                branch: x.branch.clone(),
                added,
                removed,
                note,
            }
        })
        .collect()
}

pub(crate) fn state_label(x: &TeamTask) -> String {
    match x.state.as_str() {
        "planned" => "Planned".into(),
        WAITING if !x.after.is_empty() => format!("After {}", x.after.join(", ")),
        WAITING => "Queued".into(),
        RUNNING => "Working".into(),
        "needsYou" => "Needs you".into(),
        "done" => "Done".into(),
        "failed" => "Failed".into(),
        "stopped" => "Stopped".into(),
        "merged" => "Merged".into(),
        other => other.into(),
    }
}

/// The glyph and colour of a task's mark.
pub(crate) fn mark(state: &str) -> (&'static str, Token) {
    match state {
        "done" | "merged" => ("status-three-quarters", SUCCESS),
        "needsYou" => ("status-needs", WAITING_DOT),
        "failed" => ("status-closed", FAILED),
        "stopped" => ("status-stopped", TEXT_4),
        "planned" => ("status-todo", TEXT_4),
        _ => ("status-half", WORKING),
    }
}

/// The insertions and deletions of `git diff --stat`'s last line.
pub(crate) fn diff_stat(stat: &str) -> (usize, usize) {
    let last = stat.lines().last().unwrap_or_default();
    let count = |word: &str| last.split(',').find(|part| part.contains(word)).and_then(|part| part.split_whitespace().next()?.parse().ok()).unwrap_or(0);
    (count("insertion"), count("deletion"))
}

/// "done/total" for the heading. A merged task is done too.
pub(crate) fn done_of(team: &Team) -> String {
    let done = team.tasks.iter().filter(|x| x.state == "done" || x.state == "merged").count();
    format!("{done}/{}", team.tasks.len())
}

impl Desktop {
    pub(super) fn factory_workers_tab(&self, t: &Ticket) -> Div {
        let Some(team) = self.agents.team(&t.agent_id).filter(|tm| !tm.tasks.is_empty() && tm.state != "proposed" && tm.state != "planning") else {
            return div().pt(px(18.)).text_size(px(13.5)).line_height(px(21.)).text_color(TEXT_3).child("Workers start after the plan is approved");
        };
        let states: Vec<&str> = team.tasks.iter().map(|x| x.state.as_str()).collect();
        let list = rows(team, self.agents.profiles()).into_iter().zip(states).enumerate().map(|(i, (r, state))| {
            let (glyph, color) = mark(state);
            let head = div().flex().gap(px(8.)).min_w_0().child(div().flex_none().font_family(MONO).text_size(px(12.)).text_color(TEXT_3).child(r.id.clone())).child(div().min_w_0().truncate().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(r.title.clone()));
            let meta = div()
                .flex()
                .flex_wrap()
                .items_center()
                .gap_x(px(10.))
                .gap_y(px(4.))
                .text_color(TEXT_3)
                .child(div().flex().items_center().gap(px(5.)).child(agent_avatar(id(format!("worker-agent:{}", r.id)), &r.provider, r.profile.clone())).child(r.profile.clone()))
                .child(div().font_family(MONO).text_size(px(11.5)).text_color(TEXT_2).child(r.branch.clone()))
                .when(r.added + r.removed > 0, |d| d.child(ui::meta_diff(r.added, r.removed, 11.5)));
            let note = (!r.note.is_empty()).then(|| div().child(r.note.clone()));
            let more = div().flex().flex_col().text_color(TEXT_2).child(meta).children(note);
            div()
                .px(px(12.))
                .py(px(10.))
                .flex()
                .flex_col()
                .gap(px(2.))
                .text_size(px(12.))
                .line_height(px(17.))
                .when(i > 0, |d| d.border_t(px(0.5)).border_color(SEPARATOR))
                .child(div().flex().items_start().gap(px(9.)).child(div().w(px(14.)).flex_none().mt(px(2.)).child(icon(glyph, 14., color))).child(div().flex_1().min_w_0().child(head)).child(div().flex_none().text_color(if r.you { WAITING_TEXT } else { TEXT_3 }).whitespace_nowrap().child(r.state.clone())))
                .child(div().pl(px(23.)).child(more))
        });
        div().flex().flex_col().child(heading("Sub-issues", Some(done_of(team)), None).mt(px(18.))).child(div().flex().flex_col().rounded(px(10.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR, 0.5)]).overflow_hidden().children(list))
    }
}

#[cfg(test)]
mod tests {
    // A glob would bring in gpui's `test` attribute, which shadows the std one.
    use super::{WorkerRow, diff_stat, done_of, mark, rows, state_label};
    use agents::factory::ProfileInfo;
    use agents::{Team, TeamTask};

    fn profiles() -> Vec<ProfileInfo> {
        vec![ProfileInfo { id: "go-backend".into(), name: "Go backend".into(), provider: "claude".into(), ..Default::default() }]
    }

    fn task(id: &str, state: &str) -> TeamTask {
        TeamTask { id: id.into(), title: format!("Task {id}"), agent: "codex".into(), state: state.into(), branch: format!("team-abcdef-{id}"), ..Default::default() }
    }

    #[test]
    fn a_row_shows_the_tasks_state_profile_branch_diff_and_last_word() {
        let mut t1 = task("t1", "running");
        t1.profile = "go-backend".into();
        t1.summary = "Writing the parser\nthen its tests".into();
        t1.stat = " a.go | 5 ++++-\n 1 file changed, 4 insertions(+), 1 deletion(-)".into();
        let mut t2 = task("t2", "waiting");
        t2.after = vec!["t1".into()];
        let team = Team { lead: "a1".into(), state: "running".into(), tasks: vec![t1, t2], ..Default::default() };
        let got = rows(&team, &profiles());
        assert_eq!(got[0], WorkerRow { id: "t1".into(), title: "Task t1".into(), state: "Working".into(), you: false, profile: "Go backend".into(), provider: "claude".into(), branch: "team-abcdef-t1".into(), added: 4, removed: 1, note: "Writing the parser".into() });
        assert_eq!((got[1].state.as_str(), got[1].profile.as_str(), got[1].provider.as_str(), got[1].note.as_str()), ("After t1", "codex", "codex", ""));
    }

    #[test]
    fn each_task_state_reads_as_the_prototype_writes_it() {
        let labels = ["planned", "running", "needsYou", "done", "failed", "stopped", "merged"].map(|s| state_label(&task("t1", s)));
        assert_eq!(labels, ["Planned", "Working", "Needs you", "Done", "Failed", "Stopped", "Merged"]);
        assert!(rows(&Team { tasks: vec![task("t1", "needsYou")], ..Default::default() }, &[])[0].you);
    }

    #[test]
    fn a_diff_stat_reads_its_insertions_and_deletions() {
        assert_eq!(diff_stat(" 2 files changed, 5 insertions(+), 1 deletion(-)"), (5, 1));
        assert_eq!(diff_stat(" 1 file changed, 1 insertion(+)"), (1, 0));
        assert_eq!(diff_stat(" 1 file changed, 3 deletions(-)"), (0, 3));
        assert_eq!(diff_stat(""), (0, 0));
    }

    #[test]
    fn a_task_s_mark_follows_its_state() {
        let glyphs = ["planned", "running", "needsYou", "done", "merged", "failed"].map(|s| mark(s).0);
        assert_eq!(glyphs, ["status-todo", "status-half", "status-needs", "status-three-quarters", "status-three-quarters", "status-closed"]);
    }

    #[test]
    fn the_heading_counts_the_tasks_that_are_done() {
        let team = Team { tasks: vec![task("t1", "done"), task("t2", "merged"), task("t3", "running")], ..Default::default() };
        assert_eq!(done_of(&team), "2/3");
    }
}
