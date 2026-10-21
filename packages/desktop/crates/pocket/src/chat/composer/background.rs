use super::queue::{control, surface};
use super::send_button::stop_square;
use crate::chat::sub_agent::{self, Mark, step};
use crate::chat::working::format_elapsed;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use crate::util::now_ms;
use chat::{Command, ToolCall, ToolKind, Transcript};
use gpui_kit::*;
use theme::*;

/// A sub-agent's newest tool call; none for a command.
fn latest(call: &ToolCall) -> Option<&ToolCall> {
    let ToolKind::Agent(a) = &call.kind else { return None };
    sub_agent::latest(a)
}

/// What a task does now: a sub-agent's newest step, "Starting…" before it took one; none for a command.
fn doing(call: &ToolCall) -> Option<String> {
    match (latest(call), &call.kind) {
        (Some(c), _) => Some(step(c)),
        (None, ToolKind::Agent(_)) => Some("Starting…".into()),
        _ => None,
    }
}

/// The newest step any task took: the folded tray's one line.
fn newest(tasks: &[&ToolCall]) -> Option<String> {
    tasks.iter().filter_map(|c| latest(c)).max_by_key(|c| c.at).map(step)
}

fn name(call: &ToolCall) -> String {
    match &call.kind {
        ToolKind::Agent(a) => sub_agent::name(a),
        _ => step(call),
    }
}

/// Whether the tray offers Stop: the chat's own agent stops what it left in the background, where a terminal agent's Esc
/// doesn't reach it. The composer offers Send meanwhile, as a send starts a new turn.
pub(super) fn stops(chat_only: bool, interrupt: bool) -> bool {
    chat_only && interrupt
}

impl Desktop {
    /// The tray over the box: the sub-agents and commands the agent left at work past their turn, each showing its row on a
    /// click; none when nothing is. Folded, it's one line: how many, and the newest step. With `stop`, Stop ends them all.
    pub(super) fn background_tray(&self, agent: &str, t: &Transcript, stop: bool, cx: &mut Context<Self>) -> Option<Div> {
        let tasks: Vec<_> = chat::in_background(&t.turns).collect();
        if tasks.is_empty() {
            return None;
        }
        let folded = self.chats.composer.background_folded;
        let calls: Vec<_> = tasks.iter().map(|(_, _, c)| *c).collect();
        let line = |d: Div| d.min_w_0().truncate().font_family(code_font()).text_size(px(11.5)).text_color(TEXT_3);
        let header = div()
            .id(id(format!("chat-background:{agent}")))
            .min_h(px(26.))
            .mx(px(-6.))
            .px(px(6.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(7.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_3))
            .text_size(px(12.))
            .child(icon("bot", 13., TEXT_2))
            .child(div().flex_none().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(format!("{} in background", tasks.len())))
            .child(line(div().flex_1()).children(folded.then(|| newest(&calls)).flatten()))
            .children(stop.then(|| {
                let agent = agent.to_string();
                control(format!("chat-background-stop:{agent}"), "Stop the background work")
                    .px(px(8.))
                    .gap(px(6.))
                    .text_color(TEXT_2)
                    .child(stop_square(TEXT_2).size(px(8.)).rounded(px(2.)))
                    .child("Stop")
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.chat_command(&agent, Command::Interrupt, cx);
                    }))
            }))
            .child(icon(if folded { "chevron-right" } else { "chevron-down" }, 13., TEXT_4))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.chats.composer.background_folded ^= true;
                cx.notify();
            }));
        let now = now_ms();
        let rows = (!folded).then(|| {
            tasks.iter().map(|&(turn, block, call)| {
                let mark = match &call.kind {
                    ToolKind::Agent(a) => sub_agent::mark(call, a),
                    _ => Mark::Running,
                };
                let agent = agent.to_string();
                div()
                    .id(id(format!("chat-background:{agent}:{}", call.id)))
                    .min_h(px(30.))
                    .mx(px(-6.))
                    .px(px(6.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .rounded(px(7.))
                    .cursor_pointer()
                    .hover(|s| s.bg(FILL_3))
                    .text_size(px(13.))
                    .child(sub_agent::glyph(&call.id, mark))
                    .child(div().max_w(relative(0.5)).flex_none().truncate().text_color(TEXT_BODY).child(name(call)))
                    .child(line(div().flex_1()).children(doing(call)))
                    .child(div().flex_none().text_size(px(12.)).text_color(TEXT_4).child(format_elapsed((now - call.at) / 1000)))
                    .child(div().flex_none().flex().items_center().gap(px(3.)).text_size(px(12.)).text_color(TEXT_4).child("Show").child(icon("arrow-up", 11., TEXT_4)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_show_block(&agent, turn, block, cx)))
            })
        });
        Some(surface().py(px(6.)).child(header).children(rows.into_iter().flatten()))
    }
}

#[cfg(test)]
mod tests {
    use super::{doing, name, newest, stops};
    use chat::{Block, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};

    fn call(kind: ToolKind, at: i64) -> ToolCall {
        ToolCall { id: format!("c{at}"), name: "Task".into(), kind, status: ToolStatus::Running, output: None, duration_ms: None, at }
    }

    fn agent(task: &str, blocks: Vec<ToolCall>) -> ToolCall {
        let turn = Turn { id: "t".into(), prompt: None, blocks: blocks.into_iter().map(Block::Tool).collect(), end: None };
        call(ToolKind::Agent(SubAgent { description: task.into(), transcript: Transcript { turns: vec![turn] }, ..Default::default() }), 0)
    }

    #[test]
    fn a_task_says_what_it_does_now_and_a_command_just_its_name() {
        let reading = agent("Review the spec", vec![call(ToolKind::Search { query: "fold".into() }, 1), call(ToolKind::Read { path: "docs/spec.md".into() }, 2)]);
        assert_eq!((name(&reading), doing(&reading).as_deref()), ("Review the spec".into(), Some("Read docs/spec.md")));
        assert_eq!(doing(&agent("Review the spec", vec![])).as_deref(), Some("Starting…"));
        let server = call(ToolKind::Shell { command: "bun run dev\n".into() }, 0);
        assert_eq!((name(&server), doing(&server)), ("bun run dev".into(), None));
    }

    #[test]
    fn the_folded_tray_shows_the_newest_step_of_any_task() {
        let a = agent("a", vec![call(ToolKind::Read { path: "old.rs".into() }, 5)]);
        let b = agent("b", vec![call(ToolKind::Search { query: "left-pad".into() }, 9)]);
        let server = call(ToolKind::Shell { command: "bun run dev".into() }, 20);
        assert_eq!(newest(&[&a, &b, &server]).as_deref(), Some("Find left-pad"));
        assert_eq!(newest(&[&server]), None);
    }

    #[test]
    fn a_chat_left_with_only_background_work_still_offers_a_stop() {
        assert!(stops(true, true));
        assert!(!stops(false, true), "a terminal agent's Esc doesn't reach what it left in the background");
        assert!(!stops(true, false));
    }
}
