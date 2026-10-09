use crate::desktop::Desktop;
use gpui_kit::*;
use std::time::{Duration, Instant};
use store::prefs::terminal::Bell as Ring;

/// Bells closer together than this ring once, so a program printing a binary file doesn't ring hundreds.
const QUIET: Duration = Duration::from_millis(250);
const FLASH: Duration = Duration::from_millis(150);

#[derive(Default)]
pub struct Bell {
    last: Option<Instant>,
    /// The terminal flashing for its bell.
    pub(crate) flashing: Option<String>,
}

impl Bell {
    /// Whether a bell at `now` rings, rather than folding into the one just rung.
    pub fn rings(&mut self, now: Instant) -> bool {
        if self.last.is_some_and(|last| now.duration_since(last) < QUIET) {
            return false;
        }
        self.last = Some(now);
        true
    }
}

impl Desktop {
    pub(crate) fn ring(&mut self, id: &str, cx: &mut Context<Self>) {
        let ring = self.store.terminal.bell;
        if ring == Ring::Off || !self.terminals.bell.rings(Instant::now()) {
            return;
        }
        if ring == Ring::Sound {
            objc2_app_kit::NSBeep();
            return;
        }
        self.terminals.bell.flashing = Some(id.to_string());
        let id = id.to_string();
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(FLASH).await;
            this.update(cx, |d, cx| {
                if d.terminals.bell.flashing.as_ref() == Some(&id) {
                    d.terminals.bell.flashing = None;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{Bell, QUIET};
    use std::time::{Duration, Instant};

    #[test]
    fn a_burst_of_bells_rings_once() {
        let (mut b, t) = (Bell::default(), Instant::now());
        assert!(b.rings(t));
        assert!(!b.rings(t + Duration::from_millis(10)));
        assert!(!b.rings(t + QUIET - Duration::from_millis(1)));
        assert!(b.rings(t + QUIET));
    }
}
