use crate::actions::{self, native};
use crate::desktop::pages;
use crate::{browser, inbox, settings};
use gpui_kit::{KeyBinding, Keystroke};
use std::collections::BTreeMap;

#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Topic {
    Sessions,
    Navigation,
    Panels,
    Browser,
    App,
}

pub(crate) const TOPICS: [(Topic, &str); 5] =
    [(Topic::Sessions, "Sessions"), (Topic::Navigation, "Navigation"), (Topic::Panels, "Panels & tabs"), (Topic::Browser, "Browser"), (Topic::App, "App")];

/// What an action does and the group listing it; `None` leaves it out.
pub(crate) fn label(action: &str) -> Option<(&'static str, Topic)> {
    use Topic::*;
    let action = action.strip_prefix("desktop::")?;
    if let Some(page) = pages::by_action(action) {
        return Some((page.title, Navigation));
    }
    Some(match action {
        "StartSession" => ("New session", Sessions),
        "NewWorktree" => ("New worktree", Sessions),
        "OpenSession" => ("Open session", Sessions),
        "NextNeedsYou" => ("Next Needs you", Sessions),
        "GoToUpNext" => ("Go to Up next", Sessions),
        "NextSession" => ("Next session", Sessions),
        "PrevSession" => ("Previous session", Sessions),
        "JumpTo" => ("Jump to session", Sessions),
        "BackToParent" => ("Back to the chat that delegated this one", Sessions),
        "AddToChat" => ("Add selection to chat", Sessions),
        "PrevPrompt" => ("Previous prompt in chat", Sessions),
        "NextPrompt" => ("Next prompt in chat", Sessions),
        "OpenPalette" => ("Command palette", Navigation),
        "GoToFile" => ("Go to file", Navigation),
        "ToggleSidebar" => ("Toggle sidebar", Navigation),
        "ToggleRail" => ("Toggle rail", Navigation),
        "ToggleFocus" => ("Focus mode", Navigation),
        "NewTab" => ("New tab", Panels),
        "CloseTab" => ("Close tab", Panels),
        "PrevTab" => ("Previous tab", Panels),
        "NextTab" => ("Next tab", Panels),
        "JumpToTab" => ("Jump to tab", Panels),
        "SwitchSpace" => ("Switch space", Navigation),
        "SplitRight" => ("Split right", Panels),
        "SplitDown" => ("Split down", Panels),
        "FocusPane" => ("Focus pane", Panels),
        "ZoomPane" => ("Zoom pane", Panels),
        "EqualizePanes" => ("Equalize panes", Panels),
        "NewBrowser" => ("New browser", Browser),
        "FocusAddress" => ("Focus address bar", Browser),
        "Reload" => ("Reload", Browser),
        "Back" => ("Back", Browser),
        "Forward" => ("Forward", Browser),
        "OpenSettings" => ("Settings", App),
        "ZoomIn" => ("Zoom in", App),
        "ZoomOut" => ("Zoom out", App),
        "ResetZoom" => ("Actual size", App),
        "FocusSearch" => ("Search settings", App),
        "SettingsUp" => ("Up a level in Settings", App),
        "Save" => ("Save", App),
        "Quit" => ("Quit", App),
        "CopySelection" => ("Copy", App),
        "SelectAll" => ("Select all", App),
        "Paste" => ("Paste", App),
        "CopyWorktreePath" => ("Copy worktree path", App),
        _ => return None,
    })
}

/// Where `b` works: `Some(None)` everywhere, `Some(Some(place))` in a terminal, browser, the Inbox or Settings, `None`
/// somewhere the list leaves out.
pub(crate) fn place(b: &KeyBinding) -> Option<Option<&'static str>> {
    match b.predicate().map(|p| p.to_string()).as_deref() {
        None => Some(None),
        Some(keys::CONTEXT) => Some(Some("Terminal")),
        Some(browser::CONTEXT) => Some(Some("Browser")),
        Some(inbox::CONTEXT) => Some(Some("Inbox")),
        Some(settings::CONTEXT) => Some(Some("Settings")),
        Some(_) => None,
    }
}

/// `k` as this OS writes it in `Store.keys`, so two spellings of one key compare equal.
pub(crate) fn canon(k: &str) -> String {
    Keystroke::parse_with_os(k, keys::OS).map_or_else(|_| k.to_string(), |k| k.unparse(keys::OS))
}

