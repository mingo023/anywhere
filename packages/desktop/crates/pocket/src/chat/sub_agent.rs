use super::agent_detail;
use super::shimmer::shimmer;
use super::tool_row::{self, Arg};
use crate::util::now_ms;
use chat::{Block, SubAgent, ToolCall, ToolKind, ToolStatus};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::rc::Rc;
use std::time::Duration;
use theme::*;

/// Chars a sub-agent's name keeps before it's cut.
const NAME_MAX: usize = 55;
const ROW: &str = "agent-row";

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
pub(crate) fn steps(agent: &SubAgent) -> Vec<&Block> {
    agent.transcript.turns.iter().flat_map(|t| &t.blocks).filter(|b| matches!(b, Block::Tool(_) | Block::Text(_))).collect()
}

/// "3 steps · 1m 14s": how many tool calls it made and how long it ran, or "starting" until it does anything.
pub(crate) fn meta(call: &ToolCall, agent: &SubAgent, now: i64) -> String {
    let m = mark(call, agent);
    if m == Mark::Starting {
        return "starting".into();
    }
    let n = agent_detail::calls(agent).count();
    let steps = (n > 0).then(|| format!("{n} step{}", if n == 1 { "" } else { "s" }));
    let parts: Vec<String> = [steps, agent_detail::took(call, now)].into_iter().flatten().collect();
    match (parts.is_empty(), m) {
        (true, Mark::Failed) => "failed".into(),
        (true, _) => "done".into(),
        _ => parts.join(" · "),
    }
}

/// What it reported back: the call's output, else the last thing it wrote. While it runs, only what it said so far as
/// it waits on its own work.
pub(crate) fn report(call: &ToolCall, agent: &SubAgent) -> Option<String> {
    report_text(call, agent).map(str::to_string)
}

fn report_text<'a>(call: &'a ToolCall, agent: &'a SubAgent) -> Option<&'a str> {
    let output = call.output.as_deref().map(str::trim).filter(|o| !o.is_empty());
    if call.status == ToolStatus::Running {
        return output;
    }
    let last = || steps(agent).into_iter().rev().find_map(|b| if let Block::Text(m) = b { Some(m.text.trim()) } else { None });
    output.or_else(last)
}

/// A report's first line as plain text, without its markdown's heading, list or quote marks, bold or code ticks.
pub(crate) fn first_line(text: &str) -> Option<String> {
    text.lines().find_map(|l| {
        let l = l.trim().trim_start_matches(['#', '>']).trim_start();
        let l = l.strip_prefix("- ").or_else(|| l.strip_prefix("* ")).unwrap_or(l);
        let digits = l.find(|c: char| !c.is_ascii_digit()).unwrap_or(0);
        let l = if digits > 0 { l[digits..].strip_prefix(". ").unwrap_or(l) } else { l };
        let l = l.replace("**", "").replace('`', "");
        let l = l.trim();
        (!l.is_empty()).then(|| l.to_string())
    })
}

