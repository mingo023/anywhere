mod view;

use crate::glide::spring;
use gpui_kit::{SpringConfig, SpringState};
use std::time::{Duration, Instant};

/// Pixels per second², pulling the mascot to its floor.
const GRAVITY: f32 = 2600.;
/// Where it falls from, above its floor, and higher in a window tall enough to show it there (`above`).
const DROP: f32 = -340.;
/// The share of its landing speed it bounces back up with, until it lands slower than `STILL_LANDING`.
const REBOUND: f32 = 0.3;
const STILL_LANDING: f32 = 600.;
/// Its speed up as it hops out, and how high it is gone; both grow with the window (`above`).
const HOP: f32 = -1700.;
const GONE: f32 = -420.;
/// From where it is laid out (its eyes) to below its feet: how far past the window's top it must be to be out of sight.
const OVER_TOP: f32 = 100.;
/// Frames this close together, this many in a row, mean the app's first work is done and the drop will run smoothly; it drops after `CALM_CAP` regardless.
const CALM_FRAME: f32 = 0.02;
const CALM_FRAMES: u8 = 6;
const CALM_CAP: Duration = Duration::from_millis(1200);
/// From landing to the words coming in.
const SETTLE: Duration = Duration::from_millis(200);
const REVEAL: Duration = Duration::from_millis(420);
const STAGGER: Duration = Duration::from_millis(70);
/// The crouch before the hop, and the words going as it starts.
const CROUCH: Duration = Duration::from_millis(130);
const COPY_OUT: Duration = Duration::from_millis(160);
/// The screen fading onto the app once the mascot is gone.
const FADE: Duration = Duration::from_millis(280);
const GLOW_IN: Duration = Duration::from_millis(600);
const FLASH: Duration = Duration::from_millis(420);
const BLINK: Duration = Duration::from_millis(170);
const FIRST_BLINK: Duration = Duration::from_millis(320);
/// Gaps between blinks, in turn, so they don't tick like a clock.
const BLINK_GAPS: [u64; 5] = [3100, 4400, 2700, 5000, 3600];
/// How far the eyes look toward the pointer, in pixels at the mascot's size, and from how far away they look all the way.
const EYE_X: f32 = 6.4;
const EYE_Y: f32 = 4.;
const REACH: f32 = 220.;
/// The speed that stretches it by a whole: it stretches a tenth at 300 px/s.
const STRETCH: f32 = 3000.;
/// The height its shadow has shrunk to nothing at.
const SHADOW_FADE: f32 = 260.;
/// Landing speed per unit of squash it kicks.
const IMPACT: f32 = 500.;

/// The bounce of its squash after it lands or is let go of.
fn landing() -> SpringConfig {
    spring(0.45, 0.38)
}

fn rest(at: f32) -> SpringState {
    SpringState { position: at, velocity: 0. }
}

fn ease_out(t: f32) -> f32 {
    1. - (1. - t.clamp(0., 1.)).powi(3)
}

fn progress(since: Duration, of: Duration) -> f32 {
    since.as_secs_f32() / of.as_secs_f32()
}

/// Whether this launch is the install's first: nothing saved a launch yet. An install older than the star's `first_launch` still has the version it last saw.
pub(crate) fn greets(store: &store::Store) -> bool {
    store.first_launch.is_none() && store.seen_version.is_none()
}

#[derive(Clone, Copy, Debug, PartialEq)]
enum Phase {
    /// Out of sight above, until frames come smoothly.
    Waiting { since: Option<Instant>, calm: u8 },
    Falling { bounced: bool },
    Landed(Instant),
    /// Words shown and Get started offered, since this instant.
    Ready(Instant),
    Crouching(Instant),
    Hopping,
    Fading(Instant),
    Done,
}

/// What the screen needs next: a frame, a wake at a time (the next blink), or nothing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Wake {
    Frame,
    At(Instant),
    Idle,
}

