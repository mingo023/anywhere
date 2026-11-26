use super::composer::{ChatBoxEvent, ComposerState, RADIUS};
use crate::desktop::chrome::id;
use chat::{Ask, Command, ToolKind};
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

const REPLY_PLACEHOLDER: &str = "Tell the agent what to do instead";

/// The tool's icon, its verb and what it acts on.
pub(super) fn subject<'a>(tool: &'a ToolKind, tool_name: &'a str) -> (&'static str, &'static str, &'a str) {
    match tool {
        ToolKind::Shell { command, .. } => ("terminal", "Run", command),
        ToolKind::Read { path } => ("file", "Read", path),
        ToolKind::Edit { path, .. } => ("pencil", "Edit", path),
        ToolKind::Write { path, .. } => ("file-plus", "Write", path),
        ToolKind::Search { query } => ("search", "Search", query),
        ToolKind::Fetch { url } => ("globe", "Fetch", url),
        ToolKind::Agent(a) => ("bot", "Start agent", &a.description),
        ToolKind::Workflow(w) => ("bot", "Run workflow", &w.name),
        ToolKind::Question(_) | ToolKind::Other { .. } => ("blocks", "Use", tool_name),
    }
}

impl ComposerState {
    fn resolve(&mut self, agent: &str, request: &str, allow: bool, cx: &mut Context<Self>) {
        self.replying = None;
        let command = if allow { Command::Allow(request.to_string()) } else { Command::Deny(request.to_string()) };
        cx.emit(ChatBoxEvent::Command { agent: agent.to_string(), command });
        cx.notify();
    }

    fn start_reply(&mut self, request: String, window: &mut Window, cx: &mut Context<Self>) {
        self.replying = Some(request);
        self.answer.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.set_placeholder(REPLY_PLACEHOLDER, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    /// Enter allows, Esc denies or closes the reply, R starts one; while replying, keys belong to the reply.
    fn permission_key(&mut self, agent: &str, request: &str, feedback: bool, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if e.keystroke.modifiers.modified() {
            return;
        }
        let replying = self.replying.is_some();
        match e.keystroke.key.as_str() {
            "escape" if replying => {
                self.replying = None;
                self.ask_focus.clone().focus(window, cx);
                cx.notify();
            }
            _ if replying => return,
            "enter" => self.resolve(agent, request, true, cx),
            "escape" => self.resolve(agent, request, false, cx),
            "r" if feedback => self.start_reply(request.to_string(), window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// Denies the ask with the typed reply, which pocketd hands the agent as its next prompt.
    pub(super) fn reply(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.answer.read(cx).value().trim().to_string();
        let Some(request) = self.replying.clone() else { return };
        if text.is_empty() {
            return;
        }
        self.replying = None;
        self.answer.update(cx, |s, cx| s.set_value("", window, cx));
        cx.emit(ChatBoxEvent::Command { agent: agent.to_string(), command: Command::Reply { request, text } });
        cx.notify();
    }

    pub(super) fn permission_card(&self, agent: &str, name: &str, ask: &Ask, feedback: bool, answerable: bool, cx: &mut Context<Self>) -> Div {
        let (glyph, verb, what) = subject(&ask.tool, &ask.tool_name);
        let replying = answerable && self.replying.as_deref() == Some(ask.request_id.as_str());
        let header = div()
            .flex()
            .items_center()
            .gap(px(9.))
            .text_size(px(14.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT)
            .child(ui::dot(8., WAITING).shadow(vec![ui::ring(WAITING_BG, 3.)]))
            .child(format!("Allow {}?", ask.tool_name))
            .child(
                div()
                    .min_w_0()
                    .truncate()
                    .text_size(px(12.5))
                    .font_weight(FontWeight::NORMAL)
                    .text_color(TEXT_3)
                    .child(format!("{name} is paused until you answer")),
            );
        let body = div()
            .flex()
            .items_start()
            .gap(px(8.))
            .py(px(10.))
            .px(px(12.))
            .rounded(px(8.))
            .bg(FILL_2)
            .child(div().flex_none().pt(px(2.)).child(icon(glyph, 14., TEXT_2)))
            .child(div().flex_none().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(verb))
            .child(div().flex_1().min_w_0().max_h(px(120.)).overflow_hidden().font_family(MONO).text_size(px(13.)).text_color(TEXT).child(what.to_string()));
        let reply = replying.then(|| div().mx(px(-6.)).text_size(px(14.)).child(Textarea::new(&self.answer).appearance(false)));
        let (keys, key_request) = (agent.to_string(), ask.request_id.clone());
        let footer = answerable.then(|| {
            let (deny, allow, send) = ((agent.to_string(), ask.request_id.clone()), (agent.to_string(), ask.request_id.clone()), agent.to_string());
            let request = ask.request_id.clone();
            let button = |key: &str, v, label: &'static str| {
                ui::button(id(format!("chat-permission-{key}:{agent}")), v, None, label).h(px(30.)).px(px(14.)).rounded(px(15.))
            };
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(div().flex_1())
                .child(
                    button("deny", Variant::Ghost, "Deny")
                        .text_color(FAILED)
                        .child(ui::button_kbd("esc"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.resolve(&deny.0, &deny.1, false, cx))),
                )
                .when(feedback && !replying, |el| {
                    el.child(
                        button("reply", Variant::Ghost, "Reply")
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_reply(request.clone(), window, cx))),
                    )
                })
                .when(replying, |el| {
                    el.child(
                        button("send-reply", Variant::Secondary, "Send reply")
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.reply(&send, window, cx))),
                    )
                })
                .child(
                    button("allow", Variant::Primary, "Allow once")
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(ui::button_kbd("⏎"))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.resolve(&allow.0, &allow.1, true, cx))),
                )
        });
        div()
            .when(answerable, |el| {
                el.track_focus(&self.ask_focus)
                    .on_key_down(cx.listener(move |this, e: &KeyDownEvent, window, cx| this.permission_key(&keys, &key_request, feedback, e, window, cx)))
            })
            .flex()
            .flex_col()
            .gap(px(12.))
            .p(px(16.))
            .rounded(px(RADIUS))
            .border_1()
            .border_color(Hsla::from(WAITING).opacity(0.35))
            .bg(SURFACE)
            .shadow(vec![ui::shadow(SHADOW, 8., 24.)])
            .child(header)
            .child(body)
            .children(reply)
            .children(footer)
    }
}

#[cfg(test)]
mod tests {
    use super::subject;
    use chat::ToolKind;

    #[test]
    fn a_permission_card_names_its_tool_and_path() {
        let edit = ToolKind::Edit { path: "src/lib.rs".into(), diff: None };
        assert_eq!(subject(&edit, "Edit"), ("pencil", "Edit", "src/lib.rs"));
        let shell = ToolKind::Shell { command: "rm -rf build".into(), description: String::new() };
        assert_eq!(subject(&shell, "Bash"), ("terminal", "Run", "rm -rf build"));
        assert_eq!(subject(&ToolKind::Other { input: "{}".into(), description: String::new() }, "mcp__db__query"), ("blocks", "Use", "mcp__db__query"));
    }
}
