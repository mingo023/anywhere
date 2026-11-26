use crate::desktop::Desktop;
use crate::glide::Glide;
use crate::updates::Update;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use store::Store;
use theme::*;

const REPO_URL: &str = "https://github.com/mingo023/anywhere";
const WAIT_SECS: u64 = 3 * 24 * 60 * 60;
const THANKS: Duration = Duration::from_millis(1400);
const POP: Duration = Duration::from_millis(420);
/// The card's fixed height, so it can grow in from nothing: text that would wrap truncates instead.
const CARD: f32 = 94.;
const GAP: f32 = 8.;
const RISE: f32 = 10.;

pub fn unix_now() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map_or(0, |d| d.as_secs())
}

/// Whether a launch at `now` asks for a star: from `WAIT_SECS` after the first launch until it is answered, and never over an update card.
fn asks(first_launch: u64, now: u64, asked: bool, update: &Update) -> bool {
    !asked && *update == Update::Idle && now >= first_launch.saturating_add(WAIT_SECS)
}

pub enum StarEvent {
    Star,
    Close,
}

/// The sidebar's "Star on GitHub" card: whether it shows, and its rise in and out.
pub struct Star {
    pub(crate) shown: bool,
    pub(crate) thanked: bool,
    glide: Glide,
}

impl Star {
    /// Records the first launch, then decides once per launch whether to ask.
    pub fn new(store: &mut Store, update: &Update, now: u64) -> Self {
        let first = *store.first_launch.get_or_insert(now);
        Self { shown: asks(first, now, store.star_asked, update), thanked: false, glide: Glide::from(0.).response(0.35) }
    }

    /// Steps the rise; `jump` lands at once, for a capture. Reduced motion still steps it, since the card then only fades.
    pub fn tick(&mut self, now: Instant, jump: bool) -> bool {
        self.glide.tick(f32::from(u8::from(self.shown)), now, jump)
    }

    pub fn at(&self) -> f32 {
        self.glide.at().clamp(0., 1.)
    }

    pub fn visible(&self) -> bool {
        self.shown || self.at() > 0.
    }

    pub fn thank(&mut self) {
        self.thanked = true;
    }

    pub fn close(&mut self) {
        self.shown = false;
    }
}

fn answered(store: &mut Store) {
    store.star_asked = true;
}

impl EventEmitter<StarEvent> for Star {}

impl Render for Star {
    /// "Enjoying Anywhere?" with a Star button, once, from a few days after the first launch.
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.visible() {
            return div();
        }
        let (at, thanked, reduce_motion) = (self.at(), self.thanked, cx.reduce_motion());
        let dismiss = div()
            .id("aside-star-dismiss")
            .size(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .hover(|s| s.bg(FILL_3))
            .active(|s| s.opacity(0.6))
            .child(icon("x", 12., TEXT_3))
            .on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(StarEvent::Close)));
        let copy = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(2.))
            .child(div().h(px(18.)).truncate().text_size(px(12.5)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("Enjoying Zoes?"))
            .child(div().h(px(16.)).truncate().text_size(px(12.)).text_color(TEXT_2).child("A star helps others find it."));
        let label = if thanked {
            div()
                .child("Thank you!")
                .with_animation("aside-star-thanks", Animation::new(std::time::Duration::from_millis(180)), |d, t| d.opacity(t))
                .into_any_element()
        } else {
            div().flex().items_center().gap(px(6.)).child(icon("star-fill", 12., ui::Variant::Primary.fg())).child("Star on GitHub").into_any_element()
        };
        let star = ui::button("aside-star", ui::Variant::Primary, None, label)
            .mt(px(10.))
            .w_full()
            .h(px(28.))
            .rounded(px(14.))
            .justify_center()
            .text_size(px(12.5))
            .when(thanked, |b| b.cursor_default())
            .when(!thanked, |b| b.on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(StarEvent::Star))));
        let card = div()
            .id("aside-star-card")
            .relative()
            .top(px(if reduce_motion { 0. } else { RISE * (1. - at) }))
            .opacity(if reduce_motion { at } else { (at * 1.4 - 0.2).clamp(0., 1.) })
            .mt(px(GAP))
            .mx(px(2.))
            .h(px(CARD))
            .py(px(10.))
            .pl(px(12.))
            .pr(px(10.))
            .flex()
            .flex_col()
            .rounded(px(12.))
            .bg(SIDE_NOTICE)
            .shadow(vec![ui::ring(FILL_3, 0.5)])
            .child(
                div()
                    .flex()
                    .items_start()
                    .gap(px(8.))
                    .child(div().mt(px(1.)).child(glyph("aside-star-pop", thanked, 16., reduce_motion)))
                    .child(copy)
                    .child(dismiss),
            )
            .child(star);
        // Clipped only while it grows or shrinks, so the settled card keeps its ring.
        div().flex_none().when(at < 1., |d| d.h(px((GAP + CARD + 1.) * at)).overflow_hidden()).child(card)
    }
}

