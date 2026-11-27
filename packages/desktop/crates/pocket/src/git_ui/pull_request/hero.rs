use super::PrSide;
use super::card::small;
use super::{Branch, PrTab, card_busy, fix_summary, fixing_text, resolving_text, to_fix};
use crate::git_ui::changes::commit_box::{primary_pill, split_button};
use crate::git_ui::pull_requests::MERGED_BG;
use crate::util::{ago, now_ms};
use git::github::{Gate, Label, Method, Pr, PrState, Review, base_branch};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::*;
use theme::*;
use ui::Variant;

/// The first of the reasons a verdict gives; the Checks tab has the rest.
fn lead(said: &str) -> &str {
    said.split(" · ").next().unwrap_or(said)
}

fn chip_bg(fg: Token) -> Token {
    [(SUCCESS_TEXT, SUCCESS_BG), (FAILED_TEXT, FAILED_BG), (WAITING_TEXT, WAITING_BG), (MERGED, MERGED_BG)]
        .into_iter()
        .find(|(t, _)| *t == fg)
        .map_or(FILL_3, |(_, bg)| bg)
}

fn chip_glyph(fg: Token, pr: &Pr) -> AnyElement {
    match fg {
        _ if fg == WAITING_TEXT => spinner("pr-chip-spin", 12., WAITING_TEXT).into_any_element(),
        _ if fg == SUCCESS_TEXT => icon("check", 13., SUCCESS_TEXT).into_any_element(),
        _ if fg == FAILED_TEXT => icon("x", 13., FAILED_TEXT).into_any_element(),
        _ if fg == MERGED => icon("merge", 13., MERGED).into_any_element(),
        _ if pr.draft => icon("pull-request-draft", 13., fg).into_any_element(),
        _ => icon("pull-request", 13., fg).into_any_element(),
    }
}

fn label_pill(l: &Label) -> Div {
    let c = |a: u32| Token::fixed((l.color << 8) | a);
    div()
        .h(px(20.))
        .px(px(8.))
        .flex()
        .flex_none()
        .items_center()
        .rounded(px(10.))
        .bg(c(0x24))
        .border_1()
        .border_color(c(0x66))
        .text_size(px(11.5))
        .font_weight(FontWeight::MEDIUM)
        .text_color(c(0xff))
        .child(l.name.clone())
}

