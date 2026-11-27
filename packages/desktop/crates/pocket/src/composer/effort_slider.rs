mod effects;
mod paint;
pub(crate) mod style;

use super::launch;
use crate::chat::composer::hand_focus;
use crate::glide::Glide;
use crate::models::effort_label;
use effects::{EffortAnimation, Light};
use gpui_kit::*;
use std::cell::RefCell;
use std::rc::Rc;
use std::time::Instant;
use style::EffortStyle;
use theme::*;

/// Hands the keys to `to` from the slider, for when the panel stops showing it.
pub(crate) fn hand_back(slider: &RefCell<EffortSlider>, to: FocusHandle) -> Option<impl IntoElement> {
    slider.borrow().focus.clone().map(|f| hand_focus(vec![f], to))
}

pub(crate) const HEIGHT: f32 = PAD_Y * 2. + HEADER + GAP + paint::HEIGHT + GAP + ABOUT;
const PAD_Y: f32 = 10.;
const HEADER: f32 = 18.;
const GAP: f32 = 4.;
const ABOUT: f32 = 15.;
/// The thumb's springs: `DRAG` trails the pointer, `SNAP` settles it once let go.
const DRAG: SpringConfig = SpringConfig::new(900., 52., 1.);
const SNAP: SpringConfig = SpringConfig::new(400., 34., 1.);
const LIFT: SpringConfig = SpringConfig::new(700., 46., 1.);
const LIFTED: f32 = 1.055;
/// In levels, past halfway to the next.
const HYSTERESIS: f32 = 0.15;
const LABEL_MS: f32 = 200.;

/// A drag in progress: where on the thumb it was grabbed, as a share of the track, and the level it started on.
#[derive(Clone, Copy)]
struct Drag {
    grab: f32,
    from: Option<usize>,
}

/// The effort slider's state. Positions are shares of the track: 0 at the first level, 1 at the last.
#[derive(Default)]
pub(crate) struct EffortSlider {
    levels: Vec<String>,
    labels: Vec<SharedString>,
    /// The level last committed here. It outlives the drag: a chat's effort takes a poll to come back, and the thumb
    /// would spring back to the old one until then.
    picked: Option<String>,
    /// The level the labels and colour name: the committed one, or the one under a drag.
    shown: Option<usize>,
    was: Option<(Option<usize>, Instant)>,
    thumb: Glide,
    target: f32,
    lift: Glide,
    drag: Option<Drag>,
    animation: EffortAnimation,
    /// Shows the focus ring.
    keyed: bool,
    pub(crate) focus: Option<FocusHandle>,
}

/// One frame of the slider, read off its state.
#[derive(Clone, Copy)]
pub(crate) struct Frame {
    thumb: f32,
    lift: f32,
    shown: Option<usize>,
    was: Option<usize>,
    /// 0 to 1.
    label: f32,
    lights: [Option<Light>; 2],
    keyed: bool,
    /// Reduced motion: things fade but don't move.
    still: bool,
    moving: bool,
}

impl EffortSlider {
    /// Lays the slider out for `levels`, at the level picked here, else `on`. A change from outside moves the thumb
    /// without a burst; one under a drag waits for the drag to end.
    pub(crate) fn sync(&mut self, levels: &[String], on: Option<&String>, now: Instant) {
        if self.levels != levels {
            self.levels = levels.to_vec();
            self.labels = levels.iter().map(|l| effort_label(l).into()).collect();
            self.drag = None;
            self.animation.clear();
            self.shown = self.committed(on);
            self.was = None;
            self.target = self.shown.map_or(0., |i| self.place(i));
            self.thumb = Glide::from(self.target);
            return;
        }
        let committed = self.committed(on);
        if self.drag.is_none() && committed != self.shown {
            self.show(committed, now, false);
            self.target = committed.map_or(0., |i| self.place(i));
        }
    }

    fn committed(&self, on: Option<&String>) -> Option<usize> {
        let at = |l: &String| self.levels.iter().position(|x| x == l);
        self.picked.as_ref().and_then(at).or_else(|| on.and_then(at))
    }

