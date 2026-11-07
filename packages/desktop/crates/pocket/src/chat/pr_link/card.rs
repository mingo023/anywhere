use super::Peek;
use crate::chat::markdown::Open;
use crate::desktop::Desktop;
use crate::git_ui::pull_request::{check_rows, glyph_slot, opening, plural, review_icon, review_lines, row, small};
use crate::git_ui::pull_requests::glyph;
use git::github::{Outcome, Pr, PrState, Thread};
use gpui_kit::*;
use theme::*;
use ui::Variant;

const WIDTH: f32 = 340.;
const THREADS_SHOWN: usize = 2;

/// The checks in one line: which failed, how many still run, or how many passed. `None` once the PR is closed or merged.
fn checks_line(pr: &Pr) -> Option<(Outcome, String)> {
    if pr.state != PrState::Open {
        return None;
    }
    let rows = check_rows(pr);
    let failed: Vec<&str> = rows.named.iter().filter(|c| c.outcome == Outcome::Failed).map(|c| c.name.as_str()).collect();
    let pending = rows.named.len() - failed.len();
    Some(if !failed.is_empty() {
        (Outcome::Failed, format!("{} failed", failed.join(", ")))
    } else if pending > 0 {
        (Outcome::Pending, format!("{} in progress", plural(pending, "check")))
    } else if rows.passed > 0 {
        (Outcome::Passed, format!("{} passed", plural(rows.passed, "check")))
    } else {
        (Outcome::Skipped, "No checks".into())
    })
}

/// A thread's file and line, without the folders: the card is too narrow for both a path and what was said.
fn spot(t: &Thread) -> String {
    let file = t.path.rsplit('/').next().unwrap_or(&t.path);
    t.line.map_or(file.to_string(), |line| format!("{file}:{line}"))
}

fn line(lead: impl IntoElement, text: String) -> Div {
    row().child(glyph_slot(lead)).child(div().flex_1().min_w_0().truncate().child(text))
}

/// The card a PR chip peeks: where the PR stands; while it's open, its checks, review and threads; and what can be done from here.
pub(super) fn card(p: &Peek, open: Open, url: String, desktop: WeakEntity<Desktop>) -> Div {
    let pr = &p.pr;
    let (name, color) = glyph(pr);
    let checks = checks_line(pr).map(|(outcome, text)| {
        let lead = match outcome {
            Outcome::Failed => icon("x", 13., FAILED).into_any_element(),
            Outcome::Pending => spinner("pr-peek-checks", 12., WAITING).into_any_element(),
            Outcome::Passed => icon("check", 13., SUCCESS).into_any_element(),
            Outcome::Skipped => ui::dot(6., TEXT_4).into_any_element(),
        };
        line(lead, text)
    });
    // As the panel's card: once merged or closed, who reviewed and what's unresolved no longer matter.
    let open_pr = pr.state == PrState::Open;
    let reviews = open_pr.then(|| review_lines(pr).into_iter().map(|(r, text)| line(review_icon(r), text))).into_iter().flatten();
    let unresolved = if open_pr { pr.unresolved().count() } else { 0 };
    let threads = pr.unresolved().take(unresolved.min(THREADS_SHOWN)).map(|t| {
        line(icon("comment", 13., TEXT_3), opening(t)).child(div().flex_none().font_family(MONO).text_size(px(11.5)).text_color(TEXT_4).child(spot(t)))
    });
    let more = (unresolved > THREADS_SHOWN).then(|| line(div(), format!("{} more", plural(unresolved - THREADS_SHOWN, "comment"))).text_color(TEXT_3));
    let head = div()
        .flex()
        .items_start()
        .gap(px(10.))
        .child(div().pt(px(1.)).child(icon(name, 16., color)))
        .child(
            div()
                .flex_1()
                .min_w_0()
                .flex()
                .flex_col()
                .gap(px(2.))
                .child(div().text_size(px(13.5)).line_height(px(18.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).line_clamp(2).child(pr.title.clone()))
                .child(div().text_size(px(12.)).text_color(TEXT_3).truncate().child(format!("#{} · {} → {}", pr.number, pr.head, pr.base)))
                .child(div().text_size(px(12.)).text_color(p.verdict.0).truncate().child(p.verdict.1.clone())),
        );
    let panel = p.here.then(|| {
        let desktop = desktop.clone();
        small("pr-peek-panel", Variant::Secondary, "Open in panel").on_click(move |_, _, cx| {
            desktop.update(cx, |d, cx| d.show_pr_side(cx)).ok();
        })
    });
    let web = ui::icon_button_sized("pr-peek-web", "external", 24., TEXT_3).on_click(move |_, window, cx| open(&url, window, cx));
    let fix = p.fix.clone().map(|fix| {
        small("pr-peek-fix", Variant::Primary, format!("Send to {}", p.agent)).on_click(move |_, _, cx| {
            desktop.update(cx, |d, cx| d.send_fix(&fix.checks, &fix.threads, cx)).ok();
        })
    });
    let foot = div().flex().items_center().gap(px(6.)).pt(px(10.)).border_t_1().border_color(SEPARATOR).children(panel).child(web).child(div().flex_1()).children(fix);
    ui::pop(div())
        .w(px(WIDTH))
        .p(px(12.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(head)
        .child(div().flex().flex_col().children(checks).children(reviews).children(threads).children(more))
        .child(foot)
}

#[cfg(test)]
mod tests {
    use super::checks_line;
    use git::github::{Check, Outcome, Pr, PrState};

    fn pr(runs: &[(&str, Outcome)]) -> Pr {
        Pr { runs: runs.iter().map(|(n, o)| Check { name: n.to_string(), outcome: *o, url: String::new() }).collect(), ..Default::default() }
    }

    #[test]
    fn the_checks_line_names_what_failed_before_what_still_runs() {
        use Outcome::*;
        assert_eq!(checks_line(&pr(&[("build", Passed), ("clippy", Failed), ("test", Pending)])), Some((Failed, "clippy failed".into())));
        assert_eq!(checks_line(&pr(&[("build", Passed), ("test", Pending), ("lint", Pending)])), Some((Pending, "2 checks in progress".into())));
        assert_eq!(checks_line(&pr(&[("build", Passed)])), Some((Passed, "1 check passed".into())));
        assert_eq!(checks_line(&pr(&[])), Some((Skipped, "No checks".into())));
        assert_eq!(checks_line(&Pr { state: PrState::Merged, ..pr(&[("build", Passed)]) }), None);
    }
}
