//! Arc's theme editor for a space: up to three colours on a pad, light or dark, five pages of swatches, intensity and grain.

use super::wash::{PAD, colour, dot, dots_image, grain_image};
use crate::glide::Glide;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::f32::consts::TAU;
use std::time::{Duration, Instant};
use store::prefs::sidebar::{LookMode, SpaceLook};
use theme::wash::{self, Dot, Hls};
use theme::{WHITE, icon};

const WIDTH: f32 = 356.;
const INSET: f32 = 8.;
const HANDLE: f32 = 45.;
/// How far out a handle's centre goes: its ring stays inside the pad.
const REACH: f32 = PAD / 2. * 0.85;
const CENTRE: (f32, f32) = (INSET + PAD / 2., INSET + PAD / 2.);
const SWATCH: f32 = 24.;
const PAD_RECT: Bounds<f32> = Bounds { origin: Point { x: INSET, y: INSET }, size: Size { width: PAD, height: PAD } };
/// The mode buttons and −/+ on the pad, which take their own clicks.
const BUTTONS: [Bounds<f32>; 2] = [
    Bounds { origin: Point { x: CENTRE.0 - 57., y: INSET + 24. }, size: Size { width: 114., height: 32. } },
    Bounds { origin: Point { x: CENTRE.0 - 34., y: INSET + PAD - 17. - 28. }, size: Size { width: 68., height: 28. } },
];
const ROW: f32 = 40.;
const WAVE_WIDTH: f32 = 234.;
/// The wave slider and the grain dial, as card coordinates.
const BOTTOM: f32 = INSET + PAD + 13. + ROW + 15.;
const WAVE: Bounds<f32> = Bounds { origin: Point { x: INSET + 10., y: BOTTOM }, size: Size { width: WAVE_WIDTH, height: 72. } };
const DIAL: Bounds<f32> = Bounds { origin: Point { x: INSET + 10. + WAVE_WIDTH + 10., y: BOTTOM }, size: Size { width: 72., height: 72. } };
const THUMB: Size<f32> = Size { width: 26., height: 58. };
/// Where the wave starts, and where the thumb's centre sits at none and the most.
const WAVE_START: f32 = 7.;
const WAVE_FROM: f32 = WAVE_START + THUMB.width / 2.;
const WAVE_SPAN: f32 = WAVE_WIDTH - THUMB.width - WAVE_START;
const DIAL_DOTS: usize = 24;
/// How long a colour takes to spring to a swatch's place, or back inside the pad.
const SETTLE: f32 = 0.35;
const OPEN: Duration = Duration::from_millis(220);

pub(crate) enum CardEvent {
    Changed { key: String, look: SpaceLook },
}

/// What a press is dragging: a colour by the point it was grabbed at, the wave's thumb, or the dial by the angle it was at.
#[derive(Clone, Copy, Debug, PartialEq)]
enum Drag {
    Colour { i: usize, grab: (f32, f32) },
    Intensity { grab: f32 },
    Grain { bearing: f32, angle: f32 },
}

pub(crate) struct ThemeCard {
    open: bool,
    key: String,
    look: SpaceLook,
    page: usize,
    selected: usize,
    drag: Option<Drag>,
    /// Where each colour shows, springing to where it is.
    shown: Vec<[Glide; 2]>,
    opened: usize,
}

impl EventEmitter<CardEvent> for ThemeCard {}

fn on_card(x: f32, y: f32) -> (f32, f32) {
    (CENTRE.0 + x * REACH, CENTRE.1 + y * REACH)
}

fn on_pad((x, y): (f32, f32)) -> (f32, f32) {
    wash::at(x - CENTRE.0, y - CENTRE.1, REACH)
}

/// The colour a press at `at` grabs: the topmost whose handle it lands on.
fn colour_at(at: (f32, f32), shown: &[(f32, f32)]) -> Option<usize> {
    shown.iter().rposition(|&(x, y)| {
        let (cx, cy) = on_card(x, y);
        (at.0 - cx).hypot(at.1 - cy) <= HANDLE / 2.
    })
}

fn thumb_x(v: f32) -> f32 {
    WAVE.origin.x + WAVE_FROM + v.clamp(0., 1.) * WAVE_SPAN
}

fn intensity_at(x: f32) -> f32 {
    ((x - WAVE.origin.x - WAVE_FROM) / WAVE_SPAN).clamp(0., 1.)
}