    fn place(&self, i: usize) -> f32 {
        intensity(i, self.levels.len())
    }

    /// A press at `at` of the way along: it grabs the thumb where it was pressed, or pulls the thumb to the pointer.
    pub(crate) fn press(&mut self, at: f32, on_thumb: bool, now: Instant) {
        let grab = if on_thumb && self.shown.is_some() { at - self.thumb.at() } else { 0. };
        self.drag = Some(Drag { grab, from: self.shown });
        self.keyed = false;
        self.drag_to(at, now);
    }

    pub(crate) fn drag_to(&mut self, at: f32, now: Instant) {
        let Some(drag) = self.drag else { return };
        self.target = (at - drag.grab).clamp(0., 1.);
        let level = self.level_at(self.target);
        if Some(level) != self.shown {
            self.show(Some(level), now, true);
        }
    }

    pub(crate) fn dragging(&self) -> bool {
        self.drag.is_some()
    }

    /// Ends a drag on the level it shows, and names that level if it isn't the one the drag started on.
    pub(crate) fn release(&mut self) -> Option<String> {
        let drag = self.drag.take()?;
        let i = self.shown?;
        self.target = self.place(i);
        (Some(i) != drag.from).then(|| self.commit(i))
    }

    /// Moves to level `i`, and names it if it wasn't the level already.
    pub(crate) fn choose(&mut self, i: usize, now: Instant, burst: bool) -> Option<String> {
        self.target = self.place(i);
        if Some(i) == self.shown {
            return None;
        }
        self.show(Some(i), now, burst);
        Some(self.commit(i))
    }

    /// The arrows, Home and End: the level they move to, with no burst, since a key held down would fire one per level.
    /// `None` for any other key.
    pub(crate) fn key(&mut self, key: &str, now: Instant) -> Option<Option<String>> {
        let last = self.levels.len().checked_sub(1)?;
        let to = match key {
            "left" | "down" => self.shown.map_or(0, |i| i.saturating_sub(1)),
            "right" | "up" => self.shown.map_or(0, |i| (i + 1).min(last)),
            "home" => 0,
            "end" => last,
            _ => return None,
        };
        self.keyed = true;
        Some(self.choose(to, now, false))
    }

    fn commit(&mut self, i: usize) -> String {
        let level = self.levels[i].clone();
        self.picked = Some(level.clone());
        level
    }

    /// The level nearest `t`, keeping the one shown until `t` is `HYSTERESIS` past halfway to another.
    fn level_at(&self, t: f32) -> usize {
        let f = t * self.levels.len().saturating_sub(1) as f32;
        match self.shown {
            Some(i) if (f - i as f32).abs() <= 0.5 + HYSTERESIS => i,
            _ => f.round() as usize,
        }
    }

    fn show(&mut self, at: Option<usize>, now: Instant, burst: bool) {
        self.was = Some((self.shown, now));
        self.shown = at;
        match at {
            Some(i) if burst => self.animation.start(self.place(i), now),
            _ => {}
        }
    }

    /// Steps everything that moves to `now`. With reduced motion the thumb jumps and no burst plays; the labels
    /// still fade, as a fade moves nothing.
    pub(crate) fn frame(&mut self, now: Instant, reduce_motion: bool) -> Frame {
        let dragging = self.drag.is_some();
        let mut moving = self.thumb.tick_on(if dragging { DRAG } else { SNAP }, self.target, now, reduce_motion);
        moving |= self.lift.tick_on(LIFT, if dragging { LIFTED } else { 1. }, now, reduce_motion);
        if reduce_motion {
            self.animation.clear();
        }
        let lights = self.animation.sample(now);
        moving |= self.animation.busy();
        let since = self.was.map_or(f32::INFINITY, |(_, at)| now.saturating_duration_since(at).as_secs_f32() * 1000.);
        let was = self.was.and_then(|(w, _)| w);
        moving |= since < LABEL_MS;
        Frame {
            thumb: self.thumb.at().clamp(0., 1.),
            lift: self.lift.at(),
            shown: self.shown,
            was,
            label: ease((since / LABEL_MS).min(1.)),
            lights,
            keyed: self.keyed,
            still: reduce_motion,
            moving,
        }
    }
}

