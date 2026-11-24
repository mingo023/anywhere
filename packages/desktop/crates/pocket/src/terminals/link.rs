use daemon::service::Service;
use std::time::{Duration, Instant};

pub(crate) const HINT_AFTER: Duration = Duration::from_secs(10);
/// How long pocketd gets to upgrade itself in place before the page offers a restart.
pub(crate) const STALE_AFTER: Duration = Duration::from_secs(15);
/// How long the page stays after pocketd answers as this build, to show the update done.
pub(crate) const HAND_BACK: Duration = Duration::from_millis(700);
pub(crate) const UPDATING: &str = "Anywhere's background service is updating; try again in a moment.";

pub(crate) fn replaced(ours: &str, on_disk: Option<&str>) -> bool {
    on_disk.is_some_and(|v| v != ours)
}

/// What the page tells the user once pocketd has been gone a while.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hint {
    /// No bundled service (dev build): install one by hand.
    Install,
    /// The user switched Anywhere off in Login Items.
    LoginItems,
    /// The bundled service is on, yet pocketd isn't answering.
    Reopen,
    /// pocketd is a build this app didn't ship and hasn't upgraded itself in place.
    Stale,
}

/// Where an update of pocketd to this app's build stands, as its page shows it.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Update {
    /// Waiting this long for pocketd to come back as this build.
    Waiting(Duration),
    /// pocketd couldn't update in place and needs a restart.
    Stuck,
    /// pocketd answered as this build; how far through the hand-back, 0 to 1.
    Back(f32),
}

/// Whether the connection to pocketd is up, and since when it isn't.
#[derive(Default)]
pub(crate) struct Link {
    down_since: Option<Instant>,
    /// pocketd's build when it isn't this app's, and since when.
    stale_since: Option<(String, Instant)>,
    /// pocketd went away since it last answered as another build: it is likely exec'ing the new one.
    dropped: bool,
    /// When pocketd last answered as this build after being another.
    back: Option<Instant>,
    /// The bundled service's state; only release builds read it.
    pub(crate) service: Option<Service>,
}

impl Link {
    pub(crate) fn down(&mut self, now: Instant) {
        self.down_since.get_or_insert(now);
        self.back = None;
        self.dropped |= self.stale_since.is_some();
    }

    pub(crate) fn up(&mut self) {
        self.down_since = None;
    }

    /// hello.ok arrived: a release app expects the pocketd it ships, which upgrades itself in place within seconds.
    pub(crate) fn connected(&mut self, ours: &str, theirs: &str, release: bool, now: Instant) {
        self.dropped = false;
        let was = self.stale_since.take();
        let since = was.as_ref().map_or(now, |(_, t)| *t);
        self.stale_since = (release && ours != theirs).then(|| (theirs.to_string(), since));
        if was.is_some() && self.stale_since.is_none() {
            self.back = Some(now);
        }
    }

    /// Down, or up on a pocketd whose protocol may not be this app's.
    pub(crate) fn is_down(&self) -> bool {
        self.down_since.is_some() || self.stale_since.is_some()
    }

    /// The build pocketd runs while the app waits for it to update.
    pub(crate) fn stale(&self) -> Option<&str> {
        self.stale_since.as_ref().map(|(v, _)| v.as_str())
    }

    /// The update's stage at `now`; none when pocketd is just down, or answers as this build and the hand-back is over.
    pub(crate) fn update(&self, now: Instant) -> Option<Update> {
        if let Some((_, since)) = &self.stale_since {
            return Some(if self.hint(now) == Some(Hint::Stale) { Update::Stuck } else { Update::Waiting(now.duration_since(*since)) });
        }
        if self.down_since.is_some() {
            return None;
        }
        let t = now.duration_since(self.back?).as_secs_f32() / HAND_BACK.as_secs_f32();
        (t < 1.).then_some(Update::Back(t))
    }

