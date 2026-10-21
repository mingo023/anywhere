use super::markdown;
use super::phase;
use super::shimmer::shimmer;
use super::tool_row::{self, Arg, Tools};
use super::working::format_elapsed;
use crate::util::now_ms;
use chat::{Block, SubAgent, ToolCall, ToolStatus};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;
use theme::*;

/// Chars a sub-agent's name keeps before it's cut.
const NAME_MAX: usize = 55;
/// Lines a brief and a report show until "Show more".
const BRIEF_LINES: usize = 3;
const REPORT_LINES: usize = 12;
/// Chars a line of the body holds at the chat's usual width, to guess how many lines a text wraps to.
const WRAP: usize = 90;

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

/// What it reported back: the call's output, else the last thing it wrote. While it runs, only what it said so far as
/// it waits on its own work.
pub(crate) fn report(call: &ToolCall, agent: &SubAgent) -> Option<String> {
    let output = call.output.clone().filter(|o| !o.trim().is_empty());
    if call.status == ToolStatus::Running {
        return output.map(|r| r.trim().to_string());
    }
    let last = || steps(agent).into_iter().rev().find_map(|b| if let Block::Text(m) = b { Some(m.text.clone()) } else { None });
    output.or_else(last).map(|r| r.trim().to_string())
}

/// What it was asked to do: the prompt it started on, which its task only names.
pub(crate) fn brief(agent: &SubAgent) -> Option<&str> {
    agent.transcript.turns.first()?.prompt.as_ref().map(|p| p.text.trim()).filter(|t| !t.is_empty())
}

/// Its newest tool call.
pub(crate) fn latest(agent: &SubAgent) -> Option<&ToolCall> {
    agent.transcript.turns.iter().rev().flat_map(|t| t.blocks.iter().rev()).find_map(|b| if let Block::Tool(c) = b { Some(c) } else { None })
}

/// A call as one line of text: "Read docs/adr/0003.md", or the command it ran.
pub(crate) fn step(call: &ToolCall) -> String {
    let (verb, Arg::File(arg) | Arg::Text(arg) | Arg::Raw(arg)) = tool_row::line(call);
    if verb.is_empty() { arg } else { format!("{verb} {arg}") }
}

/// About how many lines `text` wraps to; the row has no window to measure it in.
fn lines(text: &str) -> usize {
    text.lines().map(|l| l.chars().count().div_ceil(WRAP).max(1)).sum()
}

/// Under a clipped text: "Show more", or "Show less" once it's open.
fn more(key: String, expanded: bool, tools: &Entity<Tools>) -> Div {
    let tools = tools.clone();
    div().flex().child(
        div()
            .id(SharedString::from(format!("more-{key}")))
            .pt(px(4.))
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .cursor_pointer()
            .hover(|s| s.text_color(TEXT))
            .on_click(move |_, _, cx| tools.update(cx, |t, cx| t.toggle(&key, false, None, cx)))
            .child(if expanded { "Show less" } else { "Show more" }),
    )
}

/// `body`, cut to `max` lines of `line` px unless opened by its "Show more", when `text` runs longer.
fn clipped(key: String, text: &str, max: usize, line: f32, body: impl IntoElement, tools: &Entity<Tools>, cx: &App) -> Div {
    let long = lines(text) > max;
    let expanded = tools.read(cx).details.open(&key, false);
    div().flex().flex_col().child(div().when(long && !expanded, |d| d.max_h(px(line * max as f32)).overflow_hidden()).child(body)).when(long, |d| d.child(more(key, expanded, tools)))
}

pub(crate) fn glyph(id: &str, mark: Mark) -> AnyElement {
    let spin = |svg: Svg| svg.with_animation(SharedString::from(format!("agent-spin-{id}")), Animation::new(Duration::from_secs(1)).repeat().with_max_fps(20.), |s, t| s.with_transformation(Transformation::rotate(percentage(t))));
    match mark {
        Mark::Starting => icon("agent-starting", 14., TEXT_2).into_any_element(),
        Mark::Running => div().size(px(14.)).flex_none().relative().child(icon("agent-ring", 14., RAIL).absolute()).child(spin(icon("agent-arc", 14., TEXT_2).absolute())).into_any_element(),
        Mark::Done => icon("agent-done", 14., TEXT_4).into_any_element(),
        Mark::Failed => icon("agent-failed", 14., FAILED).into_any_element(),
    }
}

