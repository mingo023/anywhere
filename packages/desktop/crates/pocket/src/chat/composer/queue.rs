use super::RADIUS;
use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use crate::util::now_ms;
use agents::queue::Entry;
use chat::{Delivery, Pending};
use gpui_kit::component::input::RopeExt as _;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use theme::*;

/// The composer's text once a held send comes back to it: under what was already typed, which it doesn't replace.
fn editing(draft: &str, held: &str) -> String {
    if draft.trim().is_empty() { held.to_string() } else { format!("{draft}\n\n{held}") }
}

/// The sends held behind the agent's turn; the transcript shows the rest.
pub(super) fn waiting(sends: &[Pending]) -> Vec<&Pending> {
    sends.iter().filter(|p| p.delivery == Delivery::Queued).collect()
}

/// The place in the order it goes.
fn number(n: usize) -> Div {
    div()
        .size(px(18.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(FILL_4)
        .text_size(px(10.5))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT_3)
        .child((n + 1).to_string())
}

fn header_note(paused: bool, held: bool) -> &'static str {
    match (paused, held) {
        (_, true) => "held after Stop, your next message sends them",
        (true, false) => "paused",
        (false, false) => "send in order after this turn",
    }
}

pub(super) fn control(key: String, tip: &'static str) -> Stateful<Div> {
    div()
        .id(id(key))
        .h(px(26.))
        .min_w(px(26.))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(7.))
        .cursor_pointer()
        .hover(|s| s.bg(FILL_3))
        .tooltip(move |window, cx| Tooltip::new(tip).build(window, cx))
}

