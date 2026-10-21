use chat::Row;

/// The list's padding over and under its rows.
pub(super) const TOP: f32 = 8.;
pub(super) const BOTTOM: f32 = 24.;

/// A sent prompt held at the top of the chat while its reply grows under it, by room padded under the list's end.
/// It goes to the top at once, so what the chat showed before never scrolls up past the reader. Once the reply fills the
/// viewport the chat follows its tail again.
#[derive(Debug, PartialEq)]
pub(super) struct Pin {
    /// Until the prompt's row is measured.
    sent: bool,
    /// Whether the list sits on the prompt rather than where the user scrolled.
    pub(super) anchored: bool,
    /// What pads the list's end now.
    pub(super) spacer: f32,
    /// What holds the prompt at the top, as last measured.
    room: f32,
    /// Steers sent into the prompt's turn since, whose rows land under it.
    steers: usize,
}

impl Pin {
    pub(super) fn new() -> Self {
        Self { sent: true, anchored: false, spacer: 0., room: 0., steers: 0 }
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
    }

    /// Whether the prompt's row is still to be measured: until then it shows at the list's end, not yet held.
    pub(super) fn unmeasured(&self) -> bool {
        self.sent
    }

    /// The height of the prompt and what follows it, for `step`: as measured now, else, until first held, as last laid
    /// out once the prompt's own row was. A reply streaming in under it is remeasured on every update, so waiting for all
    /// of it to measure at once would leave the prompt hidden for as long as the reply streams.
    pub(super) fn below(&self, measured: Option<f32>, laid_out: Option<f32>) -> Option<f32> {
        measured.or(laid_out.filter(|_| self.sent))
    }

    /// Sizes the spacer from the height of the prompt and what follows it, None while any of it is unmeasured.
    /// Returns false once the reply fills the viewport and the pin lets go.
    pub(super) fn step(&mut self, viewport: f32, below: Option<f32>) -> bool {
        if let Some(below) = below {
            self.room = viewport - TOP - BOTTOM - below;
            if self.room <= 0. {
                return false;
            }
            if self.sent {
                self.sent = false;
                self.anchored = true;
            }
        }
        if !self.sent {
            self.spacer = self.room;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{Pin, Row};

    #[test]
    fn the_prompt_holds_at_the_top_as_soon_as_its_row_is_measured() {
        let mut pin = Pin::new();
        assert!(pin.step(800., None) && pin.spacer == 0. && pin.unmeasured(), "nothing moves before the prompt is measured");
        assert!(pin.step(800., Some(100.)));
        assert_eq!((pin.spacer, pin.anchored, pin.unmeasured()), (668., true, false), "no rise: what showed before never scrolls up");
    }

    #[test]
    fn the_prompt_holds_once_its_row_is_laid_out_though_the_reply_under_it_is_still_measuring() {
        let mut pin = Pin::new();
        assert_eq!(pin.below(None, None), None, "the prompt's own row isn't laid out yet");
        let below = pin.below(None, Some(100.));
        assert!(pin.step(800., below));
        assert_eq!((pin.spacer, pin.anchored, pin.unmeasured()), (668., true, false));
        assert_eq!(pin.below(None, Some(300.)), None, "once held, a row being measured keeps the last room");
        assert_eq!(pin.below(Some(120.), Some(300.)), Some(120.));
    }

    #[test]
    fn the_room_shrinks_as_the_reply_grows_and_the_pin_lets_go_once_it_fills_the_viewport() {
        let mut pin = Pin::new();
        pin.step(800., Some(100.));
        pin.step(800., Some(500.));
        assert_eq!(pin.spacer, 268.);
        assert!(pin.step(800., None) && pin.spacer == 268., "a row being measured keeps the last room");
        assert!(!pin.step(800., Some(768.)));
    }

    #[test]
    fn a_scroll_holds_the_prompt_only_from_the_end() {
        let mut pin = Pin::new();
        pin.step(800., Some(100.));
        pin.moved(false);
        pin.step(800., Some(100.));
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
}
