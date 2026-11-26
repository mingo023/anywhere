use super::agent_detail::{self, ran_ms, thousands};
use super::shimmer::shimmer;
use super::sub_agent::{self, Mark, Picks};
use super::working::format_elapsed;
use crate::util::now_ms;
use chat::workflow::{Group, Stage, Workflow, current, dense, folded, phase_line, waits_on};
use chat::{ToolCall, ToolStatus};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashSet;
use std::rc::Rc;
use theme::*;

const ROW: &str = "workflow-row";

pub(crate) fn mark(call: &ToolCall, w: &Workflow) -> Mark {
    match call.status {
        ToolStatus::Running if w.agents().next().is_none() => Mark::Starting,
        ToolStatus::Running => Mark::Running,
        ToolStatus::Ok => Mark::Done,
        ToolStatus::Failed => Mark::Failed,
    }
}

/// "Working · 2/5 agents · 41.2k tokens · 1m 14s".
pub(crate) fn summary(call: &ToolCall, w: &Workflow, now: i64) -> String {
    let status = match call.status {
        ToolStatus::Running => "Working",
        ToolStatus::Ok if call.output.as_deref() == Some("Stopped") => "Stopped",
        ToolStatus::Ok => "Done",
        ToolStatus::Failed => "Failed",
    };
    let (done, all) = w.tally();
    let tokens: u64 = w.agents().map(|(_, a)| a.tokens).sum();
    let mut parts = vec![status.to_string(), format!("{done}/{all} agents")];
    if tokens > 0 {
        parts.push(format!("{} tokens", thousands(tokens)));
    }
    parts.push(format_elapsed(ran_ms(call, now) / 1000));
    parts.join(" · ")
}

/// A workflow as one line in its chat: its mark, its name, the phase it's in, how many agents finished and for how long.
/// A click shows it in the dock.
pub(crate) fn row(call: &ToolCall, w: &Workflow, picks: &Picks) -> Div {
    let m = mark(call, w);
    let open = picks.open.as_deref() == Some(call.id.as_str());
    let line = phase_line(&w.groups());
    let name = w.label();
    let label = div().flex_none().max_w(relative(0.5)).min_w_0().overflow_hidden().text_color(if open { TEXT } else { TEXT_BODY });
    let label = if matches!(m, Mark::Running | Mark::Starting) {
        label.child(shimmer(SharedString::from(format!("workflow-name-{}", call.id)), name, TEXT_BODY))
    } else {
        label.truncate().child(name)
    };
    let spacer = line.is_none().then(|| div().flex_1());
    let line = line.map(|l| div().flex_1().min_w_0().truncate().text_size(px(13.)).text_color(TEXT_3).child(l));
    let (done, all) = w.tally();
    let side = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.))
        .text_color(TEXT_4)
        .when(all > 0, |d| d.child(format!("{done}/{all} agents")))
        .children(agent_detail::took(call, now_ms()))
        .child(div().when(!open, |d| d.invisible().group_hover(ROW, |s| s.visible())).child(icon("chevron-right", 14., if open { TEXT_2 } else { TEXT_5 })));
    let on_pick = picks.on_pick.clone();
    let id = call.id.clone();
    let head = div()
        .id(SharedString::from(format!("workflow-{}", call.id)))
        .group(ROW)
        .w(relative(1.))
        .mx(px(-6.))
        .px(px(6.))
        .py(px(4.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(7.))
        .text_size(px(14.))
        .line_height(px(20.))
        .cursor_pointer()
        .map(|d| if open { d.bg(FILL_3) } else { d.hover(|s| s.bg(FILL_2)) })
        .active(|s| s.bg(FILL_3))
        .on_click(move |_, window, cx| on_pick(&id, window, cx))
        .child(sub_agent::glyph(&call.id, m))
        .child(label)
        .children(line)
        .children(spacer)
        .child(side);
    div().w_full().min_w_0().flex().child(head)
}

