//! The PR tab of a Factory ticket's page, and the merge at the PR review gate.

use super::checks::list;
use crate::board::parts::{bar_button, start_kbd};
use crate::desktop::Desktop;
use crate::git_ui::changes::commit_box::split_button;
use agents::factory::{Finding, Severity};
use agents::tickets::{Gate as Waits, Station, Ticket};
use git::github::{Gate, Method, Pr, PrState, Review};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Instant;
use theme::*;
use ui::Variant;

/// The findings, the worst first. Findings of one severity keep the reviewer's order.
pub(crate) fn sorted(findings: &[Finding]) -> Vec<&Finding> {
    let mut out: Vec<&Finding> = findings.iter().collect();
    out.sort_by_key(|f| f.severity);
    out
}

fn severity_label(s: Severity) -> &'static str {
    match s {
        Severity::Critical => "Critical",
        Severity::Minor => "Minor",
        Severity::Nit => "Nit",
    }
}

/// Where a finding points: `src/a.rs:12`, the file alone without a line, nothing without a file.
pub(crate) fn place(f: &Finding) -> Option<String> {
    match (f.file.is_empty(), f.line) {
        (true, _) => None,
        (false, 0) => Some(f.file.clone()),
        (false, line) => Some(format!("{}:{line}", f.file)),
    }
}

/// The findings counted for one line: `0 critical · 2 minor`.
pub(crate) fn summary(findings: &[Finding]) -> String {
    if findings.is_empty() {
        return "No findings".into();
    }
    let count = |s: Severity| findings.iter().filter(|f| f.severity == s).count();
    let mut parts = vec![format!("{} critical", count(Severity::Critical))];
    for (s, name) in [(Severity::Minor, "minor"), (Severity::Nit, "nit")] {
        if count(s) > 0 {
            parts.push(format!("{} {name}", count(s)));
        }
    }
    parts.join(" · ")
}

/// Why the merge button is off, in a few words. None when the PR can merge.
pub(crate) fn blocked(gate: Gate) -> Option<&'static str> {
    match gate {
        Gate::Ready => None,
        Gate::Done => Some("Merged or closed on GitHub"),
        Gate::Draft => Some("Draft"),
        Gate::Conflicts => Some("Conflicts"),
        Gate::Checking => Some("Checking for conflicts"),
        Gate::ChecksFailed => Some("Checks failed"),
        Gate::ChecksRunning => Some("Checks in progress"),
        Gate::ChangesRequested => Some("Changes requested"),
        Gate::ReviewRequired => Some("Needs an approving review"),
        Gate::Blocked => Some("A branch rule holds the merge"),
    }
}

fn files_text(n: usize) -> String {
    if n == 1 { "1 file".into() } else { format!("{n} files") }
}

/// The PR's number and size for the bar: `#203 · +96 -4 · 7 files`.
pub(crate) fn size(pr: &Pr) -> String {
    format!("#{} · +{} -{} · {}", pr.number, pr.additions, pr.deletions, files_text(pr.files))
}

/// What the card of a merged Ticket says: `Merged · #201 · +96 -4`, without the size while the PR is not read.
pub(crate) fn merged_line(number: u32, pr: Option<&Pr>) -> String {
    match pr {
        Some(pr) => format!("Merged · #{number} · +{} -{}", pr.additions, pr.deletions),
        None => format!("Merged · #{number}"),
    }
}

/// The merge method as `gh.pr.merge` takes it.
pub(crate) fn wire(method: Method) -> &'static str {
    match method {
        Method::Squash => "squash",
        Method::Merge => "merge",
        Method::Rebase => "rebase",
    }
}

/// What the tab says while it has nothing to list.
pub(crate) fn quiet(t: &Ticket) -> Option<String> {
    match (t.pr, t.station, t.gate) {
        (0, ..) => Some("Opens after Checks".into()),
        (n, Station::Merged, _) => Some(format!("#{n} · merged")),
        (n, Station::Pr, gate) if gate != Waits::Pr => Some(format!("#{n} · the reviewer is reading the diff")),
        _ => None,
    }
}

