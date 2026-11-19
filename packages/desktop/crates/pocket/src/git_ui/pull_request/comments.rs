use super::card::{pill, small, thread_chip, who};
use super::{hunk_tail, opening, spot};
use crate::desktop::Desktop;
use crate::util::{ago, now_ms};
use git::github::{Comment, Pr, Thread, ThreadState};
use gpui_kit::component::input::Textarea;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::ops::Range;
use theme::*;
use ui::Variant;

const LATEST_SHOWN: usize = 2;

/// The PR a thread belongs to, and what every one of its threads offers.
pub(super) struct Shown<'a> {
    pub(super) tree: &'a str,
    pub(super) pr: &'a Pr,
    pub(super) agent: &'static str,
    pub(super) can_send: bool,
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

/// The replies a long thread folds away: all between its opening and its last `LATEST_SHOWN`, once that is at least two. `None` when it shows whole.
fn folded_replies(comments: usize, whole: bool) -> Option<Range<usize>> {
    let hidden = comments.saturating_sub(1 + LATEST_SHOWN);
    (!whole && hidden >= 2).then(|| 1..1 + hidden)
}

/// Who wrote `comments`, each once, first come first.
fn authors(comments: &[Comment]) -> Vec<&str> {
    let mut seen: Vec<&str> = Vec::new();
    for c in comments {
        if !seen.contains(&c.author.as_str()) {
            seen.push(&c.author);
        }
    }
    seen
}

fn at_line(t: &Thread) -> String {
    t.line.map_or_else(|| "File".into(), |l| format!("Line {l}"))
}

impl Desktop {
    /// Threads of `pr` can go to the agent when sessions aren't only watched and it isn't fixing this PR already.
    pub(super) fn shown<'a>(&self, tree: &'a str, pr: &'a Pr) -> Shown<'a> {
        let can_send = !self.agents.observe_only() && self.pr.track.fixing(tree, pr).is_none();
        Shown { tree, pr, agent: self.fix_agent(), can_send }
    }

    /// Thread `id` of the PR on screen, as a card under its line in the PR diff.
    pub(crate) fn pinned_thread(&self, k: usize, id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (tree, pr) = self.shown_pr()?;
        let t = pr.threads.iter().find(|t| t.id == id)?;
        Some(self.pr_thread(k, &self.shown(&tree, pr), t, true, cx))
    }

