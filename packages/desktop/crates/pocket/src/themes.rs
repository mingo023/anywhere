use crate::desktop::Desktop;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use store::{Appearance, Mode};
use theme::{GRAPHITE_DARK, GRAPHITE_LIGHT, Theme};

/// The theme choice that follows macOS: the day theme while it's light, the night theme while it's dark.
pub(crate) const SYSTEM: &str = "system";
const CUSTOM: &str = "custom/";

/// An imported theme and the file in `themes/` it was read from.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Custom {
    pub slug: String,
    pub theme: Theme,
}

impl Custom {
    /// What Settings saves to choose it; the prefix keeps it apart from a built-in theme of the same name.
    pub fn id(&self) -> String {
        format!("{CUSTOM}{}", self.slug)
    }
}

#[derive(Default)]
pub(crate) struct Themes {
    /// The imported themes, in slug order.
    pub custom: Vec<Custom>,
    /// Why the last import or removal failed.
    pub error: Option<SharedString>,
    /// Counts changes to `custom`, so a reload that read the folder before one doesn't undo it.
    pub edits: usize,
}

impl Themes {
    pub fn new(custom: Vec<Custom>) -> Self {
        Self { custom, ..Self::default() }
    }

    /// Adds an imported theme in place of the one kept under its slug.
    fn keep(&mut self, custom: Custom) {
        self.custom.retain(|c| c.slug != custom.slug);
        let at = self.custom.partition_point(|c| c.slug < custom.slug);
        self.custom.insert(at, custom);
        self.edits += 1;
    }

    fn remove(&mut self, slug: &str) -> Option<Custom> {
        let at = self.custom.iter().position(|c| c.slug == slug)?;
        self.edits += 1;
        Some(self.custom.remove(at))
    }

    /// Takes the themes read from disk when `edits` was the count; false when it has moved on, or nothing changed.
    fn reload(&mut self, edits: usize, custom: Vec<Custom>) -> bool {
        let changed = self.edits == edits && self.custom != custom;
        if changed {
            self.custom = custom;
            self.edits += 1;
        }
        changed
    }
}

/// The themes kept in `home`, leaving out files that don't parse.
pub(crate) fn load(home: &Path) -> Vec<Custom> {
    store::themes::list(home).into_iter().filter_map(|(slug, text)| Some(Custom { slug, theme: Theme::parse(&text).ok()? })).collect()
}

/// Reads the theme file at `path` and keeps a copy in `home`; the error says what's wrong with it.
pub(crate) fn import(home: &Path, path: &Path) -> Result<Custom, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("Couldn't read the file: {e}"))?;
    let theme = Theme::parse(&text)?;
    let slug = store::themes::keep(home, &theme.name, &text).map_err(|e| format!("Couldn't save the theme: {e}"))?;
    Ok(Custom { slug, theme })
}

/// The theme saved as `id`: a built-in one, or an imported one.
pub(crate) fn find<'a>(custom: &'a [Custom], id: &str) -> Option<&'a Theme> {
    match id.strip_prefix(CUSTOM) {
        Some(slug) => custom.iter().find(|c| c.slug == slug).map(|c| &c.theme),
        None => theme::presets().iter().find(|p| p.id == id).map(|p| &p.theme),
    }
}

/// The name of the theme saved as `id`; one whose file is gone goes by its file's name.
pub(crate) fn name(custom: &[Custom], id: &str) -> String {
    match find(custom, id) {
        Some(theme) => theme.name.clone(),
        None if id == SYSTEM => "System".into(),
        None => id.strip_prefix(CUSTOM).unwrap_or(id).into(),
    }
}

/// The theme choice: the saved one, else what the old `mode` picked (System, or Graphite light or dark).
pub(crate) fn chosen(a: &Appearance) -> &str {
    a.theme.as_deref().unwrap_or(match a.mode {
        Mode::System => SYSTEM,
        Mode::Light => GRAPHITE_LIGHT,
        Mode::Dark => GRAPHITE_DARK,
    })
}

/// The theme System uses while macOS is `dark`: the saved one while it exists and is of that appearance, else Graphite.
pub(crate) fn system_theme<'a>(a: &'a Appearance, custom: &[Custom], dark: bool) -> &'a str {
    let (saved, graphite) = if dark { (&a.night, GRAPHITE_DARK) } else { (&a.day, GRAPHITE_LIGHT) };
    saved.as_deref().filter(|id| find(custom, id).is_some_and(|t| t.dark == dark)).unwrap_or(graphite)
}

/// Whether the chosen theme makes the window dark; `None` follows macOS.
pub(crate) fn forced(a: &Appearance, custom: &[Custom]) -> Option<bool> {
    find(custom, chosen(a)).map(|t| t.dark)
}

