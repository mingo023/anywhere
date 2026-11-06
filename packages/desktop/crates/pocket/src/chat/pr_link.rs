use super::markdown::Open;
use crate::desktop::Desktop;
use crate::git_ui::pull_request::{PullRequest, card_busy, check_rows, review_icon, review_lines, small, to_fix, verdict};
use crate::git_ui::pull_requests::{PullRequests, glyph};
use git::github::{Outcome, Pr, PrState, Thread};
use gpui_kit::base::HoverCard;
use gpui_kit::base::text::{InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::sync::Arc;
use std::time::Duration;
use theme::*;
use ui::Variant;

const PEEK_WIDTH: f32 = 340.;
const THREADS_SHOWN: usize = 2;

/// `owner/repo#n` of a GitHub pull request URL, whatever follows the number.
fn pr_key(url: &str) -> Option<String> {
    let rest = url.trim_start_matches("https://").trim_start_matches("http://").trim_start_matches("www.");
    let mut parts = rest.split(['/', '#', '?']);
    let (host, owner, repo, pull, n) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?, parts.next()?);
    let numbered = !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit());
    (host.eq_ignore_ascii_case("github.com") && pull == "pull" && numbered && !owner.is_empty() && !repo.is_empty())
        .then(|| format!("{}/{}#{n}", owner.to_lowercase(), repo.to_lowercase()))
}

/// What a chip calls a PR it knows nothing about: the link's own text, else `repo#n`.
fn label(key: &str, text: &str, url: &str) -> String {
    if !text.is_empty() && text != url {
        return text.to_string();
    }
    key.split_once('/').map_or(key, |(_, repo)| repo).to_string()
}

/// Whether `text` links to `key`'s PR, cheaply, before a PR is cloned for it.
fn mentions(text: &str, key: &str) -> bool {
    let Some((repo, n)) = key.split_once('#') else { return false };
    let text = text.to_lowercase();
    let at = format!("{repo}/pull/{n}");
    text.match_indices(&at).any(|(i, _)| !text[i + at.len()..].starts_with(|c: char| c.is_ascii_digit()))
}

/// The checks in one line: which failed, how many still run, or how many passed. `None` once the PR is closed or merged.
fn checks_line(pr: &Pr) -> Option<(Outcome, String)> {
    if pr.state != PrState::Open {
        return None;
    }
    let rows = check_rows(pr);
    let failed: Vec<&str> = rows.named.iter().filter(|c| c.outcome == Outcome::Failed).map(|c| c.name.as_str()).collect();
    let pending = rows.named.len() - failed.len();
    let checks = |n: usize| if n == 1 { "1 check".to_string() } else { format!("{n} checks") };
    Some(if !failed.is_empty() {
        (Outcome::Failed, format!("{} failed", failed.join(", ")))
    } else if pending > 0 {
        (Outcome::Pending, format!("{} in progress", checks(pending)))
    } else if rows.passed > 0 {
        (Outcome::Passed, format!("{} passed", checks(rows.passed)))
    } else {
        (Outcome::Skipped, "No checks".into())
    })
}

/// A PR a chat message links to, as one of the worktrees knows it.
pub(super) struct Peek {
    key: String,
    pr: Pr,
    verdict: (Token, String),
    /// The PR of the worktree on screen: the one the panel shows and fixes go to.
    here: bool,
    /// The failed checks and unresolved threads that can still go to the agent.
    fix: Option<(Vec<String>, Vec<String>)>,
    agent: &'static str,
}

/// The PRs `text` links to, among those the worktrees have.
pub(super) fn peeks(text: &str, prs: &PullRequests, state: &PullRequest, here: Option<&str>, agent: &'static str, can_send: bool) -> Arc<[Peek]> {
    if !text.contains("/pull/") {
        return Arc::new([]);
    }
    let found = prs.all().filter_map(|(tree, pr)| Some((tree, pr, pr_key(&pr.url)?))).filter(|(_, _, key)| mentions(text, key));
    found
        .map(|(tree, pr, key)| {
            let fixing = state.track.fixing(tree, pr);
            let busy = card_busy(state.busy.as_ref(), state.composing.as_deref(), tree);
            let here = here == Some(tree);
            let fix = (here && can_send && pr.state == PrState::Open && fixing.as_ref().is_none_or(|s| s.items() == 0))
                .then(|| to_fix(pr, fixing.as_ref()))
                .filter(|(checks, threads)| !checks.is_empty() || !threads.is_empty())
                .map(|(checks, threads)| (checks.iter().map(|c| c.name.clone()).collect(), threads.iter().map(|t| t.id.clone()).collect()));
            let verdict = verdict(pr, fixing.as_ref(), busy, agent);
            Peek { key, pr: pr.clone(), verdict, here, fix, agent }
        })
        .collect()
}

