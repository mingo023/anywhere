use super::Section;
use crate::desktop::Desktop;
use gpui_kit::{Context, Div};
use store::Store;

/// Draws a custom row.
pub(crate) type Draw = fn(&mut Desktop, &'static Setting, &mut Context<Desktop>) -> Div;

/// Carries a row's stored value out to where it acts.
pub(crate) type Apply = fn(&mut Desktop, &mut Context<Desktop>);

/// A custom row's value as the reset sheet names it, and how to put it back from the defaults.
pub(crate) type Resets = (fn(&Store) -> String, fn(&mut Store, &Store));

/// What a row's control reads and writes in the store.
#[derive(Clone, Copy)]
pub(crate) enum Control {
    Switch { get: fn(&Store) -> bool, set: fn(&mut Store, bool) },
    Choice { options: &'static [&'static str], look: Look, get: fn(&Store) -> usize, set: fn(&mut Store, usize) },
    /// `unit` follows the number as written, so it carries its own space: " pt", "%".
    Stepper { min: i32, max: i32, step: i32, unit: &'static str, get: fn(&Store) -> i32, set: fn(&mut Store, i32) },
    Text { placeholder: &'static str, get: fn(&Store) -> String, set: fn(&mut Store, String) },
    /// A folder; setting it to "" puts the default back.
    Path { get: fn(&Store) -> String, set: fn(&mut Store, String) },
    /// Drawn by its section; Reset leaves it alone unless it `resets`.
    Custom(Draw),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Look {
    Segmented,
    Dropdown,
    Thumbnails,
}

/// One setting, defined once for its page, search, Advanced and Reset.
pub(crate) struct Setting {
    pub id: &'static str,
    pub group: &'static str,
    pub label: &'static str,
    pub hint: Option<&'static str>,
    /// A hint that depends on the store; it replaces `hint` when there is one.
    pub note: Option<fn(&Store) -> Option<String>>,
    pub advanced: bool,
    /// Shown only while this holds; drawn as a ↳ row.
    pub parent: Option<fn(&Store) -> bool>,
    /// Which of a dropdown's options its menu lists; all of them when unset.
    pub offers: Option<fn(&Store, usize) -> bool>,
    pub control: Control,
    pub effect: Option<Apply>,
    pub resets: Option<Resets>,
}

impl Setting {
    const fn new(id: &'static str, group: &'static str, label: &'static str, control: Control) -> Self {
        Self { id, group, label, hint: None, note: None, advanced: false, parent: None, offers: None, control, effect: None, resets: None }
    }

    pub const fn switch(id: &'static str, group: &'static str, label: &'static str, get: fn(&Store) -> bool, set: fn(&mut Store, bool)) -> Self {
        Self::new(id, group, label, Control::Switch { get, set })
    }

    pub const fn choice(id: &'static str, group: &'static str, label: &'static str, options: &'static [&'static str], look: Look, get: fn(&Store) -> usize, set: fn(&mut Store, usize)) -> Self {
        Self::new(id, group, label, Control::Choice { options, look, get, set })
    }

    pub const fn stepper(id: &'static str, group: &'static str, label: &'static str, (min, max, step): (i32, i32, i32), unit: &'static str, get: fn(&Store) -> i32, set: fn(&mut Store, i32)) -> Self {
        Self::new(id, group, label, Control::Stepper { min, max, step, unit, get, set })
    }

    pub const fn text(id: &'static str, group: &'static str, label: &'static str, placeholder: &'static str, get: fn(&Store) -> String, set: fn(&mut Store, String)) -> Self {
        Self::new(id, group, label, Control::Text { placeholder, get, set })
    }

    pub const fn path(id: &'static str, group: &'static str, label: &'static str, get: fn(&Store) -> String, set: fn(&mut Store, String)) -> Self {
        Self::new(id, group, label, Control::Path { get, set })
    }

    pub const fn custom(id: &'static str, group: &'static str, label: &'static str, draw: Draw) -> Self {
        Self::new(id, group, label, Control::Custom(draw))
    }

    pub const fn hint(mut self, hint: &'static str) -> Self {
        self.hint = Some(hint);
        self
    }

    pub const fn note(mut self, note: fn(&Store) -> Option<String>) -> Self {
        self.note = Some(note);
        self
    }

    pub const fn advanced(mut self) -> Self {
        self.advanced = true;
        self
    }

    pub const fn under(mut self, parent: fn(&Store) -> bool) -> Self {
        self.parent = Some(parent);
        self
    }

    pub const fn offers(mut self, offers: fn(&Store, usize) -> bool) -> Self {
        self.offers = Some(offers);
        self
    }

    pub const fn effect(mut self, effect: Apply) -> Self {
        self.effect = Some(effect);
        self
    }

    pub const fn resets(mut self, value: fn(&Store) -> String, reset: fn(&mut Store, &Store)) -> Self {
        self.resets = Some((value, reset));
        self
    }

    /// The value as the reset sheet names it; `None` for a custom row Reset leaves alone.
    pub fn value(&self, store: &Store) -> Option<String> {
        match self.control {
            Control::Switch { get, .. } => Some(if get(store) { "On" } else { "Off" }.into()),
            Control::Choice { options, get, .. } => options.get(get(store)).map(|o| o.to_string()),
            Control::Stepper { unit, get, .. } => Some(format!("{}{unit}", get(store))),
            Control::Text { get, .. } => Some(match get(store) {
                t if t.is_empty() => "Empty".into(),
                t => format!("\u{201c}{t}\u{201d}"),
            }),
            Control::Path { get, .. } => Some(crate::util::tilde(&get(store))),
            Control::Custom(_) => self.resets.map(|(value, _)| value(store)),
        }
    }

    /// Whether its parent allows it; Advanced is the page's call.
    pub fn allowed(&self, store: &Store) -> bool {
        self.parent.is_none_or(|p| p(store))
    }

    fn reset(&self, store: &mut Store, defaults: &Store) {
        match self.control {
            Control::Switch { get, set } => set(store, get(defaults)),
            Control::Choice { get, set, .. } => set(store, get(defaults)),
            Control::Stepper { get, set, .. } => set(store, get(defaults)),
            Control::Text { get, set, .. } => set(store, get(defaults)),
            Control::Path { set, .. } => set(store, String::new()),
            Control::Custom(_) => {
                if let Some((_, reset)) = self.resets {
                    reset(store, defaults)
                }
            }
        }
    }

    fn fields(&self) -> impl Iterator<Item = &'static str> {
        let options = match self.control {
            Control::Choice { options, .. } => options,
            _ => &[],
        };
        [self.label, self.group].into_iter().chain(self.hint).chain(options.iter().copied())
    }
}

/// A stepper's next value, kept in range.
pub(crate) fn step(value: i32, by: i32, min: i32, max: i32) -> i32 {
    (value + by).clamp(min, max)
}

/// A row whose value differs from its default.
#[derive(Debug, PartialEq)]
pub(crate) struct Change {
    pub label: &'static str,
    pub from: String,
    pub to: String,
}

/// Built once: settings render compares against it every frame.
pub(crate) fn defaults() -> &'static Store {
    static DEFAULTS: std::sync::LazyLock<Store> = std::sync::LazyLock::new(Store::default);
    &DEFAULTS
}

pub(crate) fn changed(rows: &[Setting], store: &Store) -> Vec<Change> {
    let defaults = defaults();
    rows.iter()
        .filter_map(|r| match (r.value(store), r.value(defaults)) {
            (Some(from), Some(to)) if from != to => Some(Change { label: r.label, from, to }),
            _ => None,
        })
        .collect()
}

/// Puts every row back to its default and returns those that moved, whose effects must rerun.
pub(crate) fn reset(rows: &'static [Setting], store: &mut Store) -> Vec<&'static Setting> {
    let defaults = defaults();
    let moved: Vec<&'static Setting> = rows.iter().filter(|r| r.value(store) != r.value(defaults)).collect();
    for r in &moved {
        r.reset(store, defaults);
    }
    moved
}

pub(crate) fn advanced_count(rows: &[Setting]) -> usize {
    rows.iter().filter(|r| r.advanced).count()
}

/// The groups in the order their first row comes.
pub(crate) fn groups(rows: &[Setting]) -> Vec<&'static str> {
    let mut out: Vec<&'static str> = Vec::new();
    for r in rows {
        if !out.contains(&r.group) {
            out.push(r.group);
        }
    }
    out
}

/// The query's words, lowercased; an empty query searches nothing.
pub(crate) fn words(query: &str) -> Vec<String> {
    query.split_whitespace().map(str::to_lowercase).collect()
}

/// Every row whose label, group, hint or options hold each word, by section.
pub(crate) fn search(words: &[String]) -> Vec<(Section, Vec<&'static Setting>)> {
    if words.is_empty() {
        return Vec::new();
    }
    Section::ALL
        .into_iter()
        .map(|s| (s, s.rows().iter().filter(|r| crate::palette::matches(words, &r.fields().collect::<Vec<_>>())).collect::<Vec<_>>()))
        .filter(|(_, hits)| !hits.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use gpui_kit::div;

    fn noop(_: &mut Desktop, _: &mut Context<Desktop>) {}

    const ROWS: &[Setting] = &[
        Setting::switch("banners", "Banners", "Banners", |s| s.notifications.banners, |s, on| s.notifications.banners = on).hint("A macOS banner"),
        Setting::switch("sounds", "Sounds", "Play sounds", |s| s.sounds.all, |s, on| s.sounds.all = on),
        Setting::switch("done", "Sounds", "Done", |s| s.sounds.done, |s, on| s.sounds.done = on).under(|s| s.sounds.all),
        Setting::choice("mode", "Theme", "Appearance", &["System", "Light", "Dark"], Look::Thumbnails, |s| s.appearance.mode as usize, |s, i| s.appearance.mode = [store::Mode::System, store::Mode::Light, store::Mode::Dark][i]).effect(noop).advanced(),
        Setting::stepper("lines", "Scrollback", "Lines", (100, 900, 100), " lines", |s| s.sounds.done as i32 * 500 + 400, |s, n| s.sounds.done = n > 400),
        Setting::custom("update", "Software update", "Software update", |_, _, _| div()),
    ];

    #[test]
    fn only_changed_rows_go_on_the_reset_sheet() {
        let mut store = Store::default();
        assert_eq!(changed(ROWS, &store), []);
        (store.sounds.all, store.appearance.mode) = (false, store::Mode::Dark);
        assert_eq!(changed(ROWS, &store), [Change { label: "Play sounds", from: "Off".into(), to: "On".into() }, Change { label: "Appearance", from: "Dark".into(), to: "System".into() }]);
    }

    #[test]
    fn reset_restores_defaults_and_names_the_rows_that_moved() {
        let mut store = Store::default();
        (store.sounds.done, store.appearance.mode) = (false, store::Mode::Light);
        let moved: Vec<_> = reset(ROWS, &mut store).iter().map(|r| r.id).collect();
        assert_eq!(moved, ["done", "mode", "lines"]);
        assert_eq!((store.sounds.done, store.appearance.mode), (true, store::Mode::System));
        assert!(reset(ROWS, &mut store).is_empty());
    }

    #[test]
    fn a_custom_row_that_resets_goes_on_the_sheet_and_back_to_its_default() {
        const REMOTE: &[Setting] = &[Setting::custom("remote", "Repository", "Default remote", |_, _, _| div()).resets(|s| s.git.remote.clone(), |s, d| s.git.remote = d.git.remote.clone())];
        let mut store = Store::default();
        let default = store.git.remote.clone();
        store.git.remote = "upstream".into();
        assert_eq!(changed(REMOTE, &store), [Change { label: "Default remote", from: "upstream".into(), to: default.clone() }]);
        assert_eq!(reset(REMOTE, &mut store).len(), 1);
        assert_eq!(store.git.remote, default);
    }

    #[test]
    fn a_stepper_names_its_unit_and_stays_in_range() {
        let store = Store::default();
        assert_eq!(ROWS[4].value(&store).as_deref(), Some("900 lines"));
        assert_eq!([step(900, 100, 100, 900), step(150, -100, 100, 900), step(400, 100, 100, 900)], [900, 100, 500]);
    }

    #[test]
    fn a_dependent_row_shows_only_while_its_parent_allows_it() {
        let mut store = Store::default();
        assert!(ROWS[2].allowed(&store));
        store.sounds.all = false;
        assert!(!ROWS[2].allowed(&store));
        assert!(ROWS[1].allowed(&store));
    }

    #[test]
    fn groups_keep_the_order_of_their_first_row() {
        assert_eq!(groups(ROWS), ["Banners", "Sounds", "Theme", "Scrollback", "Software update"]);
        assert_eq!(advanced_count(ROWS), 1);
    }

    #[test]
    fn a_row_matches_on_its_hint_group_and_options_too() {
        let hit = |q: &str| ROWS.iter().filter(|r| crate::palette::matches(&words(q), &r.fields().collect::<Vec<_>>())).map(|r| r.id).collect::<Vec<_>>();
        assert_eq!(hit("macos"), ["banners"]);
        assert_eq!(hit("sounds"), ["sounds", "done"]);
        assert_eq!(hit("dark theme"), ["mode"]);
    }

    #[test]
    fn an_empty_query_finds_nothing() {
        assert!(search(&words(" ")).is_empty());
    }
}
