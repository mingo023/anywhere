use crate::desktop::Desktop;
use term::{Cell, Frame, Pointer, WIDE_SPACER_TAIL};
use theme::{ON_TEXT, SELECTION, SYMBOLS, TEXT};
use gpui_kit::*;
use std::sync::Arc;

#[derive(Clone, Debug, PartialEq)]
pub struct Metrics {
    pub font: Font,
    pub size: f32,
    pub line: f32,
    /// Beside the text; above and below get half.
    pub pad: f32,
}

impl Metrics {
    pub fn of(t: &store::prefs::Terminal) -> Self {
        let family = t.font.clone().unwrap_or_else(|| theme::code_font().to_string());
        Self { font: term_font(&family, t.ligatures), size: t.size as f32, line: t.line(), pad: t.padding as f32 }
    }
}

/// `family` with ligatures off unless asked: in some fonts a merged `--` collapses two cells into one.
pub(crate) fn term_font(family: &str, ligatures: bool) -> Font {
    let off = if ligatures { Vec::new() } else { ["liga", "calt", "dlig"].map(|tag| (tag.to_string(), 0)).to_vec() };
    Font { features: FontFeatures(Arc::new(off)), ..font(family.to_string()) }
}

/// The advance of one terminal cell in `f` at `size`.
pub fn cell_width(window: &Window, f: &Font, size: f32) -> Option<f32> {
    let ts = window.text_system();
    Some(f32::from(ts.advance(ts.resolve_font(f), px(size), 'm').ok()?.width))
}

/// Where the grid's first row sits: whatever `bounds` has left under a whole row is split above and below it.
pub fn grid_top(bounds: Bounds<Pixels>, rows: u16, line: f32) -> Pixels {
    bounds.origin.y + (bounds.size.height - px(rows as f32 * line)) / 2.
}

