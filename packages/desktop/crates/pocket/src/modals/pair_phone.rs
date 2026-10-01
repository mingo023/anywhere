mod qr;

use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use crate::util::now_ms;
use agents::Event;
use gpui_kit::*;
use qr::Qr;
use std::time::{Duration, Instant};
use theme::*;
use ui::{self, Variant};

const CLOSE_AFTER: Duration = Duration::from_millis(1500);

enum Stage {
    Waiting,
    Offered { code: String, expires: Instant, qr: Option<Qr> },
    Expired,
    Paired(Instant),
    Failed(String),
}

#[derive(Debug, PartialEq)]
enum PairText {
    Waiting,
    Code { code: String, left: String },
    Expired,
    Paired,
    Failed(String),
}

fn countdown(left: Duration) -> String {
    let s = left.as_secs();
    format!("Expires in {}:{:02}", s / 60, s % 60)
}

pub struct PairPhone {
    stage: Stage,
    ticking: Option<Task<()>>,
}

impl Default for PairPhone {
    fn default() -> Self {
        Self { stage: Stage::Waiting, ticking: None }
    }
}

impl PairPhone {
    /// `expires_at` and `now_ms` are Unix ms; the countdown runs on `now`'s monotonic clock.
    fn offered(&mut self, url: &str, code: String, expires_at: i64, now_ms: i64, now: Instant) {
        let left = Duration::from_millis((expires_at - now_ms).max(0) as u64);
        self.stage = Stage::Offered { code, expires: now + left, qr: Qr::encode(url) };
    }

    fn paired(&mut self, now: Instant) {
        self.stage = Stage::Paired(now);
    }

    fn failed(&mut self, message: String) {
        self.stage = Stage::Failed(message);
    }

    /// pocketd restarted, so the code it issued, or the one on its way, is gone.
    pub(crate) fn lost(&mut self) {
        if matches!(self.stage, Stage::Waiting | Stage::Offered { .. }) {
            self.stage = Stage::Expired;
        }
    }

    fn text(&self, now: Instant) -> PairText {
        match &self.stage {
            Stage::Waiting => PairText::Waiting,
            Stage::Offered { expires, .. } if now >= *expires => PairText::Expired,
            Stage::Offered { code, expires, .. } => PairText::Code { code: code.clone(), left: countdown(*expires - now) },
            Stage::Expired => PairText::Expired,
            Stage::Paired(_) => PairText::Paired,
            Stage::Failed(m) => PairText::Failed(m.clone()),
        }
    }

    fn closes(&self, now: Instant) -> bool {
        matches!(self.stage, Stage::Paired(at) if now >= at + CLOSE_AFTER)
    }

    fn qr(&self) -> Option<&Qr> {
        match &self.stage {
            Stage::Offered { qr, .. } => qr.as_ref(),
            _ => None,
        }
    }
}

impl Desktop {
    pub(crate) fn begin_pairing(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pair = PairPhone::default();
        self.outbox.pair_begin();
        self.pair.ticking = Some(cx.spawn_in(window, async move |this, cx| {
            loop {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                let open = this.update_in(cx, |d, window, cx| {
                    if d.pair.closes(Instant::now()) {
                        d.close_overlay(window, cx);
                    }
                    cx.notify();
                    d.overlay == Some(Overlay::PairPhone)
                });
                if !matches!(open, Ok(true)) {
                    break;
                }
            }
        }));
    }

    pub(crate) fn on_pair(&mut self, ev: Event, cx: &mut Context<Self>) {
        if self.overlay != Some(Overlay::PairPhone) {
            return;
        }
        let now = Instant::now();
        match ev {
            Event::PairCode { url, code, expires_at } => self.pair.offered(&url, code, expires_at, now_ms(), now),
            Event::Paired(_) => self.pair.paired(now),
            Event::PairFailed(message) => self.pair.failed(message),
            _ => return,
        }
        cx.notify();
    }

