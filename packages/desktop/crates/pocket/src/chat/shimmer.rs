use gpui_kit::*;
use std::time::Duration;
use theme::*;

const SWEEP: Duration = Duration::from_millis(1600);
const HALF_WIDTH: f32 = 0.25;

/// How lit `x` across a label (0 to 1) is `t` through a sweep: one band crossing left to right, from off one edge to off the other.
fn amount(x: f32, t: f32) -> f32 {
    let centre = -HALF_WIDTH + t.clamp(0., 1.) * (1. + 2. * HALF_WIDTH);
    (1. - (x - centre).abs() / HALF_WIDTH).clamp(0., 1.)
}

/// A label in `base` with a band of light sweeping across it: what's at work right now.
pub(super) fn shimmer(id: impl Into<ElementId>, text: impl Into<SharedString>, base: Token) -> AnimationElement<Div> {
    let text = text.into();
    let (base, peak): (Hsla, Hsla) = (base.into(), TEXT.into());
    // Uncapped, a repeating animation redraws the whole window every vsync.
    div().with_animation(id, Animation::new(SWEEP).repeat_synced().with_max_fps(20.), move |d, t| {
        let n = text.chars().count().max(1) as f32;
        let runs: Vec<_> = text
            .char_indices()
            .enumerate()
            .map(|(i, (at, c))| {
                let lit = amount((i as f32 + 0.5) / n, t);
                (at..at + c.len_utf8(), HighlightStyle { color: Some(base.blend(Hsla { a: lit, ..peak })), ..Default::default() })
            })
            .collect();
        d.whitespace_nowrap().overflow_hidden().child(StyledText::new(text.clone()).with_highlights(runs))
    })
}

#[cfg(test)]
mod tests {
    use super::amount;

    #[test]
    fn one_band_crosses_the_label_once_per_sweep() {
        assert_eq!(amount(0., 0.), 0.);
        assert_eq!(amount(0.5, 0.5), 1.);
        assert!(amount(0.6, 0.5) > 0.5 && amount(0.6, 0.5) < 1.);
        assert_eq!(amount(1., 1.), 0., "the band ends past the far edge");
        assert!(amount(0.9, 0.85) > amount(0.1, 0.85));
    }
}