/// The theme to paint while macOS is `system_dark`. A chosen theme whose file is gone paints as Graphite.
pub(crate) fn active(a: &Appearance, custom: &[Custom], system_dark: bool) -> Theme {
    let id = match chosen(a) {
        SYSTEM => system_theme(a, custom, system_dark),
        id => id,
    };
    find(custom, id).cloned().unwrap_or_else(|| Theme::graphite(system_dark))
}

/// Whether the theme saved as `id` is one the window can be painting.
pub(crate) fn in_use(a: &Appearance, custom: &[Custom], id: &str) -> bool {
    match chosen(a) {
        SYSTEM => [false, true].iter().any(|dark| system_theme(a, custom, *dark) == id),
        chosen => chosen == id,
    }
}

/// Puts a just imported theme to use: as the day or night theme while the choice follows macOS, else as the choice.
fn choose(a: &mut Appearance, id: String, dark: bool) {
    match chosen(a) {
        SYSTEM if dark => a.night = Some(id),
        SYSTEM => a.day = Some(id),
        _ => a.theme = Some(id),
    }
}

/// Stops choosing the removed theme `id`: whatever held it goes back to Graphite of its appearance.
fn forget(a: &mut Appearance, id: &str, dark: bool) {
    if a.theme.as_deref() == Some(id) {
        a.theme = Some(if dark { GRAPHITE_DARK } else { GRAPHITE_LIGHT }.into());
    }
    for slot in [&mut a.day, &mut a.night] {
        if slot.as_deref() == Some(id) {
            *slot = None;
        }
    }
}

impl Desktop {
    /// Paints the chosen theme and forces its appearance on the window.
    pub(crate) fn apply_theme(&mut self, cx: &mut Context<Self>) {
        let forced = forced(&self.store.appearance, &self.themes.custom);
        cx.set_window_appearance(forced.map(|dark| if dark { WindowAppearance::Dark } else { WindowAppearance::Light }));
        // AppKit tells the window when its appearance changes, but not when a theme of the same appearance replaces another.
        self.paint_theme(forced.unwrap_or(theme::is_dark()), cx);
    }

    /// Paints the theme chosen for when macOS is `system_dark`. Diff and commit rows hold the colours they were highlighted with.
    pub(crate) fn paint_theme(&mut self, system_dark: bool, cx: &mut Context<Self>) {
        theme::set_theme(&active(&self.store.appearance, &self.themes.custom, system_dark), cx);
        self.recolor_diff(cx);
        self.recolor_commit(cx);
        cx.notify();
    }

