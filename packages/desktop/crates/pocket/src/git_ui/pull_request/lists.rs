use super::card::{card, list_head, list_row};
use crate::desktop::Desktop;
use crate::util::{ago, now_ms};
use git::github::{Pr, base_branch};
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::ops::Range;
use theme::*;
use workspace::Doc;

fn size(additions: u32, deletions: u32) -> Div {
    div()
        .flex()
        .flex_none()
        .gap(px(5.))
        .font_family(MONO)
        .text_size(px(11.5))
        .child(div().text_color(SUCCESS_TEXT).child(format!("+{additions}")))
        .child(div().text_color(FAILED_TEXT).child(format!("−{deletions}")))
}

/// The card's `overflow_hidden` clips to a rectangle, so the end rows round their own hover.
fn row(id: impl Into<ElementId>, i: usize, rows: usize) -> Stateful<Div> {
    list_row(i).id(id).cursor_pointer().hover(|s| s.bg(FILL_1)).when(i == 0, |d| d.rounded_t(px(12.))).when(i + 1 == rows, |d| d.rounded_b(px(12.)))
}

/// The list's card, as tall as its rows up to the room there is, so a short list doesn't leave an empty card below it.
fn list_card(id: &'static str, rows: usize, list: impl IntoElement, scroll: &UniformListScrollHandle) -> Div {
    let card = card().id(id).relative().w_full().h(px(rows as f32 * 34.)).max_h_full().child(list).vertical_scrollbar(scroll);
    div().flex_1().min_h_0().px(px(10.)).pb(px(16.)).child(card)
}

impl Desktop {
    /// Oldest first, as GitHub lists them.
    pub(crate) fn pr_commits(&self, pr: &Pr, cx: &mut Context<Self>) -> Div {
        let branches = div().font_family(MONO).text_size(px(11.5)).child(format!(
            "{} ← {}",
            base_branch(&pr.base),
            self.repo().map(|r| r.branch.clone()).unwrap_or_default()
        ));
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
                        row(("pr-commit", i), i, pr.commits.len())
                            .child(this.face(&c.author, 20.))
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
        .size_full();
        div().flex_1().min_h_0().flex().flex_col().pt(px(4.)).child(list_head(format!("{} commits", pr.commits.len()), branches)).child(list_card(
            "pr-commits-list",
            pr.commits.len(),
            list,
            &self.pr.commits_scroll,
        ))
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
                        let threads = pr.threads.iter().filter(|t| t.path == f.path).count();
                        let said = (threads > 0).then(|| {
                            div()
                                .flex()
                                .flex_none()
                                .items_center()
                                .gap(px(3.))
                                .font_family(MONO)
                                .text_size(px(11.))
                                .text_color(TEXT_3)
                                .child(icon("comment", 11., TEXT_3))
                                .child(threads.to_string())
                        });
                        row(("pr-file", i), i, pr.files.len())
                            .child(icon("file", 13., TEXT_3))
                            .child(path)
                            .children(said)
                            .child(size(f.additions, f.deletions))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_doc(doc.clone(), false, cx)))
                            .into_any_element()
                    })
                    .collect()
            }),
        )
        .track_scroll(&self.pr.files_scroll)
        .size_full();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .pt(px(4.))
            .child(list_head(format!("{} files", pr.files.len()), size(pr.additions, pr.deletions)))
            .child(list_card("pr-files-list", pr.files.len(), list, &self.pr.files_scroll))
    }
}
