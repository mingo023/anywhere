use super::body::body_view;
use super::card::{card, small, small_with};
use super::comments::Shown;
use super::timeline::{Entry, timeline};
use super::to_fix;
use crate::desktop::Desktop;
use crate::util::{ago, now_ms};
use git::github::{Pr, Review};
use gpui_kit::component::input::Textarea;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

/// Who said it and when, over what they said, indented under their name.
fn said_card(id: impl Into<ElementId>, face: Div, author: &str, when: String) -> (Stateful<Div>, Div) {
    let head = div()
        .pt(px(9.))
        .pr(px(8.))
        .pl(px(10.))
        .flex()
        .items_center()
        .gap(px(7.))
        .text_size(px(12.5))
        .child(face)
        .child(div().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(author.to_string()))
        .child(div().flex_none().text_color(TEXT_4).child(when))
        .child(div().flex_1());
    (card().id(id).flex().flex_col(), head)
}

fn said_body(md: &str) -> Div {
    div().pt(px(5.)).pr(px(12.)).pb(px(10.)).pl(px(37.)).child(body_view(md))
}

fn verdict(r: Review) -> Div {
    let (glyph, text, color) = match r {
        Review::Approved => ("check", "Approved", SUCCESS_TEXT),
        Review::ChangesRequested => ("x", "Changes requested", FAILED_TEXT),
        _ => ("comment", "Commented", TEXT_3),
    };
    div().flex().flex_none().items_center().gap(px(4.)).text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(color).child(icon(glyph, 12., color)).child(text)
}

fn fold_head(id: &'static str, title: &'static str, open: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(32.))
        .flex_1()
        .px(px(4.))
        .flex()
        .items_center()
        .gap(px(6.))
        .rounded(px(7.))
        .cursor_pointer()
        .text_size(px(12.))
        .font_weight(FontWeight::MEDIUM)
        .text_color(TEXT_3)
        .hover(|s| s.text_color(TEXT_2))
        .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_3))
        .child(title)
}