/// The model every sub-agent among `blocks` runs on, as its name, when they share one.
pub(crate) fn shared_model(blocks: &[Block]) -> Option<String> {
    let mut models = blocks.iter().filter_map(|b| match b {
        Block::Tool(ToolCall { kind: ToolKind::Agent(a), .. }) => Some(a.model.as_str()),
        _ => None,
    });
    let first = models.next().filter(|m| !m.is_empty()).map(agents::model_name)?;
    models.all(|m| !m.is_empty() && agents::model_name(m) == first).then_some(first)
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

pub(crate) fn glyph(id: &str, mark: Mark) -> AnyElement {
    let spin = |svg: Svg| {
        svg.with_animation(SharedString::from(format!("agent-spin-{id}")), Animation::new(Duration::from_secs(1)).repeat_synced().with_max_fps(20.), |s, t| {
            s.with_transformation(Transformation::rotate(percentage(t)))
        })
    };
    match mark {
        Mark::Starting => icon("agent-starting", 14., TEXT_2).into_any_element(),
        Mark::Running => div()
            .size(px(14.))
            .flex_none()
            .relative()
            .child(icon("agent-ring", 14., RAIL).absolute())
            .child(spin(icon("agent-arc", 14., TEXT_2).absolute()))
            .into_any_element(),
        Mark::Done => icon("agent-done", 14., TEXT_4).into_any_element(),
        Mark::Failed => icon("agent-failed", 14., FAILED).into_any_element(),
    }
}

/// Shows a sub-agent's call in its chat's dock.
pub(crate) type OnPick = Rc<dyn Fn(&str, &mut Window, &mut App)>;

/// The sub-agent a chat's dock shows, and what a click on a sub-agent's row does: show its call there.
pub(crate) struct Picks {
    pub(crate) open: Option<String>,
    pub(crate) on_pick: OnPick,
}

/// A sub-agent as one line: its mark, its task, what it does now while it runs or what it found once done, then how far
/// it got and for how long, after its model unless the phase's title names it. A click shows it in the dock.
pub(crate) fn row(call: &ToolCall, agent: &SubAgent, model: bool, picks: &Picks) -> Div {
    let m = mark(call, agent);
    let open = picks.open.as_deref() == Some(call.id.as_str());
    let running = matches!(m, Mark::Running | Mark::Starting);
    let line = match m {
        Mark::Running => latest(agent).map(|c| div().font_family(code_font()).text_size(px(12.)).text_color(TEXT_3).child(step(c))),
        Mark::Starting => None,
        Mark::Done | Mark::Failed => report_text(call, agent)
            .and_then(first_line)
            .map(|f| div().text_size(px(13.)).text_color(if m == Mark::Failed { FAILED_TEXT } else { TEXT_3 }).child(f)),
    };
    let name = name(agent);
    let color = match m {
        _ if open => TEXT,
        Mark::Failed => FAILED_TEXT,
        Mark::Done => TEXT_2,
        _ => TEXT_BODY,
    };
    let label = div().min_w_0().overflow_hidden().text_color(color);
    let label = if line.is_some() { label.flex_none().max_w(relative(0.44)) } else { label.flex_1() };
    let label =
        if running { label.child(shimmer(SharedString::from(format!("agent-name-{}", call.id)), name, TEXT_BODY)) } else { label.truncate().child(name) };
    let line = line.map(|l| l.flex_1().min_w_0().truncate());
    let side = div()
        .flex_none()
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.))
        .text_color(TEXT_4)
        .when(model && !agent.model.is_empty(), |d| d.child(agents::model_name(&agent.model)))
        .child(meta(call, agent, now_ms()))
        .child(div().when(!open, |d| d.invisible().group_hover(ROW, |s| s.visible())).child(icon("chevron-right", 14., if open { TEXT_2 } else { TEXT_5 })));
    let on_pick = picks.on_pick.clone();
    let id = call.id.clone();
    let head = div()
        .id(SharedString::from(format!("agent-{}", call.id)))
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
        .child(glyph(&call.id, m))
        .child(label)
        .children(line)
        .child(side);
    div().w_full().min_w_0().flex().child(head)
}

#[cfg(test)]
mod tests {
    use super::{Mark, brief, first_line, latest, mark, meta, name, report, shared_model, step};
    use chat::{Block, Message, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};

    fn tool(id: &str, status: ToolStatus) -> Block {
        Block::Tool(ToolCall {
            id: id.into(),
            name: "Bash".into(),
            kind: ToolKind::Shell { command: "ls".into(), description: String::new() },
            status,
            output: None,
            duration_ms: None,
            at: 0,
        })
    }