    pub(crate) fn pick_theme_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_path(false, window, cx, |d, paths, _, cx| {
            if let Some(path) = paths.into_iter().next() {
                d.import_theme(path, cx);
            }
        });
    }

    pub(crate) fn import_theme(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        let home = self.store.home().to_path_buf();
        cx.spawn(async move |this, cx| {
            let imported = cx.background_spawn(async move { import(&home, &path) }).await;
            this.update(cx, |d, cx| match imported {
                Ok(custom) => {
                    d.themes.error = None;
                    choose(&mut d.store.appearance, custom.id(), custom.theme.dark);
                    d.themes.keep(custom);
                    d.save_soon(cx);
                    d.apply_theme(cx);
                }
                Err(why) => {
                    d.themes.error = Some(why.into());
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(crate) fn remove_theme(&mut self, slug: &str, cx: &mut Context<Self>) {
        let Some(custom) = self.themes.custom.iter().find(|c| c.slug == slug) else { return };
        let (home, slug, id, dark) = (self.store.home().to_path_buf(), slug.to_string(), custom.id(), custom.theme.dark);
        cx.spawn(async move |this, cx| {
            // The file goes first: a reload that reads the folder after the theme left the list must not find it there.
            let file = slug.clone();
            let deleted = cx.background_spawn(async move { store::themes::remove(&home, &file) }).await;
            this.update(cx, |d, cx| {
                if let Some(why) = deleted.err().filter(|e| e.kind() != std::io::ErrorKind::NotFound) {
                    d.themes.error = Some(format!("Couldn't remove the theme: {why}").into());
                    return cx.notify();
                }
                d.themes.error = None;
                d.themes.remove(&slug);
                forget(&mut d.store.appearance, &id, dark);
                d.save_soon(cx);
                d.apply_theme(cx);
            })
            .ok();
        })
        .detach();
    }

    /// Reads the imported themes again, so a file edited while the app was in the background shows its changes.
    pub(crate) fn reload_themes(&mut self, cx: &mut Context<Self>) {
        let (home, edits) = (self.store.home().to_path_buf(), self.themes.edits);
        cx.spawn(async move |this, cx| {
            let custom = cx.background_spawn(async move { load(&home) }).await;
            this.update(cx, |d, cx| {
                if d.themes.reload(edits, custom) {
                    d.apply_theme(cx);
                }
            })
            .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{Custom, SYSTEM, Themes, active, choose, chosen, find, forced, forget, import, in_use, load, name, system_theme};
    use store::{Appearance, Mode};
    use theme::{GRAPHITE_DARK, GRAPHITE_LIGHT, TEXT, Theme};

    fn custom(name: &str, dark: bool) -> Custom {
        let appearance = if dark { "dark" } else { "light" };
        Custom { slug: store::themes::slug(name), theme: Theme::parse(&format!(r##"{{ "name": "{name}", "appearance": "{appearance}", "colors": {{ "text": "#102030" }} }}"##)).unwrap() }
    }

    fn appearance(theme: &str) -> Appearance {
        Appearance { theme: Some(theme.into()), ..Appearance::default() }
    }

    fn home(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("pocket-themes-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn settings_saved_before_themes_keep_their_look() {
        let saved = |mode| Appearance { mode, ..Appearance::default() };
        assert_eq!([Mode::System, Mode::Light, Mode::Dark].map(|m| chosen(&saved(m)).to_string()), [SYSTEM, GRAPHITE_LIGHT, GRAPHITE_DARK]);
        assert_eq!([forced(&saved(Mode::System), &[]), forced(&saved(Mode::Light), &[]), forced(&saved(Mode::Dark), &[])], [None, Some(false), Some(true)]);
        for dark in [false, true] {
            assert_eq!(active(&saved(Mode::System), &[], dark), Theme::graphite(dark));
        }
        assert_eq!(active(&saved(Mode::Dark), &[], false), Theme::graphite(true));
    }

    #[test]
    fn a_chosen_theme_wins_over_the_old_mode_and_forces_its_appearance() {
        let a = Appearance { mode: Mode::Light, ..appearance("dracula") };
        assert_eq!((chosen(&a), forced(&a, &[])), ("dracula", Some(true)));
        assert_eq!(active(&a, &[], false).name, "Dracula");
    }

    #[test]
    fn system_paints_the_day_theme_while_macos_is_light_and_the_night_theme_while_dark() {
        let a = Appearance { day: Some("solarized-light".into()), night: Some("tokyo-night".into()), ..appearance(SYSTEM) };
        assert_eq!(forced(&a, &[]), None);
        assert_eq!([active(&a, &[], false).name, active(&a, &[], true).name], ["Solarized Light", "Tokyo Night"]);
    }

    #[test]
    fn a_day_or_night_theme_of_the_wrong_appearance_or_gone_is_graphite() {
        let a = Appearance { day: Some("dracula".into()), night: Some("custom/gone".into()), ..appearance(SYSTEM) };
        assert_eq!([system_theme(&a, &[], false), system_theme(&a, &[], true)], [GRAPHITE_LIGHT, GRAPHITE_DARK]);
    }

    #[test]
    fn an_imported_theme_is_found_apart_from_a_built_in_one_of_the_same_name() {
        let mine = [custom("Dracula", false)];
        assert_eq!(mine[0].id(), "custom/dracula");
        assert_eq!([find(&mine, "custom/dracula").unwrap().dark, find(&mine, "dracula").unwrap().dark], [false, true]);
        assert_eq!(find(&mine, "custom/other"), None);
        assert_eq!(active(&appearance("custom/dracula"), &mine, true).color(TEXT), 0x102030ff);
    }

    #[test]
    fn a_chosen_theme_whose_file_is_gone_follows_macos_as_graphite() {
        let a = appearance("custom/gone");
        assert_eq!(forced(&a, &[]), None);
        assert_eq!(active(&a, &[], true), Theme::graphite(true));
    }

    #[test]
    fn an_import_becomes_the_choice_or_under_system_the_day_or_night_theme() {
        let mut a = appearance("one-dark");
        choose(&mut a, "custom/mine".into(), false);
        assert_eq!(a.theme.as_deref(), Some("custom/mine"));
        let mut a = appearance(SYSTEM);
        choose(&mut a, "custom/sun".into(), false);
        choose(&mut a, "custom/moon".into(), true);
        assert_eq!((a.theme.as_deref(), a.day.as_deref(), a.night.as_deref()), (Some(SYSTEM), Some("custom/sun"), Some("custom/moon")));
    }

    #[test]
    fn removing_the_theme_in_use_falls_back_to_graphite_of_its_appearance() {
        let mut a = appearance("custom/mine");
        forget(&mut a, "custom/mine", true);
        assert_eq!(a.theme.as_deref(), Some(GRAPHITE_DARK));
        let mut a = Appearance { day: Some("custom/sun".into()), night: Some("custom/moon".into()), ..appearance(SYSTEM) };
        forget(&mut a, "custom/moon", true);
        assert_eq!((a.theme.as_deref(), a.day.as_deref(), a.night.as_deref()), (Some(SYSTEM), Some("custom/sun"), None));
        let mut a = appearance("dracula");
        forget(&mut a, "custom/mine", true);
        assert_eq!(a.theme.as_deref(), Some("dracula"));
    }

    #[test]
    fn a_theme_is_in_use_when_chosen_or_under_system_when_day_or_night() {
        let mine = [custom("Moon", true)];
        assert!(in_use(&appearance("custom/moon"), &mine, "custom/moon") && !in_use(&appearance("dracula"), &mine, "custom/moon"));
        let a = Appearance { night: Some("custom/moon".into()), ..appearance(SYSTEM) };
        assert!(in_use(&a, &mine, "custom/moon") && in_use(&a, &mine, GRAPHITE_LIGHT) && !in_use(&a, &mine, GRAPHITE_DARK));
    }

    #[test]
    fn importing_keeps_a_copy_that_loads_again_and_a_same_named_import_replaces_it() {
        let home = home("import");
        let file = home.join("picked.json");
        std::fs::write(&file, r##"{ "name": "My Theme", "appearance": "dark", "colors": { "text": "#102030" } }"##).unwrap();
        let first = import(&home, &file).unwrap();
        assert_eq!((first.slug.as_str(), first.theme.color(TEXT)), ("my-theme", 0x102030ff));
        std::fs::write(&file, r##"{ "name": "My Theme", "appearance": "light", "colors": { "text": "#405060" } }"##).unwrap();
        import(&home, &file).unwrap();
        let kept = load(&home);
        assert_eq!((kept.len(), kept[0].theme.dark, kept[0].theme.color(TEXT)), (1, false, 0x405060ff));
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_broken_file_is_refused_and_nothing_is_kept() {
        let home = home("broken");
        let file = home.join("picked.json");
        std::fs::write(&file, r##"{ "name": "Mine", "appearance": "dark", "colors": { "text": "#zzz" } }"##).unwrap();
        assert_eq!(import(&home, &file).unwrap_err(), "\"colors.text\" is \"#zzz\", not a color like \"#1a2b3c\" or \"#1a2b3c80\"");
        assert!(import(&home, &home.join("missing.json")).unwrap_err().starts_with("Couldn't read the file: "));
        assert_eq!(load(&home), []);
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_kept_file_that_no_longer_parses_is_left_out() {
        let home = home("stale");
        store::themes::keep(&home, "Fine", r#"{ "name": "Fine", "appearance": "dark" }"#).unwrap();
        store::themes::keep(&home, "Torn", "{ nope").unwrap();
        assert_eq!(load(&home).iter().map(|c| c.slug.as_str()).collect::<Vec<_>>(), ["fine"]);
        std::fs::remove_dir_all(home).unwrap();
    }

    #[test]
    fn a_reload_takes_what_changed_on_disk_unless_an_import_or_removal_came_first() {
        let mut themes = Themes::new(vec![custom("Mine", true)]);
        assert!(!themes.reload(themes.edits, vec![custom("Mine", true)]));
        assert!(themes.reload(themes.edits, vec![custom("Mine", false)]));
        assert!(!themes.custom[0].theme.dark);
        let before = themes.edits;
        themes.keep(custom("Other", true));
        assert!(!themes.reload(before, Vec::new()));
        assert_eq!(themes.custom.len(), 2);
    }

    #[test]
    fn a_theme_goes_by_its_name_and_one_whose_file_is_gone_by_its_file_s() {
        let mine = [custom("My Theme", true)];
        assert_eq!([name(&mine, "custom/my-theme"), name(&mine, "dracula"), name(&mine, SYSTEM), name(&[], "custom/my-theme"), name(&[], "nothing")], ["My Theme", "Dracula", "System", "my-theme", "nothing"]);
    }

    #[test]
    fn the_imported_list_stays_in_slug_order_with_one_theme_per_slug() {
        let mut themes = Themes::new(vec![custom("Beta", true)]);
        themes.keep(custom("Alpha", true));
        themes.keep(custom("Beta", false));
        assert_eq!(themes.custom.iter().map(|c| (c.slug.as_str(), c.theme.dark)).collect::<Vec<_>>(), [("alpha", true), ("beta", false)]);
        assert_eq!(themes.remove("alpha").map(|c| c.slug), Some("alpha".to_string()));
        assert_eq!((themes.remove("alpha"), themes.custom.len()), (None, 1));
    }
}
