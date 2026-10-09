use crate::desktop::Desktop;
use crate::terminal_view::surface::{self, Metrics};
use gpui_kit::*;
use term::{CURSOR_BAR, CURSOR_UNDERLINE, Frame};
use theme::{ON_TEXT, TERM_CURSOR, TEXT};

/// How a pane draws its caret this frame.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cursor {
    pub style: u8,
    pub hollow: bool,
}

/// None while the program hides the cursor, or while a focused blinking cursor is in its off phase.
pub fn cursor(f: &Frame, focused: bool, blink_on: bool) -> Option<Cursor> {
    let shown = f.cursor_visible == 1 && (!focused || f.cursor_blink == 0 || blink_on);
    shown.then_some(Cursor { style: f.cursor_style, hollow: !focused })
}

/// The caret's rectangle within its cell, as (x, y, width, height).
pub fn shape(c: &Cursor, cell: f32, line: f32) -> (f32, f32, f32, f32) {
    match c.style {
        _ if c.hollow => (0., 0., cell, line),
        CURSOR_BAR => (0., 0., 2., line),
        CURSOR_UNDERLINE => (0., line - 2., cell, 2.),
        _ => (0., 0., cell, line),
    }
}

pub fn blinks(frame_blink: bool, reduce_motion: bool, window_active: bool) -> bool {
    frame_blink && !reduce_motion && window_active
}

/// Draws the caret, or the IME's uncommitted text in its place even while the program hides the caret, at cell `at` of a grid of `rows` rows.
/// The focused pane also records the caret's cell so the IME can place its candidate window there.
pub fn overlay(view: Option<Entity<Desktop>>, at: (u16, u16), rows: u16, c: Option<Cursor>, preedit: Option<String>, m: &Metrics) -> impl IntoElement {
    let (fnt, font_size, line) = (m.font.clone(), m.size, m.line);
    let cell_font = fnt.clone();
    canvas(
        move |_, window, _| surface::cell_width(window, &cell_font, font_size),
        move |bounds, cell, window, cx| {
            let Some(cell) = cell else { return };
            let origin = point(bounds.origin.x + px(at.0 as f32 * cell), surface::grid_top(bounds, rows, line) + px(at.1 as f32 * line));
            if let Some(view) = view {
                view.update(cx, |d, _| d.terminal.caret = Some(Bounds::new(origin, size(px(cell), px(line)))));
            }
            if let Some(text) = preedit {
                let run = TextRun {
                    len: text.len(),
                    font: fnt,
                    color: TEXT.into(),
                    background_color: Some(ON_TEXT.into()),
                    underline: Some(UnderlineStyle { thickness: px(1.), color: None, wavy: false }),
                    strikethrough: None,
                };
                let shaped = window.text_system().shape_line(text.into(), px(font_size), &[run], None);
                shaped.paint(origin, px(line), TextAlign::Left, None, window, cx).ok();
                return;
            }
            let Some(c) = c else { return };
            let (x, y, w, h) = shape(&c, cell, line);
            let caret = Bounds::new(point(origin.x + px(x), origin.y + px(y)), size(px(w), px(h)));
            if c.hollow {
                window.paint_quad(outline(caret, TERM_CURSOR, BorderStyle::Solid));
            } else {
                window.paint_quad(fill(caret, Hsla::from(TERM_CURSOR)));
            }
        },
    )
    .absolute()
    .inset_0()
}

#[cfg(test)]
mod tests {
    use super::{Cursor, blinks, cursor, shape};
    use term::{CURSOR_BAR, CURSOR_BLOCK, CURSOR_UNDERLINE, Frame};

    fn frame(style: u8, blink: u8) -> Frame {
        Frame { cursor_x: 3, cursor_y: 1, cursor_visible: 1, cursor_style: style, cursor_blink: blink, ..Default::default() }
    }

    #[test]
    fn a_focused_blinking_cursor_hides_in_its_off_phase() {
        let f = frame(CURSOR_BLOCK, 1);
        assert_eq!(cursor(&f, true, true), Some(Cursor { style: CURSOR_BLOCK, hollow: false }));
        assert_eq!(cursor(&f, true, false), None);
        assert!(cursor(&frame(CURSOR_BLOCK, 0), true, false).is_some());
    }

    #[test]
    fn an_unfocused_cursor_is_a_steady_hollow_block() {
        let c = cursor(&frame(CURSOR_BAR, 1), false, false).unwrap();
        assert!(c.hollow);
        assert_eq!(shape(&c, 8., 20.), (0., 0., 8., 20.));
    }

    #[test]
    fn a_hidden_cursor_draws_nothing() {
        let f = Frame { cursor_visible: 0, ..frame(CURSOR_BLOCK, 0) };
        assert_eq!(cursor(&f, true, true), None);
    }

    #[test]
    fn the_caret_takes_the_shape_the_program_asked_for() {
        let caret = |style| shape(&Cursor { style, hollow: false }, 8., 20.);
        assert_eq!(caret(CURSOR_BLOCK), (0., 0., 8., 20.));
        assert_eq!(caret(CURSOR_BAR), (0., 0., 2., 20.));
        assert_eq!(caret(CURSOR_UNDERLINE), (0., 18., 8., 2.));
    }

    #[test]
    fn the_cursor_blinks_only_in_an_active_window_with_motion_allowed() {
        assert!(blinks(true, false, true));
        assert!(!blinks(false, false, true));
        assert!(!blinks(true, true, true));
        assert!(!blinks(true, false, false));
    }
}
