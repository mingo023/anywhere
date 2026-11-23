use super::{Changes, ChangesEvent, Offer};
use crate::git_ui::pull_requests;
use crate::renames;
use git::Repo;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::icon_button_sized;

impl Changes {
    pub(super) fn changes_header(&self, repo: &Repo, cx: &mut Context<Self>) -> Div {
        let view = icon_button_sized("changes-view", if self.inputs.tree { "list-flat" } else { "list-tree" }, 26., TEXT_3)
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(ChangesEvent::ToggleTree)));
        let open = self.menu;
        let more = ui::popover_trigger(
            icon_button_sized("changes-more", "more", 26., TEXT_3).when(open, |d| d.bg(FILL_3)),
            open,
            cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.menu = true;
                this.commit_menu = false;
                cx.notify();
            }),
        );
        let pr = self.inputs.pr.as_ref().map(|(pr, color)| {
            pull_requests::chip("changes-pr", pr, *color).cursor_pointer().on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(ChangesEvent::ShowPr)))
        });
        div()
            .h(px(40.))
            .flex_none()
            .pl(px(16.))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(8.))
            .child(div().flex_none().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child("Changes"))
            .child(div().flex_1().min_w_0().flex().items_center().gap(px(4.)).text_color(TEXT_3).child(icon("branch", 12., TEXT_3)).child(
                div().truncate().font_family(MONO).text_size(px(12.)).child(match &self.inputs.cwd {
                    Some(tree) => {
                        let was = self.inputs.branch_was.as_ref().map(|(from, at)| (from.as_str(), *at));
                        renames::decoding(format!("changes-branch:{tree}:{}", repo.branch), was, &repo.branch, TEXT_3, cx.reduce_motion())
                            .unwrap_or_else(|| repo.branch.clone().into_any_element())
                    }
                    None => repo.branch.clone().into_any_element(),
                }),
            ))
            .children(pr)
            .child(view)
            .child(div().relative().child(more).when(open, |d| d.child(ui::dropdown(30., ui::menu_in("changes-menu-in", self.changes_menu_view(repo, cx))))))
    }

    fn changes_menu_view(&self, repo: &Repo, cx: &mut Context<Self>) -> Stateful<Div> {
        let info = |text: String| div().h(px(26.)).px(px(10.)).flex().items_center().text_size(px(12.5)).text_color(TEXT_3).child(text);
        let base = repo.base.as_ref().map(|b| format!("{} → {b}", repo.branch));
        let counts = repo.base.is_some().then(|| format!("{} ahead · {} behind", repo.ahead, repo.behind));
        let pr = self.inputs.offer.as_ref().map(|offer| match offer {
            Offer::Open(url) => {
                let url = url.clone();
                ui::menu_row("changes-open-pr", "external", "Open pull request", None)
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.menu = false;
                        cx.emit(ChangesEvent::OpenPr(url.clone()));
                        cx.notify();
                    }))
                    .into_any_element()
            }
            Offer::Create => ui::menu_row("changes-create-pr", "plus", "Create pull request", None)
                .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(ChangesEvent::CreatePr)))
                .into_any_element(),
            Offer::Hint(text) => info(text.to_string()).into_any_element(),
        });
        ui::pop(div().id("changes-menu"))
            .w(px(220.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.menu = false;
                cx.notify();
            }))
            .child(ui::menu_row("changes-push", "arrow-up", "Push", None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.push(cx))))
            .children(pr)
            .when(base.is_some(), |d| d.child(ui::menu_divider()))
            .children(base.map(info))
            .children(counts.map(info))
    }
}
