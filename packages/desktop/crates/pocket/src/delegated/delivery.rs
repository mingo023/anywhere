use crate::chat::transcript::{View, ViewEvent};
use chat::{Child, Delivered, Outcome};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::{HashMap, HashSet};
use std::time::{Duration, Instant};
use theme::*;

/// How far a new delivery rises into place, and how far a hovered one's chevron slides in.
const RISE: f32 = 4.;
const SLIDE: f32 = 3.;
/// How long a delivery counts as having just arrived.
const ARRIVING: Duration = Duration::from_secs(1);
/// How long a new delivery takes to fade in for a user who asked for less motion.
const FADE: Duration = Duration::from_millis(200);

/// The deliveries a chat has shown, to tell one that arrives from what the chat opened with, and from one scrolled back to.
#[derive(Default)]
pub(crate) struct Arrivals {
    seen: HashSet<String>,
    at: HashMap<String, Instant>,
}

impl Arrivals {
    /// Takes the deliveries the chat has now; `primed` is whether it had already shown what it opened with.
    pub(crate) fn observe<'a>(&mut self, ids: impl Iterator<Item = &'a str>, primed: bool, now: Instant) {
        for id in ids {
            if self.seen.insert(id.to_string()) && primed {
                self.at.insert(id.to_string(), now);
            }
        }
        self.at.retain(|_, at| now.duration_since(*at) < ARRIVING);
    }

    /// How long ago the delivery arrived, while it counts as just arrived.
    pub(crate) fn age(&self, id: &str, now: Instant) -> Option<Duration> {
        self.at.get(id).map(|at| now.duration_since(*at)).filter(|age| *age < ARRIVING)
    }
}

/// How faded in a delivery is `age` after it arrived, when it fades in place of rising.
fn faded_in(age: Duration) -> f32 {
    (age.as_secs_f32() / FADE.as_secs_f32()).min(1.)
}

/// What pocketd told the chat of the sessions it delegated to, one quiet line each: their logo and title, then how it went. A click opens the session.
pub(crate) fn delivered(id: &str, d: &Delivered, age: Option<Duration>, window: &mut Window, cx: &mut Context<View>) -> Div {
    div()
        .flex()
        .flex_col()
        .items_start()
        .gap(px(1.))
        .ml(px(-8.))
        .children(d.children.iter().enumerate().map(|(i, child)| line(&format!("{id}-{i}"), child, d, age, window, cx)))
}

fn verb(d: &Delivered) -> (String, Token) {
    match d.outcome {
        Outcome::Done => ("done".into(), TEXT_3),
        Outcome::Failed => ("failed".into(), FAILED_TEXT),
        Outcome::Asks => (format!("asks \u{201c}{}\u{201d}", d.question.split_whitespace().collect::<Vec<_>>().join(" ")), TEXT_2),
    }
}