/// How far up the scale level `i` of `n` sits, 0 to 1: what its colour and burst come from.
fn intensity(i: usize, n: usize) -> f32 {
    i as f32 / n.saturating_sub(1).max(1) as f32
}

fn ease(t: f32) -> f32 {
    ease_out_quint()(t)
}

fn describe(level: &str) -> &'static str {
    match level {
        "off" => "No extended thinking",
        "minimal" => "Barely thinks before replying",
        "low" => "Quick replies with light reasoning",
        "medium" => "Balanced speed and depth",
        "high" => "Thinks deeper before answering",
        "xhigh" | "extra-high" => "Extended reasoning for complex work",
        "max" => "Deepest reasoning, slowest replies",
        _ => "",
    }
}

/// The slider for `levels`, at `on` unless one was picked here, headed `word`, the agent's name for effort. Letting go
/// of a drag, clicking a label, or a key hands the new level to `pick`; nothing is sent while a drag moves.
pub(crate) fn effort_slider<V: 'static>(
    id: &'static str,
    word: &'static str,
    (levels, on): (&[String], Option<&String>),
    slider: &Rc<RefCell<EffortSlider>>,
    pick: launch::Pick<V>,
    cx: &mut Context<V>,
) -> Stateful<Div> {
    let now = Instant::now();
    let (frame, focus, about) = {
        let mut s = slider.borrow_mut();
        s.sync(levels, on, now);
        let frame = s.frame(now, cx.reduce_motion());
        let focus = s.focus.get_or_insert_with(|| cx.focus_handle()).clone();
        (frame, focus, frame.shown.map_or("The agent picks", |i| describe(&s.levels[i])))
    };
    let style = EffortStyle::at(frame.thumb);
    let wiring = {
        let (pick, desktop) = (pick.clone(), cx.weak_entity());
        let redraw = cx.weak_entity();
        paint::Wiring {
            state: slider.clone(),
            focus: focus.clone(),
            commit: Rc::new(move |level, cx| _ = desktop.update(cx, |this, cx| pick(this, level, cx))),
            redraw: Rc::new(move |cx| _ = redraw.update(cx, |_, cx| cx.notify())),
        }
    };
    let header = div().h(px(HEADER)).flex().items_center().text_size(px(13.)).font_weight(FontWeight::MEDIUM).child(word);
    let about = div().h(px(ABOUT)).text_size(px(11.)).line_height(px(ABOUT)).text_color(TEXT_4).opacity(0.4 + 0.6 * frame.label).truncate().child(about);
    let keys = slider.clone();
    div()
        .id(id)
        .track_focus(&focus)
        .on_key_down(cx.listener(move |this, e: &KeyDownEvent, _, cx| {
            if e.keystroke.modifiers.modified() {
                return;
            }
            let Some(level) = keys.borrow_mut().key(&e.keystroke.key, Instant::now()) else { return };
            cx.stop_propagation();
            if let Some(level) = level {
                pick(this, level, cx);
            }
            cx.notify();
        }))
        .flex_none()
        .h(px(HEIGHT))
        .px(px(14.))
        .py(px(PAD_Y))
        .flex()
        .flex_col()
        .gap(px(GAP))
        .border_t(px(0.5))
        .border_color(SEPARATOR)
        .child(header)
        .child(paint::track(wiring, frame, style))
        .child(about)
}

#[cfg(test)]
mod tests {
    use super::EffortSlider;
    use std::time::{Duration, Instant};

    fn levels() -> Vec<String> {
        ["low", "medium", "high", "xhigh", "max"].map(String::from).to_vec()
    }

    fn slider(on: &str) -> (EffortSlider, Instant) {
        let (mut s, now) = (EffortSlider::default(), Instant::now());
        s.sync(&levels(), Some(&on.to_string()), now);
        (s, now)
    }

