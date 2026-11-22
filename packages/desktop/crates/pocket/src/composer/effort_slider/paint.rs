use super::effects::Light;
use super::style::{EffortStyle, mix};
use super::{EffortSlider, Frame, intensity};
use gpui_kit::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;
use theme::*;

pub(super) const HEIGHT: f32 = TOP + TRACK + 8. + LABEL_LINE;
const TRACK: f32 = 24.;
const THUMB: f32 = TRACK + 4.;
/// Room over the track for the thumb, which is taller and grows while dragged.
const TOP: f32 = 3.;
const LABEL_SIZE: f32 = 11.;
const LABEL_LINE: f32 = 14.;
const LABEL_GAP: f32 = 8.;
const MARK: f32 = 4.;
const SWEEP: f32 = 26.;
/// Each spark's heading in degrees, clockwise from the right, and how far it flies against the others.
const SPARKS: [(f32, f32); 7] = [(-160., 0.9), (-118., 1.), (-74., 0.8), (-32., 1.1), (16., 0.85), (122., 0.95), (176., 0.75)];
const SPARK_FLIGHT: f32 = 11.;

type Commit = Rc<dyn Fn(String, &mut App)>;

/// What the pointer handlers reach: the state they step, the focus a press takes, how a level reaches the agent, and how
/// the window is asked to redraw.
#[derive(Clone)]
pub(super) struct Wiring {
    pub(super) state: Rc<RefCell<EffortSlider>>,
    pub(super) focus: FocusHandle,
    pub(super) commit: Commit,
    pub(super) redraw: Rc<dyn Fn(&mut App)>,
}

/// What a press lands on.
#[derive(Debug, PartialEq)]
enum Press {
    Label(usize),
    Track { at: f32, on_thumb: bool },
}

#[derive(Clone, Copy)]
struct Geometry {
    left: f32,
    width: f32,
    top: f32,
}

impl Geometry {
    /// The thumb's centre `t` of the way along: the ends keep it inside the track's rounded caps.
    fn x(&self, t: f32) -> f32 {
        self.left + TRACK / 2. + t * (self.width - TRACK)
    }

    fn at(&self, x: f32) -> f32 {
        ((x - self.left - TRACK / 2.) / (self.width - TRACK).max(1.)).clamp(0., 1.)
    }

    fn cy(&self) -> f32 {
        self.top + TOP + TRACK / 2.
    }

    fn pill(&self, from: f32, to: f32) -> Bounds<Pixels> {
        Bounds::from_corners(point(px(from), px(self.top + TOP)), point(px(to), px(self.top + TOP + TRACK)))
    }

    /// A press at `at`: a name it lands on picks that level; anywhere else drags, holding the thumb where it was grabbed.
    fn aim(&self, thumb: f32, hits: &[(Bounds<Pixels>, usize)], at: Point<Pixels>) -> Press {
        if let Some(&(_, i)) = hits.iter().find(|(b, _)| b.contains(&at)) {
            return Press::Label(i);
        }
        let (x, y) = (f32::from(at.x), f32::from(at.y));
        Press::Track { at: self.at(x), on_thumb: (x - thumb).abs() <= THUMB / 2. && (y - self.cy()).abs() <= THUMB / 2. }
    }
}

fn circle(centre: (f32, f32), r: f32) -> Bounds<Pixels> {
    Bounds::centered_at(point(px(centre.0), px(centre.1)), size(px(2. * r), px(2. * r)))
}

