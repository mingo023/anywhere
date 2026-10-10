use super::card::{pill, review_icon, row, small, thread_chip, who};
use super::{hunk_tail, place, review_lines, to_fix};
use crate::desktop::Desktop;
use crate::git_ui::pull_requests::chip;
use crate::util::{ago, now_ms};
use git::github::{Pr, Thread, ThreadState};
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{Segment, Variant};

const THREADS_SHOWN: usize = 100;

/// The PR a thread belongs to, and what every one of its threads offers.
struct Shown<'a> {
    tree: &'a str,
    pr: &'a Pr,
    agent: &'static str,
    can_send: bool,
}

fn hunk(t: &Thread) -> Option<Div> {
    let lines = hunk_tail(t.comments.first().map_or("", |c| c.hunk.as_str()), 3);
    (!lines.is_empty()).then(|| {
        div().py(px(4.)).rounded(px(7.)).bg(FILL_2).font_family(MONO).text_size(px(11.5)).overflow_hidden().children(lines.into_iter().map(|l| {
            let (bg, fg) = match l.chars().next() {
                Some('+') => (Some(DIFF_ADD_BG), DIFF_ADD_TEXT),
                Some('-') => (Some(DIFF_DEL_BG), DIFF_DEL_TEXT),
                _ => (None, TEXT_2),
            };
            div().px(px(8.)).h(px(18.)).flex().items_center().whitespace_nowrap().text_color(fg).when_some(bg, |d, bg| d.bg(bg)).child(l.to_string())
        }))
    })
}