    pub(crate) fn hint(&self, now: Instant) -> Option<Hint> {
        if let Some((_, since)) = &self.stale_since
            && !self.dropped
        {
            return (now.duration_since(*since) >= STALE_AFTER).then_some(Hint::Stale);
        }
        self.down_since.filter(|t| now.duration_since(*t) >= HINT_AFTER)?;
        Some(match self.service {
            None => Hint::Install,
            Some(Service::RequiresApproval) => Hint::LoginItems,
            Some(_) => Hint::Reopen,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_app_swapped_on_disk_for_another_build_is_replaced() {
        assert!(replaced("0.1.60", Some("0.1.61")));
        assert!(!replaced("0.1.61", Some("0.1.61")), "the app on disk is this build");
        assert!(!replaced("0.1.60", None), "no readable bundle, as in a dev build run from target/");
    }

    #[test]
    fn the_install_hint_waits_ten_seconds_from_the_first_down() {
        let t = Instant::now();
        let mut l = Link::default();
        l.down(t);
        l.down(t + Duration::from_secs(5));
        assert_eq!((l.is_down(), l.hint(t + Duration::from_secs(9)), l.hint(t + HINT_AFTER)), (true, None, Some(Hint::Install)));
        l.up();
        assert_eq!((l.is_down(), l.hint(t + HINT_AFTER)), (false, None));
    }

    #[test]
    fn a_switched_off_service_points_at_login_items() {
        let t = Instant::now();
        let mut l = Link { service: Some(Service::RequiresApproval), ..Link::default() };
        l.down(t);
        assert_eq!((l.hint(t + Duration::from_secs(9)), l.hint(t + HINT_AFTER)), (None, Some(Hint::LoginItems)));
    }

    #[test]
    fn a_bundled_service_that_is_on_but_silent_asks_for_a_reopen() {
        let t = Instant::now();
        let hints: Vec<_> = [Service::Enabled, Service::NotRegistered, Service::NotFound]
            .into_iter()
            .map(|s| {
                let mut l = Link { service: Some(s), ..Link::default() };
                l.down(t);
                l.hint(t + HINT_AFTER)
            })
            .collect();
        assert_eq!(hints, [Some(Hint::Reopen); 3]);
    }

    #[test]
    fn a_release_app_waits_on_a_pocketd_of_another_build_then_offers_a_restart() {
        let t = Instant::now();
        let mut l = Link::default();
        l.connected("0.2.0", "0.1.0", true, t);
        assert_eq!((l.is_down(), l.stale(), l.hint(t + Duration::from_secs(14))), (true, Some("0.1.0"), None));
        l.down(t + Duration::from_secs(2));
        l.up();
        l.connected("0.2.0", "0.1.0", true, t + Duration::from_secs(3));
        assert_eq!(l.hint(t + STALE_AFTER), Some(Hint::Stale));
        l.connected("0.2.0", "0.2.0", true, t + Duration::from_secs(20));
        assert_eq!((l.is_down(), l.stale(), l.hint(t + Duration::from_secs(60))), (false, None, None));
    }

    #[test]
    fn an_outdated_pocketd_that_goes_away_is_upgrading_and_gets_no_restart_until_it_answers_old_again() {
        let (t, gone) = (Instant::now(), Duration::from_secs(5));
        let mut l = Link { service: Some(Service::Enabled), ..Link::default() };
        l.connected("0.2.0", "0.1.0", true, t);
        l.down(t + gone);
        assert_eq!((l.stale(), l.hint(t + gone + HINT_AFTER)), (Some("0.1.0"), Some(Hint::Reopen)));
        l.up();
        assert_eq!((l.is_down(), l.hint(t + Duration::from_secs(60))), (true, None));
        l.connected("0.2.0", "0.1.0", true, t + Duration::from_secs(61));
        assert_eq!(l.hint(t + Duration::from_secs(61)), Some(Hint::Stale));
    }

    #[test]
    fn an_update_waits_then_hands_back_once_pocketd_answers_as_this_build() {
        let t = Instant::now();
        let mut l = Link::default();
        assert_eq!(l.update(t), None);
        l.connected("0.2.0", "0.1.0", true, t);
        l.down(t + Duration::from_secs(1));
        assert_eq!(l.update(t + Duration::from_secs(3)), Some(Update::Waiting(Duration::from_secs(3))));
        l.up();
        l.connected("0.2.0", "0.2.0", true, t + Duration::from_secs(4));
        assert_eq!(l.update(t + Duration::from_secs(4)), Some(Update::Back(0.)));
        assert_eq!(l.update(t + Duration::from_secs(4) + HAND_BACK), None);
        l.connected("0.2.0", "0.1.0", true, t + Duration::from_secs(5));
        l.connected("0.2.0", "0.2.0", true, t + Duration::from_secs(6));
        l.down(t + Duration::from_secs(6));
        assert_eq!(l.update(t + Duration::from_secs(6)), None, "a pocketd gone again isn't updated");
        l.up();
        assert_eq!(l.update(t + Duration::from_secs(6)), None, "nor once it answers again");
    }

    #[test]
    fn an_update_that_cant_happen_in_place_is_stuck_and_a_plain_restart_hands_nothing_back() {
        let t = Instant::now();
        let mut l = Link::default();
        l.connected("0.2.0", "0.1.0", true, t);
        assert_eq!(l.update(t + STALE_AFTER), Some(Update::Stuck));
        let mut fresh = Link::default();
        fresh.down(t);
        fresh.up();
        fresh.connected("0.2.0", "0.2.0", true, t);
        assert_eq!((fresh.update(t), fresh.update(t + HINT_AFTER)), (None, None));
    }

    #[test]
    fn a_dev_app_takes_any_pocketd_and_a_release_app_none_without_a_version() {
        let t = Instant::now();
        let mut l = Link::default();
        l.connected("dev", "8a2e4402-dirty", false, t);
        assert!(!l.is_down());
        l.connected("0.2.0", "", true, t);
        assert_eq!(l.stale(), Some(""));
    }
}
