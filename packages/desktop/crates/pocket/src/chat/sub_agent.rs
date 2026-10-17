use super::phase;
use super::shimmer::shimmer;
use super::tool_row::{self, Tools};
use chat::{Block, SubAgent, ToolCall, ToolStatus};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;
use theme::*;

/// Chars a sub-agent's name keeps before it's cut.
const NAME_MAX: usize = 55;

/// Where a sub-agent stands, as its mark shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Mark {
    /// Running, nothing heard from it yet.
    Starting,
    Running,
    Done,
    Failed,
}

pub(crate) fn mark(call: &ToolCall, agent: &SubAgent) -> Mark {
    match call.status {
        ToolStatus::Running if agent.transcript.turns.iter().all(|t| t.blocks.is_empty()) => Mark::Starting,
        ToolStatus::Running => Mark::Running,
        ToolStatus::Ok => Mark::Done,
        ToolStatus::Failed => Mark::Failed,
    }
}

/// The task's first line, cut past 56 chars.
pub(crate) fn name(agent: &SubAgent) -> String {
    let Some(line) = agent.description.lines().map(str::trim).find(|l| !l.is_empty()) else { return "Subagent".into() };
    if line.chars().count() <= NAME_MAX + 1 {
        return line.to_string();
    }
    line.chars().take(NAME_MAX).collect::<String>() + "…"
}

/// What the sub-agent did: its tool calls and the notes it wrote between them.
fn steps(agent: &SubAgent) -> Vec<&Block> {
    agent.transcript.turns.iter().flat_map(|t| &t.blocks).filter(|b| matches!(b, Block::Tool(_) | Block::Text(_))).collect()
}

/// "failed", "starting", "done" when it shows no steps, or its step count with the failed ones.
pub(crate) fn meta(call: &ToolCall, agent: &SubAgent) -> String {
    let calls = || steps(agent).into_iter().filter_map(|b| if let Block::Tool(c) = b { Some(c) } else { None });
    match mark(call, agent) {
        Mark::Failed => "failed".into(),
        Mark::Starting => "starting".into(),
        Mark::Done if calls().next().is_none() => "done".into(),
        _ => {
            let (n, failed) = (calls().count(), calls().filter(|c| c.status == ToolStatus::Failed).count());
            let steps = format!("{n} step{}", if n == 1 { "" } else { "s" });
            if failed > 0 { format!("{steps}, {failed} failed") } else { steps }
        }
    }
}

/// What it reported back: the call's output, else the last thing it wrote. None while it runs.
pub(crate) fn report(call: &ToolCall, agent: &SubAgent) -> Option<String> {
    if call.status == ToolStatus::Running {
        return None;
    }
    let last = || steps(agent).into_iter().rev().find_map(|b| if let Block::Text(m) = b { Some(m.text.clone()) } else { None });
    call.output.clone().filter(|o| !o.trim().is_empty()).or_else(last).map(|r| r.trim().to_string())
}

fn glyph(id: &str, mark: Mark) -> AnyElement {
    let spin = |svg: Svg| svg.with_animation(SharedString::from(format!("agent-spin-{id}")), Animation::new(Duration::from_secs(1)).repeat().with_max_fps(20.), |s, t| s.with_transformation(Transformation::rotate(percentage(t))));
    match mark {
        Mark::Starting => icon("agent-starting", 14., TEXT_2).into_any_element(),
        Mark::Running => div().size(px(14.)).flex_none().relative().child(icon("agent-ring", 14., RAIL).absolute()).child(spin(icon("agent-arc", 14., TEXT_2).absolute())).into_any_element(),
        Mark::Done => icon("agent-done", 14., TEXT_4).into_any_element(),
        Mark::Failed => icon("agent-failed", 14., FAILED).into_any_element(),
    }
}