fn inside(b: Bounds<f32>, (x, y): (f32, f32)) -> bool {
    x >= b.origin.x && x <= b.origin.x + b.size.width && y >= b.origin.y && y <= b.origin.y + b.size.height
}

fn dial_centre() -> (f32, f32) {
    (DIAL.origin.x + DIAL.size.width / 2., DIAL.origin.y + DIAL.size.height / 2.)
}

/// What a press at `at` starts dragging, given where the colours show and the look's intensity and grain.
fn press(at: (f32, f32), shown: &[(f32, f32)], selected: usize, look: &SpaceLook) -> Option<Drag> {
    if let Some(i) = colour_at(at, shown) {
        let (cx, cy) = on_card(shown[i].0, shown[i].1);
        return Some(Drag::Colour { i, grab: (at.0 - cx, at.1 - cy) });
    }
    if inside(PAD_RECT, at) && !BUTTONS.iter().any(|b| inside(*b, at)) {
        return (!shown.is_empty()).then(|| Drag::Colour { i: selected.min(shown.len() - 1), grab: (0., 0.) });
    }
    if inside(WAVE, at) {
        let thumb = thumb_x(look.intensity);
        let on_thumb = (at.0 - thumb).abs() <= THUMB.width / 2.;
        return Some(Drag::Intensity { grab: if on_thumb { at.0 - thumb } else { 0. } });
    }
    let (cx, cy) = dial_centre();
    ((at.0 - cx).hypot(at.1 - cy) <= DIAL.size.width / 2.)
        .then(|| Drag::Grain { bearing: wash::bearing(at.0 - cx, at.1 - cy), angle: wash::dial_angle(look.grain) })
}

/// Opaque `a` laid over `under`: overlapping strokes and caps then don't stack.
fn over(under: Hsla, a: f32) -> Hsla {
    let (u, w) = (under.to_rgb(), 1. - a);
    Rgba { r: u.r * w + a, g: u.g * w + a, b: u.b * w + a, a: 1. }.into()
}

impl ThemeCard {
    pub(crate) fn new() -> Self {
        Self { open: false, key: String::new(), look: SpaceLook::default(), page: 0, selected: 0, drag: None, shown: Vec::new(), opened: 0 }
    }

    pub(crate) fn is_open(&self) -> bool {
        self.open
    }

    pub(crate) fn open(&mut self, key: String, look: SpaceLook, cx: &mut Context<Self>) {
        self.shown = look.colours.iter().map(|c| [Glide::from(c.x).response(SETTLE), Glide::from(c.y).response(SETTLE)]).collect();
        (self.key, self.look, self.selected, self.drag, self.open) = (key, look, 0, None, true);
        self.opened += 1;
        cx.notify();
    }

    pub(crate) fn shut(&mut self, cx: &mut Context<Self>) {
        if self.open {
            (self.open, self.drag) = (false, None);
            cx.notify();
        }
    }

    fn changed(&mut self, cx: &mut Context<Self>) {
        cx.emit(CardEvent::Changed { key: self.key.clone(), look: self.look.clone() });
        cx.notify();
    }

    fn shown_at(&self) -> Vec<(f32, f32)> {
        self.shown.iter().map(|[x, y]| (x.at(), y.at())).collect()
    }

    /// Steps each colour toward where it is; true while one still moves.
    fn tick(&mut self, now: Instant, reduce_motion: bool) -> bool {
        let dragged = match self.drag {
            Some(Drag::Colour { i, .. }) => Some(i),
            _ => None,
        };
        let mut moving = false;
        for (i, (glides, c)) in self.shown.iter_mut().zip(&self.look.colours).enumerate() {
            if Some(i) != dragged {
                moving |= glides[0].tick(c.x, now, reduce_motion) | glides[1].tick(c.y, now, reduce_motion);
            }
        }
        moving
    }

    /// Gives the look `colours`, each springing from where it shows, or a new one from where the first shows.
    fn set_colours(&mut self, colours: Vec<Dot>) {
        let from = self.shown_at().first().copied().unwrap_or((0., 0.));
        self.shown.truncate(colours.len());
        while self.shown.len() < colours.len() {
            self.shown.push([Glide::from(from.0).response(SETTLE), Glide::from(from.1).response(SETTLE)]);
        }
        self.look.colours = colours.into_iter().map(colour).collect();
        self.selected = self.selected.min(self.look.colours.len().saturating_sub(1));
    }

    fn mode(&mut self, mode: LookMode, cx: &mut Context<Self>) {
        self.look.mode = mode;
        self.changed(cx);
    }

