use crate::inbox;
use agents::Agents;
use store::DockBadge;

/// The Dock badge: by default the bell's count, so every row in the Inbox; none at zero.
pub fn badge_label(agents: &Agents, counts: DockBadge) -> Option<String> {
    let n = match counts {
        DockBadge::Off => 0,
        DockBadge::NeedsYou => inbox::needing(agents),
        DockBadge::Inbox => inbox::count(agents),
    };
    (n > 0).then(|| n.to_string())
}

/// The label on the Dock icon, kept so the FFI call runs only on a change.
#[derive(Default)]
pub struct Badge(Option<String>);

impl Badge {
    pub fn show(&mut self, agents: &Agents, counts: DockBadge) {
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
    use agents::{Agents, Summary};
    use store::DockBadge;

    fn agent(id: &str, status: &str) -> Summary {
        Summary { id: id.into(), status: status.into(), attached: true, ..Default::default() }
    }

    fn agents(list: Vec<Summary>) -> Agents {
        Agents { list, ..Default::default() }
    }

    #[test]
    fn the_badge_counts_what_the_bell_counts_and_clears_at_zero() {
        let noted = agents(vec![agent("a", "needsYou"), agent("b", "done"), Summary { failed: true, ..agent("c", "done") }, agent("w", "working"), Summary { attached: false, ..agent("d", "needsYou") }]);
        assert_eq!(badge_label(&noted, DockBadge::Inbox).as_deref(), Some("3"));
        assert_eq!(badge_label(&noted, DockBadge::Inbox).map(|n| n.parse::<usize>().unwrap()), Some(crate::inbox::count(&noted)));
        assert_eq!(badge_label(&agents(vec![agent("a", "working")]), DockBadge::Inbox), None);
    }

    #[test]
    fn the_badge_can_count_only_sessions_that_need_you_or_nothing() {
        let list = agents(vec![agent("a", "needsYou"), agent("b", "done"), agent("c", "needsYou")]);
        assert_eq!(badge_label(&list, DockBadge::NeedsYou).as_deref(), Some("2"));
        assert_eq!(badge_label(&list, DockBadge::Off), None);
    }
}
