use super::body::body_view;
use super::card::{pill, small, who};
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
use ui::{Segment, Variant};

fn card(id: impl Into<ElementId>, author: &str, when: String) -> (Stateful<Div>, Div) {
    let head = div()
        .flex()
        .items_center()
        .gap(px(7.))
        .text_size(px(12.))
        .child(ui::avatar(&who(author), 20.).text_size(px(9.)))
        .child(div().min_w_0().truncate().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(author.to_string()))
        .child(div().flex_none().text_color(TEXT_4).child(when))
        .child(div().flex_1());
    (div().id(id).px(px(12.)).py(px(10.)).flex().flex_col().gap(px(8.)).rounded(px(12.)).bg(SURFACE).shadow(ui::row_shadow()), head)
}

fn verdict(r: Review) -> Div {
    match r {
        Review::Approved => pill("Approved", SUCCESS_BG, SUCCESS_TEXT),
        Review::ChangesRequested => pill("Changes requested", FAILED_BG, FAILED_TEXT),
        _ => pill("Commented", FILL_3, TEXT_3),
    }
}

fn fold_head(id: &'static str, title: &'static str, open: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.))
        .flex()
        .items_center()
        .gap(px(6.))
        .cursor_pointer()
        .text_size(px(12.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(TEXT_2)
        .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_3))
        .child(title)
}

