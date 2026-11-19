use super::{CheckRows, PrTab, check_rows, gate_text, opening, plural, resolving_text, review_lines, spot, tally, to_fix};
use crate::desktop::Desktop;
use git::github::{Check, Gate, Outcome, Pr, PrState, Review, ThreadState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

const CHECKS_SHOWN: usize = 6;
const COMMENTS_SHOWN: usize = 3;

pub(crate) fn small(id: impl Into<ElementId>, v: Variant, label: impl Into<SharedString>) -> Stateful<Div> {
    ui::button(id, v, None, label.into()).h(px(24.)).px(px(9.)).gap(px(5.)).rounded(px(7.)).text_size(px(12.))
}

pub(super) fn pill(label: impl Into<SharedString>, bg: Token, fg: Token) -> Div {
    div().h(px(18.)).px(px(6.)).flex().flex_none().items_center().rounded(px(5.)).bg(bg).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(fg).child(label.into())
}

/// Marks an item handed to the agent; the card's header carries who and the spinner.
fn fixing_mark() -> Div {
    div().flex().flex_none().items_center().gap(px(5.)).text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(WAITING_TEXT).child(ui::dot(6., WAITING)).child("Fixing")
}

/// How far a thread has got, beside where it is; nothing while it's open.
pub(super) fn thread_chip(state: ThreadState) -> Option<Div> {
    match state {
        ThreadState::Sent => Some(fixing_mark()),
        ThreadState::Replied => Some(pill("Replied", ACCENT_BG, TEXT)),
        ThreadState::Resolved => Some(pill("Resolved", SUCCESS_BG, SUCCESS_TEXT)),
        ThreadState::Open => None,
    }
}

pub(super) fn who(login: &str) -> String {
    login.chars().filter(|c| c.is_alphanumeric()).take(2).collect::<String>().to_uppercase()
}

pub(crate) fn review_icon(r: Review) -> AnyElement {
    match r {
        Review::Approved => icon("check", 13., SUCCESS).into_any_element(),
        Review::ChangesRequested => icon("warning", 13., FAILED).into_any_element(),
        Review::Required => icon("clock", 13., TEXT_3).into_any_element(),
        Review::None => glyph_slot(ui::dot(6., TEXT_4)).into_any_element(),
    }
}

pub(crate) fn row() -> Div {
    div().min_h(px(26.)).flex().items_center().gap(px(8.)).text_size(px(12.5)).text_color(TEXT_2)
}

pub(crate) fn glyph_slot(glyph: impl IntoElement) -> Div {
    div().size(px(13.)).flex_none().flex().items_center().justify_center().child(glyph)
}

pub(super) fn section(title: &'static str, status: Option<(Token, String)>) -> Div {
    let status = status.map(|(color, text)| div().flex_1().min_w_0().flex().justify_end().child(div().min_w_0().truncate().text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(color).child(text)));
    div().pt(px(16.)).pb(px(6.)).px(px(12.)).flex().items_center().gap(px(8.)).child(div().flex_none().text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_3).child(title)).children(status)
}

pub(super) fn group(rows: impl IntoIterator<Item = AnyElement>) -> Div {
    let rows = rows.into_iter().enumerate().map(|(i, r)| div().when(i > 0, |d| d.border_t_1().border_color(HAIRLINE)).child(r));
    div().px(px(12.)).flex().flex_col().rounded(px(12.)).bg(SURFACE).shadow(ui::row_shadow()).children(rows)
}

pub(super) fn group_row() -> Div {
    row().min_h(px(36.))
}

fn checks_status(pr: &Pr) -> Option<(Token, String)> {
    let CheckRows { named, passed, .. } = check_rows(pr);
    let failed = named.iter().filter(|c| c.outcome == Outcome::Failed).count();
    let pending = named.len() - failed;
    if pr.runs.is_empty() {
        None
    } else if failed > 0 {
        Some((FAILED_TEXT, tally([(failed, "failed"), (passed, "passed")])))
    } else if pending > 0 {
        Some((WAITING_TEXT, format!("{} in progress", plural(pending, "check"))))
    } else {
        Some((SUCCESS_TEXT, "All passed".into()))
    }
}

pub(super) fn review_status(pr: &Pr) -> Option<(Token, String)> {
    match pr.review {
        Review::ChangesRequested => Some((FAILED_TEXT, "Changes requested".into())),
        Review::Approved => Some((SUCCESS_TEXT, "Approved".into())),
        Review::Required => Some((TEXT_3, "Review required".into())),
        Review::None => None,
    }
}

impl Desktop {
    /// The Checks tab: each check that needs a look, the review, and what holds the merge back.
    pub(crate) fn pr_checks_tab(&self, tree: &str, pr: &Pr, cx: &mut Context<Self>) -> Stateful<Div> {
        let fixing = self.pr.track.fixing(tree, pr);
        let can_send = !self.agents.observe_only();
        let (checks, _) = to_fix(pr, fixing.as_ref());
        let resolving = fixing.as_ref().is_some_and(|s| s.conflict);
        let merge = (pr.state == PrState::Open).then(|| {
            let gate = div().py(px(10.)).child(self.pr_gate(pr, resolving, self.fix_agent()));
            div().child(section("MERGE", None)).child(group([gate.into_any_element()]))
        });
        div()
            .id("pr-checks")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .px(px(10.))
            .pb(px(16.))
            .flex()
            .flex_col()
            .child(section("CHECKS", checks_status(pr)))
            .child(group(self.pr_checks(pr, can_send, &checks, cx)))
            .child(section("REVIEW", review_status(pr)))
            .child(group(self.pr_review(tree, pr, can_send && fixing.is_none(), cx)))
            .children(merge)
    }

