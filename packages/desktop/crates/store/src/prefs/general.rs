use crate::Layout;
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// Software update, startup, quitting and background work.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct General {
    pub auto_install: bool,
    pub whats_new: bool,
    pub check_hours: u32,
    pub open_at_login: bool,
    pub restore_layouts: bool,
    /// `None` opens the layout last used.
    #[serde(deserialize_with = "crate::lenient")]
    pub launch_layout: Option<Layout>,
    pub panel_open: bool,
    #[serde(deserialize_with = "crate::lenient")]
    pub confirm_quit: ConfirmQuit,
    pub battery_saver: bool,
    pub git_secs: u32,
    pub pr_secs: u32,
    /// pocketd's own settings, as it last reported them.
    pub resume_agents: bool,
    pub keep_awake: bool,
    /// 0 sleeps as soon as agents stop.
    pub linger_minutes: u32,
}

impl Default for General {
    fn default() -> Self {
        Self {
            auto_install: true,
            whats_new: true,
            check_hours: 6,
            open_at_login: false,
            restore_layouts: true,
            launch_layout: None,
            panel_open: false,
            confirm_quit: ConfirmQuit::Always,
            battery_saver: false,
            git_secs: 2,
            pr_secs: 30,
            resume_agents: true,
            keep_awake: true,
            linger_minutes: 2,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ConfirmQuit {
    Never,
    #[default]
    Always,
    Working,
}

impl ConfirmQuit {
    pub fn asks(self, working: usize) -> bool {
        match self {
            Self::Never => false,
            Self::Always => true,
            Self::Working => working > 0,
        }
    }
}

const BATTERY_GIT_SECS: u64 = 10;
const BATTERY_PR_SECS: u64 = 5 * 60;

impl General {
    /// Seconds between git refreshes.
    pub fn git_every(&self) -> u64 {
        if self.battery_saver { BATTERY_GIT_SECS } else { u64::from(self.git_secs.max(1)) }
    }

    /// How long the pull request on screen stays fresh.
    pub fn prs_every(&self) -> Duration {
        Duration::from_secs(if self.battery_saver { BATTERY_PR_SECS } else { u64::from(self.pr_secs.max(1)) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn battery_saver_stretches_both_refreshes_whatever_was_chosen() {
        let fast = General { git_secs: 3, pr_secs: 15, ..General::default() };
        assert_eq!((fast.git_every(), fast.prs_every()), (3, Duration::from_secs(15)));
        let saver = General { battery_saver: true, ..fast };
        assert_eq!((saver.git_every(), saver.prs_every()), (10, Duration::from_secs(300)));
    }

    #[test]
    fn the_defaults_refresh_as_the_app_always_has() {
        let g = General::default();
        assert_eq!((g.git_every(), g.prs_every(), g.check_hours), (2, Duration::from_secs(30), 6));
    }

    #[test]
    fn quitting_asks_while_agents_work_only_when_one_does() {
        assert_eq!([0, 2].map(|n| ConfirmQuit::Working.asks(n)), [false, true]);
        assert_eq!([0, 2].map(|n| ConfirmQuit::Always.asks(n)), [true, true]);
        assert_eq!([0, 2].map(|n| ConfirmQuit::Never.asks(n)), [false, false]);
    }

    #[test]
    fn an_unknown_launch_layout_falls_back_to_the_last_used_one() {
        let g: General = serde_json::from_str(r#"{"launch_layout":"zen","confirm_quit":"maybe","battery_saver":true}"#).unwrap();
        assert_eq!((g.launch_layout, g.confirm_quit, g.battery_saver), (None, ConfirmQuit::Always, true));
    }
}
