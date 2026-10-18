use chat::Row;
use gpui_kit::*;
use std::time::{Duration, Instant};

/// The list's padding over and under its rows.
pub(super) const TOP: f32 = 8.;
pub(super) const BOTTOM: f32 = 24.;
/// How long the sent prompt takes to rise from the composer to the top.
pub(super) const RISE: Duration = Duration::from_millis(400);

/// Eases in and out: a full viewport in one go would leap most of the way in its first frame and flash, so the
/// prompt is seen leaving the composer before it glides up.
fn ease(t: f32) -> f32 {
    base::animation::cubic_bezier(0.77, 0., 0.175, 1.)(t)
}

/// A sent prompt held at the top of the chat while its reply grows under it, by room padded under the list's end.
/// Once the reply fills the viewport the chat follows its tail again.
#[derive(Debug, PartialEq)]
pub(super) struct Pin {
    stage: Stage,
    /// Whether the list sits on the prompt rather than where the user scrolled.
    pub(super) anchored: bool,
    /// What pads the list's end now.
    pub(super) spacer: f32,
    /// What holds the prompt at the top, as last measured.
    room: f32,
    /// Steers sent into the prompt's turn since, whose rows land under it.
    steers: usize,
}

#[derive(Debug, PartialEq)]
enum Stage {
    /// Until the prompt's row is measured.
    Sent,
    /// Rising from the end to the top, since then.
    Rising(Instant),
    Held,
}

impl Pin {
    pub(super) fn new() -> Self {
        Self { stage: Stage::Sent, anchored: false, spacer: 0., room: 0., steers: 0 }
    }

    /// A steer went into the held prompt's turn: the prompt stays held and the steer shows under its reply.
    pub(super) fn steered(&mut self) {
        self.steers += 1;
    }

    /// The row of the held prompt: the last sent but for the steers under it.
    pub(super) fn row(&self, rows: &[Row]) -> Option<usize> {
        rows.iter().enumerate().rev().filter(|(_, r)| matches!(r, Row::Prompt { .. } | Row::Pending(_))).nth(self.steers).map(|(i, _)| i)
    }

    /// The user moved the list: it stays on the prompt only if they left it at the end, where the prompt sits.
    pub(super) fn moved(&mut self, at_end: bool) {
        self.stage = Stage::Held;
        self.anchored = at_end;
        self.spacer = self.room;
    }

    pub(super) fn rising(&self) -> bool {
        matches!(self.stage, Stage::Sent | Stage::Rising(_))
    }

    /// Sizes the spacer from the height of the prompt and what follows it, None while any of it is unmeasured.
    /// The prompt rises over `rise`; returns false once the reply fills the viewport and the pin lets go.
    pub(super) fn step(&mut self, viewport: f32, below: Option<f32>, now: Instant, rise: Duration) -> bool {
        if let Some(below) = below {
            self.room = viewport - TOP - BOTTOM - below;
            if self.room <= 0. {
                return false;
            }
            if self.stage == Stage::Sent {
                self.stage = Stage::Rising(now);
            }
        }
        self.spacer = match self.stage {
            Stage::Sent => 0.,
            Stage::Rising(at) if now < at + rise => self.room * ease((now - at).as_secs_f32() / rise.as_secs_f32()),
            Stage::Rising(_) => {
                self.stage = Stage::Held;
                self.anchored = true;
                self.room
            }
            Stage::Held => self.room,
        };
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Pin, RISE, Row};
    use std::time::{Duration, Instant};

    #[test]
    fn the_prompt_rises_once_measured_then_holds_at_the_top() {
        let (mut pin, t0) = (Pin::new(), Instant::now());
        assert!(pin.step(800., None, t0, RISE) && pin.spacer == 0., "nothing moves before the prompt is measured");
        assert!(pin.step(800., Some(100.), t0, RISE));
        assert!(pin.step(800., Some(100.), t0 + Duration::from_millis(16), RISE));
        assert!(pin.spacer < 10. && !pin.anchored, "its first frame barely moves, so nothing leaps");
        assert!(pin.step(800., Some(100.), t0 + RISE / 2, RISE));
        assert!(pin.spacer > 200. && pin.spacer < 468., "it covers its distance mid-rise");
        assert!(pin.step(800., Some(100.), t0 + RISE, RISE));
        assert_eq!((pin.spacer, pin.anchored, pin.rising()), (668., true, false));
    }

    #[test]
    fn the_room_shrinks_as_the_reply_grows_and_the_pin_lets_go_once_it_fills_the_viewport() {
        let (mut pin, t0) = (Pin::new(), Instant::now());
        pin.step(800., Some(100.), t0, Duration::ZERO);
        pin.step(800., Some(100.), t0, Duration::ZERO);
        pin.step(800., Some(500.), t0, Duration::ZERO);
        assert_eq!(pin.spacer, 268.);
        assert!(pin.step(800., None, t0, Duration::ZERO) && pin.spacer == 268., "a row being measured keeps the last room");
        assert!(!pin.step(800., Some(768.), t0, Duration::ZERO));
    }

    #[test]
    fn a_scroll_stops_the_rise_and_holds_the_prompt_only_from_the_end() {
        let (mut pin, t0) = (Pin::new(), Instant::now());
        pin.step(800., Some(100.), t0, RISE);
        pin.moved(false);
        pin.step(800., Some(100.), t0 + Duration::from_millis(10), RISE);
        assert_eq!((pin.spacer, pin.anchored, pin.rising()), (668., false, false), "the room is there for a scroll back down");
        pin.moved(true);
        assert!(pin.anchored);
    }

    #[test]
    fn a_steer_leaves_the_pin_on_the_prompt_it_steers() {
        let mut pin = Pin::new();
        let rows = [Row::Prompt { turn: 0 }, Row::Fold { turn: 0 }, Row::Live { turn: 0 }, Row::Pending(0), Row::Working];
        pin.steered();
        assert_eq!(pin.row(&rows), Some(0), "the steer's row waits at the tail");
        let echoed = [Row::Prompt { turn: 0 }, Row::Fold { turn: 0 }, Row::End { turn: 0 }, Row::Prompt { turn: 1 }, Row::Live { turn: 1 }];
        assert_eq!(pin.row(&echoed), Some(0), "and once it's a turn of its own");
        assert_eq!(Pin::new().row(&echoed), Some(3), "a send pins its own prompt");
    }

    #[test]
    fn the_pin_holds_the_prompt_last_sent() {
        let rows = [Row::Prompt { turn: 0 }, Row::End { turn: 0 }, Row::Prompt { turn: 1 }, Row::Fold { turn: 1 }];
        assert_eq!(Pin::new().row(&rows), Some(2));
        assert_eq!(Pin::new().row(&[Row::Prompt { turn: 0 }, Row::Pending(0), Row::Working]), Some(1));
        assert_eq!(Pin::new().row(&[]), None);
    }
}
