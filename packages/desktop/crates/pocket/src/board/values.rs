use super::logic::{column, wait_hint};
use agents::providers;
use agents::tickets::{State, Ticket, TicketDraft};

const NOT_A_REPO: &str = "This folder is not a git repository";

/// The composer's or ticket page's choices; the texts live in the inputs.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct Values {
    pub(super) id: Option<String>,
    pub(super) state: Option<State>,
    pub(super) folder: String,
    pub(super) provider: String,
    pub(super) base: String,
    pub(super) priority: u8,
    pub(super) labels: Vec<String>,
}

impl Values {
    pub(super) fn new_in(folder: String) -> Self {
        Self { id: None, state: None, folder, provider: providers::IDS[0].to_string(), base: String::new(), priority: 0, labels: Vec::new() }
    }

    pub(super) fn of(t: &Ticket) -> Self {
        Self { id: Some(t.id.clone()), state: Some(t.state), folder: t.folder.clone(), provider: t.provider.clone(), base: t.base.clone(), priority: t.priority, labels: t.labels.clone() }
    }

    /// A queued ticket has already been started.
    pub(super) fn can_start(&self) -> bool {
        self.state != Some(State::Queued)
    }

    /// A branch belongs to one folder, so a new folder starts from its default.
    pub(super) fn set_folder(&mut self, folder: String) {
        (self.folder, self.base) = (folder, String::new());
    }

    pub(super) fn toggle_label(&mut self, label: &str) {
        match self.labels.iter().position(|l| l == label) {
            Some(i) => _ = self.labels.remove(i),
            None => self.labels.push(label.to_string()),
        }
    }

    pub(super) fn set_label(&mut self, label: &str, on: bool) {
        if self.labels.iter().any(|l| l == label) != on {
            self.toggle_label(label);
        }
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
        TicketDraft { id: self.id.clone(), title: title.trim().to_string(), prompt: prompt.trim().to_string(), provider: self.provider.clone(), folder: self.folder.clone(), base: self.base.clone(), priority: self.priority, labels: self.labels.clone() }
    }
}

/// The choices being made, with the branches of their folder as last read.
#[derive(Clone, PartialEq, Debug)]
pub(super) struct Picks {
    pub(super) values: Values,
    branches: Option<(String, Vec<String>)>,
}

impl Picks {
    pub(super) fn new(values: Values) -> Self {
        Self { values, branches: None }
    }

    pub(super) fn branches(&self) -> &[String] {
        self.branches.as_ref().filter(|(f, _)| *f == self.values.folder).map_or(&[], |(_, b)| b.as_slice())
    }

    pub(super) fn set_branches(&mut self, folder: String, branches: Vec<String>) {
        if self.values.folder == folder {
            self.branches = Some((folder, branches));
        }
    }
}

/// Whether the composer should take a new project: its own is gone, or the project filter names another.
pub(super) fn needs_aim(folder: &str, filter: Option<&str>, projects: &[String]) -> bool {
    filter.is_some_and(|f| f != folder) || !projects.iter().any(|p| p == folder)
}

/// What a ticket page owes the daemon, if anything. A field left blank keeps what the ticket had.
pub(super) fn page_save(v: &Values, title: &str, prompt: &str, t: &Ticket) -> Option<TicketDraft> {
    let kept = |typed: &'_ str, saved: &str| if typed.trim().is_empty() { saved.to_string() } else { typed.to_string() };
    let draft = v.draft(&kept(title, &t.title), &kept(prompt, &t.prompt));
    (column(t).lifts() && draft != Values::of(t).draft(&t.title, &t.prompt)).then_some(draft)
}

/// What a ticket that started under its open page still takes from it: priority and labels.
pub(super) fn late_save(v: &Values, t: &Ticket) -> Option<TicketDraft> {
    let mut kept = Values::of(t);
    (kept.priority, kept.labels) = (v.priority, v.labels.clone());
    (v.priority != t.priority || v.labels != t.labels).then(|| kept.draft(&t.title, &t.prompt))
}

