//! What a new key would clash with, read from the keymap in the order the app builds it (`vendor/gpui-pre/src/keymap.rs`):
//! the deepest context wins, and a binding with no context counts as the deepest, so where a place has focus its own
//! bindings tie with the global ones and the later binding wins. The keymap is gpui-base's bindings, then ours
//! (`actions::keymap`: built-in spots, then added keys), then the terminal's. An action the focused view doesn't handle
//! passes the key on: ⌘B formats only in a prompt with a selection, and is Toggle sidebar everywhere else.

use super::compact;
use super::model::{self, Custom, Saved, Shortcut, Slot, canon, key_of, put, remove_key};
use crate::actions::{self, FormatBold, FormatCode, FormatItalic, FormatLink, PageEdit};
use gpui_kit::{KeyBinding, Keystroke};
use std::collections::BTreeMap;

/// gpui-base's text-field bindings with ⌘, ⌃ or ⌥ on macOS (`input/base/state.rs`, `text/state.rs`), bound before ours.
/// Left out: ⌘⌥↑/↓, which our own Input bindings already take, and ⌃⌘Space, which macOS keeps.
const BASE: &[(&str, &str, &str)] = &[
    ("ctrl-backspace", "Delete back", "Text fields"),
    ("cmd-backspace", "Delete to line start", "Text fields"),
    ("cmd-delete", "Delete to line end", "Text fields"),
    ("alt-backspace", "Delete word back", "Text fields"),
    ("alt-delete", "Delete word forward", "Text fields"),
    ("cmd-enter", "Secondary Enter", "Text fields"),
    ("cmd-]", "Indent", "Text fields"),
    ("cmd-[", "Outdent", "Text fields"),
    ("cmd-a", "Select all", "Text fields"),
    ("cmd-c", "Copy", "Text fields"),
    ("cmd-x", "Cut", "Text fields"),
    ("cmd-v", "Paste", "Text fields"),
    ("cmd-z", "Undo", "Text fields"),
    ("cmd-shift-z", "Redo", "Text fields"),
    ("cmd-f", "Find", "Text fields"),
    ("cmd-shift-f", "Replace", "Text fields"),
    ("cmd-.", "Code actions", "Text fields"),
    ("ctrl-a", "Line start", "Text fields"),
    ("ctrl-e", "Line end", "Text fields"),
    ("cmd-left", "Line start", "Text fields"),
    ("cmd-right", "Line end", "Text fields"),
    ("cmd-up", "Text start", "Text fields"),
    ("cmd-down", "Text end", "Text fields"),
    ("alt-left", "Previous word", "Text fields"),
    ("alt-right", "Next word", "Text fields"),
    ("ctrl-shift-a", "Select to line start", "Text fields"),
    ("ctrl-shift-e", "Select to line end", "Text fields"),
    ("cmd-shift-left", "Select to line start", "Text fields"),
    ("cmd-shift-right", "Select to line end", "Text fields"),
    ("cmd-shift-up", "Select to start", "Text fields"),
    ("cmd-shift-down", "Select to end", "Text fields"),
    ("alt-shift-left", "Select previous word", "Text fields"),
    ("alt-shift-right", "Select next word", "Text fields"),
    ("cmd-a", "Select all", "Selectable text"),
    ("cmd-c", "Copy", "Selectable text"),
];

/// Keys macOS takes before the app sees them.
const SYSTEM: &[(&str, &str)] = &[
    ("cmd-tab", "switching apps"),
    ("cmd-space", "Spotlight"),
    ("cmd-h", "Hide Anywhere"),
    ("cmd-m", "Minimize"),
    ("cmd-alt-h", "Hide others"),
    ("ctrl-cmd-q", "Lock Screen"),
    ("cmd-shift-3", "screenshots"),
    ("cmd-shift-4", "screenshots"),
    ("cmd-shift-5", "screenshots"),
    ("cmd-`", "cycling windows"),
    ("ctrl-cmd-space", "Emoji & Symbols"),
    ("cmd-alt-escape", "Force Quit"),
];

/// ⌃ and a letter in a shell. On macOS no binding keeps them for the terminal, so a shortcut on one takes it.
const READLINE: &[(&str, &str)] = &[
    ("c", "interrupt"),
    ("d", "end of input"),
    ("z", "suspend"),
    ("r", "reverse search"),
    ("a", "start of line"),
    ("e", "end of line"),
    ("l", "clear screen"),
    ("u", "delete line"),
    ("w", "delete word"),
    ("k", "delete to end"),
    ("p", "previous command"),
    ("n", "next command"),
];

