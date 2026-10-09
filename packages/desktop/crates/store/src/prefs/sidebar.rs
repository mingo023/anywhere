use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SessionSort {
    #[default]
    Newest,
    Status,
    Activity,
    Name,
}

/// What a session row shows under and beside its title.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
#[serde(default)]
pub struct Details {
    pub model: bool,
    pub time: bool,
    pub branch: bool,
    pub diff: bool,
}

impl Default for Details {
    fn default() -> Self {
        Self { model: true, time: true, branch: true, diff: true }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum InboxSort {
    /// Needs you, then Failed, then Done; oldest first within each.
    #[default]
    Urgent,
    Newest,
    Oldest,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum InboxFilter {
    #[default]
    All,
    NeedsYou,
    Failed,
    Done,
}

/// Sidebar and inbox.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Sidebar {
    #[serde(deserialize_with = "crate::lenient")]
    pub sort: SessionSort,
    pub details: Details,
    /// Lists worktrees made outside the app, not only the ones it made or imported.
    pub external_worktrees: bool,
    pub usage_card: bool,
    /// Percent of the context window used at which it turns amber, then red.
    pub warn_at: u32,
    pub critical_at: u32,
    pub jump_hints: bool,
    pub hint_delay_ms: u32,
    #[serde(deserialize_with = "crate::lenient")]
    pub inbox_sort: InboxSort,
    #[serde(deserialize_with = "crate::lenient")]
    pub inbox_filter: InboxFilter,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self {
            sort: SessionSort::default(),
            details: Details::default(),
            external_worktrees: false,
            usage_card: true,
            warn_at: 75,
            critical_at: 90,
            jump_hints: true,
            hint_delay_ms: 280,
            inbox_sort: InboxSort::default(),
            inbox_filter: InboxFilter::default(),
        }
    }
}

impl Sidebar {
    /// Sets the amber threshold, pushing red above it.
    pub fn set_warn(&mut self, pct: u32) {
        self.warn_at = pct;
        self.critical_at = self.critical_at.max(pct.saturating_add(1));
    }

    /// Sets the red threshold, pulling amber below it.
    pub fn set_critical(&mut self, pct: u32) {
        self.critical_at = pct;
        self.warn_at = self.warn_at.min(pct.saturating_sub(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn red_always_stays_above_amber() {
        let mut s = Sidebar::default();
        s.set_warn(95);
        assert_eq!((s.warn_at, s.critical_at), (95, 96));
        s.set_critical(60);
        assert_eq!((s.warn_at, s.critical_at), (59, 60));
    }

    #[test]
    fn an_unknown_sort_falls_back_alone() {
        let s: Sidebar = serde_json::from_str(r#"{"sort":"random","usage_card":false,"inbox_filter":"needsYou"}"#).unwrap();
        assert_eq!((s.sort, s.usage_card, s.inbox_filter), (SessionSort::Newest, false, InboxFilter::NeedsYou));
    }

    #[test]
    fn a_sidebar_from_before_these_settings_keeps_todays_list() {
        let s: Sidebar = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Sidebar::default());
        assert_eq!((s.sort, s.details, s.external_worktrees, s.warn_at, s.critical_at, s.hint_delay_ms), (SessionSort::Newest, Details::default(), false, 75, 90, 280));
    }
}
