use super::{NewForm, NewFormEvent, Target};
use crate::chat::composer::drop_zone;
use crate::chat::composer::send_button::send_button;
use crate::chat::composer::suggest::{Press, launch_list};
use crate::composer::Frame;
use crate::composer::rich::rich_keys;
use crate::field::{self, with_field};
use chat::Action;
use gpui_kit::component::input::{Enter, Escape, IndentInline, MoveDown, MoveUp, Textarea};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl NewForm {
    fn suggesting(&self, cx: &App) -> (Option<String>, String) {
        (self.side.as_ref().and_then(|s| s.suggest_key.clone()), self.prompt.read(cx).value().to_string())
    }

    fn walk(&mut self, down: bool, cx: &mut Context<Self>) -> bool {
        let (key, text) = self.suggesting(cx);
        let taken = self.suggest.walk(key.as_deref(), &text, down);
        if taken {
            cx.notify();
        }
        taken
    }

    fn take_pick(&mut self, cx: &mut Context<Self>) -> bool {
        let (key, text) = self.suggesting(cx);
        let Some(i) = self.suggest.chosen(key.as_deref(), &text) else { return false };
        cx.emit(NewFormEvent::Suggest(i));
        true
    }

    fn dismiss_list(&mut self, cx: &mut Context<Self>) -> bool {
        let (key, text) = self.suggesting(cx);
        let taken = self.suggest.dismiss(key.as_deref(), &text);
        if taken {
            cx.notify();
        }
        taken
    }

    /// Takes the list's keys while it shows, before the input's own bindings do.
    fn suggest_keys(&self, d: Div, cx: &mut Context<Self>) -> Div {
        let stop = |cx: &mut Context<Self>, taken: bool| {
            if taken {
                cx.stop_propagation();
            }
        };
        d.capture_action(cx.listener(move |this, _: &MoveUp, _, cx| {
            let taken = this.walk(false, cx);
            stop(cx, taken);
        }))
        .capture_action(cx.listener(move |this, _: &MoveDown, _, cx| {
            let taken = this.walk(true, cx);
            stop(cx, taken);
        }))
        .capture_action(cx.listener(move |this, _: &IndentInline, _, cx| {
            let taken = this.take_pick(cx);
            stop(cx, taken);
        }))
        .capture_action(cx.listener(move |this, e: &Enter, _, cx| {
            let taken = !e.secondary && !e.shift && this.take_pick(cx);
            stop(cx, taken);
        }))
        .capture_action(cx.listener(move |this, _: &Escape, _, cx| {
            let taken = this.dismiss_list(cx);
            stop(cx, taken);
        }))
    }
}

impl Render for NewForm {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(side) = &self.side else { return div() };
        let f = &self.draft;
        let machine = self.machine_select(side, cx);
        let notice = self.machine_notice(side, cx);
        let project = self.project_select(side, cx);
        let scratch = f.scratch;
        let checkout = (!scratch).then(|| self.checkout_select(side, cx));
        let pr = f.linked_pr();
        let branch = match pr {
            Some(n) => Some(
                div()
                    .h(px(30.))
                    .px(px(6.))
                    .flex()
                    .flex_none()
                    .items_center()
                    .gap(px(7.))
                    .text_color(TEXT_3)
                    .child(icon("pull-request", 14., TEXT_3))
                    .child(format!("based off PR #{n}")),
            ),
            None => (!scratch && f.target() == Target::Worktree).then(|| self.branch_select(side, cx)),
        };
        let link = (!scratch && pr.is_none() && side.opens).then(|| self.pr_select(cx));
        let prompt = &self.prompt;
        let empty = prompt.read(cx).value().trim().is_empty() && !self.composer.read(cx).draft.has_attachments();
        let typing = !empty || prompt.read(cx).focus_handle(cx).is_focused(window);
        let ready = self.ready() && side.blocker.is_none();
        let send = send_button("form-start".into(), Action::Send, empty, !ready)
            .when(ready && !empty, |d| d.on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(NewFormEvent::Start))));
        let pill = pr.map(|n| {
            let unlink = div()
                .id("pr-unlink")
                .size(px(18.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(9.))
                .cursor_pointer()
                .hover(|s| s.bg(FILL_3))
                .child(icon("x", 10., TEXT_3))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.draft.unlink_pr();
                    cx.notify();
                }));
            div()
                .h(px(26.))
                .pl(px(9.))
                .pr(px(4.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.))
                .rounded(px(8.))
                .bg(FILL_2)
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .child(icon("pull-request", 13., TEXT_2))
                .child(format!("#{n}"))
                .child(unlink)
                .into_any_element()
        });
        let textarea = Textarea::new(&self.prompt);
        let trailing = link.map(IntoElement::into_any_element).into_iter().collect();
        let frame =
            Frame { project: f.repo.clone(), typing, chips: &side.chips, attachments: &side.attachments, pills: pill.into_iter().collect(), trailing, send };
        let composer = self.composer.update(cx, |c, cx| c.frame(textarea, frame, cx));
        let (key, text) = self.suggesting(cx);
        let list = launch_list(
            &self.suggest,
            key.as_deref(),
            &text,
            &side.chips.launch.pick.provider,
            |i| -> Press { Box::new(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(NewFormEvent::Suggest(i)))) },
            cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.suggest.forget_list();
                cx.notify();
            }),
        );
        let composer = self.suggest_keys(composer, cx);
        let composer = rich_keys(&self.prompt, composer, cx).children(list);
        let image = side.background.is_some();
        let veiled = |pill: Div| if image { div().rounded(px(8.)).bg(field::veil()).child(pill) } else { pill };
        let (machine, project, checkout, branch) = (veiled(machine), veiled(project), checkout.map(veiled), branch.map(veiled));
        let error = f.reply.error.clone().map(|(message, detail)| ui::failure(message, detail).w_full().max_w(px(600.)).mt(px(10.)));
        let footer = div()
            .w_full()
            .max_w(px(600.))
            .mt(px(10.))
            .flex()
            .items_center()
            .gap(px(2.))
            .text_size(px(13.))
            .child(machine)
            .child(div().text_color(TEXT_4).child("·"))
            .child(project)
            .when(!scratch, |d| d.child(div().text_color(TEXT_4).child("·")))
            .children(checkout)
            .children(branch)
            .when(!side.idle, |d| d.child(div().ml_auto().text_size(px(12.)).text_color(TEXT_4).whitespace_nowrap().child("esc to cancel")));
        let content = div()
            .flex()
            .flex_col()
            .items_center()
            .child(div().text_size(px(21.)).line_height(px(25.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("What should we build next?"))
            .child(div().w_full().max_w(px(600.)).mt(px(20.)).child(composer))
            .children(error)
            .child(footer)
            .children(notice);
        let background = side.background.clone();
        let prompt = self.prompt.clone();
        // Not a `cx.listener`: the prompt's drop handler updates the composer.
        let drop = drop_zone(move |paths, window, cx| prompt.update(cx, |s, cx| s.drop_paths(paths, window, cx)));
        with_field(&mut self.field, |this| &mut this.field, background, "new-session-body", content, cx).relative().child(drop)
    }
}
