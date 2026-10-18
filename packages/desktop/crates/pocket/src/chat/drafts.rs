use std::collections::HashMap;

/// Each chat's draft, while the one input holds a single chat's.
pub(crate) struct Drafts<T> {
    /// The agent whose chat the input holds the draft of.
    owner: Option<String>,
    parked: HashMap<String, T>,
}

impl<T> Default for Drafts<T> {
    fn default() -> Self {
        Self { owner: None, parked: HashMap::new() }
    }
}

impl<T: Default> Drafts<T> {
    /// Gives the input to `agent`'s chat; `held` takes out what the input holds now, for the chat that had it.
    /// Returns `agent`'s draft to put in, or None when its chat has the input already.
    pub(crate) fn switch(&mut self, agent: &str, held: impl FnOnce() -> T) -> Option<T> {
        if self.owner.as_deref() == Some(agent) {
            return None;
        }
        if let Some(owner) = self.owner.replace(agent.to_string()) {
            self.parked.insert(owner, held());
        }
        Some(self.parked.remove(agent).unwrap_or_default())
    }

    /// Forgets a closed chat's draft; true when the input holds it, to empty.
    pub(crate) fn close(&mut self, agent: &str) -> bool {
        self.parked.remove(agent);
        let held = self.owner.as_deref() == Some(agent);
        if held {
            self.owner = None;
        }
        held
    }

    pub(crate) fn owner(&self) -> Option<&str> {
        self.owner.as_deref()
    }

    /// The draft of a chat that hasn't the input.
    pub(crate) fn parked(&self, agent: &str) -> Option<&T> {
        self.parked.get(agent)
    }
}

#[cfg(test)]
mod tests {
    use super::Drafts;

    fn to(drafts: &mut Drafts<String>, input: &mut String, agent: &str) {
        if let Some(draft) = drafts.switch(agent, || std::mem::take(input)) {
            *input = draft;
        }
    }

    #[test]
    fn switching_chats_keeps_each_draft() {
        let (mut drafts, mut input) = (Drafts::default(), String::new());
        to(&mut drafts, &mut input, "a");
        input.push_str("for a");
        to(&mut drafts, &mut input, "b");
        assert_eq!(input, "", "a's text stays with a");
        assert_eq!(drafts.parked("a").map(String::as_str), Some("for a"));
        input.push_str("for b");
        to(&mut drafts, &mut input, "a");
        assert_eq!(input, "for a");
        to(&mut drafts, &mut input, "a");
        assert_eq!(input, "for a", "the chat holding the input keeps it as is");
        to(&mut drafts, &mut input, "b");
        assert_eq!(input, "for b");
    }

    #[test]
    fn a_closed_chat_drops_its_draft() {
        let (mut drafts, mut input) = (Drafts::default(), String::new());
        to(&mut drafts, &mut input, "a");
        input.push_str("for a");
        to(&mut drafts, &mut input, "b");
        input.push_str("for b");
        assert!(!drafts.close("a"), "the input holds b's draft");
        assert!(drafts.close("b"), "the input holds b's draft, to empty");
        assert_eq!(drafts.owner(), None);
        input.clear();
        to(&mut drafts, &mut input, "a");
        assert_eq!(input, "");
    }
}
