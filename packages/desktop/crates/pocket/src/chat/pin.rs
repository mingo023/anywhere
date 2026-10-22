use chat::Row;
use std::time::{Duration, Instant};

/// The list's padding over and under its rows.
pub(super) const TOP: f32 = 8.;
pub(super) const BOTTOM: f32 = 24.;
/// How far down the viewport a sent prompt is held, under the end of what came before it.
const HOLD: f32 = 0.25;
/// How long the chat takes to glide a sent prompt to where it's held.
const GLIDE: Duration = Duration::from_millis(520);

/// A sent prompt held a quarter down the chat while its reply grows under it, by room padded under the list's end.
/// The chat glides it there from the list's end, keeping the end of what came before in sight. Once the reply fills
/// what's under it the chat follows its tail again.
#[derive(Debug, PartialEq)]
pub(super) struct Pin {
    /// Until the prompt's row is measured.
    sent: bool,
    /// Whether the list sits on the prompt rather than where the user scrolled.
    pub(super) anchored: bool,
    /// What pads the list's end now.
    pub(super) spacer: f32,
    /// What holds the prompt where it's held, as last measured.
    room: f32,
    /// How far under the viewport's top the prompt is held.
    held: f32,
    /// Where the prompt glides from, under the viewport's top, and since when.
    glide: Option<(f32, Instant)>,
    /// Steers sent into the prompt's turn since, whose rows land under it.
    steers: usize,
}

impl Pin {
    pub(super) fn new() -> Self {
        Self { sent: true, anchored: false, spacer: 0., room: 0., held: TOP, glide: None, steers: 0 }
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
        self.sent = false;
        self.anchored = at_end;
        self.spacer = self.room;
        self.glide = None;
    }

    /// Whether the prompt's row is still to be measured: until then it shows at the list's end, not yet held.
    pub(super) fn unmeasured(&self) -> bool {
        self.sent
    }

    /// The height of the prompt and what follows it, for `step`: as measured now, else, until first held, as last laid
    /// out once the prompt's own row was. A reply streaming in under it is remeasured on every update, so waiting for all
    /// of it to measure at once would hold the prompt only once the reply stops.
    pub(super) fn below(&self, measured: Option<f32>, laid_out: Option<f32>) -> Option<f32> {
        measured.or(laid_out.filter(|_| self.sent))
    }

    /// Sizes the spacer from the height of the rows `above` the prompt and of the prompt and what follows it, None while
    /// any of it is unmeasured. A row being measured may have shrunk, and a list too short for the viewport scrolls up to
    /// the turn before, so until it's measured the spacer fills what's under the prompt but for a pixel: a list whose
    /// padding covers it paints no rows, and the prompt's row is taller than that pixel.
    /// Once first measured, the prompt glides from the list's end unless `glide` is false.
    /// Returns false once the reply fills what's under the prompt and the pin lets go.
    pub(super) fn step(&mut self, viewport: f32, above: f32, below: Option<f32>, glide: Option<Instant>) -> bool {
        self.held = (viewport * HOLD).min(TOP + above);
        if let Some(below) = below {
            self.room = viewport - self.held - BOTTOM - below;
            if self.room <= 0. {
                return false;
            }
            if self.sent {
                self.sent = false;
                self.anchored = true;
                self.glide = glide.map(|now| (viewport - BOTTOM - below, now));
            }
        }
        if !self.sent {
            self.spacer = if below.is_some() { self.room } else { viewport - self.held - BOTTOM - 1. };
        }
        true
    }

    /// How far under the viewport's top the prompt shows at `now`, and whether it's still gliding there.
    pub(super) fn top(&mut self, now: Instant) -> (f32, bool) {
        let Some((from, start)) = self.glide else { return (self.held, false) };
        let t = (now.saturating_duration_since(start).as_secs_f32() / GLIDE.as_secs_f32()).min(1.);
        if t >= 1. {
            self.glide = None;
        }
        (from + (self.held - from) * (1. - (1. - t).powi(3)), t < 1.)
    }
}

#[cfg(test)]
mod tests {
    use super::{BOTTOM, GLIDE, Pin, Row, TOP};
    use std::time::Instant;

