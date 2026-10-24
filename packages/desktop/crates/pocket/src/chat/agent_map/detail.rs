use super::{CARD, calls, close_button, mark, name, ran_ms, thousands};
use crate::chat::meta::format_timestamp;
use crate::chat::sub_agent::{self, Mark};
use crate::chat::tool_row::{self, Arg};
use crate::chat::working::format_elapsed;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use chat::{Block, Spawned, ToolCall};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Instant;
use theme::*;

/// Lines of the prompt shown until "Show more".
const PROMPT_LINES: f32 = 3.;
const PROMPT_LINE: f32 = 19.;

/// A step's tool and what it acted on: "Read" and a path, or the tool's own name and its command.
fn step(call: &ToolCall) -> (String, String) {
    let (verb, Arg::File(arg) | Arg::Text(arg) | Arg::Raw(arg)) = tool_row::line(call);
    (if verb.is_empty() { call.name.clone() } else { verb.to_string() }, arg)
}

/// "Working · Haiku 5.5", what the sub-agent is and where it stands.
fn facts(m: Mark, model: &str) -> String {
    let state = match m {
        Mark::Starting | Mark::Running => "Working",
        Mark::Done => "Done",
        Mark::Failed => "Failed",
    };
    if model.is_empty() { state.into() } else { format!("{state} · {}", agents::model_name(model)) }
}

fn section(title: &str, count: Option<usize>) -> Div {
    div()
        .mt(px(16.))
        .mb(px(6.))
        .flex()
        .gap(px(6.))
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT_2)
        .child(title.to_string())
        .children(count.map(|n| div().font_weight(FontWeight::MEDIUM).text_color(TEXT_4).child(n.to_string())))
}

fn stat(value: String, label: &str) -> Div {
    div()
        .flex_1()
        .px(px(12.))
        .py(px(8.))
        .rounded(px(10.))
        .bg(FILL_2)
        .child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(value))
        .child(div().text_size(px(11.)).text_color(TEXT_3).child(label.to_string()))
}