/// The rows under Merge gate: what stands between the PR and its merge, and what you have said so far.
pub(crate) fn gate_rows(pr: &Pr, findings: &[Finding]) -> Vec<(String, String)> {
    let c = pr.checks;
    let total = c.passed + c.failed + c.pending;
    let checks = match () {
        _ if c.failed > 0 => "Checks failed",
        _ if c.pending > 0 => "Checks in progress",
        _ if total == 0 => "No checks",
        _ => "Checks passed",
    };
    let conflicts = match pr.gate() {
        Gate::Conflicts => (format!("Conflicts with {}", pr.base), "Conflicts"),
        Gate::Checking => ("Checking for conflicts".to_string(), "…"),
        _ => (format!("No conflicts with {}", pr.base), "Clean"),
    };
    let yours = match pr.review {
        Review::Approved => "Approved",
        Review::ChangesRequested => "Changes requested",
        _ => "Not yet",
    };
    vec![
        (checks.into(), format!("{}/{total}", c.passed)),
        (conflicts.0, conflicts.1.into()),
        ("Reviewer".into(), summary(findings)),
        ("Your review".into(), yours.into()),
    ]
}

/// The PR to merge when the bar's merge button may fire: the Ticket waits at the PR gate, its PR can merge, and
/// no merge is under way. The findings are not asked: a critical one does not hold the merge.
pub(crate) fn can_merge(t: &Ticket, pr: Option<&Pr>, merging: bool) -> Option<u32> {
    let pr = pr?;
    (t.gate == Waits::Pr && pr.gate() == Gate::Ready && !merging).then_some(pr.number)
}

/// Whether **Request changes** asks the lead for a new team plan. At the PR gate it is a plain prompt.
pub(crate) fn replans(gate: Waits) -> bool {
    gate == Waits::Plan
}

fn finding_row(f: &Finding) -> Div {
    let tone = if f.severity == Severity::Critical { FAILED_TEXT } else { TEXT_3 };
    let text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .child(div().text_size(px(13.)).text_color(TEXT).child(f.message.clone()))
        .children(place(f).map(|at| div().font_family(MONO).text_size(px(11.5)).text_color(TEXT_3).truncate().child(at)));
    div().px(px(12.)).py(px(9.)).flex().items_start().gap(px(8.)).child(div().w(px(52.)).flex_none().text_size(px(12.)).text_color(tone).child(severity_label(f.severity))).child(text)
}

fn gate_row((label, value): (String, String)) -> Div {
    div().px(px(12.)).py(px(9.)).flex().items_center().text_size(px(13.)).text_color(TEXT).child(div().child(label)).child(div().ml_auto().text_color(TEXT_3).child(value))
}

fn still(text: String) -> Div {
    div().mt(px(18.)).text_size(px(13.5)).line_height(px(21.)).text_color(TEXT_3).child(text)
}

