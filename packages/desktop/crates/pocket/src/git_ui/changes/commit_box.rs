use super::{CommitKind, commit_label, commit_ready};
use crate::desktop::Desktop;
use git::Repo;
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn commit_box(&self, repo: &Repo, cx: &mut Context<Self>) -> Div {
        let has_changes = !repo.files.is_empty();
        let write = div()
            .id("commit-write")
            .size(px(24.))
            .mt(px(5.))
            .mr(px(5.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .map(|d| if self.changes.writing { d.child(spinner("commit-writing", 13., TEXT_3)) } else { d.child(icon("sparkle", 14., TEXT_3)) })
            .when(has_changes && !self.changes.writing, |d| {
                d.cursor_pointer().hover(|s| s.bg(FILL_3)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.write_message(window, cx)))
            })
            .when(!has_changes, |d| d.opacity(0.4));
        let field = div()
            .flex()
            .items_start()
            .rounded(px(10.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
            .text_size(px(13.))
            .child(div().flex_1().min_w_0().child(Textarea::new(&self.changes.input).appearance(false)))
            .child(write);
        let label = commit_label(&repo.files, self.changes.busy);
        let ready = commit_ready(&repo.files, &self.changes.input.read(cx).value(), self.changes.busy.is_some());
        let wash = Token::new(0xffffff1a, 0x1717171a);
        let commit = div()
            .id("commit")
            .flex_1()
            .h_full()
            .flex()
            .items_center()
            .justify_center()
            .gap(px(6.))
            .rounded_l(px(9.))
            .map(|d| if self.changes.busy.is_some() { d.child(spinner("commit-busy", 13., ON_TEXT)) } else { d.child(icon("check", 14., ON_TEXT)) })
            .child(label)
            .when(ready, |d| d.cursor_pointer().hover(|s| s.bg(wash)))
            .when(!ready, |d| d.text_color(Token::new(0xffffff8c, 0x1717178c)))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.commit(CommitKind::Commit, window, cx)));
        let menu_open = self.changes.commit_menu;
        let chevron = div()
            .id("commit-more")
            .w(px(30.))
            .h_full()
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded_r(px(9.))
            .border_l(px(0.5))
            .border_color(Token::new(0xffffff33, 0x17171733))
            .cursor_pointer()
            .hover(|s| s.bg(wash))
            .when(menu_open, |d| d.bg(wash))
            .child(icon("chevron-down", 12., ON_TEXT))
            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                cx.stop_propagation();
                this.changes.commit_menu = !this.changes.commit_menu;
                this.changes.menu = false;
                cx.notify();
            }));
        let menu = ui::pop(div().id("commit-menu"))
            .w(px(220.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.changes.commit_menu = false;
                cx.notify();
            }))
            .child(ui::menu_row("commit-push", "arrow-up", "Commit & Push", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.commit(CommitKind::Push, window, cx))))
            .child(ui::menu_row("commit-amend", "compose", "Amend Last Commit", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.commit(CommitKind::Amend, window, cx))));
        let button = ui::primary(div().relative().h(px(30.)).flex().rounded(px(9.)))
            .text_size(px(13.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(ON_TEXT)
            .child(commit)
            .child(chevron)
            .when(menu_open, |d| d.child(ui::dropdown(34., ui::menu_in("commit-menu-in", menu))));
        let error = self.changes.error.clone().map(|e| {
            div()
                .id("commit-error")
                .max_h(px(120.))
                .overflow_y_scroll()
                .px(px(10.))
                .py(px(8.))
                .rounded(px(9.))
                .bg(FAILED_BG)
                .font_family(MONO)
                .text_size(px(11.5))
                .text_color(FAILED)
                .child(e)
        });
        div().flex_none().px(px(10.)).pb(px(6.)).flex().flex_col().gap(px(8.)).child(field).child(button).children(error)
    }
}
