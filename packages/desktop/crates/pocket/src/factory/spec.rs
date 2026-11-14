use super::station::Column;
use crate::board::parts::station_glyph;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use crate::team::plan::Edits;
use crate::team::plural;
use agents::Team;
use agents::factory::Activity;
use agents::tickets::{Gate, Station, Ticket};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::agent_avatar;

/// A spec block's markdown, split for the Spec & plan tab.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct Spec {
    pub(crate) brief: String,
    pub(crate) criteria: Vec<String>,
}

/// One task of the plan, as a sub-issue row.
#[derive(Debug, Default, PartialEq)]
pub(crate) struct TaskRow {
    pub(crate) id: String,
    pub(crate) title: String,
    pub(crate) body: String,
    pub(crate) files: String,
    pub(crate) agent: String,
    /// At the gate: "starts first" or "after 1". Empty once the plan started.
    pub(crate) order: String,
    /// At the gate: whether the task can be removed. `None` once the plan started.
    pub(crate) removable: Option<bool>,
    pub(crate) state: String,
}

#[derive(Clone, Copy, PartialEq)]
enum Part {
    Other,
    Brief,
    Criteria,
}

/// The brief and the criteria of a spec block. Lines under any other heading are left out.
pub(crate) fn parse(markdown: &str) -> Spec {
    let mut spec = Spec::default();
    let mut part = Part::Other;
    for line in markdown.lines().map(str::trim) {
        if let Some(heading) = line.strip_prefix("## ") {
            part = match heading.trim() {
                h if h.eq_ignore_ascii_case("Brief") => Part::Brief,
                h if h.eq_ignore_ascii_case("Acceptance criteria") => Part::Criteria,
                _ => Part::Other,
            };
            continue;
        }
        match part {
            Part::Brief if !line.is_empty() => {
                if !spec.brief.is_empty() {
                    spec.brief.push(' ');
                }
                spec.brief.push_str(line);
            }
            Part::Criteria => spec.criteria.extend(item(line).map(str::to_string)),
            _ => {}
        }
    }
    spec
}

/// A list item's text, without its bullet and its checkbox.
fn item(line: &str) -> Option<&str> {
    let rest = line.strip_prefix("- ").or_else(|| line.strip_prefix("* "))?;
    let rest = rest.strip_prefix("[ ] ").or_else(|| rest.strip_prefix("[x] ")).unwrap_or(rest).trim();
    (!rest.is_empty()).then_some(rest)
}

/// What the tab says in place of a spec that isn't there. `planned` is whether the lead has a plan.
pub(crate) fn quiet(station: Station, planned: bool) -> Option<&'static str> {
    match station {
        Station::None => Some("Written when the ticket starts"),
        Station::Spec => Some("The lead is drafting the spec"),
        _ if !planned => Some("The lead is still planning"),
        _ => None,
    }
}

pub(crate) fn byline(agent: &str, clock: &str) -> String {
    if clock.is_empty() { format!("Spec · {agent}") } else { format!("Spec · {agent} · {clock}") }
}

/// When the spec was saved: the moment the ticket moved to Plan.
pub(crate) fn written(activity: &[Activity]) -> Option<i64> {
    activity.iter().find(|a| a.kind == "moved" && a.station == Station::Plan).map(|a| a.at)
}

/// The plan's rows: the tasks you kept while `edits` is given (the Plan gate), else every task with its state.
pub(crate) fn task_rows(team: &Team, edits: Option<&Edits>) -> Vec<TaskRow> {
    let body = |id: &str| team.tasks.iter().find(|t| t.id == id).map(|t| t.instructions.clone()).unwrap_or_default();
    match edits {
        Some(edits) => edits
            .tasks()
            .iter()
            .map(|k| TaskRow {
                id: k.id.clone(),
                title: k.title.clone(),
                body: body(&k.id),
                files: k.files.join("  "),
                agent: k.agent.clone(),
                order: edits.after_label(k).unwrap_or_else(|| "starts first".into()),
                removable: Some(edits.removable(&k.id)),
                state: String::new(),
            })
            .collect(),
        None => team
            .tasks
            .iter()
            .map(|t| TaskRow { id: t.id.clone(), title: t.title.clone(), body: t.instructions.clone(), files: t.files.join("  "), agent: t.agent.clone(), order: String::new(), removable: None, state: t.state.clone() })
            .collect(),
    }
}

/// "14:13" in this Mac's time zone.
fn clock(ms: i64) -> String {
    chrono::DateTime::from_timestamp_millis(ms).map(|t| t.with_timezone(&chrono::Local).format("%H:%M").to_string()).unwrap_or_default()
}