/// Opens one of a workflow's agents in the dock.
pub(crate) type OnAgent = Rc<dyn Fn(&str, &mut Window, &mut App)>;

fn stage_mark(key: &str, stage: Stage) -> AnyElement {
    match stage {
        Stage::Done => icon("agent-done", 14., TEXT_4).into_any_element(),
        Stage::Active => sub_agent::glyph(key, Mark::Running),
        Stage::Pending => div().size(px(14.)).flex_none().p(px(2.)).child(div().size_full().rounded_full().border_1().border_color(TEXT_5)).into_any_element(),
    }
}

/// Folds or unfolds one of a workflow's phases in the dock.
pub(crate) type OnFlip = Rc<dyn Fn(usize, &mut Window, &mut App)>;

/// The phases as a strip of bars, each lit by how far it got. Too many to label each get one line naming the phase
/// it's in. A click on a bar unfolds its phase and scrolls to it.
fn strip(key: &str, call: &ToolCall, groups: &[Group], seek: &OnFlip) -> Div {
    let dense = dense(groups);
    let bars = div().flex().gap(px(if dense { 3. } else { 6. })).children(groups.iter().enumerate().map(|(at, g)| {
        let (fill, text) = match g.stage() {
            Stage::Done => (TEXT_5, TEXT_3),
            Stage::Active => (TEXT_2, TEXT),
            Stage::Pending => (FILL_4, TEXT_4),
        };
        let done = g.agents.iter().filter(|(c, _)| c.status != ToolStatus::Running).count();
        let lit = if g.agents.is_empty() { 0. } else { (done as f32 / g.agents.len() as f32).max(0.04) };
        let seek = seek.clone();
        div()
            .id(SharedString::from(format!("{key}-bar-{at}")))
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(6.))
            .cursor_pointer()
            .on_click(move |_, window, cx| seek(at, window, cx))
            .child(div().h(px(3.)).rounded_full().overflow_hidden().bg(FILL_4).child(div().h_full().w(relative(lit)).bg(fill)))
            .when(!dense, |d| {
                d.child(
                    div()
                        .truncate()
                        .text_size(px(11.5))
                        .text_color(text)
                        .when(g.stage() == Stage::Active, |d| d.font_weight(FontWeight::MEDIUM))
                        .child(g.title.to_string()),
                )
            })
    }));
    let now = dense.then(|| {
        let line = div().mt(px(8.)).flex().gap(px(4.)).truncate().text_size(px(11.5)).text_color(TEXT_3);
        if call.status != ToolStatus::Running && groups.iter().all(|g| g.stage() == Stage::Done) {
            return line.child(format!("All {} phases done", groups.len()));
        }
        let at = current(groups).unwrap_or(0);
        let stopped = if call.status == ToolStatus::Running { "" } else { " · stopped" };
        line.child(div().flex_none().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(title(groups, at))).child(format!(
            "· phase {} of {}{stopped}",
            at + 1,
            groups.len()
        ))
    });
    div().flex_none().px(px(16.)).py(px(12.)).border_b(px(0.5)).border_color(SEPARATOR).child(bars).children(now)
}

fn title(groups: &[Group], at: usize) -> String {
    if groups[at].title.is_empty() { format!("Phase {}", at + 1) } else { groups[at].title.to_string() }
}

