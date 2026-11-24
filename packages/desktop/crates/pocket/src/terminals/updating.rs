//! The page shown while pocketd updates to this app's build: the mascot signalling from its antenna beside the update's steps.

use crate::creating::{Part, rise};
use crate::desktop::Desktop;
use crate::terminals::link::Update;
use crate::welcome::view::{BODY, EYES, GLOW_COLOR, LIGHT_OFF, LIGHT_ON};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Duration;
use theme::*;
use ui::Variant;

const MASCOT: f32 = 84.;
/// The antenna's light, as a share of the mascot SVG's box.
const LIGHT_AT: (f32, f32) = (131. / 200., 19. / 200.);
const RING: f32 = 40.;
const BEAT: Duration = Duration::from_millis(4400);
/// A ring starts this small, fully grown at 1, and the second leaves this far into a pulse.
const RING_FROM: f32 = 0.27;
const RING_DELAY: f32 = 0.1;
const RING_OPACITY: f32 = 0.75;
/// Where in the beat the blink starts, and for how much of it.
const BLINK_AT: f32 = 0.955;
const BLINK_FOR: f32 = 0.035;
/// Where the eyes look, in the SVG's units: at the steps, or down at the button.
const GLANCE: (f32, f32) = (5., 1.);
const DOWN: (f32, f32) = (4., 6.);
const MASCOT_IN: Part = (0., 8.);
const TITLE_IN: Part = (60., 8.);
const STEP_IN: Part = (120., 8.);
const STAGGER: f32 = 60.;

#[derive(Clone, Copy, Debug, PartialEq)]
struct Look {
    /// How open its eyes are; 1 is open.
    lid: f32,
    /// Each ring's scale and opacity.
    rings: [(f32, f32); 2],
    eyes: (f32, f32),
    happy: bool,
    lit: bool,
}

const STILL: Look = Look { lid: 1., rings: [(0., 0.); 2], eyes: GLANCE, happy: false, lit: true };

/// The mascot `t` of the way through a beat while it waits.
fn waiting(t: f32) -> Look {
    let ring = |delay: f32| {
        // Two pulses a beat, each ring moving through the first half of its pulse.
        let u = ((t * 2.).fract() - delay) / 0.5;
        if !(0. ..1.).contains(&u) {
            return (0., 0.);
        }
        (RING_FROM + (1. - RING_FROM) * (1. - (1. - u).powi(3)), RING_OPACITY * (1. - u))
    };
    let blink = ((t - BLINK_AT) / BLINK_FOR).clamp(0., 1.);
    Look { lid: 1. - 0.9 * (1. - (2. * blink - 1.).abs()), rings: [ring(0.), ring(RING_DELAY)], ..STILL }
}

fn mascot(look: Look) -> Div {
    let k = MASCOT / 200.;
    let shade = |hex: u32, alpha: f32| Hsla::from(rgb(hex)).opacity(alpha);
    let layer = |name: &str, color: Hsla| svg().path(format!("icons/mascot-{name}.svg")).absolute().top_0().left_0().size(px(MASCOT)).text_color(color);
    let eyes = Transformation::scale(size(1., look.lid)).with_translation(point(px(look.eyes.0 * k), px(look.eyes.1 * k)));
    let rings = look.rings.into_iter().filter(|(_, o)| *o > 0.).map(|(s, o)| {
        svg()
            .path("icons/agent-ring.svg")
            .absolute()
            .left(px(LIGHT_AT.0 * MASCOT - RING / 2.))
            .top(px(LIGHT_AT.1 * MASCOT - RING / 2.))
            .size(px(RING))
            .text_color(shade(LIGHT_ON, o))
            .with_transformation(Transformation::scale(size(s, s)))
    });
    let glow = MASCOT * 0.9;
    div()
        .relative()
        .flex_none()
        .size(px(MASCOT))
        .child(div().absolute().left(px((MASCOT - glow) / 2.)).top(px(MASCOT * 0.58 - glow / 2.)).size(px(glow)).rounded_full().shadow(vec![ui::shadow(shade(GLOW_COLOR, 0.4), 0., 40.)]))
        .child(div().absolute().top(px(MASCOT * 0.84)).left(px(MASCOT * 0.25)).w(px(MASCOT * 0.5)).h(px(4.)).rounded_full().shadow(vec![ui::shadow(shade(0x000000, 0.25), 0., 6.)]))
        .children(rings)
        .when(look.lit, |d| d.child(layer("halo", shade(LIGHT_ON, 0.35))))
        .child(layer("body", rgb(BODY).into()))
        .child(layer("sheen", shade(0xffffff, 0.08)))
        .child(layer("light", rgb(if look.lit { LIGHT_ON } else { LIGHT_OFF }).into()))
        .child(layer(if look.happy { "eyes-happy" } else { "eyes" }, rgb(EYES).into()).with_transformation(eyes))
}

