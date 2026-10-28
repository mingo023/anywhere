mod agent_map;
pub(crate) mod composer;
mod diff;
mod drafts;
mod end_row;
mod fold;
mod jump;
mod locate;
pub(crate) mod markdown;
pub(crate) mod mention;
mod meta;
mod permission;
mod phase;
mod pin;
mod question;
mod question_marker;
mod reveal;
mod selectable;
mod shimmer;
mod sticky;
mod sub_agent;
mod thinking;
mod todos;
mod tool_detail;
pub(crate) mod tool_row;
pub(crate) mod transcript;
mod user_bubble;
mod working;

use crate::composer::Draft;
use crate::explorer::mermaid::Diagrams;
use crate::desktop::Desktop;
use agents::Summary;
use gpui_kit::component::input::TextareaState;
use gpui_kit::*;
use std::collections::HashMap;
use theme::*;

/// Whether an agent's terminal shows its chat: one started from the New session page does, and one without its TUI has nothing else to show.
pub(crate) fn can_chat(started_for_chat: bool, summary: Option<&Summary>) -> bool {
    summary.is_some_and(|a| started_for_chat || a.chat_only)
}

/// The toast for a closed chat's unsent sends and draft, and the text it copies; none when nothing was left.
fn unsent_notice(who: &str, mut unsent: Vec<String>, typed: String) -> Option<(String, String)> {
    if !typed.trim().is_empty() {
        unsent.push(typed);
    }
    let count = match unsent.len() {
        0 => return None,
        1 => "a message".to_string(),
        n => format!("{n} messages"),
    };
    Some((format!("{who} closed before {count} went out"), unsent.join("\n\n")))
}

/// A diagram's card: a code block's `ink(0.035)` on the chat, made opaque for the diagram's own canvas.
pub(crate) const DIAGRAM_BG: Token = Token::new(0xf1f1f1ff, 0x1c1c1cff);

pub(crate) struct ChatState {
    pub(crate) input: Entity<TextareaState>,
    /// The text and attachments of each chat the input doesn't hold now.
    drafts: drafts::Drafts<(String, Draft)>,
    /// Each agent's transcript view, by agent id.
    pub(crate) views: HashMap<String, transcript::View>,
    pub(crate) composer: composer::ComposerState,
    pub(crate) files: Option<locate::Choice>,
    /// The error toast that tells of a closed chat's unsent text, and that text, which the toast copies.
    pub(crate) unsent: Option<(String, String)>,
    pub(crate) map: agent_map::AgentMap,
    /// Mermaid fences drawn in every chat, sharing one image budget.
    pub(crate) diagrams: Entity<Diagrams>,
}

impl ChatState {
    pub(crate) fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let input = composer::input(window, cx);
        let (composer, subs) = composer::ComposerState::new(&input, window, cx);
        (Self { input, drafts: Default::default(), views: HashMap::new(), composer, files: None, unsent: None, map: agent_map::AgentMap::new(cx), diagrams: cx.new(|_| Diagrams::standalone(DIAGRAM_BG)) }, subs)
    }
}

impl Desktop {
    /// Whether `terminal`'s pane shows its agent's chat, which it does only while its agent runs.
    pub(crate) fn chat_shown(&self, terminal: &str) -> bool {
        can_chat(self.store.chats.contains(terminal), self.summary(terminal))
    }

    /// Focuses the terminal pane, and its chat's input when the chat shows: keys then go to the input.
    pub(crate) fn focus_chat_or_terminal(&mut self, terminal: String, window: &mut Window, cx: &mut Context<Self>) {
        let chat = self.chat_shown(&terminal).then(|| self.summary(&terminal).map(|a| a.id.clone())).flatten();
        self.focus_pane(terminal, window, cx);
        if let Some(agent) = chat {
            self.chat_focus_input(&agent, window, cx);
        }
    }

    pub(crate) fn chat_focus_input(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.chat_take_input(agent, window, cx);
        self.chats.input.update(cx, |s, cx| s.focus(window, cx));
    }