impl Desktop {
    /// The tray over the box: what waits, numbered in the order it goes, each with Steer, Edit and Remove; none when nothing waits.
    /// Once the user stopped the turn, or it failed, nothing goes until Resume.
    /// A pocketd that keeps the queue lists its entries first, with Send now, Move and Remove; a send held here follows.
    pub(super) fn queue_tray(&self, agent: &str, sends: &[Pending], paused: bool, cx: &mut Context<Self>) -> Option<Div> {
        let rows = waiting(sends);
        let entries = self.agents.queues.of(agent);
        if rows.is_empty() && entries.is_empty() {
            return None;
        }
        let held = self.agents.queues.paused(agent);
        let resume = paused.then(|| {
            let agent = agent.to_string();
            control(format!("chat-queue-resume:{agent}"), "Send the queued messages in order")
                .px(px(8.))
                .gap(px(5.))
                .text_color(TEXT_2)
                .child(icon("play", 12., TEXT_2))
                .child("Resume")
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.chat_queue_resume(&agent, cx)))
        });
        let header = div()
            .min_h(px(26.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.))
            .text_color(TEXT_2)
            .child(icon("list", 13., TEXT_2))
            .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(format!("{} queued", rows.len() + entries.len())))
            .child(header_note(paused, held))
            .child(div().flex_1())
            .children(resume);
        let row = |(n, p): (usize, &&Pending)| {
            let on = |f: fn(&mut Self, &str, u64, &mut Window, &mut Context<Self>)| {
                let (agent, send) = (agent.to_string(), p.id);
                cx.listener(move |this, _: &ClickEvent, window, cx| f(this, &agent, send, window, cx))
            };
            let key = |what: &str| format!("chat-queue-{what}:{agent}:{}", p.id);
            let steer = control(key("steer"), "Send into the turn underway")
                .px(px(8.))
                .gap(px(5.))
                .text_size(px(12.))
                .text_color(TEXT_2)
                .child(icon("forward", 12., TEXT_2))
                .child("Steer")
                .on_click(on(Self::chat_queue_steer));
            div()
                .min_h(px(32.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(13.))
                .child(number(n))
                .child(div().flex_1().min_w_0().truncate().text_color(TEXT_BODY).child(p.text.lines().next().unwrap_or_default().to_string()))
                .child(steer)
                .child(control(key("edit"), "Edit queued message").child(icon("pencil", 13., TEXT_3)).on_click(on(Self::chat_queue_edit)))
                .child(control(key("remove"), "Remove queued message").child(icon("x", 13., TEXT_3)).on_click(on(Self::chat_queue_remove)))
        };
        let entry_row = |(n, e): (usize, &Entry)| {
            let on = |f: fn(&mut Self, &str, &str, &mut Window, &mut Context<Self>)| {
                let (agent, entry) = (agent.to_string(), e.id.clone());
                cx.listener(move |this, _: &ClickEvent, window, cx| f(this, &agent, &entry, window, cx))
            };
            let key = |what: &str| format!("chat-queue-{what}:{agent}:{}", e.id);
            let queues = &self.agents.queues;
            let move_by = |up: bool, tip: &'static str, name: &'static str| {
                let to = queues.before_move(agent, &e.id, up).is_some();
                let button = control(key(if up { "up" } else { "down" }), tip).child(icon(name, 12., if to { TEXT_3 } else { TEXT_6 }));
                if to { button.on_click(on(if up { Self::chat_entry_up } else { Self::chat_entry_down })) } else { button.cursor_default() }
            };
            let sending = e.state == "sending";
            let controls = (!sending).then(|| {
                div()
                    .flex()
                    .items_center()
                    .child(move_by(true, "Move up", "chevron-up"))
                    .child(move_by(false, "Move down", "chevron-down"))
                    .child(
                        control(key("send-now"), "Send now, ahead of the turn's end")
                            .px(px(8.))
                            .gap(px(5.))
                            .text_size(px(12.))
                            .text_color(TEXT_2)
                            .child(icon("forward", 12., TEXT_2))
                            .child("Send now")
                            .on_click(on(Self::chat_entry_send_now)),
                    )
                    .child(control(key("edit"), "Edit queued message").child(icon("pencil", 13., TEXT_3)).on_click(on(Self::chat_entry_edit)))
                    .child(control(key("remove"), "Remove queued message").child(icon("x", 13., TEXT_3)).on_click(on(Self::chat_entry_cancel)))
            });
            div()
                .min_h(px(32.))
                .flex()
                .items_center()
                .gap(px(8.))
                .text_size(px(13.))
                .child(number(n))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .text_color(if e.origin == "user" { TEXT_BODY } else { TEXT_3 })
                        .child(e.text.lines().next().unwrap_or_default().to_string()),
                )
                .children(controls)
        };
        Some(
            surface()
                .child(header)
                .children(entries.iter().enumerate().map(entry_row))
                .children(rows.iter().enumerate().map(|(n, p)| row((entries.len() + n, p)))),
        )
    }

    fn entry_do(&mut self, agent: &str, entry: &str, send: impl FnOnce(&Self)) {
        if self.agents.queues.of(agent).iter().any(|e| e.id == entry) {
            send(self);
        }
    }

    pub(crate) fn chat_entry_send_now(&mut self, agent: &str, entry: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.entry_do(agent, entry, |this| this.outbox.queue_send_now(agent, entry));
        cx.notify();
    }

    pub(crate) fn chat_entry_cancel(&mut self, agent: &str, entry: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.entry_do(agent, entry, |this| this.outbox.queue_cancel(agent, entry));
        cx.notify();
    }

    pub(crate) fn chat_entry_up(&mut self, agent: &str, entry: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.chat_entry_move(agent, entry, true, cx);
    }

    pub(crate) fn chat_entry_down(&mut self, agent: &str, entry: &str, _: &mut Window, cx: &mut Context<Self>) {
        self.chat_entry_move(agent, entry, false, cx);
    }

    fn chat_entry_move(&mut self, agent: &str, entry: &str, up: bool, cx: &mut Context<Self>) {
        if let Some(before) = self.agents.queues.before_move(agent, entry, up) {
            self.outbox.queue_reorder(agent, entry, &before);
            cx.notify();
        }
    }

    /// Takes a queued entry's text into the composer; the next send changes the entry itself, keeping its place.
    pub(crate) fn chat_entry_edit(&mut self, agent: &str, entry: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.agents.queues.of(agent).iter().find(|e| e.id == entry).map(|e| e.text.clone()) else { return };
        self.chats.editing.insert(agent.to_string(), entry.to_string());
        self.chat_take_back(agent, &text, window, cx);
    }

    /// The entry the composer's text will replace, while pocketd still has it queued.
    pub(crate) fn chat_editing(&mut self, agent: &str) -> Option<String> {
        let entry = self.chats.editing.remove(agent)?;
        self.agents.queues.of(agent).iter().any(|e| e.id == entry).then_some(entry)
    }

    pub(crate) fn chat_queue_remove(&mut self, agent: &str, send: u64, _: &mut Window, cx: &mut Context<Self>) {
        if let Some(v) = self.chats.views.get_mut(agent) {
            v.session.outgoing.withdraw(send);
            cx.notify();
        }
    }

    /// Takes a held send back into the composer to rework.
    pub(crate) fn chat_queue_edit(&mut self, agent: &str, send: u64, window: &mut Window, cx: &mut Context<Self>) {
        let Some(text) = self.chats.views.get_mut(agent).and_then(|v| v.session.outgoing.withdraw(send)) else { return };
        self.chat_take_back(agent, &text, window, cx);
    }

    /// Puts a send's text back in `agent`'s composer, under what was typed there.
    pub(crate) fn chat_take_back(&mut self, agent: &str, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.chat_take_input(agent, window, cx);
        let text = editing(&self.chats.input.read(cx).value(), text);
        self.set_chat_text(&text, window, cx);
        self.chats.input.update(cx, |s, cx| {
            let end = s.text().offset_to_position(text.len());
            s.set_cursor_position(end, window, cx);
        });
    }

    /// Lets the queue go on past the turn it was paused behind; the next send goes once the agent is idle.
    pub(crate) fn chat_queue_resume(&mut self, agent: &str, cx: &mut Context<Self>) {
        if let Some(v) = self.chats.views.get_mut(agent) {
            v.session.outgoing.resume(&v.session.transcript);
            self.refresh_chats();
            cx.notify();
        }
    }

    /// Sends a held send now: the agent takes it mid-turn.
    pub(crate) fn chat_queue_steer(&mut self, agent: &str, send: u64, _: &mut Window, cx: &mut Context<Self>) {
        let Some(v) = self.chats.views.get_mut(agent) else { return };
        let Some(text) = v.session.outgoing.steer(send, now_ms()) else { return };
        v.steered();
        if let Some(request) = self.outbox.prompt(agent, &text) {
            v.session.outgoing.sent_as(send, request);
        }
        cx.notify();
    }
}