/// Who holds a key.
#[derive(Clone, Copy, PartialEq, Debug)]
pub(crate) enum Kind {
    Row,
    Set,
    /// A prompt's format key, which runs only with text selected.
    Prompt,
    /// gpui-base's text-field keys.
    Base,
    /// A web page's own edit keys.
    Page,
    /// Keys the terminal keeps for the program in it.
    Terminal,
}

impl Kind {
    /// Built in elsewhere, so this page can't take the key from it.
    pub(crate) fn fixed(self) -> bool {
        !matches!(self, Kind::Row | Kind::Set)
    }
}

/// A key someone holds, and where it sits in the keymap.
#[derive(Clone, Debug)]
pub(crate) struct Held {
    /// The Keyboard page row, for `Kind::Row` and `Kind::Set`.
    pub row: Option<usize>,
    pub key: String,
    pub label: String,
    pub place: Option<&'static str>,
    pub kind: Kind,
    pub at: i64,
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub(crate) enum Level {
    /// It can't be used: "Pick another".
    Refuse,
    /// Another shortcut holds it: Replace or Cancel.
    Clash,
    /// Something built in loses it: Use anyway or Cancel.
    Warn,
    /// Both keep working, each in its own place: Keep both or Cancel.
    Shared,
}

/// One thing a new key would change. `text` marks bold with `**`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Note {
    pub level: Level,
    pub text: String,
    /// The row Replace takes the key from.
    pub other: Option<usize>,
}

impl Note {
    fn new(level: Level, text: String) -> Self {
        Self { level, text, other: None }
    }

    pub(crate) fn plain(&self) -> String {
        self.text.replace("**", "")
    }
}

/// Which binding id is in which spot, for bindings `actions::keymap` builds from `bindings`.
static SPOT_OF: std::sync::LazyLock<BTreeMap<String, String>> = std::sync::LazyLock::new(|| {
    let all = actions::bindings();
    all.iter().map(actions::binding_id).zip(actions::spots(&all)).collect()
});

fn row_of(rows: &[Shortcut], spot: &str) -> Option<usize> {
    rows.iter().position(|r| r.id == spot || r.set.as_ref().is_some_and(|s| s.members.iter().any(|(id, _)| id == spot)))
}

/// What a binding that isn't a row's holds its key for, if the page tells of it.
fn built_in(b: &KeyBinding) -> Option<(Kind, &'static str, Option<&'static str>)> {
    let any = b.action().as_any();
    if let Some(PageEdit(edit)) = any.downcast_ref::<PageEdit>() {
        let label = match edit {
            web::Edit::Copy => "Copy",
            web::Edit::Cut => "Cut",
            web::Edit::Paste => "Paste",
            web::Edit::SelectAll => "Select all",
            web::Edit::Undo => "Undo",
            web::Edit::Redo => "Redo",
        };
        return Some((Kind::Page, label, Some("Browser")));
    }
    let format = [(any.is::<FormatBold>(), "Bold"), (any.is::<FormatItalic>(), "Italic"), (any.is::<FormatCode>(), "Code"), (any.is::<FormatLink>(), "Link")];
    format.into_iter().find(|(is, _)| *is).map(|(_, label)| (Kind::Prompt, label, Some("Text fields")))
}

/// Every key held where this page can see it, with `c`'s shortcuts in place, in keymap order.
pub(crate) fn pool(rows: &[Shortcut], c: &Custom) -> Vec<Held> {
    pool_for(rows, c, None)
}

/// `pool`, where a text-field key counts as already beaten only by a row other than `editing`: the key being recorded
/// for it isn't bound yet.
fn pool_for(rows: &[Shortcut], c: &Custom, editing: Option<usize>) -> Vec<Held> {
    let mut out = Vec::new();
    for (i, b) in c.keymap().iter().enumerate() {
        let at = i as i64;
        if let Some((kind, label, place)) = built_in(b) {
            out.push(Held { row: None, key: key_of(b), label: label.into(), place, kind, at });
            continue;
        }
        let id = actions::binding_id(b);
        // A copy of an action in another context follows its row, so only the row's own binding counts.
        if SPOT_OF.get(&id).is_some_and(|spot| *spot != id) {
            continue;
        }
        if let Some(r) = row_of(rows, &id) {
            let row = &rows[r];
            let kind = if row.set.is_some() { Kind::Set } else { Kind::Row };
            out.push(Held { row: Some(r), key: key_of(b), label: row.label.into(), place: row.place, kind, at });
        }
    }
    if cfg!(target_os = "macos") {
        let ours: Vec<String> = out.iter().filter(|h| h.place.is_none() && (editing.is_none() || h.row != editing)).map(|h| h.key.clone()).collect();
        let base = BASE.iter().map(|(key, label, place)| (canon(key), label, place)).filter(|(key, _, _)| !ours.contains(key));
        let base: Vec<Held> =
            base.map(|(key, label, place)| Held { row: None, key, label: (*label).into(), place: Some(place), kind: Kind::Base, at: -1 }).collect();
        out.splice(0..0, base);
    }
    for b in keys::bindings() {
        let key = key_of(&b);
        let label = match b.keystrokes()[0].inner().key.as_str() {
            "tab" if b.keystrokes()[0].inner().modifiers.shift => "Shift-Tab".to_string(),
            "tab" => "Tab completion".to_string(),
            "enter" => compact(&key),
            letter => readline(letter).map_or_else(|| compact(&key), capitalized),
        };
        out.push(Held { row: None, key, label, place: Some("Terminal"), kind: Kind::Terminal, at: i64::MAX });
    }
    out
}