pub(crate) fn key_of(b: &KeyBinding) -> String {
    b.keystrokes().iter().map(|k| k.inner().unparse(keys::OS)).collect::<Vec<_>>().join(" ")
}

/// The shortcuts a user changed: `keys` holds the key in an action's built-in spot (empty once removed), `added` the
/// other keys, as (spot, key) in the order they were added. `Store.keys` and `Store.added`, apart from the store.
#[derive(Clone, Default, PartialEq, Debug)]
pub(crate) struct Custom {
    pub keys: BTreeMap<String, String>,
    pub added: Vec<(String, String)>,
}

impl Custom {
    pub(crate) fn of(s: &store::Store) -> Self {
        Self { keys: s.keys.clone(), added: s.added.clone() }
    }

    pub(crate) fn store(self, s: &mut store::Store) {
        (s.keys, s.added) = (self.keys, self.added);
    }

    pub(crate) fn keymap(&self) -> Vec<KeyBinding> {
        actions::keymap(&self.keys, &self.added)
    }
}

/// Keys one modifier changes together, such as Jump to session's ⌃1–⌃9.
#[derive(Debug)]
pub(crate) struct Set {
    /// Each member's spot and its key without modifiers ("3", "left").
    pub members: Vec<(String, String)>,
    /// The modifiers a menu offers for the whole set, as macOS writes them.
    pub options: &'static [&'static str],
    /// The keys after the modifier, as the row shows them: "1…9".
    pub shown: &'static str,
}

const DIGITS: &[&str] = &["ctrl", "cmd", "alt", "ctrl-cmd", "alt-cmd", "ctrl-alt"];
const ARROWS: &[&str] = &["alt-cmd", "ctrl-cmd", "ctrl-alt", "ctrl-shift"];

impl Set {
    /// Each member's key under `mods`, a modifier from `options`.
    pub(crate) fn keys_with(&self, mods: &str) -> Vec<String> {
        self.members.iter().map(|(_, key)| canon(&native(&format!("{mods}-{key}"), None))).collect()
    }
}

/// Where a key sits in a row: its action's built-in spot, the nth key added to it, or a key about to be added.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Slot {
    Spot,
    Added(usize),
    New,
}

#[derive(Clone, PartialEq, Debug)]
pub(crate) struct Chip {
    pub key: String,
    pub slot: Slot,
}

/// One row of the Keyboard page.
#[derive(Debug)]
pub(crate) struct Shortcut {
    /// Its spot in `Custom`; a set's first member's.
    pub id: String,
    pub label: &'static str,
    pub topic: Topic,
    pub place: Option<&'static str>,
    /// The key in its built-in spot; `None` for an action that ships without one.
    pub default: Option<String>,
    pub set: Option<Set>,
}

impl Shortcut {
    /// The key in its built-in spot now: its default until changed, `None` once removed.
    pub(crate) fn spot_key(&self, c: &Custom) -> Option<String> {
        match c.keys.get(&self.id) {
            Some(k) if k.is_empty() => None,
            Some(k) => Some(k.clone()),
            None => self.default.clone(),
        }
    }

    /// Its keys, the spot's first and then the added ones in order; a set's members' keys.
    pub(crate) fn chips(&self, c: &Custom) -> Vec<Chip> {
        if let Some(set) = &self.set {
            let defaults = self.defaults();
            return set.members.iter().zip(defaults).map(|((id, _), d)| Chip { key: c.keys.get(id).cloned().unwrap_or(d), slot: Slot::Spot }).collect();
        }
        let spot = self.spot_key(c).map(|key| Chip { key, slot: Slot::Spot });
        let added = c.added.iter().filter(|(id, _)| *id == self.id).enumerate().map(|(n, (_, key))| Chip { key: key.clone(), slot: Slot::Added(n) });
        spot.into_iter().chain(added).collect()
    }

    pub(crate) fn keys(&self, c: &Custom) -> Vec<String> {
        self.chips(c).into_iter().map(|chip| chip.key).collect()
    }

