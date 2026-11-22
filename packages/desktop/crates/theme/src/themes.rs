use crate::{KEYS, Look, Token};
use std::sync::OnceLock;

pub const GRAPHITE_LIGHT: &str = "graphite-light";
pub const GRAPHITE_DARK: &str = "graphite-dark";

/// One look of the app: a name, an appearance and the colours it sets.
#[derive(Clone, Debug, PartialEq)]
pub struct Theme {
    pub name: String,
    pub dark: bool,
    /// `0xRRGGBBff`; 0 for none.
    pub(crate) accent: u32,
    /// By position in `KEYS`; `None` keeps the token's own colour for `dark`.
    pub(crate) colors: [Option<u32>; KEYS.len()],
}

impl Theme {
    /// The default theme of an appearance.
    pub fn graphite(dark: bool) -> Self {
        Self { name: if dark { "Graphite Dark" } else { "Graphite Light" }.into(), dark, accent: 0, colors: [None; KEYS.len()] }
    }

    /// Reads a theme file: a name, an appearance, and under `colors` any of `KEYS`. Keys it leaves out keep Graphite's
    /// colour for its appearance and keys it adds are ignored. The error says what to fix.
    pub fn parse(json: &str) -> Result<Self, String> {
        let file: serde_json::Value = serde_json::from_str(json).map_err(|e| format!("Not a JSON file: {e}"))?;
        let name = file["name"].as_str().map(str::trim).filter(|n| !n.is_empty()).ok_or("\"name\" is missing")?;
        let dark = match file["appearance"].as_str() {
            Some("light") => false,
            Some("dark") => true,
            _ => return Err("\"appearance\" must be \"light\" or \"dark\"".into()),
        };
        let accent = match &file["accent"] {
            serde_json::Value::Null => 0,
            accent => color("accent", accent)? | 0xff,
        };
        let mut colors = [None; KEYS.len()];
        match &file["colors"] {
            serde_json::Value::Null => {}
            serde_json::Value::Object(set) => {
                for (slot, (key, _)) in colors.iter_mut().zip(KEYS) {
                    *slot = set.get(*key).map(|c| color(&format!("colors.{key}"), c)).transpose()?;
                }
            }
            _ => return Err("\"colors\" must be an object of colors".into()),
        }
        Ok(Self { name: name.into(), dark, accent, colors })
    }

    /// The colour this theme paints `token`, before any of the user's settings.
    pub fn color(&self, token: Token) -> u32 {
        token.paint(Look { dark: self.dark, accent: self.accent, ..Look::default() }, token.slot.and_then(|slot| self.colors[slot as usize]))
    }
}

/// `#RRGGBB` or `#RRGGBBAA` as `0xRRGGBBAA`.
fn color(key: &str, value: &serde_json::Value) -> Result<u32, String> {
    let hex = value.as_str().and_then(|c| c.strip_prefix('#')).filter(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit()));
    let rgba = match hex {
        Some(hex) if hex.len() == 6 => u32::from_str_radix(hex, 16).ok().map(|c| (c << 8) | 0xff),
        Some(hex) if hex.len() == 8 => u32::from_str_radix(hex, 16).ok(),
        _ => None,
    };
    rgba.ok_or_else(|| format!("\"{key}\" is {value}, not a color like \"#1a2b3c\" or \"#1a2b3c80\""))
}

/// A theme the app ships, under the id Settings saves.
pub struct Preset {
    pub id: &'static str,
    pub theme: Theme,
}

