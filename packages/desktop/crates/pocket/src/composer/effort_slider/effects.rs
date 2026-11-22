use gpui_kit::ease_out_quint;
use std::time::Instant;

/// How long each of the five bursts runs, low to max.
const RUN_MS: [f32; 5] = [400., 680., 850., 950., 1150.];
/// How long a burst cut short by the next one takes to fade, instead of the two stacking.
const FADE_MS: f32 = 160.;

/// One level's burst of light: which of the five it is, when it started and, once another took over, when that was.
#[derive(Clone, Copy)]
struct Burst {
    kind: usize,
    at: Instant,
    cut: Option<Instant>,
}

/// The light a level shows as the thumb crosses into it. At most two at once: the newest, and the one it cut short.
#[derive(Default)]
pub(crate) struct EffortAnimation {
    live: Option<Burst>,
    fading: Option<Burst>,
}

/// What a burst draws on one frame, each part as (extent, alpha 0 to 1). A ring's extent is how far past the thumb's
/// edge it has spread in points, a sweep's how far across the fill it has got (0 to 1), a bloom's how far back from the
/// thumb it reaches in points, and the sparks' how far through their flight they are (0 to 1).
#[derive(Clone, Copy, Default, PartialEq, Debug)]
pub(crate) struct Light {
    pub(crate) ring: Option<(f32, f32)>,
    pub(crate) sweeps: [Option<(f32, f32)>; 3],
    pub(crate) bloom: Option<(f32, f32)>,
    pub(crate) sparks: Option<(f32, f32)>,
}

impl EffortAnimation {
    /// Starts the burst for a level `intensity` (0 to 1) up the scale, fading out whichever was running.
    pub(crate) fn start(&mut self, intensity: f32, now: Instant) {
        self.fading = self.live.take().map(|b| Burst { cut: Some(now), ..b });
        let kind = (intensity.clamp(0., 1.) * (RUN_MS.len() - 1) as f32).round() as usize;
        self.live = Some(Burst { kind, at: now, cut: None });
    }

    pub(crate) fn clear(&mut self) {
        *self = Self::default();
    }

    /// What the bursts draw at `now`, dropping those that have finished.
    pub(crate) fn sample(&mut self, now: Instant) -> [Option<Light>; 2] {
        let draw = |slot: &mut Option<Burst>| {
            let b = (*slot)?;
            let ms = now.saturating_duration_since(b.at).as_secs_f32() * 1000.;
            let fade = b.cut.map_or(1., |cut| 1. - now.saturating_duration_since(cut).as_secs_f32() * 1000. / FADE_MS);
            if ms >= RUN_MS[b.kind] || fade <= 0. {
                *slot = None;
                return None;
            }
            Some(light(b.kind, ms, fade))
        };
        [draw(&mut self.fading), draw(&mut self.live)]
    }

    pub(crate) fn busy(&self) -> bool {
        self.live.is_some() || self.fading.is_some()
    }
}

/// Burst `kind`, `ms` in, scaled by `fade`.
fn light(kind: usize, ms: f32, fade: f32) -> Light {
    let along = |delay: f32, run: f32| ((ms - delay) / run).clamp(0., 1.);
    let swell = |t: f32, alpha: f32, reach: f32| (t > 0. && t < 1.).then(|| (reach * ease_out_quint()(t), alpha * envelope(t) * fade));
    let sweep = |delay: f32, run: f32, alpha: f32| {
        let t = along(delay, run);
        (t > 0. && t < 1.).then(|| (ease_in_out(t), alpha * envelope(t) * fade))
    };
    match kind {
        0 => Light { ring: swell(along(0., 400.), 0.45, 7.), bloom: swell(along(0., 400.), 0.28, 22.), ..Light::default() },
        1 => Light { sweeps: [sweep(0., 680., 0.38), None, None], ..Light::default() },
        2 => Light { bloom: swell(along(0., 850.), 0.42, 70.), sweeps: [sweep(120., 730., 0.22), None, None], ..Light::default() },
        3 => Light {
            ring: swell(along(0., 700.), 0.3, 9.),
            bloom: swell(along(0., 600.), 0.18, 26.),
            sweeps: [sweep(0., 820., 0.32), sweep(130., 820., 0.26), None],
            ..Light::default()
        },
        _ => Light {
            ring: swell(along(60., 900.), 0.38, 12.),
            bloom: swell(along(0., 900.), 0.5, 88.),
            sweeps: [sweep(0., 830., 0.34), sweep(110., 830., 0.28), sweep(220., 830., 0.22)],
            sparks: swell(along(100., 1050.), 0.85, 1.),
        },
    }
}

/// 0 to 1 and back, rising fast and dying away slowly, so no part pops in or out.
fn envelope(t: f32) -> f32 {
    const RISE: f32 = 0.18;
    const FALL_FROM: f32 = 0.4;
    smooth(t / RISE) * (1. - smooth((t - FALL_FROM) / (1. - FALL_FROM)))
}

fn smooth(x: f32) -> f32 {
    let x = x.clamp(0., 1.);
    x * x * (3. - 2. * x)
}

fn ease_in_out(t: f32) -> f32 {
    if t < 0.5 { 4. * t * t * t } else { 1. - (-2. * t + 2.).powi(3) / 2. }
}

#[cfg(test)]
mod tests {
    use super::{EffortAnimation, RUN_MS};
    use std::time::{Duration, Instant};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    #[test]
    fn a_burst_runs_its_level_s_length_then_leaves_nothing_to_draw() {
        let (mut fx, start) = (EffortAnimation::default(), Instant::now());
        fx.start(1., start);
        assert!(fx.sample(start + ms(300))[1].is_some_and(|l| l.sparks.is_some() && l.sweeps.iter().all(Option::is_some)));
        assert_eq!(fx.sample(start + ms(RUN_MS[4] as u64)), [None, None]);
        assert!(!fx.busy());
    }

    #[test]
    fn a_new_burst_fades_the_one_it_cuts_short_instead_of_stacking() {
        let (mut fx, start) = (EffortAnimation::default(), Instant::now());
        fx.start(0.25, start);
        fx.start(0.5, start + ms(100));
        fx.start(0.75, start + ms(150));
        let [fading, live] = fx.sample(start + ms(200));
        assert!(live.is_some_and(|l| l.ring.is_some()), "the newest is extra high's");
        let swept = fading.and_then(|l| l.bloom).map(|b| b.1);
        assert!(swept.is_some_and(|a| a > 0.), "high's is fading, medium's is gone");
        assert!(fx.sample(start + ms(150 + 160))[0].is_none());
    }

    #[test]
    fn levels_draw_more_the_higher_they_are() {
        let (start, at) = (Instant::now(), ms(250));
        let parts = |kind| {
            let mut fx = EffortAnimation::default();
            fx.start(kind as f32 / 4., start);
            let l = fx.sample(start + at)[1].unwrap();
            l.ring.is_some() as usize + l.sweeps.iter().flatten().count() + l.bloom.is_some() as usize + l.sparks.is_some() as usize
        };
        assert_eq!((0..5).map(parts).collect::<Vec<_>>(), [2, 1, 2, 4, 6]);
    }
}
