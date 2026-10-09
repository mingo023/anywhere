use crate::inbox;
use crate::status::Status;
use agents::Summary;
use store::DockBadge;

/// The Dock badge: by default the bell's count, so every session in the inbox; none at zero.
pub fn badge_label(agents: &[Summary], counts: DockBadge) -> Option<String> {
    let n = match counts {
        DockBadge::Off => 0,
        DockBadge::NeedsYou => agents.iter().filter(|a| inbox::noted(a) == Some(Status::NeedsYou)).count(),
        DockBadge::Inbox => agents.iter().filter(|a| inbox::noted(a).is_some()).count(),
    };
    (n > 0).then(|| n.to_string())
}

/// The label on the Dock icon, kept so the FFI call runs only on a change.
#[derive(Default)]
pub struct Badge(Option<String>);

impl Badge {
    pub fn show(&mut self, agents: &[Summary], counts: DockBadge) {
        let label = badge_label(agents, counts);
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
    use store::DockBadge;

    fn agent(id: &str, status: &str) -> Summary {
        Summary { id: id.into(), status: status.into(), attached: true, ..Default::default() }
    }

    #[test]
    fn the_badge_counts_what_the_bell_counts_and_clears_at_zero() {
        let noted = [agent("a", "needsYou"), agent("b", "done"), Summary { failed: true, ..agent("c", "done") }, agent("w", "working"), Summary { attached: false, ..agent("d", "needsYou") }];
        assert_eq!(badge_label(&noted, DockBadge::Inbox).as_deref(), Some("3"));
        assert_eq!(badge_label(&[agent("a", "working")], DockBadge::Inbox), None);
    }

    #[test]
    fn the_badge_can_count_only_sessions_that_need_you_or_nothing() {
        let list = [agent("a", "needsYou"), agent("b", "done"), agent("c", "needsYou")];
        assert_eq!(badge_label(&list, DockBadge::NeedsYou).as_deref(), Some("2"));
        assert_eq!(badge_label(&list, DockBadge::Off), None);
    }
}
