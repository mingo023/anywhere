use super::delivered;
use crate::chat::shimmer::shimmer;
use crate::chat::sub_agent::{self, Mark};
use crate::desktop::Desktop;
use agents::delegations::{Delegation, Phase};
use chat::{Child, Delivered, Outcome, Span};
use gpui_kit::*;
use std::time::{Duration, Instant};
use theme::*;

/// How far the opened reports drop into place.
const DROP: f32 = 6.;
/// How long a shut group's reports stay drawn while they leave.
const LEAVE: Duration = Duration::from_millis(300);

/// A session a group of reports speaks of, and where it stands.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Standing {
    pub child: Child,
    pub phase: Phase,
    pub question: String,
}

/// When each of `list` began and settled, 0 while it runs, as the chat's layout takes them.
pub(crate) fn spans(list: &[Delegation]) -> Vec<Span> {
    list.iter().map(|d| Span { began: d.created_at, settled: d.phase().settled().then_some(d.settled_at) }).collect()
}

/// The sessions `deliveries` speak of, as pocketd lists them now, else as their last delivery left them. The newest
/// group also counts the sessions still at work, which have yet to report.
pub(crate) fn standing<'a>(deliveries: impl Iterator<Item = &'a Delivered>, list: &[Delegation], newest: bool) -> Vec<Standing> {
    let mut out: Vec<Standing> = Vec::new();
    for d in deliveries {
        for child in &d.children {
            let listed = list.iter().find(|l| l.child_id == child.id);
            let phase = listed.map_or(
                match d.outcome {
                    Outcome::Done => Phase::Done,
                    Outcome::Failed => Phase::Failed,
                    Outcome::Asks => Phase::Asking,
                },
                Delegation::phase,
            );
            let question = listed.map_or(d.question.clone(), |l| l.question.clone());
            let s = Standing { child: child.clone(), phase, question };
            match out.iter_mut().find(|o| o.child.id == child.id) {
                Some(o) => *o = s,
                None => out.push(s),
            }
        }
    }
    if newest {
        let yet: Vec<Standing> = list
            .iter()
            .filter(|l| !l.phase().settled() && !out.iter().any(|o| o.child.id == l.child_id))
            .map(|l| Standing { child: Child { id: l.child_id.clone(), title: l.title.clone(), agent: l.agent.clone() }, phase: l.phase(), question: l.question.clone() })
            .collect();
        out.extend(yet);
    }
    out
}

fn settled(sessions: &[Standing]) -> bool {
    sessions.iter().all(|s| s.phase.settled())
}

/// "3 delegated sessions" while any is at work or asks, "3 sessions reported" once all have ended.
pub(crate) fn title(sessions: &[Standing]) -> String {
    let n = sessions.len();
    let s = if n == 1 { "" } else { "s" };
    if settled(sessions) { format!("{n} session{s} reported") } else { format!("{n} delegated session{s}") }
}

/// "2 done · 1 needs you · 1 working", what's at zero left out, each part in its colour.
pub(crate) fn tally(sessions: &[Standing]) -> Vec<(String, Token)> {
    let count = |f: fn(Phase) -> bool| sessions.iter().filter(|s| f(s.phase)).count();
    let parts = [
        (count(|p| matches!(p, Phase::Done | Phase::Stopped)), "done", TEXT_3),
        (count(|p| p == Phase::Failed), "failed", FAILED_TEXT),
        (count(|p| p == Phase::Asking), "needs you", WAITING_TEXT),
        (count(|p| matches!(p, Phase::Starting | Phase::Working)), "working", TEXT_3),
    ];
    parts.into_iter().filter(|(n, ..)| *n > 0).map(|(n, what, colour)| (format!("{n} {what}"), colour)).collect()
}

/// What the header says while the agent takes in the report `d` brought.
pub(crate) fn reading(d: Option<&Delivered>) -> String {
    match d.map(|d| &d.children[..]) {
        Some([one]) if !one.title.is_empty() => format!("Reading {}\u{2019}s report\u{2026}", one.title),
        Some(many) if many.len() > 1 => format!("Reading {} reports\u{2026}", many.len()),
        _ => "Reading the report\u{2026}".into(),
    }
}

/// One report in the group: the turn's own key, its deliveries, and what the agent made of them.
pub(crate) struct Report<'a> {
    pub key: String,
    pub deliveries: Vec<&'a Delivered>,
    pub answer: Option<AnyElement>,
}