fn readline(letter: &str) -> Option<&'static str> {
    READLINE.iter().find(|(l, _)| *l == letter).map(|(_, what)| *what)
}

fn capitalized(s: &str) -> String {
    let mut chars = s.chars();
    chars.next().map(|c| c.to_uppercase().chain(chars).collect()).unwrap_or_default()
}

/// What macOS uses `key` for, if it takes it first.
fn system(key: &str) -> Option<&'static str> {
    if !cfg!(target_os = "macos") {
        return None;
    }
    SYSTEM.iter().find(|(k, _)| canon(k) == key).map(|(_, what)| *what)
}

/// The readline key ⌃ and a letter would take from terminal programs, on macOS.
fn readline_key(key: &str) -> Option<&'static str> {
    let k = Keystroke::parse_with_os(key, keys::OS).ok()?;
    let ctrl_only = k.modifiers.control && !k.modifiers.alt && !k.modifiers.shift && !k.modifiers.platform;
    (cfg!(target_os = "macos") && ctrl_only).then(|| readline(&k.key)).flatten()
}

fn overlap(a: Option<&str>, b: Option<&str>) -> bool {
    a == b || a.is_none() || b.is_none()
}

/// What putting `key` in `slot` of row `row` would change, worst first; empty when nothing.
pub(crate) fn conflicts(rows: &[Shortcut], c: &Custom, row: usize, slot: Slot, key: &str) -> Vec<Note> {
    let me = &rows[row];
    let (place, g, mine_label) = (me.place, compact(key), me.label);
    let mut notes = Vec::new();
    if let Some(what) = system(key) {
        notes.push(Note::new(Level::Refuse, format!("macOS uses **{g}** for {what}. Pick another shortcut.")));
    }
    if me.chips(c).iter().any(|chip| chip.key == key && chip.slot != slot) {
        notes.push(Note::new(Level::Refuse, format!("This action already has **{g}**.")));
        return notes;
    }
    let mut edited = c.clone();
    put(&mut edited, me, slot, key);
    let pool = pool_for(rows, &edited, Some(row));
    let Some(mine) = pool.iter().find(|h| h.row == Some(row) && h.key == key).map(|h| h.at) else { return notes };
    for b in pool.iter().filter(|b| b.key == key && b.row != Some(row) && overlap(place, b.place)) {
        let (l, win) = (&b.label, mine > b.at);
        let p = b.place.unwrap_or_default();
        let clash = |text: String| {
            let tail = if b.kind == Kind::Set { " Change that set\u{2019}s modifier first." } else { "" };
            let rest = match b.row.map(|r| rows[r].keys(&edited).len()) {
                _ if b.kind == Kind::Set => "",
                Some(n) if n > 1 => " Replacing leaves it with its other shortcut.",
                _ => " Replacing leaves it with no shortcut.",
            };
            Note { level: Level::Clash, text: format!("{text}{tail}{rest}"), other: b.row.filter(|_| b.kind == Kind::Row) }
        };
        let note = if place == b.place {
            if !b.kind.fixed() {
                let there = b.place.map(|p| format!(" in the {p}")).unwrap_or_default();
                clash(format!("**{g}** is already **{l}**{there}."))
            } else if win {
                Note::new(Level::Warn, format!("In the {p}, **{l}** loses **{g}**."))
            } else {
                Note::new(Level::Refuse, format!("In the {p}, **{g}** stays **{l}**, which comes first, so this shortcut would never run."))
            }
        } else if place.is_none() {
            match (win, b.kind) {
                (false, Kind::Prompt) => {
                    Note::new(Level::Shared, format!("In a prompt with text selected, **{g}** is **{l}**. Everywhere else it runs **{mine_label}**."))
                }
                (false, Kind::Terminal) => {
                    Note::new(Level::Shared, format!("In a terminal, **{g}** still goes to the program. Everywhere else it runs **{mine_label}**."))
                }
                (false, _) => Note::new(Level::Shared, format!("In the {p}, **{g}** stays **{l}**. Everywhere else it runs **{mine_label}**.")),
                (true, Kind::Prompt) => {
                    Note::new(Level::Warn, format!("Prompts lose **{g}** ({l}): your shortcut comes later in the keymap, so it runs first."))
                }
                (true, Kind::Base) if p == "Text fields" => Note::new(Level::Warn, format!("Text fields lose **{g}** ({l}): your shortcut runs before them.")),
                (true, Kind::Base) => Note::new(Level::Warn, format!("Selectable text loses **{g}** ({l}): your shortcut runs before it.")),
                (true, Kind::Page) => Note::new(Level::Warn, format!("Web pages lose **{g}** ({l}): your shortcut runs before them.")),
                (true, _) => clash(format!("**{g}** is **{l}** in the {p}. Your shortcut comes later in the keymap, so it takes **{g}** there too.")),
            }
        } else {
            let here = place.unwrap_or_default();
            if win {
                Note::new(Level::Shared, format!("In the {here}, **{g}** will be **{mine_label}**; everywhere else it stays **{l}**."))
            } else {
                clash(format!("**{g}** is **{l}**, which comes first in the {here} too, so this shortcut would never run."))
            }
        };
        notes.push(note);
    }
    if let Some(what) = readline_key(key).filter(|_| place.is_none() || place == Some("Terminal")) {
        notes.push(Note::new(Level::Warn, format!("Terminal programs won\u{2019}t get **{g}** ({what}): your shortcut runs before the terminal sees it.")));
    }
    notes.sort_by_key(|n| n.level);
    notes
}