/// The track, its labels and the thumb, painted from `frame`. Presses on it, and the drags that follow wherever the
/// pointer goes, step the slider.
pub(super) fn track(wiring: Wiring, frame: Frame, style: EffortStyle) -> impl IntoElement {
    canvas(
        |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
        move |bounds, hitbox, window, cx| {
            let geo = Geometry { left: f32::from(bounds.origin.x), width: f32::from(bounds.size.width), top: f32::from(bounds.origin.y) };
            let thumb = geo.x(frame.thumb);
            window.paint_quad(fill(geo.pill(geo.left, geo.left + geo.width), style.rail).corner_radii(px(TRACK / 2.)));
            if frame.shown.is_some() {
                let filled = geo.pill(geo.left, thumb + TRACK / 2.);
                window.paint_quad(fill(filled, linear_gradient(90., linear_color_stop(style.track, 0.), linear_color_stop(mix(style.track, style.light, 0.18), 1.))).corner_radii(px(TRACK / 2.)));
                for light in frame.lights.iter().flatten() {
                    glow(window, &geo, filled, thumb, light, style.light);
                }
            }
            let hits = {
                let s = wiring.state.borrow();
                marks(window, &geo, thumb, &frame, s.levels.len(), &style);
                labels(window, cx, &geo, bounds, &frame, &s.labels)
            };
            // Off the track the light colour is lost against a light theme's popover.
            let halo_color = if is_dark() { style.light } else { style.track };
            for light in frame.lights.iter().flatten() {
                halo(window, &geo, thumb, frame.lift, light, halo_color);
            }
            knob(window, &geo, thumb, &frame, wiring.focus.is_focused(window), &style);
            listen(window, hitbox, geo, thumb, hits, wiring.clone());
            if frame.moving {
                window.request_animation_frame();
            }
        },
    )
    .w_full()
    .h(px(HEIGHT))
}

/// The light inside the filled track: the bloom back from the thumb, then the sweeps across it.
fn glow(window: &mut Window, geo: &Geometry, filled: Bounds<Pixels>, thumb: f32, light: &Light, color: Hsla) {
    let (left, width) = (geo.left, f32::from(filled.size.width));
    let stop = |x: f32| (x - left) / width;
    if let Some((reach, alpha)) = light.bloom {
        // A gradient stays at its end colours past its stops, so this lights only the stretch behind the thumb.
        let reach = reach.max(1.);
        let c = color.opacity(alpha);
        window.paint_quad(fill(filled, linear_gradient(90., linear_color_stop(c.opacity(0.), stop(thumb - reach)), linear_color_stop(c, stop(thumb)))).corner_radii(px(TRACK / 2.)));
        let core = Bounds::from_corners(point(px((thumb - reach * 0.5).max(left + 2.)), px(geo.cy() - TRACK / 4.)), point(filled.right(), px(geo.cy() + TRACK / 4.)));
        let at = (thumb - f32::from(core.left())) / f32::from(core.size.width).max(1.);
        window.paint_quad(fill(core, linear_gradient(90., linear_color_stop(c.opacity(0.), 0.), linear_color_stop(c, at))).corner_radii(px(TRACK / 4.)));
    }
    for &(along, alpha) in light.sweeps.iter().flatten() {
        band(window, filled, left - SWEEP + along * (thumb - left + SWEEP), color.opacity(alpha));
    }
}

/// A feathered band of `color` across `filled`, brightest at `centre` and fading over `SWEEP` either side along a raised
/// cosine. A gradient has two stops, so it's drawn in straight pieces: two would leave a hard ridge at the centre. Each
/// piece is a quad of the fill's own pill shape cut to its stretch, so the band never spills past the caps.
fn band(window: &mut Window, filled: Bounds<Pixels>, centre: f32, color: Hsla) {
    const PIECES: usize = 8;
    let (left, right) = (f32::from(filled.left()), f32::from(filled.right()));
    let stop = |x: f32| (x - left) / (right - left);
    // A mask is widened to whole device pixels, so cuts between them would overlap and paint a bright seam.
    let scale = window.scale_factor();
    let snap = |x: f32| (x * scale).round() / scale;
    let at = |k: usize| {
        let u = -1. + 2. * k as f32 / PIECES as f32;
        (centre + u * SWEEP, 0.5 * (1. + (std::f32::consts::PI * u).cos()))
    };
    for k in 0..PIECES {
        let ((from, a), (to, b)) = (at(k), at(k + 1));
        let cut = Bounds::from_corners(point(px(snap(from.max(left))), filled.top()), point(px(snap(to.min(right))), filled.bottom()));
        if cut.size.width <= px(0.) {
            continue;
        }
        window.with_content_mask(Some(ContentMask { bounds: cut }), |window| {
            window.paint_quad(fill(filled, linear_gradient(90., linear_color_stop(color.opacity(a), stop(from)), linear_color_stop(color.opacity(b), stop(to)))).corner_radii(px(TRACK / 2.)));
        });
    }
}