    /// A thread: on its own card under its line in the diff (`inline`), or in the Conversation under its file and line. A long one folds its middle replies.
    pub(super) fn pr_thread(&self, i: usize, shown: &Shown, t: &Thread, inline: bool, cx: &mut Context<Self>) -> AnyElement {
        let (agent, can_send, now) = (shown.agent, shown.can_send, now_ms());
        let state = t.state(self.pr.track.thread_sent(shown.tree, shown.pr, &t.id));
        let frame = |d: Stateful<Div>| if inline { d.px(px(10.)).rounded(px(10.)).bg(SURFACE).shadow(ui::row_shadow()) } else { d.px(px(10.)).rounded(px(9.)).border(px(0.5)).border_color(SEPARATOR_STRONG) };
        let at = if inline { at_line(t) } else { spot(t) };
        let toggle = |id: String| {
            cx.listener(move |this: &mut Desktop, _: &ClickEvent, _, cx| {
                this.pr.track.toggle_expanded(&id);
                cx.notify();
            })
        };
        let whole = self.pr.track.expanded.contains(&t.id);
        if state == ThreadState::Resolved && !whole {
            return frame(div().id(("pr-thread-min", i)))
                .h(px(36.))
                .flex()
                .items_center()
                .gap(px(8.))
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(TEXT_3)
                .child(icon("check", 12., SUCCESS))
                .child(div().flex_none().font_family(MONO).text_size(px(11.5)).child(at.clone()))
                .child(div().flex_1().min_w_0().truncate().child(opening(t)))
                .child(icon("chevron-right", 11., TEXT_4))
                .on_click(toggle(t.id.clone()))
                .into_any_element();
        }
        let view = (!inline && t.line.is_some() && !t.outdated).then(|| {
            let id = t.id.clone();
            div()
                .id(("pr-view", i))
                .flex_none()
                .mr(px(-4.))
                .px(px(4.))
                .py(px(2.))
                .flex()
                .items_center()
                .gap(px(2.))
                .rounded(px(5.))
                .cursor_pointer()
                .hover(|s| s.bg(FILL_2).text_color(TEXT))
                .child("View in diff")
                .child(icon("chevron-right", 10., TEXT_3))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.view_thread(&id, cx)))
        });
        let top = div()
            .min_h(px(20.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(11.5))
            .text_color(TEXT_3)
            .child(div().min_w_0().truncate().font_family(MONO).child(at))
            .children(thread_chip(state))
            .when(t.outdated, |d| d.child(pill("Outdated", FILL_3, TEXT_3)))
            .child(div().flex_1())
            .children(view);
        let comment = |c: &Comment| {
            let name = div().flex().items_center().gap(px(6.)).text_size(px(12.)).child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(c.author.clone())).child(div().text_color(TEXT_4).child(ago(c.at as i64, now)));
            div()
                .flex()
                .items_start()
                .gap(px(8.))
                .child(ui::avatar(&who(&c.author), 20.).text_size(px(9.)))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(name).child(div().text_size(px(12.5)).text_color(TEXT_BODY).child(c.body.trim().to_string())))
                .into_any_element()
        };
        let said: Vec<AnyElement> = match folded_replies(t.comments.len(), whole) {
            None => t.comments.iter().map(comment).collect(),
            Some(hidden) => {
                let faces = authors(&t.comments[hidden.clone()]).into_iter().take(3).enumerate().map(|(k, a)| {
                    let initial: String = who(a).chars().take(1).collect();
                    ui::avatar(&initial, 16.).text_size(px(8.5)).shadow(vec![ui::ring(SURFACE, 1.5)]).when(k > 0, |d| d.ml(px(-3.)))
                });
                let more = div()
                    .id(("pr-more", i))
                    .ml(px(28.))
                    .h(px(24.))
                    .pl(px(4.))
                    .pr(px(9.))
                    .flex()
                    .items_center()
                    .gap(px(7.))
                    .rounded(px(12.))
                    .bg(FILL_2)
                    .hover(|s| s.bg(FILL_3))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(TEXT_2)
                    .child(div().flex().children(faces))
                    .child(format!("{} more replies", hidden.len()))
                    .on_click(toggle(t.id.clone()));
                t.comments.first().map(comment).into_iter().chain([div().flex().child(more).into_any_element()]).chain(t.comments[hidden.end..].iter().map(comment)).collect()
            }
        };
        let id = t.id.clone();
        let foot = if self.pr.replying.as_deref() == Some(t.id.as_str()) {
            let field = div().px(px(10.)).py(px(7.)).rounded(px(9.)).bg(SURFACE).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]).text_size(px(12.5)).child(Textarea::new(&self.pr.reply).appearance(false));
            let ready = !self.pr.reply.read(cx).value().trim().is_empty();
            let send = small("pr-reply-send", Variant::Primary, "Reply").when(!ready, |d| d.opacity(0.5).cursor_default()).when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.send_reply(window, cx))));
            let actions = div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(div().flex_1().text_size(px(11.5)).text_color(TEXT_4).child(format!("Posts to GitHub as you · {}", ui::os_keys("⌘↩"))))
                .child(small("pr-reply-cancel", Variant::Secondary, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.cancel_reply(window, cx))))
                .child(send);
            div().when(!inline, |d| d.pl(px(28.))).flex().flex_col().gap(px(6.)).child(field).child(actions)
        } else {
            let reply = {
                let id = id.clone();
                small(("pr-reply", i), Variant::Secondary, "Reply").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_reply(id.clone(), window, cx)))
            };
            let resolve = |v: Variant, resolved: bool| {
                let id = id.clone();
                small(("pr-resolve", i), v, if resolved { "Resolve" } else { "Unresolve" }).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.resolve_thread(id.clone(), resolved, true, cx)))
            };
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
            div().when(!inline, |d| d.pl(px(28.))).flex().flex_wrap().gap(px(4.)).children(actions)
        };
        frame(div().id(("pr-thread", i))).py(px(10.)).flex().flex_col().gap(px(8.)).child(top).children(hunk(t).filter(|_| !inline)).children(said).child(foot).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{authors, folded_replies};
    use git::github::Comment;

    #[test]
    fn a_long_thread_folds_all_but_its_opening_and_last_two_replies() {
        assert_eq!(folded_replies(9, false), Some(1..7));
        assert_eq!(folded_replies(5, false), Some(1..3));
    }

    #[test]
    fn a_thread_stays_whole_when_folding_would_hide_a_single_reply_or_it_was_unfolded() {
        assert_eq!(folded_replies(4, false), None);
        assert_eq!(folded_replies(1, false), None);
        assert_eq!(folded_replies(0, false), None);
        assert_eq!(folded_replies(9, true), None);
    }

    #[test]
    fn a_folded_threads_faces_are_its_hidden_authors_each_once_in_order() {
        let said = |author: &str| Comment { author: author.into(), ..Comment::default() };
        assert_eq!(authors(&[said("mingo"), said("hoang"), said("mingo"), said("linh")]), vec!["mingo", "hoang", "linh"]);
        assert!(authors(&[]).is_empty());
    }
}