impl Desktop {
    /// The PR's description, then what was said on it in time order with each review's threads under it, then a box to say more.
    pub(crate) fn pr_conversation(&self, tree: &str, pr: &Pr, cx: &mut Context<Self>) -> Stateful<Div> {
        let open = !self.pr.track.about_folded;
        let about = fold_head("pr-about", "Description", open).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.pr.track.about_folded = !this.pr.track.about_folded;
            cx.notify();
        }));
        let about_body = open.then(|| if pr.body.trim().is_empty() { div().text_size(px(12.5)).text_color(TEXT_4).child("No description.") } else { body_view(&pr.body) });
        let all = self.pr.track.all;
        let unresolved = pr.unresolved().count();
        let filter = ui::segmented(
            vec![
                Segment { icon: None, value: false, label: "Unresolved".into(), badge: Some(unresolved.to_string()) },
                Segment { icon: None, value: true, label: "All".into(), badge: Some(pr.threads.len().to_string()) },
            ],
            all,
            true,
            false,
            |this: &mut Desktop, all, cx| {
                this.pr.track.all = all;
                cx.notify();
            },
            cx,
        );
        let shown = self.shown(tree, pr);
        let fixing = self.pr.track.fixing(tree, pr);
        let (_, open_threads) = to_fix(pr, fixing.as_ref());
        let bulk = (shown.can_send && open_threads.len() > 1).then(|| {
            let ids: Vec<String> = open_threads.iter().map(|t| t.id.clone()).collect();
            ui::button("pr-send-open", Variant::Secondary, None, format!("Send {} open comments to {}", ids.len(), shown.agent))
                .h(px(30.))
                .justify_center()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_fix(&[], &ids, cx)))
        });
        let entries: Vec<AnyElement> = timeline(pr, all).into_iter().enumerate().map(|(k, e)| self.pr_entry(k, e, &shown, cx)).collect();
        let resolved = (!all && unresolved == 0 && !pr.threads.is_empty())
            .then(|| div().py(px(6.)).flex().items_center().justify_center().gap(px(6.)).text_size(px(12.5)).text_color(TEXT_3).child(icon("check", 13., SUCCESS)).child("All conversations resolved"));
        div()
            .id("pr-conversation")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .track_scroll(&self.pr.conversation_scroll)
            .px(px(12.))
            .pt(px(8.))
            .pb(px(16.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().flex().flex_col().gap(px(4.)).child(about).children(about_body))
            .child(div().pt(px(6.)).flex().items_center().gap(px(8.)).child(div().flex_1().text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child("Conversation")).child(filter))
            .children(bulk)
            .children(entries)
            .children(resolved)
            .child(self.pr_comment_box(tree, cx))
    }

    fn pr_entry(&self, k: usize, e: Entry, shown: &Shown, cx: &mut Context<Self>) -> AnyElement {
        let (pr, now) = (shown.pr, now_ms());
        match e {
            Entry::Thread(i) => self.pr_thread(i, shown, &pr.threads[i], false, cx),
            Entry::Review { at, threads } => {
                let r = &pr.submitted[at];
                let (card, head) = card(("pr-review", k), &r.author, ago(r.at as i64, now));
                let quote = (!r.body.trim().is_empty()).then(|| self.quote_button(k, &r.body, cx));
                card.child(head.child(verdict(r.verdict)))
                    .when(!r.body.trim().is_empty(), |d| d.child(body_view(&r.body)))
                    .children(quote)
                    .children(threads.into_iter().map(|i| self.pr_thread(i, shown, &pr.threads[i], false, cx)))
                    .into_any_element()
            }
            Entry::Note(i) => {
                let n = &pr.notes[i];
                let sending = n.id.is_empty();
                let (card, head) = card(("pr-note", k), &n.author, if sending { "Sending…".into() } else { ago(n.at as i64, now) });
                let link = (!n.url.is_empty()).then(|| {
                    let url = n.url.clone();
                    ui::icon_button_sized(("pr-note-link", k), "copy", 22., TEXT_3)
                        .tooltip(|window, cx| Tooltip::new("Copy link").build(window, cx))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.copy_link(url.clone(), cx)))
                });
                card.when(sending, |d| d.opacity(0.6))
                    .child(head.children(link))
                    .child(body_view(&n.body))
                    .when(!sending, |d| d.child(self.quote_button(k, &n.body, cx)))
                    .into_any_element()
            }
        }
    }

    fn quote_button(&self, k: usize, said: &str, cx: &mut Context<Self>) -> Div {
        let said = said.to_string();
        div().flex().child(small(("pr-quote", k), Variant::Ghost, "Quote reply").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.quote_reply(&said, window, cx))))
    }

    fn pr_comment_box(&self, tree: &str, cx: &mut Context<Self>) -> AnyElement {
        if self.pr.commenting.as_deref() != Some(tree) {
            return div()
                .id("pr-comment-open")
                .h(px(36.))
                .px(px(12.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(10.))
                .border(px(0.5))
                .border_color(SEPARATOR_STRONG)
                .cursor_pointer()
                .hover(|s| s.bg(FILL_1))
                .text_size(px(12.5))
                .text_color(TEXT_4)
                .child(icon("comment", 13., TEXT_4))
                .child("Leave a comment…")
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_comment(window, cx)))
                .into_any_element();
        }
        let ready = !self.pr.comment.read(cx).value().trim().is_empty();
        let send = small("pr-comment-send", Variant::Primary, "Comment").when(!ready, |d| d.opacity(0.5).cursor_default()).when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.send_comment(window, cx))));
        let error = self.pr.comment_error.clone().map(|e| div().flex().items_start().gap(px(6.)).text_size(px(12.)).text_color(FAILED_TEXT).child(icon("warning", 12., FAILED_TEXT)).child(div().flex_1().min_w_0().child(e)));
        let foot = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().text_size(px(11.5)).text_color(TEXT_4).child(format!("Markdown · {}", ui::os_keys("⌘↩"))))
            .child(small("pr-comment-cancel", Variant::Secondary, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.cancel_comment(window, cx))))
            .child(send);
        div()
            .flex()
            .flex_col()
            .gap(px(6.))
            .child(div().px(px(10.)).py(px(7.)).rounded(px(9.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]).text_size(px(12.5)).child(Textarea::new(&self.pr.comment).appearance(false)))
            .children(error)
            .child(foot)
            .into_any_element()
    }
}
