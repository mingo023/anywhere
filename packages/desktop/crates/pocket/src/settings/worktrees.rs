use super::{group, row};
use crate::desktop::Desktop;
use crate::modals::form::home;
use crate::util::tilde;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(crate) fn worktree_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let shown = tilde(&self.store.worktree_root(&home()));
        let folder = ui::field_box()
            .w(px(220.))
            .child(icon("folder", 13., TEXT_3))
            .child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(13.)).child(shown.clone()))
            .child(ui::link("settings-root-change", "Change…").on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                this.pick_path(true, window, cx, |d, paths, _, cx| {
                    if let Some(p) = paths.into_iter().next() {
                        d.store.worktree.root = p.to_string_lossy().into_owned();
                        d.save_soon(cx);
                    }
                });
            })))
            .when(!self.store.worktree.root.is_empty(), |d| {
                d.child(ui::link("settings-root-reset", "Use default").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.store.worktree.root.clear();
                    this.save_soon(cx);
                    cx.notify();
                })))
            });
        let hint = format!("New worktrees go in {shown}/<project>/<name>. Existing ones stay put.");
        let defaults = group("New worktrees", vec![row("Folder", Some(&hint), folder)]);
        let projects = self.store.projects.clone().into_iter().enumerate().map(|(i, p)| {
            let custom = self.store.repos.get(&p).is_some_and(|r| !r.worktrees.is_empty());
            let dir = tilde(&self.store.worktrees_dir(&p, &home()));
            let name = div().flex().items_center().gap(px(8.)).child(ui::swatch(self.repo_color(&p), 10., 3.)).child(self.repo_name(&p));
            let edit = ui::link(("settings-project-edit", i), "Edit…").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.edit_project(Some(p.clone()), window, cx)));
            row(name, Some(&dir), div().flex().items_center().gap(px(8.)).when(custom, |d| d.child(ui::tag("custom"))).child(edit))
        });
        let projects = group("Projects", projects.collect());
        div().flex().flex_col().gap(px(24.)).child(defaults).when(!self.store.projects.is_empty(), |d| d.child(projects))
    }
}
