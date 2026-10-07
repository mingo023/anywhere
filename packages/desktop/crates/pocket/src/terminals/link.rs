use daemon::service::Service;
use std::time::{Duration, Instant};

pub(crate) const HINT_AFTER: Duration = Duration::from_secs(10);
/// How long pocketd gets to upgrade itself in place before the page offers a restart.
pub(crate) const STALE_AFTER: Duration = Duration::from_secs(15);
pub(crate) const UPDATING: &str = "Anywhere's background service is updating; try again in a moment.";

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

/// Whether the connection to pocketd is up, and since when it isn't.
#[derive(Default)]
pub(crate) struct Link {
    down_since: Option<Instant>,
    /// pocketd's build when it isn't this app's, and since when.
    stale_since: Option<(String, Instant)>,
    /// The bundled service's state; only release builds read it.
    pub(crate) service: Option<Service>,
}

impl Link {
    pub(crate) fn down(&mut self, now: Instant) {
        self.down_since.get_or_insert(now);
    }

    pub(crate) fn up(&mut self) {
        self.down_since = None;
    }

    /// hello.ok arrived: a release app expects the pocketd it ships, which upgrades itself in place within seconds.
    pub(crate) fn connected(&mut self, ours: &str, theirs: &str, release: bool, now: Instant) {
        let since = self.stale_since.take().map_or(now, |(_, t)| t);
        self.stale_since = (release && ours != theirs).then(|| (theirs.to_string(), since));
    }

    /// Down, or up on a pocketd whose protocol may not be this app's.
    pub(crate) fn is_down(&self) -> bool {
        self.down_since.is_some() || self.stale_since.is_some()
    }

    /// The build pocketd runs while the app waits for it to update.
    pub(crate) fn stale(&self) -> Option<&str> {
        self.stale_since.as_ref().map(|(v, _)| v.as_str())
    }

    pub(crate) fn hint(&self, now: Instant) -> Option<Hint> {
        if let Some((_, since)) = &self.stale_since {
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
    fn a_dev_app_takes_any_pocketd_and_a_release_app_none_without_a_version() {
        let t = Instant::now();
        let mut l = Link::default();
        l.connected("dev", "8a2e4402-dirty", false, t);
        assert!(!l.is_down());
        l.connected("0.2.0", "", true, t);
        assert_eq!(l.stale(), Some(""));
    }
}