/// A sub-agent as one line: its mark, its task, then how far it got. Opened, its steps on a rail and its report.
/// `background` says it runs on after the turn that started it ended.
pub(crate) fn row(call: &ToolCall, agent: &SubAgent, background: bool, open: bool, on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static, tools: &Entity<Tools>, cx: &App) -> Div {
    let m = mark(call, agent);
    let steps = steps(agent);
    let report = report(call, agent);
    let openable = !steps.is_empty() || report.is_some();
    let open = open && openable;
    let failed = m == Mark::Failed;
    let name = name(agent);
    let label = div().flex_1().min_w_0().overflow_hidden().text_color(if failed { FAILED } else { TEXT_BODY });
    let label = if matches!(m, Mark::Running | Mark::Starting) { label.child(shimmer(SharedString::from(format!("agent-name-{}", call.id)), name, TEXT_BODY)) } else { label.truncate().child(name) };
    let side = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.))
        .text_color(TEXT_4)
        .when(background && call.status == ToolStatus::Running, |d| d.child("background"))
        .child(meta(call, agent))
        .when(openable, |d| d.child(icon(if open { "chevron-down" } else { "chevron-right" }, 14., TEXT_5)));
    let head = div()
        .id(SharedString::from(format!("agent-{}", call.id)))
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
        .when(open, |d| d.bg(FILL_3))
        .when(openable, |d| d.cursor_pointer().hover(|s| s.bg(FILL_3)).on_click(on_toggle))
        .child(glyph(&call.id, m))
        .child(label)
        .child(side);
    let body = open.then(|| {
        let n = steps.len();
        let rail = steps.iter().enumerate().map(|(i, b)| {
            let last = i + 1 == n && report.is_none();
            let step = match b {
                Block::Tool(c) => tool_row::row(c, false, tools, cx),
                Block::Text(t) => phase::note(&t.text),
                _ => div(),
            };
            phase::step(last, step)
        });
        let report = report.clone().map(|r| {
            let d = div().pt(px(6.)).whitespace_normal();
            if failed { d.font_family(code_font()).text_size(px(12.)).line_height(px(20.)).text_color(FAILED).child(r) } else { d.text_size(px(13.5)).line_height(rems(1.6)).text_color(TEXT_BODY).child(r) }
        });
        div().pt(px(2.)).pb(px(4.)).flex().flex_col().children(rail).children(report)
    });
    div().w_full().min_w_0().flex().flex_col().child(head).children(body)
}

#[cfg(test)]
mod tests {
    use super::{Mark, mark, meta, name, report};
    use chat::{Block, Message, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};

    fn tool(id: &str, status: ToolStatus) -> Block {
        Block::Tool(ToolCall { id: id.into(), name: "Bash".into(), kind: ToolKind::Shell { command: "ls".into() }, status, output: None, duration_ms: None, at: 0 })
    }

    fn agent(blocks: Vec<Block>) -> SubAgent {
        SubAgent { description: "Audit deps".into(), transcript: Transcript { turns: vec![Turn { id: "t".into(), prompt: None, blocks, end: None }] } }
    }

    fn call(a: &SubAgent, status: ToolStatus, output: Option<&str>) -> ToolCall {
        ToolCall { id: "c".into(), name: "Task".into(), kind: ToolKind::Agent(a.clone()), status, output: output.map(Into::into), duration_ms: None, at: 0 }
    }

    #[test]
    fn a_sub_agents_name_is_its_tasks_first_line_cut_past_56_chars() {
        let a = |d: &str| SubAgent { description: d.into(), ..Default::default() };
        assert_eq!(name(&a("\n  Check the fold\nthen more")), "Check the fold");
        assert_eq!(name(&a(&"x".repeat(56))), "x".repeat(56));
        assert_eq!(name(&a(&"x".repeat(57))), format!("{}…", "x".repeat(55)));
        assert_eq!(name(&a(" ")), "Subagent");
    }

    #[test]
    fn a_sub_agent_starts_until_it_does_something_then_counts_its_steps_if_it_shows_any() {
        let quiet = agent(vec![]);
        assert_eq!((mark(&call(&quiet, ToolStatus::Running, None), &quiet), meta(&call(&quiet, ToolStatus::Running, None), &quiet).as_str()), (Mark::Starting, "starting"));
        let busy = agent(vec![tool("a", ToolStatus::Ok), tool("b", ToolStatus::Failed), tool("c", ToolStatus::Running)]);
        assert_eq!(mark(&call(&busy, ToolStatus::Running, None), &busy), Mark::Running);
        assert_eq!(meta(&call(&busy, ToolStatus::Running, None), &busy), "3 steps, 1 failed");
        let one = agent(vec![tool("a", ToolStatus::Ok)]);
        assert_eq!(meta(&call(&one, ToolStatus::Ok, None), &one), "1 step");
        assert_eq!(meta(&call(&one, ToolStatus::Failed, None), &one), "failed");
        assert_eq!(meta(&call(&quiet, ToolStatus::Ok, Some("Done.")), &quiet), "done", "not \"0 steps\"");
    }

    #[test]
    fn a_sub_agent_reports_its_output_else_its_last_words_once_done() {
        let text = |s: &str| Block::Text(Message { id: s.into(), at: 0, text: s.into() });
        let a = agent(vec![text("first"), tool("a", ToolStatus::Ok), text("left-pad is unused")]);
        assert_eq!(report(&call(&a, ToolStatus::Running, None), &a), None, "nothing to report while it runs");
        assert_eq!(report(&call(&a, ToolStatus::Ok, None), &a).as_deref(), Some("left-pad is unused"));
        assert_eq!(report(&call(&a, ToolStatus::Ok, Some(" Found 2 \n")), &a).as_deref(), Some("Found 2"));
    }
}
