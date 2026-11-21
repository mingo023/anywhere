//! The mascot dropping in to cheer a finished handoff; each click on it hops again.

use crate::welcome::view::{BODY, EYES, FEET, GLOW_COLOR as GLOW, LIGHT_ON as LIGHT};
use gpui_kit::*;
use std::time::Duration;

const STAGE_W: f32 = 200.;
const STAGE_H: f32 = 150.;
const MASCOT: f32 = 140.;
const CONFETTI: [u32; 6] = [0xf5b83d, 0x8ee6cc, 0x7a68bf, 0xf27a8b, 0x5aa9f5, 0x9bd16a];

/// Rise (negative is up), width and height scale at one moment.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Pose {
    y: f32,
    sx: f32,
    sy: f32,
}

const fn pose(y: f32, sx: f32, sy: f32) -> Pose {
    Pose { y, sx, sy }
}

/// Falls, squashes from its feet, rebounds once and settles.
const DROP: [(f32, Pose); 6] = [(0., pose(-64., 0.92, 1.1)), (0.34, pose(0., 1.14, 0.8)), (0.52, pose(-26., 0.96, 1.05)), (0.66, pose(0., 1.06, 0.93)), (0.8, pose(0., 0.98, 1.02)), (1., pose(0., 1., 1.))];
const HOP: [(f32, Pose); 4] = [(0., pose(0., 1.12, 0.84)), (0.35, pose(-34., 0.94, 1.08)), (0.7, pose(0., 1.08, 0.92)), (1., pose(0., 1., 1.))];

/// The pose `t` of the way through `keys`, eased in and out between each pair.
fn at(keys: &[(f32, Pose)], t: f32) -> Pose {
    let t = t.clamp(0., 1.);
    let i = keys.iter().rposition(|(k, _)| *k <= t).unwrap_or(0).min(keys.len() - 2);
    let ((t0, a), (t1, b)) = (keys[i], keys[i + 1]);
    let u = (t - t0) / (t1 - t0);
    let u = u * u * (3. - 2. * u);
    let mix = |a: f32, b: f32| a + (b - a) * u;
    pose(mix(a.y, b.y), mix(a.sx, b.sx), mix(a.sy, b.sy))
}

fn stage(p: Pose, t: f32, first: bool, still: bool) -> Div {
    let feet = MASCOT * (FEET - 0.5) * (1. - p.sy);
    let body = Transformation::scale(size(p.sx, p.sy)).with_translation(point(px(0.), px(p.y + feet)));
    let layer = |name: &str, color: Hsla| svg().path(format!("icons/mascot-{name}.svg")).absolute().left(px((STAGE_W - MASCOT) / 2.)).bottom(px(8.)).size(px(MASCOT)).text_color(color).with_transformation(body);
    let shade = |hex: u32, alpha: f32| Hsla::from(rgb(hex)).opacity(alpha);
    let shown = if first { (t / 0.12).min(1.) } else { 1. };
    let near = 1. - (-p.y / 64.).clamp(0., 1.);
    let shadow_w = 84. * (0.3 + 0.7 * near);
    let confetti = (!still).then(|| {
        let u = 1. - (1. - t).powi(3);
        let fade = if t < 0.7 { 1. } else { (1. - t) / 0.3 };
        CONFETTI.iter().cycle().take(18).enumerate().map(move |(i, c)| {
            let angle = -std::f32::consts::PI * (0.08 + 0.84 * i as f32 / 17.);
            let reach = 70. + 26. * ((i * 7) % 5) as f32;
            let (x, y) = (angle.cos() * reach * u, angle.sin() * reach * u + 60. * t * t);
            div().absolute().left(px(STAGE_W / 2. + x)).top(px(62. + y)).size(px(7.)).rounded(px(2.)).bg(shade(*c, fade))
        })
    });
    div()
        .relative()
        .w(px(STAGE_W))
        .h(px(STAGE_H))
        .opacity(shown)
        .child(div().absolute().left(px((STAGE_W - 150.) / 2.)).top(px(STAGE_H * 0.58 - 75.)).size(px(150.)).rounded_full().shadow(vec![ui::shadow(shade(GLOW, 0.4 * shown), 0., 60.)]))
        .child(div().absolute().bottom(px(14.)).left(px((STAGE_W - shadow_w) / 2.)).w(px(shadow_w)).h(px(6.)).rounded_full().shadow(vec![ui::shadow(shade(0x000000, 0.25 * near), 0., 6.)]))
        .child(layer("halo", shade(LIGHT, 0.55)))
        .child(layer("body", rgb(BODY).into()))
        .child(layer("sheen", shade(0xffffff, 0.08)))
        .child(layer("light", rgb(LIGHT).into()))
        .child(layer("eyes-happy", rgb(EYES).into()))
        .children(confetti.into_iter().flatten())
}

/// The mascot, cheering for the `cheers`th time: the first drops it in, each after hops.
pub(super) fn mascot(cheers: usize, still: bool) -> Stateful<Div> {
    let root = div().id("handoff-cheer").cursor_pointer();
    if still {
        return root.child(stage(pose(0., 1., 1.), 1., false, true));
    }
    let first = cheers == 0;
    let keys: &'static [(f32, Pose)] = if first { &DROP } else { &HOP };
    root.child(div().with_animation(SharedString::from(format!("handoff-cheer-{cheers}")), Animation::new(Duration::from_millis(1250)), move |d, t| {
        // The hop is quicker than the confetti it throws.
        let moved = if first { t / 0.88 } else { t / 0.48 };
        d.child(stage(at(keys, moved), t, first, false))
    }))
}

#[cfg(test)]
mod tests {
    use super::{DROP, HOP, at, pose};

    #[test]
    fn the_drop_lands_squashed_and_both_moves_end_standing() {
        assert_eq!(at(&DROP, 0.34), pose(0., 1.14, 0.8));
        assert_eq!(at(&DROP, 1.), pose(0., 1., 1.));
        assert_eq!(at(&HOP, 2.), pose(0., 1., 1.));
        assert!(at(&DROP, 0.17).y < 0., "it is still falling");
    }
}
