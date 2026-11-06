mod card;

use super::markdown::Open;
use crate::desktop::Desktop;
use crate::git_ui::pull_request::{PullRequest, Sent, card_busy, to_fix, verdict};
use crate::git_ui::pull_requests::{PullRequests, glyph};
use git::github::{Pr, PrState};
use gpui_kit::base::HoverCard;
use gpui_kit::base::text::{InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin, markdown_ast};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::sync::Arc;
use std::time::Duration;
use theme::*;

/// A GitHub pull request, as a URL names it.
#[derive(Clone, Debug, PartialEq)]
struct PrRef {
    owner: String,
    repo: String,
    number: u32,
}

impl PrRef {
    /// The PR a `http(s)://github.com/<owner>/<repo>/pull/<n>` URL points at, whatever follows the number. Without a
    /// scheme a link opens as a file, so it isn't one.
    fn parse(url: &str) -> Option<Self> {
        let rest = url.strip_prefix("https://").or_else(|| url.strip_prefix("http://"))?;
        let mut parts = rest.trim_start_matches("www.").split(['/', '#', '?']);
        let (host, owner, repo, pull, n) = (parts.next()?, parts.next()?, parts.next()?, parts.next()?, parts.next()?);
        let number = n.parse().ok().filter(|_| n.bytes().all(|b| b.is_ascii_digit()))?;
        (host.eq_ignore_ascii_case("github.com") && pull == "pull" && !owner.is_empty() && !repo.is_empty()).then(|| Self { owner: owner.into(), repo: repo.into(), number })
    }

    /// GitHub names are case-insensitive.
    fn same(&self, other: &Self) -> bool {
        self.number == other.number && self.owner.eq_ignore_ascii_case(&other.owner) && self.repo.eq_ignore_ascii_case(&other.repo)
    }

    /// Whether `lower`, a message lowercased, links to this PR: a cheap check before a PR is cloned for it.
    fn in_text(&self, lower: &str) -> bool {
        let at = format!("{}/{}/pull/{}", self.owner, self.repo, self.number).to_lowercase();
        lower.match_indices(&at).any(|(i, _)| !lower[i + at.len()..].starts_with(|c: char| c.is_ascii_digit()))
    }

    /// What a chip calls a PR it knows nothing about: the link's own words, else `repo#n`.
    fn label(&self, text: &str, url: &str) -> String {
        if !text.is_empty() && text != url { text.to_string() } else { format!("{}#{}", self.repo, self.number) }
    }
}

/// Where fixes from a card go: the worktree on screen and its agent, if it takes them.
pub(super) struct FixTo {
    pub(super) tree: Option<String>,
    pub(super) agent: &'static str,
    pub(super) can_send: bool,
}

/// The failed checks, by name, and the unresolved threads, by id, a card can hand to the agent.
#[derive(Clone, Debug, PartialEq)]
struct Fix {
    checks: Vec<String>,
    threads: Vec<String>,
}

/// What a card offers to fix: only the on-screen worktree's open PR, while nothing sent to its agent is in flight.
fn fix_offer(pr: &Pr, fixing: Option<&Sent>, here: bool, can_send: bool) -> Option<Fix> {
    if !(here && can_send && pr.state == PrState::Open && fixing.is_none_or(|s| s.items() == 0)) {
        return None;
    }
    let (checks, threads) = to_fix(pr, fixing);
    (!checks.is_empty() || !threads.is_empty()).then(|| Fix { checks: checks.iter().map(|c| c.name.clone()).collect(), threads: threads.iter().map(|t| t.id.clone()).collect() })
}

/// A PR a chat message links to, as one of the worktrees knows it.
pub(super) struct Peek {
    at: PrRef,
    pr: Pr,
    verdict: (Token, String),
    /// The PR of the worktree on screen: the one the panel shows.
    here: bool,
    fix: Option<Fix>,
    agent: &'static str,
}

/// The PRs `text` links to, among those the worktrees have.
pub(super) fn peeks(text: &str, prs: &PullRequests, state: &PullRequest, to: &FixTo) -> Arc<[Peek]> {
    let lower = text.to_lowercase();
    prs.all()
        .filter_map(|(tree, pr)| Some((tree, pr, PrRef::parse(&pr.url).filter(|at| at.in_text(&lower))?)))
        .map(|(tree, pr, at)| {
            let fixing = state.track.fixing(tree, pr);
            let busy = card_busy(state.busy.as_ref(), state.composing.as_deref(), tree);
            let here = to.tree.as_deref() == Some(tree);
            let fix = fix_offer(pr, fixing.as_ref(), here, to.can_send);
            Peek { at, pr: pr.clone(), verdict: verdict(pr, fixing.as_ref(), busy, to.agent), here, fix, agent: to.agent }
        })
        .collect()
}