/// The built-in themes in the order Settings lists them; their files sit in `packages/desktop/themes`.
pub fn presets() -> &'static [Preset] {
    static PRESETS: OnceLock<Vec<Preset>> = OnceLock::new();
    PRESETS.get_or_init(|| {
        let file = |id, json| Preset { id, theme: Theme::parse(json).expect("a built-in theme parses") };
        vec![
            Preset { id: GRAPHITE_LIGHT, theme: Theme::graphite(false) },
            file("github-light", include_str!("../../../themes/github-light.json")),
            file("solarized-light", include_str!("../../../themes/solarized-light.json")),
            Preset { id: GRAPHITE_DARK, theme: Theme::graphite(true) },
            file("one-dark", include_str!("../../../themes/one-dark.json")),
            file("rose-pine", include_str!("../../../themes/rose-pine.json")),
            file("tokyo-night", include_str!("../../../themes/tokyo-night.json")),
            file("dracula", include_str!("../../../themes/dracula.json")),
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::{Theme, presets};
    use crate::contrast::{over, ratio};
    use crate::{
        ACCENT, ACCENT_LINK, CANVAS, DIFF_ADD_TEXT, DIFF_DEL_TEXT, FAILED, FAILED_TEXT, GOAL_TEXT, INDIGO, KEYS, MODIFIED, ON_PRIMARY, ON_TEXT, PAGE, POPOVER,
        PRIMARY, SELECTION, SIDE, SUCCESS, SUCCESS_TEXT, SURFACE, SURFACE_SUNKEN, TEXT, TEXT_2, TEXT_3, TEXT_BODY, Token, WAITING, WAITING_TEXT, WHITE, WINDOW,
        WINDOW_SOLID, WORKING,
    };

    const GRAPHITE_FILES: [(bool, &str); 2] =
        [(false, include_str!("../../../themes/graphite-light.json")), (true, include_str!("../../../themes/graphite-dark.json"))];

    fn theme(colors: &str) -> Result<Theme, String> {
        Theme::parse(&format!(r#"{{ "name": "Mine", "appearance": "dark", {colors} }}"#))
    }

    #[test]
    fn a_theme_paints_the_keys_it_sets_and_graphite_for_the_rest() {
        let t = theme(r##""colors": { "text": "#102030", "window": "#11223344", "not.a.key": "#000000" }"##).unwrap();
        assert_eq!((t.name.as_str(), t.dark), ("Mine", true));
        assert_eq!([t.color(TEXT), t.color(WINDOW)], [0x102030ff, 0x11223344]);
        assert_eq!(t.color(TEXT_2), TEXT_2.pick(true));
        let light = Theme::parse(r#"{ "name": "Day", "appearance": "light" }"#).unwrap();
        assert_eq!(light.color(TEXT), TEXT.pick(false));
    }

    #[test]
    fn a_theme_s_accent_paints_the_accent_roles_whatever_their_keys_say() {
        let t = theme(r##""accent": "#3e8ef7", "colors": { "selection": "#ffffff", "text": "#102030" }"##).unwrap();
        assert_eq!([t.color(PRIMARY), t.color(SELECTION), t.color(TEXT)], [0x3e8ef7ff, 0x3e8ef740, 0x102030ff]);
        let plain = theme(r##""colors": { "selection": "#ffffff", "text": "#102030" }"##).unwrap();
        assert_eq!([plain.color(PRIMARY), plain.color(SELECTION)], [0x102030ff, 0xffffffff]);
    }

    #[test]
    fn a_broken_file_is_refused_with_what_to_fix() {
        let said = |json: &str| Theme::parse(json).unwrap_err();
        assert!(said("{ nope").starts_with("Not a JSON file: "));
        assert_eq!(said(r#"{ "appearance": "dark" }"#), "\"name\" is missing");
        assert_eq!(said(r#"{ "name": " ", "appearance": "dark" }"#), "\"name\" is missing");
        assert_eq!(said(r#"{ "name": "Mine" }"#), "\"appearance\" must be \"light\" or \"dark\"");
        assert_eq!(said(r#"{ "name": "Mine", "appearance": "night" }"#), "\"appearance\" must be \"light\" or \"dark\"");
        assert_eq!(theme(r#""colors": []"#).unwrap_err(), "\"colors\" must be an object of colors");
        for bad in ["\"#zzz\"", "\"#12345\"", "\"#1234567\"", "\"1a2b3c\"", "\"#+a2b3c\"", "12"] {
            assert_eq!(
                theme(&format!(r#""colors": {{ "text": {bad} }}"#)).unwrap_err(),
                format!("\"colors.text\" is {bad}, not a color like \"#1a2b3c\" or \"#1a2b3c80\"")
            );
        }
        assert_eq!(theme(r#""accent": "blue""#).unwrap_err(), "\"accent\" is \"blue\", not a color like \"#1a2b3c\" or \"#1a2b3c80\"");
    }

    #[test]
    fn the_graphite_files_list_every_key_with_the_colour_the_app_paints() {
        for (dark, json) in GRAPHITE_FILES {
            let file: serde_json::Value = serde_json::from_str(json).unwrap();
            let listed: Vec<&str> = file["colors"].as_object().unwrap().keys().map(String::as_str).collect();
            assert_eq!(listed, KEYS.iter().map(|(key, _)| *key).collect::<Vec<_>>(), "dark: {dark}");
            let (parsed, graphite) = (Theme::parse(json).unwrap(), Theme::graphite(dark));
            assert_eq!((parsed.name.as_str(), parsed.dark), (graphite.name.as_str(), dark));
            for (key, token) in KEYS {
                assert_eq!(parsed.color(*token), graphite.color(*token), "{key} dark: {dark}");
            }
        }
    }

    #[test]
    fn graphite_paints_what_the_tokens_hold() {
        for dark in [false, true] {
            let graphite = Theme::graphite(dark);
            for (key, token) in KEYS {
                assert_eq!(graphite.color(*token), token.pick(dark), "{key}");
            }
        }
    }

    #[test]
    fn the_presets_are_three_light_and_five_dark_each_under_its_own_id() {
        let ids: std::collections::HashSet<&str> = presets().iter().map(|p| p.id).collect();
        assert_eq!((ids.len(), presets().len()), (8, 8));
        assert_eq!(presets().iter().filter(|p| !p.theme.dark).count(), 3);
        assert_eq!(
            presets().iter().map(|p| p.theme.name.as_str()).collect::<Vec<_>>(),
            ["Graphite Light", "GitHub Light", "Solarized Light", "Graphite Dark", "One Dark", "Rosé Pine", "Tokyo Night", "Dracula"]
        );
    }

    const TEXT_ROLES: [(&str, Token); 12] = [
        ("text", TEXT),
        ("text.body", TEXT_BODY),
        ("text.2", TEXT_2),
        ("waiting.text", WAITING_TEXT),
        ("success.text", SUCCESS_TEXT),
        ("failed.text", FAILED_TEXT),
        ("goal.text", GOAL_TEXT),
        ("accent.text", ACCENT),
        ("link", ACCENT_LINK),
        ("diff.added.text", DIFF_ADD_TEXT),
        ("diff.removed.text", DIFF_DEL_TEXT),
        ("modified", MODIFIED),
    ];
    const GLYPHS: [(&str, Token); 7] =
        [("waiting", WAITING), ("success", SUCCESS), ("failed", FAILED), ("accent.text", ACCENT), ("working", WORKING), ("text.3", TEXT_3), ("action", INDIGO)];

    /// The pairs of a theme that fall short of the contrast Graphite is held to.
    fn low(t: &Theme) -> Vec<String> {
        let window = t.color(WINDOW_SOLID);
        let surfaces =
            [("window", WINDOW_SOLID), ("surface", SURFACE), ("surface.sunken", SURFACE_SUNKEN), ("page", PAGE), ("popover", POPOVER), ("side", SIDE)]
                .map(|(name, s)| (name, over(t.color(s), window)));
        let below = |roles: &[(&str, Token)], least: f32| -> Vec<String> {
            surfaces
                .iter()
                .flat_map(|(on, bg)| roles.iter().filter(move |(_, role)| ratio(t.color(*role), *bg) < least).map(move |(role, _)| format!("{role} on {on}")))
                .collect()
        };
        let mut low = [below(&TEXT_ROLES, 4.5), below(&GLYPHS, 3.)].concat();
        low.extend(
            [("failed", FAILED), ("waiting", WAITING)]
                .into_iter()
                .filter(|(_, fill)| ratio(WHITE.pick(t.dark), t.color(*fill)) < 4.5)
                .map(|(fill, _)| format!("white on {fill}")),
        );
        if ratio(t.color(ON_TEXT), t.color(ACCENT)) < 4.5 {
            low.push("text.on on accent.text".into());
        }
        if ratio(t.color(ON_PRIMARY), t.color(PRIMARY)) < 4.5 {
            low.push("the label on a primary button".into());
        }
        low
    }

    #[test]
    fn every_preset_clears_the_contrast_graphite_is_held_to() {
        for preset in presets() {
            let exempt: &[&str] = if preset.theme.dark { &["white on failed", "white on waiting"] } else { &[] };
            let low: Vec<String> = low(&preset.theme).into_iter().filter(|pair| !exempt.contains(&pair.as_str())).collect();
            assert_eq!((preset.id, low), (preset.id, Vec::new()));
        }
    }

    #[test]
    fn dark_presets_leave_the_panes_clear_for_the_window_s_glass() {
        for preset in presets().iter().filter(|p| p.theme.dark) {
            for (key, token) in [("page", PAGE), ("canvas", CANVAS), ("side", SIDE), ("surface.sunken", SURFACE_SUNKEN)] {
                assert_eq!(preset.theme.color(token) & 0xff, 0, "{} {key}", preset.id);
            }
            assert!(preset.theme.color(SURFACE) & 0xff < 0x40, "{} surface", preset.id);
        }
    }
}