/// Who else holds any of `keys` where row `row` works, leaving out pairs that share a key out of the box (⌘L: Add to
/// chat, and Focus address bar in the Browser).
pub(crate) fn taken_by<'a>(rows: &[Shortcut], pool: &'a [Held], row: usize, keys: &[String]) -> Vec<&'a Held> {
    let me = &rows[row];
    let mine = me.defaults();
    pool.iter()
        .filter(|b| {
            let Some(r) = b.row.filter(|r| *r != row) else { return false };
            let shipped = rows[r].defaults().contains(&b.key) && mine.contains(&b.key) && b.place != me.place;
            keys.contains(&b.key) && !b.kind.fixed() && overlap(b.place, me.place) && !shipped
        })
        .collect()
}

/// Whoever now holds a key that resetting row `row` would take back.
pub(crate) fn reset_blockers(rows: &[Shortcut], c: &Custom, row: usize) -> Vec<Held> {
    let pool = pool(rows, c);
    taken_by(rows, &pool, row, &rows[row].defaults()).into_iter().cloned().collect()
}

/// What to say about blockers before taking keys back, and whether they can be: a set has to change its modifier first.
pub(crate) fn blocked_text(blockers: &[Held]) -> (String, bool) {
    let mut holders: Vec<(&str, Vec<String>)> = Vec::new();
    for b in blockers {
        match holders.iter_mut().find(|(l, _)| *l == b.label) {
            Some((_, keys)) => keys.push(compact(&b.key)),
            None => holders.push((&b.label, vec![compact(&b.key)])),
        }
    }
    let list = holders
        .iter()
        .map(|(l, keys)| format!("**{}** now {} to **{l}**", keys.join(", "), if keys.len() > 1 { "belong" } else { "belongs" }))
        .collect::<Vec<_>>()
        .join("; ");
    match blockers.iter().find(|b| b.kind == Kind::Set) {
        Some(set) => (format!("{list}. Change {}\u{2019}s modifier first.", set.label), false),
        None => (format!("{list}. Taking them back removes them there."), true),
    }
}

/// Applies `apply` unless a key it gives row `row` back is now another action's; then says whose.
pub(crate) fn restore_keys(rows: &[Shortcut], c: &mut Custom, row: usize, apply: impl FnOnce(&mut Custom)) -> Result<(), String> {
    let was = rows[row].keys(c);
    let mut next = c.clone();
    apply(&mut next);
    let back: Vec<String> = rows[row].keys(&next).into_iter().filter(|k| !was.contains(k)).collect();
    let pool = pool(rows, &next);
    if let Some(b) = taken_by(rows, &pool, row, &back).first() {
        return Err(format!("{} now belongs to {}", compact(&b.key), b.label));
    }
    *c = next;
    Ok(())
}

