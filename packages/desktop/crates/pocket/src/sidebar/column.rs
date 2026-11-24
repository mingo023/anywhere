use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, Screen};
use git::Repo;
use gpui_kit::*;
use theme::*;

/// Whether the worktree has any changed file.
pub(crate) fn has_changes(repo: Option<&Repo>) -> bool {
    repo.is_some_and(|r| !r.files.is_empty())
}

/// Marks a sidebar toggle while the hidden Changes list has something in it.
pub(crate) fn changes_dot() -> Div {
    ui::dot(6., ACCENT).absolute().top(px(5.)).right(px(5.)).shadow(vec![ui::ring(RAIL_CUTOUT, 1.5)])
}

/// How many files the Changes list holds, at the bottom right of its 32px rail icon.
pub(crate) fn changes_badge(n: usize) -> Div {
    ui::count_badge(n).top(px(19.)).right(px(-4.)).bg(PRIMARY).text_color(ON_PRIMARY).shadow(vec![ui::ring(RAIL_CUTOUT, 2.)])
}

impl Desktop {
    /// The inbox or Automations list left of the page.
    pub(crate) fn column_view(&mut self, cx: &mut Context<Self>) -> Div {
        let width = self.width(Column::Sessions, 348.);
        let list = if self.screen == Screen::Automations {
            self.automations_column(cx).w(px(width))
        } else {
            div().h_full().flex_none().child(self.inbox_list(width, cx))
        };
        self.resizable(list, Column::Sessions, cx)
    }
}

#[cfg(test)]
mod tests {
    use super::has_changes;
    use git::{FileStat, Repo};

    #[test]
    fn a_clean_worktree_has_no_changes() {
        assert!(!has_changes(Some(&Repo::default())));
        assert!(!has_changes(None));
    }

    #[test]
    fn binary_changes_alone_still_mark_the_worktree() {
        let repo = Repo {
            files: vec![FileStat { path: "a.rs".into(), added: 0, removed: 0, staged: false, unstaged: true, status: 'M', old_path: None }],
            ..Default::default()
        };
        assert!(has_changes(Some(&repo)));
    }
}
