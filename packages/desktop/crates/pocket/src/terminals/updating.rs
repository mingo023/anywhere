//! The page shown while pocketd updates to this app's build: the mascot signalling from its antenna beside the update's steps.

use crate::creating::{Part, rise};
use crate::mascot::{BEAT, DOWN, Look, STILL, mascot, waiting};
use crate::terminals::link::{Hint, Update};
use crate::terminals::{Terminals, TerminalsEvent};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::Instant;
use theme::*;
use ui::Variant;

const MASCOT: f32 = 84.;
const MASCOT_IN: Part = (0., 8.);
const TITLE_IN: Part = (60., 8.);
const STEP_IN: Part = (120., 8.);
const STAGGER: f32 = 60.;

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

impl Render for Terminals {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let now = Instant::now();
        let (hint, update, stale) = (self.link.hint(now), self.link.update(now), self.link.stale().unwrap_or_default().to_string());
        let tip = match hint {
            None | Some(Hint::Stale) => Vec::new(),
            Some(Hint::Install) => {
                vec![div().child("Not starting? In Terminal:").into_any_element(), div().font_family(MONO).child("pocketd daemon install").into_any_element()]
            }
            Some(Hint::LoginItems) => vec![
                div().child("Zoes's background service is off.").into_any_element(),
                ui::button("open-login-items", ui::Variant::Primary, None, "Open Login Items")
                    .mt(px(6.))
                    .on_click(|_, _, _| daemon::service::open_login_items())
                    .into_any_element(),
            ],
            Some(Hint::Reopen) => vec![div().child("Not starting? Quit and reopen Zoes.").into_any_element()],
        };
        match update {
            Some(update) => updating_page(update, &stale, tip, window, cx),
            None => div()
                .flex_1()
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .text_size(px(14.))
                .text_color(TEXT_3)
                .when(hint != Some(Hint::LoginItems), |d| d.child("Starting Zoes's terminal service…"))
                .children(tip),
        }
    }
}

/// The page for `update`; `stale` is the build pocketd still runs and `tip` what to try when it doesn't come back.
fn updating_page(update: Update, stale: &str, tip: Vec<AnyElement>, window: &mut Window, cx: &mut Context<Terminals>) -> Div {
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
        Update::Waiting(_) if !reduce => div()
            .with_animation("updating-mascot", Animation::new(BEAT).repeat_synced().with_max_fps(10.), |d, t| d.child(mascot(waiting(t), MASCOT)))
            .into_any_element(),
        Update::Waiting(_) => mascot(STILL, MASCOT).into_any_element(),
        Update::Stuck => mascot(Look { eyes: DOWN, lit: false, ..STILL }, MASCOT).into_any_element(),
        Update::Back(_) => mascot(Look { eyes: (0., 0.), happy: true, ..STILL }, MASCOT).into_any_element(),
    };
    let installed = done(format!("Zoes {} installed", channel::version()));
    let note = |el: Div| el.pl(px(26.)).text_size(px(13.)).line_height(px(19.)).text_color(TEXT_3);
    let (title, steps): (&str, Vec<Div>) = match update {
        Update::Waiting(waited) => {
            let updating = step(spinner("updating-spin", 14., TEXT_3), "Updating the background service", TEXT_2)
                .child(div().ml_auto().pl(px(16.)).font_family(MONO).text_size(px(12.)).text_color(TEXT_4).child(format!("{}s", waited.as_secs())));
            let tip = (!tip.is_empty()).then(|| note(div().flex().flex_col().gap(px(6.)).items_start()).children(tip));
            let resume = step(div().size(px(6.)).rounded_full().bg(TEXT_5), "Sessions resume when it's back", TEXT_4);
            ("Updating Zoes", [Some(installed), Some(updating), tip, Some(resume)].into_iter().flatten().collect())
        }
        Update::Stuck => (
            "The update needs a restart",
            vec![
                installed,
                step(icon("warning", 14., WAITING_DOT), "Couldn't update the background service in place", TEXT_2),
                note(div().flex().flex_col().gap(px(10.)).items_start())
                    .child(format!("It's still on {stale}. Restarting it interrupts open sessions, which then resume."))
                    .child(
                        ui::button("restart-service", Variant::Primary, None, "Restart service")
                            .on_click(cx.listener(|_, _, _, cx| cx.emit(TerminalsEvent::RestartService))),
                    ),
            ],
        ),
        Update::Back(_) => ("Updating Zoes", vec![installed, done("Background service updated")]),
    };
    let opacity = if let Update::Back(t) = update { hand_back_opacity(t) } else { 1. };
    let text = div()
        .w(px(340.))
        .flex()
        .flex_col()
        .gap(px(12.))
        .child(rising(div().text_size(px(15.)).line_height(px(20.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child(title), TITLE_IN))
        .child(
            div().flex().flex_col().gap(px(9.)).children(steps.into_iter().enumerate().map(|(i, s)| rising(s, (STEP_IN.0 + STAGGER * i as f32, STEP_IN.1)))),
        );
    let face = rising(div().mt(px(-4.)).child(face), MASCOT_IN);
    if moving {
        window.request_animation_frame();
    }
    div().flex_1().flex().items_center().justify_center().pb(px(40.)).opacity(opacity).child(div().flex().items_start().gap(px(22.)).child(face).child(text))
}

#[cfg(test)]
mod tests {
    use super::hand_back_opacity;

    #[test]
    fn the_hand_back_holds_the_page_then_fades_it_out() {
        assert_eq!((hand_back_opacity(0.), hand_back_opacity(0.69)), (1., 1.));
        assert!(hand_back_opacity(0.85) < 1.);
        assert_eq!(hand_back_opacity(1.), 0.);
    }
}