/// The row heading a group of reports, and what it opens.
pub(crate) struct Group<'a> {
    pub id: &'a str,
    pub sessions: Vec<Standing>,
    /// The report the agent is taking in now, if it is.
    pub live: Option<String>,
    pub reports: Vec<Report<'a>>,
    pub open: bool,
    /// When the user last opened or shut it.
    pub toggled: Option<Instant>,
    /// When a delivery came, by its id, while it counts as having just arrived.
    pub age: &'a dyn Fn(&str) -> Option<Duration>,
}

/// One line for the group, the questions sessions in it wait on under it, and, opened, each report and what the agent
/// made of it, hung from a hairline. They come and go on a spring, or a fade when motion is reduced.
pub(crate) fn group(g: Group, on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static, window: &mut Window, cx: &mut Context<Desktop>) -> Div {
    let (id, updates) = (g.id, g.reports.iter().map(|r| r.deliveries.len()).sum());
    let pins: Vec<Div> = g.sessions.iter().filter(|s| s.phase == Phase::Asking).map(|s| pin(id, s, window, cx)).collect();
    let leaving = !g.open && g.toggled.is_some_and(|at| at.elapsed() < LEAVE);
    if leaving {
        window.request_animation_frame();
    }
    let body = (g.open || leaving).then(|| {
        let reports = g.reports.into_iter().map(|r| {
            let lines = r.deliveries.into_iter().map(|d| delivered(&format!("{}-{}", r.key, d.id), d, (g.age)(&d.id), window, cx));
            div().flex().flex_col().children(lines).children(r.answer.map(|a| div().pl(px(24.)).pr(px(8.)).pb(px(6.)).child(a)))
        });
        let column = div().ml(px(15.)).pl(px(1.)).border_l_1().border_color(SEPARATOR).flex().flex_col().gap(px(4.)).py(px(4.)).children(reports);
        let open = g.open;
        if cx.reduce_motion() && g.toggled.is_none() {
            div().child(column)
        } else if cx.reduce_motion() {
            let fade = Animation::new(Duration::from_millis(200));
            div().child(column.with_animation(SharedString::from(format!("{id}-fade-{:?}", g.toggled)), fade, move |d, t| d.opacity(if open { t } else { 1. - t })))
        } else {
            // Opened before this chat showed, it is just there.
            let drop = SpringAnimation::new(SPRING).to(open).from(!open || g.toggled.is_none());
            div().child(column.with_spring(SharedString::from(format!("{id}-drop-{:?}", g.toggled)), drop, |d, t| d.relative().top(px(-DROP * (1. - t.0))).opacity(t.0.clamp(0., 1.))))
        }
    });
    div().flex().flex_col().gap(px(4.)).ml(px(-8.)).child(head(id, &g.sessions, g.live, updates, g.open, on_toggle)).children(pins).children(body)
}