    #[test]
    fn a_prompt_with_nothing_before_it_holds_at_the_top_as_soon_as_its_row_is_measured() {
        let mut pin = Pin::new();
        assert!(pin.step(800., 0., None, None) && pin.spacer == 0. && pin.unmeasured(), "nothing moves before the prompt is measured");
        assert!(pin.step(800., 0., Some(100.), None));
        assert_eq!((pin.spacer, pin.anchored, pin.unmeasured()), (668., true, false));
    }

    #[test]
    fn the_prompt_holds_once_its_row_is_laid_out_though_the_reply_under_it_is_still_measuring() {
        let mut pin = Pin::new();
        assert_eq!(pin.below(None, None), None, "the prompt's own row isn't laid out yet");
        let below = pin.below(None, Some(100.));
        assert!(pin.step(800., 0., below, None));
        assert_eq!((pin.spacer, pin.anchored, pin.unmeasured()), (668., true, false));
        assert_eq!(pin.below(None, Some(300.)), None, "once held, a row being measured keeps the last room");
        assert_eq!(pin.below(Some(120.), Some(300.)), Some(120.));
    }

    #[test]
    fn the_room_shrinks_as_the_reply_grows_and_the_pin_lets_go_once_it_fills_the_viewport() {
        let mut pin = Pin::new();
        pin.step(800., 0., Some(100.), None);
        pin.step(800., 0., Some(500.), None);
        assert_eq!(pin.spacer, 268.);
        assert!(!pin.step(800., 0., Some(768.), None));
    }

    #[test]
    fn while_a_row_under_the_prompt_is_measured_the_spacer_fills_the_viewport() {
        let mut pin = Pin::new();
        pin.step(800., 0., Some(500.), None);
        assert!(pin.step(800., 0., None, None));
        assert_eq!(pin.spacer, 767., "the row may have shrunk: the list still fills from the prompt");
        assert!(TOP + BOTTOM + pin.spacer < 800., "a list whose padding covers it paints no rows");
        pin.step(800., 0., Some(100.), None);
        assert_eq!(pin.spacer, 668.);
    }

    #[test]
    fn a_scroll_holds_the_prompt_only_from_the_end() {
        let mut pin = Pin::new();
        pin.step(800., 0., Some(100.), None);
        pin.moved(false);
        pin.step(800., 0., Some(100.), None);
        assert_eq!((pin.spacer, pin.anchored), (668., false), "the room is there for a scroll back down");
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

    #[test]
    fn a_prompt_holds_a_quarter_down_under_what_came_before_it() {
        let mut pin = Pin::new();
        assert!(pin.step(800., 1000., Some(100.), None));
        assert_eq!((pin.top(Instant::now()), pin.spacer), ((200., false), 476.), "the room fills what's under the reply");
        assert!(!pin.step(800., 1000., Some(576.), None), "lets go once the reply fills what's under the prompt");
    }

    #[test]
    fn a_prompt_with_little_before_it_holds_just_under_that() {
        let mut pin = Pin::new();
        pin.step(800., 60., Some(100.), None);
        assert_eq!(pin.top(Instant::now()), (68., false));
    }

    #[test]
    fn the_chat_glides_a_sent_prompt_from_the_end_to_where_its_held() {
        let (mut pin, t0) = (Pin::new(), Instant::now());
        pin.step(800., 1000., Some(100.), Some(t0));
        assert_eq!(pin.top(t0), (676., true), "starts where the list's end showed it");
        let (half, gliding) = pin.top(t0 + GLIDE / 2);
        assert!(gliding && half < 676. - (676. - 200.) / 2., "eases out: most of the way by half time");
        assert_eq!(pin.top(t0 + GLIDE), (200., false));
        assert_eq!(pin.top(t0 + GLIDE * 2), (200., false));
    }

    #[test]
    fn a_scroll_stops_the_glide() {
        let (mut pin, t0) = (Pin::new(), Instant::now());
        pin.step(800., 1000., Some(100.), Some(t0));
        pin.moved(true);
        assert_eq!(pin.top(t0), (200., false));
    }
}
