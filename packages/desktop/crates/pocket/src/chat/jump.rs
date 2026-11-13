use gpui_kit::*;
use std::time::Duration;
use theme::*;

pub(super) const GLIDE: Duration = Duration::from_millis(300);

/// A scroll to the end, eased over 300ms; past 2.5 viewports there's no glide and the list jumps.
#[derive(Debug, PartialEq)]
pub(super) struct Glide {
    distance: f32,
    moved: f32,
}

impl Glide {
    pub(super) fn new(distance: f32, viewport: f32) -> Option<Self> {
        (distance <= viewport * 2.5).then_some(Self { distance, moved: 0. })
    }

    /// The pixels to scroll `elapsed` into the glide, and whether it has arrived.
    pub(super) fn step(&mut self, elapsed: Duration) -> (f32, bool) {
        let t = (elapsed.as_secs_f32() / GLIDE.as_secs_f32()).min(1.);
        let to = self.distance * ease_out_quint()(t);
        let by = to - self.moved;
        self.moved = to;
        (by, t >= 1.)
    }
}

/// A scroll to a prompt row, eased over 300ms. It aims afresh each frame, as the rows it passes are measured only once
/// they show.
#[derive(Debug, PartialEq)]
pub(super) struct Seek {
    pub(super) row: usize,
    /// Whether the prompt lights up once there.
    pub(super) lights: bool,
    eased: f32,
}

impl Seek {
    pub(super) fn new(row: usize) -> Self {
        Self { row, lights: false, eased: 0. }
    }

    /// Has the prompt light up on arrival if it's the one this seek is going to; whether it was.
    pub(super) fn land(&mut self, row: usize) -> bool {
        self.lights |= row == self.row;
        row == self.row
    }

    /// The pixels to scroll `elapsed` into the seek, `remaining` short of the row, and whether it has arrived.
    pub(super) fn step(&mut self, elapsed: Duration, remaining: f32) -> (f32, bool) {
        let t = (elapsed.as_secs_f32() / GLIDE.as_secs_f32()).min(1.);
        let eased = ease_out_quint()(t);
        // The curve reaches 1 in f32 a few ms before the end; what's left goes at once rather than as 0/0.
        let left = 1. - self.eased;
        let by = if t >= 1. || left <= 0. { remaining } else { remaining * (eased - self.eased) / left };
        self.eased = eased;
        (by, t >= 1.)
    }
}

/// The row a seek from row `top` to row `target` starts on: the rows more than a viewport short of the target are
/// jumped over, so it glides the last viewport only. A row never measured counts as a quarter of one.
pub(super) fn lead_in(top: usize, target: usize, viewport: f32, height: impl Fn(usize) -> Option<f32>) -> usize {
    let mut covered = 0.;
    let mut fills_viewport = |ix: usize| {
        covered += height(ix).unwrap_or(viewport / 4.);
        covered >= viewport
    };
    if target < top { (target..top).find(|&ix| fills_viewport(ix)).map_or(top, |ix| ix + 1) } else { (top..target).rev().find(|&ix| fills_viewport(ix)).unwrap_or(top) }
}

/// Once offered, the pill stays until the end is within 2px; one 320px threshold dropped it halfway through a scroll down.
/// While the agent works it isn't offered: a reply streaming in moves the end away from whoever scrolled up to read.
pub(super) fn jump_visibility(was_shown: bool, distance: f32, busy: bool) -> bool {
    !busy && distance > if was_shown { 2. } else { 320. }
}

/// "↓ Scroll to bottom", floating just above the composer.
pub(super) fn pill(on_click: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Div {
    let face = div()
        .id("chat-jump")
        .h(px(30.))
        .flex()
        .items_center()
        .gap(px(6.))
        .pl(px(11.))
        .pr(px(13.))
        .rounded_full()
        .border_1()
        .border_color(SEPARATOR)
        .bg(POPOVER)
        .hover(|d| d.bg(Hsla::from(POPOVER).blend(ink(0.06))))
        .active(|d| d.bg(Hsla::from(POPOVER).blend(ink(0.10))))
        .shadow(vec![ui::shadow(SHADOW, 4., 12.)])
        .text_size(px(13.))
        .cursor_pointer()
        .child(div().text_color(TEXT_2).child("↓"))
        .child(div().text_color(TEXT).child("Scroll to bottom"))
        .on_click(on_click)
        .with_animation("chat-jump-in", Animation::new(Duration::from_millis(180)).with_easing(ease_out_quint()), |d, t| d.opacity(t).mt(px(2. * (1. - t))));
    div().absolute().left_0().right_0().bottom(px(6.)).flex().justify_center().child(face)
}

#[cfg(test)]
mod tests {
    use super::{Glide, Seek, jump_visibility, lead_in};
    use std::time::Duration;

    #[test]
    fn a_seek_eases_onto_its_row_even_as_the_rows_between_grow() {
        let mut seek = Seek::new(3);
        let (first, done) = seek.step(Duration::from_millis(60), -1000.);
        assert!(!done && first < -500., "eases out: most of the way early");
        let remaining = -1000. - first - 400.;
        let (next, _) = seek.step(Duration::from_millis(150), remaining);
        let (last, done) = seek.step(Duration::from_millis(400), remaining - next);
        assert!(done);
        assert!((first + next + last + 1400.).abs() < 0.01, "lands on the row, 400px further than first thought");
    }

    #[test]
    fn a_seek_never_scrolls_by_a_non_number_as_the_curve_rounds_to_its_end() {
        let mut seek = Seek::new(0);
        for ms in [100, 292, 295, 299] {
            let (by, _) = seek.step(Duration::from_millis(ms), -10.);
            assert!(by.is_finite(), "at {ms}ms");
        }
    }

    #[test]
    fn letting_go_on_the_prompt_being_sought_lights_it_on_arrival() {
        let mut seek = Seek::new(4);
        assert!(!seek.land(2) && !seek.lights, "another prompt");
        assert!(seek.land(4) && seek.lights);
    }

    #[test]
    fn a_far_seek_jumps_to_a_viewport_short_of_its_row() {
        let heights = |_: usize| Some(300.);
        assert_eq!(lead_in(20, 2, 1000., heights), 6, "up: starts four 300px rows under the target");
        assert_eq!(lead_in(2, 20, 1000., heights), 16, "down: starts four rows over it");
        assert_eq!(lead_in(5, 3, 1000., heights), 5, "near: glides from where it is");
        assert_eq!(lead_in(20, 2, 1000., |_| None), 6, "unmeasured rows count a quarter viewport each");
    }

    #[test]
    fn the_jump_pill_shows_past_320px_and_hides_at_two() {
        assert!(!jump_visibility(false, 320., false));
        assert!(jump_visibility(false, 321., false));
        assert!(jump_visibility(true, 100., false));
        assert!(!jump_visibility(true, 2., false));
    }

    #[test]
    fn the_jump_pill_waits_for_the_agent_to_finish() {
        assert!(!jump_visibility(false, 1000., true));
        assert!(!jump_visibility(true, 1000., true));
        assert!(jump_visibility(false, 1000., false));
    }

    #[test]
    fn a_glide_eases_the_whole_distance_and_a_far_end_jumps() {
        assert_eq!(Glide::new(2501., 1000.), None);
        let mut glide = Glide::new(1000., 1000.).unwrap();
        let (first, done) = glide.step(Duration::from_millis(60));
        assert!(!done && first > 500., "eases out: most of the way early");
        let (rest, done) = glide.step(Duration::from_millis(400));
        assert!(done);
        assert!((first + rest - 1000.).abs() < 0.01);
    }
}
