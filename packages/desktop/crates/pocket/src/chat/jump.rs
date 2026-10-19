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

/// Once offered, the pill stays until the end is within 2px; one 320px threshold dropped it halfway through a scroll down.
pub(super) fn jump_visibility(was_shown: bool, distance: f32) -> bool {
    distance > if was_shown { 2. } else { 320. }
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
    use super::{Glide, jump_visibility};
    use std::time::Duration;

    #[test]
    fn the_jump_pill_shows_past_320px_and_hides_at_two() {
        assert!(!jump_visibility(false, 320.));
        assert!(jump_visibility(false, 321.));
        assert!(jump_visibility(true, 100.));
        assert!(!jump_visibility(true, 2.));
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
