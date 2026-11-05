use crate::desktop::Desktop;
use crate::desktop::chrome::{Side, empty};
use crate::git_ui::pull_requests::PrItem;
use git::Repo;
use gpui_kit::*;
use ui::Variant;

impl Desktop {
    /// Opens the panel on the Pull request side.
    pub(crate) fn show_pr_side(&mut self, cx: &mut Context<Self>) {
        (self.side, self.sidebar.panel_open) = (Side::PullRequest, true);
        cx.notify();
    }

    /// The worktree's pull request: the composer while one is written, its card, or every review thread; else how to open one.
    pub(crate) fn pr_side(&mut self, cx: &mut Context<Self>) -> Div {
        let panel = div().flex_1().min_h_0().flex().flex_col();
        let Some(repo) = self.repo().cloned() else {
            return panel.child(empty("Not a git repository."));
        };
        if let Some(comments) = self.pr.track.comments.then(|| self.pr_comments(cx)).flatten() {
            return panel.child(comments);
        }
        let head = div().h(px(40.)).flex_none().pl(px(16.)).flex().items_center().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child("Pull request");
        let shown: Vec<Div> = self.pr_composer(&repo, cx).into_iter().chain(self.pr_card(cx)).collect();
        let body = if shown.is_empty() { self.no_pr(&repo, cx) } else { div().id("pr-side").flex_1().min_h_0().overflow_y_scroll().children(shown).into_any_element() };
        panel.child(head).child(body)
    }

    /// Why the worktree has no PR to show, with Create pull request when it can have one.
    fn no_pr(&self, repo: &Repo, cx: &mut Context<Self>) -> AnyElement {
        match self.cwd().and_then(|tree| self.prs.item(&tree, &repo.branch, repo.base.as_deref())) {
            Some(PrItem::Create) => {
                let create = ui::button("pr-side-create", Variant::Primary, Some("pull-request"), "Create pull request")
                    .h(px(30.))
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_pr_composer(window, cx)));
                div().flex().flex_col().items_start().child(empty("No pull request yet.")).child(div().px(px(16.)).child(create)).into_any_element()
            }
            Some(PrItem::Hint(hint)) => empty(hint.to_string()).into_any_element(),
            Some(PrItem::Open(_)) | None => empty("No pull request.").into_any_element(),
        }
    }
}
