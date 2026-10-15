use super::action;
use super::catalog::{Look, Setting};
use crate::desktop::Desktop;
use crate::modals::form::home;
use crate::util::{initials, tilde};
use gpui_kit::*;
use theme::*;

fn worktree_count(n: usize) -> String {
    format!("{n} worktree{}", if n == 1 { "" } else { "s" })
}

pub(super) const ROWS: &[Setting] = &[
    Setting::path("worktree-folder", "Locations", "Worktree folder", |s| s.worktree_root(&home()), |s, p| s.worktree.root = p).hint("New worktrees go in <folder>/<project>/<name>"),
    Setting::path("clone-folder", "Locations", "Clone folder", |s| s.clone_root(&home()), |s, p| s.worktree.clone_root = p).hint("Where Add project › Clone puts new repositories"),
    Setting::custom("projects", "Projects", "Projects", |d, _, cx| d.project_list(cx)).hint("Open one to edit its icon and setup script, or remove it"),
    Setting::switch("delete-branch", "Deleting a worktree", "Also delete its branch", |s| s.worktree.delete_branch, |s, on| s.worktree.delete_branch = on).hint("Local branch only. Remote branches are never deleted"),
    Setting::choice("worktree-files", "Deleting a worktree", "Worktree files", &["Move to Trash", "Delete permanently"], Look::Segmented, |s| usize::from(!s.worktree.trash), |s, i| s.worktree.trash = i == 0),
    Setting::stepper("teardown-timeout", "Deleting a worktree", "Teardown script timeout", (10, 600, 10), " s", |s| s.worktree.teardown_secs as i32, |s, n| s.worktree.teardown_secs = n as u32)
        .hint("Stops the project's teardown script if it runs longer")
        .advanced(),
];

impl Desktop {
    pub(super) fn project_tile(&self, p: &str, size: f32) -> Div {
        ui::colour_tile(&self.project_mark(p, initials(&self.repo_name(p))), self.repo_color(p), size, size / 4.)
    }

    fn project_list(&mut self, cx: &mut Context<Self>) -> Div {
        let rows = self.store.projects.clone().into_iter().enumerate().map(|(i, p)| {
            let name = self.repo_name(&p);
            let tile = self.project_tile(&p, 24.);
            let text = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(name))
                .child(div().truncate().font_family(MONO).text_size(px(11.5)).text_color(TEXT_3).child(tilde(&p)));
            let count = self.worktrees.get(&p).map(Vec::len);
            div()
                .id(("settings-project", i))
                .px(px(14.))
                .py(px(9.))
                .flex()
                .items_center()
                .gap(px(12.))
                .border_b(px(0.5))
                .border_color(SEPARATOR)
                .cursor_pointer()
                .hover(|d| d.bg(FILL_1))
                .child(tile)
                .child(text)
                .children(count.map(|n| div().flex_none().text_size(px(12.)).text_color(TEXT_3).child(worktree_count(n))))
                .child(icon("chevron-right", 13., TEXT_4))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.edit_project(Some(p.clone()), window, cx)))
        });
        let add = action("settings-add-project", "")
            .pl(px(4.))
            .gap(px(6.))
            .child(icon("plus", 13., TEXT_2))
            .child("Add project…")
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.add_local_project(window, cx)));
        let clone = ui::link("settings-clone-project", "Clone…").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.clone_project(window, cx)));
        let foot = div().px(px(14.)).py(px(8.)).flex().items_center().gap(px(12.)).bg(FILL_1).child(add).child(clone);
        div().flex().flex_col().children(rows).child(foot)
    }
}

#[cfg(test)]
mod tests {
    use super::worktree_count;

    #[test]
    fn one_worktree_is_singular() {
        assert_eq!([worktree_count(1), worktree_count(3)], ["1 worktree", "3 worktrees"]);
    }
}
