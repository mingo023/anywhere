mod composer;
mod diff;
mod end_row;
mod fold;
mod jump;
mod locate;
pub(crate) mod markdown;
mod meta;
mod permission;
mod phase;
mod question;
mod question_marker;
mod reveal;
mod shimmer;
mod sub_agent;
mod thinking;
mod todos;
mod tool_detail;
pub(crate) mod tool_row;
pub(crate) mod transcript;
mod user_bubble;
mod working;

use crate::desktop::Desktop;
use agents::Summary;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use std::collections::HashMap;
use theme::*;

/// Only an agent's terminal has a chat to show.
pub(crate) fn can_chat(summary: Option<&Summary>) -> bool {
    summary.is_some()
}

pub(crate) struct ChatState {
    pub(crate) input: Entity<TextareaState>,
    /// Each agent's transcript view, by agent id.
    pub(crate) views: HashMap<String, transcript::View>,
    pub(crate) composer: composer::ComposerState,
    pub(crate) files: Option<locate::Choice>,
}

impl ChatState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let input = composer::input(window, cx);
        let (composer, subs) = composer::ComposerState::new(&input, window, cx);
        (Self { input, views: HashMap::new(), composer, files: None }, subs)
    }
}

impl Desktop {
    /// Whether `terminal`'s pane shows its agent's chat: only a session started from the New session page does, and only while its agent runs.
    pub(crate) fn chat_shown(&self, terminal: &str) -> bool {
        self.store.chats.contains(terminal) && can_chat(self.summary(terminal))
    }

    /// Focuses the terminal pane, and its chat's input when the chat shows: keys then go to the input.
    pub(crate) fn focus_chat_or_terminal(&mut self, terminal: String, window: &mut Window, cx: &mut Context<Self>) {
        let chat = self.chat_shown(&terminal);
        self.focus_pane(terminal, window, cx);
        if chat {
            self.chats.input.update(cx, |s, cx| s.focus(window, cx));
        }
    }

    /// A session's agent can be listed after its terminal took focus; keys then belong to the chat's input.
    pub(crate) fn focus_arrived_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal.focus.is_focused(window) && self.terminal.focused.as_ref().is_some_and(|t| self.chat_shown(t)) {
            self.chats.input.update(cx, |s, cx| s.focus(window, cx));
        }
    }

    /// An agent terminal's body while it shows the chat: the transcript over the composer.
    pub(crate) fn chat_view(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let transcript = self.transcript(terminal, cx);
        let composer = self.chat_composer(terminal, window, cx);
        let files = self.summary(terminal).map(|s| s.id.clone()).and_then(|agent| self.chat_files_menu(&agent, cx));
        div().flex_1().min_h_0().flex().flex_col().border_t(px(0.5)).border_color(SEPARATOR).child(transcript).child(composer).children(files)
    }
}

#[cfg(test)]
mod tests {
    use super::{Summary, can_chat};

    #[test]
    fn chat_shows_only_for_terminals_that_run_an_agent() {
        let agent = Summary { id: "a".into(), terminal_id: "t".into(), ..Default::default() };
        assert!(can_chat(Some(&agent)));
        assert!(!can_chat(None));
    }
}
