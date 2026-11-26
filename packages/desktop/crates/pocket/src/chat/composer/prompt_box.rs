use super::access_pill::Shown;
use super::model_pill::Pill;
use super::{
    ChatBoxEvent, ComposerState, INPUT_LINE_HEIGHT, INPUT_TEXT_SIZE, MIN_ROWS, attachments, frame_on, marks_overlay, pointer_left, send_button, shows_button,
};
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use agents::Summary;
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::PathBuf;
use theme::*;

pub(super) struct Face {
    pub(super) terminal: String,
    pub(super) locked: Option<&'static str>,
    pub(super) busy: bool,
    pub(super) stopping: bool,
    pub(super) caps: chat::Capabilities,
    pub(super) pill: Pill,
    pub(super) access: Option<Shown>,
    pub(super) map: Option<Stateful<Div>>,
    pub(super) listed: bool,
    pub(super) suggestions: Option<Div>,
    pub(super) attachments: PathBuf,
}

impl Desktop {
    pub(super) fn prompt_face(&self, terminal: &str, agent: &Summary, locked: Option<&'static str>, cx: &mut Context<Self>) -> Face {
        let view_only = locked.is_some();
        let composer = self.chats.composer.clone();
        let held = composer.read(cx).holds(&agent.id);
        let map = self.chats.views.get(&agent.id).map(|v| v.read(cx).transcript()).and_then(|t| self.chats.map.update(cx, |m, cx| m.chip(agent, &t, cx)));
        Face {
            terminal: terminal.to_string(),
            locked,
            busy: self.chat_busy(agent, cx),
            stopping: self.chat_stopping(agent, cx),
            caps: self.chat_caps(&agent.provider),
            pill: self.chat_model(agent, view_only, &composer, cx),
            access: self.chat_access(agent, view_only),
            map,
            listed: composer.read(cx).owner().is_some_and(|id| self.agents.get(id).is_some()),
            suggestions: (!view_only && held).then(|| self.suggest_list(cx)).flatten(),
            attachments: self.store.home().join("attachments"),
        }
    }
}

impl ComposerState {
    pub(super) fn prompt_box(&self, agent: &Summary, face: Face, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let Face { terminal, locked, busy, stopping, caps, pill, access, map, listed, suggestions, attachments: dir } = face;
        let view_only = locked.is_some();
        let held = self.holds(&agent.id);
        let (typed, draft) = self.draft_of(&agent.id, cx);
        let empty = typed.trim().is_empty() && !draft.is_some_and(crate::composer::Draft::sends_without_text);
        let typing = !empty || (held && self.input.read(cx).focus_handle(cx).is_focused(window));
        let action = chat::action(busy, stopping, empty);
        let blocked = !caps.allows(action);
        let press = |t: &str| {
            let t = t.to_string();
            cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(ChatBoxEvent::Press { terminal: t.clone(), action }))
        };
        let left = cx.listener(|this, over: &bool, _, _| {
            if pointer_left(*over, this.pressing_send) {
                this.clicked_send = None;
            }
        });
        let released = || cx.listener(|this, _: &MouseUpEvent, _, _| this.pressing_send = false);
        // The id names the action so the tooltip of the button a send was clicked on doesn't stay on the Stop that replaced it.
        let send = (!view_only && shows_button(action, empty)).then(|| {
            send_button::send_button(id(format!("chat-send:{terminal}:{action:?}")), action, empty, blocked)
                .on_hover(left)
                .on_mouse_down(MouseButton::Left, cx.listener(|this, _: &MouseDownEvent, _, _| this.pressing_send = true))
                .on_mouse_up(MouseButton::Left, released())
                .on_mouse_up_out(MouseButton::Left, released())
                .when(!blocked, |b| b.on_click(press(&terminal)))
        });
        let attach = (!view_only).then(|| {
            attachments::attach_button(
                id(format!("chat-attach:{terminal}")),
                cx.listener(|_, _: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    cx.emit(ChatBoxEvent::Attach);
                }),
            )
        });
        let strip = draft.and_then(|d| attachments::strip(d, cx));
        let quote = draft.and_then(|d| d.quote.as_ref()).map(|q| {
            let a = agent.id.clone();
            crate::composer::quote_strip::quote_strip(q, cx.listener(move |this, _: &ClickEvent, _, cx| this.edit_draft(&a, cx, |d| d.quote = None)))
        });
        let input = if let Some(why) = locked {
            div().px(px(10.)).py(px(8.)).text_color(TEXT_3).child(why).into_any_element()
        } else if !held {
            // Another chat's draft is in the input; this one's shows until a press here takes the input.
            let shown = if typed.is_empty() { (self.placeholder.get().to_string(), TEXT_3) } else { (typed, TEXT) };
            div()
                .px(px(10.))
                .py(px(8.))
                .min_h(px(MIN_ROWS as f32 * INPUT_LINE_HEIGHT + 16.))
                .line_height(px(INPUT_LINE_HEIGHT))
                .text_color(shown.1)
                .child(shown.0)
                .into_any_element()
        } else {
            let (this, input) = (cx.weak_entity(), self.input.clone());
            let textarea = crate::composer::pills::with_pills(Textarea::new(&self.input), &self.input)
                .appearance(false)
                .text_size(px(INPUT_TEXT_SIZE))
                .line_height(px(INPUT_LINE_HEIGHT))
                .on_paste(move |item, window, cx| crate::composer::paste_in(&this, &input, &dir, item, window, cx));
            div().child(textarea).child(marks_overlay(self.input.clone())).into_any_element()
        };
        let (t, a) = (terminal.clone(), agent.id.clone());
        let text = crate::composer::prompt_text().child(input);
        let bar = crate::composer::prompt_bar()
            .children(self.goal_pill(agent, view_only, cx))
            .child(self.model_pill(&pill, cx))
            .children(access.map(|shown| self.access_pill(&agent.id, shown, cx)))
            .children(map)
            .child(div().flex_1())
            .children(attach)
            .children(send);
        crate::composer::rich::rich_keys(&self.input, frame_on(self.suggest_keys(listed, div(), cx), typing), cx)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _: &MouseDownEvent, window, cx| {
                    this.take_input(&a, window, cx);
                    this.input.update(cx, |s, cx| s.focus(window, cx));
                }),
            )
            .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                if e.keystroke.key == "escape" {
                    if this.draft.escape_account() || this.draft.escape_model() {
                        cx.stop_propagation();
                        return cx.notify();
                    }
                    cx.emit(ChatBoxEvent::Escape(t.clone()));
                }
            }))
            .capture_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
                if held && e.keystroke.key == "backspace" && this.draft.quote.is_some() && this.input.read(cx).value().is_empty() {
                    this.draft.quote = None;
                    cx.stop_propagation();
                    cx.notify();
                }
            }))
            .children(suggestions)
            .children(quote)
            .children(strip)
            .child(text)
            .child(bar)
    }
}
