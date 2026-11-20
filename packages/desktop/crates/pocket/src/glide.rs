use gpui_kit::{SpringConfig, SpringState};
use std::f32::consts::TAU;
use std::time::Instant;
use theme::SPRING;

const SETTLED: f32 = 0.001;

/// A unit-mass spring by its damping ratio (1 never overshoots) and the seconds it takes to arrive: a design prototype's spring.
pub(crate) fn spring(damping: f32, response: f32) -> SpringConfig {
    let omega = TAU / response;
    SpringConfig::new(omega * omega, 2. * damping * omega, 1.)
}

/// A value springing to where it is sent, stepped once a frame; it starts where it is first sent, unless made `from` somewhere.
#[derive(Default)]
pub(crate) struct Glide {
    spring: Option<SpringState>,
    stepped: Option<Instant>,
    config: Option<SpringConfig>,
}

impl Glide {
    pub(crate) fn from(at: f32) -> Self {
        Self { spring: Some(SpringState { position: at, velocity: 0. }), stepped: None, config: None }
    }

    /// Takes about `seconds` to arrive instead of the app's usual spring.
    pub(crate) fn response(self, seconds: f32) -> Self {
        Self { config: Some(spring(1., seconds)), ..self }
    }

    /// Steps toward `to` by the time since the last step; true while it still moves.
    pub(crate) fn tick(&mut self, to: f32, now: Instant, reduce_motion: bool) -> bool {
        let rest = SpringState { position: to, velocity: 0. };
        let was = self.spring.filter(|_| !reduce_motion).unwrap_or(rest);
        // Capped, so a slow frame doesn't skip the motion.
        let dt = self.stepped.map_or(0., |t| now.duration_since(t).as_secs_f32().min(0.064));
        let spring = self.config.unwrap_or(SPRING);
        let next = spring.step(was, to, dt);
        let moving = !spring.is_settled(next, to, SETTLED);
        self.spring = Some(if moving { next } else { rest });
        self.stepped = moving.then_some(now);
        moving
    }

    pub(crate) fn at(&self) -> f32 {
        self.spring.map_or(0., |s| s.position)
    }
}

#[cfg(test)]
mod tests {
    use super::Glide;
    use std::time::{Duration, Instant};

    fn run(glide: &mut Glide, to: f32, start: Instant, frames: u32) -> bool {
        (0..=frames).map(|i| glide.tick(to, start + Duration::from_millis(8) * i, false)).last().unwrap()
    }

    #[test]
    fn a_glide_starts_where_it_is_first_sent_and_springs_to_where_it_is_sent_next() {
        let (mut glide, start) = (Glide::default(), Instant::now());
        assert!(!glide.tick(1., start, false));
        assert_eq!(glide.at(), 1.);
        assert!(run(&mut glide, 0., start, 5));
        assert!(glide.at() > 0. && glide.at() < 1.);
        assert!(!run(&mut glide, 0., start + Duration::from_millis(40), 40));
        assert_eq!(glide.at(), 0.);
    }

    #[test]
    fn a_glide_sent_back_midway_turns_round_from_where_it_is() {
        let (mut glide, start) = (Glide::from(0.), Instant::now());
        run(&mut glide, 1., start, 5);
        let midway = glide.at();
        glide.tick(0., start + Duration::from_millis(48), false);
        assert!(glide.at() > midway * 0.5 && glide.at() < 1.);
    }

    #[test]
    fn a_glide_with_a_response_is_nearly_there_after_that_long_and_not_a_quarter_of_the_way_in() {
        // Frames are capped at 64 ms, so each is stepped in turn.
        let run = |ms: u64| {
            let (mut glide, start) = (Glide::from(0.).response(0.38), Instant::now());
            (0..=ms / 8).for_each(|i| _ = glide.tick(1., start + Duration::from_millis(8 * i), false));
            glide.at()
        };
        assert!(run(95) < 0.5, "a quarter of the response in, {}", run(95));
        assert!(run(380) > 0.95, "the response in, {}", run(380));
    }

    #[test]
    fn reduced_motion_jumps_to_where_it_is_sent() {
        let mut glide = Glide::from(0.);
        assert!(!glide.tick(1., Instant::now(), true));
        assert_eq!(glide.at(), 1.);
    }
}