/// How the welcome draws at a moment: plain numbers for its view.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Pose {
    /// Above its floor, in pixels.
    pub y: f32,
    pub sx: f32,
    pub sy: f32,
    pub lean: f32,
    pub eyes: (f32, f32),
    /// The eyes' height while blinking, 1 open.
    pub lid: f32,
    pub lit: bool,
    pub glow: f32,
    pub halo: f32,
    /// Each line of words: title, line, button.
    pub copy: [f32; 3],
    /// Its shadow: 1 on the floor, 0 far above.
    pub near: f32,
    pub opacity: f32,
    /// Not drawn while it waits to drop.
    pub hidden: bool,
}

/// The first-launch screen: the mascot drops in, watches the pointer until dismissed, hops out, then fades onto the app.
pub(crate) struct Welcome {
    phase: Phase,
    y: f32,
    vy: f32,
    squash: SpringState,
    squash_to: f32,
    squash_spring: SpringConfig,
    eyes: [SpringState; 2],
    look: [f32; 2],
    lit: Option<Instant>,
    flash: Option<Instant>,
    blink: Option<Instant>,
    next_blink: Option<Instant>,
    blinks: usize,
    left: Option<Instant>,
    pressed: bool,
    stepped: Option<Instant>,
    /// Where the mascot stands on screen, from its last layout: what the eyes look from.
    pub(crate) center: Option<(f32, f32)>,
    wake: Option<(Instant, gpui_kit::Task<()>)>,
}

impl Welcome {
    pub(crate) fn new() -> Self {
        Self {
            phase: Phase::Waiting { since: None, calm: 0 },
            y: DROP,
            vy: 0.,
            squash: rest(1.),
            squash_to: 1.,
            squash_spring: landing(),
            eyes: [rest(0.); 2],
            look: [0.; 2],
            lit: None,
            flash: None,
            blink: None,
            next_blink: None,
            blinks: 0,
            left: None,
            pressed: false,
            stepped: None,
            center: None,
            wake: None,
        }
    }

    /// Whether it hides the app, which shows through only once it fades.
    pub(crate) fn covers(&self) -> bool {
        !matches!(self.phase, Phase::Fading(_) | Phase::Done)
    }

    pub(crate) fn done(&self) -> bool {
        self.phase == Phase::Done
    }

    /// Steps the motion to `now`; `still` (reduced motion) lands everything where it ends.
    pub(crate) fn tick(&mut self, now: Instant, still: bool) -> Wake {
        let gap = self.stepped.map(|t| now.duration_since(t).as_secs_f32());
        // Capped, so a slow frame doesn't skip the motion.
        let dt = gap.map_or(0., |g| g.min(1. / 30.));
        self.stepped = Some(now);
        if still {
            self.hold_still(now);
        }
        let mut thrown = false;
        match self.phase {
            Phase::Waiting { since, calm } => {
                let since = since.unwrap_or(now);
                let calm = if gap.is_some_and(|g| g < CALM_FRAME) { calm + 1 } else { 0 };
                if calm >= CALM_FRAMES || now >= since + CALM_CAP {
                    (self.y, self.phase) = (self.above(DROP), Phase::Falling { bounced: false });
                } else {
                    self.phase = Phase::Waiting { since: Some(since), calm };
                }
            }
            Phase::Falling { bounced } => {
                self.vy += GRAVITY * dt;
                self.y += self.vy * dt;
                thrown = !bounced;
                if thrown {
                    self.squash = rest(1. + (self.vy / STRETCH).min(0.12));
                }
                if self.y >= 0. {
                    self.land(now);
                    thrown = false;
                }
            }
            Phase::Landed(at) if now >= at + SETTLE => self.phase = Phase::Ready(now),
            Phase::Crouching(at) if now >= at + CROUCH => {
                // Fast enough to rise past where it is gone, with some to spare.
                (self.vy, self.phase) = (HOP.min(-(2.4 * GRAVITY * -self.above(GONE)).sqrt()), Phase::Hopping);
            }
            Phase::Hopping => {
                self.vy += GRAVITY * dt;
                self.y += self.vy * dt;
                self.squash = rest(1. + (self.vy.abs() / STRETCH).min(0.15));
                thrown = true;
                if self.y < self.above(GONE) {
                    self.phase = Phase::Fading(now);
                }
            }
            Phase::Fading(at) if now >= at + FADE => self.phase = Phase::Done,
            _ => {}
        }
        let squashing = !thrown && self.step_squash(dt);
        let looking = self.step_eyes(dt, still);
        let blinking = !still && self.step_blink(now);
        let moving = match self.phase {
            Phase::Ready(since) => squashing || looking || blinking || now < since + STAGGER * 2 + REVEAL || self.flash.is_some_and(|f| now < f + FLASH),
            Phase::Done => false,
            _ => true,
        };
        match (moving, self.next_blink) {
            (true, _) => Wake::Frame,
            (false, Some(next)) if !still => Wake::At(next),
            _ => Wake::Idle,
        }
    }

