use crate::Desktop;
use crate::term::{Cell, Frame, WIDE_SPACER_TAIL};
use crate::theme::{MONO, TEXT, WHITE};
use gpui_kit::*;

pub struct Metrics {
    pub size: f32,
    pub line: f32,
}

pub const MAIN: Metrics = Metrics { size: 13., line: 21. };
pub const SMALL: Metrics = Metrics { size: 12., line: 19. };

/// Sizes the pane's session to its bounds and, for the focused pane, takes the IME input.
pub fn surface(view: Entity<Desktop>, id: String, m: &Metrics, focus: Option<FocusHandle>) -> impl IntoElement {
    let fit_view = view.clone();
    let (size, line) = (m.size, m.line);
    canvas(
        move |bounds, window, cx| {
            let ts = window.text_system();
            let Ok(cell) = ts.advance(ts.resolve_font(&font(MONO)), px(size), 'm') else { return };
            let cols = (f32::from(bounds.size.width) / f32::from(cell.width)).max(1.) as u16;
            let rows = (f32::from(bounds.size.height) / line).max(1.) as u16;
            fit_view.update(cx, |d, _| d.fit(&id, cols, rows));
        },
        move |bounds, _, window, cx| {
            if let Some(focus) = focus {
                window.handle_input(&focus, ElementInputHandler::new(bounds, view), cx);
            }
        },
    )
    .absolute()
    .size_full()
}

fn color([r, g, b]: [u8; 3]) -> Hsla {
    rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into()
}

/// Paints with the card's ink and paper where the program leaves colors at their defaults.
pub fn screen(f: &Frame, cells: &[Cell], m: &Metrics) -> Div {
    let ink: Hsla = rgba(TEXT).into();
    let paper: Hsla = rgba(WHITE).into();
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
            let ch = char::from_u32(c.cp).filter(|_| c.cp != 0).unwrap_or(' ');
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
        rows.push(div().h(px(m.line)).whitespace_nowrap().child(StyledText::new(text).with_runs(runs)));
    }
    div().children(rows)
}