    fn pr_checks(&self, pr: &Pr, can_send: bool, unsent: &[&Check], cx: &mut Context<Self>) -> Vec<AnyElement> {
        if pr.runs.is_empty() {
            return vec![group_row().text_color(TEXT_3).child(glyph_slot(ui::dot(6., TEXT_4))).child("No checks on this branch").into_any_element()];
        }
        let CheckRows { named, passed, skipped } = check_rows(pr);
        let hidden = named.len().saturating_sub(CHECKS_SHOWN);
        let rows = named.into_iter().take(CHECKS_SHOWN).enumerate().map(|(i, check)| {
            if check.outcome == Outcome::Pending {
                return group_row()
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
                .when(sent, |d| d.child(fixing_mark()))
                .when(!sent && !log.is_empty(), |d| d.child(small(("pr-check-log", i), Variant::Ghost, "Log").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_pr(log.clone(), window, cx)))))
                .children(send);
            group_row()
                .child(glyph_slot(icon("x", 13., FAILED)))
                .child(div().flex_1().min_w_0().truncate().text_color(FAILED_TEXT).font_weight(FontWeight::MEDIUM).child(check.name.clone()))
                .child(trail)
        });
        let more = (hidden > 0).then(|| group_row().text_color(TEXT_4).child(glyph_slot(div())).child(format!("{hidden} more")));
        let quiet = (passed + skipped > 0).then(|| {
            group_row().child(glyph_slot(icon("check", 13., SUCCESS))).child(div().flex_1().min_w_0().truncate().child(tally([(passed, "passed"), (skipped, "skipped")])))
        });
        rows.chain(more).chain(quiet).map(IntoElement::into_any_element).collect()
    }

    fn pr_review(&self, tree: &str, pr: &Pr, can_send: bool, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let unresolved = pr.unresolved().count();
        let draft = pr.draft && pr.state == PrState::Open;
        let lines = review_lines(pr).into_iter().enumerate().map(|(i, (r, text))| {
            group_row().child(review_icon(r)).child(div().flex_1().min_w_0().truncate().child(text)).when(i == 0 && draft, |d| d.child(pill("Draft", FILL_3, TEXT_3))).into_any_element()
        });
        let comments = pr.unresolved().take(COMMENTS_SHOWN).enumerate().map(|(i, t)| {
            let state = t.state(self.pr.track.thread_sent(tree, pr, &t.id));
            let first = t.comments.first();
            let (id, open) = (t.id.clone(), t.id.clone());
            let send = (can_send && state == ThreadState::Open).then(|| {
                small(("pr-comment-send", i), Variant::Secondary, "Send").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.send_fix(&[], std::slice::from_ref(&id), cx);
                }))
            });
            let at = div().flex().items_center().gap(px(6.)).font_family(MONO).text_size(px(11.5)).text_color(TEXT_3).child(div().min_w_0().truncate().child(spot(t))).children(thread_chip(state));
            div()
                .id(("pr-comment", i))
                .py(px(8.))
                .flex()
                .items_center()
                .gap(px(8.))
                .cursor_pointer()
                .child(ui::avatar(&who(first.map_or("", |c| c.author.as_str())), 20.).text_size(px(9.)))
                .child(div().flex_1().min_w_0().flex().flex_col().gap(px(2.)).child(at).child(div().truncate().text_size(px(12.5)).text_color(TEXT_2).child(opening(t))))
                .children(send)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.view_thread(&open, cx)))
                .into_any_element()
        });
        let all = (!pr.threads.is_empty()).then(|| {
            group_row()
                .id("pr-all-comments")
                .cursor_pointer()
                .child(glyph_slot(icon("comment", 13., TEXT_3)))
                .child(div().flex_1().min_w_0().truncate().text_color(TEXT).child("All comments"))
                .when(unresolved > 0, |d| d.child(div().flex_none().font_family(MONO).text_size(px(11.5)).text_color(TEXT_4).child(format!("{unresolved} unresolved"))))
                .child(icon("chevron-right", 12., TEXT_4))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.pr.track.tab = PrTab::Conversation;
                    cx.notify();
                }))
                .into_any_element()
        });
        lines.chain(comments).chain(all).collect()
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
}

#[cfg(test)]
mod tests {
    use super::checks_status;
    use git::github::{Check, Outcome, Pr};
    use theme::{FAILED_TEXT, SUCCESS_TEXT, WAITING_TEXT};

    fn pr(runs: &[Outcome]) -> Pr {
        Pr { runs: runs.iter().enumerate().map(|(i, o)| Check { name: format!("c{i}"), outcome: *o, url: String::new() }).collect(), ..Default::default() }
    }

    #[test]
    fn the_checks_title_counts_failures_first_then_what_still_runs_then_says_all_passed() {
        use Outcome::*;
        assert_eq!(checks_status(&pr(&[Failed, Passed, Passed, Pending])), Some((FAILED_TEXT, "1 failed · 2 passed".into())));
        assert_eq!(checks_status(&pr(&[Failed])), Some((FAILED_TEXT, "1 failed".into())));
        assert_eq!(checks_status(&pr(&[Passed, Pending, Pending])), Some((WAITING_TEXT, "2 checks in progress".into())));
        assert_eq!(checks_status(&pr(&[Passed, Skipped])), Some((SUCCESS_TEXT, "All passed".into())));
        assert_eq!(checks_status(&pr(&[])), None);
    }
}
