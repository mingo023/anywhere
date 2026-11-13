use crate::status::Status;
use std::cmp::Reverse;
use std::collections::BTreeSet;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Col {
    Idle,
    Working,
    NeedsYou,
    Done,
    InReview,
}

pub(crate) const COLS: [Col; 5] = [Col::Idle, Col::Working, Col::NeedsYou, Col::Done, Col::InReview];

impl Col {
    pub(crate) fn name(self) -> &'static str {
        match self {
            Col::Idle => "Idle",
            Col::Working => "Working",
            Col::NeedsYou => "Needs you",
            Col::Done => "Done",
            Col::InReview => "In review",
        }
    }

    /// What it says while it holds nothing.
    pub(crate) fn hint(self) -> &'static str {
        match self {
            Col::Idle => "Sessions at the prompt with nothing unseen.",
            Col::Working => "Sessions mid-turn show up here.",
            Col::NeedsYou => "Questions, permissions and failed turns land here.",
            Col::Done => "Ended turns you haven't seen yet.",
            Col::InReview => "Idle sessions whose branch has an open PR.",
        }
    }
}

/// One session as the page shows it, gathered from the agent list, its terminal and its worktree.
#[derive(Clone, PartialEq, Debug, Default)]
pub(crate) struct Item {
    pub id: String,
    /// `None` while the agent isn't attached: it reports only terminal activity.
    pub status: Option<Status>,
    /// Its worktree's branch has an open or draft PR.
    pub pr_open: bool,
    pub at: i64,
    pub title: String,
    /// The project's path, and the name the sidebar shows for it.
    pub project: String,
    pub project_name: String,
    pub branch: String,
    /// Its worktree's path, which PRs and diffs are kept under; empty outside a worktree.
    pub tree: String,
    pub provider: String,
    /// The pending ask, the error, what it's doing or what it last said, by status.
    pub note: String,
}

/// Status decides; an open PR files a session with nothing for you under In review, where it waits on other people.
pub(crate) fn column(status: Option<Status>, pr_open: bool) -> Col {
    match status {
        Some(Status::NeedsYou | Status::Failed) => Col::NeedsYou,
        Some(Status::Working) => Col::Working,
        Some(Status::Done) => Col::Done,
        Some(Status::Idle) | None if pr_open => Col::InReview,
        Some(Status::Idle) | None => Col::Idle,
    }
}

/// What the session's status reads as: its column, but a failed turn says so.
pub(crate) fn label(status: Option<Status>, pr_open: bool) -> &'static str {
    if status == Some(Status::Failed) { "Failed" } else { column(status, pr_open).name() }
}

pub(crate) type Columns = [Vec<usize>; COLS.len()];

/// The sessions as last gathered, and the columns their kept ones fill.
#[derive(Default)]
pub(crate) struct Grouped {
    pub items: Vec<Item>,
    pub columns: Columns,
}

impl Grouped {
    /// Takes the sessions as they are now; false when nothing shown changed.
    pub(crate) fn take(&mut self, items: Vec<Item>, filters: &Filters) -> bool {
        let columns = arrange(&items, filters);
        if items == self.items && columns == self.columns {
            return false;
        }
        (self.items, self.columns) = (items, columns);
        true
    }

    /// The preview, while its session is still here.
    pub(crate) fn keep(&self, previewed: Option<String>) -> Option<String> {
        previewed.filter(|id| self.items.iter().any(|i| &i.id == id))
    }

    pub(crate) fn shown(&self, col: Col, ix: usize) -> Option<&Item> {
        self.columns[col as usize].get(ix).and_then(|&i| self.items.get(i))
    }

    pub(crate) fn item(&self, id: &str) -> Option<&Item> {
        self.items.iter().find(|i| i.id == id)
    }
}

#[derive(Clone, Default, PartialEq, Debug)]
pub(crate) struct Filters {
    pub query: String,
    /// Project paths; empty keeps every project.
    pub projects: BTreeSet<String>,
    /// Agent providers; empty keeps every agent.
    pub providers: BTreeSet<String>,
}

impl Filters {
    pub(crate) fn keeps(&self, item: &Item) -> bool {
        let text = format!("{} {} {}", item.title, item.project_name, item.branch).to_lowercase();
        (self.projects.is_empty() || self.projects.contains(&item.project))
            && (self.providers.is_empty() || self.providers.contains(&item.provider))
            && self.query.split_whitespace().all(|w| text.contains(&w.to_lowercase()))
    }

    pub(crate) fn picks(&self, menu: Menu) -> &BTreeSet<String> {
        match menu {
            Menu::Project => &self.projects,
            Menu::Agent => &self.providers,
        }
    }

    /// Picks `value` in `menu`, or lets go of it when it was picked.
    pub(crate) fn toggle(&mut self, menu: Menu, value: &str) {
        let set = match menu {
            Menu::Project => &mut self.projects,
            Menu::Agent => &mut self.providers,
        };
        if !set.remove(value) {
            set.insert(value.to_string());
        }
    }
}

