use super::card::who;
use crate::desktop::Desktop;
use crate::util::{ago, now_ms};
use git::github::{Pr, base_branch};
use gpui_kit::*;
use gpui_kit::component::scroll::ScrollableElement as _;
use std::ops::Range;
use theme::*;
use workspace::Doc;

fn summary(left: String, right: Div) -> Div {
    div().h(px(36.)).flex_none().px(px(16.)).flex().items_center().gap(px(8.)).text_size(px(12.)).text_color(TEXT_3).child(div().flex_1().child(left)).child(right)
}

fn size(additions: u32, deletions: u32) -> Div {
    div().flex().flex_none().gap(px(5.)).font_family(MONO).text_size(px(11.5)).child(div().text_color(SUCCESS_TEXT).child(format!("+{additions}"))).child(div().text_color(FAILED_TEXT).child(format!("−{deletions}")))
}

fn row(id: impl Into<ElementId>) -> Stateful<Div> {
    div().id(id).w_full().h(px(34.)).px(px(8.)).flex().items_center().gap(px(8.)).rounded(px(8.)).cursor_pointer().hover(|s| s.bg(FILL_1)).text_size(px(12.5))
}

impl Desktop {
    /// Oldest first, as GitHub lists them.
    pub(crate) fn pr_commits(&self, pr: &Pr, cx: &mut Context<Self>) -> Div {
        let branches = div().font_family(MONO).text_size(px(11.5)).child(format!("{} ← {}", base_branch(&pr.base), self.repo().map(|r| r.branch.clone()).unwrap_or_default()));
        let list = uniform_list(
            "pr-commits",
            pr.commits.len(),
            cx.processor(|this, range: Range<usize>, _, cx| {
                let Some((_, pr)) = this.shown_pr() else { return Vec::new() };
                let now = now_ms();
                range
                    .filter_map(|i| pr.commits.get(i).map(|c| (i, c)))
                    .map(|(i, c)| {
                        let url = format!("{}/commits/{}", pr.url, c.oid);
                        row(("pr-commit", i))
                            .child(ui::avatar(&who(&c.author), 20.).text_size(px(9.)))
                            .child(div().flex_1().min_w_0().truncate().text_color(TEXT).child(c.subject.clone()))
                            .child(div().flex_none().font_family(MONO).text_size(px(11.5)).text_color(TEXT_3).child(c.oid.chars().take(7).collect::<String>()))
                            .child(div().flex_none().w(px(32.)).flex().justify_end().text_size(px(11.5)).text_color(TEXT_4).child(ago(c.at as i64, now)))
                            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_pr(url.clone(), window, cx)))
                            .into_any_element()
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.pr.commits_scroll)
        .size_full()
        .px(px(8.))
        .pb(px(8.));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(summary(format!("{} commits", pr.commits.len()), branches))
            .child(div().id("pr-commits-list").relative().flex_1().min_h_0().child(list).vertical_scrollbar(&self.pr.commits_scroll))
    }

    pub(crate) fn pr_files(&self, pr: &Pr, cx: &mut Context<Self>) -> Div {
        let list = uniform_list(
            "pr-files",
            pr.files.len(),
            cx.processor(|this, range: Range<usize>, _, cx| {
                let Some((_, pr)) = this.shown_pr() else { return Vec::new() };
                let base = this.pr_base(pr);
                range
                    .filter_map(|i| pr.files.get(i).map(|f| (i, f)))
                    .map(|(i, f)| {
                        let cut = f.path.rfind('/').map_or(0, |k| k + 1);
                        let doc = Doc::PrFile { base: base.clone(), path: f.path.clone() };
                        let path = div()
                            .flex_1()
                            .min_w_0()
                            .flex()
                            .font_family(MONO)
                            .text_size(px(12.))
                            .child(div().min_w_0().truncate().text_color(TEXT_4).child(f.path[..cut].to_string()))
                            .child(div().flex_none().text_color(TEXT).child(f.path[cut..].to_string()));
                        row(("pr-file", i))
                            .child(icon("file", 13., TEXT_3))
                            .child(path)
                            .child(size(f.additions, f.deletions))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_doc(doc.clone(), false, cx)))
                            .into_any_element()
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.pr.files_scroll)
        .size_full()
        .px(px(8.))
        .pb(px(8.));
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(summary(format!("{} files", pr.files.len()), size(pr.additions, pr.deletions)))
            .child(div().id("pr-files-list").relative().flex_1().min_h_0().child(list).vertical_scrollbar(&self.pr.files_scroll))
    }
}