/// Undoes a reset of `row` that took keys from `holders` (row, what it had, what it had after): `row` gets `saved` back,
/// and each holder its own unless it changed since.
pub(crate) fn undo_take_back(rows: &[Shortcut], c: &mut Custom, row: usize, saved: &Saved, holders: &[(usize, Saved, Saved)]) -> Result<(), String> {
    restore_keys(rows, c, row, |c| {
        model::restore(c, saved);
        for (h, had, after) in holders {
            if model::save(c, &rows[*h]) == *after {
                model::reset(c, &rows[*h]);
                model::restore(c, had);
            }
        }
    })
}

/// Puts back `key`, removed from row `row`. If a newer key has filled its built-in spot since, that key keeps the spot and
/// this one comes back as an added key, but only when that changes nothing, since its precedence moves. Returns what to
/// tell, or why it can't.
pub(crate) fn undo_remove(rows: &[Shortcut], c: &mut Custom, row: usize, key: &str, was_spot: bool) -> Result<Option<String>, String> {
    let r = &rows[row];
    match r.spot_key(c) {
        Some(spot) if was_spot => {
            if let Some(n) = conflicts(rows, c, row, Slot::New, key).first() {
                return Err(n.plain());
            }
            restore_keys(rows, c, row, |c| c.added.push((r.id.clone(), key.to_string())))?;
            Ok(Some(format!("{} is back as an extra shortcut; {} keeps the built-in spot", compact(key), compact(&spot))))
        }
        _ => {
            restore_keys(rows, c, row, |c| match was_spot {
                true => put(c, r, Slot::New, key),
                false => c.added.push((r.id.clone(), key.to_string())),
            })?;
            Ok(None)
        }
    }
}

/// Takes `key` from each row a Replace note names, so row `row` can have it.
pub(crate) fn replace(rows: &[Shortcut], c: &mut Custom, notes: &[Note], key: &str) {
    for n in notes.iter().filter(|n| n.level == Level::Clash) {
        if let Some(other) = n.other {
            remove_key(c, &rows[other], key);
        }
    }
}

