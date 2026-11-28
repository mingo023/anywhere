use serde::{Deserialize, Serialize};
use std::collections::HashMap;

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

/// Whose light or dark a space paints in: the app's, or its own.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum LookMode {
    #[default]
    Auto,
    Light,
    Dark,
}

/// A colour on the theme editor's pad: a point in the unit circle, whose angle is its hue and distance its lightness, at saturation `s`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub struct LookColour {
    pub x: f32,
    pub y: f32,
    pub s: f32,
}

/// A space's theme: up to three colours washed over the window, how strongly, and how much grain lies on them.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SpaceLook {
    #[serde(deserialize_with = "crate::lenient")]
    pub mode: LookMode,
    #[serde(deserialize_with = "crate::lenient")]
    pub colours: Vec<LookColour>,
    /// 0 to 1.
    pub intensity: f32,
    /// 0 to 1.
    pub grain: f32,
}

impl Default for SpaceLook {
    fn default() -> Self {
        Self { mode: LookMode::Auto, colours: Vec::new(), intensity: 0.45, grain: 0. }
    }
}

impl SpaceLook {
    pub const MAX_COLOURS: usize = 3;
}

/// Sidebar and inbox.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct Sidebar {
    #[serde(deserialize_with = "crate::lenient")]
    pub sort: SessionSort,
    pub details: Details,
    pub usage_card: bool,
    /// Percent of the context window used at which it turns amber, then red.
    pub warn_at: u32,
    pub critical_at: u32,
    pub jump_hints: bool,
    pub hint_delay_ms: u32,
    #[serde(deserialize_with = "crate::lenient")]
    pub inbox_sort: InboxSort,
    /// The host whose space the sidebar shows; `None` is This Mac.
    #[serde(deserialize_with = "crate::lenient")]
    pub space: Option<String>,
    /// Each host's chosen colour (RGB), by host id.
    #[serde(deserialize_with = "crate::lenient")]
    pub space_colours: HashMap<String, u32>,
    /// Each host's name as the user renamed it, by host id; shown on this Mac only.
    #[serde(deserialize_with = "crate::lenient")]
    pub space_names: HashMap<String, String>,
    /// Each host's chosen icon, by host id.
    #[serde(deserialize_with = "crate::lenient")]
    pub space_icons: HashMap<String, String>,
    /// Each space's theme, by host id; This Mac's under "".
    #[serde(deserialize_with = "crate::lenient")]
    pub space_looks: HashMap<String, SpaceLook>,
}

impl Default for Sidebar {
    fn default() -> Self {
        Self {
            sort: SessionSort::default(),
            details: Details::default(),
            usage_card: true,
            warn_at: 75,
            critical_at: 90,
            jump_hints: true,
            hint_delay_ms: 280,
            inbox_sort: InboxSort::default(),
            space: None,
            space_colours: HashMap::new(),
            space_names: HashMap::new(),
            space_icons: HashMap::new(),
            space_looks: HashMap::new(),
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
    fn a_space_look_keeps_what_it_can_read_and_defaults_the_rest() {
        let s: Sidebar = serde_json::from_str(
            r#"{"space_looks":{"devbox":{"mode":"dark","colours":[{"x":0.5,"y":-0.2,"s":0.8}],"grain":0.3},"":{"mode":"sepia","colours":"red"}}}"#,
        )
        .unwrap();
        let devbox = &s.space_looks["devbox"];
        assert_eq!((devbox.mode, devbox.colours.len(), devbox.intensity, devbox.grain), (LookMode::Dark, 1, 0.45, 0.3));
        assert_eq!(s.space_looks[""], SpaceLook::default());
    }

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
        assert_eq!((s.sort, s.usage_card), (SessionSort::Newest, false));
    }

    #[test]
    fn a_sidebar_from_before_these_settings_keeps_todays_list() {
        let s: Sidebar = serde_json::from_str("{}").unwrap();
        assert_eq!(s, Sidebar::default());
        assert_eq!((s.sort, s.details, s.warn_at, s.critical_at, s.hint_delay_ms), (SessionSort::Newest, Details::default(), 75, 90, 280));
    }

    #[test]
    fn the_space_and_its_colours_are_kept_and_a_bad_value_falls_back_alone() {
        let s: Sidebar = serde_json::from_str(r#"{"space":"devbox","space_colours":{"devbox":4294967}}"#).unwrap();
        assert_eq!((s.space.as_deref(), s.space_colours.get("devbox")), (Some("devbox"), Some(&4294967)));
        let bad: Sidebar = serde_json::from_str(r#"{"space":7,"space_colours":{"devbox":"red"},"usage_card":false}"#).unwrap();
        assert_eq!((bad.space, bad.space_colours, bad.usage_card), (None, HashMap::new(), false));
    }

    #[test]
    fn a_hosts_name_and_icon_are_kept_and_a_bad_value_falls_back_alone() {
        let s: Sidebar = serde_json::from_str(r#"{"space_names":{"devbox":"GPU box"},"space_icons":{"devbox":"cloud"}}"#).unwrap();
        assert_eq!((s.space_names.get("devbox").map(String::as_str), s.space_icons.get("devbox").map(String::as_str)), (Some("GPU box"), Some("cloud")));
        let bad: Sidebar = serde_json::from_str(r#"{"space_names":["GPU box"],"space_icons":{"devbox":7},"usage_card":false}"#).unwrap();
        assert_eq!((bad.space_names, bad.space_icons, bad.usage_card), (HashMap::new(), HashMap::new(), false));
    }
}
