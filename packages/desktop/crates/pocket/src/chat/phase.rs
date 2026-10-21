use super::shimmer::shimmer;
use super::tool_detail;
use super::tool_row::{self, Tools};
use super::{sub_agent, thinking, working::format_elapsed};
use chat::{Block, ToolKind, ToolStatus};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// Steps a live phase keeps in view; older ones scroll away above them.
const LIVE_STEPS: usize = 9;

/// A rail's spine down the left and an elbow into `child`; the last one's spine stops at its elbow.
/// `top` lowers the elbow, for a row that starts with space above.
fn rail(last: bool, top: f32, child: impl IntoElement) -> Div {
    div()
        .relative()
        .pl(px(20.))
        .min_w_0()
        .child(div().absolute().left(px(7.)).top_0().w(px(1.)).bg(RAIL).map(|d| if last { d.h(px(top + 7.)) } else { d.h_full() }))
        .child(div().absolute().left(px(7.)).top(px(top + 6.)).size(px(8.)).border_l_1().border_b_1().border_color(RAIL).rounded_bl(px(8.)))
        .child(child)
}

/// One step on a phase's or a sub-agent's rail.
pub(super) fn step(last: bool, child: impl IntoElement) -> Div {
    rail(last, 0., child)
}

/// A row inside the fold, hung off its rail; `last` ends the rail.
pub(super) fn in_fold(last: bool, child: impl IntoElement) -> Div {
    rail(last, 6., div().pt(px(6.)).pb(px(4.)).child(child))
}

/// Text the agent wrote between steps.
pub(super) fn note(text: &str) -> Div {
    div().py(px(4.)).text_size(px(13.5)).line_height(rems(1.6)).text_color(TEXT_BODY).child(text.trim().to_string())
}

/// The icon heading a phase: what its work mostly is.
pub(crate) fn kind_icon(blocks: &[Block]) -> &'static str {
    let first = blocks.iter().find(|b| chat::is_work(b));
    match first {
        Some(Block::Thinking(_)) => "bulb",
        Some(Block::Tool(c)) => match &c.kind {
            ToolKind::Edit { .. } | ToolKind::Write { .. } => "pencil",
            ToolKind::Shell { .. } => "terminal",
            ToolKind::Agent(_) => "bot",
            ToolKind::Read { .. } | ToolKind::Search { .. } | ToolKind::Fetch { .. } => "search",
            _ => "tool",
        },
        _ => "tool",
    }
}

/// The phase's steps: its thinking and tool calls, without the todos updated among them.
pub(crate) fn steps(blocks: &[Block]) -> Vec<(usize, &Block)> {
    blocks.iter().enumerate().filter(|(_, b)| chat::is_work(b)).collect()
}

/// How long a thought took: until the next thing the agent did. None under a second.
pub(crate) fn thought_for(at: i64, next: Option<i64>) -> Option<String> {
    next.filter(|&n| n - at >= 1000).map(|n| format!("Thought for {}", format_elapsed((n - at) / 1000)))
}

fn thought(id: &str, text: &str, label: Option<String>, live: bool, tools: &Entity<Tools>, cx: &App) -> Div {
    let open = tools.read(cx).details.open(id, false);
    let head = div()
        .id(SharedString::from(format!("thought-{id}")))
        .py(px(4.))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(14.))
        .line_height(px(20.))
        .text_color(TEXT_3)
        .cursor_pointer()
        .hover(|s| s.text_color(TEXT_2))
        .on_click({
            let (tools, id) = (tools.clone(), id.to_string());
            move |_, _, cx| tools.update(cx, |t, cx| t.toggle(&id, false, None, cx))
        })
        .map(|d| match (live, label) {
            (true, _) => d.child(shimmer(SharedString::from(format!("thinking-{id}")), "Thinking", TEXT_3)),
            (false, Some(l)) => d.child(l),
            (false, None) => d.child("Thought"),
        })
        .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_4));
    let body = open.then(|| tool_detail::body(&thinking::lines(text, live), false));
    div().flex().flex_col().child(head).children(body)
}

/// One step of a phase. `next` is when the agent did its next thing, which ends a thought.
fn step_view(block: &Block, next: Option<i64>, live: bool, tools: &Entity<Tools>, cx: &App) -> Div {
    match block {
        Block::Thinking(m) => thought(&m.id, &m.text, thought_for(m.at, next), live && next.is_none(), tools, cx),
        Block::Tool(c @ chat::ToolCall { kind: ToolKind::Agent(a), .. }) => {
            let failed = c.status == ToolStatus::Failed;
            let on_toggle = {
                let (tools, id) = (tools.clone(), c.id.clone());
                move |_: &ClickEvent, _: &mut Window, cx: &mut App| tools.update(cx, |t, cx| t.toggle(&id, failed, None, cx))
            };
            sub_agent::row(c, a, tools.read(cx).details.open(&c.id, failed), on_toggle, tools, cx)
        }
        Block::Tool(c) => tool_row::row(c, !live, tools, cx),
        _ => div(),
    }
}