/// What holds `key` that this page can't change: text fields, web pages, the terminal's keys and readline's.
pub(crate) fn elsewhere(rows: &[Shortcut], c: &Custom, key: &str) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = pool(rows, c)
        .into_iter()
        .filter(|h| h.kind.fixed() && h.key == key)
        .map(|h| {
            let place = match h.kind {
                Kind::Terminal => "Terminal programs".to_string(),
                Kind::Prompt => "Prompt with a selection".to_string(),
                _ => h.place.unwrap_or_default().to_string(),
            };
            (h.label, place)
        })
        .collect();
    if let Some(what) = readline_key(key) {
        out.push((capitalized(what), "Terminal programs".into()));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::actions::native;
    use crate::settings::keybindings::model::{SHORTCUTS, remove, set_mods};
    use gpui_kit::{KeyContext, Keymap, actions};

    fn k(keys: &str) -> String {
        canon(&native(keys, None))
    }

    fn at(label: &str) -> usize {
        SHORTCUTS.iter().position(|r| r.label == label).unwrap()
    }

    fn notes(c: &Custom, label: &str, slot: Slot, keys: &str) -> Vec<(Level, String)> {
        conflicts(&SHORTCUTS, c, at(label), slot, &k(keys)).into_iter().map(|n| (n.level, n.plain())).collect()
    }

    fn first(c: &Custom, label: &str, slot: Slot, keys: &str) -> (Level, String) {
        notes(c, label, slot, keys).into_iter().next().expect("a note")
    }

    actions!(test, [BaseKey]);

    /// The actions a press of `keys` runs where `stack` has focus, in the order dispatch tries them, with `c`'s shortcuts
    /// in place between stand-ins for gpui-base's text-field keys and the terminal's.
    fn dispatch(c: &Custom, keys: &str, stack: &[&str]) -> Vec<&'static str> {
        let mut keymap = Keymap::default();
        keymap.add_bindings(BASE.iter().map(|(key, _, place)| KeyBinding::new(key, BaseKey, Some(if *place == "Text fields" { "Input" } else { "TextView" }))));
        keymap.add_bindings(c.keymap());
        keymap.add_bindings(keys::bindings());
        let stack: Vec<KeyContext> = stack.iter().map(|s| KeyContext::parse(s).unwrap()).collect();
        let press = Keystroke::parse_with_os(&k(keys), keys::OS).unwrap();
        keymap.bindings_for_input(&[press], &stack).0.iter().map(|b| b.action().name()).collect()
    }

    #[test]
    fn a_key_another_shortcut_has_in_the_same_place_clashes_and_replacing_names_what_loses_it() {
        let c = Custom::default();
        assert_eq!(
            first(&c, "New worktree", Slot::Spot, "cmd-k"),
            (Level::Clash, format!("{} is already Command palette. Replacing leaves it with no shortcut.", compact(&k("cmd-k"))))
        );
        let n = conflicts(&SHORTCUTS, &c, at("New worktree"), Slot::Spot, &k("cmd-k"));
        let mut c = c;
        replace(&SHORTCUTS, &mut c, &n, &k("cmd-k"));
        assert!(SHORTCUTS[at("Command palette")].keys(&c).is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_global_key_added_later_takes_a_places_key_there_too_and_dispatch_agrees() {
        let c = Custom::default();
        let (level, text) = first(&c, "Toggle sidebar", Slot::New, "cmd-r");
        assert_eq!(level, Level::Clash);
        assert!(text.starts_with("⌘R is Reload in the Browser. Your shortcut comes later in the keymap, so it takes ⌘R there too."), "{text}");
        let mut edited = c.clone();
        put(&mut edited, &SHORTCUTS[at("Toggle sidebar")], Slot::New, &k("cmd-r"));
        assert_eq!(dispatch(&edited, "cmd-r", &["Root", "Browser"])[0], "desktop::ToggleSidebar");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_global_key_in_an_earlier_spot_leaves_a_place_its_key_and_dispatch_agrees() {
        let c = Custom::default();
        assert_eq!(
            first(&c, "Focus mode", Slot::Spot, "cmd-r"),
            (Level::Shared, "In the Browser, ⌘R stays Reload. Everywhere else it runs Focus mode.".into())
        );
        let mut edited = c.clone();
        put(&mut edited, &SHORTCUTS[at("Focus mode")], Slot::Spot, &k("cmd-r"));
        assert_eq!(dispatch(&edited, "cmd-r", &["Root", "Browser"])[0], "desktop::Reload");
        assert_eq!(dispatch(&edited, "cmd-r", &["Root"]), ["desktop::ToggleFocus"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_places_key_later_than_a_global_one_works_there_and_the_global_one_elsewhere() {
        let c = Custom::default();
        assert_eq!(
            first(&c, "Reload", Slot::Spot, "cmd-b"),
            (Level::Shared, "In the Browser, ⌘B will be Reload; everywhere else it stays Toggle sidebar.".into())
        );
        let mut edited = c.clone();
        put(&mut edited, &SHORTCUTS[at("Reload")], Slot::Spot, &k("cmd-b"));
        assert_eq!(dispatch(&edited, "cmd-b", &["Root", "Browser"])[0], "desktop::Reload");
        assert_eq!(dispatch(&edited, "cmd-b", &["Root"]), ["desktop::ToggleSidebar"]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_places_key_added_after_a_global_one_still_works_there() {
        let c = Custom::default();
        assert_eq!(first(&c, "Reload", Slot::New, "cmd-b").0, Level::Shared);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_global_key_takes_a_text_fields_key_since_gpui_base_binds_first() {
        let c = Custom::default();
        assert_eq!(first(&c, "Go to Activity", Slot::New, "cmd-z"), (Level::Warn, "Text fields lose ⌘Z (Undo): your shortcut runs before them.".into()));
        assert_eq!(first(&c, "Go to Activity", Slot::New, "cmd-backspace").1, "Text fields lose ⌘⌫ (Delete to line start): your shortcut runs before them.");
        let mut edited = c.clone();
        put(&mut edited, &SHORTCUTS[at("Go to Activity")], Slot::New, &k("cmd-z"));
        assert_eq!(dispatch(&edited, "cmd-z", &["Root", "Input"])[0], "desktop::OpenActivity");
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_text_field_key_our_own_binding_already_beats_is_left_out() {
        let c = Custom::default();
        assert!(notes(&c, "Go to Activity", Slot::New, "cmd-.").iter().all(|(_, t)| !t.contains("Code actions")));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_prompts_format_key_keeps_working_ahead_of_an_earlier_spot_and_loses_to_an_added_key() {
        let c = Custom::default();
        assert_eq!(
            first(&c, "Toggle rail", Slot::Spot, "cmd-i"),
            (Level::Shared, "In a prompt with text selected, ⌘I is Italic. Everywhere else it runs Toggle rail.".into())
        );
        let mut rail = c.clone();
        put(&mut rail, &SHORTCUTS[at("Toggle rail")], Slot::Spot, &k("cmd-i"));
        assert_eq!(dispatch(&rail, "cmd-i", &["Root", "Input"]), ["desktop::FormatItalic", "desktop::ToggleRail"]);
        assert_eq!(
            first(&c, "Go to Activity", Slot::New, "cmd-i"),
            (Level::Warn, "Prompts lose ⌘I (Italic): your shortcut comes later in the keymap, so it runs first.".into())
        );
        let mut activity = c.clone();
        put(&mut activity, &SHORTCUTS[at("Go to Activity")], Slot::New, &k("cmd-i"));
        assert_eq!(dispatch(&activity, "cmd-i", &["Root", "Input"])[0], "desktop::OpenActivity");
    }

    #[test]
    fn the_terminal_keeps_its_keys_for_the_program() {
        let c = Custom::default();
        let all = notes(&c, "New worktree", Slot::Spot, "cmd-enter");
        assert!(
            all.iter().any(|(level, text)| *level == Level::Shared && text.starts_with("In a terminal,") && text.contains("still goes to the program")),
            "{all:?}"
        );
        let mut edited = c.clone();
        put(&mut edited, &SHORTCUTS[at("New worktree")], Slot::Spot, &k("cmd-enter"));
        assert!(dispatch(&edited, "cmd-enter", &["Root", keys::CONTEXT]).is_empty());
    }

    #[test]
    fn a_terminal_only_shortcut_on_a_key_the_terminal_keeps_is_refused() {
        let c = Custom::default();
        let (level, text) = first(&c, "Copy", Slot::Spot, "shift-tab");
        assert_eq!(level, Level::Refuse);
        assert!(text.ends_with("which comes first, so this shortcut would never run."), "{text}");
        let mut edited = c.clone();
        put(&mut edited, &SHORTCUTS[at("Copy")], Slot::Spot, &k("shift-tab"));
        assert!(dispatch(&edited, "shift-tab", &["Root", keys::CONTEXT]).is_empty());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_global_ctrl_letter_warns_that_terminal_programs_lose_it_on_macos() {
        let c = Custom::default();
        assert_eq!(
            first(&c, "Next session", Slot::Spot, "ctrl-r"),
            (Level::Warn, "Terminal programs won\u{2019}t get ⌃R (reverse search): your shortcut runs before the terminal sees it.".into())
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn keys_macos_takes_are_refused() {
        assert_eq!(
            first(&Custom::default(), "Toggle sidebar", Slot::New, "cmd-h"),
            (Level::Refuse, "macOS uses ⌘H for Hide Anywhere. Pick another shortcut.".into())
        );
    }

    #[test]
    fn a_key_the_row_already_has_is_refused() {
        assert_eq!(first(&Custom::default(), "Command palette", Slot::New, "cmd-k").0, Level::Refuse);
    }

    #[test]
    fn a_key_held_by_a_set_clashes_but_cant_be_replaced() {
        let n = conflicts(&SHORTCUTS, &Custom::default(), at("New worktree"), Slot::Spot, &k("ctrl-3"));
        assert_eq!((n[0].level, n[0].other), (Level::Clash, None));
        assert!(n[0].plain().contains("Change that set\u{2019}s modifier first."));
    }

    #[test]
    fn a_free_key_raises_nothing() {
        assert!(notes(&Custom::default(), "New worktree", Slot::Spot, "cmd-alt-shift-y").is_empty());
    }

    #[test]
    fn reset_asks_before_taking_back_a_key_another_action_now_holds() {
        let mut c = Custom::default();
        put(&mut c, &SHORTCUTS[at("Command palette")], Slot::Spot, &k("cmd-shift-p"));
        put(&mut c, &SHORTCUTS[at("New worktree")], Slot::Spot, &k("cmd-k"));
        let blockers = reset_blockers(&SHORTCUTS, &c, at("Command palette"));
        assert_eq!(blockers.iter().map(|b| b.label.as_str()).collect::<Vec<_>>(), ["New worktree"]);
        assert!(blocked_text(&blockers).1);
        assert!(reset_blockers(&SHORTCUTS, &Custom::default(), at("Add selection to chat")).is_empty(), "⌘L ships on both");
    }

    #[test]
    fn reset_of_a_key_a_set_now_holds_says_to_change_the_sets_modifier_first() {
        let mut c = Custom::default();
        set_mods(&mut c, &SHORTCUTS[at("Jump to tab")], "alt");
        set_mods(&mut c, &SHORTCUTS[at("Jump to session")], "cmd");
        let blockers = reset_blockers(&SHORTCUTS, &c, at("Jump to tab"));
        let (text, can) = blocked_text(&blockers);
        assert!(!can && text.ends_with("Change Jump to session\u{2019}s modifier first."), "{text}");
    }

    #[test]
    fn undo_refuses_a_key_another_action_took_in_the_meantime() {
        let mut c = Custom::default();
        let palette = at("Command palette");
        remove(&mut c, &SHORTCUTS[palette], Slot::Spot);
        put(&mut c, &SHORTCUTS[at("New worktree")], Slot::Spot, &k("cmd-k"));
        let before = c.clone();
        let why = undo_remove(&SHORTCUTS, &mut c, palette, &k("cmd-k"), true).unwrap_err();
        assert_eq!((why, c), (format!("{} now belongs to New worktree", compact(&k("cmd-k"))), before));
    }

    #[test]
    fn undoing_a_reset_gives_back_the_key_it_took_from_another_action() {
        let mut c = Custom::default();
        let (palette, worktree) = (at("Command palette"), at("New worktree"));
        put(&mut c, &SHORTCUTS[palette], Slot::Spot, &k("cmd-shift-p"));
        put(&mut c, &SHORTCUTS[worktree], Slot::Spot, &k("cmd-k"));
        let before = c.clone();
        let (saved, had) = (model::save(&c, &SHORTCUTS[palette]), model::save(&c, &SHORTCUTS[worktree]));
        remove_key(&mut c, &SHORTCUTS[worktree], &k("cmd-k"));
        model::reset(&mut c, &SHORTCUTS[palette]);
        let holders = [(worktree, had, model::save(&c, &SHORTCUTS[worktree]))];
        let mut undone = c.clone();
        undo_take_back(&SHORTCUTS, &mut undone, palette, &saved, &holders).unwrap();
        assert_eq!(undone, before);
        put(&mut c, &SHORTCUTS[worktree], Slot::Spot, &k("cmd-alt-shift-y"));
        undo_take_back(&SHORTCUTS, &mut c, palette, &saved, &holders).unwrap();
        assert_eq!(SHORTCUTS[worktree].keys(&c), [k("cmd-alt-shift-y")], "a holder changed since keeps its new key");
    }

    #[test]
    fn undo_restores_a_removed_key_to_its_empty_spot() {
        let mut c = Custom::default();
        let palette = at("Command palette");
        remove(&mut c, &SHORTCUTS[palette], Slot::Spot);
        assert_eq!(undo_remove(&SHORTCUTS, &mut c, palette, &k("cmd-k"), true), Ok(None));
        assert_eq!(c, Custom::default());
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn undo_brings_a_removed_key_back_as_an_extra_one_when_a_newer_key_holds_its_spot() {
        let mut c = Custom::default();
        let rail = at("Toggle rail");
        remove(&mut c, &SHORTCUTS[rail], Slot::Spot);
        put(&mut c, &SHORTCUTS[rail], Slot::New, &k("cmd-u"));
        let told = undo_remove(&SHORTCUTS, &mut c, rail, &k("cmd-\\"), true).unwrap();
        assert_eq!(told.as_deref(), Some("⌘\\ is back as an extra shortcut; ⌘U keeps the built-in spot"));
        assert_eq!(SHORTCUTS[rail].keys(&c), [k("cmd-u"), k("cmd-\\")]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn undo_refuses_to_bring_a_key_back_as_an_extra_one_when_that_would_change_what_it_does() {
        let mut c = Custom::default();
        let rail = at("Toggle rail");
        put(&mut c, &SHORTCUTS[rail], Slot::Spot, &k("cmd-i"));
        remove(&mut c, &SHORTCUTS[rail], Slot::Spot);
        put(&mut c, &SHORTCUTS[rail], Slot::New, &k("cmd-u"));
        let before = c.clone();
        let why = undo_remove(&SHORTCUTS, &mut c, rail, &k("cmd-i"), true).unwrap_err();
        assert!(why.starts_with("Prompts lose ⌘I"), "{why}");
        assert_eq!(c, before);
    }

    #[test]
    fn find_by_keys_lists_what_holds_a_key_outside_this_page() {
        let c = Custom::default();
        let copy = elsewhere(&SHORTCUTS, &c, &k("cmd-c"));
        assert!(copy.iter().any(|(l, w)| l == "Copy" && w == "Browser"), "{copy:?}");
        let tab = elsewhere(&SHORTCUTS, &c, &k("tab"));
        assert_eq!(tab, [("Tab completion".to_string(), "Terminal programs".to_string())]);
    }
}