    fn plus(&mut self, cx: &mut Context<Self>) {
        let mut colours: Vec<Dot> = self.look.colours.iter().map(|c| dot(*c)).collect();
        if colours.len() >= SpaceLook::MAX_COLOURS {
            return;
        }
        colours.push(wash::added(&colours));
        self.set_colours(colours);
        self.selected = self.look.colours.len() - 1;
        self.changed(cx);
    }

    fn minus(&mut self, cx: &mut Context<Self>) {
        if self.look.colours.is_empty() {
            return;
        }
        let i = self.selected.min(self.look.colours.len() - 1);
        self.look.colours.remove(i);
        self.shown.remove(i);
        self.selected = self.look.colours.len().saturating_sub(1);
        self.changed(cx);
    }

    fn swatch(&mut self, swatch: &[Hls], cx: &mut Context<Self>) {
        self.set_colours(swatch.iter().map(|c| Dot::from_hls(*c)).collect());
        self.changed(cx);
    }

    fn page_by(&mut self, by: isize, cx: &mut Context<Self>) {
        self.page = self.page.saturating_add_signed(by).min(wash::PAGE_NAMES.len() - 1);
        cx.notify();
    }

    fn press(&mut self, at: (f32, f32), cx: &mut Context<Self>) -> bool {
        let shown = self.shown_at();
        self.drag = press(at, &shown, self.selected, &self.look);
        match self.drag {
            Some(Drag::Colour { i, .. }) => {
                self.selected = i;
                // A press on the pad off every handle brings the selected one there.
                if colour_at(at, &shown).is_none() {
                    self.drag_to(at, cx);
                }
            }
            Some(Drag::Intensity { grab: 0. }) => {
                self.look.intensity = intensity_at(at.0);
                self.changed(cx);
            }
            _ => {}
        }
        cx.notify();
        self.drag.is_some()
    }

    fn drag_to(&mut self, at: (f32, f32), cx: &mut Context<Self>) {
        match self.drag {
            Some(Drag::Colour { i, grab }) => {
                let (x, y) = on_pad((at.0 - grab.0, at.1 - grab.1));
                self.look.colours[i] = colour(dot(self.look.colours[i]).moved(x, y));
                let (sx, sy) = wash::stretched(x, y);
                self.shown[i] = [Glide::from(sx).response(SETTLE), Glide::from(sy).response(SETTLE)];
            }
            Some(Drag::Intensity { grab }) => self.look.intensity = intensity_at(at.0 - grab),
            Some(Drag::Grain { bearing, angle }) => {
                let (cx_, cy_) = dial_centre();
                let now = wash::bearing(at.0 - cx_, at.1 - cy_);
                let angle = (angle + wash::turn(bearing, now)).clamp(wash::DIAL_FROM, wash::DIAL_FROM + wash::DIAL_SWEEP);
                self.look.grain = wash::dial_value(angle);
                self.drag = Some(Drag::Grain { bearing: now, angle });
            }
            None => return,
        }
        self.changed(cx);
    }

    fn release(&mut self, cx: &mut Context<Self>) {
        if self.drag.take().is_some() {
            cx.notify();
        }
    }

