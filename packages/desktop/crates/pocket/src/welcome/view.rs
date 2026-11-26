use super::{Pose, Wake, Welcome, WelcomeEvent};
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Instant;
use theme::*;
use ui::Variant;

const MASCOT: f32 = 160.;
const BOX_W: f32 = 170.;
const BOX_H: f32 = 176.;
/// Where squash and stretch pivot and its shadow falls: its feet, as a share of its height from the top (y 168 of the SVG's 200).
pub(crate) const FEET: f32 = 0.84;
const GLOW: f32 = 300.;
// The mascot keeps the app icon's colours in both appearances.
pub(crate) const BODY: u32 = 0x1a1a1e;
pub(crate) const EYES: u32 = 0xf4f2ee;
pub(crate) const LIGHT_ON: u32 = 0x8ee6cc;
pub(crate) const LIGHT_OFF: u32 = 0x24302c;
pub(crate) const GLOW_COLOR: u32 = 0x7a68bf;

impl EventEmitter<WelcomeEvent> for Welcome {}

impl Render for Welcome {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        match self.drawn {
            Some(pose) => view(pose, cx),
            None => div().into_any_element(),
        }
    }
}

impl Welcome {
    pub(crate) fn entity<T: 'static>(cx: &mut Context<T>) -> Entity<Self> {
        cx.new(|cx| Self { _keys: Some(Self::on_enter(cx)), ..Self::new() })
    }

    /// Ahead of the keymap and of focus: Return takes the way in while the welcome covers the app.
    fn on_enter(cx: &mut Context<Self>) -> Subscription {
        let this = cx.entity().downgrade();
        cx.intercept_keystrokes(move |ev, _, cx| {
            if this.update(cx, |w, cx| w.enter(&ev.keystroke, cx)).unwrap_or(false) {
                cx.stop_propagation();
            }
        })
    }

    fn enter(&mut self, key: &Keystroke, cx: &mut Context<Self>) -> bool {
        if !self.covers() || key.key != "enter" || key.modifiers.modified() {
            return false;
        }
        self.leave(cx);
        true
    }

    fn leave(&mut self, cx: &mut Context<Self>) {
        self.go(Instant::now());
        cx.emit(WelcomeEvent::Taken);
        cx.notify();
    }

    /// Steps the motion to `now` and keeps the pose to draw; whether it needs a frame next. Between blinks nothing moves, so it sleeps instead of drawing every frame.
    pub(crate) fn step(&mut self, now: Instant, still: bool, cx: &mut Context<Self>) -> bool {
        let wake = self.tick(now, still);
        self.drawn = Some(self.pose(now, still));
        if let Wake::At(at) = wake
            && self.wake.as_ref().is_none_or(|(t, _)| *t != at)
        {
            let task = cx.spawn(async move |this, cx| {
                cx.background_executor().timer(at.saturating_duration_since(Instant::now())).await;
                this.update(cx, |_, cx| cx.notify()).ok();
            });
            self.wake = Some((at, task));
        }
        wake == Wake::Frame
    }

    fn act(&mut self, cx: &mut Context<Self>, f: impl FnOnce(&mut Self)) {
        f(self);
        cx.notify();
    }
}

impl Desktop {
    pub(crate) fn greet(window: &mut Window, cx: &mut Context<Self>) -> (Entity<Welcome>, Subscription) {
        let welcome = Welcome::entity(cx);
        let sub = cx.subscribe_in(&welcome, window, Self::on_welcome_event);
        (welcome, sub)
    }

    fn on_welcome_event(&mut self, _: &Entity<Welcome>, _: &WelcomeEvent, _: &mut Window, cx: &mut Context<Self>) {
        self.save_soon(cx);
        cx.notify();
    }

    /// Steps the welcome and draws it over the app; drops it once it has faded onto the app.
    pub(crate) fn welcome_view(&mut self, now: Instant, still: bool, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let welcome = self.welcome.clone()?;
        let (frame, done, covers) = welcome.update(cx, |w, cx| (w.step(now, still, cx), w.done(), w.covers()));
        if frame {
            window.request_animation_frame();
        }
        if done {
            self.welcome = None;
            if self.new_form.read(cx).shown() {
                self.focus_prompt(window, cx);
            }
            return None;
        }
        // The composer under it would otherwise take what is typed.
        if covers && !self.root.is_focused(window) {
            window.focus(&self.root, cx);
        }
        Some(welcome.into_any_element())
    }
}