impl Desktop {
    /// Every review thread of the PR on screen, in place of Changes.
    pub(crate) fn pr_comments(&self, cx: &mut Context<Self>) -> Option<Div> {
        let (tree, pr) = self.shown_pr()?;
        let fixing = self.pr.track.fixing(&tree, pr);
        let shown = self.shown(&tree, pr);
        let (agent, can_send) = (shown.agent, shown.can_send);
        let url = pr.url.clone();
        let back = ui::icon_button_sized("pr-comments-back", "back", 26., TEXT_3).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.pr.track.comments = false;
            cx.notify();
        }));
        let head = div()
            .h(px(40.))
            .flex_none()
            .pl(px(8.))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .child(back)
            .child(div().flex_1().text_size(px(13.5)).font_weight(FontWeight::SEMIBOLD).child("Comments"))
            .child(chip("pr-comments-chip", pr))
            .child(ui::icon_button_sized("pr-comments-open", "external", 26., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_pr(url.clone(), window, cx))));
        let unresolved = pr.unresolved().count();
        let all = self.pr.track.all;
        let filter = ui::segmented(
            vec![
                Segment { icon: None, value: false, label: "Unresolved".into(), badge: Some(unresolved.to_string()) },
                Segment { icon: None, value: true, label: "All".into(), badge: Some(pr.threads.len().to_string()) },
            ],
            all,
            true,
            true,
            |this: &mut Desktop, all, cx| {
                this.pr.track.all = all;
                cx.notify();
            },
            cx,
        );
        let review = div().px(px(2.)).flex().flex_col().children(review_lines(pr).into_iter().map(|(r, text)| row().child(review_icon(r)).child(div().flex_1().min_w_0().truncate().child(text))));
        let listed: Vec<&Thread> = pr.threads.iter().filter(|t| all || !t.resolved).take(THREADS_SHOWN).collect();
        let empty = listed.is_empty().then(|| row().justify_center().py(px(16.)).text_color(TEXT_3).child(icon("check", 13., SUCCESS)).child("All conversations resolved"));
        let threads: Vec<AnyElement> = listed.into_iter().enumerate().map(|(i, t)| self.pr_thread(i, &shown, t, false, cx)).collect();
        let (_, open) = to_fix(pr, fixing);
        let bulk = (can_send && open.len() > 1).then(|| {
            let ids: Vec<String> = open.iter().map(|t| t.id.clone()).collect();
            ui::button("pr-send-open", Variant::Primary, None, format!("Send {} open comments to {agent}", ids.len()))
                .h(px(30.))
                .justify_center()
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_fix(&[], &ids, cx)))
        });
        let body = div()
            .id("pr-comments")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(10.))
            .pb(px(10.))
            .flex()
            .flex_col()
            .gap(px(8.))
            .child(filter)
            .child(review)
            .children(threads)
            .children(empty)
            .children(bulk);
        Some(div().flex_1().min_h_0().flex().flex_col().child(head).child(body))
    }

    /// Threads of `pr` can go to the agent when sessions aren't only watched and it isn't fixing this PR already.
    fn shown<'a>(&self, tree: &'a str, pr: &'a Pr) -> Shown<'a> {
        let can_send = !self.agents.observe_only() && self.pr.track.fixing(tree, pr).is_none();
        Shown { tree, pr, agent: self.fix_agent(), can_send }
    }

    /// Thread `id` of the PR on screen, as a card under its line in the PR diff.
    pub(crate) fn pinned_thread(&self, k: usize, id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (tree, pr) = self.shown_pr()?;
        let t = pr.threads.iter().find(|t| t.id == id)?;
        Some(self.pr_thread(k, &self.shown(&tree, pr), t, true, cx))
    }

    /// A thread's card; `inline` under its line in the diff, where the code it quotes is already on show.
    fn pr_thread(&self, i: usize, shown: &Shown, t: &Thread, inline: bool, cx: &mut Context<Self>) -> AnyElement {
        let (agent, can_send, now) = (shown.agent, shown.can_send, now_ms());
        let state = t.state(self.pr.track.thread_sent(shown.tree, shown.pr, &t.id));
        let card = div().p(px(10.)).flex().flex_col().gap(px(8.)).rounded(px(10.)).bg(SURFACE).shadow(ui::row_shadow());
        let toggle = |id: String| {
            cx.listener(move |this: &mut Desktop, _: &ClickEvent, _, cx| {
                this.pr.track.toggle_expanded(&id);
                cx.notify();
            })
        };
        if state == ThreadState::Resolved && !self.pr.track.expanded.contains(&t.id) {
            let body = t.comments.first().map(|c| c.body.lines().next().unwrap_or_default().to_string()).unwrap_or_default();
            return div()
                .id(("pr-thread-min", i))
                .px(px(10.))
                .h(px(34.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(10.))
                .bg(SURFACE)
                .shadow(ui::row_shadow())
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(TEXT_3)
                .child(icon("check", 12., SUCCESS))
                .child(div().flex_none().font_family(MONO).text_size(px(11.5)).child(place(t)))
                .child(div().flex_1().min_w_0().truncate().child(body))
                .child(icon("chevron-right", 11., TEXT_4))
                .on_click(toggle(t.id.clone()))
                .into_any_element();
        }
        let top = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .font_family(MONO)
            .text_size(px(11.5))
            .text_color(TEXT_3)
            .child(div().min_w_0().truncate().child(place(t)))
            .children(thread_chip(state, agent))
            .when(t.outdated, |d| d.child(pill("Outdated", FILL_3, TEXT_3)));
        let said = t.comments.iter().map(|c| {
            let name = div().flex().items_center().gap(px(6.)).text_size(px(12.)).child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(c.author.clone())).child(div().text_color(TEXT_4).child(ago(c.at as i64, now)));
            div()
                .flex()
                .items_start()
                .gap(px(8.))
                .child(ui::avatar(&who(&c.author), 20.))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(name).child(div().text_size(px(12.5)).text_color(TEXT_BODY).child(c.body.trim().to_string())))
        });
        let id = t.id.clone();
        let foot = if self.pr.replying.as_deref() == Some(t.id.as_str()) {
            let field = div().px(px(10.)).py(px(7.)).rounded(px(9.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]).text_size(px(12.5)).child(Textarea::new(&self.pr.reply).appearance(false));
            let ready = !self.pr.reply.read(cx).value().trim().is_empty();
            let send = small("pr-reply-send", Variant::Primary, "Reply").when(!ready, |d| d.opacity(0.5).cursor_default()).when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.send_reply(window, cx))));
            let actions = div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(div().flex_1().text_size(px(11.5)).text_color(TEXT_4).child("Posts to GitHub as you · ⌘↩"))
                .child(small("pr-reply-cancel", Variant::Ghost, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.cancel_reply(window, cx))))
                .child(send);
            div().flex().flex_col().gap(px(6.)).child(field).child(actions)
        } else {
            let reply = {
                let id = id.clone();
                small(("pr-reply", i), Variant::Secondary, "Reply").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_reply(id.clone(), window, cx)))
            };
            let resolve = |v: Variant, resolved: bool| {
                let id = id.clone();
                small(("pr-resolve", i), v, if resolved { "Resolve" } else { "Unresolve" }).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.resolve_thread(id.clone(), resolved, true, cx)))
            };
            let view = (!inline && t.line.is_some() && !t.outdated).then(|| {
                let id = id.clone();
                small(("pr-view", i), Variant::Ghost, "View in diff").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.view_thread(&id, cx)))
            });
            let actions: Vec<Stateful<Div>> = match state {
                ThreadState::Open => {
                    let send = can_send.then(|| {
                        let id = id.clone();
                        small(("pr-thread-send", i), Variant::Primary, format!("Send to {agent}")).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_fix(&[], std::slice::from_ref(&id), cx)))
                    });
                    send.into_iter().chain([reply, resolve(Variant::Secondary, true)]).collect()
                }
                ThreadState::Sent => vec![reply],
                ThreadState::Replied => vec![resolve(Variant::Primary, true), reply],
                ThreadState::Resolved => vec![resolve(Variant::Secondary, false), small(("pr-collapse", i), Variant::Ghost, "Collapse").on_click(toggle(id.clone()))],
            };
            div().flex().flex_wrap().gap(px(4.)).children(actions).children(view)
        };
        card.id(("pr-thread", i)).child(top).children(hunk(t).filter(|_| !inline)).children(said).child(foot).into_any_element()
    }
}