/// A tray resting on the box's top edge.
/// Trays could tuck under the box, but in dark the box's SURFACE is translucent and would show the overlap through, so they stop at its edge.
pub(super) fn surface() -> Div {
    div().mx(px(10.)).px(px(12.)).py(px(10.)).flex().flex_col().rounded_t(px(RADIUS)).border_1().border_b_0().border_color(SEPARATOR).bg(SURFACE_SUNKEN)
}

#[cfg(test)]
mod tests {
    use super::{editing, waiting};
    use chat::{Outgoing, Transcript};

    #[test]
    fn the_queue_lists_only_queued_sends() {
        let mut out = Outgoing::default();
        out.push("lost".into(), false, 0);
        out.settle(&Transcript::default(), false, 200_000);
        out.push("next".into(), true, 200_000);
        out.push("sending".into(), false, 200_000);
        let texts: Vec<&str> = waiting(out.list()).iter().map(|p| p.text.as_str()).collect();
        assert_eq!(texts, ["next"], "a send not delivered waits in the transcript, with its retry");
    }

    #[test]
    fn editing_a_held_send_keeps_what_was_typed() {
        assert_eq!(editing("", "held"), "held");
        assert_eq!(editing("  ", "held"), "held");
        assert_eq!(editing("typed", "held"), "typed\n\nheld");
    }
}