/// A GitHub PR link, as the plugin parses it.
struct Link {
    key: String,
    url: String,
    text: String,
}

/// A link to a GitHub pull request, as a chip: where the PR stands, with a card of its checks, review and threads on hover.
pub(super) struct PrLinks {
    pub(super) peeks: Arc<[Peek]>,
    pub(super) open: Open,
    pub(super) desktop: WeakEntity<Desktop>,
}

impl MarkdownPlugin for PrLinks {
    fn name(&self) -> &str {
        "pr-link"
    }

    fn parse(&self, node: &markdown_ast::Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let markdown_ast::Node::Link(link) = node else { return None };
        let key = pr_key(&link.url)?;
        let text: String = link.children.iter().map(|c| c.to_string()).collect();
        Some(MarkdownNode::new("pr-link", Link { key, url: link.url.clone(), text }).text(link.url.clone()))
    }

    fn render_inline(&self, node: &MarkdownNode, cx: &InlineRenderContext, window: &mut Window, _: &mut App) -> Option<InlineElement> {
        let link = node.data::<Link>()?;
        let at = self.peeks.iter().position(|p| p.key == link.key);
        let size = cx.font_size() * 0.93;
        let text = window.text_system();
        let sans = text.resolve_font(&font(SANS));
        let (ascent, descent) = (text.ascent(sans, size), px(f32::from(text.descent(sans, size)).abs()));
        let (name, color) = at.map_or(("pull-request", TEXT_3), |i| glyph(&self.peeks[i].pr));
        let open = (self.open.clone(), link.url.clone());
        let chip = div()
            .id("pr-link")
            .flex()
            .items_center()
            .gap(px(5.))
            .pl(px(5.))
            .pr(px(7.))
            .py(size * 0.15)
            .rounded(px(6.))
            .bg(FILL_4)
            .hover(|s| s.bg(FILL_3))
            .cursor_pointer()
            .text_size(size)
            .line_height(ascent + descent)
            .text_color(TEXT)
            .whitespace_nowrap()
            .child(icon(name, 13., color))
            .map(|chip| match at {
                Some(i) => {
                    let p = &self.peeks[i];
                    chip.child(div().max_w(px(320.)).truncate().font_weight(FontWeight::MEDIUM).child(p.pr.title.clone()))
                        .child(div().text_color(TEXT_3).child(format!("#{}", p.pr.number)))
                        .when(p.pr.state == PrState::Open, |c| c.child(ui::dot(6., p.verdict.0)))
                }
                None => chip.child(label(&link.key, &link.text, &link.url)),
            })
            .on_click(move |_, window, cx| (open.0)(&open.1, window, cx));
        let baseline = size * 0.15 + ascent;
        let Some(i) = at else { return Some(InlineElement::new(chip).with_baseline(baseline)) };
        let (peeks, desktop) = (self.peeks.clone(), self.desktop.clone());
        let card = HoverCard::new(SharedString::from(format!("pr-peek-{}", link.key)))
            // Above the chip: a PR link mostly sits in the latest reply, just over the composer.
            .anchor(Anchor::BottomLeft)
            .open_delay(Duration::from_millis(350))
            .trigger(chip)
            .content(move |_, _, _| div().id("pr-peek").pb(px(6.)).child(ui::menu_in("pr-peek-in", card(&peeks[i], desktop))));
        Some(InlineElement::new(card).with_baseline(baseline))
    }
}

/// A thread's file and line, without the folders: the card is too narrow for both a path and what was said.
fn spot(t: &Thread) -> String {
    let file = t.path.rsplit('/').next().unwrap_or(&t.path);
    t.line.map_or(file.to_string(), |line| format!("{file}:{line}"))
}

fn row(lead: impl IntoElement, text: String) -> Div {
    div().min_h(px(26.)).flex().items_center().gap(px(8.)).text_size(px(12.5)).text_color(TEXT_2).child(div().size(px(13.)).flex_none().flex().items_center().justify_center().child(lead)).child(div().flex_1().min_w_0().truncate().child(text))
}

