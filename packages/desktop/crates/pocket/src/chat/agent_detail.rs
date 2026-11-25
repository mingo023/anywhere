use super::markdown;
use super::meta::format_timestamp;
use super::shimmer::shimmer;
use super::sub_agent::{self, Mark};
use super::tool_row::{self, Tools};
use super::working::format_elapsed;
use chat::{Block, SubAgent, ToolCall, ToolStatus};
use gpui_kit::base::text::TextView;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

/// Steps shown until "Show N more".
const RECENT: usize = 4;
const LABEL_W: f32 = 84.;

/// How long `call` has run: until now while it runs.
pub(crate) fn ran_ms(call: &ToolCall, now: i64) -> i64 {
    match call.status {
        ToolStatus::Running => now - call.at,
        _ => call.duration_ms.map_or(0, |ms| ms as i64),
    }
}

/// How long it ran, while it runs or once its duration is known.
pub(crate) fn took(call: &ToolCall, now: i64) -> Option<String> {
    (call.status == ToolStatus::Running || call.duration_ms.is_some()).then(|| format_elapsed(ran_ms(call, now) / 1000))
}

/// "32.9k", "640".
pub(crate) fn thousands(n: u64) -> String {
    if n < 1_000 { n.to_string() } else { format!("{:.1}k", n as f64 / 1_000.) }
}

/// The tool calls a sub-agent made itself, oldest first.
pub(crate) fn calls(agent: &SubAgent) -> impl Iterator<Item = &ToolCall> {
    agent.transcript.turns.iter().flat_map(|t| &t.blocks).filter_map(|b| if let Block::Tool(c) = b { Some(c) } else { None })
}

/// Where the first of `n` steps shown starts: the last four, unless all are asked for.
fn first_shown(n: usize, all: bool) -> usize {
    if all { 0 } else { n.saturating_sub(RECENT) }
}

/// One line of the facts: its label, its value and the value's colour, and a part that went wrong, drawn red after it.
pub(crate) struct Fact {
    label: &'static str,
    value: String,
    color: Token,
    bad: Option<String>,
}

fn fact(label: &'static str, value: String) -> Fact {
    Fact { label, value, color: TEXT_BODY, bad: None }
}

/// Status, model, start, duration, steps and tokens; a fact not known yet is left out.
pub(crate) fn facts(call: &ToolCall, agent: &SubAgent, now: i64, local: impl Fn(i64) -> String) -> Vec<Fact> {
    let (status, color) = match sub_agent::mark(call, agent) {
        Mark::Starting | Mark::Running => ("Working", TEXT_2),
        Mark::Done => ("Done", SUCCESS_TEXT),
        Mark::Failed => ("Failed", FAILED_TEXT),
    };
    let (steps, failed) = (calls(agent).count(), calls(agent).filter(|c| c.status == ToolStatus::Failed).count());
    [
        Some(Fact { color, ..fact("Status", status.into()) }),
        (!agent.model.is_empty()).then(|| fact("Model", agents::model_name(&agent.model))),
        Some(fact("Started", local(call.at))),
        took(call, now).map(|t| fact("Duration", t)),
        Some(Fact { bad: (failed > 0).then(|| format!("{failed} failed")), ..fact("Steps", steps.to_string()) }),
        (agent.tokens > 0).then(|| fact("Tokens", thousands(agent.tokens))),
    ]
    .into_iter()
    .flatten()
    .collect()
}

/// A sub-agent's details as the Facts layout draws them, in the dock beside the chat and on the agent map.
pub(crate) struct Detail<'a> {
    /// Unique to where it's drawn, for its elements' ids.
    pub(crate) key: String,
    pub(crate) call: &'a ToolCall,
    pub(crate) agent: &'a SubAgent,
    pub(crate) now: i64,
    pub(crate) all_steps: bool,
    pub(crate) brief_open: bool,
}

