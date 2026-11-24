use super::{Branch, PrEvent, PrSide, commits};
use crate::git_ui::changes::commit_box::split_button;
use git::github::base_branch;
use gpui_kit::component::input::{Input, Textarea};
use gpui_kit::*;
use theme::*;
use ui::Variant;

fn field(label: &str, input: impl IntoElement) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(6.))
        .child(ui::field_label(label.to_string()))
        .child(div().px(px(10.)).py(px(7.)).rounded(px(9.)).bg(WELL).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]).text_size(px(13.)).child(input))
}

impl PrSide {
    /// The PR about to open for the worktree on screen: its title and description, who wrote them, and Draft.
    pub(crate) fn pr_composer(&self, repo: &Branch, cx: &mut Context<Self>) -> Option<Div> {
        let base = repo.base.as_deref().filter(|_| self.composing == self.frame.tree)?;
        let busy = self.busy.as_ref().filter(|(t, _)| self.composing.as_ref() == Some(t)).map(|(_, text)| *text);
        let (draft, writing) = (self.frame.draft, self.writing);
        let agent = self.frame.writer;
        let top = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .font_family(MONO)
            .whitespace_nowrap()
            .overflow_hidden()
            .child(icon("pull-request", 13., TEXT_3))
            .child(div().min_w_0().truncate().text_color(TEXT).child(repo.name.clone()))
            .child(icon("arrow-right", 11., TEXT_4))
            .child(div().flex_none().text_color(TEXT_2).child(base_branch(base).to_string()));
        let note = self.frame.ai.then(|| {
            let note = div().flex().items_center().gap(px(6.)).text_size(px(12.)).text_color(TEXT_3);
            if writing {
                note.child(spinner("pr-writing", 12., TEXT_3)).child(div().min_w_0().truncate().child(format!("{agent} is writing…")))
            } else {
                note.child(div().flex_none().child(icon("sparkle", 12., TEXT_3)))
                    .child(div().min_w_0().truncate().child(format!("Written by {agent} from {} ·", commits(repo.ahead))))
                    .child(
                        ui::link("pr-rewrite", "Regenerate")
                            .text_size(px(12.))
                            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.write_pr(window, cx))),
                    )
            }
        });
        let error = self
            .error
            .clone()
            .map(|e| div().px(px(10.)).py(px(8.)).rounded(px(9.)).bg(FAILED_BG).font_family(MONO).text_size(px(11.5)).text_color(FAILED).child(e));
        let ready = busy.is_none() && !writing && !self.title.read(cx).value().trim().is_empty();
        let label = busy.unwrap_or(if draft { "Create draft PR" } else { "Create PR" });
        let lead =
            if busy.is_some() { spinner("pr-creating", 13., ON_PRIMARY).into_any_element() } else { icon("pull-request", 14., ON_PRIMARY).into_any_element() };
        let menu = div()
            .id("pr-create-menu")
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.create_menu = false;
                cx.notify();
            }))
            .children([(false, "Create pull request"), (true, "Create draft pull request")].into_iter().map(|(choice, name)| {
                ui::menu_row(("pr-create-kind", choice as u32), "pull-request", name, None)
                    .children((choice == draft).then(|| icon("check", 13., TEXT_2)))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.create_menu = false;
                        cx.emit(PrEvent::SetDraft(choice));
                        cx.notify();
                    }))
            }));
        let create = split_button::<PrSide>(
            "pr-create",
            lead,
            label,
            ready,
            self.create_menu,
            Box::new(move |this, window, cx| {
                if ready {
                    this.create_pr(window, cx);
                }
            }),
            Box::new(|this, _, _| this.create_menu = true),
            menu,
            cx,
        )
        .flex_1();
        let cancel = ui::button("pr-cancel", Variant::Secondary, None, "Cancel")
            .h(px(30.))
            .rounded(px(9.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_composer(window, cx)));
        let buttons = div().flex().items_center().gap(px(8.)).child(cancel).child(create);
        Some(
            div()
                .flex_none()
                .px(px(16.))
                .pt(px(2.))
                .pb(px(12.))
                .flex()
                .flex_col()
                .gap(px(12.))
                .child(top)
                .child(field("Title", Input::new(&self.title).appearance(false).p_0().text_size(px(13.))))
                .child(field("Description", Textarea::new(&self.body).appearance(false)))
                .children(note)
                .children(error)
                .child(buttons),
        )
    }
}