    /// The keys it ships with.
    pub(crate) fn defaults(&self) -> Vec<String> {
        match &self.set {
            Some(set) => set.keys_with(self.default_mods()),
            None => self.default.iter().cloned().collect(),
        }
    }

    pub(crate) fn changed(&self, c: &Custom) -> bool {
        match &self.set {
            Some(set) => set.members.iter().any(|(id, _)| c.keys.contains_key(id)),
            None => c.keys.contains_key(&self.id) || c.added.iter().any(|(id, _)| *id == self.id),
        }
    }

    /// A set's modifier out of the box.
    pub(crate) fn default_mods(&self) -> &'static str {
        self.set.as_ref().and_then(|set| set.options.iter().find(|o| self.default.as_deref() == Some(&*set.keys_with(o)[0])).copied()).unwrap_or("")
    }

    /// A set's modifier now; `None` when its keys match no option, as hand-edited keys can.
    pub(crate) fn mods(&self, c: &Custom) -> Option<&'static str> {
        let set = self.set.as_ref()?;
        let keys = self.keys(c);
        set.options.iter().find(|o| set.keys_with(o) == keys).copied()
    }

    /// Its default, as its reset names it: "⌘ K", "⌃ 1…9", "no shortcut".
    pub(crate) fn default_shown(&self) -> String {
        match &self.set {
            Some(set) => format!("{}{}", mods_shown(self.default_mods()), set.shown),
            None => self.default.as_deref().map_or_else(|| "no shortcut".to_string(), super::compact),
        }
    }
}

/// `mods`, as macOS writes them, in glyphs: "⌃⌘".
pub(crate) fn mods_shown(mods: &str) -> String {
    let k = super::compact(&native(&format!("{mods}-x"), None));
    k.trim_end_matches('X').trim_end().to_string()
}

/// Each labelled action, in binding order, then the actions that ship without a key. Bindings of one action with an
/// argument (Jump to session 1–9) make one row, a `Set`.
pub(crate) fn shortcut_rows(bindings: &[KeyBinding]) -> Vec<Shortcut> {
    let spots = actions::spots(bindings);
    let mut rows: Vec<Shortcut> = Vec::new();
    for (b, spot) in bindings.iter().zip(&spots) {
        let (Some(place), Some((label, topic))) = (place(b), label(b.action().name())) else { continue };
        if actions::binding_id(b) != *spot {
            continue;
        }
        let key = key_of(b);
        if actions::action_id(b.action()) == b.action().name() {
            rows.push(Shortcut { id: spot.clone(), label, topic, place, default: Some(key), set: None });
            continue;
        }
        let Some(k) = b.keystrokes().first().map(|k| k.inner().key.clone()) else { continue };
        match rows.iter_mut().find(|r| r.label == label && r.place == place) {
            Some(Shortcut { set: Some(set), .. }) => set.members.push((spot.clone(), k)),
            _ => {
                let (options, shown) = if k.chars().all(|c| c.is_ascii_digit()) { (DIGITS, "1…9") } else { (ARROWS, "←→↑↓") };
                let set = Set { members: vec![(spot.clone(), k)], options, shown };
                rows.push(Shortcut { id: spot.clone(), label, topic, place, default: Some(key), set: Some(set) });
            }
        }
    }
    for a in actions::unbound() {
        if let Some((label, topic)) = label(a.name()) {
            rows.push(Shortcut { id: actions::action_id(a.as_ref()), label, topic, place: None, default: None, set: None });
        }
    }
    rows
}

/// Built once: render runs on every poll, and parsing the keymap each time is wasted.
pub(crate) static SHORTCUTS: std::sync::LazyLock<Vec<Shortcut>> = std::sync::LazyLock::new(|| shortcut_rows(&actions::bindings()));

/// Puts `key` in `slot` of `row`. A new key takes the built-in spot while it's empty, else joins the added keys. A key
/// re-recorded in place of an added one goes to the end of them, as a key added now.
pub(crate) fn put(c: &mut Custom, row: &Shortcut, slot: Slot, key: &str) {
    let spot_free = row.default.is_some() && row.spot_key(c).is_none();
    match slot {
        Slot::Spot => set_spot(c, row, key),
        Slot::New if spot_free => set_spot(c, row, key),
        Slot::New => c.added.push((row.id.clone(), key.to_string())),
        Slot::Added(_) => {
            remove(c, row, slot);
            c.added.push((row.id.clone(), key.to_string()));
        }
    }
}

