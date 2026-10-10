use super::commits;
use crate::desktop::Desktop;
use git::Repo;
use git::github::base_branch;
use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

fn field(label: &str, input: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(ui::field_label(label.to_string()))
        .child(div().px(px(10.)).py(px(7.)).rounded(px(9.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]).text_size(px(13.)).child(input))
}

impl Desktop {
    /// The PR about to open for the worktree on screen: its title and description, who wrote them, and Draft.
    pub(crate) fn pr_composer(&self, repo: &Repo, cx: &mut Context<Self>) -> Option<Div> {
        let base = repo.base.as_deref().filter(|_| self.pr.composing == self.cwd())?;
        let (draft, busy, writing) = (self.store.git.draft, self.pr.busy, self.pr.writing);
        let agent = provider_name(&self.store.git.agent);
        let top = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .font_family(MONO)
            .whitespace_nowrap()
            .overflow_hidden()
            .child(icon("pull-request", 13., TEXT_3))
            .child(div().min_w_0().truncate().text_color(TEXT).child(repo.branch.clone()))
            .child(icon("chevron-right", 10., TEXT_4))
            .child(div().flex_none().text_color(TEXT_2).child(base_branch(base).to_string()));
        let note = self.store.git.ai.then(|| {
            let note = div().flex().items_center().gap(px(6.)).text_size(px(12.)).text_color(TEXT_3);
            if writing {
                note.child(spinner("pr-writing", 12., TEXT_3)).child(format!("{agent} is writing…"))
            } else {
                note.child(icon("sparkle", 12., TEXT_3))
                    .child(format!("Written by {agent} from {} ·", commits(repo.ahead)))
                    .child(ui::link("pr-rewrite", "Regenerate").text_size(px(12.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.write_pr(window, cx))))
            }
        });
        let error = self.pr.error.clone().map(|e| div().px(px(10.)).py(px(8.)).rounded(px(9.)).bg(FAILED_BG).font_family(MONO).text_size(px(11.5)).text_color(FAILED).child(e));
        let ready = busy.is_none() && !writing && !self.pr.title.read(cx).value().trim().is_empty();
        let label = busy.unwrap_or(if draft { "Create draft PR" } else { "Create PR" });
        let create = ui::button("pr-create", Variant::Primary, (busy.is_none()).then_some("pull-request"), label)
            .h(px(30.))
            .when_some(busy, |d, _| d.child(spinner("pr-creating", 13., ON_PRIMARY)))
            .when(!ready, |d| d.opacity(0.5).cursor_default())
            .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.create_pr(window, cx))));
        let cancel = ui::button("pr-cancel", Variant::Ghost, None, "Cancel").h(px(30.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_pr_composer(window, cx)));
        let toggle = div()
            .id("pr-draft")
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .text_size(px(12.5))
            .text_color(TEXT_2)
            .child(ui::toggle(draft))
            .child("Draft")
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.store.git.draft = !this.store.git.draft;
                this.save_soon(cx);
                cx.notify();
            }));
        let footer = div().flex().items_center().gap(px(6.)).child(toggle).child(div().flex_1()).child(cancel).child(create);
        Some(
            div().flex_none().px(px(10.)).pb(px(8.)).child(
                div()
                    .p(px(12.))
                    .flex()
                    .flex_col()
                    .gap(px(10.))
                    .rounded(px(12.))
                    .bg(SURFACE)
                    .shadow(ui::row_shadow())
                    .child(top)
                    .child(field("Title", Input::new(&self.pr.title).appearance(false).p_0().text_size(px(13.))))
                    .child(field("Description", Textarea::new(&self.pr.body).appearance(false)))
                    .children(note)
                    .children(error)
                    .child(footer),
            ),
        )
    }
}
