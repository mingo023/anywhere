use super::catalog::{Look, Setting};
use super::{Settings, SettingsEvent, action};
use crate::modals::form::home;
use crate::util::tilde;
use gpui_kit::*;
use theme::*;

fn worktree_count(n: usize) -> String {
    format!("{n} worktree{}", if n == 1 { "" } else { "s" })
}

pub(super) const ROWS: &[Setting] = &[
    Setting::path("worktree-folder", "Locations", "Worktree folder", |s| s.worktree_root(&home()), |s, p| s.worktree.root = p)
        .hint("New worktrees go in <folder>/<project>/<name>"),
    Setting::path("clone-folder", "Locations", "Clone folder", |s| s.clone_root(&home()), |s, p| s.worktree.clone_root = p)
        .hint("Where Add project › Clone puts new repositories"),
    Setting::custom("projects", "Projects", "Projects", |d, _, cx| d.project_list(cx))
        .hint("Open one to edit its icon, accounts, agents and setup script, or remove it"),
    Setting::switch("delete-branch", "Deleting a worktree", "Also delete its branch", |s| s.worktree.delete_branch, |s, on| s.worktree.delete_branch = on)
        .hint("Local branch only. Remote branches are never deleted"),
    Setting::choice(
        "worktree-files",
        "Deleting a worktree",
        "Worktree files",
        &["Move to Trash", "Delete permanently"],
        Look::Segmented,
        |s| usize::from(!s.worktree.trash),
        |s, i| s.worktree.trash = i == 0,
    ),
    Setting::stepper(
        "teardown-timeout",
        "Deleting a worktree",
        "Teardown script timeout",
        (10, 600, 10),
        " s",
        |s| s.worktree.teardown_secs as i32,
        |s, n| s.worktree.teardown_secs = n as u32,
    )
    .hint("Stops the project's teardown script if it runs longer")
    .advanced(),
];

impl Settings {
    pub(super) fn project_list(&mut self, cx: &mut Context<Self>) -> Div {
        let rows = self.side.projects.iter().enumerate().map(|(i, card)| {
            let p = card.path.clone();
            let picks = card.picks.iter().map(|(provider, name)| {
                div()
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(3.))
                    .font_family(SANS)
                    .text_color(TEXT_2)
                    .child(provider_icon(provider, 10., TEXT_3))
                    .child(name.clone())
            });
            let text = div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(card.name.clone()))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .min_w_0()
                        .text_size(px(11.5))
                        .text_color(TEXT_3)
                        .child(div().min_w_0().truncate().font_family(MONO).child(tilde(&card.path)))
                        .children(picks.map(|pick| div().flex().flex_none().items_center().gap(px(6.)).child("\u{b7}").child(pick))),
                );
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
                .child(card.tile(24.))
                .child(text)
                .children(card.worktrees.map(|n| div().flex_none().text_size(px(12.)).text_color(TEXT_3).child(worktree_count(n))))
                .child(icon("chevron-right", 13., TEXT_4))
                .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::EditProject(p.clone()))))
        });
        let add = action("settings-add-project", "")
            .pl(px(4.))
            .gap(px(6.))
            .child(icon("plus", 13., TEXT_2))
            .child("Add project…")
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::AddProject)));
        let clone = ui::link("settings-clone-project", "Clone…").on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::CloneProject)));
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