/// The filter menus in the page's bar.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Menu {
    Project,
    Agent,
}

impl Menu {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Menu::Project => "Project",
            Menu::Agent => "Agent",
        }
    }

    /// What the menu filters an item by: its project's path or its agent's provider.
    pub(crate) fn value(self, item: &Item) -> &String {
        match self {
            Menu::Project => &item.project,
            Menu::Agent => &item.provider,
        }
    }
}

/// The values a filter menu offers, each with how many sessions have it, in the order they first appear.
pub(crate) fn values(items: &[Item], menu: Menu) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for item in items {
        match out.iter_mut().find(|(v, _)| v == menu.value(item)) {
            Some((_, n)) => *n += 1,
            None => out.push((menu.value(item).clone(), 1)),
        }
    }
    out
}

/// What Esc does on Activity, nearest thing first.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Escape {
    LeaveSearch,
    ClosePreview,
    LeavePage,
}

pub(crate) fn escape(searching: bool, previewing: bool) -> Escape {
    match (searching, previewing) {
        (true, _) => Escape::LeaveSearch,
        (false, true) => Escape::ClosePreview,
        (false, false) => Escape::LeavePage,
    }
}

/// ↵ or ⌘↵ opens the previewed session; other modifiers leave the key alone.
pub(crate) fn opens_preview(key: &str, plain: bool, command: bool) -> bool {
    key == "enter" && (plain || command)
}

/// Each column's kept items as indexes into `items`: what wants you oldest first, as Up next orders it; the rest newest first.
pub(crate) fn arrange(items: &[Item], filters: &Filters) -> Columns {
    let mut out: Columns = Default::default();
    for (i, item) in items.iter().enumerate().filter(|(_, item)| filters.keeps(item)) {
        out[column(item.status, item.pr_open) as usize].push(i);
    }
    for (col, line) in COLS.iter().zip(out.iter_mut()) {
        match col {
            Col::NeedsYou => line.sort_by_key(|&i| (items[i].status, items[i].at)),
            Col::Working | Col::Done => line.sort_by_key(|&i| items[i].at),
            Col::Idle | Col::InReview => line.sort_by_key(|&i| Reverse(items[i].at)),
        }
    }
    out
}

