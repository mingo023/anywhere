//! The mascot (`mascot-*.svg`): body, sheen, antenna light and eyes over a soft glow. Waiting, rings rise off its
//! antenna twice a beat and it blinks before the next.

use crate::welcome::view::{BODY, EYES, GLOW_COLOR, LIGHT_OFF, LIGHT_ON};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;

/// The antenna's light, as a share of the mascot SVG's box.
const LIGHT_AT: (f32, f32) = (131. / 200., 19. / 200.);
/// A ring's size, as a share of the mascot's.
const RING: f32 = 40. / 84.;
pub(crate) const BEAT: Duration = Duration::from_millis(4400);
/// A ring starts this small, fully grown at 1, and the second leaves this far into a pulse.
const RING_FROM: f32 = 0.27;
const RING_DELAY: f32 = 0.1;
const RING_OPACITY: f32 = 0.75;
/// Where in the beat the blink starts, and for how much of it.
const BLINK_AT: f32 = 0.955;
const BLINK_FOR: f32 = 0.035;
/// Where the eyes look, in the SVG's units: aside, or down.
const GLANCE: (f32, f32) = (5., 1.);
pub(crate) const DOWN: (f32, f32) = (4., 6.);

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Look {
    /// How open its eyes are; 1 is open.
    pub(crate) lid: f32,
    /// Each ring's scale and opacity.
    pub(crate) rings: [(f32, f32); 2],
    pub(crate) eyes: (f32, f32),
    pub(crate) happy: bool,
    pub(crate) lit: bool,
}

pub(crate) const STILL: Look = Look { lid: 1., rings: [(0., 0.); 2], eyes: GLANCE, happy: false, lit: true };

/// The mascot `t` of the way through a beat while it waits.
pub(crate) fn waiting(t: f32) -> Look {
    let ring = |delay: f32| {
        // Two pulses a beat, each ring moving through the first half of its pulse.
        let u = ((t * 2.).fract() - delay) / 0.5;
        if !(0. ..1.).contains(&u) {
            return (0., 0.);
        }
        (RING_FROM + (1. - RING_FROM) * (1. - (1. - u).powi(3)), RING_OPACITY * (1. - u))
    };
    let blink = ((t - BLINK_AT) / BLINK_FOR).clamp(0., 1.);
    Look { lid: 1. - 0.9 * (1. - (2. * blink - 1.).abs()), rings: [ring(0.), ring(RING_DELAY)], ..STILL }
}

/// The mascot in `look`, `side` px square.
pub(crate) fn mascot(look: Look, side: f32) -> Div {
    let k = side / 200.;
    let ring = RING * side;
    let shade = |hex: u32, alpha: f32| Hsla::from(rgb(hex)).opacity(alpha);
    let layer = |name: &str, color: Hsla| svg().path(format!("icons/mascot-{name}.svg")).absolute().top_0().left_0().size(px(side)).text_color(color);
    let eyes = Transformation::scale(size(1., look.lid)).with_translation(point(px(look.eyes.0 * k), px(look.eyes.1 * k)));
    let rings = look.rings.into_iter().filter(|(_, o)| *o > 0.).map(|(s, o)| {
        svg()
            .path("icons/agent-ring.svg")
            .absolute()
            .left(px(LIGHT_AT.0 * side - ring / 2.))
            .top(px(LIGHT_AT.1 * side - ring / 2.))
            .size(px(ring))
            .text_color(shade(LIGHT_ON, o))
            .with_transformation(Transformation::scale(size(s, s)))
    });
    let glow = side * 0.9;
    div()
        .relative()
        .flex_none()
        .size(px(side))
        .child(div().absolute().left(px((side - glow) / 2.)).top(px(side * 0.58 - glow / 2.)).size(px(glow)).rounded_full().shadow(vec![ui::shadow(
            shade(GLOW_COLOR, 0.4),
            0.,
            40.,
        )]))
        .child(div().absolute().top(px(side * 0.84)).left(px(side * 0.25)).w(px(side * 0.5)).h(px(4.)).rounded_full().shadow(vec![ui::shadow(
            shade(0x000000, 0.25),
            0.,
            6.,
        )]))
        .children(rings)
        .when(look.lit, |d| d.child(layer("halo", shade(LIGHT_ON, 0.35))))
        .child(layer("body", rgb(BODY).into()))
        .child(layer("sheen", shade(0xffffff, 0.08)))
        .child(layer("light", rgb(if look.lit { LIGHT_ON } else { LIGHT_OFF }).into()))
        .child(layer(if look.happy { "eyes-happy" } else { "eyes" }, rgb(EYES).into()).with_transformation(eyes))
}

#[cfg(test)]
mod tests {
    use super::{STILL, waiting};

    #[test]
    fn rings_rise_off_the_antenna_then_the_mascot_rests_and_blinks_before_the_next_beat() {
        assert_eq!(waiting(0.).rings[0], (0.27, 0.75));
        assert_eq!(waiting(0.4), STILL);
        assert!((waiting(0.9725).lid - 0.1).abs() < 1e-3, "eyes shut mid-blink");
        assert_eq!(waiting(0.999).lid, 1.);
    }
}
