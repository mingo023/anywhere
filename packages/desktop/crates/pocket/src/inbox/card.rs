use super::{Body, Card, Inbox, InboxEvent, Replying};
use crate::desktop::chrome::id;
use crate::util::ago;
use agents::{Decision, Permission};
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

const FOLDED_LINES: usize = 3;
const DIFF_LINES: usize = 6;

#[derive(Clone, Copy, PartialEq)]
pub(super) enum Btn {
    Primary,
    Secondary,
    Ghost,
}

pub(super) fn btn(id: impl Into<ElementId>, kind: Btn, label: impl Into<SharedString>) -> Stateful<Div> {
    let b = div()
        .id(id)
        .h(px(28.))
        .px(px(if kind == Btn::Ghost { 8. } else { 11. }))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .rounded(px(8.))
        .cursor_pointer()
        .text_size(px(12.5))
        .font_weight(FontWeight::SEMIBOLD)
        .whitespace_nowrap()
        .active(|s| s.opacity(0.85))
        .child(label.into());
    match kind {
        Btn::Primary => b.bg(PRIMARY).text_color(ON_PRIMARY).hover(|s| s.opacity(0.9)),
        Btn::Secondary => b.bg(FILL_3).text_color(TEXT).hover(|s| s.bg(FILL_4)),
        Btn::Ghost => b.text_color(TEXT_2).hover(|s| s.bg(FILL_2).text_color(TEXT)),
    }
}

fn hint(text: &'static str) -> Div {
    div().text_size(px(12.)).text_color(TEXT_3).child(text)
}

fn field() -> Div {
    div().h(px(34.)).pl(px(11.)).pr(px(4.)).flex().items_center().gap(px(6.)).rounded(px(9.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
}

pub(super) fn reply_field(input: &Entity<InputState>, cx: &mut Context<Inbox>) -> Div {
    let send = div()
        .id("inbox-send")
        .size(px(26.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded(px(13.))
        .cursor_pointer()
        .bg(PRIMARY)
        .hover(|s| s.opacity(0.9))
        .child(icon("arrow-up", 13., ON_PRIMARY))
        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.send_reply(window, cx)));
    field().child(div().flex_1().min_w_0().text_size(px(13.)).child(Input::new(input).appearance(false).p_0().text_size(px(13.)))).child(send)
}

/// Observe-only asks are answered in the session.
pub(super) fn in_session(agent: &str, label: &'static str, cx: &mut Context<Inbox>) -> Div {
    let agent = agent.to_string();
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .child(
            btn(id(format!("inbox-in-session:{agent}")), Btn::Secondary, label)
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(InboxEvent::Open(agent.clone())))),
        )
        .child(hint("Answer it in the session"))
}

fn code_block() -> Div {
    div().px(px(10.)).py(px(8.)).rounded(px(8.)).bg(SURFACE_SUNKEN).font_family(MONO).text_size(px(11.5)).line_height(px(16.)).text_color(TEXT)
}