fn halo(window: &mut Window, geo: &Geometry, thumb: f32, lift: f32, light: &Light, color: Hsla) {
    let edge = THUMB * lift / 2.;
    if let Some((spread, alpha)) = light.ring {
        let r = edge + 1. + spread;
        window.paint_quad(fill(circle((thumb, geo.cy()), r), transparent_black()).border_widths(px(1.)).border_color(color.opacity(alpha)).corner_radii(px(r)));
    }
    if let Some((flight, alpha)) = light.sparks {
        for (heading, reach) in SPARKS {
            let (dy, dx) = heading.to_radians().sin_cos();
            let d = edge + 2. + SPARK_FLIGHT * reach * flight;
            let at = point(px(thumb + dx * d), px(geo.cy() + dy * d));
            window.paint_quad(fill(Bounds::centered_at(at, size(px(1.6), px(1.6))), color.opacity(alpha * (1. - 0.4 * flight))).corner_radii(px(0.8)));
        }
    }
}

fn marks(window: &mut Window, geo: &Geometry, thumb: f32, frame: &Frame, n: usize, style: &EffortStyle) {
    for i in 0..n {
        let x = geo.x(intensity(i, n));
        if (x - thumb).abs() < THUMB / 2. {
            continue;
        }
        let color = if frame.shown.is_some() && x < thumb { style.mark_lit } else { style.mark };
        window.paint_quad(fill(Bounds::centered_at(point(px(x), px(geo.cy())), size(px(MARK), px(MARK))), color).corner_radii(px(MARK / 2.)));
    }
}

/// The focus ring shows only once keys move the thumb, not after every click.
fn knob(window: &mut Window, geo: &Geometry, thumb: f32, frame: &Frame, focused: bool, style: &EffortStyle) {
    let d = THUMB * frame.lift;
    let knob = Bounds::centered_at(point(px(thumb), px(geo.cy())), size(px(d), px(d)));
    window.paint_drop_shadows(knob, Corners::all(px(d / 2.)), &[ui::shadow(hsla(0., 0., 0., 0.2), 1., 3.), ui::shadow(hsla(0., 0., 0., 0.12), 3., 10.)]);
    let shade = linear_gradient(180., linear_color_stop(hsla(0., 0., 1., 1.), 0.), linear_color_stop(rgb(0xeef1f6), 1.));
    let edge = if is_dark() { transparent_black() } else { hsla(0., 0., 0., 0.06) };
    window.paint_quad(fill(knob, shade).border_widths(px(0.5)).border_color(edge).corner_radii(px(d / 2.)));
    if focused && frame.keyed {
        let r = d / 2. + 3.;
        window.paint_quad(fill(circle((thumb, geo.cy()), r), transparent_black()).border_widths(px(2.)).border_color(style.track.opacity(0.6)).corner_radii(px(r)));
    }
}

/// Each level's name under its dot, the shown one brightening in. Too many to fit side by side, only the ends and the
/// shown one stay. Returns where each name painted can be clicked.
fn labels(window: &mut Window, cx: &mut App, geo: &Geometry, bounds: Bounds<Pixels>, frame: &Frame, labels: &[SharedString]) -> Vec<(Bounds<Pixels>, usize)> {
    let n = labels.len();
    let font = font(ui_font());
    let (dim, bright) = (Hsla::from(TEXT_4), Hsla::from(TEXT));
    let top = geo.top + TOP + TRACK + 8.;
    let placed: Vec<(f32, ShapedLine)> = labels
        .iter()
        .enumerate()
        .map(|(i, text)| {
            let lit = if Some(i) == frame.shown { frame.label } else if Some(i) == frame.was { 1. - frame.label } else { 0. };
            let run = TextRun { len: text.len(), font: font.clone(), color: mix(dim, bright, lit), background_color: None, underline: None, strikethrough: None };
            let line = window.text_system().shape_line(text.clone(), px(LABEL_SIZE), &[run], None);
            let w = f32::from(line.width);
            let left = (geo.x(intensity(i, n)) - w / 2.).clamp(f32::from(bounds.left()), (f32::from(bounds.right()) - w).max(f32::from(bounds.left())));
            (left, line)
        })
        .collect();
    let span = |i: usize| (placed[i].0, placed[i].0 + f32::from(placed[i].1.width));
    let apart = |a: usize, b: usize| {
        let ((al, ar), (bl, br)) = (span(a), span(b));
        ar + LABEL_GAP <= bl || br + LABEL_GAP <= al
    };
    let crowded = (1..n).any(|i| !apart(i - 1, i));
    let shows = |i: usize| !crowded || Some(i) == frame.shown || ((i == 0 || i + 1 == n) && frame.shown.is_none_or(|s| apart(i, s)));
    let mut hits = Vec::new();
    for (i, (left, line)) in placed.iter().enumerate().filter(|(i, _)| shows(*i)) {
        let rise = if Some(i) == frame.shown && !frame.still { 1.5 * (1. - frame.label) } else { 0. };
        _ = line.paint(point(px(*left), px(top + rise)), px(LABEL_LINE), TextAlign::Left, None, window, cx);
        hits.push((Bounds::new(point(px(left - 4.), px(top - 2.)), size(line.width + px(8.), px(LABEL_LINE + 4.))), i));
    }
    hits
}