fn agent_row(key: &str, call: &ToolCall, agent: &chat::SubAgent, now: i64, on_agent: &OnAgent) -> Stateful<Div> {
    let m = sub_agent::mark(call, agent);
    let doing = match m {
        Mark::Running => sub_agent::latest(agent).map(sub_agent::step),
        Mark::Failed => sub_agent::report(call, agent).as_deref().and_then(sub_agent::first_line),
        Mark::Starting | Mark::Done => None,
    };
    let meta = match (agent.tokens, agent_detail::took(call, now)) {
        (0, ran) => ran.unwrap_or_default(),
        (t, ran) => [Some(thousands(t)), ran].into_iter().flatten().collect::<Vec<_>>().join(" · "),
    };
    let head = div()
        .h(px(18.))
        .flex()
        .items_center()
        .gap(px(8.))
        .child(sub_agent::glyph(&format!("{key}-{}", call.id), m))
        .child(div().flex_1().min_w_0().truncate().text_color(if m == Mark::Failed { FAILED_TEXT } else { TEXT_BODY }).child(sub_agent::name(agent)))
        .child(div().flex_none().text_size(px(11.5)).text_color(TEXT_4).child(meta));
    let line = doing.map(|d| {
        div()
            .pt(px(2.))
            .pl(px(22.))
            .truncate()
            .font_family(code_font())
            .text_size(px(11.))
            .line_height(px(16.))
            .text_color(if m == Mark::Failed { FAILED_TEXT } else { TEXT_3 })
            .child(d)
    });
    let (on_agent, id) = (on_agent.clone(), call.id.clone());
    div()
        .id(SharedString::from(format!("{key}-agent-{}", call.id)))
        .pl(px(31.))
        .pr(px(8.))
        .py(px(5.))
        .flex()
        .flex_col()
        .rounded(px(7.))
        .text_size(px(13.))
        .cursor_pointer()
        .hover(|s| s.bg(FILL_2))
        .active(|s| s.bg(FILL_3))
        .on_click(move |_, window, cx| on_agent(&id, window, cx))
        .child(head)
        .children(line)
}

/// What the dock needs to show a workflow.
pub(crate) struct Inspect<'a> {
    pub key: &'a str,
    pub call: &'a ToolCall,
    pub w: &'a Workflow,
    pub now: i64,
    /// Phases a click folded or unfolded from how `folded` has them.
    pub flipped: &'a HashSet<usize>,
    pub scroll: &'a ScrollHandle,
}