fn head(id: &str, sessions: &[Standing], live: Option<String>, updates: usize, open: bool, on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Stateful<Div> {
    let mut agents: Vec<&str> = Vec::new();
    for s in sessions {
        if !agents.contains(&s.child.agent.as_str()) && agents.len() < 3 {
            agents.push(&s.child.agent);
        }
    }
    let logos = div().flex().flex_none().items_center().gap(px(2.)).children(agents.into_iter().map(|a| provider_icon(a, 13., TEXT_3)));
    let mark = sub_agent::glyph(&format!("{id}-mark"), if settled(sessions) { Mark::Done } else { Mark::Running });
    let summary = match live {
        Some(label) => div().min_w_0().child(shimmer(SharedString::from(format!("{id}-live")), label, TEXT_3)),
        None => {
            let mut parts = div().min_w_0().flex().items_center().gap(px(6.)).whitespace_nowrap();
            for (i, (text, colour)) in tally(sessions).into_iter().enumerate() {
                if i > 0 {
                    parts = parts.child(div().size(px(3.)).rounded_full().bg(TEXT_5));
                }
                parts = parts.child(div().text_color(colour).child(text));
            }
            parts
        }
    };
    let chevron = icon("chevron-right", 12., TEXT_4).with_spring(SharedString::from(format!("{id}-chevron")), SpringAnimation::new(SPRING).to(open), |s, t| {
        s.with_transformation(Transformation::rotate(radians(std::f32::consts::FRAC_PI_2 * t.0)))
    });
    let updates = format!("{updates} update{}", if updates == 1 { "" } else { "s" });
    div()
        .id(SharedString::from(format!("{id}-head")))
        .h(px(32.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(10.))
        .rounded(px(9.))
        .text_size(px(14.))
        .text_color(TEXT_3)
        .cursor_pointer()
        .hover(|s| s.bg(FILL_2))
        .active(|s| s.bg(FILL_3))
        .on_click(on_toggle)
        .child(logos)
        .child(div().flex_none().font_weight(FontWeight::MEDIUM).text_color(TEXT_BODY).child(title(sessions)))
        .child(div().flex_none().child(mark))
        .child(summary)
        .child(div().flex_1())
        .child(div().flex_none().text_size(px(12.5)).text_color(TEXT_4).child(updates))
        .child(div().flex_none().child(chevron))
}

/// A question a session waits on, held outside the group until it is answered; a click opens the session to answer it.
fn pin(id: &str, s: &Standing, window: &mut Window, cx: &mut Context<Desktop>) -> Div {
    let asks = Delivered { id: format!("{id}-asks-{}", s.child.id), outcome: Outcome::Asks, question: s.question.clone(), children: vec![s.child.clone()] };
    div().ml(px(8.)).pl(px(8.)).rounded(px(10.)).bg(WAITING_BG).child(delivered(&asks.id, &asks, None, window, cx))
}

#[cfg(test)]
mod tests {
    use super::{Standing, reading, spans, standing, tally, title};
    use agents::delegations::{Delegation, Phase};
    use chat::{Child, Delivered, Outcome, Span};

    fn child(id: &str) -> Child {
        Child { id: id.into(), title: format!("Task {id}"), agent: "codex".into() }
    }

    fn news(outcome: Outcome, ids: &[&str]) -> Delivered {
        Delivered { id: ids.join("+"), outcome, question: if outcome == Outcome::Asks { "Which runner?".into() } else { String::new() }, children: ids.iter().map(|i| child(i)).collect() }
    }

    fn listed(id: &str, status: &str, question: &str, settled_at: i64) -> Delegation {
        Delegation { child_id: id.into(), title: format!("Task {id}"), agent: "claude".into(), status: status.into(), question: question.into(), created_at: 10, settled_at, ..Default::default() }
    }

    #[test]
    fn a_session_stands_as_pocketd_lists_it_else_as_its_last_delivery_left_it() {
        let deliveries = [news(Outcome::Asks, &["a"]), news(Outcome::Done, &["b"]), news(Outcome::Done, &["a"])];
        let list = [listed("b", "failed", "", 300)];
        let phases: Vec<(String, Phase)> = standing(deliveries.iter(), &list, false).into_iter().map(|s| (s.child.id, s.phase)).collect();
        assert_eq!(phases, [("a".into(), Phase::Done), ("b".into(), Phase::Failed)]);
    }

    #[test]
    fn only_the_newest_group_counts_the_sessions_yet_to_report() {
        let deliveries = [news(Outcome::Done, &["a"])];
        let list = [listed("a", "completed", "", 100), listed("b", "working", "", 0), listed("c", "completed", "", 50)];
        assert_eq!(standing(deliveries.iter(), &list, true).len(), 2, "b runs on; c settled and reported elsewhere");
        assert_eq!(standing(deliveries.iter(), &list, false).len(), 1);
    }

    #[test]
    fn the_header_names_the_sessions_by_whether_all_ended_and_counts_each_phase_it_has() {
        let s = |id: &str, phase| Standing { child: child(id), phase, question: String::new() };
        let mixed = [s("a", Phase::Done), s("b", Phase::Asking), s("c", Phase::Working), s("d", Phase::Failed)];
        assert_eq!(title(&mixed), "4 delegated sessions");
        assert_eq!(tally(&mixed).into_iter().map(|(t, _)| t).collect::<Vec<_>>(), ["1 done", "1 failed", "1 needs you", "1 working"]);
        let ended = [s("a", Phase::Done), s("b", Phase::Stopped)];
        assert_eq!((title(&ended), tally(&ended).into_iter().map(|(t, _)| t).collect::<Vec<_>>()), ("2 sessions reported".into(), vec!["2 done".to_string()]));
        assert_eq!(title(&ended[..1]), "1 session reported");
    }

    #[test]
    fn the_header_reads_out_the_report_the_agent_takes_in() {
        assert_eq!(reading(Some(&news(Outcome::Done, &["a"]))), "Reading Task a\u{2019}s report\u{2026}");
        assert_eq!(reading(Some(&news(Outcome::Done, &["a", "b"]))), "Reading 2 reports\u{2026}");
        assert_eq!(reading(None), "Reading the report\u{2026}");
    }

    #[test]
    fn a_session_settled_without_a_time_still_counts_as_settled() {
        let settled = |at| Span { began: 10, settled: at };
        assert_eq!(spans(&[listed("a", "working", "", 0), listed("b", "completed", "", 0), listed("c", "completed", "", 300)]), [settled(None), settled(Some(0)), settled(Some(300))]);
    }
}