/// Opening a session's preview marks it seen when an unseen turn is all it was waiting on.
pub(crate) fn marks_seen(status: Option<Status>) -> bool {
    matches!(status, Some(Status::Done | Status::Failed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, status: Option<Status>, pr_open: bool, at: i64) -> Item {
        Item { id: id.into(), status, pr_open, at, ..Item::default() }
    }

    fn ids(items: &[Item], columns: &Columns, col: Col) -> Vec<String> {
        columns[col as usize].iter().map(|&i| items[i].id.clone()).collect()
    }

    #[test]
    fn a_session_sits_in_the_column_its_status_says() {
        assert_eq!(column(Some(Status::NeedsYou), false), Col::NeedsYou);
        assert_eq!(column(Some(Status::Working), false), Col::Working);
        assert_eq!(column(Some(Status::Done), false), Col::Done);
        assert_eq!(column(Some(Status::Idle), false), Col::Idle);
    }

    // Failed asks for a look like Needs you does.
    #[test]
    fn a_failed_turn_needs_you() {
        assert_eq!(column(Some(Status::Failed), false), Col::NeedsYou);
    }

    // An open PR only matters once the agent has nothing for you.
    #[test]
    fn an_idle_session_with_an_open_pr_is_in_review() {
        assert_eq!(column(Some(Status::Idle), true), Col::InReview);
        assert_eq!(column(None, true), Col::InReview);
        assert_eq!(column(None, false), Col::Idle);
    }

    #[test]
    fn a_pr_never_hides_work_or_a_question() {
        assert_eq!(column(Some(Status::Working), true), Col::Working);
        assert_eq!(column(Some(Status::NeedsYou), true), Col::NeedsYou);
        assert_eq!(column(Some(Status::Done), true), Col::Done);
    }

    #[test]
    fn what_wants_you_comes_oldest_first_and_the_rest_newest_first() {
        let items = vec![
            item("done-new", Some(Status::Done), false, 30),
            item("done-old", Some(Status::Done), false, 10),
            item("idle-old", Some(Status::Idle), false, 10),
            item("idle-new", Some(Status::Idle), false, 30),
            item("work-new", Some(Status::Working), false, 30),
            item("work-old", Some(Status::Working), false, 10),
        ];
        let columns = arrange(&items, &Filters::default());
        assert_eq!(ids(&items, &columns, Col::Done), ["done-old", "done-new"]);
        assert_eq!(ids(&items, &columns, Col::Working), ["work-old", "work-new"]);
        assert_eq!(ids(&items, &columns, Col::Idle), ["idle-new", "idle-old"]);
    }

    #[test]
    fn needs_you_lists_questions_before_failed_turns() {
        let items = vec![item("failed", Some(Status::Failed), false, 1), item("asks", Some(Status::NeedsYou), false, 9)];
        assert_eq!(ids(&items, &arrange(&items, &Filters::default()), Col::NeedsYou), ["asks", "failed"]);
    }

    // Previewing clears what was waiting to be seen, never a question.
    #[test]
    fn opening_marks_only_a_done_or_failed_turn_seen() {
        assert!(marks_seen(Some(Status::Done)));
        assert!(marks_seen(Some(Status::Failed)));
        assert!(!marks_seen(Some(Status::NeedsYou)));
        assert!(!marks_seen(Some(Status::Working)));
        assert!(!marks_seen(Some(Status::Idle)));
        assert!(!marks_seen(None));
    }

    fn named(id: &str, project: &str, provider: &str, branch: &str) -> Item {
        Item { id: id.into(), title: format!("Fix {id}"), project: format!("/repos/{project}"), project_name: project.into(), provider: provider.into(), branch: branch.into(), ..Item::default() }
    }

    #[test]
    fn no_filter_keeps_every_session() {
        assert!(Filters::default().keeps(&named("a", "app", "claude", "main")));
    }

    #[test]
    fn a_project_filter_keeps_only_its_projects() {
        let f = Filters { projects: ["/repos/app".to_string()].into(), ..Filters::default() };
        assert!(f.keeps(&named("a", "app", "claude", "")));
        assert!(!f.keeps(&named("b", "web", "claude", "")));
    }

    #[test]
    fn an_agent_filter_keeps_only_its_agents() {
        let f = Filters { providers: ["codex".to_string()].into(), ..Filters::default() };
        assert!(f.keeps(&named("a", "app", "codex", "")));
        assert!(!f.keeps(&named("b", "app", "claude", "")));
    }

    #[test]
    fn search_matches_every_word_in_title_project_or_branch() {
        let item = named("parser", "app", "claude", "fix/lexer");
        let q = |s: &str| Filters { query: s.into(), ..Filters::default() };
        assert!(q("PARSER app").keeps(&item));
        assert!(q("lexer").keeps(&item));
        assert!(!q("parser web").keeps(&item));
    }

    #[test]
    fn filtered_out_sessions_leave_their_column() {
        let items = vec![named("a", "app", "claude", ""), named("b", "web", "claude", "")];
        let f = Filters { projects: ["/repos/web".to_string()].into(), ..Filters::default() };
        assert_eq!(ids(&items, &arrange(&items, &f), Col::Idle), ["b"]);
    }

    #[test]
    fn a_menu_counts_each_value_once_in_first_seen_order() {
        let items = vec![named("a", "web", "claude", ""), named("b", "app", "codex", ""), named("c", "web", "codex", "")];
        assert_eq!(values(&items, Menu::Agent), [("claude".to_string(), 1), ("codex".to_string(), 2)]);
        assert_eq!(values(&items, Menu::Project), [("/repos/web".to_string(), 2), ("/repos/app".to_string(), 1)]);
    }

    #[test]
    fn picking_a_value_again_lets_go_of_it() {
        let mut f = Filters::default();
        f.toggle(Menu::Agent, "codex");
        assert!(f.picks(Menu::Agent).contains("codex") && f.picks(Menu::Project).is_empty());
        f.toggle(Menu::Agent, "codex");
        assert_eq!(f, Filters::default());
    }

    #[test]
    fn esc_lets_go_of_search_then_closes_the_preview_then_leaves() {
        assert_eq!(escape(true, true), Escape::LeaveSearch);
        assert_eq!(escape(false, true), Escape::ClosePreview);
        assert_eq!(escape(false, false), Escape::LeavePage);
    }

    #[test]
    fn enter_opens_the_preview_bare_or_with_command_only() {
        assert!(opens_preview("enter", true, false));
        assert!(opens_preview("enter", false, true));
        assert!(!opens_preview("enter", false, false));
        assert!(!opens_preview("space", true, false));
    }

    #[test]
    fn the_same_sessions_again_change_nothing() {
        let mut g = Grouped::default();
        let items = vec![named("a", "app", "claude", "")];
        assert!(g.take(items.clone(), &Filters::default()));
        assert!(!g.take(items, &Filters::default()));
    }

    #[test]
    fn a_new_filter_regroups_the_same_sessions() {
        let mut g = Grouped::default();
        let items = vec![named("a", "app", "claude", ""), named("b", "web", "claude", "")];
        g.take(items.clone(), &Filters::default());
        let f = Filters { projects: ["/repos/web".to_string()].into(), ..Filters::default() };
        assert!(g.take(items, &f));
        assert_eq!(g.shown(Col::Idle, 0).map(|i| i.id.as_str()), Some("b"));
        assert!(g.shown(Col::Idle, 1).is_none());
    }

    #[test]
    fn a_preview_closes_once_its_session_is_gone() {
        let mut g = Grouped::default();
        g.take(vec![named("a", "app", "claude", "")], &Filters::default());
        assert_eq!(g.keep(Some("a".into())), Some("a".into()));
        g.take(Vec::new(), &Filters::default());
        assert_eq!(g.keep(Some("a".into())), None);
    }
}