impl Desktop {
    /// The PR tab of a Factory ticket's page: the reviewer's findings, then what holds the merge back.
    pub(super) fn factory_pr_tab(&self, t: &Ticket) -> Div {
        if let Some(text) = quiet(t) {
            return still(text);
        }
        let Some(pr) = self.prs.get(&t.worktree) else { return still(format!("#{} · reading the pull request", t.pr)) };
        let findings = self.agents.detail(&t.id).map(|d| d.findings.as_slice()).unwrap_or_default();
        let branch = self.agents.get(&t.agent_id).and_then(|a| self.worktree_of(&a.cwd)).map(|w| w.branch.clone()).unwrap_or_default();
        let mono = |text: String| div().font_family(MONO).text_size(px(11.5)).child(text);
        let head = div().mt(px(18.)).flex().items_baseline().gap(px(8.)).child(div().font_family(MONO).text_size(px(12.5)).text_color(TEXT_3).child(format!("#{}", pr.number))).child(div().min_w_0().text_size(px(14.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(pr.title.clone()));
        let mut meta = div().mt(px(2.)).flex().flex_wrap().items_center().gap(px(5.)).text_size(px(12.)).text_color(TEXT_3);
        if !branch.is_empty() {
            meta = meta.child(mono(branch)).child("into");
        }
        meta = meta.child(mono(pr.base.clone())).child(format!("· {} ·", files_text(pr.files))).child(ui::meta_diff(pr.additions, pr.deletions, 12.));
        let reviewer = match self.agents.factory_config(&t.folder).and_then(|c| self.agents.profiles().iter().find(|p| p.id == c.review)) {
            Some(p) => format!("Reviewer · {}", self.agent_name(&p.provider)),
            None => "Reviewer".into(),
        };
        let found = if findings.is_empty() {
            list(std::iter::once(div().px(px(12.)).py(px(9.)).text_size(px(13.)).text_color(TEXT_3).child("No findings")))
        } else {
            list(sorted(findings).into_iter().map(finding_row))
        };
        div()
            .flex()
            .flex_col()
            .child(head)
            .child(meta)
            .child(heading_for(reviewer))
            .child(found)
            .child(heading_for("Merge gate".into()))
            .child(list(gate_rows(pr, findings).into_iter().map(gate_row)))
    }
}

impl Desktop {
    /// The footer bar at the PR review gate: the PR, what holds its merge back, Request changes and the merge.
    pub(super) fn pr_bar(&self, t: &Ticket, gate: Gate, number: u32, page: &super::page::Page, cx: &mut Context<Self>) -> Div {
        if page.changes {
            return self.changes_bar(cx);
        }
        let pr = self.prs.get(&t.worktree);
        let detail = pr.map_or_else(|| format!("#{number}"), size);
        let why = blocked(gate);
        let what = div()
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(7.))
            .child(div().flex_none().size(px(6.)).rounded(px(3.)).bg(WAITING_DOT))
            .child(div().flex_none().font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("PR review"))
            .child(div().min_w_0().truncate().text_color(TEXT_3).child(detail))
            .children(why.map(|w| div().flex_none().text_color(TEXT_3).child(format!("· {w}"))));
        let method = self.pr.track.method;
        let busy = self.factory.merging;
        let ready = why.is_none() && !busy && !self.agents.observe_only();
        let lead = if busy { spinner("factory-merging", 13., ON_PRIMARY).into_any_element() } else { icon("merge", 14., ON_PRIMARY).into_any_element() };
        let label = div().flex().items_center().gap(px(8.)).child(if busy { "Merging…" } else { method.label() }).when(ready, |d| d.child(start_kbd()));
        let open = self.factory.merge_menu;
        let menu = div()
            .id("factory-merge-menu")
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.factory.merge_menu = false;
                cx.notify();
            }))
            .children(Method::ALL.into_iter().enumerate().map(|(i, m)| {
                ui::menu_pick(("factory-merge-method", i), div(), m.label(), m == method).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.pr.track.method = m;
                    this.factory.merge_menu = false;
                    cx.notify();
                }))
            }));
        let ticket = t.id.clone();
        let merge = split_button(
            "factory-merge",
            lead,
            label,
            ready,
            false,
            Box::new(move |this, _, cx| {
                if let Some(t) = this.agents.board.get(&ticket).cloned() {
                    this.factory_merge(&t, cx);
                }
            }),
            Box::new(|this, _, _| this.factory.merge_menu = !this.factory.merge_menu),
            div().id("factory-merge-closed"),
            cx,
        );
        // The bar sits at the window's foot, so the menu opens upward: three 32px rows, 6px padding and a 4px gap.
        let merge = merge.when(open, |d| d.child(ui::dropdown_right(-(3. * 32. + 12. + 4.), ui::menu_in("factory-merge-pop", ui::pop(menu).w(px(232.)).p(px(6.)).flex().flex_col().occlude()))));
        div()
            .flex_1()
            .flex()
            .items_center()
            .gap(px(10.))
            .child(what)
            .child(bar_button("factory-changes", Variant::Secondary, None, "Request changes").h(px(30.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.factory_changes(true, window, cx))))
            .child(merge)
    }

    /// Asks pocketd to merge the Ticket's PR with the chosen method, when the bar's merge may fire.
    pub(crate) fn factory_merge(&mut self, t: &Ticket, cx: &mut Context<Self>) -> bool {
        let Some(number) = can_merge(t, self.prs.get(&t.worktree), self.factory.merging).filter(|_| !self.agents.observe_only()) else { return false };
        self.outbox.local.gh_merge(&t.worktree, number, wire(self.pr.track.method));
        self.factory.merging = true;
        self.factory.merge_menu = false;
        cx.notify();
        true
    }

    /// pocketd merged the PR of `tree`: show it merged now rather than at the next read.
    pub(crate) fn factory_merged(&mut self, tree: &str, cx: &mut Context<Self>) {
        self.factory.merging = false;
        self.prs.edit(tree, |pr| pr.state = PrState::Merged);
        self.prs.expire(tree, Instant::now());
        cx.notify();
    }
}