fn button(key: String, label: &str) -> Stateful<Div> {
    div().id(id(key)).h(px(30.)).px(px(12.)).flex().items_center().rounded(px(9.)).bg(FILL_3).hover(|s| s.bg(FILL_4)).cursor_pointer().text_size(px(12.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(label.to_string())
}

impl Desktop {
    /// The details of `map[ix]`: what it is, what it took, its prompt, its steps and its report. `live` takes clicks, as
    /// the details do while they're the page showing.
    pub(super) fn chat_map_detail(&self, agent: &str, map: &[Spawned], ix: usize, live: bool, now: i64, cx: &mut Context<Self>) -> Div {
        let s = &map[ix];
        let (call, sub) = (s.call, s.agent);
        let m = mark(s);
        let running = matches!(m, Mark::Running | Mark::Starting);
        let local = |ms: i64| format_timestamp(ms, &chrono::Local);
        let by = s.parent.map_or_else(|| "the main agent".to_string(), |p| name(&map[p]));
        let ended = (!running).then(|| format!(" · ended {}", local(call.at + ran_ms(call, now))));
        let spawned = format!("Spawned by {by} at {}{}", local(call.at), ended.unwrap_or_default());

        let back = div()
            .id(id(format!("chat-map-back:{agent}")))
            .size(px(26.))
            .flex_none()
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_3))
            .child(icon("back", 14., TEXT_2))
            .when(live, |d| {
                d.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.chats.map.nav.back();
                    cx.notify();
                }))
            });
        let close = close_button(format!("chat-map-close2:{agent}")).when(live, |d| {
            d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.chats.map.nav.close(Instant::now());
                this.chats.input.update(cx, |s, cx| s.focus(window, cx));
                cx.notify();
            }))
        });
        let head = div()
            .flex_none()
            .flex()
            .items_start()
            .gap(px(8.))
            .pt(px(14.))
            .pl(px(12.))
            .pr(px(10.))
            .child(back)
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .child(
                        div()
                            .mt(px(3.))
                            .flex()
                            .items_center()
                            .gap(px(8.))
                            .child(sub_agent::glyph(&call.id, m))
                            .child(div().min_w_0().truncate().text_size(px(15.)).font_weight(FontWeight::BOLD).text_color(if m == Mark::Failed { FAILED } else { TEXT }).child(name(s))),
                    )
                    .child(div().mt(px(6.)).text_size(px(12.5)).line_height(px(20.)).text_color(TEXT_2).child(facts(m, &sub.model)))
                    .child(div().text_size(px(12.5)).line_height(px(20.)).text_color(TEXT_3).child(spawned)),
            )
            .child(close);
        let tokens = if sub.tokens == 0 { "—".to_string() } else { thousands(sub.tokens) };
        let stats = div()
            .flex_none()
            .flex()
            .gap(px(8.))
            .my(px(12.))
            .ml(px(46.))
            .mr(px(14.))
            .child(stat(format_elapsed(ran_ms(call, now) / 1000), if running { "so far" } else { "ran for" }))
            .child(stat(tokens, "tokens"))
            .child(stat(calls(sub).to_string(), "tool calls"));

        let now_on = running.then(|| sub_agent::latest(sub)).flatten().map(|c| {
            let (tool, arg) = step(c);
            div()
                .mt(px(14.))
                .px(px(12.))
                .py(px(8.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(10.))
                .bg(ACCENT_BG)
                .text_size(px(12.5))
                .text_color(ACCENT)
                .child(sub_agent::glyph(&format!("{}-now", call.id), Mark::Running))
                .child("Now")
                .child(div().font_weight(FontWeight::SEMIBOLD).child(tool))
                .child(div().min_w_0().truncate().font_family(code_font()).text_size(px(12.)).text_color(TEXT_2).child(arg))
        });

        let prompt = sub_agent::brief(sub).map(|brief| {
            let open = self.chats.map.nav.prompt_open;
            let long = sub_agent::lines(brief) as f32 > PROMPT_LINES;
            let text = div().whitespace_normal().text_size(px(12.5)).line_height(px(PROMPT_LINE)).text_color(TEXT_2).child(brief.to_string());
            let more = long.then(|| {
                div()
                    .id(id(format!("chat-map-prompt:{agent}")))
                    .pt(px(3.))
                    .text_size(px(12.))
                    .text_color(TEXT_3)
                    .cursor_pointer()
                    .hover(|s| s.text_color(TEXT))
                    .child(if open { "Show less" } else { "Show more" })
                    .when(live, |d| {
                        d.on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                            this.chats.map.nav.prompt_open ^= true;
                            cx.notify();
                        }))
                    })
            });
            div().child(section("Prompt", None)).child(div().when(long && !open, |d| d.max_h(px(PROMPT_LINE * PROMPT_LINES)).overflow_hidden()).child(text)).children(more)
        });

        let blocks = sub_agent::steps(sub);
        let last = blocks.len().saturating_sub(1);
        let rows = blocks.iter().enumerate().map(|(i, b)| {
            let current = running && i == last;
            let dot = div().absolute().left(px(4.)).top(px(11.)).size(px(5.)).rounded(px(3.)).bg(if current { ACCENT } else { TEXT_5 });
            let rail = (i < last).then(|| div().absolute().left(px(6.)).top(px(19.)).bottom(px(-11.)).w(px(1.)).bg(RAIL));
            let row = div().relative().flex().items_baseline().gap(px(8.)).py(px(4.)).pl(px(18.)).text_size(px(13.)).line_height(px(19.)).child(dot).children(rail);
            match b {
                Block::Tool(c) => {
                    let (tool, arg) = step(c);
                    row.child(div().flex_none().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(tool))
                        .child(div().flex_1().min_w_0().truncate().font_family(code_font()).text_size(px(12.)).text_color(TEXT_2).child(arg))
                        .child(div().flex_none().font_family(code_font()).text_size(px(11.)).text_color(TEXT_4).child(format_elapsed((c.at - call.at) / 1000)))
                }
                Block::Text(t) => row.child(div().flex_1().min_w_0().truncate().text_size(px(12.5)).text_color(TEXT_2).child(t.text.trim().to_string())),
                _ => row,
            }
        });
        let steps = div().child(section("Steps", Some(calls(sub)))).children(rows);

        let report = (!running).then(|| sub_agent::report(call, sub)).flatten().map(|r| {
            let failed = m == Mark::Failed;
            let body = div().px(px(12.)).py(px(10.)).rounded(px(10.)).whitespace_normal().text_size(px(13.)).line_height(px(20.)).child(r);
            let body = if failed { body.bg(FAILED_BG).text_color(FAILED).font_family(code_font()).text_size(px(12.)) } else { body.bg(CARD).text_color(TEXT_BODY).shadow(vec![ui::ring(SEPARATOR, 0.5)]) };
            div().child(section(if failed { "Error" } else { "Report" }, None)).child(body)
        });

        // One flex child: a scroll's height counts from its first child's top, so a first child's margin would cut off the end.
        let content = div().flex().flex_col().children(now_on).children(prompt).child(steps).children(report);
        let body = div().id(id(format!("chat-map-detail-body:{agent}"))).flex_1().min_h_0().overflow_y_scroll().border_t(px(0.5)).border_color(HAIRLINE).pt(px(4.)).pb(px(8.)).pl(px(46.)).pr(px(18.)).child(content);
        let foot = div().flex_none().flex().gap(px(8.)).pt(px(10.)).pb(px(14.)).pl(px(46.)).pr(px(14.)).border_t(px(0.5)).border_color(HAIRLINE).child(button(format!("chat-map-reveal:{agent}"), "Show in chat").when(live, |d| {
            let agent = agent.to_string();
            d.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.chat_map_reveal(&agent, ix, window, cx)))
        }));
        div().absolute().top_0().bottom_0().w_full().flex().flex_col().child(head).child(stats).child(body).child(foot)
    }
}

#[cfg(test)]
mod tests {
    use super::{Mark, facts, step};
    use chat::{ToolCall, ToolKind, ToolStatus};

    fn call(name: &str, kind: ToolKind) -> ToolCall {
        ToolCall { id: "c".into(), name: name.into(), kind, status: ToolStatus::Ok, output: None, duration_ms: None, at: 0 }
    }

    #[test]
    fn a_step_names_its_tool_beside_what_it_acted_on() {
        assert_eq!(step(&call("Read", ToolKind::Read { path: "src/a.rs".into() })), ("Read".into(), "src/a.rs".into()));
        assert_eq!(step(&call("Bash", ToolKind::Shell { command: "cargo test\n".into(), description: String::new() })), ("Bash".into(), "cargo test".into()));
    }

    #[test]
    fn the_details_say_where_the_agent_stands_and_its_model() {
        assert_eq!(facts(Mark::Starting, ""), "Working");
        assert_eq!(facts(Mark::Failed, "claude-haiku-4-5"), format!("Failed · {}", agents::model_name("claude-haiku-4-5")));
    }
}