    /// `at_least` up, or past the window's top when that is higher.
    fn above(&self, at_least: f32) -> f32 {
        self.center.map_or(at_least, |(_, y)| at_least.min(-(y + OVER_TOP)))
    }

    fn land(&mut self, now: Instant) {
        self.y = 0.;
        self.squash_toward(1., landing());
        // The fall's stretch gives way to a squash as hard as it hit.
        self.squash = SpringState { position: 1., velocity: -self.vy / IMPACT };
        self.lit.get_or_insert(now);
        self.flash = Some(now);
        if self.vy > STILL_LANDING {
            (self.vy, self.phase) = (-self.vy * REBOUND, Phase::Falling { bounced: true });
        } else {
            (self.vy, self.phase) = (0., Phase::Landed(now));
        }
    }

    fn squash_toward(&mut self, to: f32, spring: SpringConfig) {
        (self.squash_to, self.squash_spring) = (to, spring);
    }

    fn step_squash(&mut self, dt: f32) -> bool {
        self.squash = self.squash_spring.step(self.squash, self.squash_to, dt);
        let moving = !self.squash_spring.is_settled(self.squash, self.squash_to, 0.001);
        if !moving {
            self.squash = rest(self.squash_to);
        }
        moving
    }

    fn step_eyes(&mut self, dt: f32, still: bool) -> bool {
        let follow = spring(1., 0.28);
        let mut moving = false;
        for (eye, to) in self.eyes.iter_mut().zip(self.look) {
            *eye = if still { rest(to) } else { follow.step(*eye, to, dt) };
            if follow.is_settled(*eye, to, 0.02) {
                *eye = rest(to);
            } else {
                moving = true;
            }
        }
        moving
    }

    fn step_blink(&mut self, now: Instant) -> bool {
        let Phase::Ready(since) = self.phase else { return false };
        let next = *self.next_blink.get_or_insert(since + FIRST_BLINK);
        if now >= next {
            self.blink = Some(now);
            self.next_blink = Some(now + Duration::from_millis(BLINK_GAPS[self.blinks % BLINK_GAPS.len()]));
            self.blinks += 1;
        }
        self.blink = self.blink.filter(|b| now < *b + BLINK);
        self.blink.is_some()
    }

    /// Reduced motion: the mascot stands on its floor, lit, with the words in; leaving goes straight to the app.
    fn hold_still(&mut self, now: Instant) {
        match self.phase {
            Phase::Waiting { .. } | Phase::Falling { .. } | Phase::Landed(_) => self.settle(now),
            Phase::Crouching(_) | Phase::Hopping | Phase::Fading(_) => self.phase = Phase::Done,
            Phase::Ready(_) | Phase::Done => {}
        }
        (self.squash, self.blink, self.flash) = (rest(self.squash_to), None, None);
    }

    /// Lands it at once with the words in: a click or Return before it has.
    fn settle(&mut self, now: Instant) {
        (self.y, self.vy, self.squash, self.squash_to) = (0., 0., rest(1.), 1.);
        self.lit.get_or_insert(now);
        self.phase = Phase::Ready(now.checked_sub(STAGGER * 2 + REVEAL).unwrap_or(now));
    }