/// A sub-agent as one line: its mark, its task, what it does now while it runs, its model, then how far it got and,
/// running, for how long. Opened, its brief, its steps on a rail and its report.
pub(crate) fn row(call: &ToolCall, agent: &SubAgent, open: bool, on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static, links: &markdown::Open, tools: &Entity<Tools>, cx: &App) -> Div {
    let m = mark(call, agent);
    let steps = steps(agent);
    let report = report(call, agent);
    let brief = brief(agent);
    let openable = !steps.is_empty() || report.is_some() || brief.is_some();
    let open = open && openable;
    let failed = m == Mark::Failed;
    let running = matches!(m, Mark::Running | Mark::Starting);
    let doing = (m == Mark::Running && !open).then(|| latest(agent).map(step)).flatten();
    let name = name(agent);
    let label = div().min_w_0().overflow_hidden().text_color(if failed { FAILED } else { TEXT_BODY });
    let label = if doing.is_some() { label.flex_none().max_w(relative(0.5)) } else { label.flex_1() };
    let label = if running { label.child(shimmer(SharedString::from(format!("agent-name-{}", call.id)), name, TEXT_BODY)) } else { label.truncate().child(name) };
    let doing = doing.map(|s| div().flex_1().min_w_0().truncate().font_family(code_font()).text_size(px(12.)).text_color(TEXT_3).child(s));
    let side = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.))
        .text_color(TEXT_4)
        .when(!agent.model.is_empty(), |d| d.child(agents::model_name(&agent.model)))
        .child(meta(call, agent))
        .when(running, |d| d.child(format_elapsed((now_ms() - call.at) / 1000)))
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
        .children(doing)
        .child(side);
    let body = open.then(|| {
        let brief = brief.map(|b| {
            let text = div().whitespace_normal().text_size(px(13.)).line_height(px(20.)).text_color(TEXT_3).child(b.to_string());
            div().pl(px(20.)).pt(px(2.)).pb(px(6.)).child(clipped(format!("brief-{}", call.id), b, BRIEF_LINES, 20., text, tools, cx))
        });
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
            let d = div().pt(px(6.));
            if failed {
                d.whitespace_normal().font_family(code_font()).text_size(px(12.)).line_height(px(20.)).text_color(FAILED).child(r)
            } else {
                let text = markdown::block(SharedString::from(format!("report-{}", call.id)), SharedString::from(r.clone()), false, links.clone()).text_size(px(13.5)).line_height(px(21.));
                d.child(clipped(format!("report-{}", call.id), &r, REPORT_LINES, 21., text, tools, cx))
            }
        });
        div().pt(px(2.)).pb(px(4.)).flex().flex_col().children(brief).children(rail).children(report)
    });
    div().w_full().min_w_0().flex().flex_col().child(head).children(body)
}

#[cfg(test)]
mod tests {
    use super::{Mark, brief, latest, lines, mark, meta, name, report, step};
    use chat::{Block, Message, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};

    fn tool(id: &str, status: ToolStatus) -> Block {
        Block::Tool(ToolCall { id: id.into(), name: "Bash".into(), kind: ToolKind::Shell { command: "ls".into() }, status, output: None, duration_ms: None, at: 0 })
    }

    fn agent(blocks: Vec<Block>) -> SubAgent {
        SubAgent { description: "Audit deps".into(), transcript: Transcript { turns: vec![Turn { id: "t".into(), prompt: None, blocks, end: None }] }, ..Default::default() }
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

    #[test]
    fn a_sub_agent_waiting_on_its_own_work_shows_what_it_said_so_far() {
        let a = agent(vec![tool("a", ToolStatus::Running)]);
        assert_eq!(report(&call(&a, ToolStatus::Running, Some(" Started the build; waiting on it.\n")), &a).as_deref(), Some("Started the build; waiting on it."));
        assert_eq!(report(&call(&a, ToolStatus::Running, Some("  ")), &a), None);
    }

    #[test]
    fn a_sub_agents_brief_is_the_prompt_it_started_on() {
        let mut a = agent(vec![]);
        assert_eq!(brief(&a), None);
        a.transcript.turns[0].prompt = Some(Message { id: "p".into(), at: 0, text: "\n Review the diff.\n".into() });
        assert_eq!(brief(&a), Some("Review the diff."));
        a.transcript.turns[0].prompt = Some(Message { id: "p".into(), at: 0, text: " ".into() });
        assert_eq!(brief(&a), None);
    }

    #[test]
    fn a_running_sub_agent_shows_its_newest_call_as_a_step() {
        let mut read = ToolCall { id: "r".into(), name: "Read".into(), kind: ToolKind::Read { path: "docs/spec.md".into() }, status: ToolStatus::Ok, output: None, duration_ms: None, at: 0 };
        let a = agent(vec![tool("a", ToolStatus::Ok), Block::Tool(read.clone()), Block::Text(Message::default())]);
        assert_eq!(latest(&a).map(step).as_deref(), Some("Read docs/spec.md"));
        read.kind = ToolKind::Shell { command: "\ncargo test\n".into() };
        assert_eq!(step(&read), "cargo test");
        assert_eq!(latest(&agent(vec![])), None);
    }

    #[test]
    fn a_text_counts_the_lines_it_wraps_to() {
        assert_eq!(lines("one"), 1);
        assert_eq!(lines("one\n\nthree"), 3);
        assert_eq!(lines(&"x".repeat(91)), 2);
    }
}
