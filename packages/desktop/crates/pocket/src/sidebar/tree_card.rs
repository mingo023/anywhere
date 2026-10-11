use crate::git_ui::pull_requests::{badge, detail, glyph};
use git::github::Pr;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::{Duration, Instant};
use theme::*;

/// How long the pointer rests on a tree row before its card shows.
pub(crate) const WAIT: Duration = Duration::from_millis(350);
/// How long after a card hides the next row's shows without waiting.
const WARM: Duration = Duration::from_millis(300);

/// Which tree row's card shows: once the pointer rests on a row, then at once on each row it moves to.
#[derive(Default)]
pub(crate) struct Hover {
    over: Option<(String, Instant)>,
    /// The row whose card shows, and whether it fades in rather than swapping for another's.
    shown: Option<(String, bool)>,
    hid: Option<Instant>,
}

impl Hover {
    /// The pointer entered `row`. True when its card shows at once; else ask [`Self::waited`] after [`WAIT`].
    pub(crate) fn enter(&mut self, row: &str, now: Instant) -> bool {
        self.over = Some((row.to_string(), now));
        let warm = self.shown.is_some() || self.hid.is_some_and(|at| now.duration_since(at) < WARM);
        if warm {
            self.shown = Some((row.to_string(), false));
        }
        warm
    }

    /// True when the pointer has rested on `row` since [`WAIT`] ago and its card now shows.
    pub(crate) fn waited(&mut self, row: &str, now: Instant) -> bool {
        let rested = self.over.as_ref().is_some_and(|(r, at)| r == row && now.duration_since(*at) >= WAIT);
        let shows = rested && self.shown.is_none();
        if shows {
            self.shown = Some((row.to_string(), true));
        }
        shows
    }

    /// The pointer left `row`. True when that hid its card.
    pub(crate) fn leave(&mut self, row: &str, now: Instant) -> bool {
        if self.over.as_ref().is_some_and(|(r, _)| r == row) {
            self.over = None;
        }
        let hides = self.shows(row).is_some();
        if hides {
            self.shown = None;
            self.hid = Some(now);
        }
        hides
    }

    /// A press hides the card until the pointer rests on a row again. True when one showed.
    pub(crate) fn press(&mut self) -> bool {
        self.over = None;
        self.hid = None;
        self.shown.take().is_some()
    }

    /// Whether `row`'s card shows, and if so whether it fades in.
    pub(crate) fn shows(&self, row: &str) -> Option<bool> {
        self.shown.as_ref().filter(|(r, _)| r == row).map(|(_, fades)| *fades)
    }
}

fn eyebrow(label: &'static str) -> Div {
    div().mt(px(16.)).text_size(px(11.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_4).child(label)
}

/// A tree row's card: its name, branch and PR, and how long since its sessions last moved.
fn card(title: String, branch: String, pr: Option<&Pr>, when: Option<String>) -> Div {
    let pr = pr.map(|pr| {
        let (name, color) = glyph(pr);
        let (bg, fg) = badge(pr);
        div()
            .mt(px(4.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_color(TEXT_BODY)
            .child(icon(name, 13., color))
            .child(div().font_family(MONO).child(format!("#{}", pr.number)))
            .child(div().h(px(20.)).px(px(7.)).flex().items_center().rounded(px(6.)).bg(bg).text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(fg).child(detail(pr)))
    });
    ui::pop(div())
        .w(px(300.))
        .pt(px(18.))
        .px(px(20.))
        .pb(px(16.))
        .text_size(px(13.))
        .text_color(TEXT_2)
        .child(div().text_size(px(16.)).line_height(rems(1.35)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(title))
        .child(eyebrow("BRANCH"))
        .child(div().mt(px(4.)).font_family(MONO).text_size(px(14.)).text_color(TEXT_BODY).truncate().child(branch))
        .when_some(pr, |d, pr| d.child(eyebrow("PULL REQUEST")).child(pr))
        .when_some(when, |d, when| d.child(div().mt(px(14.)).text_color(TEXT_4).child(when)))
}

/// The card hung right of its `relative` row, past the column's edge, painted above the panes and kept inside the window.
pub(crate) fn hung(title: String, branch: String, pr: Option<&Pr>, when: Option<String>, fades: bool) -> Div {
    let card = card(title, branch, pr, when);
    let card = if fades { card.with_animation("aside-tree-card-in", Animation::new(Duration::from_millis(140)).with_easing(ease_out_quint()), |c, t| c.opacity(t)).into_any_element() } else { card.into_any_element() };
    div().absolute().top(px(-6.)).right(px(-16.)).child(deferred(anchored().snap_to_window_with_margin(px(8.)).child(card)).with_priority(1))
}

#[cfg(test)]
mod tests {
    use super::{Hover, WAIT, WARM};
    use std::time::{Duration, Instant};

    #[test]
    fn a_rows_card_shows_once_the_pointer_rests_on_it() {
        let (mut hover, t0) = (Hover::default(), Instant::now());
        assert!(!hover.enter("/a", t0));
        assert!(!hover.waited("/a", t0 + WAIT - Duration::from_millis(1)));
        assert!(hover.waited("/a", t0 + WAIT));
        assert_eq!(hover.shows("/a"), Some(true));
    }

    #[test]
    fn leaving_before_the_wait_shows_nothing_even_when_the_pointer_comes_back() {
        let (mut hover, t0) = (Hover::default(), Instant::now());
        hover.enter("/a", t0);
        assert!(!hover.leave("/a", t0 + WAIT / 2));
        hover.enter("/a", t0 + WAIT / 2 + Duration::from_millis(10));
        assert!(!hover.waited("/a", t0 + WAIT));
        assert_eq!(hover.shows("/a"), None);
    }

    #[test]
    fn once_a_card_shows_moving_to_another_row_swaps_it_at_once_whichever_event_comes_first() {
        let t0 = Instant::now();
        let shown = || {
            let mut hover = Hover::default();
            hover.enter("/a", t0);
            hover.waited("/a", t0 + WAIT);
            hover
        };
        let mut entered_first = shown();
        assert!(entered_first.enter("/b", t0 + WAIT * 2));
        assert!(!entered_first.leave("/a", t0 + WAIT * 2));
        let mut left_first = shown();
        assert!(left_first.leave("/a", t0 + WAIT * 2));
        assert!(left_first.enter("/b", t0 + WAIT * 2));
        assert_eq!([entered_first.shows("/b"), left_first.shows("/b"), left_first.shows("/a")], [Some(false), Some(false), None]);
    }

    #[test]
    fn after_a_card_hides_the_next_row_waits_again() {
        let (mut hover, t0) = (Hover::default(), Instant::now());
        hover.enter("/a", t0);
        hover.waited("/a", t0 + WAIT);
        hover.leave("/a", t0 + WAIT);
        assert!(!hover.enter("/b", t0 + WAIT + WARM));
    }

    #[test]
    fn a_press_hides_the_card_until_the_pointer_rests_again() {
        let (mut hover, t0) = (Hover::default(), Instant::now());
        hover.enter("/a", t0);
        hover.waited("/a", t0 + WAIT);
        assert!(hover.press());
        assert!(!hover.waited("/a", t0 + WAIT * 3));
        assert!(!hover.enter("/b", t0 + WAIT * 3));
    }
}