/// The card a PR chip peeks: where the PR stands, its checks, review and open threads, and what can be done from here.
fn card(p: &Peek, desktop: WeakEntity<Desktop>) -> Div {
    let pr = &p.pr;
    let (name, color) = glyph(pr);
    let checks = checks_line(pr).map(|(outcome, text)| {
        let lead = match outcome {
            Outcome::Failed => icon("x", 13., FAILED).into_any_element(),
            Outcome::Pending => spinner("pr-peek-checks", 12., WAITING).into_any_element(),
            Outcome::Passed => icon("check", 13., SUCCESS).into_any_element(),
            Outcome::Skipped => ui::dot(6., TEXT_4).into_any_element(),
        };
        row(lead, text)
    });
    let reviews = review_lines(pr).into_iter().map(|(r, text)| row(review_icon(r), text));
    let threads = pr.unresolved().take(THREADS_SHOWN).map(|t| {
        let said = t.comments.first().and_then(|c| c.body.lines().find(|l| !l.trim().is_empty())).unwrap_or_default().to_string();
        row(icon("comment", 13., TEXT_3), said).child(div().flex_none().font_family(MONO).text_size(px(11.5)).text_color(TEXT_4).child(spot(t)))
    });
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
    let web = {
        let (desktop, url) = (desktop.clone(), pr.url.clone());
        ui::icon_button_sized("pr-peek-web", "external", 24., TEXT_3).on_click(move |_, window, cx| {
            desktop.update(cx, |d, cx| d.open_pr(url.clone(), window, cx)).ok();
        })
    };
    let fix = p.fix.clone().map(|(checks, threads)| {
        small("pr-peek-fix", Variant::Primary, format!("Send to {}", p.agent)).on_click(move |_, _, cx| {
            desktop.update(cx, |d, cx| d.send_fix(&checks, &threads, cx)).ok();
        })
    });
    let foot = div().flex().items_center().gap(px(6.)).pt(px(10.)).border_t_1().border_color(SEPARATOR).children(panel).child(web).child(div().flex_1()).children(fix);
    ui::pop(div())
        .w(px(PEEK_WIDTH))
        .p(px(12.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(head)
        .child(div().flex().flex_col().children(checks).children(reviews).children(threads))
        .child(foot)
}

#[cfg(test)]
mod tests {
    use super::{checks_line, label, mentions, pr_key};
    use git::github::{Check, Outcome, Pr, PrState};

    fn pr(runs: &[(&str, Outcome)]) -> Pr {
        Pr { runs: runs.iter().map(|(n, o)| Check { name: n.to_string(), outcome: *o, url: String::new() }).collect(), ..Default::default() }
    }

    #[test]
    fn a_github_pull_request_url_names_its_repo_and_number() {
        assert_eq!(pr_key("https://github.com/Mingo023/Anywhere/pull/198").as_deref(), Some("mingo023/anywhere#198"));
        assert_eq!(pr_key("https://github.com/o/r/pull/7/files#diff-1").as_deref(), Some("o/r#7"));
        assert_eq!(pr_key("github.com/o/r/pull/7?w=1").as_deref(), Some("o/r#7"));
        for url in ["https://github.com/o/r/issues/7", "https://github.com/o/r/pull/", "https://gitlab.com/o/r/pull/7", "https://github.com/o/r/pull/7a", "src/pull/7.rs"] {
            assert_eq!(pr_key(url), None, "{url}");
        }
    }

    #[test]
    fn a_message_mentions_a_pr_only_by_its_exact_number() {
        assert!(mentions("Opened https://github.com/O/R/pull/19.", "o/r#19"));
        assert!(!mentions("Opened https://github.com/o/r/pull/198", "o/r#19"));
        assert!(!mentions("Opened https://github.com/o/other/pull/19", "o/r#19"));
    }

    #[test]
    fn a_chip_for_an_unknown_pr_keeps_the_links_own_words() {
        assert_eq!(label("o/r#7", "the fix", "https://github.com/o/r/pull/7"), "the fix");
        assert_eq!(label("o/r#7", "https://github.com/o/r/pull/7", "https://github.com/o/r/pull/7"), "r#7");
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
