//! A task's ring on the capsule.

use super::progress::{Mark, fill};
use gpui_kit::*;
use std::f32::consts::TAU;
use std::time::Duration;
use theme::*;

const SIZE: f32 = 16.;
const RADIUS: f32 = 6.;
const STROKE: f32 = 2.;

/// (track, arc) on the dark capsule.
fn colors(m: Mark) -> (Hsla, Hsla) {
    let white = Hsla::from(WHITE);
    let tint = |c: u32| {
        let h = Hsla::from(Token::fixed(c));
        (h.opacity(0.35), h)
    };
    match m {
        Mark::NeedsYou => tint(0xffb224ff),
        Mark::Done => tint(0x3dd68cff),
        Mark::Merged => tint(0xa371f7ff),
        Mark::Failed => tint(0xff9592ff),
        Mark::Stopped => (white.opacity(0.18), white.opacity(0.35)),
        Mark::Waiting | Mark::Running => (white.opacity(0.18), white),
    }
}

/// The point `turn` of the way round, clockwise from the top.
fn on_circle(c: Point<Pixels>, turn: f32) -> Point<Pixels> {
    let a = turn * TAU;
    point(c.x + px(RADIUS * a.sin()), c.y - px(RADIUS * a.cos()))
}

/// Adds the arc from `from` to `to` turns in pieces of at most half a turn: an arc between the same two points is ambiguous past that.
fn arc(path: &mut PathBuilder, c: Point<Pixels>, from: f32, to: f32) {
    path.move_to(on_circle(c, from));
    let mut at = from;
    while to - at > 1e-4 {
        let next = (at + 0.5).min(to);
        path.arc_to(point(px(RADIUS), px(RADIUS)), px(0.), false, true, on_circle(c, next));
        at = next;
    }
}

fn paint(m: Mark, phase: f32, bounds: Bounds<Pixels>, window: &mut Window) {
    let c = bounds.center();
    let (track, value) = colors(m);
    let mut ring = PathBuilder::stroke(px(STROKE));
    if m == Mark::Waiting {
        ring = ring.dash_array(&[px(2.), px(2.4)]);
    }
    arc(&mut ring, c, 0., 1.);
    if let Ok(p) = ring.build() {
        window.paint_path(p, track);
    }
    if fill(m) > 0. {
        let mut v = PathBuilder::stroke(px(STROKE));
        arc(&mut v, c, phase, phase + fill(m));
        if let Ok(p) = v.build() {
            window.paint_path(p, value);
        }
    }
}

/// A task's ring. A running one turns at 20fps, sharing frames with the sidebar's spinners, and holds still under Reduce motion.
pub(super) fn ring(key: ElementId, m: Mark, reduce_motion: bool) -> AnyElement {
    let face = move |phase: f32| canvas(|_, _, _| (), move |bounds, _, window, _| paint(m, phase, bounds, window)).size(px(SIZE));
    if m != Mark::Running || reduce_motion {
        return div().size(px(SIZE)).flex_none().child(face(0.)).into_any_element();
    }
    div().size(px(SIZE)).flex_none().with_animation(key, Animation::new(Duration::from_secs(1)).repeat_synced().with_max_fps(20.), move |d, t| d.child(face(t))).into_any_element()
}