    pub(super) fn pair_view(&mut self, cx: &mut Context<Self>) -> Div {
        let note = |text: String| div().text_size(px(13.5)).text_color(TEXT_2).child(text).into_any_element();
        let card = |inner: AnyElement| div().flex().justify_center().child(div().p(px(16.)).rounded(px(12.)).bg(WHITE).child(inner)).into_any_element();
        let blank = || div().size(px(qr::SIDE)).into_any_element();
        let new_code = || {
            ui::large(ui::button("pair-new", Variant::Secondary, None, "New code"))
                .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.begin_pairing(window, cx)))
                .into_any_element()
        };
        let body = match self.pair.text(Instant::now()) {
            PairText::Waiting => vec![note("Scan with the iPhone Camera".into()), card(blank())],
            PairText::Code { code, left } => vec![
                note("Scan with the iPhone Camera".into()),
                card(self.pair.qr().map_or_else(blank, |q| qr::view(q).into_any_element())),
                div()
                    .flex()
                    .items_baseline()
                    .gap(px(6.))
                    .text_size(px(13.5))
                    .text_color(TEXT_2)
                    .child("Or enter this code:")
                    .child(div().font_family(MONO).text_size(px(14.)).text_color(TEXT).child(code))
                    .into_any_element(),
                div().font_family(MONO).text_size(px(12.)).text_color(TEXT_3).child(left).into_any_element(),
            ],
            PairText::Expired => vec![note("Code expired".into()), div().child(new_code()).into_any_element()],
            PairText::Paired => vec![note("Paired".into())],
            PairText::Failed(message) => vec![note(message), div().child(new_code()).into_any_element()],
        };
        let close = ui::icon_button("pair-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        ui::modal("Pair phone", 440., 160., close, body)
    }
}

#[cfg(test)]
mod tests {
    use super::{CLOSE_AFTER, PairPhone, PairText, countdown};
    use std::time::{Duration, Instant};

    const URL: &str = "anywhere://pair?v=1&h=100.64.0.1:4517&c=abcdefghijklmnopqrstuv";
    const NOW_MS: i64 = 1_790_000_000_000;

    fn offered(left_ms: i64, now: Instant) -> PairPhone {
        let mut p = PairPhone::default();
        p.offered(URL, "abcdefghijklmnopqrstuv".into(), NOW_MS + left_ms, NOW_MS, now);
        p
    }

    #[test]
    fn the_countdown_reads_minutes_and_seconds() {
        let now = Instant::now();
        let p = offered(299_900, now);
        assert_eq!(p.text(now), PairText::Code { code: "abcdefghijklmnopqrstuv".into(), left: "Expires in 4:59".into() });
        assert_eq!(countdown(Duration::from_secs(9)), "Expires in 0:09");
    }

    #[test]
    fn an_expired_code_offers_a_new_one() {
        let now = Instant::now();
        let p = offered(5_000, now);
        assert_eq!(p.text(now + Duration::from_secs(5)), PairText::Expired);
    }

    #[test]
    fn a_code_that_expired_on_its_way_reads_expired() {
        let now = Instant::now();
        assert_eq!(offered(-1_000, now).text(now), PairText::Expired);
    }

    #[test]
    fn a_failure_shows_its_message() {
        let mut p = offered(300_000, Instant::now());
        p.failed("Tailscale isn't running.".into());
        assert_eq!(p.text(Instant::now()), PairText::Failed("Tailscale isn't running.".into()));
    }

    #[test]
    fn paired_replaces_the_code_and_closes_a_moment_later() {
        let now = Instant::now();
        let mut p = offered(300_000, now);
        p.paired(now);
        assert_eq!(p.text(now), PairText::Paired);
        assert!(!p.closes(now + Duration::from_millis(1400)));
        assert!(p.closes(now + CLOSE_AFTER));
    }

    #[test]
    fn a_restarted_pocketd_expires_the_code_but_not_a_pairing() {
        let now = Instant::now();
        let mut p = offered(300_000, now);
        p.lost();
        assert_eq!(p.text(now), PairText::Expired);
        p.paired(now);
        p.lost();
        assert_eq!(p.text(now), PairText::Paired);
    }
}