/// What the plugin keeps of a PR link: where it points and the words it showed.
struct Link {
    at: PrRef,
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
        let at = PrRef::parse(&link.url)?;
        let text: String = link.children.iter().map(|c| c.to_string()).collect();
        Some(MarkdownNode::new("pr-link", Link { at, url: link.url.clone(), text }).text(link.url.clone()))
    }

    fn render_inline(&self, node: &MarkdownNode, cx: &InlineRenderContext, window: &mut Window, _: &mut App) -> Option<InlineElement> {
        let link = node.data::<Link>()?;
        let known = self.peeks.iter().position(|p| p.at.same(&link.at));
        let peek = known.map(|i| &self.peeks[i]);
        let size = cx.font_size() * 0.93;
        let text = window.text_system();
        let sans = text.resolve_font(&font(SANS));
        let (ascent, descent) = (text.ascent(sans, size), px(f32::from(text.descent(sans, size)).abs()));
        let (name, color) = peek.map_or(("pull-request", TEXT_3), |p| glyph(&p.pr));
        let (open, url) = (self.open.clone(), link.url.clone());
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
            .map(|chip| match peek {
                Some(p) => chip
                    .child(div().max_w(px(320.)).truncate().font_weight(FontWeight::MEDIUM).child(p.pr.title.clone()))
                    .child(div().text_color(TEXT_3).child(format!("#{}", p.pr.number)))
                    .when(p.pr.state == PrState::Open, |c| c.child(ui::dot(6., p.verdict.0))),
                None => chip.child(link.at.label(&link.text, &link.url)),
            })
            .on_click(move |_, window, cx| open(&url, window, cx));
        let baseline = size * 0.15 + ascent;
        let Some(i) = known else { return Some(InlineElement::new(chip).with_baseline(baseline)) };
        let (peeks, open, url, desktop) = (self.peeks.clone(), self.open.clone(), link.url.clone(), self.desktop.clone());
        let hover = HoverCard::new(SharedString::from(format!("pr-peek-{}", link.url)))
            // Above the chip: a PR link mostly sits in the latest reply, just over the composer.
            .anchor(Anchor::BottomLeft)
            .open_delay(Duration::from_millis(350))
            .trigger(chip)
            .content(move |_, _, _| div().id("pr-peek").pb(px(6.)).child(ui::menu_in("pr-peek-in", card::card(&peeks[i], open, url, desktop))));
        Some(InlineElement::new(hover).with_baseline(baseline))
    }
}

#[cfg(test)]
mod tests {
    use super::{Fix, PrRef, fix_offer};
    use crate::git_ui::pull_request::Track;
    use git::github::{Check, Comment, Outcome, Pr, PrState, Thread};

    fn at(url: &str) -> Option<(String, String, u32)> {
        PrRef::parse(url).map(|p| (p.owner, p.repo, p.number))
    }

    fn failing() -> Pr {
        let thread = Thread { id: "t1".into(), comments: vec![Comment { body: "Rename this".into(), ..Comment::default() }], ..Thread::default() };
        Pr { runs: vec![Check { name: "test".into(), outcome: Outcome::Failed, url: String::new() }], threads: vec![thread], ..Pr::default() }
    }

    #[test]
    fn a_github_pull_request_url_names_its_repo_and_number() {
        assert_eq!(at("https://github.com/Mingo023/Anywhere/pull/198"), Some(("Mingo023".into(), "Anywhere".into(), 198)));
        assert_eq!(at("https://www.github.com/o/r/pull/7/files#diff-1"), Some(("o".into(), "r".into(), 7)));
        for url in ["https://github.com/o/r/issues/7", "https://github.com/o/r/pull/", "https://gitlab.com/o/r/pull/7", "https://github.com/o/r/pull/7a", "github.com/o/r/pull/7"] {
            assert_eq!(at(url), None, "{url}");
        }
    }

    #[test]
    fn a_message_links_a_pr_only_by_its_exact_number() {
        let pr = PrRef::parse("https://github.com/o/R/pull/19").unwrap();
        assert!(pr.in_text("opened https://github.com/o/r/pull/19."));
        assert!(!pr.in_text("opened https://github.com/o/r/pull/198"));
        assert!(!pr.in_text("opened https://github.com/o/other/pull/19"));
        assert!(pr.same(&PrRef::parse("https://github.com/O/r/pull/19#x").unwrap()));
    }

    #[test]
    fn a_chip_for_an_unknown_pr_keeps_the_links_own_words() {
        let url = "https://github.com/o/Anywhere/pull/7";
        let pr = PrRef::parse(url).unwrap();
        assert_eq!(pr.label("the fix", url), "the fix");
        assert_eq!(pr.label(url, url), "Anywhere#7");
    }

    #[test]
    fn a_card_offers_a_fix_only_for_the_open_pr_on_screen_with_nothing_in_flight() {
        let fix = Some(Fix { checks: vec!["test".into()], threads: vec!["t1".into()] });
        assert_eq!(fix_offer(&failing(), None, true, true), fix);
        assert_eq!(fix_offer(&failing(), None, false, true), None, "another worktree's PR");
        assert_eq!(fix_offer(&failing(), None, true, false), None, "an observe-only agent");
        assert_eq!(fix_offer(&Pr { state: PrState::Merged, ..failing() }, None, true, true), None);
        let (pr, mut track) = (failing(), Track::default());
        track.send("/w", &pr, &[], &[&pr.runs[0]], false);
        assert_eq!(fix_offer(&pr, track.fixing("/w", &pr).as_ref(), true, true), None, "a fix already sent");
        assert_eq!(fix_offer(&Pr::default(), None, true, true), None, "nothing to fix");
    }
}
