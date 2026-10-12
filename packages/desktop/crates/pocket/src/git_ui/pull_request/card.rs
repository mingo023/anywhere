use super::{CheckRows, check_rows, fix_summary, fixing_text, gate_text, place, resolving_text, review_lines, tally, to_fix};
use crate::desktop::Desktop;
use crate::git_ui::changes::commit_box::split_button;
use git::github::{Check, Gate, Method, Outcome, Pr, PrState, Review, ThreadState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

const CHECKS_SHOWN: usize = 6;
const COMMENTS_SHOWN: usize = 3;

pub(super) fn small(id: impl Into<ElementId>, v: Variant, label: impl Into<SharedString>) -> Stateful<Div> {
    ui::button(id, v, None, label.into()).h(px(24.)).px(px(9.)).gap(px(5.)).rounded(px(7.)).text_size(px(12.))
}

pub(super) fn pill(label: impl Into<SharedString>, bg: Token, fg: Token) -> Div {
    div().h(px(18.)).px(px(6.)).flex().flex_none().items_center().rounded(px(5.)).bg(bg).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(fg).child(label.into())
}

/// How far a thread has got, beside where it is; nothing while it's open.
pub(super) fn thread_chip(state: ThreadState, agent: &str) -> Option<Div> {
    match state {
        ThreadState::Sent => Some(pill(format!("{agent} fixing"), WAITING_BG, WAITING_TEXT)),
        ThreadState::Replied => Some(pill("Replied", ACCENT_BG, TEXT)),
        ThreadState::Resolved => Some(pill("Resolved", SUCCESS_BG, SUCCESS_TEXT)),
        ThreadState::Open => None,
    }
}

pub(super) fn who(login: &str) -> String {
    login.chars().filter(|c| c.is_alphanumeric()).take(2).collect::<String>().to_uppercase()
}

pub(super) fn review_icon(r: Review) -> AnyElement {
    match r {
        Review::Approved => icon("check", 13., SUCCESS).into_any_element(),
        Review::ChangesRequested => icon("warning", 13., FAILED).into_any_element(),
        Review::Required => icon("clock", 13., TEXT_3).into_any_element(),
        Review::None => glyph_slot(ui::dot(6., TEXT_4)).into_any_element(),
    }
}

pub(super) fn row() -> Div {
    div().min_h(px(26.)).flex().items_center().gap(px(8.)).text_size(px(12.5)).text_color(TEXT_2)
}

/// A labelled line of the card: the label in a narrow column, so it costs no row of its own.
fn fact(label: &'static str, value: Div) -> Div {
    let label = div().w(px(52.)).h(px(26.)).flex_none().flex().items_center().text_size(px(12.)).text_color(TEXT_4).child(label);
    div().flex().items_start().gap(px(8.)).child(label).child(value.flex_1().min_w_0())
}

fn glyph_slot(glyph: impl IntoElement) -> Div {
    div().size(px(13.)).flex_none().flex().items_center().justify_center().child(glyph)
}

fn callout(lead: impl IntoElement, bg: Token, title: String, sub: String, action: Option<Stateful<Div>>) -> Div {
    let text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(1.))
        .child(div().text_size(px(12.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title))
        .child(div().text_size(px(12.)).text_color(TEXT_3).child(sub));
    div().p(px(10.)).flex().items_center().gap(px(10.)).rounded(px(10.)).bg(bg).child(lead).child(text).children(action)
}

impl Desktop {
    /// The PR of the worktree on screen: where it stands in one line, and, unfolded, its checks, review and merge, and what can be handed to its agent.
    pub(crate) fn pr_card(&self, cx: &mut Context<Self>) -> Option<Div> {
        let (tree, pr) = self.shown_pr()?;
        let agent = self.fix_agent();
        let can_send = !self.agents.observe_only();
        let fixing = self.pr.track.fixing(&tree, pr);
        let busy = self.pr.busy.filter(|_| self.pr.composing.is_none());
        let (color, said) = self.pr_verdict(&tree, pr);
        let open = pr.state == PrState::Open;
        let merged = pr.state == PrState::Merged && self.deletable(&tree);
        // An unanswered "Delete the worktree?" stays in sight, however the card was left.
        let asks = merged && !self.pr.track.kept(&tree);
        let unfolds = (open || merged) && !asks;
        let folded = !asks && (self.pr.track.folded || !unfolds);
        let url = pr.url.clone();
        let line = div()
            .flex()
            .items_center()
            .gap(px(5.))
            .text_size(px(12.))
            .text_color(TEXT_3)
            .whitespace_nowrap()
            .child(div().flex_none().font_family(MONO).text_color(color).child(format!("#{}", pr.number)))
            .child("·")
            .child(div().min_w_0().truncate().child(said));
        let title = div()
            .text_size(px(13.))
            .line_height(px(17.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT)
            .when(folded, |d| d.truncate())
            .when(!folded, |d| d.line_clamp(2))
            .child(pr.title.clone());
        let open_pr = ui::icon_button_sized("pr-open", "external", 22., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            cx.stop_propagation();
            this.open_pr(url.clone(), window, cx);
        }));
        let chevron = unfolds.then(|| glyph_slot(icon(if folded { "chevron-right" } else { "chevron-down" }, 13., TEXT_3)).size(px(22.)));
        let head = div()
            .id("pr-head")
            .pl(px(12.))
            .pr(px(10.))
            .py(px(11.))
            .flex()
            .items_start()
            .gap(px(10.))
            .rounded(px(12.))
            .child(div().pt(px(1.)).child(icon(if pr.state == PrState::Merged { "merge" } else { "pull-request" }, 16., color)))
            .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(title).child(line))
            .child(div().mt(px(-2.)).flex().flex_none().items_center().gap(px(2.)).child(open_pr).children(chevron))
            .when(unfolds, |d| {
                d.cursor_pointer().hover(|s| s.bg(FILL_1)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.pr.track.folded = !this.pr.track.folded;
                    this.pr.merge_menu = false;
                    cx.notify();
                }))
            });
        let card = div().flex().flex_col().rounded(px(12.)).bg(SURFACE).shadow(ui::row_shadow()).child(head);
        // The body's slot stays when folded, so its spring knows an unfold from the card first showing.
        let wrap = |card: Div, body: Option<Div>| {
            let body = div().children(body).with_spring("pr-body", SpringAnimation::new(SPRING).to(!folded), |d, t| d.opacity(t.0).mt(px(-4. * (1. - t.0))));
            Some(div().flex_none().px(px(10.)).pb(px(8.)).child(card.child(body)))
        };
        if folded {
            return wrap(card, None);
        }
        let (checks, threads) = to_fix(pr, fixing);
        let resolving = fixing.is_some_and(|s| s.conflict);
        let items = fixing.map_or(0, |s| s.items());
        let callout = match pr.state {
            PrState::Open if items > 0 => {
                Some(callout(spinner("pr-fixing", 14., WAITING), WAITING_BG, fixing_text(agent, items), "Its session shows Working until it pushes".into(), None))
            }
            PrState::Open if !checks.is_empty() || !threads.is_empty() => {
                let (title, sub) = fix_summary(checks.len(), threads.len());
                let names: Vec<String> = checks.iter().map(|c| c.name.clone()).collect();
                let ids: Vec<String> = threads.iter().map(|t| t.id.clone()).collect();
                let send = can_send.then(|| {
                    small("pr-send-all", Variant::Primary, format!("Send all to {agent}")).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_fix(&names, &ids, cx)))
                });
                Some(callout(icon("warning", 16., FAILED), FAILED_BG, title, sub, send))
            }
            _ => None,
        };
        let facts = open.then(|| {
            div()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(fact("Checks", self.pr_checks(pr, can_send, agent, &checks, cx)))
                .child(fact("Review", self.pr_review(&tree, pr, can_send && fixing.is_none(), agent, cx)))
                .child(fact("Merge", self.pr_gate(pr, resolving, agent)))
        });
        let action = if open { self.pr_merge(&tree, pr, resolving, busy, agent, cx) } else { self.pr_merged(&tree, cx) };
        wrap(card, Some(div().px(px(12.)).pb(px(12.)).flex().flex_col().gap(px(10.)).children(callout).children(facts).child(action)))
    }

    fn pr_checks(&self, pr: &Pr, can_send: bool, agent: &str, unsent: &[&Check], cx: &mut Context<Self>) -> Div {
        if pr.runs.is_empty() {
            return row().text_color(TEXT_3).child(glyph_slot(ui::dot(6., TEXT_4))).child("No checks on this branch");
        }
        let CheckRows { named, passed, skipped } = check_rows(pr);
        let hidden = named.len().saturating_sub(CHECKS_SHOWN);
        let rows = named.into_iter().take(CHECKS_SHOWN).enumerate().map(|(i, check)| {
            if check.outcome == Outcome::Pending {
                return row()
                    .child(glyph_slot(spinner(("pr-run", i), 11., WAITING)))
                    .child(div().flex_1().min_w_0().truncate().child(check.name.clone()))
                    .child(div().flex_none().font_family(MONO).text_size(px(11.5)).text_color(TEXT_4).child("in progress"));
            }
            let (log, name) = (check.url.clone(), check.name.clone());
            let sent = !unsent.iter().any(|u| u.name == check.name);
            let send = (can_send && !sent).then(|| {
                small(("pr-check-send", i), Variant::Secondary, "Send").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_fix(std::slice::from_ref(&name), &[], cx)))
            });
            let trail = div()
                .flex()
                .flex_none()
                .gap(px(4.))
                .when(sent, |d| d.child(pill(format!("{agent} fixing"), WAITING_BG, WAITING_TEXT)))
                .when(!sent && !log.is_empty(), |d| d.child(small(("pr-check-log", i), Variant::Secondary, "Log").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_pr(log.clone(), window, cx)))))
                .children(send);
            row()
                .child(glyph_slot(icon("x", 13., FAILED)))
                .child(div().flex_1().min_w_0().truncate().text_color(FAILED_TEXT).font_weight(FontWeight::MEDIUM).child(check.name.clone()))
                .child(trail)
        });
        let more = (hidden > 0).then(|| row().text_color(TEXT_4).child(glyph_slot(div())).child(format!("{hidden} more")));
        let quiet = (passed + skipped > 0).then(|| {
            row().child(glyph_slot(icon("check", 13., SUCCESS))).child(div().flex_1().min_w_0().truncate().child(tally([(passed, "passed"), (skipped, "skipped")])))
        });
        div().flex().flex_col().children(rows).children(more).children(quiet)
    }

    fn pr_review(&self, tree: &str, pr: &Pr, can_send: bool, agent: &str, cx: &mut Context<Self>) -> Div {
        let unresolved = pr.unresolved().count();
        let all = (!pr.threads.is_empty()).then(|| {
            let label = if unresolved > 0 { format!("{unresolved} unresolved · All comments") } else { "All comments".into() };
            row().child(ui::link("pr-all-comments", label).text_size(px(12.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.pr.track.comments = true;
                cx.notify();
            })))
        });
        let draft = pr.draft && pr.state == PrState::Open;
        let lines = review_lines(pr).into_iter().enumerate().map(|(i, (r, text))| {
            row().child(review_icon(r)).child(div().flex_1().min_w_0().truncate().child(text)).when(i == 0 && draft, |d| d.child(pill("Draft", FILL_3, TEXT_3)))
        });
        let comments = pr.unresolved().take(COMMENTS_SHOWN).enumerate().map(|(i, t)| {
            let state = t.state(self.pr.track.thread_sent(tree, pr, &t.id));
            let first = t.comments.first();
            let body = first.map(|c| c.body.lines().next().unwrap_or_default().to_string()).unwrap_or_default();
            let (id, open) = (t.id.clone(), t.id.clone());
            let send = (can_send && state == ThreadState::Open).then(|| {
                small(("pr-comment-send", i), Variant::Secondary, "Send").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.send_fix(&[], std::slice::from_ref(&id), cx);
                }))
            });
            let at = div().flex().items_center().gap(px(6.)).font_family(MONO).text_size(px(11.5)).text_color(TEXT_3).child(div().min_w_0().truncate().child(place(t))).children(thread_chip(state, agent));
            div()
                .id(("pr-comment", i))
                .px(px(6.))
                .py(px(6.))
                .mx(px(-6.))
                .flex()
                .items_center()
                .gap(px(8.))
                .rounded(px(8.))
                .cursor_pointer()
                .hover(|s| s.bg(FILL_2))
                .child(ui::avatar(&who(first.map_or("", |c| c.author.as_str())), 22.))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(at).child(div().truncate().text_size(px(12.5)).text_color(TEXT_2).child(body)))
                .children(send)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.view_thread(&open, cx)))
        });
        div().flex().flex_col().children(lines).children(comments).children(all)
    }

    /// What holds the merge back, or that nothing does.
    fn pr_gate(&self, pr: &Pr, resolving: bool, agent: &str) -> Div {
        let gate = pr.gate();
        let (lead, color, text) = match gate {
            _ if resolving => (spinner("pr-resolving", 12., WAITING).into_any_element(), TEXT_2, resolving_text(agent)),
            Gate::Ready => (icon("check", 13., SUCCESS).into_any_element(), TEXT_2, gate_text(gate, &pr.base)),
            Gate::Conflicts => (icon("warning", 13., FAILED).into_any_element(), FAILED_TEXT, gate_text(gate, &pr.base)),
            Gate::Checking => (spinner("pr-checking", 12., TEXT_3).into_any_element(), TEXT_3, gate_text(gate, &pr.base)),
            _ => (ui::dot(6., TEXT_4).into_any_element(), TEXT_3, gate_text(gate, &pr.base)),
        };
        row().text_color(color).child(glyph_slot(lead)).child(div().flex_1().min_w_0().child(text))
    }

    /// Merge, or what stands in for it while the PR can't be: resolving its conflict, or marking it ready.
    fn pr_merge(&self, tree: &str, pr: &Pr, resolving: bool, busy: Option<&'static str>, agent: &str, cx: &mut Context<Self>) -> Div {
        let gate = pr.gate();
        let primary = match gate {
            Gate::Conflicts => {
                let b = ui::button("pr-resolve", Variant::Primary, None, format!("Resolve with {agent}")).h(px(30.)).justify_center();
                if resolving || self.agents.observe_only() { b.opacity(0.5).cursor_default() } else { b.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.send_conflict(cx))) }.into_any_element()
            }
            Gate::Draft => {
                let b = ui::button("pr-ready", Variant::Primary, None, busy.unwrap_or("Ready for review")).h(px(30.)).justify_center();
                if busy.is_some() { b.opacity(0.5).cursor_default() } else { b.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.mark_ready(cx))) }.into_any_element()
            }
            _ => {
                let method = self.pr.track.method;
                let lead = if busy.is_some() { spinner("pr-merging", 13., ON_PRIMARY).into_any_element() } else { icon("merge", 14., ON_PRIMARY).into_any_element() };
                let menu = div()
                    .id("merge-menu")
                    .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                        this.pr.merge_menu = false;
                        cx.notify();
                    }))
                    .children(Method::ALL.into_iter().enumerate().map(|(i, m)| {
                        ui::menu_row(("merge-method", i), "merge", m.label(), None).children((m == method).then(|| icon("check", 13., TEXT_2))).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.pr.track.method = m;
                            this.pr.merge_menu = false;
                            cx.notify();
                        }))
                    }));
                split_button(
                    "merge",
                    lead,
                    busy.unwrap_or(method.label()),
                    gate == Gate::Ready && busy.is_none(),
                    self.pr.merge_menu,
                    Box::new(|this, _, cx| this.merge_pr(cx)),
                    Box::new(|this, _, _| {
                        this.pr.merge_menu = !this.pr.merge_menu;
                        this.pr.ship_menu = false;
                    }),
                    menu,
                    cx,
                )
                .into_any_element()
            }
        };
        let after = self.pr.track.delete_after;
        let delete_after = self.deletable(tree).then(|| {
            div()
                .id("pr-delete-after")
                .flex()
                .items_center()
                .gap(px(8.))
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(TEXT_3)
                .child(ui::checkbox(after))
                .child(div().min_w_0().truncate().child("Delete the worktree after merging · keeps the branch"))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.pr.track.delete_after = !this.pr.track.delete_after;
                    cx.notify();
                }))
        });
        div().flex().flex_col().gap(px(8.)).child(primary).children(delete_after)
    }

    fn pr_merged(&self, tree: &str, cx: &mut Context<Self>) -> Div {
        let branch = self.repo().map(|r| r.branch.clone()).unwrap_or_default();
        let kept = self.pr.track.kept(tree);
        let (keep_tree, delete_tree) = (tree.to_string(), tree.to_string());
        let keep = small("pr-keep", Variant::Secondary, if kept { "Kept" } else { "Keep" })
            .flex_1()
            .h(px(30.))
            .justify_center()
            .when(kept, |d| d.opacity(0.5).cursor_default())
            .when(!kept, |d| {
                d.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.pr.track.keep(&keep_tree);
                    cx.notify();
                }))
            });
        let delete = ui::button("pr-delete-tree", Variant::Primary, Some("trash"), "Delete worktree")
            .flex_1()
            .h(px(30.))
            .justify_center()
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.delete_merged(&delete_tree, cx)));
        let text = div().text_size(px(12.5)).text_color(TEXT_2).child(format!("Delete the worktree {branch}? Its terminals close; the branch stays."));
        div().flex().flex_col().gap(px(8.)).child(text).child(div().flex().gap(px(8.)).child(keep).child(delete))
    }
}
