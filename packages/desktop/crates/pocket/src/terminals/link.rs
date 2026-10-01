use std::time::{Duration, Instant};

pub(crate) const HINT_AFTER: Duration = Duration::from_secs(10);

/// Whether the connection to pocketd is up, and since when it isn't.
#[derive(Default)]
pub(crate) struct Link {
    down_since: Option<Instant>,
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

    /// Whether pocketd has been gone long enough that it likely isn't installed as a service.
    pub(crate) fn hint(&self, now: Instant) -> bool {
        self.down_since.is_some_and(|t| now.duration_since(t) >= HINT_AFTER)
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
        assert_eq!((l.is_down(), l.hint(t + Duration::from_secs(9)), l.hint(t + HINT_AFTER)), (true, false, true));
        l.up();
        assert_eq!((l.is_down(), l.hint(t + HINT_AFTER)), (false, false));
    }
}