fn reviewer(face: Div, verdict: Review) -> Div {
    let (bg, mark) = match verdict {
        Review::Approved => (SUCCESS, "check"),
        Review::ChangesRequested => (FAILED, "x"),
        _ => (TEXT_4, "clock"),
    };
    let badge = div()
        .absolute()
        .right(px(-3.))
        .bottom(px(-3.))
        .size(px(11.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(bg)
        .shadow(vec![ui::ring(SIDE, 1.5)])
        .child(icon(mark, 7., ON_PRIMARY));
    div().relative().child(face).child(badge)
}

impl PrSide {
    /// The PR at a glance: its title, where it stands and the next step, its branches, people and labels, and what the agent is doing for it.
    pub(crate) fn pr_hero(&self, tree: &str, pr: &Pr, repo: &Branch, cx: &mut Context<Self>) -> Div {
        let (fg, said) = self.verdict(tree, pr, self.frame.agent);
        let num = format!("#{}", pr.number);
        let title = StyledText::new(format!("{} {num}", pr.title)).with_highlights([(
            pr.title.len() + 1..pr.title.len() + 1 + num.len(),
            HighlightStyle { color: Some(TEXT_3.into()), font_weight: Some(FontWeight::MEDIUM), ..Default::default() },
        )]);
        let chip = div()
            .id("pr-chip")
            .h(px(28.))
            .min_w_0()
            .pl(px(9.))
            .pr(px(8.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(8.))
            .bg(chip_bg(fg))
            .cursor_pointer()
            .text_size(px(12.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(fg)
            .child(chip_glyph(fg, pr))
            .child(div().min_w_0().truncate().child(lead(&said).to_string()))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.track.tab = PrTab::Checks;
                cx.notify();
            }));
        let state = div().flex().items_center().gap(px(8.)).child(chip).child(div().flex_1()).children(self.pr_action(tree, pr, cx));
        let branch =
            |name: String| div().min_w_0().flex().items_center().gap(px(4.)).child(icon("branch", 12., TEXT_4)).child(div().min_w_0().truncate().child(name));
        let branches = div()
            .flex()
            .items_center()
            .gap(px(6.))
            .font_family(MONO)
            .text_size(px(12.))
            .text_color(TEXT_2)
            .whitespace_nowrap()
            .child(branch(base_branch(&pr.base).to_string()))
            .child(div().flex_none().text_color(TEXT_4).child("←"))
            .child(branch(repo.name.clone()))
            .child(div().flex_none().text_color(SUCCESS_TEXT).child(format!("+{}", pr.additions)))
            .child(div().flex_none().text_color(FAILED).child(format!("−{}", pr.deletions)));
        let mut seen: Vec<(String, Review)> = pr.reviews.clone();
        seen.extend(pr.requested.iter().filter(|r| !pr.reviews.iter().any(|(w, _)| w == *r)).map(|r| (r.clone(), Review::Required)));
        let opened = (!pr.author.is_empty()).then(|| {
            div().min_w_0().flex().items_center().gap(px(6.)).child(self.face(&pr.author, 20.)).child(
                div()
                    .min_w_0()
                    .truncate()
                    .flex()
                    .gap(px(4.))
                    .child(div().font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(pr.author.clone()))
                    .child(format!("opened {}", ago(pr.opened as i64, now_ms()))),
            )
        });
        let reviewers = (!seen.is_empty()).then(|| {
            div()
                .flex()
                .flex_none()
                .items_center()
                .gap(px(6.))
                .child(div().w(px(1.)).h(px(14.)).bg(SEPARATOR_STRONG))
                .children(seen.iter().map(|(w, r)| reviewer(self.face(w, 20.), *r)))
        });
        let people = div().min_h(px(22.)).flex().items_center().gap(px(8.)).text_size(px(12.5)).text_color(TEXT_3).children(opened).children(reviewers);
        let labels =
            (!pr.labels.is_empty()).then(|| div().min_h(px(22.)).flex().flex_wrap().items_center().gap(px(6.)).children(pr.labels.iter().map(label_pill)));
        div()
            .flex_none()
            .px(px(16.))
            .pt(px(2.))
            .pb(px(12.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().text_size(px(16.)).line_height(px(21.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).line_clamp(3).child(title))
            .child(state)
            .child(branches)
            .child(people)
            .children(labels)
            .children(self.pr_banner(tree, pr, cx))
    }

    /// The one next step for the PR: hand what failed to the agent, resolve a conflict, mark a draft ready, or merge.
    fn pr_action(&self, tree: &str, pr: &Pr, cx: &mut Context<Self>) -> Option<AnyElement> {
        if pr.state != PrState::Open {
            return None;
        }
        let agent = self.frame.agent;
        let fixing = self.track.fixing(tree, pr);
        let can_send = !self.frame.observe_only && fixing.is_none();
        let busy = card_busy(self.busy.as_ref(), self.composing.as_deref(), tree);
        let (checks, threads) = to_fix(pr, fixing.as_ref());
        let gate = pr.gate();
        let button = |id: &'static str, label: String| ui::button(id, Variant::Primary, None, label).h(px(28.)).px(px(11.)).text_size(px(12.5));
        if can_send && (!checks.is_empty() || !threads.is_empty()) {
            let names: Vec<String> = checks.iter().map(|c| c.name.clone()).collect();
            let ids: Vec<String> = threads.iter().map(|t| t.id.clone()).collect();
            let (title, sub) = fix_summary(checks.len(), threads.len());
            let tip = format!("{title}: {sub}");
            return Some(
                button("pr-fix-all", format!("Fix with {agent}"))
                    .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.send_fix(&names, &ids, cx)))
                    .into_any_element(),
            );
        }
        let resolving = fixing.as_ref().is_some_and(|s| s.conflict);
        match gate {
            Gate::Conflicts => {
                let b = button("pr-resolve", format!("Resolve with {agent}"));
                Some(
                    if resolving || self.frame.observe_only {
                        b.opacity(0.5).cursor_default()
                    } else {
                        b.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.send_conflict(cx)))
                    }
                    .into_any_element(),
                )
            }
            Gate::Draft => {
                let b = button("pr-ready", busy.unwrap_or("Ready for review").to_string());
                Some(
                    if busy.is_some() { b.opacity(0.5).cursor_default() } else { b.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.mark_ready(cx))) }
                        .into_any_element(),
                )
            }
            _ => Some(div().w(px(116.)).child(self.pr_merge(gate == Gate::Ready && busy.is_none(), busy, cx)).into_any_element()),
        }
    }

    /// Merge, with the method and what happens to the worktree after in its menu.
    fn pr_merge(&self, ready: bool, busy: Option<&'static str>, cx: &mut Context<Self>) -> Div {
        let method = self.track.method;
        if let Some(label) = busy {
            return primary_pill().items_center().justify_center().gap(px(6.)).opacity(0.5).child(spinner("pr-merging", 13., ON_PRIMARY)).child(label);
        }
        let lead = icon("merge", 14., ON_PRIMARY);
        let methods = Method::ALL.into_iter().enumerate().map(|(i, m)| {
            ui::menu_row(("merge-method", i), "merge", m.label(), None).children((m == method).then(|| icon("check", 13., TEXT_2))).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    this.track.method = m;
                    this.merge_menu = false;
                    cx.notify();
                },
            ))
        });
        let after = self.frame.deletable.then(|| {
            let on = self.track.delete_after;
            div()
                .id("pr-delete-after")
                .mt(px(4.))
                .pt(px(8.))
                .px(px(8.))
                .pb(px(4.))
                .border_t(px(0.5))
                .border_color(SEPARATOR)
                .flex()
                .items_start()
                .gap(px(8.))
                .cursor_pointer()
                .text_size(px(12.5))
                .line_height(px(16.))
                .text_color(TEXT)
                .child(ui::checkbox(on).mt(px(1.)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child("Delete the worktree after merging")
                        .child(div().text_size(px(11.5)).text_color(TEXT_4).child("Keeps the branch")),
                )
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.track.delete_after = !this.track.delete_after;
                    cx.notify();
                }))
        });
        let menu = div()
            .id("merge-menu")
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.merge_menu = false;
                cx.notify();
            }))
            .children(methods)
            .children(after);
        split_button::<PrSide>(
            "merge",
            lead,
            "Merge",
            ready,
            self.merge_menu,
            Box::new(|this, _, cx| this.merge_pr(cx)),
            Box::new(|this, _, _| {
                this.merge_menu = true;
                this.ship_menu = false;
            }),
            menu,
            cx,
        )
    }

    /// What the agent is doing for the PR, or, once merged, whether to keep its worktree.
    fn pr_banner(&self, tree: &str, pr: &Pr, cx: &mut Context<Self>) -> Option<Div> {
        let banner = |lead: AnyElement, bg: Token, title: String, sub: String| {
            div().px(px(10.)).py(px(8.)).flex().flex_col().gap(px(8.)).rounded(px(9.)).bg(bg).text_size(px(12.)).line_height(px(16.)).text_color(TEXT_2).child(
                div().flex().items_center().gap(px(9.)).child(lead).child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .flex()
                        .flex_col()
                        .child(div().text_size(px(12.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title))
                        .child(sub),
                ),
            )
        };
        if pr.state == PrState::Merged && self.frame.deletable && !self.track.kept(tree) {
            let branch = self.frame.branch.as_ref().map(|b| b.name.clone()).unwrap_or_default();
            let (keep_tree, delete_tree) = (tree.to_string(), tree.to_string());
            let keep = small("pr-keep", Variant::Secondary, "Keep").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.track.keep(&keep_tree);
                cx.notify();
            }));
            let delete = small("pr-delete-tree", Variant::Primary, "Delete")
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.delete_merged(&delete_tree, cx)));
            return Some(
                banner(
                    icon("merge", 15., MERGED).into_any_element(),
                    FILL_2,
                    format!("Delete the worktree {branch}?"),
                    "Its terminals close; the branch stays.".into(),
                )
                .child(div().flex().justify_end().gap(px(4.)).child(keep).child(delete)),
            );
        }
        let fixing = self.track.fixing(tree, pr)?;
        let agent = self.frame.agent;
        let title = if fixing.items() > 0 { fixing_text(agent, fixing.items()) } else { resolving_text(agent) };
        Some(banner(spinner("pr-banner", 13., WAITING).into_any_element(), WAITING_BG, title, "The checks run again once it pushes".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::lead;

    #[test]
    fn the_chip_says_only_the_first_reason() {
        assert_eq!(lead("1 check failed · changes requested"), "1 check failed");
        assert_eq!(lead("Ready to merge"), "Ready to merge");
    }
}