fn heading_for(title: String) -> Div {
    div().mt(px(20.)).mb(px(8.)).text_size(px(12.)).line_height(px(16.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child(title)
}

#[cfg(test)]
mod tests {
    // A glob would bring in gpui's `test` attribute, which shadows the std one.
    use super::{blocked, can_merge, gate_rows, merged_line, place, quiet, replans, size, sorted, summary, wire};
    use agents::factory::{Finding, Severity};
    use agents::tickets::{Gate as Waits, Station, Ticket};
    use git::github::{Gate, Method, Mergeable, Pr, PrState};

    fn finding(severity: Severity, file: &str, line: u32, message: &str) -> Finding {
        Finding { severity, file: file.into(), line, message: message.into() }
    }

    fn at_gate() -> (Ticket, Pr) {
        let t = Ticket { pr: 203, station: Station::Pr, gate: Waits::Pr, ..Ticket::sample() };
        (t, Pr { number: 203, mergeable: Mergeable::Clean, ..Pr::default() })
    }

    #[test]
    fn findings_list_critical_first_and_keep_the_reviewers_order_within_a_severity() {
        let all = [finding(Severity::Nit, "", 0, "n1"), finding(Severity::Minor, "", 0, "m1"), finding(Severity::Critical, "", 0, "c1"), finding(Severity::Minor, "", 0, "m2")];
        let order: Vec<&str> = sorted(&all).iter().map(|f| f.message.as_str()).collect();
        assert_eq!(order, ["c1", "m1", "m2", "n1"]);
    }

    #[test]
    fn a_finding_points_at_its_file_and_line_when_it_has_them() {
        assert_eq!(place(&finding(Severity::Minor, "src/a.rs", 12, "x")).as_deref(), Some("src/a.rs:12"));
        assert_eq!(place(&finding(Severity::Minor, "src/a.rs", 0, "x")).as_deref(), Some("src/a.rs"));
        assert_eq!(place(&finding(Severity::Minor, "", 12, "x")), None);
    }

    #[test]
    fn the_reviewer_line_always_counts_critical_and_the_rest_only_when_there_are_some() {
        assert_eq!(summary(&[finding(Severity::Minor, "", 0, "a"), finding(Severity::Minor, "", 0, "b")]), "0 critical · 2 minor");
        assert_eq!(summary(&[finding(Severity::Critical, "", 0, "a"), finding(Severity::Nit, "", 0, "b")]), "1 critical · 1 nit");
        assert_eq!(summary(&[]), "No findings");
    }

    #[test]
    fn the_merge_button_says_why_it_is_off_and_nothing_when_the_pr_can_merge() {
        assert_eq!(blocked(Gate::Ready), None);
        assert_eq!(blocked(Gate::ChecksRunning), Some("Checks in progress"));
        assert_eq!(blocked(Gate::Conflicts), Some("Conflicts"));
        assert_eq!(blocked(Gate::Draft), Some("Draft"));
    }

    #[test]
    fn a_pr_says_its_size_for_the_bar_and_for_a_merged_card() {
        let pr = Pr { number: 203, additions: 96, deletions: 4, files: 7, ..Pr::default() };
        assert_eq!(size(&pr), "#203 · +96 -4 · 7 files");
        assert_eq!(size(&Pr { files: 1, ..pr.clone() }), "#203 · +96 -4 · 1 file");
        assert_eq!(merged_line(201, Some(&pr)), "Merged · #201 · +96 -4");
        assert_eq!(merged_line(201, None), "Merged · #201");
    }

    #[test]
    fn each_merge_method_goes_out_as_pocketds_word() {
        assert_eq!(Method::ALL.map(wire), ["squash", "merge", "rebase"]);
    }

    #[test]
    fn the_tab_is_quiet_until_the_gate_opens_and_after_the_merge() {
        let at = |pr, station, gate| Ticket { pr, station, gate, ..Ticket::sample() };
        assert_eq!(quiet(&at(0, Station::Checks, Waits::None)).as_deref(), Some("Opens after Checks"));
        assert_eq!(quiet(&at(203, Station::Pr, Waits::None)).as_deref(), Some("#203 · the reviewer is reading the diff"));
        assert_eq!(quiet(&at(203, Station::Pr, Waits::Pr)), None);
        assert_eq!(quiet(&at(203, Station::Checks, Waits::None)), None);
        assert_eq!(quiet(&at(201, Station::Merged, Waits::None)).as_deref(), Some("#201 · merged"));
    }

    #[test]
    fn the_gate_rows_say_how_checks_conflicts_the_reviewer_and_you_stand() {
        let pr = Pr { number: 203, base: "main".into(), mergeable: Mergeable::Clean, checks: git::github::Checks { passed: 4, ..Default::default() }, ..Pr::default() };
        let found = [finding(Severity::Minor, "", 0, "a")];
        let rows = gate_rows(&pr, &found);
        assert_eq!(rows, [("Checks passed".to_string(), "4/4".to_string()), ("No conflicts with main".into(), "Clean".into()), ("Reviewer".into(), "0 critical · 1 minor".into()), ("Your review".into(), "Not yet".into())]);
        let failing = Pr { mergeable: Mergeable::Conflicting, checks: git::github::Checks { passed: 3, failed: 1, pending: 0 }, ..pr.clone() };
        let rows = gate_rows(&failing, &[]);
        assert_eq!(rows[0], ("Checks failed".to_string(), "3/4".to_string()));
        assert_eq!(rows[1], ("Conflicts with main".to_string(), "Conflicts".to_string()));
        assert_eq!(rows[2].1, "No findings");
        let waiting = Pr { checks: git::github::Checks { passed: 1, failed: 0, pending: 2 }, ..pr };
        assert_eq!(gate_rows(&waiting, &[])[0], ("Checks in progress".to_string(), "1/3".to_string()));
    }

    #[test]
    fn the_merge_fires_only_at_the_pr_gate_for_a_pr_that_can_merge_and_only_once() {
        let (t, ready) = at_gate();
        assert_eq!(can_merge(&t, Some(&ready), false), Some(203));
        assert_eq!(can_merge(&t, Some(&ready), true), None);
        assert_eq!(can_merge(&t, Some(&Pr { draft: true, ..ready.clone() }), false), None);
        assert_eq!(can_merge(&t, Some(&Pr { mergeable: Mergeable::Conflicting, ..ready.clone() }), false), None);
        assert_eq!(can_merge(&t, Some(&Pr { state: PrState::Merged, ..ready.clone() }), false), None);
        assert_eq!(can_merge(&t, None, false), None);
        assert_eq!(can_merge(&Ticket { gate: Waits::None, ..t.clone() }, Some(&ready), false), None);
    }

    #[test]
    fn a_critical_finding_does_not_hold_the_merge() {
        let (t, ready) = at_gate();
        // can_merge takes no findings: the decision is yours.
        assert_eq!(can_merge(&t, Some(&ready), false), Some(203));
    }

    #[test]
    fn request_changes_asks_for_a_new_plan_only_at_the_plan_gate() {
        assert!(replans(Waits::Plan));
        assert!(!replans(Waits::Pr));
    }
}