    fn pad(&self, cx: &mut Context<Self>) -> Div {
        let handles = self.shown_at().into_iter().zip(&self.look.colours).map(|((x, y), c)| {
            let (cx_, cy_) = on_card(x, y);
            div()
                .absolute()
                .left(px(cx_ - INSET - HANDLE / 2.))
                .top(px(cy_ - INSET - HANDLE / 2.))
                .size(px(HANDLE))
                .rounded_full()
                .border(px(4.5))
                .border_color(WHITE)
                .bg(wash::handle(dot(*c)))
                .shadow(vec![ui::shadow(rgba(0x00000040), 1., 4.)])
        });
        let modes =
            [(LookMode::Auto, "look-auto", "Auto"), (LookMode::Light, "look-light", "Light"), (LookMode::Dark, "look-dark", "Dark")].map(|(mode, glyph, _)| {
                let on = self.look.mode == mode;
                div()
                    .id(glyph)
                    .size(px(32.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .rounded(px(8.))
                    .cursor_pointer()
                    .when(on, |d| d.bg(rgba(0xffffff14)))
                    .child(icon(glyph, 19., rgba(if on { 0xffffffc7 } else { 0xffffff80 })))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.mode(mode, cx)))
            });
        let full = self.look.colours.len() >= SpaceLook::MAX_COLOURS;
        let empty = self.look.colours.is_empty();
        let step = |id: &'static str, glyph: &'static str, off: bool| {
            div().id(id).size(px(28.)).flex().items_center().justify_center().when(off, |d| d.opacity(0.35)).when(!off, |d| d.cursor_pointer()).child(icon(
                glyph,
                15.,
                rgba(0xffffff80),
            ))
        };
        div()
            .relative()
            .size(px(PAD))
            .rounded(px(10.))
            .bg(rgba(0x0000000d))
            .border(px(0.5))
            .border_color(rgba(0xffffff0d))
            .child(img(dots_image()).absolute().inset_0().size(px(PAD)).rounded(px(10.)))
            .child(div().absolute().top(px(24.)).left_0().right_0().flex().justify_center().gap(px(9.)).children(modes))
            .when(empty, |d| {
                d.child(
                    div()
                        .absolute()
                        .inset_0()
                        .flex()
                        .items_center()
                        .justify_center()
                        .text_size(px(12.))
                        .text_color(rgba(0xffffff73))
                        .child("Pick a swatch or press +"),
                )
            })
            .children(handles)
            .child(
                div()
                    .absolute()
                    .bottom(px(17.))
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .gap(px(12.))
                    .child(step("theme-minus", "minus", empty).when(!empty, |d| d.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.minus(cx)))))
                    .child(step("theme-plus", "plus", full).when(!full, |d| d.on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.plus(cx))))),
            )
    }

    fn swatches(&self, card: Hsla, cx: &mut Context<Self>) -> Div {
        let page = wash::page(self.page);
        let colours: Vec<Dot> = self.look.colours.iter().map(|c| dot(*c)).collect();
        let swatches = page.into_iter().enumerate().map(|(i, swatch)| {
            let on = wash::is_swatch(&swatch, &colours);
            let fill: Background = match swatch.as_slice() {
                [a, b] => linear_gradient(135., linear_color_stop(wash::swatch(*a), 0.), linear_color_stop(wash::swatch(*b), 1.)),
                [a, ..] => wash::swatch(*a).into(),
                [] => transparent_black().into(),
            };
            div()
                .id(("theme-swatch", i))
                .size(px(SWATCH))
                .rounded_full()
                .cursor_pointer()
                .bg(fill)
                .border(px(1.5))
                .border_color(rgba(0x00000029))
                .when(on, |d| d.shadow(vec![ui::ring(rgba(0xffffffa6), 3.5), ui::ring(card, 2.)]))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.swatch(&swatch, cx)))
        });
        let pager = |id: &'static str, glyph: &'static str, by: isize, off: bool| {
            div()
                .id(id)
                .w(px(30.))
                .h(px(32.))
                .flex()
                .items_center()
                .justify_center()
                .child(icon(glyph, 17., rgba(if off { 0xffffff4d } else { 0xffffff99 })))
                .when(!off, |d| d.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.page_by(by, cx))))
        };
        div()
            .mt(px(13.))
            .h(px(ROW))
            .flex()
            .items_center()
            .justify_between()
            .child(pager("theme-prev", "back", -1, self.page == 0))
            .child(div().flex().items_center().gap(px(5.5)).children(swatches))
            .child(pager("theme-next", "chevron-right", 1, self.page + 1 == wash::PAGE_NAMES.len()))
    }

    fn wave(&self, card: Hsla) -> Div {
        let v = self.look.intensity;
        let (stroke, track) = (over(card, 0.55), rgba(0xffffff12));
        let paint = canvas(
            |_, _, _| (),
            move |b, _, window, _| {
                let (ox, oy) = (f32::from(b.origin.x), f32::from(b.origin.y));
                let mid = oy + WAVE.size.height / 2.;
                window.paint_quad(fill(Bounds { origin: point(px(ox), px(mid - 7.5)), size: size(px(WAVE.size.width), px(15.)) }, track).corner_radii(px(7.5)));
                let (start, end) = (ox + WAVE_START, ox + WAVE_FROM + v * WAVE_SPAN);
                let (amp, period) = (14. * (0.25 + 0.75 * v), 47.);
                let y = |x: f32| mid - amp * ((x - start) / period * TAU).sin();
                let mut path = PathBuilder::stroke(px(5.));
                path.move_to(point(px(start), px(y(start))));
                let mut x = start;
                while x < end {
                    x = (x + 1.5).min(end);
                    path.line_to(point(px(x), px(y(x))));
                }
                if let Ok(path) = path.build() {
                    window.paint_path(path, stroke);
                }
                for cap in [start, end] {
                    window.paint_quad(fill(Bounds::centered_at(point(px(cap), px(y(cap))), size(px(5.), px(5.))), stroke).corner_radii(px(2.5)));
                }
            },
        )
        .absolute()
        .inset_0();
        div().relative().w(px(WAVE.size.width)).h(px(WAVE.size.height)).flex_none().child(paint).child(
            div()
                .absolute()
                .left(px(thumb_x(v) - WAVE.origin.x - THUMB.width / 2.))
                .top(px((WAVE.size.height - THUMB.height) / 2.))
                .w(px(THUMB.width))
                .h(px(THUMB.height))
                .rounded_full()
                .bg(WHITE)
                .shadow(vec![ui::shadow(rgba(0x00000040), 1., 5.)]),
        )
    }

    fn dial(&self) -> Div {
        let v = self.look.grain;
        let paint = canvas(
            |_, _, _| (),
            move |b, _, window, _| {
                let c = (f32::from(b.origin.x) + DIAL.size.width / 2., f32::from(b.origin.y) + DIAL.size.height / 2.);
                let at = |deg: f32, r: f32| {
                    let a = deg.to_radians();
                    point(px(c.0 + r * a.sin()), px(c.1 - r * a.cos()))
                };
                let knob = wash::dial_angle(v);
                for i in 0..DIAL_DOTS {
                    let mut deg = 360. * i as f32 / DIAL_DOTS as f32;
                    if deg > 180. {
                        deg -= 360.;
                    }
                    if wash::turn(deg, knob).abs() < 9. {
                        continue;
                    }
                    let lit = v > 0. && deg >= wash::DIAL_FROM && deg <= knob;
                    let dot_fill = rgba(if lit { 0xffffffd9 } else { 0xffffff59 });
                    window.paint_quad(fill(Bounds::centered_at(at(deg, 33.5), size(px(5.), px(5.))), dot_fill).corner_radii(px(2.5)));
                }
                let (inner, outer) = (at(knob, 27.), at(knob, 36.));
                let mut path = PathBuilder::stroke(px(7.));
                path.move_to(inner);
                path.line_to(outer);
                if let Ok(path) = path.build() {
                    window.paint_path(path, Hsla::from(WHITE));
                }
                for end in [inner, outer] {
                    window.paint_quad(fill(Bounds::centered_at(end, size(px(7.), px(7.))), Hsla::from(WHITE)).corner_radii(px(3.5)));
                }
            },
        )
        .absolute()
        .inset_0();
        let disc = div()
            .absolute()
            .left(px(16.))
            .top(px(16.))
            .size(px(40.))
            .rounded_full()
            .bg(rgb(0x8e908f))
            .child(img(grain_image(false)).size(px(40.)).rounded_full().opacity(0.55 + 0.45 * v));
        div().relative().size(px(DIAL.size.width)).flex_none().child(disc).child(paint)
    }

    /// Takes presses on the colours, the wave and the dial, and follows a drag past the card until the button comes up.
    fn input(&self, cx: &mut Context<Self>) -> Canvas<Hitbox> {
        let card = cx.entity();
        canvas(
            |bounds, window, _| window.insert_hitbox(bounds, HitboxBehavior::Normal),
            move |bounds, hitbox, window, _| {
                let local = move |p: Point<Pixels>| (f32::from(p.x - bounds.origin.x), f32::from(p.y - bounds.origin.y));
                window.on_mouse_event({
                    let card = card.clone();
                    move |e: &MouseDownEvent, phase, window, cx| {
                        if phase == DispatchPhase::Bubble
                            && e.button == MouseButton::Left
                            && hitbox.is_hovered(window)
                            && card.update(cx, |c, cx| c.press(local(e.position), cx))
                        {
                            cx.stop_propagation();
                        }
                    }
                });
                window.on_mouse_event({
                    let card = card.clone();
                    move |e: &MouseMoveEvent, phase, _, cx| {
                        if phase != DispatchPhase::Bubble || card.read(cx).drag.is_none() {
                            return;
                        }
                        // A button let go where no event reached us, as over another app, ends the drag on the next move.
                        if e.pressed_button != Some(MouseButton::Left) {
                            return card.update(cx, |c, cx| c.release(cx));
                        }
                        card.update(cx, |c, cx| c.drag_to(local(e.position), cx));
                    }
                });
                window.on_mouse_event(move |e: &MouseUpEvent, phase, _, cx| {
                    if phase == DispatchPhase::Bubble && e.button == MouseButton::Left {
                        card.update(cx, |c, cx| c.release(cx));
                    }
                });
            },
        )
        .absolute()
        .inset_0()
    }
}

