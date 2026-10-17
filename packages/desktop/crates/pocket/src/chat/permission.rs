use super::composer::RADIUS;
use crate::desktop::Desktop;
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
        ToolKind::Shell { command } => ("terminal", "Run", command),
        ToolKind::Read { path } => ("file", "Read", path),
        ToolKind::Edit { path, .. } => ("pencil", "Edit", path),
        ToolKind::Write { path, .. } => ("file-plus", "Write", path),
        ToolKind::Search { query } => ("search", "Search", query),
        ToolKind::Fetch { url } => ("globe", "Fetch", url),
        ToolKind::Agent(a) => ("bot", "Start agent", &a.description),
        ToolKind::Question(_) | ToolKind::Other { .. } => ("blocks", "Use", tool_name),
    }
}

impl Desktop {
    fn chat_resolve(&mut self, terminal: &str, allow: bool, cx: &mut Context<Self>) {
        let (Some(a), Some((ask, _))) = (self.summary(terminal), self.chat_ask(terminal)) else { return };
        let agent = a.id.clone();
        self.chats.composer.replying = None;
        self.chat_command(&agent, if allow { Command::Allow(ask.request_id) } else { Command::Deny(ask.request_id) }, cx);
    }

    fn chat_start_reply(&mut self, request: String, window: &mut Window, cx: &mut Context<Self>) {
        self.chats.composer.replying = Some(request);
        self.chats.composer.answer.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.set_placeholder(REPLY_PLACEHOLDER, window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    /// Enter allows, Esc denies or closes the reply, R starts one; while replying, keys belong to the reply.
    fn permission_key(&mut self, terminal: &str, request: &str, feedback: bool, e: &KeyDownEvent, window: &mut Window, cx: &mut Context<Self>) {
        if e.keystroke.modifiers.modified() {
            return;
        }
        let replying = self.chats.composer.replying.is_some();
        match e.keystroke.key.as_str() {
            "escape" if replying => {
                self.chats.composer.replying = None;
                self.chats.composer.ask_focus.focus(window, cx);
                cx.notify();
            }
            _ if replying => return,
            "enter" => self.chat_resolve(terminal, true, cx),
            "escape" => self.chat_resolve(terminal, false, cx),
            "r" if feedback => self.chat_start_reply(request.to_string(), window, cx),
            _ => return,
        }
        cx.stop_propagation();
    }

    /// Denies the ask with the typed reply, which pocketd hands the agent as its next prompt.
    pub(super) fn chat_reply(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) {
        let text = self.chats.composer.answer.read(cx).value().trim().to_string();
        let (Some(a), Some(request)) = (self.summary(terminal), self.chats.composer.replying.clone()) else { return };
        if text.is_empty() {
            return;
        }
        let agent = a.id.clone();
        self.chats.composer.replying = None;
        self.chats.composer.answer.update(cx, |s, cx| s.set_value("", window, cx));
        self.chat_command(&agent, Command::Reply { request, text }, cx);
    }

    /// The ask in the pill's place: what the tool wants to do, and Deny / Reply / Allow when this client may answer.
    pub(super) fn permission_card(&self, terminal: &str, ask: &Ask, feedback: bool, answerable: bool, cx: &mut Context<Self>) -> Div {
        let (glyph, verb, what) = subject(&ask.tool, &ask.tool_name);
        let replying = answerable && self.chats.composer.replying.as_deref() == Some(ask.request_id.as_str());
        let name = self.summary(terminal).map(|a| self.agent_name(&a.provider)).unwrap_or_default();
        let header = div()
            .flex()
            .items_center()
            .gap(px(9.))
            .text_size(px(14.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT)
            .child(ui::dot(8., WAITING).shadow(vec![ui::ring(WAITING_BG, 3.)]))
            .child(format!("Allow {}?", ask.tool_name))
            .child(div().min_w_0().truncate().text_size(px(12.5)).font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(format!("{name} is paused until you answer")));
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
        let reply = replying.then(|| div().mx(px(-6.)).text_size(px(14.)).child(Textarea::new(&self.chats.composer.answer).appearance(false)));
        let t = terminal.to_string();
        let (keys, key_request) = (t.clone(), ask.request_id.clone());
        let footer = answerable.then(|| {
            let (deny, allow, send) = (t.clone(), t.clone(), t.clone());
            let request = ask.request_id.clone();
            let button = |key: &str, v, label: &'static str| ui::button(id(format!("chat-permission-{key}:{terminal}")), v, None, label).h(px(30.)).px(px(14.)).rounded(px(15.));
            div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(div().flex_1())
                .child(button("deny", Variant::Ghost, "Deny").text_color(FAILED).child(ui::button_kbd("esc")).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_resolve(&deny, false, cx))))
                .when(feedback && !replying, |el| {
                    el.child(button("reply", Variant::Ghost, "Reply").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.chat_start_reply(request.clone(), window, cx))))
                })
                .when(replying, |el| el.child(button("send-reply", Variant::Secondary, "Send reply").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.chat_reply(&send, window, cx)))))
                .child(button("allow", Variant::Primary, "Allow once").font_weight(FontWeight::SEMIBOLD).child(ui::button_kbd("⏎")).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_resolve(&allow, true, cx))))
        });
        div()
            .when(answerable, |el| {
                el.track_focus(&self.chats.composer.ask_focus)
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
        let shell = ToolKind::Shell { command: "rm -rf build".into() };
        assert_eq!(subject(&shell, "Bash"), ("terminal", "Run", "rm -rf build"));
        assert_eq!(subject(&ToolKind::Other { input: "{}".into() }, "mcp__db__query"), ("blocks", "Use", "mcp__db__query"));
    }
}