fn step(lead: impl IntoElement, label: impl Into<SharedString>, color: Token) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(10.))
        .text_size(px(13.))
        .line_height(px(18.))
        .text_color(color)
        .child(div().size(px(16.)).flex().flex_none().items_center().justify_center().child(lead))
        .child(label.into())
}

fn done(label: impl Into<SharedString>) -> Div {
    step(icon("check", 14., SUCCESS), label, TEXT_2)
}

fn hand_back_opacity(t: f32) -> f32 {
    if t < 0.7 { 1. } else { ((1. - t) / 0.3).max(0.) }
}

impl Desktop {
    /// The page for `update`; `stale` is the build pocketd still runs and `tip` what to try when it doesn't come back.
    pub(crate) fn updating_page(&self, update: Update, stale: &str, tip: Vec<AnyElement>, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let reduce = cx.reduce_motion();
        let since = if let Update::Waiting(s) = update { Some(s) } else { None };
        let mut moving = false;
        let mut rising = |el: Div, part: Part| match since.and_then(|s| rise(s, part, reduce)) {
            Some(r) => {
                moving = true;
                r.on(el)
            }
            None => el,
        };
        let face = match update {
            Update::Waiting(_) if !reduce => div().with_animation("updating-mascot", Animation::new(BEAT).repeat_synced().with_max_fps(10.), |d, t| d.child(mascot(waiting(t)))).into_any_element(),
            Update::Waiting(_) => mascot(STILL).into_any_element(),
            Update::Stuck => mascot(Look { eyes: DOWN, lit: false, ..STILL }).into_any_element(),
            Update::Back(_) => mascot(Look { eyes: (0., 0.), happy: true, ..STILL }).into_any_element(),
        };
        let installed = done(format!("Anywhere {} installed", channel::version()));
        let note = |el: Div| el.pl(px(26.)).text_size(px(13.)).line_height(px(19.)).text_color(TEXT_3);
        let (title, steps): (&str, Vec<Div>) = match update {
            Update::Waiting(waited) => {
                let updating = step(spinner("updating-spin", 14., TEXT_3), "Updating the background service", TEXT_2).child(div().ml_auto().pl(px(16.)).font_family(MONO).text_size(px(12.)).text_color(TEXT_4).child(format!("{}s", waited.as_secs())));
                let tip = (!tip.is_empty()).then(|| note(div().flex().flex_col().gap(px(6.)).items_start()).children(tip));
                let resume = step(div().size(px(6.)).rounded_full().bg(TEXT_5), "Sessions resume when it's back", TEXT_4);
                ("Updating Anywhere", [Some(installed), Some(updating), tip, Some(resume)].into_iter().flatten().collect())
            }
            Update::Stuck => (
                "The update needs a restart",
                vec![
                    installed,
                    step(icon("warning", 14., WAITING_DOT), "Couldn't update the background service in place", TEXT_2),
                    note(div().flex().flex_col().gap(px(10.)).items_start())
                        .child(format!("It's still on {stale}. Restarting it interrupts open sessions, which then resume."))
                        .child(ui::button("restart-service", Variant::Primary, None, "Restart service").on_click(cx.listener(|this, _, _, cx| this.relaunch_or_restart_service(cx)))),
                ],
            ),
            Update::Back(_) => ("Updating Anywhere", vec![installed, done("Background service updated")]),
        };
        let opacity = if let Update::Back(t) = update { hand_back_opacity(t) } else { 1. };
        let text = div()
            .w(px(340.))
            .flex()
            .flex_col()
            .gap(px(12.))
            .child(rising(div().text_size(px(15.)).line_height(px(20.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title), TITLE_IN))
            .child(div().flex().flex_col().gap(px(9.)).children(steps.into_iter().enumerate().map(|(i, s)| rising(s, (STEP_IN.0 + STAGGER * i as f32, STEP_IN.1)))));
        let face = rising(div().mt(px(-4.)).child(face), MASCOT_IN);
        if moving {
            window.request_animation_frame();
        }
        div().flex_1().flex().items_center().justify_center().pb(px(40.)).opacity(opacity).child(div().flex().items_start().gap(px(22.)).child(face).child(text))
    }
}

#[cfg(test)]
mod tests {
    use super::{STILL, hand_back_opacity, waiting};

    #[test]
    fn rings_rise_off_the_antenna_then_the_mascot_rests_and_blinks_before_the_next_beat() {
        assert_eq!(waiting(0.).rings[0], (0.27, 0.75));
        assert_eq!(waiting(0.4), STILL);
        assert!((waiting(0.9725).lid - 0.1).abs() < 1e-3, "eyes shut mid-blink");
        assert_eq!(waiting(0.999).lid, 1.);
    }

    #[test]
    fn the_hand_back_holds_the_page_then_fades_it_out() {
        assert_eq!((hand_back_opacity(0.), hand_back_opacity(0.69)), (1., 1.));
        assert!(hand_back_opacity(0.85) < 1.);
        assert_eq!(hand_back_opacity(1.), 0.);
    }
}