fn set_spot(c: &mut Custom, row: &Shortcut, key: &str) {
    if row.default.as_deref() == Some(key) {
        c.keys.remove(&row.id);
    } else {
        c.keys.insert(row.id.clone(), key.to_string());
    }
}

/// Takes the key in `slot` off `row`. An emptied built-in spot stays empty, so a key added later can take it.
pub(crate) fn remove(c: &mut Custom, row: &Shortcut, slot: Slot) {
    match slot {
        Slot::Spot => {
            c.keys.insert(row.id.clone(), String::new());
        }
        Slot::Added(n) => {
            let at = c.added.iter().enumerate().filter(|(_, (id, _))| *id == row.id).nth(n).map(|(i, _)| i);
            if let Some(i) = at {
                c.added.remove(i);
            }
        }
        Slot::New => {}
    }
}

/// Takes `key` off `row`, wherever it holds it.
pub(crate) fn remove_key(c: &mut Custom, row: &Shortcut, key: &str) {
    if let Some(chip) = row.chips(c).into_iter().find(|chip| chip.key == key) {
        remove(c, row, chip.slot);
    }
}

/// Gives a set `mods` for every member.
pub(crate) fn set_mods(c: &mut Custom, row: &Shortcut, mods: &str) {
    let Some(set) = &row.set else { return };
    for (((id, _), key), default) in set.members.iter().zip(set.keys_with(mods)).zip(row.defaults()) {
        if key == default {
            c.keys.remove(id);
        } else {
            c.keys.insert(id.clone(), key);
        }
    }
}

/// Puts `row` back to the keys it ships with.
pub(crate) fn reset(c: &mut Custom, row: &Shortcut) {
    match &row.set {
        Some(set) => set.members.iter().for_each(|(id, _)| {
            c.keys.remove(id);
        }),
        None => {
            c.keys.remove(&row.id);
            c.added.retain(|(id, _)| *id != row.id);
        }
    }
}

/// What `row` had in `c`, to put back after a reset.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Saved {
    keys: Vec<(String, Option<String>)>,
    added: Vec<(String, String)>,
}

pub(crate) fn save(c: &Custom, row: &Shortcut) -> Saved {
    let ids: Vec<String> = match &row.set {
        Some(set) => set.members.iter().map(|(id, _)| id.clone()).collect(),
        None => vec![row.id.clone()],
    };
    Saved {
        keys: ids.into_iter().map(|id| (id.clone(), c.keys.get(&id).cloned())).collect(),
        added: c.added.iter().filter(|(id, _)| *id == row.id).cloned().collect(),
    }
}

pub(crate) fn restore(c: &mut Custom, saved: &Saved) {
    for (id, k) in &saved.keys {
        match k {
            Some(k) => c.keys.insert(id.clone(), k.clone()),
            None => c.keys.remove(id),
        };
    }
    c.added.extend(saved.added.iter().cloned());
}