fn settled(line: &str) -> Div {
    div()
        .min_h(px(20.))
        .flex()
        .items_center()
        .gap(px(8.))
        .text_size(px(12.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(SUCCESS_TEXT)
        .child(icon("check", 14., SUCCESS_TEXT))
        .child(div().min_w_0().child(line.to_string()))
}

fn asked_text(p: &Permission, what: String) -> String {
    match (&p.detail.input, p.detail.kind.as_str()) {
        (input, "other") if !input.is_null() => serde_json::to_string_pretty(input).unwrap_or(what),
        _ => what,
    }
}

impl Inbox {
    pub(super) fn card_view(&self, c: &Card, now: i64, cx: &mut Context<Self>) -> Stateful<Div> {
        let agent = c.agent.clone();
        let meta = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .min_w_0()
            .text_size(px(12.))
            .text_color(TEXT_2)
            .child(ui::repo_tile(&c.mark, 20., false, None))
            .child(div().truncate().text_size(px(13.)).text_color(TEXT).child(c.project.clone()))
            .child(div().text_color(TEXT_6).child("·"))
            .child(
                div()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .child(provider_icon(&c.provider, 12., TEXT_2))
                    .child(div().truncate().child(c.agent_name.clone())),
            )
            .child(div().ml_auto().flex_none().text_color(TEXT_4).child(ago(c.at, now)));
        let open = ui::icon_button_sized(id(format!("inbox-open:{agent}")), "external", 24., TEXT_2)
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(InboxEvent::Open(agent.clone()))));
        let title = div()
            .mt(px(3.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(div().flex_1().min_w_0().truncate().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(c.title.clone()))
            .child(open);
        let via = c.delegated_by.clone().map(|parent| {
            div()
                .mt(px(2.))
                .flex()
                .min_w_0()
                .items_center()
                .gap(px(4.))
                .text_size(px(12.))
                .text_color(TEXT_3)
                .child(icon("corner-down-right", 13., TEXT_4))
                .child("Delegated by")
                .child(div().truncate().font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(parent))
        });
        let body = match self.answers.settled(c) {
            Some(line) => settled(line),
            None => self.ask_box(c, cx),
        };
        div()
            .id(id(format!("inbox-card:{}", c.agent)))
            .p(px(12.))
            .rounded(px(12.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(SEPARATOR, 0.5), ui::shadow(rgba(0x0000000a), 1., 2.)])
            .child(meta)
            .child(title)
            .children(via)
            .child(div().mt(px(8.)).child(body))
    }

    fn ask_box(&self, c: &Card, cx: &mut Context<Self>) -> Div {
        match &c.body {
            Body::Asks(asks) => match self.answers.shown(asks) {
                Some((_, p)) if !p.questions().is_empty() => self.question_box(&c.agent, p, cx),
                Some((i, p)) => self.permission_box(c, i, asks.len(), p, cx),
                None => div(),
            },
            Body::Said { text, chat: true } => self.chat_box(c, text, cx),
            Body::Said { text, chat: false } => {
                let agent = c.agent.clone();
                let open = btn(id(format!("inbox-terminal:{agent}")), Btn::Secondary, "Open terminal")
                    .child(icon("terminal", 13., TEXT))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(InboxEvent::Open(agent.clone()))));
                div()
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .when(!text.is_empty(), |d| d.child(code_block().child(text.clone())))
                    .child(div().flex().items_center().gap(px(6.)).child(open).child(hint("Answer it in the terminal")))
            }
            Body::Failed { error, retry } => self.failed_box(c, error, *retry, cx),
            Body::Done => div(),
        }
    }

    fn chat_box(&self, c: &Card, text: &str, cx: &mut Context<Self>) -> Div {
        let said = div().text_size(px(13.)).line_height(px(19.)).text_color(TEXT).child(text.to_string());
        let answer = if self.observe_only {
            in_session(&c.agent, "Open session", cx)
        } else if self.replying == Some(Replying::Prompt(c.agent.clone())) {
            reply_field(&self.reply, cx)
        } else {
            let (agent, placeholder) = (c.agent.clone(), format!("Reply to {}…", c.agent_name));
            div().child(
                field()
                    .id(id(format!("inbox-reply:{agent}")))
                    .cursor_text()
                    .child(div().flex_1().truncate().text_size(px(13.)).text_color(TEXT_4).child(placeholder.clone()))
                    .on_click(
                        cx.listener(move |this, _: &ClickEvent, window, cx| this.start_reply(Replying::Prompt(agent.clone()), placeholder.clone(), window, cx)),
                    ),
            )
        };
        div().flex().flex_col().gap(px(10.)).when(!text.is_empty(), |d| d.child(said)).child(answer)
    }

    fn failed_box(&self, c: &Card, error: &str, retry: bool, cx: &mut Context<Self>) -> Div {
        let line = div().text_size(px(13.)).line_height(px(19.)).text_color(TEXT).child(
            div()
                .flex()
                .flex_wrap()
                .child(div().font_weight(FontWeight::SEMIBOLD).text_color(FAILED_TEXT).child("Turn failed.\u{a0}"))
                .child(div().min_w_0().child(error.to_string())),
        );
        let (a, b) = (c.agent.clone(), c.agent.clone());
        let retry_btn = (retry && !self.observe_only)
            .then(|| btn(id(format!("inbox-retry:{a}")), Btn::Primary, "Retry").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.retry(&a, cx))));
        let dismiss = btn(id(format!("inbox-dismiss:{b}")), Btn::Secondary, "Dismiss")
            .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(InboxEvent::Seen(vec![b.clone()]))));
        div()
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(line)
            .child(div().flex().items_center().gap(px(6.)).children(retry_btn).child(dismiss).when(!retry, |d| d.child(hint("No prompt to retry"))))
    }

    fn permission_box(&self, c: &Card, i: usize, n: usize, p: &Permission, cx: &mut Context<Self>) -> Div {
        let (verb, what) = p.parts();
        let request = p.request_id.clone();
        let lead = div()
            .flex()
            .items_start()
            .gap(px(8.))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .flex_wrap()
                    .text_size(px(13.))
                    .line_height(px(18.))
                    .text_color(TEXT)
                    .child(div().min_w_0().font_weight(FontWeight::SEMIBOLD).child(format!("{}\u{a0}", p.tool_name)))
                    .child(div().min_w_0().child(verb.replacen("Wants", "wants", 1))),
            )
            .when(n > 1, |d| d.child(div().flex_none().pt(px(2.)).font_family(MONO).text_size(px(11.)).text_color(TEXT_2).child(format!("{} of {n}", i + 1))));
        let expanded = self.answers.expanded(&request);
        let more = |lines: usize, cx: &mut Context<Self>| {
            let request = request.clone();
            div()
                .id(id(format!("inbox-more:{request}")))
                .mt(px(6.))
                .text_size(px(12.))
                .text_color(TEXT_2)
                .cursor_pointer()
                .underline()
                .hover(|s| s.text_color(TEXT))
                .child(format!("Show all {lines} lines"))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.expand(&request, cx)))
        };
        let code = match &p.detail.diff {
            Some(diff) => {
                let path = code_block()
                    .flex()
                    .gap(px(8.))
                    .child(div().flex_1().min_w_0().truncate().child(what))
                    .child(div().flex_none().text_color(SUCCESS_TEXT).child(format!("+{}", diff.additions)))
                    .child(div().flex_none().text_color(FAILED).child(format!("−{}", diff.deletions)));
                let shown = if expanded { diff.lines.len() } else { DIFF_LINES };
                let lines = diff.lines.iter().take(shown).map(|l| {
                    let (sign, color) = match l.kind.as_str() {
                        "add" => ("+", DIFF_ADD_TEXT),
                        "del" => ("−", DIFF_DEL_TEXT),
                        _ => (" ", TEXT_3),
                    };
                    div().truncate().text_color(color).child(format!("{sign} {}", l.text))
                });
                div()
                    .mt(px(6.))
                    .flex()
                    .flex_col()
                    .gap(px(4.))
                    .child(path)
                    .when(!diff.lines.is_empty(), |d| d.child(code_block().py(px(6.)).children(lines)))
                    .when(diff.lines.len() > shown, |d| d.child(more(diff.lines.len(), cx)))
            }
            None => {
                let text = asked_text(p, what);
                let lines = text.lines().count();
                let folded = lines > FOLDED_LINES && !expanded;
                let wraps = matches!(p.detail.kind.as_str(), "shell" | "other");
                div()
                    .mt(px(6.))
                    .child(code_block().when(!wraps, |d| d.truncate()).when(folded, |d| d.max_h(px(56.)).overflow_hidden()).child(text))
                    .when(folded, |d| d.child(more(lines, cx)))
            }
        };
        let answer = if self.observe_only {
            in_session(&c.agent, "Open session", cx)
        } else if self.replying == Some(Replying::Deny(request.clone())) {
            let cancel = div()
                .id("inbox-reply-cancel")
                .text_size(px(12.))
                .text_color(TEXT_2)
                .cursor_pointer()
                .underline()
                .hover(|s| s.text_color(TEXT))
                .child("Cancel")
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| _ = this.cancel_reply(cx)));
            div().flex().flex_col().gap(px(6.)).child(reply_field(&self.reply, cx)).child(div().flex().child(cancel))
        } else {
            let (deny, allow, reply) = (request.clone(), request.clone(), request.clone());
            let placeholder = format!("Tell {} what to do instead…", c.agent_name);
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(
                    btn(id(format!("inbox-deny:{request}")), Btn::Ghost, "Deny")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.decide(&deny, Decision::Deny, cx))),
                )
                .when(p.feedback, |d| {
                    d.child(btn(id(format!("inbox-deny-reply:{request}")), Btn::Ghost, "Reply").on_click(
                        cx.listener(move |this, _: &ClickEvent, window, cx| this.start_reply(Replying::Deny(reply.clone()), placeholder.clone(), window, cx)),
                    ))
                })
                .child(div().flex_1())
                .child(
                    btn(id(format!("inbox-allow:{request}")), Btn::Primary, "Allow once")
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.decide(&allow, Decision::Allow, cx))),
                )
        };
        div().flex().flex_col().child(lead).child(code).child(div().mt(px(10.)).child(answer))
    }
}

#[cfg(test)]
mod tests {
    use super::asked_text;
    use agents::{Detail, Permission};
    use serde_json::json;

    #[test]
    fn an_other_tool_shows_its_whole_input_and_the_rest_what_they_act_on() {
        let mcp = Permission { detail: Detail { kind: "other".into(), input: json!({"sql": "select 1"}), ..Default::default() }, ..Default::default() };
        assert_eq!(asked_text(&mcp, "mcp__db__query".into()), "{\n  \"sql\": \"select 1\"\n}");
        let shell = Permission { detail: Detail { kind: "shell".into(), command: "ls".into(), ..Default::default() }, ..Default::default() };
        assert_eq!(asked_text(&shell, "ls".into()), "ls");
    }
}
