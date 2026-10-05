use term::Button;

/// Which mouse events go to a program tracking the mouse rather than to selection.
#[derive(Default)]
pub struct Reporting {
    /// The pane and button whose press the program took, so its drags and release follow it there.
    held: Option<(String, Button)>,
}

#[derive(Debug, PartialEq)]
pub enum Move {
    Drag(Button),
    Hover,
    Skip,
}

impl Reporting {
    /// Takes the press for a program tracking the mouse; Shift keeps it for selecting text.
    pub fn press(&mut self, pane: &str, button: Button, tracking: bool, shift: bool) -> bool {
        let take = tracking && !shift;
        if take {
            self.held = Some((pane.to_string(), button));
        }
        take
    }

    /// What a move over `pane` reports. A move with no button down ends a press whose release never came.
    pub fn motion(&mut self, pane: &str, hovered: bool, pressed: bool) -> Move {
        if !pressed {
            self.held = None;
            return if hovered { Move::Hover } else { Move::Skip };
        }
        match &self.held {
            Some((p, b)) if p == pane => Move::Drag(*b),
            _ => Move::Skip,
        }
    }

    /// Whether this release ends the press the program took in `pane`.
    pub fn release(&mut self, pane: &str, button: Button) -> bool {
        let ends = self.held.as_ref().is_some_and(|(p, b)| p == pane && *b == button);
        if ends {
            self.held = None;
        }
        ends
    }
}

#[cfg(test)]
mod tests {
    use super::{Move, Reporting};
    use term::Button;

    #[test]
    fn a_press_goes_to_the_program_unless_shift_is_held_or_it_tracks_nothing() {
        let mut r = Reporting::default();
        assert!(!r.press("a", Button::Left, false, false));
        assert!(!r.press("a", Button::Left, true, true));
        assert!(r.press("a", Button::Left, true, false));
    }

    #[test]
    fn drags_and_the_release_follow_the_press_to_its_pane() {
        let mut r = Reporting::default();
        r.press("a", Button::Left, true, false);
        assert_eq!(r.motion("b", true, true), Move::Skip);
        assert_eq!(r.motion("a", false, true), Move::Drag(Button::Left));
        assert!(!r.release("b", Button::Left));
        assert!(!r.release("a", Button::Right));
        assert!(r.release("a", Button::Left));
        assert!(!r.release("a", Button::Left));
    }

    #[test]
    fn a_bare_move_reports_only_over_the_pane() {
        let mut r = Reporting::default();
        assert_eq!(r.motion("a", true, false), Move::Hover);
        assert_eq!(r.motion("a", false, false), Move::Skip);
    }

    #[test]
    fn a_drag_the_program_did_not_take_reports_nothing() {
        let mut r = Reporting::default();
        r.press("a", Button::Left, true, true);
        assert_eq!(r.motion("a", true, true), Move::Skip);
    }

    #[test]
    fn a_move_with_no_button_down_ends_a_press_whose_release_went_missing() {
        let mut r = Reporting::default();
        r.press("a", Button::Left, true, false);
        r.motion("b", false, false);
        assert_eq!(r.motion("a", true, true), Move::Skip);
        assert!(!r.release("a", Button::Left));
    }
}
