use crate::desktop::Desktop;
use gpui_kit::*;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Instant;
use theme::*;

const CELL_W: f32 = 10.;
const CELL_H: f32 = 18.;
const LENS_RADIUS: f32 = 210.;
const RAMP: [&str; 7] = ["·", ":", "-", "=", "+", "*", "#"];

/// A dot grid behind a prompt page, hiding a contour map the cursor lights up.
#[derive(Default)]
pub(crate) struct Field {
    lens: Lens,
    stepped: Option<Instant>,
    levels: Rc<RefCell<Levels>>,
}

impl Field {
    fn aim(&mut self, at: Point<Pixels>) {
        self.lens.aim(f32::from(at.x), f32::from(at.y), true);
    }

    fn leave(&mut self) {
        let (x, y) = (self.lens.tx, self.lens.ty);
        self.lens.aim(x, y, false);
    }

    /// Unlights the lens, whose cursor may have left while the page wasn't drawn.
    pub(crate) fn hide(&mut self) {
        (self.lens, self.stepped) = (Lens::default(), None);
    }

    /// Eases the lens by the time since the last frame; true while it still moves.
    fn tick(&mut self, now: Instant, reduce_motion: bool) -> bool {
        let dt = self.stepped.map_or(0., |t| now.duration_since(t).as_secs_f32().min(0.064));
        if reduce_motion {
            self.lens.settle();
        }
        let moving = self.lens.step(dt);
        self.stepped = moving.then_some(now);
        moving
    }

    /// Paints the field over its parent, fading out around `content`'s bounds, set by the time this paints.
    fn view(&self, content: Rc<Cell<Option<Bounds<Pixels>>>>, moving: bool) -> impl IntoElement {
        let (lens, levels) = (self.lens, self.levels.clone());
        canvas(
            |_, _, _| {},
            move |bounds, _, window, cx| {
                let Some(content) = content.get() else { return };
                let cols = (f32::from(bounds.size.width) / CELL_W).ceil() as usize;
                let rows = (f32::from(bounds.size.height) / CELL_H).ceil() as usize;
                let mut levels = levels.borrow_mut();
                let map = levels.sized(cols, rows);
                let rect = (f32::from(content.left()), f32::from(content.top()), f32::from(content.right()), f32::from(content.bottom()));
                let (faint, soft, strong) = (Hsla::from(TEXT_6), Hsla::from(TEXT_5), Hsla::from(TEXT_2));
                let mono = font(code_font());
                // The grid rounds up, so its last row and column would spill past the page.
                window.with_content_mask(Some(ContentMask { bounds }), |window| {
                    for row in 0..rows {
                        for col in 0..cols {
                            let x = f32::from(bounds.origin.x) + col as f32 * CELL_W + CELL_W / 2.;
                            let y = f32::from(bounds.origin.y) + row as f32 * CELL_H + CELL_H / 2.;
                            match mark(map[row * cols + col], lens.glow(x, y), mask(outside(rect, x, y))) {
                                Mark::Hidden => {}
                                Mark::Dot { alpha } => {
                                    let dot = Bounds::new(point(px(x - 1.), px(y - 1.)), size(px(2.), px(2.)));
                                    window.paint_quad(fill(dot, faint.opacity(alpha)).corner_radii(px(1.)));
                                }
                                Mark::Glyph { glyph, shade, alpha } => {
                                    let color = mix(soft, strong, shade).opacity(alpha);
                                    let run = TextRun { len: glyph.len(), font: mono.clone(), color, background_color: None, underline: None, strikethrough: None };
                                    let shaped = window.text_system().shape_line(glyph.into(), px(12.), &[run], None);
                                    let origin = point(px(x) - shaped.width / 2., px(y - CELL_H / 2.));
                                    shaped.paint(origin, px(CELL_H), TextAlign::Left, None, window, cx).ok();
                                }
                            }
                        }
                    }
                });
                if moving {
                    window.request_animation_frame();
                }
            },
        )
        .absolute()
        .inset_0()
    }
}

impl Desktop {
    /// A prompt page: `content`, a column at most 600px wide, centred over `field`, which fades around it.
    pub(crate) fn with_field(&mut self, field: fn(&mut Desktop) -> &mut Field, id: &'static str, content: Div, cx: &mut Context<Self>) -> Div {
        let moving = field(self).tick(Instant::now(), cx.reduce_motion());
        let bounds = Rc::new(Cell::new(None));
        let measured = bounds.clone();
        let body = div()
            .id(id)
            .relative()
            .flex_1()
            .flex()
            .flex_col()
            .items_center()
            .justify_center()
            .pt(px(72.))
            .px(px(24.))
            .pb(px(56.))
            .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, _, cx| {
                field(this).aim(ev.position);
                cx.notify();
            }))
            .on_hover(cx.listener(move |this, over: &bool, _, cx| {
                if !over {
                    field(this).leave();
                    cx.notify();
                }
            }))
            .child(field(self).view(bounds, moving))
            // Measured before the field paints, so its dots clear the content.
            .child(content.relative().w_full().max_w(px(600.)).child(canvas(move |b, _, _| measured.set(Some(b)), |_, _, _, _| {}).absolute().inset_0()));
        div().flex_1().flex().flex_col().child(body)
    }
}

