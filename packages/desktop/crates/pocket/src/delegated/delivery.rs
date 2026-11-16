use crate::desktop::Desktop;
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

    pub(crate) fn arriving(&self, id: &str, now: Instant) -> bool {
        self.at.get(id).is_some_and(|at| now.duration_since(*at) < ARRIVING)
    }
}

/// What pocketd told the chat of the sessions it delegated to, one quiet line each: their logo and title, then how it went. A click opens the session.
pub(crate) fn delivered(id: &str, d: &Delivered, arriving: bool, window: &mut Window, cx: &mut Context<Desktop>) -> Div {
    div().flex().flex_col().items_start().gap(px(1.)).ml(px(-8.)).children(d.children.iter().enumerate().map(|(i, child)| line(&format!("{id}-{i}"), child, d, arriving, window, cx)))
}

fn verb(d: &Delivered) -> (String, Token) {
    match d.outcome {
        Outcome::Done => ("done".into(), TEXT_3),
        Outcome::Failed => ("failed".into(), FAILED_TEXT),
        Outcome::Asks => (format!("asks \u{201c}{}\u{201d}", d.question.split_whitespace().collect::<Vec<_>>().join(" ")), TEXT_2),
    }
}

fn line(key: &str, child: &Child, d: &Delivered, arriving: bool, window: &mut Window, cx: &mut Context<Desktop>) -> SpringAnimationElement<Stateful<Div>> {
    let (said, colour) = verb(d);
    let open = child.id.clone();
    let title = if child.title.is_empty() { "Session".to_string() } else { child.title.clone() };
    let hover = window.use_keyed_state(SharedString::from(format!("delivery-hover-{key}")), cx, |_, _| false);
    let hovered = *hover.read(cx);
    let chevron = div().flex_none().text_color(TEXT_4).child(icon("chevron-right", 12., TEXT_4)).with_spring(SharedString::from(format!("delivery-chevron-{key}")), SpringAnimation::new(SPRING).to(hovered), |d, t| {
        d.relative().left(px(-SLIDE * (1. - t.0))).opacity(t.0.clamp(0., 1.))
    });
    // A new line starts unseen and low; one scrolled back to starts in place.
    let rise = SpringAnimation::new(SPRING).to(true);
    let rise = if arriving { rise.from(false) } else { rise };
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
        .on_click(cx.listener(move |this, _, window, cx| this.delegated_open(&open, window, cx)))
        .child(provider_icon(&child.agent, 13., TEXT_3))
        .child(div().min_w_0().truncate().font_weight(FontWeight::MEDIUM).text_color(TEXT_BODY).child(title))
        .child(div().min_w_0().truncate().text_color(colour).when(d.outcome == Outcome::Asks, |s| s.max_w(px(340.))).child(said))
        .child(chevron)
        .with_spring(SharedString::from(format!("delivery-rise-{key}")), rise, |d, t| d.relative().top(px(RISE * (1. - t.0))).opacity(t.0.clamp(0., 1.)))
}

#[cfg(test)]
mod tests {
    use super::{Arrivals, verb};
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
        assert!(!arrivals.arriving("old", start), "what the chat opened with is no arrival");
        arrivals.observe(["old", "new"].into_iter(), true, start + Duration::from_secs(5));
        let at = |s: u64| start + Duration::from_secs(5) + Duration::from_secs(s);
        assert_eq!([arrivals.arriving("old", at(0)), arrivals.arriving("new", at(0))], [false, true]);
        assert!(!arrivals.arriving("new", at(2)), "scrolled back to later, it is just there");
        arrivals.observe(["old", "new"].into_iter(), true, at(3));
        assert!(!arrivals.arriving("new", at(3)), "seeing it again is no arrival");
    }
}