fn heading(title: &'static str, note: Option<String>, right: Option<&'static str>) -> Div {
    let small = |text: String| div().text_size(px(12.)).line_height(px(16.)).font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(text);
    div()
        .mt(px(26.))
        .mb(px(10.))
        .flex()
        .items_end()
        .gap(px(8.))
        .text_size(px(14.))
        .line_height(px(20.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT)
        .child(title)
        .children(note.map(small))
        .children(right.map(|r| div().flex_1().flex().justify_end().child(small(r.to_string()))))
}

impl Desktop {
    /// The Spec & plan tab of `t`'s page.
    pub(super) fn factory_spec_tab(&self, t: &Ticket, cx: &mut Context<Self>) -> Div {
        let detail = self.agents.detail(&t.id);
        let team = self.agents.team(&t.agent_id).filter(|team| !team.tasks.is_empty());
        let spec = detail.map(|d| parse(&d.spec)).filter(|s| !s.brief.is_empty() || !s.criteria.is_empty());
        let mut doc = div().flex().flex_col().child(heading("Spec", None, None).mt(px(18.)));
        match spec {
            Some(spec) => {
                let at = detail.and_then(|d| written(&d.activity)).map(clock).unwrap_or_default();
                doc = doc.child(div().mt(px(-4.)).mb(px(10.)).text_size(px(12.)).text_color(TEXT_3).child(byline(&self.agent_name(&t.provider), &at))).child(div().text_size(px(13.5)).line_height(px(21.)).text_color(TEXT_BODY).child(spec.brief));
                if !spec.criteria.is_empty() {
                    let boxed = |text: String| {
                        div()
                            .py(px(4.))
                            .flex()
                            .gap(px(9.))
                            .text_size(px(13.5))
                            .line_height(px(20.))
                            .text_color(TEXT_BODY)
                            .child(div().mt(px(3.)).size(px(14.)).flex_none().rounded(px(4.)).border_1().border_color(SEPARATOR_STRONG))
                            .child(div().min_w_0().child(text))
                    };
                    doc = doc.child(div().mt(px(18.)).mb(px(8.)).text_size(px(12.)).line_height(px(16.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child("Acceptance criteria")).children(spec.criteria.into_iter().map(boxed));
                }
            }
            None => doc = doc.children(quiet(t.station, team.is_some()).map(|text| div().text_size(px(13.5)).line_height(px(21.)).text_color(TEXT_3).child(text))),
        }
        let Some(team) = team else { return doc };
        let edits = (t.gate == Gate::Plan).then(|| self.team.island.edits.get(&t.agent_id)).flatten();
        let rows = task_rows(team, edits);
        let cap = edits.is_some().then_some("Up to 4 workers at once");
        let list = rows.iter().enumerate().map(|(i, row)| self.plan_row(row, edits, cx).when(i > 0, |r| r.border_t(px(0.5)).border_color(SEPARATOR)));
        doc.child(heading("Plan", Some(plural(rows.len(), "task", "tasks")), cap)).child(div().flex().flex_col().rounded(px(10.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR, 0.5)]).overflow_hidden().children(list))
    }

    fn plan_row(&self, row: &TaskRow, edits: Option<&Edits>, cx: &mut Context<Self>) -> Div {
        let task = row.id.clone();
        let end: AnyElement = match row.removable {
            Some(can) => {
                let tip = if can { "Remove from the plan" } else { "Another task starts after this one" };
                ui::icon_button_sized(id(format!("plan-remove:{task}")), "x", 20., if can { TEXT_3 } else { TEXT_4 })
                    .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
                    .when(can, |b| b.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.team_remove(&task, cx))))
                    .into_any_element()
            }
            None => div().text_size(px(12.)).text_color(TEXT_3).whitespace_nowrap().child(row.state.clone()).into_any_element(),
        };
        let head = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .children(row.removable.map(|_| station_glyph(Column::Todo, Gate::None, false)))
            .child(div().font_family(MONO).text_size(px(12.)).text_color(TEXT_3).child(row.id.clone()))
            .child(div().flex_1().min_w_0().truncate().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(row.title.clone()))
            .child(end);
        let meta = div()
            .flex()
            .flex_wrap()
            .items_center()
            .gap_x(px(10.))
            .gap_y(px(4.))
            .text_color(TEXT_3)
            .children((!row.files.is_empty()).then(|| div().font_family(MONO).text_size(px(11.5)).text_color(TEXT_2).child(row.files.clone())))
            .child(div().flex().items_center().gap(px(5.)).child(agent_avatar(id(format!("plan-agent:{}", row.id)), &row.agent, self.agent_name(&row.agent))).child(self.agent_name(&row.agent)))
            .children((!row.order.is_empty()).then(|| div().child(row.order.clone())));
        let allow = edits.and_then(|e| e.tasks().iter().find(|k| k.id == row.id)).map(|k| self.allow_row(k, cx));
        div()
            .px(px(12.))
            .py(px(10.))
            .flex()
            .flex_col()
            .gap(px(4.))
            .text_size(px(12.))
            .line_height(px(17.))
            .text_color(TEXT_2)
            .child(head)
            .children((!row.body.is_empty()).then(|| div().child(row.body.clone())))
            .child(meta)
            .children(allow)
    }
}

