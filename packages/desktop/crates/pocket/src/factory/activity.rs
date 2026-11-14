use super::station::Column;
use crate::board::parts::station_glyph;
use crate::desktop::Desktop;
use crate::util::{ago, now_ms};
use agents::factory::Activity;
use agents::tickets::{Gate, Station, Ticket};
use gpui_kit::*;
use std::ops::Range;
use theme::*;

/// One row of the Activity tab: a station's heading, or the event at this index.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Line {
    Station(Station),
    Event(usize),
}

/// The rows of the Activity tab: each run of events under its station's heading.
pub(crate) fn lines(activity: &[Activity]) -> Vec<Line> {
    let mut out = Vec::with_capacity(activity.len() + 6);
    let mut at = None;
    for (i, a) in activity.iter().enumerate() {
        if at != Some(a.station) {
            at = Some(a.station);
            out.push(Line::Station(a.station));
        }
        out.push(Line::Event(i));
    }
    out
}

pub(crate) fn column_of(station: Station) -> Column {
    match station {
        Station::Spec => Column::Spec,
        Station::Plan => Column::Plan,
        Station::Implement => Column::Implement,
        Station::Checks => Column::Checks,
        Station::Pr => Column::Pr,
        Station::Merged => Column::Merged,
        Station::None => Column::Todo,
    }
}

/// "You", "Lead", "Worker t2".
pub(crate) fn who(actor: &str) -> String {
    match actor.strip_prefix("worker:") {
        Some(task) => format!("Worker {task}"),
        None => match actor {
            "you" => "You".into(),
            "lead" => "Lead".into(),
            "reviewer" => "Reviewer".into(),
            _ => "Anywhere".into(),
        },
    }
}

/// What the actor did. Plans 4 to 6 add the wording of the kinds they write.
pub(crate) fn what(a: &Activity) -> String {
    let with = |text: &str| if a.text.is_empty() { text.to_string() } else { format!("{text} · {}", a.text) };
    match a.kind.as_str() {
        "moved" => format!("moved the ticket to {}", column_of(a.station).name()),
        "approved" => with("approved the plan"),
        "changes" => with("asked the lead to change the plan"),
        "failed" => format!("stopped the ticket: {}", a.text),
        kind => with(kind),
    }
}

const ROW: f32 = 28.;

impl Desktop {
    /// The Activity tab of `t`'s page: one row per event, drawn only where it shows.
    pub(super) fn factory_activity_tab(&self, t: &Ticket, cx: &mut Context<Self>) -> Div {
        let count = self.agents.detail(&t.id).map_or(0, |d| lines(&d.activity).len());
        if count == 0 {
            return div().pt(px(18.)).text_size(px(13.5)).text_color(TEXT_3).child("Nothing has happened yet");
        }
        let ticket = t.id.clone();
        let list = uniform_list("factory-activity", count, cx.processor(move |this, range: Range<usize>, _, _| this.activity_rows(&ticket, range)));
        div().flex_1().min_h_0().pt(px(8.)).flex().flex_col().child(list.flex_1())
    }

    fn activity_rows(&self, ticket: &str, range: Range<usize>) -> Vec<Div> {
        let Some(detail) = self.agents.detail(ticket) else { return Vec::new() };
        let now = now_ms();
        let all = lines(&detail.activity);
        range
            .filter_map(|i| all.get(i))
            .map(|line| match *line {
                Line::Station(station) => {
                    let col = column_of(station);
                    div().w_full().h(px(ROW)).flex().items_end().pb(px(5.)).gap(px(7.)).text_size(px(12.)).text_color(TEXT_3).child(station_glyph(col, Gate::None, false)).child(col.name())
                }
                Line::Event(i) => {
                    let a = &detail.activity[i];
                    div()
                        .w_full()
                        .h(px(ROW))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .text_size(px(13.))
                        .text_color(TEXT_2)
                        .child(div().flex_none().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(who(&a.actor)))
                        .child(div().flex_1().min_w_0().truncate().child(what(a)))
                        .child(div().flex_none().text_size(px(12.)).text_color(TEXT_3).child(ago(a.at, now)))
                }
            })
            .collect()
    }
}

#[cfg(test)]
mod tests {
    // A glob would bring in gpui's `test` attribute, which shadows the std one.
    use super::{Activity, Column, Line, Station, column_of, lines, what, who};

    fn ev(station: Station, actor: &str, kind: &str, text: &str) -> Activity {
        Activity { at: 0, station, actor: actor.into(), kind: kind.into(), text: text.into() }
    }

    #[test]
    fn the_activity_gets_a_heading_each_time_the_station_changes() {
        let acts = [ev(Station::Spec, "anywhere", "moved", ""), ev(Station::Plan, "anywhere", "moved", ""), ev(Station::Plan, "you", "approved", "2 workers"), ev(Station::Implement, "anywhere", "moved", "")];
        assert_eq!(lines(&acts), [Line::Station(Station::Spec), Line::Event(0), Line::Station(Station::Plan), Line::Event(1), Line::Event(2), Line::Station(Station::Implement), Line::Event(3)]);
        assert!(lines(&[]).is_empty());
    }

    #[test]
    fn an_event_reads_as_who_did_what() {
        let said = |a: Activity| (who(&a.actor), what(&a));
        assert_eq!(said(ev(Station::Plan, "anywhere", "moved", "")), ("Anywhere".into(), "moved the ticket to Plan".into()));
        assert_eq!(said(ev(Station::Plan, "you", "approved", "2 workers")), ("You".into(), "approved the plan · 2 workers".into()));
        assert_eq!(said(ev(Station::Plan, "you", "changes", "")), ("You".into(), "asked the lead to change the plan".into()));
        assert_eq!(said(ev(Station::Spec, "anywhere", "failed", "The lead's turn failed")), ("Anywhere".into(), "stopped the ticket: The lead's turn failed".into()));
        assert_eq!(said(ev(Station::Implement, "worker:t2", "handback", "Done")), ("Worker t2".into(), "handback · Done".into()));
        assert_eq!(who("lead"), "Lead");
        assert_eq!(who("reviewer"), "Reviewer");
    }

    #[test]
    fn each_station_has_its_column() {
        assert_eq!([Station::Spec, Station::Plan, Station::Implement, Station::Checks, Station::Pr, Station::Merged, Station::None].map(column_of), [Column::Spec, Column::Plan, Column::Implement, Column::Checks, Column::Pr, Column::Merged, Column::Todo]);
    }
}
