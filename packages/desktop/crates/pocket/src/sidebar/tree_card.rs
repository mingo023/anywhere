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
/// The farthest a swapped card slides in from, about one row.
const SLIDE: f32 = 29.;

/// Which tree row's card shows: once the pointer rests on a row, then at once on each row it moves to.
/// Also which row the pointer is on, where a swapped card slides from, and how far the hovered row's label overflows.
#[derive(Default)]
pub(crate) struct Hover {
    /// The row under the pointer, when and at what height it entered.
    over: Option<(String, Instant, f32)>,
    /// The row under the pointer, even after a press.
    on: Option<String>,
    /// The row whose card shows, and whether it fades in rather than swapping for another's.
    shown: Option<(String, bool)>,
    hid: Option<Instant>,
    /// The last row whose label was measured, the label's width and how many px it overflows that row.
    label: Option<(String, (f32, f32))>,
    /// The pointer's height when it entered the shown card's row.
    at: Option<f32>,
    /// How far above the swapped-in card it slides from, and a count that restarts the slide on each swap.
    slide: (f32, usize),
}

impl Hover {
    /// The pointer entered `row`. True when its card shows at once; else ask [`Self::waited`] after [`WAIT`].
    pub(crate) fn enter(&mut self, row: &str, now: Instant, y: f32) -> bool {
        self.over = Some((row.to_string(), now, y));
        self.on = Some(row.to_string());
        let warm = self.shown.is_some() || self.hid.is_some_and(|at| now.duration_since(at) < WARM);
        if warm {
            self.shown = Some((row.to_string(), false));
            if let Some(from) = self.at {
                self.slide = ((from - y).clamp(-SLIDE, SLIDE), self.slide.1 + 1);
            }
            self.at = Some(y);
        }
        warm
    }

    /// True when the pointer has rested on `row` since [`WAIT`] ago and its card now shows.
    pub(crate) fn waited(&mut self, row: &str, now: Instant) -> bool {
        let rested = self.over.as_ref().filter(|(r, at, _)| r == row && now.duration_since(*at) >= WAIT);
        let shows = rested.is_some() && self.shown.is_none();
        if shows {
            self.shown = Some((row.to_string(), true));
            self.at = rested.map(|(_, _, y)| *y);
            self.slide.0 = 0.;
        }
        shows
    }

    /// The pointer left `row`. True when that hid its card.
    pub(crate) fn leave(&mut self, row: &str, now: Instant) -> bool {
        if self.over.as_ref().is_some_and(|(r, _, _)| r == row) {
            self.over = None;
        }
        if self.on.as_deref() == Some(row) {
            self.on = None;
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
        self.at = None;
        self.shown.take().is_some()
    }

    /// Whether the pointer is on `row`. Rows lay out by this, since GPUI applies hover styles only when painting.
    pub(crate) fn on(&self, row: &str) -> bool {
        self.on.as_deref() == Some(row)
    }

    /// Whether `row`'s card shows, and if so whether it fades in.
    pub(crate) fn shows(&self, row: &str) -> Option<bool> {
        self.shown.as_ref().filter(|(r, _)| r == row).map(|(_, fades)| *fades)
    }

    /// While a swapped-in card shows: how far above it slides from, and a count that tells each swap apart.
    pub(crate) fn slide(&self) -> Option<(f32, usize)> {
        Some(self.slide).filter(|(dy, _)| *dy != 0. && self.shown.as_ref().is_some_and(|(_, fades)| !fades))
    }

    /// `row`'s label's width and how many px it overflows the row, once measured.
    pub(crate) fn label(&self, row: &str) -> (f32, f32) {
        self.label.as_ref().filter(|(r, _)| r == row).map_or((0., 0.), |(_, fit)| *fit)
    }

    /// `row`'s label is `width` px wide and overflows the row by `overflow`. True when that changed what [`Self::label`] says.
    pub(crate) fn measured(&mut self, row: &str, width: f32, overflow: f32) -> bool {
        let moved = |(w, o): (f32, f32)| (w - width).abs() > 0.5 || (o - overflow).abs() > 0.5;
        let changed = self.label.as_ref().is_none_or(|(r, fit)| r != row || moved(*fit));
        if changed {
            self.label = Some((row.to_string(), (width, overflow)));
        }
        changed
    }
}

fn eyebrow(label: &'static str) -> Div {
    div().mt(px(12.)).text_size(px(10.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_4).child(label)
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
            .child(div().h(px(20.)).px(px(7.)).flex().items_center().rounded(px(6.)).bg(bg).text_size(px(11.)).font_weight(FontWeight::MEDIUM).text_color(fg).child(detail(pr)))
    });
    ui::pop(div())
        .w(px(260.))
        .pt(px(12.))
        .px(px(14.))
        .pb(px(12.))
        .text_size(px(12.))
        .text_color(TEXT_2)
        .child(div().text_size(px(13.)).line_height(rems(1.35)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(title))
        .child(eyebrow("BRANCH"))
        .child(div().mt(px(4.)).font_family(MONO).text_size(px(12.)).text_color(TEXT_BODY).truncate().child(branch))
        .when_some(pr, |d, pr| d.child(eyebrow("PULL REQUEST")).child(pr))
        .when_some(when, |d, when| d.child(div().mt(px(10.)).text_color(TEXT_4).child(when)))
}

/// The card hung right of its `relative` row, past the column's edge, painted above the panes and kept inside the window.
/// `slide` moves a swapped-in card from the previous row's height to its own.
pub(crate) fn hung(title: String, branch: String, pr: Option<&Pr>, when: Option<String>, fades: bool, slide: Option<(f32, usize)>) -> Div {
    let card = card(title, branch, pr, when);
    let card = match (fades, slide) {
        (true, _) => card.with_animation("aside-tree-card-in", Animation::new(Duration::from_millis(140)).with_easing(ease_out_quint()), |c, t| c.opacity(t)).into_any_element(),
        (false, Some((dy, swap))) => card.with_animation(("aside-tree-card-slide", swap), Animation::new(Duration::from_millis(120)).with_easing(ease_out_quint()), move |c, t| c.mt(px(dy * (1. - t)))).into_any_element(),
        (false, None) => card.into_any_element(),
    };
    div().absolute().top(px(-6.)).right(px(-16.)).child(deferred(anchored().snap_to_window_with_margin(px(8.)).child(card)).with_priority(1))
}

#[cfg(test)]
mod tests {
    use super::{Hover, SLIDE, WAIT, WARM};
    use std::time::{Duration, Instant};

