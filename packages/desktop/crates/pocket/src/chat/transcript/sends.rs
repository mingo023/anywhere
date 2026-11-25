use super::View;
use crate::chat::tool_row::Tools;
use crate::util::now_ms;
use chat::{Session, Transcript};
use gpui_kit::{Entity, ListState};
use std::sync::Arc;

impl View {
    pub(crate) fn session(&self) -> &Session {
        &self.session
    }

    pub(crate) fn session_mut(&mut self) -> &mut Session {
        &mut self.session
    }

    pub(crate) fn transcript(&self) -> Arc<Transcript> {
        self.session.transcript.clone()
    }

    pub(crate) fn list(&self) -> &ListState {
        &self.list
    }

    pub(crate) fn tools(&self) -> Entity<Tools> {
        self.tools.clone()
    }

    pub(crate) fn echo(&mut self, prompt: String) {
        self.session.outgoing.push(prompt, false, now_ms());
    }

    pub(crate) fn send(&mut self, text: String, working: bool, request: Option<String>) {
        let send = self.session.outgoing.push(text, working, now_ms());
        if let Some((send, request)) = send.zip(request) {
            self.session.outgoing.sent_as(send, request);
        }
        if !working {
            self.pin();
        }
    }

    pub(crate) fn steer(&mut self, text: String, request: Option<String>) {
        let send = self.session.outgoing.steer_text(text, now_ms());
        if let Some(request) = request {
            self.session.outgoing.sent_as(send, request);
        }
        self.steered();
    }

    pub(crate) fn steer_held(&mut self, send: u64) -> Option<String> {
        let text = self.session.outgoing.steer(send, now_ms())?;
        self.steered();
        Some(text)
    }

    pub(crate) fn compact(&mut self, busy: bool) {
        self.session.outgoing.compact(busy, now_ms());
        if busy {
            self.steered();
        } else {
            self.pin();
        }
    }

    pub(crate) fn interrupt(&mut self) {
        self.session.outgoing.stop(now_ms());
    }

    pub(crate) fn stop_refused(&mut self) {
        self.session.outgoing.stop_refused();
    }

    pub(crate) fn send_now(&mut self) -> bool {
        self.session.outgoing.send_now(now_ms())
    }

    pub(crate) fn sent_as(&mut self, send: u64, request: String) {
        self.session.outgoing.sent_as(send, request);
    }

    pub(crate) fn send_failed(&mut self, request: &str) {
        self.session.outgoing.fail(request);
    }

    pub(crate) fn withdraw(&mut self, send: u64) -> Option<String> {
        self.session.outgoing.withdraw(send)
    }

    pub(crate) fn resume_queue(&mut self) {
        self.session.outgoing.resume(&self.session.transcript);
    }
}