    /// A session's agent can be listed after its terminal took focus; keys then belong to the chat's input.
    pub(crate) fn focus_arrived_chat(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.terminal.focus.is_focused(window) && self.terminal.focused.as_ref().is_some_and(|t| self.chat_shown(t)) {
            self.chats.input.update(cx, |s, cx| s.focus(window, cx));
        }
    }

    /// Hands the input to `agent`'s chat; the chat that had it keeps its draft for when it's back.
    pub(crate) fn chat_take_input(&mut self, agent: &str, window: &mut Window, cx: &mut Context<Self>) {
        let ChatState { input, drafts, composer, .. } = &mut self.chats;
        let Some((text, draft)) = drafts.switch(agent, || (input.read(cx).value().to_string(), std::mem::take(&mut composer.draft))) else { return };
        composer.draft = draft;
        input.update(cx, |s, cx| s.set_value(text, window, cx));
    }

    /// Drops a closed agent's chat. What it never sent, and its draft, wait in the error toast to copy: no other chat's composer takes them.
    pub(crate) fn chat_closed(&mut self, agent: &str, who: &str, window: &mut Window, cx: &mut Context<Self>) {
        let (typed, _) = self.chat_draft(agent, cx);
        if self.chats.drafts.close(agent) {
            self.chats.composer.draft.clear();
            self.chats.input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        let unsent = self.chats.views.remove(agent).map(|mut v| v.session.outgoing.drain()).unwrap_or_default();
        if let Some((message, text)) = unsent_notice(who, unsent, typed) {
            self.error = Some(message.clone());
            self.chats.unsent = Some((message, text));
        }
    }

    /// An agent terminal's body while it shows the chat: the transcript over the composer.
    pub(crate) fn chat_view(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let agent = self.summary(terminal).map(|s| s.id.clone());
        if let Some(agent) = agent.as_ref().filter(|_| self.terminal.focused.as_deref() == Some(terminal)) {
            self.chat_take_input(agent, window, cx);
        }
        let transcript = self.transcript(terminal, window, cx);
        let composer = self.chat_composer(terminal, window, cx);
        let composer = match &agent {
            Some(agent) => self.arriving(agent, crate::creating::DOCK, composer, window, cx),
            None => composer,
        };
        let files = agent.and_then(|agent| self.chat_files_menu(&agent, cx));
        div().flex_1().min_h_0().flex().flex_col().border_t(px(0.5)).border_color(SEPARATOR).child(transcript).child(composer).children(files)
    }
}

#[cfg(test)]
mod tests {
    use super::{Summary, can_chat, unsent_notice};

    #[test]
    fn chat_shows_only_for_terminals_that_run_an_agent() {
        let agent = Summary { id: "a".into(), terminal_id: "t".into(), ..Default::default() };
        assert!(can_chat(true, Some(&agent)));
        assert!(!can_chat(true, None));
    }

    #[test]
    fn an_agent_only_the_chat_drives_shows_its_chat_wherever_it_was_started() {
        let tui = Summary { id: "a".into(), terminal_id: "t".into(), ..Default::default() };
        assert!(!can_chat(false, Some(&tui)));
        assert!(can_chat(false, Some(&Summary { chat_only: true, ..tui })));
    }

    #[test]
    fn a_closed_chat_with_two_unsent_sends_says_two_messages() {
        let (message, text) = unsent_notice("Fix CI", vec!["a".into(), "b".into()], "  ".into()).unwrap();
        assert_eq!((message.as_str(), text.as_str()), ("Fix CI closed before 2 messages went out", "a\n\nb"));
        assert_eq!(unsent_notice("Fix CI", Vec::new(), "typed".into()).map(|(m, _)| m).as_deref(), Some("Fix CI closed before a message went out"));
        assert_eq!(unsent_notice("Fix CI", Vec::new(), " \n".into()), None);
    }
}
