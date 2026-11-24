use super::timeline::said;
use super::{PrTab, commits};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Side, empty};
use crate::git_ui::pull_requests::PrItem;
use git::Repo;
use git::github::{Outcome, Pr, base_branch};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

impl Desktop {
    /// Opens the panel on the Pull request side.
    pub(crate) fn show_pr_side(&mut self, cx: &mut Context<Self>) {
        self.side = Side::PullRequest;
        self.sidebar.update(cx, |s, _| s.panel_open = true);
        cx.notify();
    }

    /// The worktree's pull request: the composer while one is written, else the PR over its tabs, else how to open one.
    pub(crate) fn pr_side(&mut self, cx: &mut Context<Self>) -> Div {
        let panel = div().flex_1().min_h_0().flex().flex_col();
        let Some(repo) = self.repo_handle(cx) else {
            return panel.child(empty("Not a git repository."));
        };
        let panel = panel.child(self.pr_head(cx));
        if let Some(composer) = self.pr_composer(&repo, cx) {
            return panel.child(div().id("pr-side").flex_1().min_h_0().overflow_y_scroll().child(composer));
        }
        let Some((tree, pr)) = self.shown_pr(cx) else {
            return panel.child(self.no_pr(&repo, cx));
        };
        let body = match self.pr.track.tab {
            PrTab::Conversation => self.pr_conversation(&tree, pr, cx).into_any_element(),
            PrTab::Checks => self.pr_checks_tab(&tree, pr, cx).into_any_element(),
            PrTab::Commits => self.pr_commits(pr, cx).into_any_element(),
            PrTab::Files => self.pr_files(pr, cx).into_any_element(),
        };
        panel.child(self.pr_hero(&tree, pr, &repo, cx)).child(self.pr_tabs(pr, cx)).child(body)
    }

    fn pr_head(&self, cx: &mut Context<Self>) -> Div {
        let tree = self.cwd(cx).unwrap_or_default();
        let refresh: AnyElement = if self.prs.asking(&tree) {
            div().size(px(26.)).flex().items_center().justify_center().child(spinner("pr-refreshing", 13., TEXT_3)).into_any_element()
        } else {
            ui::icon_button_sized("pr-refresh", "reload", 26., TEXT_3)
                .tooltip(|window, cx| Tooltip::new("Refresh").build(window, cx))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.refresh_pr(cx)))
                .into_any_element()
        };
        let open = self.shown_pr(cx).map(|(_, pr)| {
            let url = pr.url.clone();
            ui::icon_button_sized("pr-open", "external", 26., TEXT_3)
                .tooltip(|window, cx| Tooltip::new("Open on GitHub").build(window, cx))
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_pr(url.clone(), window, cx)))
        });
        div()
            .h(px(40.))
            .flex_none()
            .pl(px(16.))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(2.))
            .child(div().flex_1().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child("Pull request"))
            .child(refresh)
            .children(open)
    }

    /// Conversation, Checks, Commits and Files; only the open tab names itself, the others show their count.
    fn pr_tabs(&self, pr: &Pr, cx: &mut Context<Self>) -> Div {
        let open = self.pr.track.tab;
        let failed = pr.runs.iter().filter(|c| c.outcome == Outcome::Failed).count();
        let tabs = PrTab::ALL.into_iter().map(|tab| {
            let on = tab == open;
            let (glyph, count, color): (&str, usize, Token) = match tab {
                PrTab::Conversation => ("comment", said(pr), TEXT_4),
                PrTab::Checks if failed > 0 => ("list-checks", failed, FAILED_TEXT),
                PrTab::Checks if pr.checks.pending > 0 => ("list-checks", pr.checks.pending.into(), WAITING_TEXT),
                PrTab::Checks => ("list-checks", pr.checks.passed.into(), if pr.runs.is_empty() { TEXT_4 } else { SUCCESS_TEXT }),
                PrTab::Commits => ("commit", pr.commits.len(), TEXT_4),
                PrTab::Files => ("file", pr.files.len(), TEXT_4),
            };
            div()
                .id(("pr-tab", tab as usize))
                .h(px(28.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(5.))
                .rounded(px(8.))
                .cursor_pointer()
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if on { TEXT } else { TEXT_3 })
                .when(on, |d| d.bg(FILL_3))
                .when(!on, |d| d.hover(|s| s.bg(FILL_1).text_color(TEXT_2)))
                .child(icon(glyph, 14., if on { TEXT } else { TEXT_3 }))
                .when(on, |d| d.child(tab.label()))
                .child(div().font_family(MONO).text_size(px(11.5)).text_color(color).child(count.to_string()))
                .tooltip(move |window, cx| Tooltip::new(tab.label()).build(window, cx))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.pr.track.tab = tab;
                    cx.notify();
                }))
        });
        let comment = ui::icon_button_sized("pr-comment-new", "comment", 26., TEXT_3)
            .tooltip(|window, cx| Tooltip::new("Leave a comment").build(window, cx))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_comment(window, cx)));
        div().h(px(44.)).flex_none().px(px(10.)).flex().items_center().gap(px(2.)).children(tabs).child(div().flex_1()).child(comment)
    }

    /// Why the worktree has no PR to show, with Create pull request when it can have one.
    fn no_pr(&self, repo: &Repo, cx: &mut Context<Self>) -> AnyElement {
        match self.cwd(cx).and_then(|tree| self.prs.item(&tree, &repo.branch, repo.base.as_deref())) {
            Some(PrItem::Create) => {
                let create = ui::button("pr-side-create", Variant::Primary, Some("pull-request"), "Create pull request")
                    .h(px(30.))
                    .mt(px(10.))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_pr_composer(window, cx)));
                let badge =
                    div().size(px(40.)).mb(px(8.)).flex().items_center().justify_center().rounded_full().bg(FILL_2).child(icon("pull-request", 18., TEXT_2));
                let base = repo.base.as_deref().map(base_branch).unwrap_or_default();
                let branches = div()
                    .max_w_full()
                    .flex()
                    .items_center()
                    .gap(px(5.))
                    .font_family(MONO)
                    .text_size(px(11.5))
                    .text_color(TEXT_3)
                    .whitespace_nowrap()
                    .child(div().min_w_0().truncate().child(repo.branch.clone()))
                    .child(icon("arrow-right", 10., TEXT_4))
                    .child(div().flex_none().child(base.to_string()));
                div()
                    .flex_1()
                    .px(px(16.))
                    .pb(px(40.))
                    .flex()
                    .flex_col()
                    .items_center()
                    .justify_center()
                    .gap(px(4.))
                    .child(badge)
                    .child(div().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("No pull request yet"))
                    .child(branches)
                    .child(div().text_size(px(12.)).text_color(TEXT_3).child(format!("{} ahead", commits(repo.ahead))))
                    .child(create)
                    .into_any_element()
            }
            Some(PrItem::Hint(hint)) => empty(hint.to_string()).into_any_element(),
            Some(PrItem::Open(_)) | None => empty("No pull request.").into_any_element(),
        }
    }
}
