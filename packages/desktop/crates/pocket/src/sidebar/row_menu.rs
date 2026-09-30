use crate::desktop::Desktop;
use crate::desktop::chrome::{RowMenu, id};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, icon_button_sized};

impl Desktop {
    pub(super) fn open_row_menu(menu: RowMenu, cx: &mut Context<Self>) -> impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static {
        cx.listener(move |this, _: &MouseDownEvent, _, cx| {
            this.close_menus();
            this.row_menu = Some(menu.clone());
            cx.notify();
        })
    }

    pub(super) fn row_menu_button(&self, key: &str, menu: RowMenu, cx: &mut Context<Self>) -> AnyElement {
        let open = self.row_menu.as_ref() == Some(&menu);
        let toggle = menu.clone();
        let button = icon_button_sized(id(format!("aside-more:{key}")), "more", 22., TEXT_3).rounded(px(6.)).capture_any_mouse_down(cx.listener(
            move |this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.row_menu = (this.row_menu.as_ref() != Some(&toggle)).then(|| toggle.clone());
                this.terminal.tab_menu = false;
                cx.notify();
            },
        ));
        div()
            .relative()
            .child(button)
            .when(open, |d| {
                d.child(ui::dropdown(
                    26.,
                    ui::pop(div().id("aside-menu")).w(px(210.)).p(px(6.)).rounded(px(14.)).flex().flex_col()
                        .occlude()
                        .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                            this.row_menu = None;
                            cx.notify();
                        }))
                        .children(self.row_menu_items(&menu, cx)),
                ))
            })
            .into_any_element()
    }

    fn row_menu_items(&self, menu: &RowMenu, cx: &mut Context<Self>) -> Vec<AnyElement> {
        match menu.clone() {
            RowMenu::Project(p) => {
                let kept = self.store.projects.contains(&p);
                let target = p.clone();
                let first = if kept {
                    ui::menu_row("aside-menu-settings", "settings", "Settings…", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.select_project(target.clone(), cx);
                        this.project_settings(&crate::actions::ProjectSettings, window, cx);
                    }))
                } else {
                    ui::menu_row("aside-menu-keep", "check", "Keep in Pocket", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.row_menu = None;
                        this.keep_project(&target, cx);
                    }))
                };
                vec![
                    first.into_any_element(),
                    ui::menu_divider().into_any_element(),
                    ui::danger_row("aside-menu-remove", "x", if kept { "Remove from Pocket" } else { "Remove" })
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.row_menu = None;
                            this.ask_remove_project(p.clone(), cx);
                        }))
                        .into_any_element(),
                ]
            }
            RowMenu::Tree { project, tree } => vec![
                ui::danger_row("aside-menu-delete", "trash", "Delete worktree…")
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.row_menu = None;
                        this.ask_delete_worktree(project.clone(), tree.clone(), cx);
                    }))
                    .into_any_element(),
            ],
        }
    }
}