    fn agent(blocks: Vec<Block>) -> SubAgent {
        SubAgent {
            description: "Audit deps".into(),
            transcript: Transcript { turns: vec![Turn { id: "t".into(), prompt: None, blocks, end: None }] },
            ..Default::default()
        }
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
    fn a_sub_agent_starts_until_it_does_something_then_counts_its_steps_and_time() {
        let quiet = agent(vec![]);
        assert_eq!(
            (mark(&call(&quiet, ToolStatus::Running, None), &quiet), meta(&call(&quiet, ToolStatus::Running, None), &quiet, 5_000).as_str()),
            (Mark::Starting, "starting")
        );
        let busy = agent(vec![tool("a", ToolStatus::Ok), tool("b", ToolStatus::Failed), tool("c", ToolStatus::Running)]);
        assert_eq!(mark(&call(&busy, ToolStatus::Running, None), &busy), Mark::Running);
        assert_eq!(meta(&call(&busy, ToolStatus::Running, None), &busy, 74_000), "3 steps · 1m 14s");
        let one = agent(vec![tool("a", ToolStatus::Ok)]);
        let timed = |status, ms| ToolCall { duration_ms: ms, ..call(&one, status, None) };
        assert_eq!(meta(&timed(ToolStatus::Ok, Some(9_000)), &one, 0), "1 step · 9s");
        assert_eq!(meta(&timed(ToolStatus::Failed, None), &one, 0), "1 step", "no duration known, none shown");
        assert_eq!(meta(&call(&quiet, ToolStatus::Ok, Some("Done.")), &quiet, 0), "done", "not \"0 steps\"");
        assert_eq!(meta(&call(&quiet, ToolStatus::Failed, None), &quiet, 0), "failed");
    }

    #[test]
    fn a_reports_first_line_drops_its_markdown() {
        assert_eq!(first_line("\n## Summary\nmore").as_deref(), Some("Summary"));
        assert_eq!(first_line("- **left-pad** is unused").as_deref(), Some("left-pad is unused"));
        assert_eq!(first_line("1. Use `cargo test`").as_deref(), Some("Use cargo test"));
        assert_eq!(first_line("> quoted").as_deref(), Some("quoted"));
        assert_eq!(first_line("2024 was fine").as_deref(), Some("2024 was fine"), "a number that starts a sentence stays");
        assert_eq!(first_line("#\n  \n"), None);
    }

    #[test]
    fn the_phase_names_the_model_only_when_every_sub_agent_shares_it() {
        let on = |model: &str| Block::Tool(call(&SubAgent { model: model.into(), ..Default::default() }, ToolStatus::Ok, None));
        let sonnet = agents::model_name("claude-sonnet-4-5");
        assert_eq!(shared_model(&[on("claude-sonnet-4-5"), tool("s", ToolStatus::Ok), on("claude-sonnet-4-5")]), Some(sonnet));
        assert_eq!(shared_model(&[on("claude-sonnet-4-5"), on("claude-haiku-4-5")]), None);
        assert_eq!(shared_model(&[on("claude-sonnet-4-5"), on("")]), None, "one not known yet");
        assert_eq!(shared_model(&[on("")]), None);
        assert_eq!(shared_model(&[tool("s", ToolStatus::Ok)]), None);
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
        assert_eq!(
            report(&call(&a, ToolStatus::Running, Some(" Started the build; waiting on it.\n")), &a).as_deref(),
            Some("Started the build; waiting on it.")
        );
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
        let mut read = ToolCall {
            id: "r".into(),
            name: "Read".into(),
            kind: ToolKind::Read { path: "docs/spec.md".into() },
            status: ToolStatus::Ok,
            output: None,
            duration_ms: None,
            at: 0,
        };
        let a = agent(vec![tool("a", ToolStatus::Ok), Block::Tool(read.clone()), Block::Text(Message::default())]);
        assert_eq!(latest(&a).map(step).as_deref(), Some("Read docs/spec.md"));
        read.kind = ToolKind::Shell { command: "\ncargo test\n".into(), description: String::new() };
        assert_eq!(step(&read), "cargo test");
        assert_eq!(latest(&agent(vec![])), None);
    }
}