/// What a cell shows: nothing near the content, a dot, or under the lens a glyph of the map.
#[derive(Debug, PartialEq)]
enum Mark {
    Hidden,
    Dot { alpha: f32 },
    /// `shade` runs from the soft colour (0) to the strong one (1).
    Glyph { glyph: &'static str, shade: f32, alpha: f32 },
}

/// The mark for a cell at map `level`, lit `glow` by the lens, and `mask` from 0 at the content to 1 clear of it.
fn mark(level: f32, glow: f32, mask: f32) -> Mark {
    if mask < 0.02 {
        return Mark::Hidden;
    }
    let lit = glow * mask;
    let strength = level * lit;
    if strength < 0.06 {
        return Mark::Dot { alpha: mask * (0.35 + level * 0.65) };
    }
    let glyph = RAMP[((strength * RAMP.len() as f32) as usize).min(RAMP.len() - 1)];
    Mark::Glyph { glyph, shade: (strength * 1.3).min(1.), alpha: (0.25 + lit).min(1.) }
}

/// The cursor's lens, in window pixels, with `k` how far it has faded in; `t…` is where it's heading.
#[derive(Clone, Copy, Default)]
struct Lens {
    x: f32,
    y: f32,
    k: f32,
    tx: f32,
    ty: f32,
    tk: f32,
}

impl Lens {
    fn aim(&mut self, x: f32, y: f32, on: bool) {
        (self.tx, self.ty, self.tk) = (x, y, if on { 1. } else { 0. });
        // An unlit lens appears where the cursor is rather than sliding in from where it faded out.
        if self.k < 0.01 {
            (self.x, self.y) = (x, y);
        }
    }

    fn settle(&mut self) {
        (self.x, self.y, self.k) = (self.tx, self.ty, self.tk);
    }

    /// Eases toward the target over `dt` seconds; true until it's there.
    fn step(&mut self, dt: f32) -> bool {
        let (follow, fade) = (1. - (-23. * dt).exp(), 1. - (-13. * dt).exp());
        self.x += (self.tx - self.x) * follow;
        self.y += (self.ty - self.y) * follow;
        self.k += (self.tk - self.k) * fade;
        (self.tx - self.x).abs() + (self.ty - self.y).abs() > 0.4 || (self.tk - self.k).abs() > 0.004
    }

    /// How brightly the lens lights a point: 1 at its centre, 0 from `LENS_RADIUS` out.
    fn glow(&self, x: f32, y: f32) -> f32 {
        let d = (x - self.x).hypot(y - self.y);
        if self.k < 0.01 || d >= LENS_RADIUS { 0. } else { smooth(1. - d / LENS_RADIUS) * self.k }
    }
}

/// Each cell's height on the hidden contour map, kept until the grid's size changes.
#[derive(Default)]
struct Levels {
    cols: usize,
    level: Vec<f32>,
}

impl Levels {
    fn sized(&mut self, cols: usize, rows: usize) -> &[f32] {
        if self.cols != cols || self.level.len() != cols * rows {
            self.cols = cols;
            self.level = (0..rows).flat_map(|r| (0..cols).map(move |c| level(c, r))).collect();
        }
        &self.level
    }
}

/// Near 1 on a contour line, rising gently with height in between.
fn level(col: usize, row: usize) -> f32 {
    let v = fbm(col as f32 * 0.035, row as f32 * 0.065) * 7.;
    let contour = 1. - ((v - v.round()).abs() * 4.5).min(1.);
    (contour * 0.8 + v / 7. * 0.35).clamp(0., 1.)
}

fn mask(distance: f32) -> f32 {
    smooth(((distance - 24.) / 110.).clamp(0., 1.))
}

/// How far `(x, y)` lies outside `(left, top, right, bottom)`; 0 inside.
fn outside((left, top, right, bottom): (f32, f32, f32, f32), x: f32, y: f32) -> f32 {
    (left - x).max(x - right).max(0.).hypot((top - y).max(y - bottom).max(0.))
}

fn mix(a: Hsla, b: Hsla, t: f32) -> Hsla {
    let lerp = |a: f32, b: f32| a + (b - a) * t;
    Hsla { h: b.h, s: lerp(a.s, b.s), l: lerp(a.l, b.l), a: lerp(a.a, b.a) }
}

fn smooth(t: f32) -> f32 {
    t * t * (3. - 2. * t)
}

fn fbm(x: f32, y: f32) -> f32 {
    noise(x, y) * 0.55 + noise(x * 2.1, y * 2.1) * 0.3 + noise(x * 4.3, y * 4.3) * 0.15
}

/// Smoothly interpolated value noise in 0..1.
fn noise(x: f32, y: f32) -> f32 {
    let (xi, yi) = (x.floor() as i32, y.floor() as i32);
    let (u, v) = (smooth(x - xi as f32), smooth(y - yi as f32));
    let (a, b, c, d) = (hash(xi, yi), hash(xi + 1, yi), hash(xi, yi + 1), hash(xi + 1, yi + 1));
    a + (b - a) * u + (c - a) * v + (a - b - c + d) * u * v
}

fn hash(x: i32, y: i32) -> f32 {
    let h = x.wrapping_mul(374761393).wrapping_add(y.wrapping_mul(668265263)) as u32;
    let h = (h ^ (h >> 13)).wrapping_mul(1274126177);
    (h ^ (h >> 16)) as f32 / 4294967296.
}

#[cfg(test)]
mod tests {
    use super::{Field, LENS_RADIUS, Lens, Levels, Mark, level, mark, mask, noise, outside};
    use gpui_kit::{point, px};
    use std::time::{Duration, Instant};

