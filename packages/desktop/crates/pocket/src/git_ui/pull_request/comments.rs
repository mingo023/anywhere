use super::body::selectable_text;
use super::card::{pill, small, small_with, thread_chip};
use super::{opening, spot};
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

/// The last `n` lines of a hunk, each with its line number in the new file; a removed line has none.
fn numbered_tail(hunk: &str, n: usize) -> Vec<(Option<u32>, char, &str)> {
    let mut at = 0;
    let mut lines = Vec::new();
    for l in hunk.lines() {
        if let Some(head) = l.strip_prefix("@@") {
            at = head.split('+').nth(1).and_then(|h| h.split([',', ' ']).next()?.parse::<u32>().ok()).map_or(0, |n| n.saturating_sub(1));
            continue;
        }
        let kind = l.chars().next().unwrap_or(' ');
        let number = (kind != '-').then(|| {
            at += 1;
            at
        });
        lines.push((number, kind, l.get(1..).unwrap_or("")));
    }
    lines.split_off(lines.len().saturating_sub(n))
}

fn code(t: &Thread) -> Option<Div> {
    let lines = numbered_tail(t.comments.first().map_or("", |c| c.hunk.as_str()), 3);
    (!lines.is_empty()).then(|| {
        div().py(px(3.)).bg(FILL_2).font_family(MONO).text_size(px(11.5)).overflow_hidden().children(lines.into_iter().map(|(n, kind, text)| {
            let (bg, fg, edge) = match kind {
                '+' => (Some(DIFF_ADD_BG), DIFF_ADD_TEXT, Some(SUCCESS)),
                '-' => (Some(DIFF_DEL_BG), DIFF_DEL_TEXT, Some(FAILED)),
                _ => (None, TEXT_2, None),
            };
            div()
                .h(px(18.))
                .flex()
                .items_center()
                .whitespace_nowrap()
                .text_color(fg)
                .when_some(bg, |d, bg| d.bg(bg))
                .when_some(edge, |d, edge| d.border_l_2().border_color(edge))
                .child(div().w(px(34.)).flex_none().pr(px(10.)).flex().justify_end().text_color(TEXT_4).children(n.map(|n| n.to_string())))
                .child(text.to_string())
        }))
    })
}

