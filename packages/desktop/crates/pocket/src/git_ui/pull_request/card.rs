use super::{duration, fix_summary, gate_text, place, review_lines, shown_checks, to_fix};
use crate::desktop::Desktop;
use crate::git_ui::changes::commit_box::split_button;
use crate::git_ui::pull_requests::{detail, tone};
use git::github::{Check, Gate, Method, Outcome, Pr, PrState, Review, ThreadState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

const MERGED_BG: Token = Token::fixed(0x8250df1f);
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
        Review::None => div().size(px(13.)).flex().items_center().justify_center().child(ui::dot(6., TEXT_4)).into_any_element(),
    }
}

pub(super) fn row() -> Div {
    div().min_h(px(28.)).flex().items_center().gap(px(8.)).text_size(px(12.5)).text_color(TEXT_2)
}

fn section(label: &str) -> Div {
    div().pt(px(6.)).flex().items_center().gap(px(6.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(label.to_string())
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
    /// The PR of the worktree on screen: where its checks, review and merge stand, and what can be handed to its agent.
    pub(crate) fn pr_card(&self, cx: &mut Context<Self>) -> Option<Div> {
        let (tree, pr) = self.shown_pr()?;
        let agent = self.fix_agent();
        let can_send = !self.agents.observe_only();
        let fixing = self.pr.track.fixing(&tree, pr);
        let url = pr.url.clone();
        let (glyph, color) = match pr.state {
            PrState::Merged => ("merge", MERGED),
            _ if pr.draft => ("pull-request", TEXT_4),
            _ => ("pull-request", tone(pr)),
        };
        let line = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .text_size(px(12.))
            .text_color(TEXT_3)
            .whitespace_nowrap()
            .child(div().flex_none().font_family(MONO).text_color(tone(pr)).child(format!("#{}", pr.number)))
            .child("·")
            .child(div().min_w_0().truncate().child(detail(pr)));
        let head = div()
            .flex()
            .items_start()
            .gap(px(10.))
            .child(div().pt(px(1.)).child(icon(glyph, 16., color)))
            .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(div().text_size(px(13.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(pr.title.clone())).child(line))
            .child(ui::icon_button_sized("pr-open", "external", 22., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_pr(url.clone(), window, cx))));
        let open = pr.state == PrState::Open;
        let (checks, threads) = to_fix(pr, fixing);
        let callout = match pr.state {
            PrState::Merged => Some(callout(icon("merge", 16., MERGED), MERGED_BG, format!("Merged into {}", pr.base), format!("#{} · {}", pr.number, pr.title), None)),
            PrState::Open if fixing.is_some_and(|s| s.items() > 0) => {
                let n = fixing.map_or(0, |s| s.items());
                let title = format!("{agent} is fixing {}", if n == 1 { "1 item".into() } else { format!("{n} items") });
                Some(callout(spinner("pr-fixing", 14., WAITING), WAITING_BG, title, "Its session shows Working until it pushes".into(), None))
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
        let card = div()
            .p(px(12.))
            .flex()
            .flex_col()
            .gap(px(6.))
            .rounded(px(12.))
            .bg(SURFACE)
            .shadow(ui::row_shadow())
            .child(head)
            .children(callout)
            .child(self.pr_checks(pr, can_send && open, agent, &checks, cx))
            .child(self.pr_review(&tree, pr, can_send && open && fixing.is_none(), agent, cx))
            .children(if open { self.pr_merge(&tree, pr, agent, cx) } else { self.pr_merged(&tree, pr, cx) });
        Some(div().flex_none().px(px(10.)).pb(px(8.)).child(card))
    }

    fn pr_checks(&self, pr: &Pr, can_send: bool, agent: &str, unsent: &[&Check], cx: &mut Context<Self>) -> Div {
        let c = pr.checks;
        let total = c.passed + c.failed + c.pending;
        let head = section("Checks").children((total > 0).then(|| div().font_weight(FontWeight::MEDIUM).text_color(TEXT_4).child(format!("{}/{total}", c.passed))));
        let (runs, hidden) = shown_checks(pr, CHECKS_SHOWN);
        let rows = runs.into_iter().enumerate().map(|(i, check)| {
            let glyph = match check.outcome {
                Outcome::Passed => icon("check", 13., SUCCESS).into_any_element(),
                Outcome::Failed => icon("x", 13., FAILED).into_any_element(),
                Outcome::Pending => div().size(px(13.)).flex().items_center().justify_center().child(spinner(("pr-run", i), 11., WAITING)).into_any_element(),
                Outcome::Skipped => div().size(px(13.)).flex().items_center().justify_center().child(ui::dot(6., TEXT_4)).into_any_element(),
            };
            let failed = check.outcome == Outcome::Failed;
            let name = div().flex_1().min_w_0().truncate().when(failed, |d| d.text_color(FAILED_TEXT).font_weight(FontWeight::MEDIUM)).child(check.name.clone());
            let trail = if failed {
                let (log, name) = (check.url.clone(), check.name.clone());
                let send = (can_send && unsent.iter().any(|u| u.name == check.name)).then(|| {
                    small(("pr-check-send", i), Variant::Secondary, format!("Send to {agent}")).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_fix(std::slice::from_ref(&name), &[], cx)))
                });
                div()
                    .flex()
                    .flex_none()
                    .gap(px(4.))
                    .when(!log.is_empty(), |d| d.child(small(("pr-check-log", i), Variant::Secondary, "Log").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_pr(log.clone(), window, cx)))))
                    .children(send)
                    .into_any_element()
            } else {
                let when = match check.outcome {
                    Outcome::Pending => "in progress".to_string(),
                    Outcome::Skipped => "skipped".to_string(),
                    _ => check.secs.map(duration).unwrap_or_default(),
                };
                div().flex_none().font_family(MONO).text_size(px(11.5)).text_color(TEXT_4).child(when).into_any_element()
            };
            row().child(glyph).child(name).child(trail)
        });
        let more = (hidden > 0).then(|| row().text_color(TEXT_4).child(div().w(px(13.))).child(format!("{hidden} more")));
        let none = pr.runs.is_empty().then(|| row().text_color(TEXT_3).child(div().w(px(13.))).child("No checks"));
        div().flex().flex_col().child(head).children(rows).children(more).children(none)
    }

    fn pr_review(&self, tree: &str, pr: &Pr, can_send: bool, agent: &str, cx: &mut Context<Self>) -> Div {
        let unresolved = pr.unresolved().count();
        let all = (!pr.threads.is_empty()).then(|| {
            let label = if unresolved > 0 { format!("{unresolved} unresolved · All comments") } else { "All comments".into() };
            ui::link("pr-all-comments", label).text_size(px(12.)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.pr.track.comments = true;
                cx.notify();
            }))
        });
        let head = section("Review").child(div().flex_1()).children(all);
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
        div().flex().flex_col().child(head).children(lines).children(comments)
    }

    fn pr_merge(&self, tree: &str, pr: &Pr, agent: &str, cx: &mut Context<Self>) -> Option<Div> {
        let gate = pr.gate();
        let resolving = self.pr.track.fixing(tree, pr).is_some_and(|s| s.conflict);
        let (lead, color, text) = match gate {
            _ if resolving => (spinner("pr-resolving", 12., WAITING).into_any_element(), TEXT_2, format!("{agent} is resolving the conflict")),
            Gate::Ready => (icon("check", 13., SUCCESS).into_any_element(), TEXT_2, gate_text(gate, &pr.base)),
            Gate::Conflicts => (icon("warning", 13., FAILED).into_any_element(), FAILED_TEXT, gate_text(gate, &pr.base)),
            Gate::Checking => (spinner("pr-checking", 12., TEXT_3).into_any_element(), TEXT_3, gate_text(gate, &pr.base)),
            _ => (div().size(px(13.)).flex().items_center().justify_center().child(ui::dot(6., TEXT_4)).into_any_element(), TEXT_3, gate_text(gate, &pr.base)),
        };
        let gate_row = row().text_color(color).child(div().size(px(13.)).flex().items_center().justify_center().child(lead)).child(div().flex_1().min_w_0().child(text));
        let busy = self.pr.busy.filter(|_| self.pr.composing.is_none());
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
        Some(div().flex().flex_col().gap(px(6.)).child(section("Merge")).child(gate_row).child(primary).children(delete_after))
    }

    fn pr_merged(&self, tree: &str, pr: &Pr, cx: &mut Context<Self>) -> Option<Div> {
        if pr.state != PrState::Merged || !self.deletable(tree) {
            return None;
        }
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
        Some(div().pt(px(6.)).flex().flex_col().gap(px(8.)).child(text).child(div().flex().gap(px(8.)).child(keep).child(delete)))
    }
}
