use super::{CommitKind, commit_label, commit_ready};
use crate::desktop::Desktop;
use crate::git_ui::pull_request::{Ship, commits, ship};
use crate::git_ui::pull_requests::PrItem;
use git::Repo;
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

type Act = Box<dyn Fn(&mut Desktop, &mut Window, &mut Context<Desktop>)>;

/// The primary button with a chevron beside it that opens `menu`; it looks pressable only when `ready`.
#[allow(clippy::too_many_arguments)]
pub(crate) fn split_button(id: &'static str, lead: impl IntoElement, label: impl IntoElement, ready: bool, open: bool, main: Act, toggle: Act, menu: Stateful<Div>, cx: &mut Context<Desktop>) -> Div {
    let wash = Token::new(0xffffff1a, 0x1717171a);
    let primary = div()
        .id(id)
        .flex_1()
        .min_w_0()
        .h_full()
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .rounded_l(px(9.))
        .child(lead)
        .child(label)
        .when(ready, |d| d.cursor_pointer().hover(|s| s.bg(wash)))
        .when(!ready, |d| d.text_color(Token::new(0xffffff8c, 0x1717178c)))
        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| main(this, window, cx)));
    let chevron = div()
        .id((id, 1u32))
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
        .when(open, |d| d.bg(wash))
        .child(icon("chevron-down", 12., ON_PRIMARY))
        // Runs before the open menu's click-outside handler, which would otherwise close it only for this click to reopen it.
        .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| {
            cx.stop_propagation();
            toggle(this, window, cx);
            cx.notify();
        }));
    let button = ui::primary(div().h(px(30.)).flex().rounded(px(9.))).text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(ON_PRIMARY).child(primary).child(chevron);
    let menu = ui::pop(menu).w(px(232.)).p(px(6.)).flex().flex_col().occlude();
    div().relative().child(button).when(open, |d| d.child(ui::dropdown(34., ui::menu_in((id, 2u32), menu))))
}

impl Desktop {
    pub(super) fn commit_box(&self, repo: &Repo, cx: &mut Context<Self>) -> Div {
        let item = self.cwd().and_then(|tree| self.prs.item(&tree, &repo.branch, repo.base.as_deref()));
        let shipping = ship(repo, item.as_ref());
        let wrap = div().flex_none().px(px(10.)).pb(px(6.)).flex().flex_col().gap(px(8.));
        if self.pr.composing == self.cwd() && self.pr.composing.is_some() && shipping != Ship::Commit {
            return wrap.children(self.commit_error());
        }
        if let Some(ship) = Some(shipping).filter(|s| *s != Ship::Commit) {
            return wrap.child(self.ship_row(repo, ship, cx)).children(self.commit_error());
        }
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
            .when(self.store.git.ai, |d| d.child(write));
        let commit_all = self.store.git.commit_all;
        let label = commit_label(&repo.files, self.changes.busy, commit_all);
        let ready = commit_ready(&repo.files, &self.changes.input.read(cx).value(), self.changes.busy.is_some(), commit_all);
        let lead = if self.changes.busy.is_some() { spinner("commit-busy", 13., ON_PRIMARY).into_any_element() } else { icon("check", 14., ON_PRIMARY).into_any_element() };
        let menu = div()
            .id("commit-menu")
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.changes.commit_menu = false;
                cx.notify();
            }))
            .child(ui::menu_row("commit-push", "arrow-up", "Commit & Push", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.commit(CommitKind::Push, window, cx))))
            .when(item == Some(PrItem::Create), |d| {
                d.child(ui::menu_row("commit-ship", "pull-request", "Commit, push and create PR", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.commit(CommitKind::Ship, window, cx))))
            })
            .child(ui::menu_row("commit-amend", "compose", "Amend Last Commit", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.commit(CommitKind::Amend, window, cx))))
            .child(ui::menu_divider())
            .child(self.drafts_row(cx));
        let button = split_button(
            "commit",
            lead,
            label,
            ready,
            self.changes.commit_menu,
            Box::new(|this, window, cx| this.commit(CommitKind::Commit, window, cx)),
            Box::new(|this, _, _| {
                this.changes.commit_menu = !this.changes.commit_menu;
                this.changes.menu = false;
            }),
            menu,
            cx,
        );
        wrap.child(field).child(button).children(self.commit_error())
    }

    fn commit_error(&self) -> Option<Stateful<Div>> {
        self.changes.error.clone().map(|e| {
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
        })
    }

    /// What there is to ship once everything is committed: commits to push, or a pushed branch to open a PR for.
    fn ship_row(&self, repo: &Repo, ship: Ship, cx: &mut Context<Self>) -> Div {
        let busy = self.changes.busy;
        let line = div().h(px(18.)).flex().items_center().gap(px(6.)).text_size(px(12.)).text_color(TEXT_3).whitespace_nowrap().overflow_hidden();
        let (line, lead, label, menu): (Div, AnyElement, String, Stateful<Div>) = match ship {
            Ship::Push { commits: n, create } => {
                let newest = repo.commits.first().map(|c| c.subject.clone()).unwrap_or_default();
                let line = line.child(div().flex_none().child(format!("{} to push", commits(n)))).child("·").child(div().min_w_0().truncate().text_color(TEXT_2).child(newest));
                let label = busy.map_or_else(|| format!("Push {}", commits(n)), str::to_string);
                let menu = div()
                    .id("ship-menu")
                    .child(ui::menu_row("ship-push", "arrow-up", "Push", None).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.pr.ship_menu = false;
                        this.push(cx);
                    })))
                    .when(create, |d| d.child(ui::menu_row("ship-push-pr", "pull-request", "Push and create PR", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_pr_composer(window, cx)))));
                (line, icon("arrow-up", 14., ON_PRIMARY).into_any_element(), label, menu)
            }
            _ => {
                let upstream = format!("{}/{}", self.store.git.remote, repo.branch);
                let line = line.child(icon("check", 13., SUCCESS)).child(div().min_w_0().truncate().child(format!("Up to date with {upstream}")));
                let menu = div().id("ship-menu").child(ui::menu_row("ship-create", "pull-request", "Create pull request", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_pr_composer(window, cx))));
                (line, icon("pull-request", 14., ON_PRIMARY).into_any_element(), "Create Pull Request".into(), menu)
            }
        };
        let menu = menu
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.pr.ship_menu = false;
                cx.notify();
            }))
            .child(ui::menu_divider())
            .child(self.drafts_row(cx));
        let lead = if busy.is_some() { spinner("ship-busy", 13., ON_PRIMARY).into_any_element() } else { lead };
        let push = matches!(ship, Ship::Push { .. });
        let button = split_button(
            "ship",
            lead,
            label,
            busy.is_none(),
            self.pr.ship_menu,
            Box::new(move |this, window, cx| {
                if push {
                    this.push(cx)
                } else if this.changes.busy.is_none() {
                    this.open_pr_composer(window, cx)
                }
            }),
            Box::new(|this, _, _| {
                this.pr.ship_menu = !this.pr.ship_menu;
                this.changes.menu = false;
            }),
            menu,
            cx,
        );
        div().flex().flex_col().gap(px(6.)).child(line).child(button)
    }

    fn drafts_row(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        ui::menu_row("pr-drafts", "pull-request", "Create PRs as drafts", None).child(ui::checkbox(self.store.git.draft)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.store.git.draft = !this.store.git.draft;
            this.save_soon(cx);
            cx.notify();
        }))
    }
}