/// A press on a name picks it; a press elsewhere starts a drag, which follows the pointer outside the slider until the
/// button comes up, then commits the level it ended on.
fn listen(window: &mut Window, hitbox: Hitbox, geo: Geometry, thumb: f32, hits: Vec<(Bounds<Pixels>, usize)>, wiring: Wiring) {
    let release = {
        let w = wiring.clone();
        move |cx: &mut App| {
            let level = w.state.borrow_mut().release();
            if let Some(level) = level {
                (w.commit)(level, cx);
            }
            (w.redraw)(cx);
        }
    };
    window.on_mouse_event({
        let w = wiring.clone();
        move |e: &MouseDownEvent, phase, window, cx| {
            if phase != DispatchPhase::Bubble || e.button != MouseButton::Left || !hitbox.is_hovered(window) {
                return;
            }
            cx.stop_propagation();
            window.focus(&w.focus, cx);
            let now = Instant::now();
            let picked = match geo.aim(thumb, &hits, e.position) {
                Press::Label(i) => w.state.borrow_mut().choose(i, now, true),
                Press::Track { at, on_thumb } => {
                    w.state.borrow_mut().press(at, on_thumb, now);
                    None
                }
            };
            if let Some(level) = picked {
                (w.commit)(level, cx);
            }
            (w.redraw)(cx);
        }
    });
    window.on_mouse_event({
        let (w, release) = (wiring.clone(), release.clone());
        move |e: &MouseMoveEvent, phase, _, cx| {
            if phase != DispatchPhase::Bubble || !w.state.borrow().dragging() {
                return;
            }
            // A button let go where no event reached us, like over another app, ends the drag on the next move.
            if e.pressed_button != Some(MouseButton::Left) {
                return release(cx);
            }
            w.state.borrow_mut().drag_to(geo.at(f32::from(e.position.x)), Instant::now());
            (w.redraw)(cx);
        }
    });
    window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
        if phase == DispatchPhase::Bubble && e.button == MouseButton::Left && wiring.state.borrow().dragging() {
            release(cx);
        }
    });
}

#[cfg(test)]
mod tests {
    use super::{Geometry, Press, THUMB, TOP, TRACK};
    use gpui_kit::{Bounds, point, px, size};

    #[test]
    fn a_press_on_a_name_picks_it_and_one_on_the_thumb_grabs_it() {
        let geo = Geometry { left: 0., width: 224., top: 0. };
        let thumb = geo.x(0.5);
        let name = Bounds::new(point(px(150.), px(40.)), size(px(40.), px(18.)));
        let hits = [(name, 3)];
        assert_eq!(geo.aim(thumb, &hits, point(px(160.), px(45.))), Press::Label(3));
        let cy = TOP + TRACK / 2.;
        assert_eq!(geo.aim(thumb, &hits, point(px(thumb + THUMB / 2. - 1.), px(cy))), Press::Track { at: geo.at(thumb + THUMB / 2. - 1.), on_thumb: true });
        assert_eq!(geo.aim(thumb, &hits, point(px(TRACK / 2.), px(cy))), Press::Track { at: 0., on_thumb: false });
        assert_eq!(geo.aim(thumb, &hits, point(px(-50.), px(cy))), Press::Track { at: 0., on_thumb: false }, "past the end clamps");
    }
}