    /// A click on the screen: lands it if it hasn't yet.
    pub(crate) fn skip(&mut self, now: Instant) {
        if matches!(self.phase, Phase::Waiting { .. } | Phase::Falling { .. } | Phase::Landed(_)) {
            self.settle(now);
        }
    }

    /// Get started or Return: lands it first if it hasn't, else it crouches and hops out.
    pub(crate) fn go(&mut self, now: Instant) {
        match self.phase {
            Phase::Waiting { .. } | Phase::Falling { .. } | Phase::Landed(_) => self.settle(now),
            Phase::Ready(_) => {
                (self.phase, self.left, self.pressed) = (Phase::Crouching(now), Some(now), false);
                self.squash_toward(0.8, spring(1., 0.14));
            }
            _ => {}
        }
    }

    /// A press on the mascot squishes it and flashes its light; letting go springs it back.
    pub(crate) fn press(&mut self, now: Instant) {
        if matches!(self.phase, Phase::Ready(_)) {
            (self.pressed, self.flash) = (true, Some(now));
            self.squash_toward(0.86, spring(1., 0.15));
        }
    }

    pub(crate) fn release(&mut self) {
        if std::mem::take(&mut self.pressed) && matches!(self.phase, Phase::Ready(_)) {
            self.squash_toward(1., spring(0.4, 0.4));
        }
    }

    /// The eyes turn toward the pointer at `(x, y)`, further the further away it is, up to `REACH`.
    pub(crate) fn look_at(&mut self, x: f32, y: f32) {
        let Some((cx, cy)) = self.center else { return };
        let (dx, dy) = (x - cx, y - cy);
        let d = dx.hypot(dy);
        if d < 1. {
            self.look = [0.; 2];
            return;
        }
        let k = (d / REACH).min(1.);
        self.look = [dx / d * k * EYE_X, dy / d * k * EYE_Y];
    }

    pub(crate) fn look_ahead(&mut self) {
        self.look = [0.; 2];
    }

    pub(crate) fn pose(&self, now: Instant, still: bool) -> Pose {
        let since = |at: Instant| now.saturating_duration_since(at);
        let copy = match self.phase {
            Phase::Ready(_) if still => [1.; 3],
            Phase::Ready(at) => std::array::from_fn(|i| ease_out(progress(since(at).saturating_sub(STAGGER * i as u32), REVEAL))),
            Phase::Crouching(_) | Phase::Hopping | Phase::Fading(_) => [1. - progress(self.left.map_or(COPY_OUT, since), COPY_OUT).min(1.); 3],
            _ => [0.; 3],
        };
        let opacity = match self.phase {
            Phase::Fading(at) => 1. - ease_out(progress(since(at), FADE)),
            Phase::Done => 0.,
            _ => 1.,
        };
        let sy = self.squash.position;
        let lid = self.blink.map_or(1., |b| 1. - 0.92 * (1. - (2. * progress(since(b), BLINK) - 1.).abs()).max(0.));
        let flash = self.flash.map_or(0., |f| 1. - ease_out(progress(since(f), FLASH)));
        Pose {
            y: self.y,
            sx: 1. + (1. - sy) * 0.8,
            sy,
            lean: (self.eyes[0].position * 0.5).to_radians(),
            eyes: (self.eyes[0].position, self.eyes[1].position),
            lid,
            lit: self.lit.is_some(),
            glow: self.lit.map_or(0., |l| if still { 1. } else { ease_out(progress(since(l), GLOW_IN)) }),
            halo: if self.lit.is_some() { 0.35 + 0.55 * flash } else { 0. },
            copy,
            near: (1. + self.y / SHADOW_FADE).clamp(0., 1.),
            opacity,
            hidden: matches!(self.phase, Phase::Waiting { .. }),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{DROP, OVER_TOP, Phase, Wake, Welcome, greets};
    use std::time::{Duration, Instant};
    use store::Store;

    /// Steps it a frame (8ms) at a time for `ms`, from `at`; returns the time it got to.
    fn run(w: &mut Welcome, at: Instant, ms: u64) -> Instant {
        (0..=ms / 8).map(|i| at + Duration::from_millis(8 * i)).inspect(|&t| _ = w.tick(t, false)).last().unwrap()
    }

    #[test]
    fn only_an_install_that_never_launched_is_greeted() {
        assert!(greets(&Store::default()));
        let mut launched = Store::default();
        launched.first_launch = Some(1_700_000_000);
        assert!(!greets(&launched));
        let mut from_before_the_star = Store::default();
        from_before_the_star.seen_version = Some("0.1.40".into());
        assert!(!greets(&from_before_the_star));
    }

    #[test]
    fn the_mascot_falls_bounces_once_and_comes_to_rest_lit_on_its_floor() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        assert_eq!(w.pose(t0, false).y, DROP);
        let mut lowest_bounce = 0f32;
        let mut squashed = false;
        let t = (0..=200u64).map(|i| t0 + Duration::from_millis(8 * i)).find(|&t| {
            w.tick(t, false);
            if matches!(w.phase, Phase::Falling { bounced: true }) {
                lowest_bounce = lowest_bounce.min(w.y);
            }
            squashed |= w.pose(t, false).sy < 0.95;
            matches!(w.phase, Phase::Ready(_))
        });
        let t = t.expect("it lands and settles within 1.6s");
        assert!(lowest_bounce < -10., "it bounced, to {lowest_bounce}");
        assert!(squashed, "landing squashes it");
        let pose = w.pose(t, false);
        assert_eq!((pose.y, pose.lit), (0., true));
    }

    #[test]
    fn it_waits_out_of_sight_while_frames_stall_and_drops_once_they_come_smoothly() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        // Eight frames, each a stall of 60ms: more than enough frames, none of them smooth.
        (0..8u64).for_each(|i| _ = w.tick(t0 + Duration::from_millis(60 * i), false));
        assert!(matches!(w.phase, Phase::Waiting { .. }), "{:?}", w.phase);
        assert_eq!(w.pose(t0, false).y, DROP);
        run(&mut w, t0 + Duration::from_millis(420), 48);
        assert!(matches!(w.phase, Phase::Falling { .. }), "{:?}", w.phase);
    }