fn line(key: &str, child: &Child, d: &Delivered, age: Option<Duration>, window: &mut Window, cx: &mut Context<View>) -> SpringAnimationElement<Stateful<Div>> {
    let (said, colour) = verb(d);
    let open = child.id.clone();
    let title = if child.title.is_empty() { "Session".to_string() } else { child.title.clone() };
    let hover = window.use_keyed_state(SharedString::from(format!("delivery-hover-{key}")), cx, |_, _| false);
    let hovered = *hover.read(cx);
    let chevron = div().flex_none().text_color(TEXT_4).child(icon("chevron-right", 12., TEXT_4)).with_spring(
        SharedString::from(format!("delivery-chevron-{key}")),
        SpringAnimation::new(SPRING).to(hovered),
        |d, t| d.relative().left(px(-SLIDE * (1. - t.0))).opacity(t.0.clamp(0., 1.)),
    );
    let rise = SpringAnimation::new(SPRING).to(true);
    let rise = if age.is_some() { rise.from(false) } else { rise };
    // A spring snaps under reduced motion, so the fade there is timed by hand.
    let fade = age.filter(|_| cx.reduce_motion()).map_or(1., faded_in);
    if fade < 1. {
        window.request_animation_frame();
    }
    div()
        .id(SharedString::from(format!("delivery-{key}")))
        .max_w(relative(1.))
        .h(px(28.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(8.))
        .text_size(px(14.))
        .text_color(TEXT_3)
        .whitespace_nowrap()
        .cursor_pointer()
        .hover(|s| s.bg(FILL_2))
        .active(|s| s.bg(FILL_3))
        .on_hover(move |over, _, cx| {
            hover.update(cx, |h, cx| {
                *h = *over;
                cx.notify();
            })
        })
        .on_click(cx.listener(move |_, _, _, cx| cx.emit(ViewEvent::OpenSession(open.clone()))))
        .child(provider_icon(&child.agent, 13., TEXT_3))
        .child(div().min_w_0().truncate().font_weight(FontWeight::MEDIUM).text_color(TEXT_BODY).child(title))
        .child(div().min_w_0().truncate().text_color(colour).when(d.outcome == Outcome::Asks, |s| s.max_w(px(340.))).child(said))
        .child(chevron)
        .with_spring(SharedString::from(format!("delivery-rise-{key}")), rise, move |d, t| {
            d.relative().top(px(RISE * (1. - t.0))).opacity(t.0.clamp(0., 1.).min(fade))
        })
}

#[cfg(test)]
mod tests {
    use super::{Arrivals, faded_in, verb};
    use chat::{Delivered, Outcome};
    use std::time::{Duration, Instant};

    fn said(outcome: Outcome, question: &str) -> String {
        verb(&Delivered { id: "d".into(), outcome, question: question.into(), children: Vec::new() }).0
    }

    #[test]
    fn a_delivery_says_done_failed_or_what_the_session_asks_on_one_line() {
        assert_eq!([said(Outcome::Done, ""), said(Outcome::Failed, "")], ["done", "failed"]);
        assert_eq!(said(Outcome::Asks, "Which\ntest   runner?"), "asks \u{201c}Which test runner?\u{201d}");
    }

    #[test]
    fn a_delivery_arriving_after_the_chat_opened_rises_for_a_moment_and_one_it_opened_with_never_does() {
        let (mut arrivals, start) = (Arrivals::default(), Instant::now());
        arrivals.observe(["old"].into_iter(), false, start);
        assert!(arrivals.age("old", start).is_none(), "what the chat opened with is no arrival");
        arrivals.observe(["old", "new"].into_iter(), true, start + Duration::from_secs(5));
        let at = |s: u64| start + Duration::from_secs(5) + Duration::from_secs(s);
        assert_eq!([arrivals.age("old", at(0)).is_some(), arrivals.age("new", at(0)).is_some()], [false, true]);
        assert!(!arrivals.age("new", at(2)).is_some(), "scrolled back to later, it is just there");
        arrivals.observe(["old", "new"].into_iter(), true, at(3));
        assert!(!arrivals.age("new", at(3)).is_some(), "seeing it again is no arrival");
    }

    #[test]
    fn a_delivery_fades_in_over_a_fifth_of_a_second_when_motion_is_reduced() {
        let ms = Duration::from_millis;
        assert_eq!([faded_in(ms(0)), faded_in(ms(100)), faded_in(ms(200)), faded_in(ms(900))], [0., 0.5, 1., 1.]);
    }

    #[test]
    fn an_arrival_knows_how_long_ago_it_came_until_it_is_just_there() {
        let (mut arrivals, start) = (Arrivals::default(), Instant::now());
        arrivals.observe(std::iter::empty::<&str>(), false, start);
        arrivals.observe(["new"].into_iter(), true, start);
        assert_eq!(arrivals.age("new", start + Duration::from_millis(150)), Some(Duration::from_millis(150)));
        assert_eq!(arrivals.age("new", start + Duration::from_secs(2)), None);
    }
}
