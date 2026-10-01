use crate::status::Status;
use agents::Summary;

/// The Dock badge: how many sessions need you, seen or not; none at zero.
pub fn badge_label(agents: &[Summary]) -> Option<String> {
    let n = agents.iter().filter(|a| Status::of(a) == Some(Status::NeedsYou)).count();
    (n > 0).then(|| n.to_string())
}

/// The label on the Dock icon, kept so the FFI call runs only on a change.
#[derive(Default)]
pub struct Badge(Option<String>);

impl Badge {
    pub fn show(&mut self, agents: &[Summary]) {
        let label = badge_label(agents);
        if label != self.0 {
            set_badge(label.as_deref());
            self.0 = label;
        }
    }
}

/// Sets the Dock badge; off the main thread it does nothing.
fn set_badge(label: Option<&str>) {
    #[cfg(target_os = "macos")]
    {
        use objc2::MainThreadMarker;
        use objc2_app_kit::NSApplication;
        use objc2_foundation::NSString;
        let Some(mtm) = MainThreadMarker::new() else { return };
        let label = label.map(NSString::from_str);
        NSApplication::sharedApplication(mtm).dockTile().setBadgeLabel(label.as_deref());
    }
    #[cfg(not(target_os = "macos"))]
    let _ = label;
}

#[cfg(test)]
mod tests {
    use super::badge_label;
    use agents::Summary;

    fn agent(id: &str, status: &str) -> Summary {
        Summary { id: id.into(), status: status.into(), attached: true, ..Default::default() }
    }

    #[test]
    fn the_badge_counts_needs_you_and_clears_at_zero() {
        let asking = [agent("a", "needsYou"), agent("b", "done"), agent("c", "needsYou"), Summary { attached: false, ..agent("d", "needsYou") }];
        assert_eq!(badge_label(&asking).as_deref(), Some("2"));
        assert_eq!(badge_label(&[agent("a", "working")]), None);
    }
}