fn path_line(t: &Thread) -> Div {
    let cut = t.path.rfind('/').map_or(0, |i| i + 1);
    div()
        .min_w_0()
        .flex()
        .child(div().min_w_0().truncate().child(t.path[..cut].to_string()))
        .child(div().flex_none().text_color(TEXT_2).child(t.path[cut..].to_string()))
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

fn comment_text(t: &Thread, c: &Comment) -> impl IntoElement {
    selectable_text(SharedString::from(format!("pr-c:{}:{}", t.id, c.id)), c.body.trim().to_string())
}

impl Desktop {
    /// Threads of `pr` can go to the agent when sessions aren't only watched and it isn't fixing this PR already.
    pub(super) fn shown<'a>(&self, tree: &'a str, pr: &'a Pr, cx: &App) -> Shown<'a> {
        let can_send = !self.agents.observe_only() && self.pr.track.fixing(tree, pr).is_none();
        Shown { tree, pr, agent: self.fix_agent(cx), can_send }
    }

    /// Thread `id` of the PR on screen, as a card under its line in the PR diff.
    pub(crate) fn pinned_thread(&self, k: usize, id: &str, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (tree, pr) = self.shown_pr(cx)?;
        let t = pr.threads.iter().find(|t| t.id == id)?;
        Some(self.pr_thread(k, &self.shown(&tree, pr, cx), t, cx))
    }

    /// A thread on its own card under its line in the diff. A long one folds its middle replies.
    fn pr_thread(&self, i: usize, shown: &Shown, t: &Thread, cx: &mut Context<Self>) -> AnyElement {
        let (agent, can_send, now) = (shown.agent, shown.can_send, now_ms());
        let state = t.state(self.pr.track.thread_sent(shown.tree, shown.pr, &t.id));
        let frame = |d: Stateful<Div>| d.px(px(10.)).rounded(px(10.)).bg(SURFACE).shadow(ui::row_shadow());
        let at = at_line(t);
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
            .child(div().flex_1());
        let comment = |c: &Comment| {
            let name = div()
                .flex()
                .items_center()
                .gap(px(6.))
                .text_size(px(12.))
                .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(c.author.clone()))
                .child(div().text_color(TEXT_4).child(ago(c.at as i64, now)));
            div()
                .flex()
                .items_start()
                .gap(px(8.))
                .child(self.face(&c.author, 20.))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .gap(px(2.))
                        .child(name)
                        .child(div().text_size(px(12.5)).text_color(TEXT_BODY).child(comment_text(t, c))),
                )
                .into_any_element()
        };
        let said: Vec<AnyElement> = match folded_replies(t.comments.len(), whole) {
            None => t.comments.iter().map(comment).collect(),
            Some(hidden) => {
                let faces = authors(&t.comments[hidden.clone()])
                    .into_iter()
                    .take(3)
                    .enumerate()
                    .map(|(k, a)| self.face(a, 16.).shadow(vec![ui::ring(SURFACE, 1.5)]).when(k > 0, |d| d.ml(px(-3.))));
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
                t.comments
                    .first()
                    .map(comment)
                    .into_iter()
                    .chain([div().flex().child(more).into_any_element()])
                    .chain(t.comments[hidden.end..].iter().map(comment))
                    .collect()
            }
        };
        let id = t.id.clone();
        let foot = if self.pr.replying.as_deref() == Some(t.id.as_str()) {
            let field = div()
                .px(px(10.))
                .py(px(7.))
                .rounded(px(9.))
                .bg(SURFACE)
                .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
                .text_size(px(12.5))
                .child(Textarea::new(&self.pr.reply).appearance(false));
            let ready = !self.pr.reply.read(cx).value().trim().is_empty();
            let send = small("pr-reply-send", Variant::Primary, "Reply")
                .when(!ready, |d| d.opacity(0.5).cursor_default())
                .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.send_reply(window, cx))));
            let actions = div()
                .flex()
                .items_center()
                .gap(px(4.))
                .child(div().flex_1().text_size(px(11.5)).text_color(TEXT_4).child(format!("Posts to GitHub as you · {}", ui::os_keys("⌘↩"))))
                .child(
                    small("pr-reply-cancel", Variant::Secondary, "Cancel")
                        .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.cancel_reply(window, cx))),
                )
                .child(send);
            div().flex().flex_col().gap(px(6.)).child(field).child(actions)
        } else {
            let reply = {
                let id = id.clone();
                small(("pr-reply", i), Variant::Secondary, "Reply")
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_reply(id.clone(), window, cx)))
            };
            let resolve = |v: Variant, resolved: bool| {
                let id = id.clone();
                small(("pr-resolve", i), v, if resolved { "Resolve" } else { "Unresolve" })
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.resolve_thread(id.clone(), resolved, true, cx)))
            };
            let actions: Vec<Stateful<Div>> = match state {
                ThreadState::Open => {
                    let send = can_send.then(|| {
                        let id = id.clone();
                        small(("pr-thread-send", i), Variant::Primary, format!("Send to {agent}"))
                            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_fix(&[], std::slice::from_ref(&id), cx)))
                    });
                    send.into_iter().chain([reply, resolve(Variant::Secondary, true)]).collect()
                }
                ThreadState::Sent => vec![reply],
                ThreadState::Replied => vec![resolve(Variant::Primary, true), reply],
                ThreadState::Resolved => {
                    vec![resolve(Variant::Secondary, false), small(("pr-collapse", i), Variant::Ghost, "Collapse").on_click(toggle(id.clone()))]
                }
            };
            div().flex().flex_wrap().gap(px(4.)).children(actions)
        };
        frame(div().id(("pr-thread", i))).py(px(10.)).flex().flex_col().gap(px(8.)).child(top).children(said).child(foot).into_any_element()
    }

    /// A thread in the Conversation, under the review that opened it: its file and line, the code, what was said, and what to do with it. A resolved one folds to a line.
    pub(super) fn conv_thread(&self, i: usize, shown: &Shown, t: &Thread, cx: &mut Context<Self>) -> AnyElement {
        let (agent, can_send, now) = (shown.agent, shown.can_send, now_ms());
        let state = t.state(self.pr.track.thread_sent(shown.tree, shown.pr, &t.id));
        let whole = self.pr.track.expanded.contains(&t.id);
        let toggle = |id: String| {
            cx.listener(move |this: &mut Desktop, _: &ClickEvent, _, cx| {
                this.pr.track.toggle_expanded(&id);
                cx.notify();
            })
        };
        let frame = |d: Stateful<Div>| d.mx(px(8.)).mb(px(8.)).rounded(px(9.)).border(px(0.5)).border_color(SEPARATOR_STRONG).overflow_hidden();
        let outdated = t.outdated.then(|| pill("Outdated", FILL_3, TEXT_3).font_weight(FontWeight::MEDIUM));
        if state == ThreadState::Resolved && !whole {
            let n = t.comments.len();
            return frame(div().id(("pr-thread-min", i)))
                .h(px(32.))
                .px(px(10.))
                .flex()
                .items_center()
                .gap(px(7.))
                .cursor_pointer()
                .hover(|s| s.bg(FILL_1))
                .text_size(px(11.5))
                .text_color(TEXT_3)
                .child(icon("check", 12., SUCCESS))
                .child(div().min_w_0().truncate().font_family(MONO).child(spot(t)))
                .children(outdated)
                .child(div().flex_1())
                .child(div().flex_none().child(if n == 1 { "1 comment".to_string() } else { format!("{n} comments") }))
                .child(icon("chevron-right", 12., TEXT_3))
                .on_click(toggle(t.id.clone()))
                .into_any_element();
        }
        let view = t.id.clone();
        let head = div()
            .id(("pr-thread-head", i))
            .h(px(30.))
            .pl(px(10.))
            .pr(px(6.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded_t(px(8.5))
            .bg(FILL_1)
            .cursor_pointer()
            .font_family(MONO)
            .text_size(px(11.5))
            .text_color(TEXT_3)
            .child(icon("file", 12., TEXT_3))
            .child(path_line(t))
            .children(t.line.map(|l| div().flex_none().child(format!("L{l}"))))
            .children(outdated)
            .child(div().flex_1())
            .children(thread_chip(state))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.view_thread(&view, cx)));
        let said = |c: &Comment| {
            let name = div()
                .flex()
                .items_center()
                .gap(px(6.))
                .child(div().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(c.author.clone()))
                .child(div().text_size(px(11.5)).text_color(TEXT_4).child(ago(c.at as i64, now)));
            div()
                .pt(px(9.))
                .px(px(10.))
                .flex()
                .gap(px(8.))
                .text_size(px(12.5))
                .line_height(px(18.))
                .text_color(TEXT_BODY)
                .child(self.face(&c.author, 20.))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(1.)).child(name).child(comment_text(t, c)))
                .into_any_element()
        };
        let rows: Vec<AnyElement> = match folded_replies(t.comments.len(), whole) {
            None => t.comments.iter().map(said).collect(),
            Some(hidden) => {
                let more = div()
                    .id(("pr-more", i))
                    .mt(px(6.))
                    .ml(px(38.))
                    .mr(px(10.))
                    .cursor_pointer()
                    .text_size(px(12.))
                    .text_color(TEXT_3)
                    .hover(|s| s.text_color(TEXT))
                    .child(format!("Show {} more replies", hidden.len()))
                    .on_click(toggle(t.id.clone()));
                t.comments.first().map(said).into_iter().chain([more.into_any_element()]).chain(t.comments[hidden.end..].iter().map(said)).collect()
            }
        };
        let id = t.id.clone();
        let foot = if self.pr.replying.as_deref() == Some(t.id.as_str()) {
            let ready = !self.pr.reply.read(cx).value().trim().is_empty();
            let send = small("pr-reply-send", Variant::Primary, "Reply")
                .when(!ready, |d| d.opacity(0.5).cursor_default())
                .when(ready, |d| d.on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.send_reply(window, cx))));
            let actions = div()
                .px(px(6.))
                .pt(px(4.))
                .pb(px(6.))
                .flex()
                .items_center()
                .gap(px(4.))
                .child(div().flex_1().pl(px(4.)).text_size(px(11.)).text_color(TEXT_4).child(ui::os_keys("⌘↩")))
                .child(
                    small("pr-reply-cancel", Variant::Ghost, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.cancel_reply(window, cx))),
                )
                .child(send);
            let field = div().px(px(10.)).pt(px(8.)).text_size(px(12.5)).child(Textarea::new(&self.pr.reply).appearance(false));
            div()
                .mt(px(8.))
                .mr(px(8.))
                .mb(px(8.))
                .ml(px(36.))
                .rounded(px(8.))
                .bg(FILL_2)
                .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])
                .child(field)
                .child(actions)
        } else {
            let reply = {
                let id = id.clone();
                small_with(("pr-reply", i), Variant::Ghost, "reply", "Reply")
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_reply(id.clone(), window, cx)))
            };
            let send = (can_send && state == ThreadState::Open).then(|| {
                let id = id.clone();
                small(("pr-thread-send", i), Variant::Secondary, format!("Send to {agent}"))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_fix(&[], std::slice::from_ref(&id), cx)))
            });
            let resolved = state == ThreadState::Resolved;
            let resolve = small(("pr-resolve", i), Variant::Ghost, if resolved { "Reopen" } else { "Resolve" })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.resolve_thread(id.clone(), !resolved, true, cx)));
            div().pt(px(8.)).pr(px(8.)).pb(px(8.)).pl(px(36.)).flex().gap(px(4.)).child(reply).children(send).child(div().flex_1()).child(resolve)
        };
        frame(div().id(("pr-thread", i)))
            .when(state == ThreadState::Resolved, |d| d.opacity(0.72))
            .child(head)
            .children(code(t))
            .children(rows)
            .child(foot)
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{authors, folded_replies, numbered_tail};
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

    #[test]
    fn a_hunks_tail_numbers_its_lines_in_the_new_file_and_leaves_removed_ones_unnumbered() {
        let hunk = "@@ -40,3 +41,3 @@\n a\n-b\n+c\n d";
        assert_eq!(numbered_tail(hunk, 3), vec![(None, '-', "b"), (Some(42), '+', "c"), (Some(43), ' ', "d")]);
        assert!(numbered_tail("", 3).is_empty());
    }
}