/// Rows that differ from their defaults.
pub(crate) fn changed_rows<'a>(rows: &'a [Shortcut], c: &Custom) -> Vec<&'a Shortcut> {
    rows.iter().filter(|r| r.changed(c)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(label: &str) -> &'static Shortcut {
        SHORTCUTS.iter().find(|r| r.label == label).unwrap()
    }

    fn k(keys: &str) -> String {
        canon(&native(keys, None))
    }

    #[test]
    fn every_binding_appears_in_the_shortcut_table() {
        // Copy, paste and undo inside a web page are the page's own text editing.
        for b in actions::bindings().iter().filter(|b| place(b).is_some() && b.action().name() != "desktop::PageEdit") {
            assert!(label(b.action().name()).is_some(), "{} has no label", b.action().name());
        }
    }

    #[test]
    fn actions_that_ship_without_a_key_are_listed_with_none() {
        let activity = row("Go to Activity");
        assert_eq!((activity.id.as_str(), activity.default.as_ref(), activity.topic), ("desktop::OpenActivity", None, Topic::Navigation));
        assert!(activity.chips(&Custom::default()).is_empty());
    }

    #[test]
    fn an_action_bound_once_per_argument_is_one_set_whose_modifier_changes_together() {
        let jump = row("Jump to session");
        let set = jump.set.as_ref().unwrap();
        assert_eq!((set.members.len(), jump.default_mods(), jump.topic), (9, "ctrl", Topic::Sessions));
        let mut c = Custom::default();
        set_mods(&mut c, jump, "cmd");
        assert_eq!(c.keys.get("desktop::JumpTo(3)"), Some(&k("cmd-3")));
        assert_eq!((jump.mods(&c), jump.keys(&c)[8].clone()), (Some("cmd"), k("cmd-9")));
        set_mods(&mut c, jump, "ctrl");
        assert!(c.keys.is_empty());
        assert_eq!(row("Focus pane").set.as_ref().unwrap().members.len(), 4);
    }

    #[test]
    fn terminal_browser_and_inbox_shortcuts_are_marked_with_where_they_work() {
        let at = |l: &str| row(l).place;
        assert_eq!([at("Copy"), at("Reload"), at("Open session"), at("Settings")], [Some("Terminal"), Some("Browser"), Some("Inbox"), None]);
    }

    #[test]
    fn a_changed_key_stays_in_its_built_in_spot_and_its_default_forgets_the_change() {
        let mut c = Custom::default();
        let split = row("Split right");
        put(&mut c, split, Slot::Spot, &k("cmd-shift-y"));
        assert_eq!(split.chips(&c), [Chip { key: k("cmd-shift-y"), slot: Slot::Spot }]);
        put(&mut c, split, Slot::Spot, &k("cmd-d"));
        assert_eq!(c, Custom::default());
    }

    #[test]
    fn a_second_key_joins_the_added_keys_after_the_spot() {
        let mut c = Custom::default();
        let palette = row("Command palette");
        put(&mut c, palette, Slot::New, &k("cmd-shift-p"));
        assert_eq!(palette.keys(&c), [k("cmd-k"), k("cmd-shift-p")]);
        assert_eq!(c.added, [(palette.id.clone(), k("cmd-shift-p"))]);
        assert!(palette.changed(&c));
    }

    #[test]
    fn removing_the_built_in_key_leaves_its_spot_empty_and_a_key_added_then_takes_it() {
        let mut c = Custom::default();
        let rail = row("Toggle rail");
        put(&mut c, rail, Slot::New, &k("cmd-i"));
        remove(&mut c, rail, Slot::Spot);
        assert_eq!(rail.chips(&c), [Chip { key: k("cmd-i"), slot: Slot::Added(0) }]);
        put(&mut c, rail, Slot::New, &k("cmd-u"));
        assert_eq!(rail.chips(&c)[0], Chip { key: k("cmd-u"), slot: Slot::Spot });
    }

    #[test]
    fn a_re_recorded_added_key_goes_to_the_end_of_the_added_keys() {
        let mut c = Custom::default();
        let (palette, file) = (row("Command palette"), row("Go to file"));
        put(&mut c, palette, Slot::New, &k("cmd-shift-p"));
        put(&mut c, file, Slot::New, &k("cmd-shift-o"));
        put(&mut c, palette, Slot::Added(0), &k("cmd-alt-p"));
        assert_eq!(c.added.iter().map(|(_, k)| k.clone()).collect::<Vec<_>>(), [k("cmd-shift-o"), k("cmd-alt-p")]);
    }

    #[test]
    fn reset_puts_a_row_back_and_restore_puts_back_what_it_had() {
        let mut c = Custom::default();
        let palette = row("Command palette");
        put(&mut c, palette, Slot::Spot, &k("cmd-shift-p"));
        put(&mut c, palette, Slot::New, &k("cmd-alt-p"));
        let saved = save(&c, palette);
        reset(&mut c, palette);
        assert_eq!(c, Custom::default());
        restore(&mut c, &saved);
        assert_eq!(palette.keys(&c), [k("cmd-shift-p"), k("cmd-alt-p")]);
    }

    #[test]
    fn a_rows_reset_names_its_default() {
        assert_eq!(
            [row("Command palette").default_shown(), row("Jump to session").default_shown(), row("Go to Activity").default_shown()],
            [super::super::compact(&k("cmd-k")), format!("{}1…9", mods_shown("ctrl")), "no shortcut".to_string()]
        );
    }
}