impl Desktop {
    /// The PR's description, then what was said on it in time order with each review's threads under it, then a box to say more.
    pub(crate) fn pr_conversation(&self, tree: &str, pr: &Pr, cx: &mut Context<Self>) -> Stateful<Div> {
        let open = !self.pr.track.about_folded;
        let about = div().flex().child(fold_head("pr-about", "Description", open).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.pr.track.about_folded = !this.pr.track.about_folded;
            cx.notify();
        })));
        let about_body = open.then(|| {
            let text = if pr.body.trim().is_empty() { div().text_color(TEXT_4).child("No description.") } else { body_view(&pr.body).line_height(px(20.)) };
            div().pt(px(2.)).px(px(4.)).pb(px(14.)).child(text)
        });
        let all = self.pr.track.all;
        let unresolved = pr.unresolved().count();
        let shown = self.shown(tree, pr);
        let fixing = self.pr.track.fixing(tree, pr);
        let (_, open_threads) = to_fix(pr, fixing.as_ref());
        let bulk = (shown.can_send && open_threads.len() > 1).then(|| {
            let ids: Vec<String> = open_threads.iter().map(|t| t.id.clone()).collect();
            ui::button("pr-send-open", Variant::Secondary, None, format!("Send {} open comments to {}", ids.len(), shown.agent))
                .h(px(30.))
                .text_size(px(12.5))
                .justify_center()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_fix(&[], &ids, cx)))
        });
        let entries: Vec<AnyElement> = timeline(pr, all).into_iter().enumerate().map(|(k, e)| self.pr_entry(k, e, &shown, cx)).collect();
        let resolved = (!all && unresolved == 0 && !pr.threads.is_empty())
            .then(|| card().h(px(44.)).flex().items_center().justify_center().gap(px(8.)).text_size(px(12.5)).text_color(TEXT_3).child(icon("check", 13., SUCCESS)).child("All conversations resolved"));
        let talk = div().flex().items_center().gap(px(8.)).child(fold_head("pr-talk", "Conversation", true)).child(self.thread_filter(all, unresolved, pr.threads.len(), cx));
        div()
            .id("pr-conversation")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.pr.conversation_scroll)
            .px(px(12.))
            .pt(px(4.))
            .pb(px(24.))
            .flex()
            .flex_col()
            .child(about)
            .children(about_body)
            .child(talk)
            .child(div().pt(px(2.)).pb(px(8.)).flex().flex_col().gap(px(10.)).children(bulk).children(entries).children(resolved))
            .child(self.pr_comment_box(tree, cx))
    }

    /// Unresolved or All, with how many threads each shows.
    fn thread_filter(&self, all: bool, unresolved: usize, total: usize, cx: &mut Context<Self>) -> Div {
        let option = |value: bool, label: &'static str, n: usize| {
            let on = value == all;
            div()
                .id(("pr-filter", usize::from(value)))
                .h(px(20.))
                .px(px(7.))
                .flex()
                .items_center()
                .gap(px(4.))
                .rounded(px(5.))
                .cursor_pointer()
                .text_size(px(11.5))
                .text_color(if on { TEXT } else { TEXT_3 })
                .when(on, |d| d.bg(SURFACE).shadow(ui::row_shadow()))
                .child(label)
                .child(div().font_weight(FontWeight::MEDIUM).text_color(TEXT_4).child(n.to_string()))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.pr.track.all = value;
                    cx.notify();
                }))
        };
        div().flex().flex_none().gap(px(2.)).p(px(2.)).rounded(px(7.)).bg(FILL_2).child(option(false, "Unresolved", unresolved)).child(option(true, "All", total))
    }

    fn pr_entry(&self, k: usize, e: Entry, shown: &Shown, cx: &mut Context<Self>) -> AnyElement {
        let (pr, now) = (shown.pr, now_ms());
        match e {
            Entry::Thread(i) => self.conv_thread(i, shown, &pr.threads[i], cx),
            Entry::Review { at, threads } => {
                let r = &pr.submitted[at];
                let (card, head) = said_card(("pr-review", k), self.face(&r.author, 20.), &r.author, ago(r.at as i64, now));
                let has_body = !r.body.trim().is_empty();
                let quote = has_body.then(|| {
                    let said = r.body.clone();
                    ui::icon_button_sized(("pr-review-quote", k), "reply", 22., TEXT_3)
                        .tooltip(|window, cx| Tooltip::new("Quote reply").build(window, cx))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.quote_reply(&said, window, cx)))
                });
                card.child(head.child(verdict(r.verdict)).children(quote))
                    .child(if has_body { said_body(&r.body) } else { div().h(px(8.)) })
                    .children(threads.into_iter().map(|i| self.conv_thread(i, shown, &pr.threads[i], cx)))
                    .into_any_element()
            }
            Entry::Note(i) => {
                let n = &pr.notes[i];
                let sending = n.id.is_empty();
                let (card, head) = said_card(("pr-note", k), self.face(&n.author, 20.), &n.author, if sending { "Sending…".into() } else { ago(n.at as i64, now) });
                let link = (!n.url.is_empty()).then(|| {
                    let url = n.url.clone();
                    ui::icon_button_sized(("pr-note-link", k), "copy", 22., TEXT_3)
                        .tooltip(|window, cx| Tooltip::new("Copy link").build(window, cx))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.copy_link(url.clone(), cx)))
                });
                let quote = (!sending).then(|| {
                    let said = n.body.clone();
                    div().p(px(8.)).pl(px(36.)).flex().child(small_with(("pr-quote", k), Variant::Ghost, "reply", "Quote reply").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.quote_reply(&said, window, cx))))
                });
                card.when(sending, |d| d.opacity(0.55)).child(head.children(link)).child(said_body(&n.body)).children(quote).into_any_element()
            }
        }
    }

    /// "Leave a comment…" until clicked, then the box with Cancel and Comment.
    fn pr_comment_box(&self, tree: &str, cx: &mut Context<Self>) -> AnyElement {
        if self.pr.commenting.as_deref() != Some(tree) {
            let me = self.login(&self.gh_in(tree));
            return div()
                .id("pr-comment-open")
                .h(px(36.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(10.))
                .bg(FILL_2)
                .cursor_pointer()
                .hover(|s| s.bg(FILL_3))
                .text_size(px(12.5))
                .text_color(TEXT_4)
                .child(self.face(&me, 20.))
                .child("Leave a comment…")
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_comment(window, cx)))
                .into_any_element();
        }
        let ready = !self.pr.comment.read(cx).value().trim().is_empty();
        let send = small("pr-comment-send", Variant::Primary, "Comment").when(!ready, |d| d.opacity(0.5).cursor_default()).when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.send_comment(window, cx))));
        let error = self.pr.comment_error.clone().map(|e| div().mx(px(10.)).mb(px(4.)).flex().items_start().gap(px(6.)).text_size(px(12.)).text_color(FAILED_TEXT).child(icon("warning", 12., FAILED_TEXT)).child(div().flex_1().min_w_0().child(e)));
        let foot = div()
            .px(px(6.))
            .pt(px(4.))
            .pb(px(6.))
            .flex()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().pl(px(4.)).text_size(px(11.)).text_color(TEXT_4).child(format!("Markdown · {}", ui::os_keys("⌘↩"))))
            .child(small("pr-comment-cancel", Variant::Ghost, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.cancel_comment(window, cx))))
            .child(send);
        div()
            .rounded(px(10.))
            .bg(FILL_2)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
            .child(div().px(px(10.)).pt(px(9.)).min_h(px(56.)).text_size(px(12.5)).child(Textarea::new(&self.pr.comment).appearance(false)))
            .children(error)
            .child(foot)
            .into_any_element()
    }
}
