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

pub(crate) type Columns = [Vec<usize>; COLS.len()];

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
}

/// The values a filter menu offers, each with how many sessions have it, in the order they first appear.
pub(crate) fn values(items: &[Item], value: fn(&Item) -> &String) -> Vec<(String, usize)> {
    let mut out: Vec<(String, usize)> = Vec::new();
    for item in items {
        match out.iter_mut().find(|(v, _)| v == value(item)) {
            Some((_, n)) => *n += 1,
            None => out.push((value(item).clone(), 1)),
        }
    }
    out
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
        assert_eq!(values(&items, |i| &i.provider), [("claude".to_string(), 1), ("codex".to_string(), 2)]);
        assert_eq!(values(&items, |i| &i.project), [("/repos/web".to_string(), 2), ("/repos/app".to_string(), 1)]);
    }
}