#[cfg(test)]
mod tests {
    // A glob would bring in gpui's `test` attribute, which shadows the std one.
    use super::{Activity, Edits, Spec, Station, Team, byline, parse, quiet, task_rows, written};
    use agents::TeamTask;

    const SPEC: &str = "## Brief\nSends wait in a queue.\nThey survive a restart.\n\n## Acceptance criteria\n- [ ] A send survives a restart\n- [x] The queue shows in the composer\n* Plain item\n- \n## Risks\n- None";

    fn team() -> Team {
        let task = |id: &str, after: &[&str]| TeamTask { id: id.into(), title: format!("Do {id}"), agent: "claude".into(), files: vec![format!("{id}.rs"), "lib.rs".into()], after: after.iter().map(|a| a.to_string()).collect(), instructions: format!("Write {id}"), state: "planned".into(), ..TeamTask::default() };
        Team { lead: "a1".into(), state: "proposed".into(), tasks: vec![task("t1", &[]), task("t2", &["t1"])], ..Team::default() }
    }

    #[test]
    fn a_spec_splits_into_its_brief_and_its_criteria() {
        let spec = parse(SPEC);
        assert_eq!(spec.brief, "Sends wait in a queue. They survive a restart.");
        assert_eq!(spec.criteria, ["A send survives a restart", "The queue shows in the composer", "Plain item"]);
    }

    #[test]
    fn a_spec_with_other_headings_keeps_what_it_knows() {
        assert_eq!(parse("## brief\nShort.\n## Notes\n- Not a criterion"), Spec { brief: "Short.".into(), criteria: vec![] });
        assert_eq!(parse(""), Spec::default());
    }

    #[test]
    fn the_tab_says_why_there_is_no_spec_yet() {
        assert_eq!(quiet(Station::None, false), Some("Written when the ticket starts"));
        assert_eq!(quiet(Station::Spec, false), Some("The lead is drafting the spec"));
        assert_eq!(quiet(Station::Plan, false), Some("The lead is still planning"));
        assert_eq!(quiet(Station::Plan, true), None);
        assert_eq!(quiet(Station::Implement, true), None);
    }

    #[test]
    fn the_byline_names_who_wrote_the_spec_and_when() {
        assert_eq!(byline("Claude", "14:13"), "Spec · Claude · 14:13");
        assert_eq!(byline("Claude", ""), "Spec · Claude");
    }

    #[test]
    fn the_spec_was_written_when_the_ticket_moved_to_plan() {
        let ev = |at: i64, station: Station, kind: &str| Activity { at, station, actor: "anywhere".into(), kind: kind.into(), text: String::new() };
        let acts = [ev(1, Station::Spec, "moved"), ev(2, Station::Plan, "moved"), ev(3, Station::Plan, "failed"), ev(4, Station::Implement, "moved")];
        assert_eq!(written(&acts), Some(2));
        assert_eq!(written(&acts[..1]), None);
    }

    #[test]
    fn at_the_plan_gate_the_rows_are_the_tasks_you_kept() {
        let team = team();
        let mut edits = Edits::new(&team, |_| String::new());
        let rows = task_rows(&team, Some(&edits));
        assert_eq!(rows.len(), 2);
        assert_eq!((rows[0].order.as_str(), rows[0].removable), ("starts first", Some(false)));
        assert_eq!((rows[1].order.as_str(), rows[1].removable), ("after 1", Some(true)));
        assert_eq!((rows[1].body.as_str(), rows[1].files.as_str()), ("Write t2", "t2.rs  lib.rs"));
        edits.remove("t2");
        let rows = task_rows(&team, Some(&edits));
        assert_eq!(rows.iter().map(|r| r.id.as_str()).collect::<Vec<_>>(), ["t1"]);
        assert_eq!(rows[0].removable, Some(true));
    }

    #[test]
    fn a_started_plan_lists_every_task_with_its_state() {
        let mut team = team();
        team.state = "running".into();
        team.tasks[0].state = "running".into();
        let rows = task_rows(&team, None);
        assert_eq!(rows.iter().map(|r| (r.id.as_str(), r.state.as_str(), r.removable)).collect::<Vec<_>>(), [("t1", "running", None), ("t2", "planned", None)]);
        assert!(rows.iter().all(|r| r.order.is_empty()));
    }
}
