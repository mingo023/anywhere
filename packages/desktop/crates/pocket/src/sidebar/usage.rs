use crate::desktop::Desktop;
use agents::{Agents, Level, level, percent};
use store::prefs::Sidebar;

/// Context left in each provider's newest session, and how full it is against the sidebar's thresholds.
fn usage(agents: &Agents, s: &Sidebar) -> Vec<(&'static str, u64, Level)> {
    let latest = |p: &str| agents.list.iter().filter(|a| a.provider == p).max_by_key(|a| a.updated_at)?.context();
    ["claude", "codex"].into_iter().filter_map(|p| latest(p).map(|(used, window)| (p, 100 - percent(used, window), level(used, window, s.warn_at, s.critical_at)))).collect()
}

impl Desktop {
    pub(super) fn usage(&self) -> Vec<(&'static str, u64, Level)> {
        if !self.store.sidebar.usage_card {
            return Vec::new();
        }
        usage(&self.agents, &self.store.sidebar)
    }
}

#[cfg(test)]
mod tests {
    use super::usage;
    use agents::{Agents, Level, Summary};
    use store::prefs::Sidebar;

    fn session(agents: &mut Agents, id: &str, provider: &str, at: i64, used: u64) {
        agents.list.push(Summary { id: id.into(), provider: provider.into(), updated_at: at, tokens_used: used, context_window: Some(200_000), ..Default::default() });
    }

    #[test]
    fn usage_reads_the_context_left_in_each_providers_newest_session() {
        let mut agents = Agents::default();
        session(&mut agents, "codex", "codex", 1, 184_000);
        session(&mut agents, "old", "claude", 1, 20_000);
        session(&mut agents, "new", "claude", 2, 150_000);
        assert_eq!(usage(&agents, &Sidebar::default()), vec![("claude", 25, Level::Warn), ("codex", 8, Level::Danger)]);
    }

    #[test]
    fn usage_colours_follow_the_sidebar_thresholds() {
        let mut agents = Agents::default();
        session(&mut agents, "new", "claude", 2, 150_000);
        let strict = Sidebar { warn_at: 50, critical_at: 70, ..Sidebar::default() };
        assert_eq!(usage(&agents, &strict), vec![("claude", 25, Level::Danger)]);
    }

    #[test]
    fn usage_card_hides_without_a_window() {
        let mut agents = Agents::default();
        session(&mut agents, "old", "claude", 1, 20_000);
        agents.list.push(Summary { id: "new".into(), provider: "claude".into(), updated_at: 2, tokens_used: 20_000, ..Default::default() });
        assert_eq!(usage(&agents, &Sidebar::default()), vec![]);
    }
}
