use crate::desktop::Desktop;
use crate::desktop::chrome::id;
use gpui_kit::*;
use theme::ui_font;
use ui::Variant;

/// Turns wheel and trackpad deltas into whole rows, carrying the fraction over within one gesture on one pane.
#[derive(Default)]
pub struct WheelRows {
    pane: String,
    rest: f32,
}

impl WheelRows {
    /// Rows to move; positive is toward older output.
    pub fn rows(&mut self, pane: &str, delta: ScrollDelta, phase: TouchPhase, line: f32) -> isize {
        if self.pane != pane || phase == TouchPhase::Started {
            self.pane = pane.to_string();
            self.rest = 0.;
        }
        let y = match delta {
            ScrollDelta::Lines(l) => l.y,
            ScrollDelta::Pixels(p) => f32::from(p.y) / line,
        };
        let total = self.rest + y;
        self.rest = total.fract();
        total.trunc() as isize
    }
}

#[derive(Debug, PartialEq)]
pub enum Wheel {
    Viewport(isize),
    Arrows { up: bool, n: usize },
    Report { up: bool, n: usize },
}

/// Apps tracking the mouse get the wheel as mouse events, other full-screen apps that ask for it (mode 1007) as arrow keys; Shift always scrolls the history.
pub fn route(rows: isize, shift: bool, mouse: bool, alt_screen: bool, alt_scroll: bool) -> Wheel {
    if shift {
        Wheel::Viewport(-rows)
    } else if mouse {
        Wheel::Report { up: rows > 0, n: rows.unsigned_abs() }
    } else if alt_screen && alt_scroll {
        Wheel::Arrows { up: rows > 0, n: rows.unsigned_abs() }
    } else {
        Wheel::Viewport(-rows)
    }
}

pub fn arrows(up: bool, n: usize, app_cursor: bool) -> Vec<u8> {
    let key = [if app_cursor { b'O' } else { b'[' }, if up { b'A' } else { b'B' }];
    [&[0x1b][..], &key].concat().repeat(n)
}

pub fn jump_pill(pane: &str, cx: &mut Context<Desktop>) -> Div {
    let pane = pane.to_string();
    let button = ui::button(id(format!("jump-{pane}")), Variant::Glass, None, "Jump to bottom ↓")
        .h(px(28.))
        .rounded(px(14.))
        .text_size(px(12.))
        .font_family(ui_font())
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.scroll_to_bottom(&pane, cx)));
    div().absolute().right(px(12.)).bottom(px(12.)).occlude().child(button)
}

#[cfg(test)]
mod tests {
    use super::{Wheel, WheelRows, arrows, route};
    use gpui_kit::{ScrollDelta, TouchPhase, point, px};

    fn pixels(y: f32) -> ScrollDelta {
        ScrollDelta::Pixels(point(px(0.), px(y)))
    }

    #[test]
    fn pixel_deltas_accumulate_into_whole_rows() {
        let mut w = WheelRows::default();
        assert_eq!(w.rows("a", pixels(15.), TouchPhase::Started, 20.), 0);
        assert_eq!(w.rows("a", pixels(15.), TouchPhase::Moved, 20.), 1);
        assert_eq!(w.rows("a", pixels(-30.), TouchPhase::Moved, 20.), -1);
        assert_eq!(w.rows("a", ScrollDelta::Lines(point(0., 3.)), TouchPhase::Moved, 20.), 3);
    }

    #[test]
    fn a_new_gesture_or_pane_drops_the_remainder() {
        let mut w = WheelRows::default();
        w.rows("a", pixels(15.), TouchPhase::Started, 20.);
        assert_eq!(w.rows("a", pixels(15.), TouchPhase::Started, 20.), 0);
        assert_eq!(w.rows("b", pixels(15.), TouchPhase::Moved, 20.), 0);
    }

    #[test]
    fn shift_always_scrolls_the_viewport() {
        assert_eq!(route(3, true, true, true, true), Wheel::Viewport(-3));
        assert_eq!(route(-2, false, false, false, true), Wheel::Viewport(2));
    }

    #[test]
    fn alt_screen_with_alternate_scroll_sends_arrows() {
        assert_eq!(route(3, false, false, true, true), Wheel::Arrows { up: true, n: 3 });
        assert_eq!(route(-1, false, false, true, true), Wheel::Arrows { up: false, n: 1 });
        assert_eq!(route(2, false, false, true, false), Wheel::Viewport(-2));
        assert_eq!(arrows(true, 2, false), b"\x1b[A\x1b[A");
        assert_eq!(arrows(false, 1, true), b"\x1bOB");
    }

    #[test]
    fn apps_tracking_the_mouse_get_wheel_events_rather_than_arrows() {
        assert_eq!(route(3, false, true, true, true), Wheel::Report { up: true, n: 3 });
        assert_eq!(route(-2, false, true, false, false), Wheel::Report { up: false, n: 2 });
    }
}
