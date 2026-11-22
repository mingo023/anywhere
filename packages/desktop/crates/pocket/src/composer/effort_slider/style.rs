use gpui_kit::*;
use theme::*;

/// Each step of the scale, low to max, as its track and its light.
const PALETTE: [(u32, u32); 5] = [(0x70859c, 0xb9c7d8), (0x6e9cd7, 0xc1dcff), (0x8497f4, 0xc1cbff), (0xa294ef, 0xd7ceff), (0xbb9feb, 0xf4dcff)];

/// The slider's colours with the thumb at `t` of the way along, blended between the palette's steps so they never jump.
#[derive(Clone, Copy)]
pub(crate) struct EffortStyle {
    pub(crate) track: Hsla,
    pub(crate) light: Hsla,
    pub(crate) rail: Hsla,
    pub(crate) mark: Hsla,
    pub(crate) mark_lit: Hsla,
}

impl EffortStyle {
    pub(crate) fn at(t: f32) -> Self {
        let (track, light) = sample(t);
        Self { track, light, rail: FILL_4.into(), mark: ink(0.22), mark_lit: hsla(0., 0., 1., 0.55) }
    }
}

fn sample(t: f32) -> (Hsla, Hsla) {
    let at = t.clamp(0., 1.) * (PALETTE.len() - 1) as f32;
    let i = (at as usize).min(PALETTE.len() - 2);
    let f = at - i as f32;
    let ((a, la), (b, lb)) = (PALETTE[i], PALETTE[i + 1]);
    (mix(rgb(a).into(), rgb(b).into(), f), mix(rgb(la).into(), rgb(lb).into(), f))
}

/// Blends in RGB: blending hue would pass through colours neither end has.
pub(crate) fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let (a, b) = (a.to_rgb(), b.to_rgb());
    let lerp = |x: f32, y: f32| x + (y - x) * t;
    Rgba { r: lerp(a.r, b.r), g: lerp(a.g, b.g), b: lerp(a.b, b.b), a: lerp(a.a, b.a) }.into()
}

#[cfg(test)]
mod tests {
    use super::{PALETTE, sample};
    use gpui_kit::{Hsla, rgb};

    fn close(a: Hsla, hex: u32) -> bool {
        let (a, b) = (a.to_rgb(), rgb(hex));
        [(a.r, b.r), (a.g, b.g), (a.b, b.b)].iter().all(|(x, y)| (x - y).abs() < 0.01)
    }

    #[test]
    fn each_step_of_the_scale_is_its_own_colour_and_the_way_between_blends_them() {
        for (i, (track, light)) in PALETTE.iter().enumerate() {
            let (t, l) = sample(i as f32 / 4.);
            assert!(close(t, *track) && close(l, *light), "step {i}");
        }
        let (low, medium) = (rgb(PALETTE[0].0), rgb(PALETTE[1].0));
        assert!((sample(0.125).0.to_rgb().r - (low.r + medium.r) / 2.).abs() < 0.01);
    }
}
