use crate::desktop::Desktop;
use term::{Cell, Frame, Pointer, WIDE_SPACER_TAIL};
use theme::{MONO, ON_TEXT, SELECTION, SYMBOLS, TEXT};
use gpui_kit::*;
use std::sync::Arc;

pub struct Metrics {
    pub size: f32,
    pub line: f32,
}

pub const MAIN: Metrics = Metrics { size: 13., line: 17. };
pub const SMALL: Metrics = Metrics { size: 12., line: 15. };

/// Geist Mono with ligatures off: a merged `--` would collapse two cells into one.
pub(crate) fn term_font() -> Font {
    let off = ["liga", "calt", "dlig"].map(|tag| (tag.to_string(), 0)).to_vec();
    Font { features: FontFeatures(Arc::new(off)), ..font(MONO) }
}

/// The advance of one terminal cell at `size`.
pub fn cell_width(window: &Window, size: f32) -> Option<f32> {
    let ts = window.text_system();
    Some(f32::from(ts.advance(ts.resolve_font(&term_font()), px(size), 'm').ok()?.width))
}

/// Where the grid's first row sits: whatever `bounds` has left under a whole row is split above and below it.
pub fn grid_top(bounds: Bounds<Pixels>, rows: u16, line: f32) -> Pixels {
    bounds.origin.y + (bounds.size.height - px(rows as f32 * line)) / 2.
}

/// Sizes the pane's session to its bounds, selects its text with the mouse over the `grid` on screen,
/// and, for the focused pane, takes the IME input.
pub fn surface(view: Entity<Desktop>, id: String, m: &Metrics, grid: Option<(u16, u16)>, focus: Option<FocusHandle>) -> impl IntoElement {
    let fit_view = view.clone();
    let pane = id.clone();
    let (size, line) = (m.size, m.line);
    canvas(
        move |bounds, window, cx| {
            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            let cell = cell_width(window, size)?;
            let cols = (f32::from(bounds.size.width) / cell).max(1.) as u16;
            let rows = (f32::from(bounds.size.height) / line).max(1.) as u16;
            fit_view.update(cx, |d, cx| d.fit(&id, cols, rows, cx));
            Some((hitbox, cell))
        },
        move |bounds, prepaint, window, cx| {
            if let Some(focus) = focus {
                window.handle_input(&focus, ElementInputHandler::new(bounds, view.clone()), cx);
            }
            let (Some((hitbox, cell)), Some((cols, rows))) = (prepaint, grid) else { return };
            window.set_cursor_style(CursorStyle::IBeam, &hitbox);
            let top = grid_top(bounds, rows, line);
            let at = move |p: Point<Pixels>| Pointer::at(f32::from(p.x - bounds.origin.x), f32::from(p.y - top), cell, line, cols, rows);
            let (down, moved, up, wheel) = (view.clone(), view.clone(), view.clone(), view);
            let (down_pane, moved_pane, wheel_pane) = (pane.clone(), pane.clone(), pane);
            let wheel_hitbox = hitbox.clone();
            window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && e.button == MouseButton::Left && hitbox.is_hovered(window) {
                    down.update(cx, |d, cx| d.select_start(&down_pane, at(e.position), e.click_count, e.modifiers.shift, line, cx));
                }
            });
            window.on_mouse_event(move |e: &MouseMoveEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble && e.pressed_button == Some(MouseButton::Left) {
                    moved.update(cx, |d, cx| d.select_extend(&moved_pane, at(e.position), cx));
                }
            });
            window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
                    up.update(cx, |d, _| d.select_release());
                }
            });
            window.on_mouse_event(move |e: &ScrollWheelEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && wheel_hitbox.is_hovered(window) {
                    wheel.update(cx, |d, cx| d.wheel(&wheel_pane, e, at(e.position), line, cx));
                }
            });
        },
    )
    .absolute()
    .size_full()
}

/// Nerd Font icons, as shell prompts draw them, sit in the Private Use Areas that Geist Mono leaves empty.
fn is_icon(ch: char) -> bool {
    matches!(ch, '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{FFFFD}')
}

fn color([r, g, b]: [u8; 3]) -> Hsla {
    rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into()
}

/// Paints with the card's ink and paper where the program leaves colors at their defaults.
pub fn screen(f: &Frame, cells: &[Cell], m: &Metrics) -> Div {
    let ink: Hsla = TEXT.into();
    let paper: Hsla = ON_TEXT.into();
    let base = term_font();
    let mut rows = Vec::new();
    for row in cells.chunks(f.cols.max(1) as usize) {
        let mut text = String::new();
        let mut runs: Vec<TextRun> = Vec::new();
        for c in row {
            if c.wide == WIDE_SPACER_TAIL {
                continue;
            }
            let mut fg = if c.has_fg == 1 { color(c.fg) } else { ink };
            let mut bg = (c.has_bg == 1).then(|| color(c.bg));
            if c.inverse == 1 {
                (fg, bg) = (bg.unwrap_or(paper), Some(fg));
            }
            if c.selected == 1 {
                bg = Some(SELECTION.into());
            }
            let ch = c.ch();
            text.push(ch);
            let mut fnt = if is_icon(ch) { font(SYMBOLS) } else { base.clone() };
            if c.bold == 1 {
                fnt.weight = FontWeight::SEMIBOLD;
            }
            if c.italic == 1 {
                fnt.style = FontStyle::Italic;
            }
            let run = TextRun {
                len: ch.len_utf8(),
                font: fnt,
                color: fg,
                background_color: bg,
                underline: (c.underline == 1).then(|| UnderlineStyle { thickness: px(1.), color: None, wavy: false }),
                strikethrough: None,
            };
            match runs.last_mut() {
                Some(last)
                    if last.font == run.font
                        && last.color == run.color
                        && last.background_color == run.background_color
                        && last.underline == run.underline =>
                {
                    last.len += run.len
                }
                _ => runs.push(run),
            }
        }
        rows.push(div().h(px(m.line)).flex_none().whitespace_nowrap().child(StyledText::new(text).with_runs(runs)));
    }
    div().size_full().flex().flex_col().justify_center().children(rows)
}

#[cfg(test)]
mod tests {
    use super::{grid_top, is_icon, term_font};
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn leftover_height_is_split_above_and_below_the_grid() {
        let bounds = Bounds::new(point(px(0.), px(100.)), size(px(80.), px(110.)));
        assert_eq!(grid_top(bounds, 6, 17.), px(104.));
    }

    #[test]
    fn nerd_font_icons_are_told_apart_from_text() {
        assert!(is_icon('\u{E0A0}') && is_icon('\u{F0001}'));
        assert!(!is_icon('a') && !is_icon('世') && !is_icon('→'));
    }

    #[test]
    fn terminal_font_turns_off_every_ligature_feature() {
        let font = term_font();
        let features = font.features.tag_value_list();
        for tag in ["liga", "calt", "dlig"] {
            assert!(features.contains(&(tag.to_string(), 0)), "{tag}");
        }
    }
}
