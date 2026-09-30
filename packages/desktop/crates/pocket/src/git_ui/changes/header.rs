use crate::desktop::Desktop;
use git::Repo;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::icon_button_sized;

impl Desktop {
    pub(super) fn changes_header(&self, repo: &Repo, cx: &mut Context<Self>) -> Div {
        let view = icon_button_sized("changes-view", if self.changes.tree { "list-flat" } else { "list-tree" }, 26., TEXT_3).on_click(cx.listener(
            |this, _: &ClickEvent, _, cx| {
                this.changes.tree = !this.changes.tree;
                cx.notify();
            },
        ));
        let open = self.changes.menu;
        // Runs before the open menu's click-outside handler, which would otherwise close it only for this click to reopen it.
        let more = icon_button_sized("changes-more", "more", 26., TEXT_3).when(open, |d| d.bg(FILL_3)).capture_any_mouse_down(cx.listener(
            |this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.changes.menu = !this.changes.menu;
                this.changes.commit_menu = false;
                cx.notify();
            },
        ));
        div()
            .h(px(40.))
            .flex_none()
            .pl(px(16.))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().flex_none().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child("Changes"))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .flex()
                    .items_center()
                    .gap(px(4.))
                    .text_color(TEXT_3)
                    .child(icon("branch", 12., TEXT_3))
                    .child(div().truncate().font_family(MONO).text_size(px(12.)).child(repo.branch.clone())),
            )
            .child(view)
            .child(div().relative().child(more).when(open, |d| d.child(ui::dropdown(30., self.changes_menu_view(repo, cx)))))
    }

    fn changes_menu_view(&self, repo: &Repo, cx: &mut Context<Self>) -> Stateful<Div> {
        let info = |text: String| div().h(px(26.)).px(px(10.)).flex().items_center().text_size(px(12.5)).text_color(TEXT_3).child(text);
        let base = repo.base.as_ref().map(|b| format!("{} → {b}", repo.branch));
        let counts = repo.base.is_some().then(|| format!("{} ahead · {} behind", repo.ahead, repo.behind));
        ui::pop(div().id("changes-menu"))
            .w(px(220.))
            .p(px(6.))
            .rounded(px(14.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.changes.menu = false;
                cx.notify();
            }))
            .child(ui::menu_row("changes-push", "arrow-up", "Push", None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.push(cx))))
            .when(base.is_some(), |d| d.child(ui::menu_divider()))
            .children(base.map(info))
            .children(counts.map(info))
    }
}