/// The dock's view of a workflow: a head with `actions`, how it stands and its phases as a strip, then each phase with
/// the agents it started. A click on an agent goes to `on_agent`, on a finished phase to `on_flip`.
pub(crate) fn inspector(i: Inspect, actions: Vec<AnyElement>, on_agent: OnAgent, on_flip: OnFlip) -> Div {
    let Inspect { key, call, w, now, flipped, scroll } = i;
    let m = mark(call, w);
    let groups = w.groups();
    let name = if w.name.is_empty() { "Workflow" } else { w.name.as_str() };
    let head = div()
        .flex()
        .items_start()
        .gap(px(10.))
        .pt(px(12.))
        .pl(px(16.))
        .pr(px(10.))
        .child(div().mt(px(3.)).child(sub_agent::glyph(&format!("{key}-{}", call.id), m)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .child(div().truncate().text_size(px(14.)).line_height(px(20.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(name.to_string()))
                .child(div().mt(px(2.)).text_size(px(12.)).text_color(TEXT_3).child(summary(call, w, now))),
        )
        .child(div().flex_none().flex().items_center().gap(px(2.)).children(actions));
    let fold: Vec<bool> = (0..groups.len()).map(|at| folded(&groups, at, flipped)).collect();
    let seek: OnFlip = {
        let (fold, on_flip, scroll) = (fold.clone(), on_flip.clone(), scroll.clone());
        Rc::new(move |at, window, cx| {
            if fold[at] {
                on_flip(at, window, cx);
            }
            scroll.scroll_to_top_of_item(at);
        })
    };
    let first_pending = groups.iter().position(|g| g.stage() == Stage::Pending);
    let phases = groups.iter().enumerate().map(|(at, g)| {
        let s = g.stage();
        let done = g.agents.iter().filter(|(c, _)| c.status != ToolStatus::Running).count();
        let on_flip = on_flip.clone();
        let header = div()
            .id(SharedString::from(format!("{key}-phase-{at}")))
            .h(px(30.))
            .mt(px(6.))
            .px(px(8.))
            .flex()
            .items_center()
            .gap(px(9.))
            .child(stage_mark(&format!("{key}-phase-{at}"), s))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.5))
                    .font_weight(if s == Stage::Pending { FontWeight::MEDIUM } else { FontWeight::SEMIBOLD })
                    .text_color(if s == Stage::Pending { TEXT_3 } else { TEXT })
                    .child(title(&groups, at)),
            )
            .when(!g.agents.is_empty(), |d| d.child(div().flex_none().text_size(px(11.5)).text_color(TEXT_4).child(format!("{done}/{}", g.agents.len()))))
            .when(s == Stage::Done, |d| {
                d.cursor_pointer().on_click(move |_, window, cx| on_flip(at, window, cx)).child(icon(
                    if fold[at] { "chevron-right" } else { "chevron-down" },
                    12.,
                    TEXT_4,
                ))
            });
        let open = !fold[at];
        let detail = (open && !g.detail.is_empty()).then(|| {
            div()
                .pl(px(31.))
                .pr(px(8.))
                .pb(px(4.))
                .mt(px(-4.))
                .whitespace_normal()
                .text_size(px(11.5))
                .line_height(px(16.))
                .text_color(TEXT_4)
                .child(g.detail.to_string())
        });
        let waiting =
            (first_pending == Some(at)).then(|| div().ml(px(31.)).mr(px(8.)).mb(px(4.)).text_size(px(11.5)).text_color(TEXT_4).child(waits_on(&groups, at)));
        let agents = open.then(|| g.agents.iter().map(|(c, a)| agent_row(key, c, a, now, &on_agent))).into_iter().flatten();
        div().flex().flex_col().child(header).children(detail).children(agents).children(waiting)
    });
    let body = div()
        .id(SharedString::from(format!("{key}-body")))
        .flex_1()
        .min_h_0()
        .overflow_y_scroll()
        .track_scroll(scroll)
        .pt(px(4.))
        .pb(px(10.))
        .px(px(8.))
        .flex()
        .flex_col()
        .children(phases);
    div().size_full().flex().flex_col().child(head).child(strip(key, call, &groups, &seek)).child(body)
}

#[cfg(test)]
mod tests {
    use super::summary;
    use chat::workflow::{Phase, Workflow};
    use chat::{Block, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};

    fn call(id: &str, status: ToolStatus, kind: ToolKind) -> ToolCall {
        ToolCall { id: id.into(), name: "Agent".into(), kind, status, output: None, duration_ms: Some(74_000), at: 0 }
    }

    fn workflow(agents: &[(ToolStatus, u64)]) -> Workflow {
        let blocks = agents
            .iter()
            .enumerate()
            .map(|(i, (status, tokens))| {
                Block::Tool(call(&format!("a{i}"), *status, ToolKind::Agent(SubAgent { phase: "Plan".into(), tokens: *tokens, ..Default::default() })))
            })
            .collect();
        Workflow {
            name: "measure".into(),
            phases: vec![Phase { title: "Plan".into(), detail: String::new() }],
            transcript: Transcript { turns: vec![Turn { blocks, ..Turn::default() }] },
        }
    }

    #[test]
    fn the_summary_counts_finished_agents_and_their_tokens_and_says_a_stop() {
        let w = workflow(&[(ToolStatus::Ok, 1_200), (ToolStatus::Running, 40_000)]);
        let wf = |status, output: Option<&str>| ToolCall { output: output.map(Into::into), ..call("wf", status, ToolKind::Workflow(Workflow::default())) };
        assert_eq!(summary(&wf(ToolStatus::Running, None), &w, 74_000), "Working · 1/2 agents · 41.2k tokens · 1m 14s");
        assert!(summary(&wf(ToolStatus::Ok, Some("Stopped")), &w, 0).starts_with("Stopped · "));
        assert_eq!(summary(&wf(ToolStatus::Ok, None), &workflow(&[]), 0), "Done · 0/0 agents · 1m 14s");
    }
}
