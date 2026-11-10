use super::logic::wait_hint;
use agents::providers;
use agents::tickets::{State, Ticket, TicketDraft};

const NOT_A_REPO: &str = "This folder is not a git repository";

/// The New ticket dialog's choices; its texts live in the inputs.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct Values {
    pub(super) id: Option<String>,
    pub(super) state: Option<State>,
    pub(super) folder: String,
    pub(super) provider: String,
    pub(super) base: String,
}

impl Values {
    pub(super) fn new_in(folder: String) -> Self {
        Self { id: None, state: None, folder, provider: providers::IDS[0].to_string(), base: String::new() }
    }

    pub(super) fn of(t: &Ticket) -> Self {
        Self { id: Some(t.id.clone()), state: Some(t.state), folder: t.folder.clone(), provider: t.provider.clone(), base: t.base.clone() }
    }

    pub(super) fn editing(&self) -> bool {
        self.id.is_some()
    }

    /// A queued ticket has already been started.
    pub(super) fn can_start(&self) -> bool {
        self.state != Some(State::Queued)
    }

    /// A branch belongs to one folder, so a new folder starts from its default.
    pub(super) fn set_folder(&mut self, folder: String) {
        (self.folder, self.base) = (folder, String::new());
    }

    /// The branch picked, else the folder's default, which `branches` lists first.
    pub(super) fn base_label(&self, branches: &[String]) -> String {
        Some(self.base.clone()).filter(|b| !b.is_empty()).or_else(|| branches.first().cloned()).unwrap_or_else(|| "Default branch".into())
    }

    pub(super) fn ready(&self, title: &str, prompt: &str, is_repo: bool) -> bool {
        !title.trim().is_empty() && !prompt.trim().is_empty() && !self.folder.is_empty() && is_repo
    }

    pub(super) fn hint(&self, is_repo: bool, busy: usize, limit: usize) -> Option<String> {
        if !is_repo {
            return Some(NOT_A_REPO.to_string());
        }
        wait_hint(busy, limit).filter(|_| self.can_start())
    }

    pub(super) fn draft(&self, title: &str, prompt: &str) -> TicketDraft {
        TicketDraft { id: self.id.clone(), title: title.trim().to_string(), prompt: prompt.trim().to_string(), provider: self.provider.clone(), folder: self.folder.clone(), base: self.base.clone() }
    }
}

/// The folder a new ticket starts in: the one filtered on, else the one open, else the first.
pub(super) fn default_folder(filter: Option<&str>, current: Option<&str>, first: Option<&str>) -> String {
    filter.or(current).or(first).unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn values() -> Values {
        Values::new_in("/app".into())
    }

    #[test]
    fn a_new_ticket_starts_in_the_filtered_project_then_the_open_one_then_the_first() {
        assert_eq!(default_folder(Some("/f"), Some("/c"), Some("/a")), "/f");
        assert_eq!(default_folder(None, Some("/c"), Some("/a")), "/c");
        assert_eq!(default_folder(None, None, Some("/a")), "/a");
        assert_eq!(default_folder(None, None, None), "");
    }

    #[test]
    fn moving_to_another_folder_forgets_the_branch() {
        let mut v = values();
        v.base = "dev".into();
        v.set_folder("/other".into());
        assert_eq!((v.folder.as_str(), v.base.as_str()), ("/other", ""));
    }

    #[test]
    fn the_base_shows_the_pick_else_the_default_branch() {
        let mut v = values();
        assert_eq!(v.base_label(&[]), "Default branch");
        assert_eq!(v.base_label(&["main".into(), "dev".into()]), "main");
        v.base = "dev".into();
        assert_eq!(v.base_label(&["main".into()]), "dev");
    }

    #[test]
    fn saving_needs_a_title_a_prompt_and_a_repository() {
        let v = values();
        assert!(v.ready("T", "P", true));
        assert!(!v.ready("  ", "P", true));
        assert!(!v.ready("T", "\n", true));
        assert!(!v.ready("T", "P", false));
        assert!(!Values::new_in(String::new()).ready("T", "P", true));
    }

    #[test]
    fn the_hint_names_a_folder_that_is_no_repository_first() {
        assert_eq!(values().hint(false, 4, 4).as_deref(), Some(NOT_A_REPO));
        assert_eq!(values().hint(true, 4, 4).as_deref(), Some("4 of 4 working, so it will wait in Queued"));
        assert_eq!(values().hint(true, 1, 4), None);
    }

    #[test]
    fn a_queued_ticket_cannot_start_again_and_gets_no_wait_hint() {
        let mut v = values();
        (v.id, v.state) = (Some("t".into()), Some(State::Queued));
        assert!(!v.can_start() && v.editing());
        assert_eq!(v.hint(true, 4, 4), None);
    }

    #[test]
    fn the_draft_trims_the_texts_and_keeps_the_choices() {
        let d = values().draft(" T ", "\nP\n");
        assert_eq!((d.title.as_str(), d.prompt.as_str(), d.folder.as_str(), d.provider.as_str()), ("T", "P", "/app", "claude"));
    }
}
