//! A space's theme as colour: where a colour sits on the editor's pad, and what it paints.
//! The pad is Arc's: a colour's angle is its hue, clockwise from the right, and its distance from the centre its lightness.

use gpui_kit::{Hsla, hsla};
use std::f32::consts::PI;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dot {
    pub x: f32,
    pub y: f32,
    pub s: f32,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hls {
    pub h: f32,
    pub l: f32,
    pub s: f32,
}

const fn p(h: f32, l: f32, s: f32) -> Hls {
    Hls { h, l, s }
}

/// Lightness at the centre, and what the rim adds.
const CENTRE: f32 = 0.06;
const RIM: f32 = 0.9;
/// The pad colour a swatch row starts from when the space has none.
const FIRST: Hls = p(210., 0.62, 0.8);
/// Saturation a grey colour takes once it is moved or joined: without it a hue has nothing to show.
const GREY: f32 = 0.05;
const REVIVED: f32 = 0.8;

pub const PAGE_NAMES: [&str; 5] = ["Brand", "Brand pairs", "Pastel", "Drab", "Greyscale"];
const BRAND: [Hls; 9] = [
    p(40., 0.95, 0.4),
    p(330., 0.76, 0.75),
    p(285., 0.55, 0.35),
    p(355., 0.6, 0.8),
    p(22., 0.6, 0.92),
    p(46., 0.64, 0.9),
    p(150., 0.55, 0.7),
    p(200., 0.6, 0.6),
    p(240., 0.45, 0.3),
];
const HUES: [f32; 9] = [0., 28., 52., 95., 150., 180., 205., 240., 285.];
const GREYS: [f32; 9] = [0.97, 0.88, 0.78, 0.66, 0.55, 0.44, 0.33, 0.22, 0.1];
/// How far round a pair's second colour sits from its first.
const PAIR: f32 = 36.;

impl Dot {
    pub fn from_hls(c: Hls) -> Self {
        let r = ((c.l - CENTRE) / RIM).clamp(0., 1.);
        let a = c.h.to_radians();
        Self { x: r * a.cos(), y: r * a.sin(), s: c.s }
    }

    pub fn hls(self) -> Hls {
        let r = self.x.hypot(self.y).min(1.);
        Hls { h: self.y.atan2(self.x).to_degrees().rem_euclid(360.), l: CENTRE + RIM * r, s: self.s }
    }

    /// This colour moved to `(x, y)`, kept on the pad, with a grey one given a hue to show.
    pub fn moved(self, x: f32, y: f32) -> Self {
        let (x, y) = on_pad(x, y);
        Self { x, y, s: if self.s < GREY { REVIVED } else { self.s } }
    }
}

/// `(x, y)` pulled back inside the unit circle.
pub fn on_pad(x: f32, y: f32) -> (f32, f32) {
    let r = x.hypot(y);
    if r > 1. { (x / r, y / r) } else { (x, y) }
}

/// Where a colour dragged to `(x, y)` shows: past the rim it follows less the further it goes, as a rubber band.
pub fn stretched(x: f32, y: f32) -> (f32, f32) {
    let r = x.hypot(y);
    if r <= 1. {
        return (x, y);
    }
    const K: f32 = 0.55;
    let over = r - 1.;
    let shown = 1. + over * K / (1. + K * over);
    (x / r * shown, y / r * shown)
}

fn hsl(h: f32, s: f32, l: f32, a: f32) -> Hsla {
    hsla(h.rem_euclid(360.) / 360., s.clamp(0., 1.), l.clamp(0., 1.), a)
}

/// The lightness a colour of pad lightness `l` washes the window in: pale in light, deep in dark.
fn painted(l: f32, dark: bool) -> f32 {
    if dark { 0.01 + 0.415 * l } else { 0.575 + 0.395 * l }
}

/// What `c` washes the window in, at `intensity` 0 to 1.
pub fn wash(c: Dot, dark: bool, intensity: f32) -> Hsla {
    let Hls { h, l, s } = c.hls();
    hsl(h, s, painted(l, dark), 0.2 + 0.55 * intensity.clamp(0., 1.))
}

/// The fill of `c`'s handle on the pad: the pale colour it washes a light window in.
pub fn handle(c: Dot) -> Hsla {
    let Hls { h, l, s } = c.hls();
    hsl(h, s, painted(l, false), 1.)
}

/// The space's icon tile and switcher mark, as RGB.
pub fn tile(c: Dot) -> u32 {
    let Hls { h, l, s } = c.hls();
    rgb(hsl(h, s, l.clamp(0.36, 0.6), 1.))
}

/// The editor card: a dark material faintly of the lead colour's hue.
pub fn card(lead: Option<Dot>) -> Hsla {
    match lead {
        Some(c) => hsl(c.hls().h, 0.09, 0.25, 1.),
        None => hsl(240., 0.02, 0.23, 1.),
    }
}

fn rgb(c: Hsla) -> u32 {
    let c = c.to_rgb();
    let byte = |v: f32| (v.clamp(0., 1.) * 255.).round() as u32;
    byte(c.r) << 16 | byte(c.g) << 8 | byte(c.b)
}

/// The colour `+` adds: the lead's opposite as the second, 150° back from it as the third; a first one when there is none.
pub fn added(colours: &[Dot]) -> Dot {
    let Some(lead) = colours.first() else { return Dot::from_hls(FIRST) };
    let Hls { h, l, s } = lead.hls();
    let turn = if colours.len() == 1 { 180. } else { -150. };
    Dot::from_hls(p((h + turn).rem_euclid(360.), l, if s < GREY { REVIVED } else { s }))
}

pub fn page(page: usize) -> Vec<Vec<Hls>> {
    match page {
        0 => BRAND.iter().map(|c| vec![*c]).collect(),
        1 => BRAND.iter().map(|c| vec![*c, p((c.h + PAIR).rem_euclid(360.), c.l, c.s)]).collect(),
        2 => HUES.iter().map(|h| vec![p(*h, 0.86, 0.7)]).collect(),
        3 => HUES.iter().map(|h| vec![p(*h, 0.5, 0.2)]).collect(),
        _ => GREYS.iter().map(|l| vec![p(0., *l, 0.)]).collect(),
    }
}

pub fn swatch(c: Hls) -> Hsla {
    hsl(c.h, c.s, c.l, 1.)
}

pub fn is_swatch(swatch: &[Hls], colours: &[Dot]) -> bool {
    swatch.len() == colours.len()
        && swatch.iter().zip(colours).all(|(w, c)| {
            let d = Dot::from_hls(*w);
            (d.x - c.x).abs() < 0.02 && (d.y - c.y).abs() < 0.02 && (d.s - c.s).abs() < 0.02
        })
}

/// The pad point under a pointer `(dx, dy)` px from the pad's centre, on a pad whose rim is `reach` px out.
pub fn at(dx: f32, dy: f32, reach: f32) -> (f32, f32) {
    (dx / reach.max(1.), dy / reach.max(1.))
}

/// The grain dial turns from the bottom left to the bottom right, through the top: degrees clockwise from the top.
pub const DIAL_FROM: f32 = -143.;
pub const DIAL_SWEEP: f32 = 286.;

pub fn dial_angle(v: f32) -> f32 {
    DIAL_FROM + DIAL_SWEEP * v.clamp(0., 1.)
}

pub fn dial_value(angle: f32) -> f32 {
    ((angle - DIAL_FROM) / DIAL_SWEEP).clamp(0., 1.)
}

/// The turn, in degrees, from one pointer angle to the next, taken the short way round so crossing the bottom doesn't jump.
pub fn turn(from: f32, to: f32) -> f32 {
    (to - from + 540.).rem_euclid(360.) - 180.
}

/// A pointer's angle round a centre, in degrees clockwise from the top.
pub fn bearing(dx: f32, dy: f32) -> f32 {
    dx.atan2(-dy) * 180. / PI
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f32, b: f32) -> bool {
        (a - b).abs() < 1e-3
    }

    #[test]
    fn a_colour_keeps_its_hue_and_lightness_through_the_pad() {
        for c in [p(151., 0.66, 0.55), p(0., 0.96, 0.8), p(359., 0.3, 0.2)] {
            let back = Dot::from_hls(c).hls();
            assert!(close(back.h, c.h) && close(back.l, c.l), "{c:?} came back {back:?}");
        }
    }

    #[test]
    fn a_colour_past_the_rim_lies_on_it_and_shows_a_little_past_it() {
        assert_eq!(on_pad(2., 0.), (1., 0.));
        assert_eq!(on_pad(0.3, 0.4), (0.3, 0.4));
        let (x, _) = stretched(2., 0.);
        assert!(x > 1. && x < 1.4, "{x}");
        assert_eq!(stretched(0.5, 0.), (0.5, 0.));
    }

    #[test]
    fn a_light_window_is_washed_pale_and_a_dark_one_deep_at_the_set_strength() {
        let c = Dot::from_hls(p(150., 0.66, 0.6));
        let (light, dark) = (wash(c, false, 1.), wash(c, true, 0.));
        assert!(light.l > 0.8 && dark.l < 0.3);
        assert!(close(light.a, 0.75) && close(dark.a, 0.2));
    }

    #[test]
    fn plus_adds_the_opposite_then_one_150_degrees_back_and_revives_a_grey() {
        let lead = Dot::from_hls(p(40., 0.6, 0.7));
        assert!(close(added(&[lead]).hls().h, 220.));
        assert!(close(added(&[lead, lead]).hls().h, 250.));
        let grey = Dot::from_hls(p(0., 0.5, 0.));
        assert_eq!(added(&[grey]).s, REVIVED);
        assert_eq!(added(&[]), Dot::from_hls(FIRST));
    }

    #[test]
    fn a_grey_moved_takes_a_hue_and_a_colour_keeps_its_own() {
        assert_eq!(Dot { x: 0., y: 0., s: 0. }.moved(0.5, 0.).s, REVIVED);
        assert_eq!(Dot { x: 0., y: 0., s: 0.4 }.moved(3., 0.), Dot { x: 1., y: 0., s: 0.4 });
    }

    #[test]
    fn five_pages_of_nine_swatches_with_pairs_only_on_the_second() {
        for i in 0..PAGE_NAMES.len() {
            let swatches = page(i);
            assert_eq!(swatches.len(), 9);
            assert!(swatches.iter().all(|s| s.len() == if i == 1 { 2 } else { 1 }), "page {i}");
        }
    }

    #[test]
    fn a_swatch_matches_the_colours_it_set_and_nothing_else() {
        let swatch = page(1).remove(3);
        let colours: Vec<Dot> = swatch.iter().map(|c| Dot::from_hls(*c)).collect();
        assert!(is_swatch(&swatch, &colours));
        assert!(!is_swatch(&swatch, &colours[..1]));
        assert!(!is_swatch(&page(0)[3], &colours[..1].iter().map(|c| Dot { s: 0.1, ..*c }).collect::<Vec<_>>()));
    }

    #[test]
    fn the_dial_turns_through_the_top_and_stops_at_its_ends() {
        assert!(close(dial_value(dial_angle(0.37)), 0.37));
        assert_eq!(dial_value(170.), 1.);
        assert_eq!(dial_value(-170.), 0.);
        assert_eq!((dial_angle(-1.), dial_angle(2.)), (DIAL_FROM, DIAL_FROM + DIAL_SWEEP));
        assert!(close(turn(170., -170.), 20.));
        assert!(close(bearing(0., -1.), 0.) && close(bearing(1., 0.), 90.));
    }

    #[test]
    fn the_tile_takes_the_lead_hue_with_its_lightness_kept_off_the_ends() {
        let lightness = |rgb: u32| {
            let ch = [rgb >> 16 & 0xff, rgb >> 8 & 0xff, rgb & 0xff];
            (ch.iter().max().unwrap() + ch.iter().min().unwrap()) as f32 / 510.
        };
        assert!(lightness(tile(Dot::from_hls(p(150., 0.95, 0.7)))) <= 0.605);
        assert!(lightness(tile(Dot::from_hls(p(150., 0.1, 0.7)))) >= 0.355);
    }
}