/// The star, outlined until thanked, then filled with a pop.
pub fn glyph(id: &'static str, thanked: bool, side: f32, reduce_motion: bool) -> AnyElement {
    if !thanked {
        return icon("star", side, TEXT_2).into_any_element();
    }
    let filled = icon("star-fill", side, WAITING_DOT);
    if reduce_motion {
        return filled.into_any_element();
    }
    filled.with_animation(id, Animation::new(POP), |i, t| i.with_transformation(Transformation::scale(size(pop(t), pop(t))))).into_any_element()
}

fn pop(t: f32) -> f32 {
    let lerp = |a: f32, b: f32, t: f32| a + (b - a) * t;
    match t {
        t if t < 0.35 => lerp(1., 1.35, t / 0.35),
        t if t < 0.7 => lerp(1.35, 0.94, (t - 0.35) / 0.35),
        t => lerp(0.94, 1., (t - 0.7) / 0.3),
    }
}

impl Desktop {
    pub(crate) fn on_star_event(&mut self, _: &Entity<Star>, ev: &StarEvent, _: &mut Window, cx: &mut Context<Self>) {
        match ev {
            StarEvent::Star => self.star_on_github(cx),
            StarEvent::Close => self.close_star(cx),
        }
    }

    pub(crate) fn star_on_github(&mut self, cx: &mut Context<Self>) {
        // A capture runs on the user's Mac, where opening a URL would open their browser.
        if !self.capturing {
            cx.open_url(REPO_URL);
        }
        self.star.update(cx, |s, _| s.thank());
        answered(&mut self.store);
        self.save_soon(cx);
        cx.notify();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(THANKS).await;
            this.update(cx, |d, cx| {
                d.star.update(cx, |s, _| s.close());
                cx.notify();
            })
        })
        .detach();
    }

    pub(crate) fn close_star(&mut self, cx: &mut Context<Self>) {
        self.star.update(cx, |s, _| s.close());
        answered(&mut self.store);
        self.save_soon(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{Star, WAIT_SECS, answered};
    use crate::updates::Update;
    use store::Store;

    const T0: u64 = 1_700_000_000;

    fn started() -> Store {
        let mut store = Store::default();
        store.first_launch = Some(T0);
        store
    }

    fn launch(store: &mut Store, now: u64) -> bool {
        Star::new(store, &Update::Idle, now).shown
    }

    #[test]
    fn a_first_launch_records_its_time_and_does_not_ask() {
        let mut store = Store::default();
        assert!(!launch(&mut store, T0));
        assert_eq!(store.first_launch, Some(T0));
    }

    #[test]
    fn it_asks_from_three_days_after_the_first_launch_and_not_a_second_sooner() {
        let mut store = Store::default();
        launch(&mut store, T0);
        assert!(!launch(&mut store, T0 + WAIT_SECS - 1));
        assert!(launch(&mut store, T0 + WAIT_SECS));
        assert_eq!(store.first_launch, Some(T0));
    }

    #[test]
    fn starring_or_closing_means_it_never_asks_again() {
        let later = T0 + WAIT_SECS * 10;
        for starred in [true, false] {
            let mut store = started();
            let mut star = Star::new(&mut store, &Update::Idle, later);
            if starred {
                star.thank()
            } else {
                star.close()
            }
            answered(&mut store);
            assert!(!launch(&mut store, later + 1), "starred: {starred}");
        }
    }

    #[test]
    fn an_update_card_takes_the_launch_and_the_ask_waits_for_a_later_one() {
        let later = T0 + WAIT_SECS;
        let mut store = started();
        assert!(!Star::new(&mut store, &Update::WhatsNew { version: "0.2.0".into() }, later).shown);
        assert!(launch(&mut store, later + 1));
    }

    #[test]
    fn a_clock_set_before_the_first_launch_does_not_ask() {
        let mut store = started();
        assert!(!launch(&mut store, T0 - 1));
    }
}
