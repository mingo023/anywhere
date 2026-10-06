use daemon::service::Service;
use std::time::{Duration, Instant};

pub(crate) const HINT_AFTER: Duration = Duration::from_secs(10);

/// What the page tells the user once pocketd has been gone a while.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Hint {
    /// No bundled service (dev build): install one by hand.
    Install,
    /// The user switched Anywhere off in Login Items.
    LoginItems,
    /// The bundled service is on, yet pocketd isn't answering.
    Reopen,
}

/// Whether the connection to pocketd is up, and since when it isn't.
#[derive(Default)]
pub(crate) struct Link {
    down_since: Option<Instant>,
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

    pub(crate) fn is_down(&self) -> bool {
        self.down_since.is_some()
    }

    pub(crate) fn hint(&self, now: Instant) -> Option<Hint> {
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
}