/// Sizes the pane's session to its bounds, selects its text with the mouse over the `grid` on screen
/// or hands the mouse to a program tracking it, and, for the focused pane, takes the IME input.
pub fn surface(view: Entity<Desktop>, id: String, m: &Metrics, grid: Option<(u16, u16)>, focus: Option<FocusHandle>) -> impl IntoElement {
    let fit_view = view.clone();
    let pane = id.clone();
    let (fnt, size, line) = (m.font.clone(), m.size, m.line);
    canvas(
        move |bounds, window, cx| {
            let hitbox = window.insert_hitbox(bounds, HitboxBehavior::Normal);
            let cell = cell_width(window, &fnt, size)?;
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
            let on_link = view.read(cx).terminal.link_hover.as_ref() == Some(&pane);
            window.set_cursor_style(if on_link { CursorStyle::PointingHand } else { CursorStyle::IBeam }, &hitbox);
            let top = grid_top(bounds, rows, line);
            let at = move |p: Point<Pixels>| Pointer::at(f32::from(p.x - bounds.origin.x), f32::from(p.y - top), cell, line, cols, rows);
            let (down, moved, up, wheel, mods) = (view.clone(), view.clone(), view.clone(), view.clone(), view);
            let (down_pane, moved_pane, up_pane, wheel_pane, mods_pane) = (pane.clone(), pane.clone(), pane.clone(), pane.clone(), pane);
            let (moved_hitbox, wheel_hitbox, mods_hitbox) = (hitbox.clone(), hitbox.clone(), hitbox.clone());
            window.on_mouse_event(move |e: &MouseDownEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble && hitbox.is_hovered(window) {
                    down.update(cx, |d, cx| {
                        let left = e.button == MouseButton::Left;
                        if left && e.modifiers.secondary() && d.open_link(&down_pane, at(e.position), window, cx) {
                            cx.stop_propagation();
                            return;
                        }
                        // Never reported, even to a program tracking the mouse: agents like Claude Code track it, and would hide the menu.
                        if e.button == MouseButton::Right {
                            d.open_term_menu(&down_pane, e.position, cx);
                            return;
                        }
                        let taken = term_button(e.button).is_some_and(|b| d.mouse_press(&down_pane, at(e.position), b, e.modifiers, cx));
                        if left && !taken {
                            d.select_start(&down_pane, at(e.position), e.click_count, e.modifiers.shift, line, cx);
                        }
                    });
                }
            });
            window.on_mouse_event(move |e: &MouseMoveEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    let hovered = moved_hitbox.is_hovered(window);
                    moved.update(cx, |d, cx| {
                        d.mouse_motion(&moved_pane, at(e.position), hovered, e.pressed_button.is_some(), e.modifiers);
                        let pointer = (hovered && e.pressed_button.is_none()).then(|| at(e.position));
                        d.hover_link(&moved_pane, pointer, e.modifiers, cx);
                        if e.pressed_button == Some(MouseButton::Left) {
                            d.select_extend(&moved_pane, at(e.position), cx);
                        }
                    });
                }
            });
            window.on_key_event(move |e: &ModifiersChangedEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    let pointer = mods_hitbox.is_hovered(window).then(|| at(window.mouse_position()));
                    mods.update(cx, |d, cx| d.hover_link(&mods_pane, pointer, e.modifiers, cx));
                }
            });
            window.on_mouse_event(move |e: &MouseUpEvent, phase, window, cx| {
                if phase == DispatchPhase::Bubble {
                    up.update(cx, |d, cx| {
                        if let Some(b) = term_button(e.button) {
                            d.mouse_release(&up_pane, at(e.position), b, e.modifiers);
                        }
                        if e.button == MouseButton::Left {
                            d.select_release(window, cx);
                        }
                    });
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

fn term_button(b: MouseButton) -> Option<term::Button> {
    match b {
        MouseButton::Left => Some(term::Button::Left),
        MouseButton::Right => Some(term::Button::Right),
        MouseButton::Middle => Some(term::Button::Middle),
        _ => None,
    }
}

/// Nerd Font icons, as shell prompts draw them, sit in the Private Use Areas that Geist Mono leaves empty.
fn is_icon(ch: char) -> bool {
    matches!(ch, '\u{E000}'..='\u{F8FF}' | '\u{F0000}'..='\u{FFFFD}')
}

/// Below this contrast ratio text has all but vanished into its background, as in Claude Code's sticky prompt
/// (`rgb(80,80,80)` on `rgb(70,70,70)`, 1.2). Its dim hints on the plain background sit at 2.2 and stay as drawn.
const MIN_CONTRAST: f32 = 2.;

/// Whitespace, icons and the box, block and legacy-computing glyphs that draw shapes rather than text keep their colors:
/// a block drawn in the color of what's behind it is meant to blend in.
fn keeps_its_colors(ch: char) -> bool {
    ch.is_whitespace() || is_icon(ch) || matches!(ch, '\u{2500}'..='\u{259F}' | '\u{1FB00}'..='\u{1FBFF}')
}

/// `fg` as painted over the opaque `bg`, or, as Ghostty's `minimum-contrast` does, white or black (whichever stands
/// out more, at `fg`'s own opacity) when it is under `MIN_CONTRAST` against `bg`.
fn legible(fg: Hsla, bg: Hsla) -> Hsla {
    let (front, back) = (Rgba::from(fg), Rgba::from(bg));
    let over = |f: f32, b: f32| f * front.a + b * (1. - front.a);
    let shown = luminance([over(front.r, back.r), over(front.g, back.g), over(front.b, back.b)]);
    let behind = luminance([back.r, back.g, back.b]);
    if contrast(shown, behind) >= MIN_CONTRAST {
        return fg;
    }
    let pick = if contrast(1., behind) >= contrast(0., behind) { white() } else { black() };
    pick.opacity(fg.a)
}

/// WCAG relative luminance of an sRGB color.
fn luminance(rgb: [f32; 3]) -> f32 {
    let [r, g, b] = rgb.map(|c| if c <= 0.04045 { c / 12.92 } else { ((c + 0.055) / 1.055).powf(2.4) });
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

fn contrast(a: f32, b: f32) -> f32 {
    (a.max(b) + 0.05) / (a.min(b) + 0.05)
}

fn color([r, g, b]: [u8; 3]) -> Hsla {
    rgb(((r as u32) << 16) | ((g as u32) << 8) | b as u32).into()
}

/// Paints with the card's ink and paper where the program leaves colors at their defaults.
pub fn screen(f: &Frame, cells: &[Cell], m: &Metrics) -> Div {
    let ink: Hsla = TEXT.into();
    let paper: Hsla = ON_TEXT.into();
    let base = m.font.clone();
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
            if c.faint == 1 {
                fg = fg.opacity(0.5);
            }
            let ch = c.ch();
            if !keeps_its_colors(ch) {
                fg = legible(fg, bg.unwrap_or(paper));
            }
            if c.selected == 1 {
                bg = Some(SELECTION.into());
            }
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
    use super::{Metrics, color, grid_top, is_icon, keeps_its_colors, legible, term_font};
    use gpui_kit::{Bounds, black, point, px, size, white};

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
    fn text_that_vanishes_into_its_background_paints_white_on_dark_and_black_on_light() {
        assert_eq!(legible(color([80, 80, 80]), color([70, 70, 70])), white());
        assert_eq!(legible(color([80, 80, 80]), color([55, 55, 55])), white());
        assert_eq!(legible(color([175, 175, 175]), color([240, 240, 240])), black());
    }

    #[test]
    fn dim_but_readable_text_keeps_its_color() {
        let hint = color([80, 80, 80]);
        assert_eq!(legible(hint, color([23, 23, 23])), hint);
        let faint = color([235, 235, 235]).opacity(0.5);
        assert_eq!(legible(faint, color([23, 23, 23])), faint);
    }

    #[test]
    fn faint_text_that_vanishes_stays_faint_once_legible() {
        assert_eq!(legible(color([60, 60, 60]).opacity(0.5), color([70, 70, 70])), white().opacity(0.5));
    }

    #[test]
    fn shapes_and_spaces_keep_their_colors_but_text_does_not() {
        assert!(keeps_its_colors(' ') && keeps_its_colors('█') && keeps_its_colors('─') && keeps_its_colors('\u{1FB00}'));
        assert!(!keeps_its_colors('a') && !keeps_its_colors('❯') && !keeps_its_colors('đ'));
    }

    #[test]
    fn terminal_font_turns_off_every_ligature_feature_unless_ligatures_are_on() {
        let font = term_font("Geist Mono", false);
        let features = font.features.tag_value_list();
        for tag in ["liga", "calt", "dlig"] {
            assert!(features.contains(&(tag.to_string(), 0)), "{tag}");
        }
        assert!(term_font("Geist Mono", true).features.tag_value_list().is_empty());
    }

    #[test]
    fn default_settings_keep_todays_terminal_text() {
        let m = Metrics::of(&store::prefs::Terminal::default());
        assert_eq!((m.font.family.as_ref(), m.size, m.line, m.pad), ("Geist Mono", 13., 17., 4.));
    }

    #[test]
    fn a_terminal_without_its_own_font_follows_the_code_font() {
        let m = Metrics::of(&store::prefs::Terminal { font: None, ..Default::default() });
        assert_eq!(m.font.family, theme::code_font());
    }
}