    #[test]
    fn the_level_shown_holds_until_the_thumb_is_clearly_past_halfway() {
        let (mut s, now) = slider("low");
        s.press(0., true, now);
        s.drag_to(0.13, now);
        assert_eq!(s.shown, Some(0), "past halfway to medium, but inside the hysteresis");
        s.drag_to(0.17, now);
        assert_eq!(s.shown, Some(1));
        s.drag_to(0.11, now);
        assert_eq!(s.shown, Some(1), "back over halfway, still inside the hysteresis");
        s.drag_to(0.08, now);
        assert_eq!(s.shown, Some(0));
    }

    #[test]
    fn letting_go_snaps_to_the_level_shown_and_names_it_once() {
        let (mut s, now) = slider("low");
        s.press(0.1, false, now);
        s.drag_to(0.55, now);
        assert_eq!(s.release(), Some("high".to_string()));
        assert_eq!(s.target, 0.5);
        assert_eq!(s.release(), None, "the drag is over");
        s.press(0.5, false, now);
        s.drag_to(0.52, now);
        assert_eq!(s.release(), None, "it ended on the level it started on");
    }

    #[test]
    fn a_grabbed_thumb_keeps_where_it_was_held() {
        let (mut s, now) = slider("medium");
        s.press(0.27, true, now);
        assert_eq!(s.target, 0.25);
        s.drag_to(0.52, now);
        assert!((s.target - 0.5).abs() < 1e-6);
    }

    #[test]
    fn the_level_picked_here_stays_until_the_agent_reports_it() {
        let (mut s, now) = slider("low");
        assert_eq!(s.choose(3, now, true), Some("xhigh".to_string()));
        s.sync(&levels(), Some(&"low".to_string()), now);
        assert_eq!(s.shown, Some(3), "the chat's effort hasn't come back yet");
        s.sync(&levels()[..3], Some(&"low".to_string()), now);
        assert_eq!(s.shown, Some(0), "a model without it shows its own");
    }

    #[test]
    fn a_change_from_outside_moves_the_thumb_without_a_burst() {
        let (mut s, now) = slider("low");
        s.sync(&levels(), Some(&"max".to_string()), now);
        assert_eq!((s.shown, s.target, s.animation.busy()), (Some(4), 1., false));
    }

    #[test]
    fn crossing_into_a_level_bursts_once_and_keys_never_do() {
        let (mut s, now) = slider("low");
        s.press(0., true, now);
        s.drag_to(0.3, now);
        assert!(s.animation.busy());
        s.animation.clear();
        s.drag_to(0.31, now);
        assert!(!s.animation.busy(), "still medium");
        s.release();
        assert_eq!(s.key("end", now), Some(Some("max".to_string())));
        assert_eq!(s.key("right", now), Some(None));
        assert_eq!(s.key("a", now), None);
        assert!(!s.animation.busy());
    }

    #[test]
    fn it_works_with_any_number_of_levels() {
        let (mut s, now) = (EffortSlider::default(), Instant::now());
        let three = ["low", "medium", "high"].map(String::from);
        s.sync(&three, None, now);
        assert_eq!(s.shown, None);
        assert_eq!(s.key("right", now), Some(Some("low".to_string())));
        s.press(0., true, now);
        s.drag_to(1., now);
        assert_eq!(s.release(), Some("high".to_string()));
        let mut one = EffortSlider::default();
        one.sync(&["high".to_string()], None, now);
        assert_eq!(one.choose(0, now, true), Some("high".to_string()));
    }

    #[test]
    fn once_everything_settles_no_more_frames_are_asked_for() {
        let (mut s, now) = slider("low");
        s.choose(4, now, true);
        let frames = (0..=200).map(|i| s.frame(now + Duration::from_millis(8 * i), false));
        let last = frames.last().unwrap();
        assert!(!last.moving);
        assert_eq!((last.thumb, last.lights), (1., [None, None]));
        let mut s = slider("low").0;
        assert!(!s.frame(now, false).moving, "nothing moves on opening");
    }

    #[test]
    fn reduced_motion_jumps_the_thumb_and_plays_no_burst() {
        let (mut s, now) = slider("low");
        s.choose(4, now, true);
        let frame = s.frame(now, true);
        assert_eq!((frame.thumb, frame.lights, frame.still), (1., [None, None], true));
    }
}