    #[test]
    fn noise_stays_within_zero_and_one() {
        for i in 0..2000 {
            let n = noise(i as f32 * 0.37 - 300., i as f32 * 0.91 - 700.);
            assert!((0. ..=1.).contains(&n), "{n}");
        }
    }

    #[test]
    fn the_map_is_the_same_wherever_the_grid_is_resized() {
        let mut levels = Levels::default();
        let wide = levels.sized(80, 10)[3 * 80 + 5];
        assert_eq!(levels.sized(40, 20)[3 * 40 + 5], wide);
        assert_eq!(wide, level(5, 3));
    }

    #[test]
    fn dots_fade_out_near_the_content() {
        let rect = (100., 100., 300., 200.);
        assert_eq!(mask(outside(rect, 200., 150.)), 0.);
        assert_eq!(mask(outside(rect, 310., 150.)), 0.);
        assert_eq!(mask(outside(rect, 450., 150.)), 1.);
        assert!(mask(outside(rect, 380., 150.)) > 0. && mask(outside(rect, 380., 150.)) < 1.);
    }

    #[test]
    fn a_lens_appears_where_the_cursor_enters_and_fades_in() {
        let mut lens = Lens::default();
        lens.aim(500., 300., true);
        assert_eq!((lens.x, lens.y), (500., 300.));
        assert_eq!(lens.glow(500., 300.), 0.);
        while lens.step(1. / 60.) {}
        assert!(lens.glow(500., 300.) > 0.99);
        assert_eq!(lens.glow(500. + LENS_RADIUS, 300.), 0.);
    }

    #[test]
    fn a_lit_lens_trails_the_cursor_then_catches_up() {
        let mut lens = Lens::default();
        lens.aim(0., 0., true);
        while lens.step(1. / 60.) {}
        lens.aim(100., 0., true);
        assert!(lens.step(1. / 60.));
        assert!(lens.x > 0. && lens.x < 100.);
        while lens.step(1. / 60.) {}
        assert!((lens.x - 100.).abs() < 0.4);
    }

    #[test]
    fn a_settled_lens_jumps_to_its_target() {
        let mut lens = Lens::default();
        lens.aim(10., 10., true);
        lens.aim(90., 40., false);
        lens.settle();
        assert!(!lens.step(0.));
        assert_eq!((lens.x, lens.y, lens.k), (90., 40., 0.));
    }

    #[test]
    fn the_lens_shows_the_map_as_glyphs_denser_where_it_is_higher() {
        assert_eq!(mark(1., 1., 0.), Mark::Hidden);
        assert!(matches!(mark(0.5, 0., 1.), Mark::Dot { .. }));
        assert!(matches!(mark(0.02, 1., 1.), Mark::Dot { .. }));
        assert!(matches!(mark(0.3, 1., 1.), Mark::Glyph { glyph: ":" | "-", .. }));
        assert!(matches!(mark(1., 1., 1.), Mark::Glyph { glyph: "#", alpha: 1., .. }));
    }

    #[test]
    fn a_field_eases_until_its_lens_settles_then_stops_asking_for_frames() {
        let mut field = Field::default();
        let start = Instant::now();
        field.aim(point(px(300.), px(200.)));
        let mut frames = 0;
        while field.tick(start + Duration::from_millis(16 * frames), false) {
            frames += 1;
        }
        assert!(frames > 1 && frames < 120, "{frames}");
        assert!(field.lens.glow(300., 200.) > 0.99);
        field.leave();
        assert!(!field.tick(start, true));
        assert_eq!(field.lens.glow(300., 200.), 0.);
    }

    #[test]
    fn a_hidden_field_is_unlit_when_its_page_returns() {
        let mut field = Field::default();
        field.aim(point(px(300.), px(200.)));
        field.tick(Instant::now(), true);
        field.hide();
        assert_eq!(field.lens.glow(300., 200.), 0.);
        assert!(!field.tick(Instant::now(), false));
    }
}
