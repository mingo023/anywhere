use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, HEADER, Overlay, Screen, Side, drag_area};
use crate::util::basename;
use git::Repo;
use gpui_kit::*;
use theme::*;
use ui::{self, icon_button_sized};

/// Whether the worktree has any changed file.
pub(crate) fn has_changes(repo: Option<&Repo>) -> bool {
    repo.is_some_and(|r| !r.files.is_empty())
}

/// Marks a sidebar toggle while the hidden Changes list has something in it.
pub(crate) fn changes_dot() -> Div {
    ui::dot(6., ACCENT).absolute().top(px(5.)).right(px(5.)).shadow(vec![ui::ring(Token::new(0xfafafaff, 0x171717ff), 1.5)])
}

impl Desktop {
    /// The inbox or Automations list left of the page.
    pub(crate) fn column_view(&mut self, cx: &mut Context<Self>) -> Div {
        let list = if self.screen == Screen::Automations { self.automations_column(cx) } else { self.inbox_list(cx) };
        let list = list.w(px(self.width(Column::Sessions, 348.)));
        self.resizable(list, Column::Sessions, cx)
    }

    /// The Workspace panel's content: the side the rail picked, under the worktree's title.
    pub(crate) fn workspace_column(&mut self, cx: &mut Context<Self>) -> Div {
        let body = match self.side {
            Side::Sessions => self.session_list(cx).into_any_element(),
            Side::Explorer => self.explorer(cx).into_any_element(),
            Side::Changes => self.changes_list(cx).into_any_element(),
        };
        div()
            .bg(SIDE)
            .h_full()
            .flex()
            .flex_col()
            .overflow_hidden()
            .child(drag_area(self.column_header(cx)).h(px(HEADER)).flex_none().border_b(px(0.5)).border_color(SEPARATOR))
            .child(body)
    }

    fn column_header(&self, cx: &mut Context<Self>) -> Div {
        let title = self.cwd().map(|t| basename(&t)).or_else(|| self.project.as_deref().map(|p| self.repo_name(p))).unwrap_or_default();
        let add = icon_button_sized("column-add", "plus", 28., TEXT_2)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx)));
        div()
            .pl(px(18.))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().text_size(px(16.)).font_weight(FontWeight::BOLD).truncate().child(title))
            .child(add)
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
        let repo = Repo { files: vec![FileStat { path: "a.rs".into(), added: 0, removed: 0, staged: false, unstaged: true, status: 'M' }], ..Default::default() };
        assert!(has_changes(Some(&repo)));
    }
}
