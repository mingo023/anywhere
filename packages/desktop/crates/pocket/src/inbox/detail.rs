use super::{Inbox, InboxEvent};
use crate::desktop::Desktop;
use crate::desktop::chrome::drag_area;
use crate::status::Status;
use crate::util::{basename, now_ms};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, State, Variant, kbd};

type Key = Box<dyn Fn(&KeyDownEvent, &mut Window, &mut App)>;

/// What `Desktop` hands the inbox for the note on screen: what it draws that it can't read for itself.
pub(crate) struct Open {
    agent: String,
    project: String,
    mark: ui::Mark,
    title: String,
    /// Seconds the agent has needed you, while it does.
    needs_you: Option<i64>,
    chat: bool,
    pane: AnyElement,
    focus: FocusHandle,
    key: Key,
}

impl Inbox {
    /// The note on screen, or the line saying none is.
    pub(crate) fn detail(&mut self, open: Option<Open>, cx: &mut Context<Self>) -> Div {
        let Some(Open { agent, project, mark, title, needs_you, chat, pane, focus, key }) = open else {
            return drag_area(div()).flex_1().flex().items_center().justify_center().text_size(px(14.)).text_color(TEXT_3).child("You're all caught up.");
        };
        let header =
            drag_area(div())
                .h(px(52.))
                .flex_none()
                .pl(px(20.))
                .pr(px(20.))
                .flex()
                .items_center()
                .gap(px(10.))
                .text_size(px(14.))
                .child(ui::repo_mark(&mark, false, None))
                .child(div().text_color(TEXT_2).child(project))
                .child(div().text_color(TEXT_6).child("/"))
                .child(div().truncate().font_weight(FontWeight::SEMIBOLD).child(title))
                .when_some(needs_you, |d, secs| {
                    d.child(ui::status("needs-you", State::NeedsYou))
                        .child(div().font_family(MONO).text_size(px(11.5)).text_color(WAITING_TEXT).child(format!("{}:{:02}", secs / 60, secs % 60)))
                })
                .child(div().flex_1())
                .child(
                    ui::button("open-session", Variant::Secondary, None, "Open session")
                        .child(icon("forward", 14., TEXT))
                        .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(InboxEvent::Open { agent: agent.clone(), mark_seen: false }))),
                );
        let hints = div()
            .h(px(36.))
            .flex_none()
            .px(px(20.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(TEXT_2)
            .child(if chat { "Answer in the chat ·" } else { "Answer in the terminal ·" })
            .child(kbd("J"))
            .child(kbd("K"))
            .child("next / previous ·")
            .child(kbd("⌘ ↵"))
            .child("open session");
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(
                div().flex_1().min_h_0().px(px(10.)).pt(px(10.)).flex().flex_col().key_context(keys::CONTEXT).track_focus(&focus).on_key_down(key).child(pane),
            )
            .child(hints)
    }
}

impl Desktop {
    pub fn inbox_detail(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let selected = self.inbox.read(cx).selected;
        let open = self.shown_notes(cx).into_iter().nth(selected).map(|n| {
            let path = self.project_path(&n.agent, cx);
            let project = basename(&path);
            let chat = self.chat_shown(&n.terminal);
            let pane =
                if chat { self.chat_view(&n.terminal, window, cx).into_any_element() } else { self.terminal_pane(&n.terminal, window, cx).into_any_element() };
            Open {
                project: project.clone(),
                mark: self.project_mark(&path, ui::mark_letter(&project)),
                title: self.agents.get(&n.agent).map(|a| a.title.clone()).unwrap_or_default(),
                needs_you: (n.status == Status::NeedsYou).then(|| (now_ms() - n.at).max(0) / 1000),
                agent: n.agent,
                chat,
                pane,
                focus: self.terminal.focus.clone(),
                key: Box::new(cx.listener(Self::on_term_key)),
            }
        });
        self.inbox.update(cx, |i, cx| i.detail(open, cx))
    }
}