/// A phase in the fold: a title line that opens to its steps, or, for a lone step, just that step.
/// `next_at` is when whatever follows the phase began; `live` forces it open with its newest steps showing, as a
/// sub-agent still running does. A sub-agent always shows under a title, which counts the sub-agents.
#[allow(clippy::too_many_arguments)]
pub(super) fn phase(id: &str, blocks: &[Block], next_at: Option<i64>, live: bool, open: bool, on_toggle: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static, tools: &Entity<Tools>, cx: &App) -> Div {
    let steps = steps(blocks);
    let at = |i: usize| blocks.get(i).and_then(super::transcript::block_at);
    let next = |i: usize| (i + 1..blocks.len()).find_map(at).or(next_at);
    if let [(i, b)] = steps[..]
        && !matches!(b, Block::Tool(c) if matches!(c.kind, ToolKind::Agent(_)))
    {
        let icon_name = kind_icon(blocks);
        return div().flex().gap(px(6.)).child(div().mt(px(7.)).child(icon(icon_name, 14., TEXT_4))).child(div().flex_1().min_w_0().child(step_view(b, next(i), live, tools, cx)));
    }
    let open = live || open || blocks.iter().any(chat::running_agent);
    let head_icon = if open && !live { "chevron-down" } else { kind_icon(blocks) };
    let label = chat::phase_title(blocks, live);
    let title = div().min_w_0().flex_1().overflow_hidden().text_color(TEXT_3);
    let title = if live { title.child(shimmer(SharedString::from(format!("phase-{id}")), label, TEXT_3)) } else { title.truncate().child(label) };
    let head = div()
        .id(SharedString::from(format!("phase-head-{id}")))
        .py(px(4.))
        .flex()
        .items_center()
        .gap(px(6.))
        .text_size(px(14.))
        .line_height(px(20.))
        .child(icon(head_icon, 14., TEXT_4))
        .child(title)
        .when(!live, |d| d.cursor_pointer().hover(|s| s.opacity(0.8)).on_click(on_toggle));
    let body = open.then(|| {
        let skip = if live { steps.len().saturating_sub(LIVE_STEPS) } else { 0 };
        let n = steps.len();
        div().flex().flex_col().children(steps.iter().enumerate().skip(skip).map(|(k, &(i, b))| step(k + 1 == n, step_view(b, next(i), live, tools, cx))))
    });
    div().flex().flex_col().child(head).children(body)
}

#[cfg(test)]
mod tests {
    use super::{kind_icon, steps, thought_for};
    use chat::{Block, Message, ToolCall, ToolKind, ToolStatus};

    fn tool(kind: ToolKind) -> Block {
        Block::Tool(ToolCall { id: "t".into(), name: "x".into(), kind, status: ToolStatus::Ok, output: None, duration_ms: None, at: 0 })
    }

    fn text(s: &str) -> Block {
        Block::Text(Message { id: s.into(), at: 0, text: s.into() })
    }

    #[test]
    fn a_phase_is_headed_by_the_kind_of_its_first_step() {
        assert_eq!(kind_icon(&[text("Let me look."), tool(ToolKind::Read { path: "a".into() }), tool(ToolKind::Shell { command: "ls".into() })]), "search");
        assert_eq!(kind_icon(&[tool(ToolKind::Edit { path: "a".into(), diff: None })]), "pencil");
        assert_eq!(kind_icon(&[tool(ToolKind::Shell { command: "ls".into() })]), "terminal");
        assert_eq!(kind_icon(&[Block::Thinking(Message::default())]), "bulb");
        assert_eq!(kind_icon(&[tool(ToolKind::Other { input: "{}".into() })]), "tool");
    }

    #[test]
    fn a_phases_steps_leave_out_its_todos() {
        let blocks = [tool(ToolKind::Read { path: "a".into() }), Block::Todos { id: "t".into(), todos: vec![] }, Block::Thinking(Message::default())];
        assert_eq!(steps(&blocks).iter().map(|(i, _)| *i).collect::<Vec<_>>(), [0, 2]);
    }

    #[test]
    fn a_thought_lasts_until_the_agents_next_move() {
        assert_eq!(thought_for(1_000, Some(4_500)).as_deref(), Some("Thought for 3s"));
        assert_eq!(thought_for(1_000, None), None);
        assert_eq!(thought_for(1_000, Some(1_999)), None, "not \"Thought for 0s\"");
    }
}
