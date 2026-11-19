use super::{gate_text, resolving_text, review_lines, tally, to_fix};
use crate::desktop::Desktop;
use git::github::{Check, Gate, Outcome, Pr, PrState, Review, ThreadState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::Variant;

pub(crate) fn small(id: impl Into<ElementId>, v: Variant, label: impl Into<SharedString>) -> Stateful<Div> {
    sized(ui::button(id, v, None, label.into()))
}

pub(super) fn small_with(id: impl Into<ElementId>, v: Variant, glyph: &str, label: impl Into<SharedString>) -> Stateful<Div> {
    sized(ui::button(id, v, Some(glyph), label.into()))
}

fn sized(b: Stateful<Div>) -> Stateful<Div> {
    b.h(px(24.)).px(px(9.)).gap(px(5.)).rounded(px(7.)).text_size(px(12.))
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

/// `login`'s initial on a colour of its own, the same for them everywhere in the panel.
pub(super) fn face(login: &str, size: f32) -> Div {
    let hue = login.bytes().fold(0u32, |h, b| h.wrapping_mul(31).wrapping_add(b.into())) % 360;
    div()
        .size(px(size))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(hsla(hue as f32 / 360., 0.45, 0.42, 1.))
        .text_size(px(size / 2.))
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(gpui_kit::white())
        .child(initial(login))
}

fn initial(login: &str) -> String {
    login.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().collect()).unwrap_or_default()
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

/// Every check, what failed first; passed and skipped ones last.
fn by_outcome(pr: &Pr) -> Vec<&Check> {
    let rank = |o: Outcome| [Outcome::Failed, Outcome::Pending, Outcome::Passed, Outcome::Skipped].iter().position(|x| *x == o);
    let mut runs: Vec<&Check> = pr.runs.iter().collect();
    runs.sort_by_key(|c| rank(c.outcome));
    runs
}

fn count(pr: &Pr, o: Outcome) -> usize {
    pr.runs.iter().filter(|c| c.outcome == o).count()
}

pub(super) fn card() -> Div {
    div().rounded(px(12.)).bg(SURFACE).shadow(ui::row_shadow()).overflow_hidden()
}

/// A row of a list card, under a hairline unless it is the first.
pub(super) fn list_row(i: usize) -> Div {
    div().w_full().h(px(34.)).flex_none().pl(px(10.)).pr(px(8.)).flex().items_center().gap(px(8.)).text_size(px(12.5)).text_color(TEXT_2).when(i > 0, |d| d.border_t(px(0.5)).border_color(HAIRLINE))
}

pub(super) fn list_head(left: impl IntoElement, right: impl IntoElement) -> Div {
    div().flex_none().px(px(14.)).pt(px(4.)).pb(px(8.)).flex().items_center().justify_between().gap(px(8.)).text_size(px(12.)).text_color(TEXT_3).child(left).child(right)
}

impl Desktop {
    /// Every check, then who reviewed and what holds the merge back.
    pub(crate) fn pr_checks_tab(&self, tree: &str, pr: &Pr, cx: &mut Context<Self>) -> Stateful<Div> {
        let fixing = self.pr.track.fixing(tree, pr);
        let can_send = !self.agents.observe_only();
        let (unsent, _) = to_fix(pr, fixing.as_ref());
        let resolving = fixing.as_ref().is_some_and(|s| s.conflict);
        let (failed, pending) = (count(pr, Outcome::Failed), count(pr, Outcome::Pending));
        let rest = tally([(count(pr, Outcome::Passed), "passed"), (count(pr, Outcome::Skipped), "skipped")]);
        let sum = div()
            .flex()
            .gap(px(4.))
            .when(failed > 0, |d| d.child(div().font_weight(FontWeight::MEDIUM).text_color(FAILED_TEXT).child(format!("{failed} failed"))))
            .when(pending > 0, |d| d.child(div().font_weight(FontWeight::MEDIUM).text_color(WAITING_TEXT).child(format!("{pending} in progress"))))
            .when(!rest.is_empty() && failed + pending > 0, |d| d.child("·"))
            .child(rest);
        let rows = by_outcome(pr).into_iter().enumerate().map(|(i, c)| self.check_row(i, c, !unsent.iter().any(|u| u.name == c.name), can_send, cx));
        let list = if pr.runs.is_empty() { card().child(list_row(0).text_color(TEXT_3).child(glyph_slot(ui::dot(6., TEXT_4))).child("No checks on this branch")) } else { card().children(rows) };
        let reviews = review_lines(pr).into_iter().map(|(r, text)| row().min_h(px(22.)).child(glyph_slot(review_icon(r))).child(div().flex_1().min_w_0().truncate().child(text)));
        let gate = (pr.state == PrState::Open).then(|| self.pr_gate(pr, resolving, self.fix_agent()).min_h(px(22.)).pt(px(6.)).border_t(px(0.5)).border_color(HAIRLINE));
        div()
            .id("pr-checks")
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .pt(px(4.))
            .pb(px(16.))
            .flex()
            .flex_col()
            .child(list_head(sum, div()))
            .child(div().mx(px(10.)).child(list))
            .child(card().mt(px(12.)).mx(px(10.)).px(px(12.)).py(px(10.)).flex().flex_col().gap(px(6.)).children(reviews).children(gate))
    }

    /// `sent`: the agent is fixing it.
    fn check_row(&self, i: usize, check: &Check, sent: bool, can_send: bool, cx: &mut Context<Self>) -> Div {
        let name = |color: Token| div().flex_1().min_w_0().truncate().text_color(color).child(check.name.clone());
        let meta = |text: &'static str| div().flex_none().font_family(MONO).text_size(px(11.5)).text_color(TEXT_4).child(text);
        match check.outcome {
            Outcome::Failed => {
                let (log, send) = (check.url.clone(), check.name.clone());
                let acts = div()
                    .flex()
                    .flex_none()
                    .gap(px(4.))
                    .when(!log.is_empty(), |d| d.child(small(("pr-check-log", i), Variant::Ghost, "Log").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_pr(log.clone(), window, cx)))))
                    .when(can_send, |d| d.child(small(("pr-check-send", i), Variant::Secondary, "Send").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_fix(std::slice::from_ref(&send), &[], cx)))));
                list_row(i).child(glyph_slot(icon("x", 13., FAILED))).child(name(FAILED_TEXT).font_weight(FontWeight::MEDIUM)).child(if sent { fixing_mark() } else { acts })
            }
            Outcome::Pending => list_row(i).child(glyph_slot(spinner(("pr-run", i), 11., WAITING))).child(name(TEXT_2)).child(meta("in progress")),
            Outcome::Skipped => list_row(i).text_color(TEXT_4).child(glyph_slot(icon("chevron-right", 12., TEXT_4))).child(name(TEXT_4)).child(meta("skipped")),
            Outcome::Passed => list_row(i).child(glyph_slot(icon("check", 13., SUCCESS))).child(name(TEXT_2)),
        }
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
    use super::{by_outcome, initial};
    use git::github::{Check, Outcome, Pr};

    #[test]
    fn checks_list_what_failed_then_what_runs_then_what_passed_then_what_was_skipped() {
        use Outcome::*;
        let runs = [Passed, Skipped, Pending, Failed, Passed];
        let pr = Pr { runs: runs.iter().enumerate().map(|(i, o)| Check { name: format!("c{i}"), outcome: *o, url: String::new() }).collect(), ..Default::default() };
        let names: Vec<&str> = by_outcome(&pr).iter().map(|c| c.name.as_str()).collect();
        assert_eq!(names, ["c3", "c2", "c0", "c4", "c1"]);
    }

    #[test]
    fn a_face_shows_the_first_letter_of_the_login() {
        assert_eq!(initial("-mingo"), "M");
        assert_eq!(initial(""), "");
    }
}