fn heading(title: &str) -> Div {
    div().mb(px(6.)).text_size(px(11.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_4).child(title.to_string())
}

fn fact_row(f: Fact) -> Div {
    let bad = f.bad.map(|b| div().flex().child(div().text_color(TEXT_4).child("\u{a0}·\u{a0}")).child(div().text_color(FAILED_TEXT).child(b)));
    div()
        .flex()
        .child(div().w(px(LABEL_W)).flex_none().text_color(TEXT_4).child(f.label))
        .child(div().min_w_0().flex().text_color(f.color).child(f.value).children(bad))
}

/// A step on the details' rail: a call, opening to its output, error or diff as in the chat, beside when into the run
/// it started, or a note the sub-agent wrote between calls.
fn step_row(b: &Block, start: i64, tools: &Entity<Tools>, cx: &App) -> Div {
    let Block::Tool(c) = b else {
        let text = if let Block::Text(m) = b { m.text.trim().to_string() } else { String::new() };
        let dot = div().absolute().left(px(4.)).top(px(11.)).size(px(5.)).rounded_full().bg(TEXT_5);
        return div()
            .relative()
            .pl(px(20.))
            .py(px(4.))
            .whitespace_normal()
            .text_size(px(13.))
            .line_height(px(19.))
            .text_color(TEXT_BODY)
            .child(dot)
            .child(text);
    };
    let failed = c.status == ToolStatus::Failed;
    let dot = div()
        .absolute()
        .left(px(3.))
        .top(px(11.))
        .size(px(7.))
        .rounded_full()
        .border_1()
        .map(|d| if failed { d.border_color(FAILED).bg(FAILED) } else { d.border_color(TEXT_5).bg(window_fill()) });
    div()
        .relative()
        .pl(px(20.))
        .flex()
        .items_start()
        .gap(px(8.))
        .text_size(px(13.))
        .child(dot)
        .child(div().flex_1().min_w_0().child(tool_row::row(c, false, tools, cx)))
        .child(div().flex_none().pt(px(6.)).text_size(px(11.)).text_color(TEXT_5).child(format_elapsed((c.at - start).max(0) / 1000)))
}

/// The details: a head with `lead` before the mark and `actions` after the name, then the facts, the newest steps,
/// and the report or error. `on_brief` shows the whole brief above the facts, or hides it again; `on_more` shows every step, or
/// only the newest again.
#[expect(clippy::too_many_arguments, reason = "a view helper takes each part it draws")]
pub(crate) fn view(
    d: Detail,
    lead: Option<AnyElement>,
    actions: Vec<AnyElement>,
    on_brief: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    on_more: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
    tools: &Entity<Tools>,
    links: &markdown::Open,
    cx: &mut App,
) -> Div {
    let Detail { key, call, agent, now, all_steps, brief_open } = d;
    let m = sub_agent::mark(call, agent);
    let brief = sub_agent::brief(agent).map(|b| {
        div()
            .id(SharedString::from(format!("{key}-brief")))
            .mt(px(2.))
            .whitespace_normal()
            .text_size(px(12.5))
            .line_height(px(18.))
            .text_color(TEXT_3)
            .cursor_pointer()
            .hover(|s| s.text_color(TEXT_2))
            .line_clamp(2)
            .on_click(on_brief)
            .child(b.to_string())
    });
    let title = div()
        .flex_1()
        .min_w_0()
        .child(div().truncate().text_size(px(14.)).line_height(px(20.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(sub_agent::name(agent)))
        .children(brief);
    let head = div()
        .flex_none()
        .flex()
        .items_start()
        .gap(px(10.))
        .pt(px(12.))
        .pb(px(12.))
        .pl(px(16.))
        .pr(px(10.))
        .border_b(px(0.5))
        .border_color(SEPARATOR)
        .children(lead)
        .child(div().mt(px(3.)).child(sub_agent::glyph(&format!("{key}-{}", call.id), m)))
        .child(title)
        .child(div().flex_none().flex().gap(px(2.)).children(actions));

    let local = |ms: i64| format_timestamp(ms, &chrono::Local);
    let facts = div().flex().flex_col().gap(px(7.)).text_size(px(13.)).line_height(px(19.)).children(facts(call, agent, now, local).into_iter().map(fact_row));

    let all = sub_agent::steps(agent);
    let first = first_shown(all.len(), all_steps);
    let steps = (!all.is_empty()).then(|| {
        let rail = div().absolute().left(px(6.)).top(px(4.)).bottom(px(4.)).w(px(1.)).bg(RAIL);
        let more = (all.len() > RECENT).then(|| {
            div().flex().child(
                div()
                    .id(SharedString::from(format!("{key}-more")))
                    .mt(px(4.))
                    .ml(px(20.))
                    .text_size(px(12.))
                    .text_color(TEXT_3)
                    .cursor_pointer()
                    .hover(|s| s.text_color(TEXT))
                    .on_click(on_more)
                    .child(if all_steps { "Show fewer".to_string() } else { format!("Show {} more", first) }),
            )
        });
        div()
            .mt(px(18.))
            .child(heading("Steps"))
            .child(div().relative().flex().flex_col().child(rail).children(all[first..].iter().map(|b| step_row(b, call.at, tools, cx))))
            .children(more)
    });

    let report = match m {
        Mark::Starting | Mark::Running => Some(
            div()
                .flex()
                .text_size(px(13.))
                .text_color(TEXT_4)
                .child(shimmer(SharedString::from(format!("{key}-wait-{}", call.id)), "Working", TEXT_4))
                .child("\u{a0}· the report lands here when it finishes"),
        ),
        Mark::Failed => sub_agent::report(call, agent)
            .map(|r| div().whitespace_normal().font_family(code_font()).text_size(px(12.)).line_height(px(19.)).text_color(FAILED_TEXT).child(r)),
        Mark::Done => sub_agent::report(call, agent).map(|r| {
            let doc = tools.update(cx, |t, cx| t.docs.get(&call.id, &r, false, cx));
            div().child(markdown::block(TextView::new(&doc), links.clone()).text_size(px(13.5)).line_height(px(21.)))
        }),
    };
    let report = report.map(|r| div().mt(px(18.)).child(heading(if m == Mark::Failed { "Error" } else { "Report" })).child(r));

    // One flex child: a scroll's height counts from its first child's top, so a first child's margin would cut off the end.
    let full = sub_agent::brief(agent)
        .filter(|_| brief_open)
        .map(|b| div().mb(px(18.)).whitespace_normal().text_size(px(13.)).line_height(px(20.)).text_color(TEXT_BODY).child(b.to_string()));
    let content = div().flex().flex_col().children(full).child(facts).children(steps).children(report);
    let body = div().id(SharedString::from(format!("{key}-body"))).flex_1().min_h_0().overflow_y_scroll().pt(px(14.)).pb(px(20.)).px(px(16.)).child(content);
    div().size_full().flex().flex_col().child(head).child(body)
}

/// A square button in the details' head.
pub(crate) fn action(id: String, glyph: &'static str, on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> AnyElement {
    div()
        .id(SharedString::from(id))
        .size(px(24.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(6.))
        .cursor_pointer()
        .hover(|s| s.bg(FILL_3))
        .active(|s| s.bg(FILL_4))
        .on_click(on_click)
        .child(icon(glyph, 13., TEXT_3))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use super::{Fact, facts, first_shown, took};
    use chat::{Block, SubAgent, ToolCall, ToolKind, ToolStatus, Transcript, Turn};

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

    fn sub(blocks: Vec<Block>, model: &str, tokens: u64) -> SubAgent {
        SubAgent {
            description: "Audit".into(),
            transcript: Transcript { turns: vec![Turn { id: "t".into(), prompt: None, blocks, end: None }] },
            model: model.into(),
            tokens,
        }
    }

    fn call(status: ToolStatus, duration_ms: Option<u64>) -> ToolCall {
        ToolCall { id: "c".into(), name: "Task".into(), kind: ToolKind::Agent(SubAgent::default()), status, output: None, duration_ms, at: 1_000 }
    }

    fn shown(fs: &[Fact]) -> Vec<String> {
        fs.iter().map(|f| format!("{}: {}{}", f.label, f.value, f.bad.as_ref().map(|b| format!(" · {b}")).unwrap_or_default())).collect()
    }

    #[test]
    fn the_facts_say_where_it_stands_what_it_ran_on_and_how_far_it_got() {
        let a = sub(vec![tool("a", ToolStatus::Ok), tool("b", ToolStatus::Failed)], "claude-haiku-4-5", 32_900);
        let fs = facts(&call(ToolStatus::Ok, Some(74_000)), &a, 0, |_| "14:02".into());
        let model = agents::model_name("claude-haiku-4-5");
        assert_eq!(
            shown(&fs),
            [
                "Status: Done".to_string(),
                format!("Model: {model}"),
                "Started: 14:02".into(),
                "Duration: 1m 14s".into(),
                "Steps: 2 · 1 failed".into(),
                "Tokens: 32.9k".into()
            ]
        );
    }

    #[test]
    fn a_fact_not_known_yet_is_left_out() {
        let quiet = sub(vec![], "", 0);
        assert_eq!(shown(&facts(&call(ToolStatus::Ok, None), &quiet, 0, |_| "9:00".into())), ["Status: Done", "Started: 9:00", "Steps: 0"]);
        assert_eq!(
            shown(&facts(&call(ToolStatus::Running, None), &quiet, 16_000, |_| "9:00".into())),
            ["Status: Working", "Started: 9:00", "Duration: 15s", "Steps: 0"]
        );
    }

    #[test]
    fn a_finished_run_without_a_duration_shows_none_rather_than_zero() {
        assert_eq!(took(&call(ToolStatus::Ok, None), 99_000), None);
        assert_eq!(took(&call(ToolStatus::Failed, Some(27_000)), 0).as_deref(), Some("27s"));
    }

    #[test]
    fn the_details_show_the_newest_four_steps_until_asked_for_all() {
        assert_eq!(first_shown(4, false), 0);
        assert_eq!(first_shown(5, false), 1);
        assert_eq!(first_shown(5, true), 0);
        assert_eq!(first_shown(0, false), 0);
    }
}