/// The folder a new ticket starts in: the one filtered on, else the one open, else the first.
pub(super) fn default_folder(filter: Option<&str>, current: Option<&str>, first: Option<&str>) -> String {
    filter.or(current).or(first).unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agents::tickets::{ClosedWhy, Status};

    fn values() -> Values {
        Values::new_in("/app".into())
    }

    #[test]
    fn setting_a_label_leaves_a_ticket_that_already_agrees_alone() {
        let mut v = values();
        v.set_label("Bug", true);
        v.set_label("Bug", true);
        assert_eq!(v.labels, vec!["Bug"]);
        v.set_label("Bug", false);
        v.set_label("Bug", false);
        assert!(v.labels.is_empty());
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
        assert!(!v.can_start() && v.id.is_some());
        assert_eq!(v.hint(true, 4, 4), None);
    }

    fn saved(state: State) -> Ticket {
        Ticket {
            id: "t".into(),
            title: "T".into(),
            prompt: "P".into(),
            provider: "claude".into(),
            folder: "/app".into(),
            base: String::new(),
            state,
            order: 0,
            status: Status::None,
            why: String::new(),
            agent_id: String::new(),
            terminal_id: String::new(),
            worktree: String::new(),
            created_at: 0,
            changed_at: 0,
            closed_at: 0,
            closed_why: ClosedWhy::default(),
            number: 0,
            priority: 0,
            labels: Vec::new(),
        }
    }

    #[test]
    fn the_composer_changes_project_when_its_own_is_gone_or_the_filter_names_another() {
        let projects = ["/app".to_string(), "/web".to_string()];
        assert!(!needs_aim("/app", None, &projects));
        assert!(!needs_aim("/app", Some("/app"), &projects));
        assert!(needs_aim("/app", Some("/web"), &projects));
        assert!(needs_aim("/gone", None, &projects));
    }

    #[test]
    fn a_page_saves_only_what_changed_and_a_blank_field_keeps_the_ticket_text() {
        let t = saved(State::Todo);
        let mut v = Values::of(&t);
        assert_eq!(page_save(&v, "T", "P", &t), None);
        v.priority = 2;
        let draft = page_save(&v, "  ", "", &t).unwrap();
        assert_eq!((draft.title.as_str(), draft.prompt.as_str(), draft.priority), ("T", "P", 2));
        assert_eq!(page_save(&v, "New", "P", &t).unwrap().title, "New");
    }

    #[test]
    fn a_started_ticket_takes_only_the_priority_and_labels_of_its_open_page() {
        let t = Ticket { status: Status::Working, ..saved(State::Live) };
        let mut v = Values::of(&t);
        assert_eq!(page_save(&v, "Other", "P", &t), None);
        assert_eq!(late_save(&v, &t), None);
        v.provider = "codex".into();
        v.toggle_label("Bug");
        let draft = late_save(&v, &t).unwrap();
        assert_eq!((draft.provider.as_str(), draft.labels), ("claude", vec!["Bug".to_string()]));
    }

    #[test]
    fn a_label_toggles_on_and_off() {
        let mut v = values();
        v.toggle_label("Bug");
        v.toggle_label("Docs");
        v.toggle_label("Bug");
        assert_eq!(v.labels, ["Docs"]);
    }

    #[test]
    fn the_values_of_a_ticket_carry_its_priority_and_labels_into_the_draft() {
        let t = Ticket { priority: 3, labels: vec!["Infra".into()], ..saved(State::Todo) };
        let d = Values::of(&t).draft(&t.title, &t.prompt);
        assert_eq!((d.priority, d.labels), (3, vec!["Infra".to_string()]));
    }

    #[test]
    fn branches_read_for_a_folder_since_left_are_dropped() {
        let mut p = Picks::new(values());
        p.set_branches("/other".into(), vec!["x".into()]);
        assert!(p.branches().is_empty());
        p.set_branches("/app".into(), vec!["main".into()]);
        assert_eq!(p.branches(), ["main"]);
        p.values.set_folder("/web".into());
        assert!(p.branches().is_empty());
    }

    #[test]
    fn the_draft_trims_the_texts_and_keeps_the_choices() {
        let d = values().draft(" T ", "\nP\n");
        assert_eq!((d.title.as_str(), d.prompt.as_str(), d.folder.as_str(), d.provider.as_str()), ("T", "P", "/app", "claude"));
    }
}
