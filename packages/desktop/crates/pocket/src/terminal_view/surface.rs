use crate::desktop::Desktop;
use term::{Cell, Frame, Pos, Selection, WIDE_SPACER_TAIL};
use theme::{MONO, ON_TEXT, SELECTION, TEXT};
use gpui_kit::*;

pub struct Metrics {
    pub size: f32,
    pub line: f32,
}

pub const MAIN: Metrics = Metrics { size: 13., line: 22. };
pub const SMALL: Metrics = Metrics { size: 12., line: 19. };

/// Sizes the pane's session to its bounds, selects its text with the mouse over the `grid` on screen,
/// and, for the focused pane, takes the IME input.
pub fn surface(view: Entity<Desktop>, id: String, m: &Metrics, grid: Option<(u16, u16)>, focus: Option<FocusHandle>) -> impl IntoElement {
    let fit_view = view.clone();
    let pane = id.clone();
    let (size, line) = (m.size, m.line);
    canvas(
        move |bounds, window, cx| {
            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            let ts = window.text_system();
            let cell = f32::from(ts.advance(ts.resolve_font(&font(MONO)), px(size), 'm').ok()?.width);
            let cols = (f32::from(bounds.size.width) / cell).max(1.) as u16;
            let rows = (f32::from(bounds.size.height) / line).max(1.) as u16;
            fit_view.update(cx, |d, _| d.fit(&id, cols, rows));
            Some((hitbox, cell))
        },
        move |bounds, prepaint, window, cx| {
            if let Some(focus) = focus {
                window.handle_input(&focus, ElementInputHandler::new(bounds, view.clone()), cx);
            }
            let (Some((hitbox, cell)), Some((cols, rows))) = (prepaint, grid) else { return };
            window.set_cursor_style(CursorStyle::IBeam, &hitbox);
            let at = move |p: Point<Pixels>| grid_point(f32::from(p.x - bounds.origin.x), f32::from(bounds.bottom() - p.y), cell, line, cols, rows);
            let (down, moved, up) = (view.clone(), view.clone(), view);
            let (down_pane, moved_pane) = (pane.clone(), pane);
            window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && e.button == MouseButton::Left && hitbox.is_hovered(window) {
                    down.update(cx, |d, cx| d.select_start(&down_pane, at(e.position), cx));
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
        },
    )
    .absolute()
    .size_full()
}

/// The cell boundary nearest a point `x` from the left and `from_bottom` up, on a grid drawn bottom-aligned.
fn grid_point(x: f32, from_bottom: f32, cell: f32, line: f32, cols: u16, rows: u16) -> Pos {
    let col = (x / cell).round().clamp(0., cols as f32) as u16;
    let up = (from_bottom / line).floor().max(0.) as u16;
    Pos { row: rows.saturating_sub(1).saturating_sub(up), col }
}

fn color([r, g, b]: [u8; 3]) -> Hsla {
    rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into()
}

/// Paints with the card's ink and paper where the program leaves colors at their defaults.
pub fn screen(f: &Frame, cells: &[Cell], m: &Metrics, selection: Option<Selection>) -> Div {
    let ink: Hsla = TEXT.into();
    let paper: Hsla = ON_TEXT.into();
    let mut rows = Vec::new();
    for (y, row) in cells.chunks(f.cols.max(1) as usize).enumerate() {
        let mut text = String::new();
        let mut runs: Vec<TextRun> = Vec::new();
        for (x, c) in row.iter().enumerate() {
            if c.wide == WIDE_SPACER_TAIL {
                continue;
            }
            let mut fg = if c.has_fg == 1 { color(c.fg) } else { ink };
            let mut bg = (c.has_bg == 1).then(|| color(c.bg));
            if c.inverse == 1 || (f.cursor_visible == 1 && x == f.cursor_x as usize && y == f.cursor_y as usize) {
                (fg, bg) = (bg.unwrap_or(paper), Some(fg));
            }
            if selection.is_some_and(|s| s.contains(x as u16, y as u16)) {
                bg = Some(SELECTION.into());
            }
            let ch = c.ch();
            text.push(ch);
            let mut fnt = font(MONO);
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
    div().size_full().flex().flex_col().justify_end().children(rows)
}

#[cfg(test)]
mod tests {
    use super::grid_point;
    use term::Pos;

    #[test]
    fn a_point_takes_the_nearest_boundary_on_the_row_it_is_in_counting_up_from_the_bottom() {
        assert_eq!(grid_point(0.4 * 8., 1., 8., 20., 10, 5), Pos { row: 4, col: 0 });
        assert_eq!(grid_point(0.6 * 8., 1., 8., 20., 10, 5), Pos { row: 4, col: 1 });
        assert_eq!(grid_point(3. * 8., 20. * 2.5, 8., 20., 10, 5), Pos { row: 2, col: 3 });
    }

    #[test]
    fn a_point_off_the_grid_keeps_to_its_edge() {
        assert_eq!(grid_point(-5., -3., 8., 20., 10, 5), Pos { row: 4, col: 0 });
        assert_eq!(grid_point(500., 500., 8., 20., 10, 5), Pos { row: 0, col: 10 });
    }
}