    #[test]
    fn a_rows_card_shows_once_the_pointer_rests_on_it() {
        let (mut hover, t0) = (Hover::default(), Instant::now());
        assert!(!hover.enter("/a", t0, 0.));
        assert!(!hover.waited("/a", t0 + WAIT - Duration::from_millis(1)));
        assert!(hover.waited("/a", t0 + WAIT));
        assert_eq!(hover.shows("/a"), Some(true));
    }

    #[test]
    fn leaving_before_the_wait_shows_nothing_even_when_the_pointer_comes_back() {
        let (mut hover, t0) = (Hover::default(), Instant::now());
        hover.enter("/a", t0, 0.);
        assert!(!hover.leave("/a", t0 + WAIT / 2));
        hover.enter("/a", t0 + WAIT / 2 + Duration::from_millis(10), 0.);
        assert!(!hover.waited("/a", t0 + WAIT));
        assert_eq!(hover.shows("/a"), None);
    }

    #[test]
    fn once_a_card_shows_moving_to_another_row_swaps_it_at_once_whichever_event_comes_first() {
        let t0 = Instant::now();
        let shown = || {
            let mut hover = Hover::default();
            hover.enter("/a", t0, 0.);
            hover.waited("/a", t0 + WAIT);
            hover
        };
        let mut entered_first = shown();
        assert!(entered_first.enter("/b", t0 + WAIT * 2, 0.));
        assert!(!entered_first.leave("/a", t0 + WAIT * 2));
        let mut left_first = shown();
        assert!(left_first.leave("/a", t0 + WAIT * 2));
        assert!(left_first.enter("/b", t0 + WAIT * 2, 0.));
        assert_eq!([entered_first.shows("/b"), left_first.shows("/b"), left_first.shows("/a")], [Some(false), Some(false), None]);
    }

    #[test]
    fn after_a_card_hides_the_next_row_waits_again() {
        let (mut hover, t0) = (Hover::default(), Instant::now());
        hover.enter("/a", t0, 0.);
        hover.waited("/a", t0 + WAIT);
        hover.leave("/a", t0 + WAIT);
        assert!(!hover.enter("/b", t0 + WAIT + WARM, 0.));
    }

    #[test]
    fn a_row_holds_the_pointer_from_entering_until_leaving() {
        let (mut hover, t0) = (Hover::default(), Instant::now());
        hover.enter("/a", t0, 0.);
        assert!(hover.on("/a") && !hover.on("/b"));
        hover.leave("/a", t0);
        assert!(!hover.on("/a"));
    }

    #[test]
    fn a_press_hides_the_card_until_the_pointer_rests_again() {
        let (mut hover, t0) = (Hover::default(), Instant::now());
        hover.enter("/a", t0, 0.);
        hover.waited("/a", t0 + WAIT);
        assert!(hover.press());
        assert!(hover.on("/a"));
        assert!(!hover.waited("/a", t0 + WAIT * 3));
        assert!(!hover.enter("/b", t0 + WAIT * 3, 0.));
    }

    #[test]
    fn a_swapped_card_slides_from_the_previous_rows_height_at_most_a_row_away() {
        let (mut hover, t0) = (Hover::default(), Instant::now());
        hover.enter("/a", t0, 100.);
        hover.waited("/a", t0 + WAIT);
        assert_eq!(hover.slide(), None);
        hover.enter("/b", t0 + WAIT, 110.);
        assert_eq!(hover.slide(), Some((-10., 1)));
        hover.enter("/c", t0 + WAIT, 300.);
        assert_eq!(hover.slide(), Some((-SLIDE, 2)));
        hover.press();
        hover.enter("/a", t0 + WAIT * 2, 100.);
        hover.waited("/a", t0 + WAIT * 3);
        assert_eq!(hover.slide(), None);
    }

    #[test]
    fn a_label_overflows_only_the_row_it_was_measured_in_and_ignores_subpixel_jitter() {
        let mut hover = Hover::default();
        assert_eq!(hover.label("/a"), (0., 0.));
        assert!(hover.measured("/a", 240., 40.));
        assert!(!hover.measured("/a", 240.2, 40.3));
        assert!(hover.measured("/a", 240., 18.));
        assert_eq!((hover.label("/a"), hover.label("/b")), ((240., 18.), (0., 0.)));
        assert!(hover.measured("/b", 90., -12.));
        assert_eq!(hover.label("/a"), (0., 0.));
    }
}