impl Render for ThemeCard {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.open {
            return div().into_any_element();
        }
        if self.tick(Instant::now(), cx.reduce_motion()) {
            window.request_animation_frame();
        }
        let card = wash::card(self.look.colours.first().map(|c| dot(*c)));
        div()
            .id("theme-card")
            .relative()
            .occlude()
            .w(px(WIDTH))
            .p(px(INSET))
            .pb(px(14.))
            .rounded(px(22.))
            .bg(card)
            .border(px(1.))
            .border_color(rgba(0xffffff1a))
            .shadow(vec![ui::ring(rgba(0x00000080), 0.5), ui::shadow(rgba(0x00000059), 24., 60.), ui::shadow(rgba(0x00000026), 4., 12.)])
            .text_color(rgba(0xffffff8c))
            .child(self.pad(cx))
            .child(self.swatches(card, cx))
            .child(div().mt(px(15.)).pl(px(10.)).flex().items_center().gap(px(10.)).child(self.wave(card)).child(self.dial()))
            .child(self.input(cx))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| this.shut(cx)))
            .with_animation(("theme-card-in", self.opened), Animation::new(OPEN).with_easing(ease_out_quint()), |d, t| d.opacity(t).ml(px(-6. * (1. - t))))
            .into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{CENTRE, Drag, HANDLE, INSET, PAD, WAVE, dial_centre, intensity_at, on_card, on_pad, press, thumb_x};
    use store::prefs::sidebar::SpaceLook;
    use theme::wash;

    fn look() -> SpaceLook {
        SpaceLook { intensity: 0.5, grain: 0.25, ..SpaceLook::default() }
    }

    #[test]
    fn a_press_on_a_handle_grabs_the_topmost_by_where_it_was_grabbed() {
        let shown = [(0., 0.), (0.1, 0.)];
        let (hx, hy) = on_card(0.1, 0.);
        assert_eq!(press((hx + 3., hy), &shown, 0, &look()), Some(Drag::Colour { i: 1, grab: (3., 0.) }));
        assert_eq!(press((INSET / 2., CENTRE.1), &shown, 0, &look()), None);
    }

    #[test]
    fn a_press_on_the_wave_grabs_the_thumb_or_jumps_it_there() {
        let y = WAVE.origin.y + 30.;
        assert_eq!(press((thumb_x(0.5) + 4., y), &[], 0, &look()), Some(Drag::Intensity { grab: 4. }));
        assert_eq!(press((WAVE.origin.x + 2., y), &[], 0, &look()), Some(Drag::Intensity { grab: 0. }));
        assert_eq!(intensity_at(thumb_x(0.3)), 0.3);
        assert_eq!(intensity_at(WAVE.origin.x), 0.);
    }

    #[test]
    fn a_press_on_the_dial_holds_its_angle_and_one_off_it_does_nothing() {
        let (cx, cy) = dial_centre();
        assert_eq!(press((cx, cy - 30.), &[], 0, &look()), Some(Drag::Grain { bearing: 0., angle: wash::dial_angle(0.25) }));
        assert_eq!(press((cx + 40., cy + 40.), &[], 0, &look()), None);
    }

    #[test]
    fn a_press_on_the_pad_brings_the_selected_colour_but_not_one_on_its_buttons() {
        let shown = [(0., 0.), (0.5, 0.)];
        let (x, y) = on_card(-0.5, 0.3);
        assert_eq!(press((x, y), &shown, 1, &look()), Some(Drag::Colour { i: 1, grab: (0., 0.) }));
        assert_eq!(press((x, y), &[], 0, &look()), None);
        assert_eq!(press((CENTRE.0, INSET + 40.), &shown, 1, &look()), None);
        assert_eq!(press((CENTRE.0 + 20., INSET + PAD - 30.), &shown, 1, &look()), None);
    }

    #[test]
    fn a_handle_at_the_rim_stays_inside_the_pad() {
        let (x, _) = on_card(1., 0.);
        assert!(x + HANDLE / 2. <= INSET + PAD);
        let (x, y) = on_pad(on_card(0.4, -0.2));
        assert!((x - 0.4).abs() < 1e-4 && (y + 0.2).abs() < 1e-4, "{x} {y}");
    }
}