fn view(pose: Pose, cx: &mut Context<Welcome>) -> AnyElement {
    let copy = |i: usize, e: Div| e.relative().top(px((1. - pose.copy[i]) * 10.)).opacity(pose.copy[i]);
    let mut go = ui::large(ui::button("welcome-go", Variant::Primary, None, "Get started")).mt(px(26.));
    if pose.copy[2] > 0.5 {
        go = go.on_click(cx.listener(|w, _, _, cx| w.leave(cx)));
    }
    div()
        .id("welcome")
        .absolute()
        .inset_0()
        .occlude()
        .opacity(pose.opacity)
        .bg(WINDOW_SOLID)
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .on_mouse_down(MouseButton::Left, cx.listener(|w, _, _, cx| w.act(cx, |w| w.skip(Instant::now()))))
        .on_mouse_up(MouseButton::Left, cx.listener(|w, _, _, cx| w.act(cx, Welcome::release)))
        .on_mouse_up_out(MouseButton::Left, cx.listener(|w, _, _, cx| w.act(cx, Welcome::release)))
        .on_mouse_move(cx.listener(|w, ev: &MouseMoveEvent, _, cx| w.act(cx, |w| w.look_at(f32::from(ev.position.x), f32::from(ev.position.y)))))
        .on_hover(cx.listener(|w, hovered: &bool, _, cx| {
            if !hovered {
                w.act(cx, Welcome::look_ahead);
            }
        }))
        .child(crate::desktop::chrome::drag_area(div().absolute().top_0().left_0().right_0().h(px(40.))))
        .child(mascot(pose, cx))
        .child(copy(
            0,
            div().mt(px(18.)).text_size(px(30.)).line_height(px(36.)).font_weight(FontWeight::BOLD).text_color(TEXT).child("Hi. Let's get your agents going."),
        ))
        .child(copy(
            1,
            div()
                .mt(px(10.))
                .max_w(px(440.))
                .text_center()
                .text_size(px(15.))
                .line_height(px(22.))
                .text_color(TEXT_2)
                .child("Zoes keeps every coding agent you run in one window, and tells you which one needs you."),
        ))
        .child(copy(2, div().child(go)))
        .into_any_element()
}

/// The mascot in layers, each one colour: its glow, shadow and halo, then body, sheen, light and eyes, all squashed from its feet.
fn mascot(pose: Pose, cx: &mut Context<Welcome>) -> Stateful<Div> {
    let feet = MASCOT * (FEET - 0.5) * (1. - pose.sy);
    let body = Transformation::scale(size(pose.sx, pose.sy)).with_rotation(radians(pose.lean)).with_translation(point(px(0.), px(pose.y + feet)));
    let eyes = Transformation::scale(size(pose.sx, pose.sy * pose.lid))
        .with_rotation(radians(pose.lean))
        .with_translation(point(px(pose.eyes.0), px(pose.y + feet + pose.eyes.1)));
    let layer = |name: &str, color: Hsla, t: Transformation| {
        svg()
            .path(format!("icons/mascot-{name}.svg"))
            .absolute()
            .left(px((BOX_W - MASCOT) / 2.))
            .bottom(px(6.))
            .size(px(MASCOT))
            .text_color(color)
            .with_transformation(t)
    };
    let shade = |hex: u32, alpha: f32| Hsla::from(rgb(hex)).opacity(alpha);
    let shadow_w = 116. * (0.4 + 0.6 * pose.near);
    let this = cx.weak_entity();
    let measure = canvas(
        move |b, _, cx| {
            // The eyes sit a little above the middle of its box.
            let center = (f32::from(b.center().x), f32::from(b.top()) + f32::from(b.size.height) * 0.45);
            this.update(cx, |w, _| w.center = Some(center)).ok();
        },
        |_, _, _, _| {},
    )
    .absolute()
    .size_full();
    div()
        .id("welcome-mascot")
        .relative()
        .w(px(BOX_W))
        .h(px(BOX_H))
        .cursor_pointer()
        .on_mouse_down(MouseButton::Left, cx.listener(|w, _, _, cx| w.act(cx, |w| w.press(Instant::now()))))
        .when(!pose.hidden, |d| {
            d.child(div().absolute().left(px((BOX_W - GLOW) / 2.)).top(px((BOX_H - GLOW) / 2.)).size(px(GLOW)).rounded_full().shadow(vec![BoxShadow {
                color: shade(GLOW_COLOR, 0.45 * pose.glow),
                offset: point(px(0.), px(0.)),
                blur_radius: px(110.),
                spread_radius: px(0.),
                inset: false,
            }]))
            .child(div().absolute().bottom(px(4. + MASCOT * (1. - FEET))).left(px((BOX_W - shadow_w) / 2.)).w(px(shadow_w)).h(px(4.)).rounded_full().shadow(
                vec![BoxShadow {
                    color: shade(0x000000, 0.1 + 0.4 * pose.near),
                    offset: point(px(0.), px(0.)),
                    blur_radius: px(10.),
                    spread_radius: px(2.),
                    inset: false,
                }],
            ))
            .child(layer("halo", shade(LIGHT_ON, pose.halo), body))
            .child(layer("body", rgb(BODY).into(), body))
            .child(layer("sheen", shade(0xffffff, 0.08), body))
            .child(layer("light", rgb(if pose.lit { LIGHT_ON } else { LIGHT_OFF }).into(), body))
            .child(layer("eyes", rgb(EYES).into(), eyes))
        })
        .child(measure)
}