    #[test]
    fn it_drops_anyway_when_frames_never_come_smoothly() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        // Too few frames to be calm, but past the cap.
        [0u64, 300, 600, 900, 1250].iter().for_each(|&ms| _ = w.tick(t0 + Duration::from_millis(ms), false));
        assert!(matches!(w.phase, Phase::Falling { .. }), "{:?}", w.phase);
    }

    #[test]
    fn in_a_tall_window_it_waits_unseen_drops_from_past_the_top_and_hops_back_out_past_it() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        w.center = Some((500., 700.));
        let top = -(700. + OVER_TOP);
        assert!(w.pose(t0, false).hidden);
        let t = (0..50u64).map(|i| t0 + Duration::from_millis(8 * i)).find(|&t| {
            w.tick(t, false);
            matches!(w.phase, Phase::Falling { .. })
        });
        let t = t.expect("it drops");
        let pose = w.pose(t, false);
        assert!(!pose.hidden && pose.y <= top, "drops from out of sight: {}", pose.y);
        let t = run(&mut w, t, 1500);
        w.go(t);
        let mut highest = 0f32;
        let t = (0..200u64).map(|i| t + Duration::from_millis(8 * i)).find(|&t| {
            w.tick(t, false);
            highest = highest.min(w.y);
            !w.covers()
        });
        assert!(t.is_some(), "it leaves");
        assert!(highest < top, "out of sight before the screen fades: {highest}");
    }

    #[test]
    fn the_words_come_in_one_after_another_once_it_has_landed() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        let t = run(&mut w, t0, 1200);
        let Phase::Ready(since) = w.phase else { panic!("not ready: {:?}", w.phase) };
        let early = w.pose(since + Duration::from_millis(100), false).copy;
        assert!(early[0] > early[1] && early[1] > early[2], "{early:?}");
        assert_eq!(w.pose(t + Duration::from_secs(1), false).copy, [1.; 3]);
    }

    #[test]
    fn the_eyes_turn_toward_the_pointer_and_back_when_it_leaves() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        w.center = Some((500., 300.));
        let t = run(&mut w, t0, 1200);
        w.look_at(900., 300.);
        let t = run(&mut w, t, 600);
        let (x, y) = w.pose(t, false).eyes;
        assert!((x - 6.4).abs() < 0.05 && y.abs() < 0.05, "looks right: {x},{y}");
        assert!(w.pose(t, false).lean > 0., "and leans that way");
        w.look_at(500., 200.);
        let t = run(&mut w, t, 600);
        let (x, y) = w.pose(t, false).eyes;
        assert!(x.abs() < 0.05 && y < -1.5 && y > -4., "a bit up, closer by: {x},{y}");
        w.look_ahead();
        let t = run(&mut w, t, 600);
        assert_eq!(w.pose(t, false).eyes, (0., 0.));
    }

    #[test]
    fn the_eyes_look_nowhere_until_the_mascot_has_been_laid_out() {
        let mut w = Welcome::new();
        w.look_at(900., 300.);
        assert_eq!(w.look, [0.; 2]);
    }

    #[test]
    fn at_rest_it_sleeps_until_the_next_blink_then_blinks() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        let t = run(&mut w, t0, 3000);
        let Wake::At(next) = w.tick(t, false) else { panic!("waits for a blink") };
        assert!(next > t);
        assert_eq!(w.tick(next, false), Wake::Frame);
        assert!(w.pose(next + Duration::from_millis(85), false).lid < 0.2, "shut halfway through");
        let after = run(&mut w, next, 300);
        assert_eq!(w.pose(after, false).lid, 1.);
    }

    #[test]
    fn get_started_crouches_hops_it_out_and_fades_onto_the_app() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        let t = run(&mut w, t0, 1200);
        w.go(t);
        let crouched = run(&mut w, t, 120);
        assert!(w.pose(crouched, false).sy < 0.9, "crouches first");
        assert!(w.covers());
        let t = run(&mut w, crouched, 1200);
        assert!(w.done(), "{:?}", w.phase);
        assert_eq!(w.pose(t, false).opacity, 0.);
    }

    #[test]
    fn return_before_it_lands_lands_it_with_the_words_in_and_the_next_one_leaves() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        let t = run(&mut w, t0, 80);
        w.go(t);
        let pose = w.pose(t, false);
        assert_eq!((pose.y, pose.copy, pose.lit), (0., [1.; 3], true));
        w.go(t);
        assert!(matches!(w.phase, Phase::Crouching(_)));
    }

    #[test]
    fn a_click_lands_it_but_never_takes_the_way_in() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        w.skip(t0);
        assert!(matches!(w.phase, Phase::Ready(_)));
        w.skip(t0);
        assert!(matches!(w.phase, Phase::Ready(_)));
    }

    #[test]
    fn a_press_squishes_it_and_letting_go_springs_it_back() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        let t = run(&mut w, t0, 1200);
        w.press(t);
        let t = run(&mut w, t, 300);
        assert!((w.pose(t, false).sy - 0.86).abs() < 0.01);
        w.release();
        let t = run(&mut w, t, 1500);
        assert_eq!(w.pose(t, false).sy, 1.);
    }

    #[test]
    fn a_press_before_it_has_landed_does_nothing() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        w.press(t0);
        assert!(!w.pressed);
    }

    #[test]
    fn reduced_motion_stands_it_lit_with_the_words_in_and_leaving_goes_straight_to_the_app() {
        let (mut w, t0) = (Welcome::new(), Instant::now());
        assert_eq!(w.tick(t0, true), Wake::Idle);
        let pose = w.pose(t0, true);
        assert_eq!((pose.y, pose.sy, pose.copy, pose.lit, pose.glow), (0., 1., [1.; 3], true, 1.));
        w.center = Some((0., 0.));
        w.look_at(500., 0.);
        w.tick(t0, true);
        assert_eq!(w.pose(t0, true).eyes, (6.4, 0.), "the eyes still follow, without gliding");
        w.go(t0);
        w.tick(t0, true);
        assert!(w.done());
    }
}
