mod conflicts;
pub(crate) mod model;

use super::catalog::Setting;
use super::toasts::{Toast, Undo};
use super::{Section, Settings, SettingsEvent, dropdown, focus_ring, group, row};
use crate::actions;
use crate::desktop::chrome::Confirm;
use conflicts::{Held, Level, Note};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use model::{Custom, SHORTCUTS, Shortcut, Slot, TOPICS};
use std::collections::BTreeMap;
use theme::*;

/// Only here so Reset puts every shortcut back; the page draws its own rows.
pub(super) const ROWS: &[Setting] = &[Setting::custom("shortcuts", "Shortcuts", "Changed shortcuts", |d, _, _| {
    row("Changed shortcuts", None, div().text_size(px(12.5)).text_color(TEXT_2).child(changed(&d.store)))
})
.resets(changed, |s, from| (s.keys, s.added) = (from.keys.clone(), from.added.clone()))
.effect(|d, cx| actions::rebind(&d.store.keys, &d.store.added, cx))];

fn changed(s: &store::Store) -> String {
    match model::changed_rows(&SHORTCUTS, &Custom::of(s)).len() {
        0 => "None".into(),
        1 => "1 shortcut".into(),
        n => format!("{n} shortcuts"),
    }
}

/// Keys handled outside the keymap, so no binding lists them.
const FIXED: [(&str, &[&str]); 3] =
    [("Close a menu, then recording, then search, then Settings", &["Esc"]), ("Move in the palette", &["↑", "↓"]), ("Open from the palette", &["↩"])];

/// `k` as this OS's menus print it, one glyph or name per key: "⌘ ⇧ J" on macOS, "Ctrl Shift J" elsewhere.
fn glyphs(k: &Keystroke) -> String {
    glyphs_on(k, cfg!(target_os = "macos"))
}

fn glyphs_on(k: &Keystroke, mac: bool) -> String {
    let m = k.modifiers;
    let key = match k.key.as_str() {
        "enter" if mac => "↩".to_string(),
        "tab" if mac => "⇥".to_string(),
        "backspace" if mac => "⌫".to_string(),
        "delete" if mac => "⌦".to_string(),
        "escape" => "Esc".to_string(),
        "left" => "←".to_string(),
        "right" => "→".to_string(),
        "up" => "↑".to_string(),
        "down" => "↓".to_string(),
        "space" => "Space".to_string(),
        key if !mac && key.len() > 1 => key[..1].to_uppercase() + &key[1..],
        key => key.to_uppercase(),
    };
    [m.control, m.alt, m.shift, m.platform]
        .into_iter()
        .zip(modifier_names(mac))
        .filter(|(on, _)| *on)
        .map(|(_, g)| g.to_string())
        .chain([key])
        .collect::<Vec<_>>()
        .join(" ")
}

fn modifier_names(mac: bool) -> [&'static str; 4] {
    if mac { ["⌃", "⌥", "⇧", "⌘"] } else { ["Ctrl", "Alt", "Shift", if cfg!(target_os = "windows") { "Win" } else { "Super" }] }
}

/// Keys as `Store.keys` holds them ("cmd-shift-j"), in glyphs.
fn shown(keys: &str) -> String {
    keys.split_whitespace().map(|k| Keystroke::parse_with_os(k, keys::OS).map_or_else(|_| k.to_string(), |k| glyphs(&k))).collect::<Vec<_>>().join("  ")
}

/// One key as a sentence names it: "⌘K", "Ctrl+K".
pub(crate) fn compact(keys: &str) -> String {
    let s = shown(keys);
    if cfg!(target_os = "macos") { s.replace(' ', "") } else { s.replace(' ', "+") }
}

/// The key that runs `action` now, as menus and hints show it ("⌘⇧B"): the last one bound outside any context, else the
/// last one bound. Read from the live keymap, so a changed shortcut shows its new key.
pub(crate) fn hint(action: &dyn Action, cx: &App) -> Option<String> {
    let keymap = cx.key_bindings();
    let keymap = keymap.borrow();
    let all: Vec<&KeyBinding> = keymap.bindings_for_action(action).collect();
    let b = all.iter().rev().find(|b| b.predicate().is_none()).or(all.last())?;
    Some(compact(&model::key_of(b)))
}

/// Shortcuts whose action, group, place or keys hold every word.
fn find<'a>(rows: &'a [Shortcut], c: &Custom, words: &[String]) -> Vec<&'a Shortcut> {
    rows.iter()
        .filter(|r| {
            let topic = TOPICS.iter().find(|(t, _)| *t == r.topic).map_or("", |(_, title)| title);
            let keys = r.keys(c);
            let glyphs: Vec<String> = keys.iter().map(|k| shown(k).replace(' ', "")).collect();
            let fields: Vec<&str> = [r.label, topic, r.place.unwrap_or_default()]
                .into_iter()
                .chain(keys.iter().map(String::as_str))
                .chain(glyphs.iter().map(String::as_str))
                .collect();
            crate::palette::matches(words, &fields)
        })
        .collect()
}

/// Whether `k` can be a shortcut: it holds ⌘, ⌃ or ⌥, or is a function key, so typing never triggers one.
fn takes(k: &Keystroke) -> bool {
    let m = k.modifiers;
    m.platform || m.control || m.alt || k.key.strip_prefix('f').is_some_and(|n| n.parse::<u8>().is_ok())
}

/// Why a plain key can't be a shortcut.
fn refused(k: &Keystroke) -> String {
    let key = glyphs(&Keystroke { modifiers: Modifiers::default(), ..k.clone() });
    format!("{key} alone types text. Add {}.", if cfg!(target_os = "macos") { "⌘, ⌃ or ⌥" } else { "Ctrl or Alt" })
}

/// Each matching shortcut's action, and its place and keys as the Keyboard page shows them, for Settings search.
pub(super) fn search(words: &[String], keys: &BTreeMap<String, String>, added: &[(String, String)]) -> Vec<(&'static str, Div)> {
    let c = Custom { keys: keys.clone(), added: added.to_vec() };
    find(&SHORTCUTS, &c, words)
        .into_iter()
        .map(|r| (r.label, div().flex().items_center().gap(px(8.)).children(r.place.map(ui::tag)).child(key_list(&r.keys(&c)))))
        .collect()
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Filter {
    All,
    Changed,
    Unassigned,
}

/// A key pressed for a row that would change something, waiting on Replace, Keep both, Use anyway or Cancel.
pub(crate) struct Pending {
    key: String,
    notes: Vec<Note>,
}

/// A row taking a new key.
pub(crate) struct Recording {
    row: usize,
    slot: Slot,
    /// The modifiers held so far, shown before a key completes them.
    held: Modifiers,
    /// Why the last key pressed can't be a shortcut.
    refused: Option<String>,
    pending: Option<Pending>,
}

/// The Keyboard page's filters, the row taking new keys, and the reset or lookup waiting on the user.
pub(crate) struct Keyboard {
    pub(crate) recording: Option<Recording>,
    filter: Filter,
    search: Entity<InputState>,
    /// Find by keys: waiting for a press (`Some(None)`), or the key looked up.
    find: Option<Option<String>>,
    /// A row whose reset would take keys back from these.
    taking: Option<(usize, Vec<Held>)>,
    /// The recording field's, so it gets the modifiers as they're held.
    focus: FocusHandle,
}

impl Keyboard {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Settings>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search by name or keys"));
        let subs = vec![cx.subscribe(&search, |_, _, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                cx.notify();
            }
        })];
        (Self { recording: None, filter: Filter::All, search, find: None, taking: None, focus: cx.focus_handle() }, subs)
    }

    pub(super) fn stop(&mut self) {
        (self.recording, self.find, self.taking) = (None, None, None);
    }
}

/// `text`, with the parts between `**` in bold.
fn rich(text: &str) -> StyledText {
    let (mut plain, mut bold) = (String::new(), Vec::new());
    for (i, part) in text.split("**").enumerate() {
        let start = plain.len();
        plain.push_str(part);
        if i % 2 == 1 {
            bold.push((start..plain.len(), HighlightStyle { font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() }));
        }
    }
    StyledText::new(plain).with_highlights(bold)
}

fn key_list<S: AsRef<str>>(keys: &[S]) -> Div {
    // Some rows have nine; the first and last say it.
    let keys: Vec<String> = keys.iter().map(|k| shown(k.as_ref())).collect();
    let shown: Vec<&String> = if keys.len() > 4 { vec![&keys[0], &keys[keys.len() - 1]] } else { keys.iter().collect() };
    let sep = if keys.len() > 4 { "–" } else { "or" };
    div().flex().flex_none().items_center().gap(px(6.)).text_size(px(12.)).text_color(TEXT_3).when(keys.is_empty(), |d| d.child("None")).children(
        shown.into_iter().enumerate().flat_map(|(i, k)| {
            let sep = (i > 0).then(|| div().child(sep).into_any_element());
            sep.into_iter().chain([keycaps(k).into_any_element()])
        }),
    )
}

/// One cap per space-separated glyph, so "⌘ K" is ⌘ and K.
fn keycaps(keys: &str) -> Div {
    div().flex().flex_none().gap(px(3.)).children(keys.split_whitespace().map(|k| {
        div()
            .min_w(px(20.))
            .h(px(20.))
            .px(px(5.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .bg(SURFACE)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5), ui::shadow(rgba(0x00000017), 1., 0.)])
            .text_size(px(11.5))
            .font_weight(FontWeight::MEDIUM)
            .text_color(TEXT_2)
            .child(k.to_string())
    }))
}

fn shortcut_row(label: impl IntoElement, control: impl IntoElement) -> Div {
    div()
        .min_h(px(40.))
        .px(px(14.))
        .py(px(7.))
        .flex()
        .items_center()
        .gap(px(10.))
        .child(div().flex_1().min_w_0().flex().items_center().gap(px(8.)).text_size(px(13.)).text_color(TEXT).child(label))
        .child(control)
}

fn small_button(id: impl Into<ElementId>, label: &'static str, primary: bool) -> Stateful<Div> {
    focus_ring(
        ui::button(id, if primary { ui::Variant::Primary } else { ui::Variant::Secondary }, None, label).h(px(24.)).px(px(9.)).text_size(px(12.)).tab_index(0),
    )
}

/// A note under a row, tinted by how much it asks of the user.
fn note_box(level: Level, body: impl IntoElement, actions: Vec<Stateful<Div>>) -> Div {
    let (bg, tint, glyph) = match level {
        Level::Refuse => (FAILED_BG, FAILED_TEXT, "warning"),
        Level::Clash | Level::Warn => (WAITING_BG, WAITING, "warning"),
        Level::Shared => (FILL_2, TEXT_3, "keyboard"),
    };
    div()
        .mx(px(10.))
        .mb(px(10.))
        .pl(px(12.))
        .pr(px(10.))
        .py(px(9.))
        .flex()
        .items_start()
        .gap(px(9.))
        .rounded(px(9.))
        .bg(bg)
        .child(div().mt(px(1.)).child(icon(glyph, 14., tint)))
        .child(div().flex_1().min_w_0().text_size(px(12.5)).line_height(px(17.)).text_color(TEXT).child(body))
        .child(div().flex_none().flex().gap(px(6.)).children(actions))
}

impl Settings {
    fn custom(&self) -> Custom {
        Custom::of(&self.store)
    }

    /// Writes `edit` to the store's shortcuts and rebinds.
    fn edit_keys(&mut self, cx: &mut Context<Self>, edit: impl Fn(&mut Custom) + 'static) {
        self.write(Some(&ROWS[0]), cx, move |s, _| {
            let mut c = Custom::of(s);
            edit(&mut c);
            c.store(s);
        });
    }

    /// Takes `k` for the Keyboard page when it's waiting on a press: the row being changed, or Find by keys. Runs ahead of
    /// the keymap, so ⌘K becomes the new keys instead of opening the palette. True when it took the key.
    pub(crate) fn record_shortcut(&mut self, k: &Keystroke, shown: bool, cx: &mut Context<Self>) -> bool {
        if !shown || self.section != Section::Keyboard {
            self.keyboard.stop();
            return false;
        }
        let plain = k.modifiers == Modifiers::default();
        let escape = plain && k.key == "escape";
        if let Some(found) = &self.keyboard.find {
            match (found, escape) {
                (_, true) => self.keyboard.find = None,
                (None, false) => self.keyboard.find = Some(Some(k.unparse(keys::OS))),
                (Some(_), false) => return false,
            }
            cx.notify();
            return true;
        }
        if self.keyboard.taking.is_some() && escape {
            self.keyboard.taking = None;
            cx.notify();
            return true;
        }
        let Some(rec) = &mut self.keyboard.recording else { return false };
        // While a note asks, keys reach its buttons; Esc goes back to recording.
        if rec.pending.is_some() {
            if escape {
                rec.pending = None;
                cx.notify();
            }
            return escape;
        }
        let (row, slot) = (rec.row, rec.slot);
        if !escape && !(plain && k.key == "backspace") && !takes(k) {
            rec.refused = Some(refused(k));
        } else if escape {
            self.stop_recording(cx);
        } else if plain && k.key == "backspace" {
            self.stop_recording(cx);
            if slot != Slot::New {
                self.remove_chip(row, slot, cx);
            }
        } else {
            let key = k.unparse(keys::OS);
            self.assign(row, slot, key, cx);
        }
        cx.notify();
        true
    }

    fn stop_recording(&mut self, cx: &mut Context<Self>) {
        self.keyboard.recording = None;
        cx.emit(SettingsEvent::FocusRoot);
    }

    fn start_recording(&mut self, row: usize, slot: Slot, window: &mut Window, cx: &mut Context<Self>) {
        self.keyboard.stop();
        self.menu = None;
        self.keyboard.recording = Some(Recording { row, slot, held: Modifiers::default(), refused: None, pending: None });
        window.focus(&self.keyboard.focus, cx);
        cx.notify();
    }

    /// Puts `key` in `slot` of `row` if nothing it would change needs asking; else shows what it would change.
    fn assign(&mut self, row: usize, slot: Slot, key: String, cx: &mut Context<Self>) {
        let c = self.custom();
        let r = &SHORTCUTS[row];
        if slot != Slot::New && r.chips(&c).iter().any(|chip| chip.slot == slot && chip.key == key) {
            return self.stop_recording(cx);
        }
        let notes = conflicts::conflicts(&SHORTCUTS, &c, row, slot, &key);
        if notes.is_empty() {
            return self.commit(row, slot, key, Vec::new(), cx);
        }
        if let Some(rec) = &mut self.keyboard.recording {
            (rec.pending, rec.refused) = (Some(Pending { key, notes }), None);
        }
    }

    /// Puts `key` in `slot` of `row`, first taking it from the rows `replacing` names.
    fn commit(&mut self, row: usize, slot: Slot, key: String, replacing: Vec<Note>, cx: &mut Context<Self>) {
        self.stop_recording(cx);
        let r = &SHORTCUTS[row];
        let told = format!("{} is now {}", r.label, compact(&key));
        self.edit_keys(cx, move |c| {
            conflicts::replace(&SHORTCUTS, c, &replacing, &key);
            model::put(c, &SHORTCUTS[row], slot, &key);
        });
        self.toast(Toast::tell(told), cx);
    }

    /// Answers the note under the row being changed: `take` puts the key in, replacing where the note says.
    fn resolve(&mut self, take: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(rec) = &mut self.keyboard.recording else { return };
        let Some(pending) = rec.pending.take() else { return };
        if take && pending.notes.first().is_some_and(|n| n.level != Level::Refuse) {
            let (row, slot) = (rec.row, rec.slot);
            self.commit(row, slot, pending.key, pending.notes, cx);
        } else {
            window.focus(&self.keyboard.focus, cx);
        }
        cx.notify();
    }

    /// Takes the key in `slot` off `row`, with an Undo that puts it back if nothing took it meanwhile.
    fn remove_chip(&mut self, row: usize, slot: Slot, cx: &mut Context<Self>) {
        let r = &SHORTCUTS[row];
        let Some(key) = r.chips(&self.custom()).into_iter().find(|chip| chip.slot == slot).map(|chip| chip.key) else { return };
        let was_spot = slot == Slot::Spot;
        self.edit_keys(cx, move |c| model::remove(c, &SHORTCUTS[row], slot));
        let told = format!("Removed {} from {}", compact(&key), r.label);
        let undo = Undo::new(move |d, _, cx| {
            let mut c = Custom::of(&d.store);
            let told = conflicts::undo_remove(&SHORTCUTS, &mut c, row, &key, was_spot);
            match told {
                Ok(told) => {
                    c.store(&mut d.store);
                    d.changed(Some(&ROWS[0]), cx);
                    d.sync_settings(cx);
                    if let Some(told) = told {
                        d.settings_toast(Toast::tell(told), cx);
                    }
                }
                Err(why) => d.settings_toast(Toast::failed(format!("Can\u{2019}t undo: {why}"), None), cx),
            }
        });
        self.toast(Toast::undo(told, undo), cx);
    }

    /// Puts `row` back to its default keys, unless another action now holds one: then asks first.
    fn reset_shortcut(&mut self, row: usize, cx: &mut Context<Self>) {
        let blockers = conflicts::reset_blockers(&SHORTCUTS, &self.custom(), row);
        if blockers.is_empty() {
            self.take_back(row, Vec::new(), cx);
        } else {
            self.keyboard.recording = None;
            self.keyboard.taking = Some((row, blockers));
        }
        cx.notify();
    }

    /// Resets `row`, taking its keys back from `from`, with an Undo that puts back what it had while it's still at its
    /// default and nothing took those keys meanwhile.
    fn take_back(&mut self, row: usize, from: Vec<Held>, cx: &mut Context<Self>) {
        self.keyboard.taking = None;
        let r = &SHORTCUTS[row];
        let saved = model::save(&self.custom(), r);
        let had: Vec<(usize, model::Saved)> = from.iter().filter_map(|b| b.row).map(|h| (h, model::save(&self.custom(), &SHORTCUTS[h]))).collect();
        let take = move |c: &mut Custom| {
            for b in &from {
                if let Some(holder) = b.row {
                    model::remove_key(c, &SHORTCUTS[holder], &b.key);
                }
            }
            model::reset(c, &SHORTCUTS[row]);
        };
        // The write lands later, so the state after it is worked out here.
        let mut now = self.custom();
        take(&mut now);
        self.edit_keys(cx, take);
        let holders: Vec<_> = had.into_iter().map(|(h, had)| (h, had, model::save(&now, &SHORTCUTS[h]))).collect();
        let undo = Undo::new(move |d, _, cx| {
            let mut c = Custom::of(&d.store);
            if SHORTCUTS[row].changed(&c) {
                return;
            }
            match conflicts::undo_take_back(&SHORTCUTS, &mut c, row, &saved, &holders) {
                Ok(()) => {
                    c.store(&mut d.store);
                    d.changed(Some(&ROWS[0]), cx);
                    d.sync_settings(cx);
                }
                Err(why) => d.settings_toast(Toast::failed(format!("Can\u{2019}t undo: {why}"), None), cx),
            }
        });
        self.toast(Toast::undo(format!("{} is back to {}", r.label, r.default_shown()), undo), cx);
    }

    fn pick_mods(&mut self, row: usize, mods: &'static str, cx: &mut Context<Self>) {
        self.menu = None;
        let r = &SHORTCUTS[row];
        self.edit_keys(cx, move |c| model::set_mods(c, &SHORTCUTS[row], mods));
        let set = r.set.as_ref().map_or("", |s| s.shown);
        self.toast(Toast::tell(format!("{} is now {}{set}", r.label, model::mods_shown(mods))), cx);
    }

    /// Starts recording `label`'s key in `slot`, as a click on it does, and presses `keys` when given: capture's steps.
    pub(crate) fn try_shortcut(&mut self, label: &str, new: bool, keys: Option<&str>, window: &mut Window, cx: &mut Context<Self>) {
        let Some(row) = SHORTCUTS.iter().position(|r| r.label == label) else { return };
        let slot = if new || SHORTCUTS[row].chips(&self.custom()).is_empty() { Slot::New } else { Slot::Spot };
        self.start_recording(row, slot, window, cx);
        if let Some(keys) = keys {
            self.assign(row, slot, model::canon(&actions::native(keys, None)), cx);
        }
    }

    /// Looks up what `keys` do, as Find by keys does: capture's steps.
    pub(crate) fn find_keys(&mut self, keys: &str, cx: &mut Context<Self>) {
        self.keyboard.find = Some(Some(model::canon(&actions::native(keys, None))));
        cx.notify();
    }

    /// The Unassigned filter with Fixed keys open: capture's steps.
    pub(crate) fn show_unassigned(&mut self, cx: &mut Context<Self>) {
        (self.keyboard.filter, self.fixed_open) = (Filter::Unassigned, true);
        cx.notify();
    }

    fn chip(&self, row: usize, chip: &model::Chip, cx: &mut Context<Self>) -> Stateful<Div> {
        let (r, slot) = (&SHORTCUTS[row], chip.slot);
        let id = SharedString::from(format!("key-{row}-{slot:?}"));
        let group = SharedString::from(format!("{id}-group"));
        let button = div()
            .id(id.clone())
            .px(px(5.))
            .py(px(3.))
            .flex()
            .items_center()
            .rounded(px(7.))
            .bg(FILL_1)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5).inset()])
            .cursor_pointer()
            .tab_index(0)
            .hover(|d| d.bg(FILL_3))
            .child(keycaps(&shown(&chip.key)))
            .tooltip({
                let tip = format!("Change {} for {}", compact(&chip.key), r.label);
                move |window, cx| Tooltip::new(tip.clone()).build(window, cx)
            })
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_recording(row, slot, window, cx)));
        let remove = div()
            .id(SharedString::from(format!("{id}-remove")))
            .absolute()
            .top(px(-6.))
            .right(px(-6.))
            .size(px(16.))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(TEXT_3)
            .cursor_pointer()
            .opacity(0.)
            .group_hover(group.clone(), |s| s.opacity(1.))
            .child(icon("x", 9., PAGE))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.remove_chip(row, slot, cx)));
        div().id(SharedString::from(format!("{id}-chip"))).group(group).relative().child(focus_ring(button)).child(remove)
    }

    fn recording_field(&self, rec: &Recording, cx: &mut Context<Self>) -> Stateful<Div> {
        let level = rec.pending.as_ref().and_then(|p| p.notes.first()).map(|n| n.level);
        let border = match level {
            Some(Level::Refuse) => FAILED,
            Some(_) => WAITING,
            None => ACCENT,
        };
        let held = rec.held;
        let mods: Vec<&str> = [held.control, held.alt, held.shift, held.platform]
            .into_iter()
            .zip(modifier_names(cfg!(target_os = "macos")))
            .filter(|(on, _)| *on)
            .map(|(_, g)| g)
            .collect();
        let inside: AnyElement = match &rec.pending {
            Some(p) => keycaps(&shown(&p.key)).into_any_element(),
            None if !mods.is_empty() => keycaps(&mods.join(" ")).into_any_element(),
            None => div().text_size(px(12.)).text_color(TEXT_3).child("Press keys\u{2026}").into_any_element(),
        };
        div()
            .id("key-recording")
            .track_focus(&self.keyboard.focus)
            .min_w(px(84.))
            .h(px(28.))
            .px(px(6.))
            .flex()
            .items_center()
            .justify_center()
            .bg(SURFACE)
            .rounded(px(7.))
            .border_1()
            .border_color(border)
            .when(level.is_none(), |d| d.shadow(vec![ui::ring(ACCENT_GLOW, 3.)]))
            .child(inside)
            .on_modifiers_changed(cx.listener(|this, ev: &ModifiersChangedEvent, _, cx| {
                if let Some(rec) = &mut this.keyboard.recording {
                    rec.held = ev.modifiers;
                    cx.notify();
                }
            }))
    }

    /// A set's modifier button and its menu, which leaves out modifiers another shortcut's keys would clash with.
    fn set_control(&self, row: usize, cx: &mut Context<Self>) -> Div {
        let (r, c) = (&SHORTCUTS[row], self.custom());
        let Some(set) = &r.set else { return div() };
        let id: &'static str = SHORTCUTS[row].id.as_str();
        let open = self.menu == Some(id);
        let mods = r.mods(&c);
        let button = div()
            .id(SharedString::from(format!("set-{row}")))
            .px(px(5.))
            .py(px(3.))
            .flex()
            .items_center()
            .gap(px(4.))
            .rounded(px(7.))
            .bg(if open { FILL_3 } else { FILL_1 })
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5).inset()])
            .tab_index(0)
            .hover(|d| d.bg(FILL_3))
            .child(match mods {
                Some(m) => set_caps(m, set.shown),
                None => div().px(px(3.)).text_size(px(12.)).text_color(TEXT_2).child("Mixed"),
            })
            .child(icon("chevron-down", 10., TEXT_3));
        let menu = open.then(|| {
            let pool = conflicts::pool(&SHORTCUTS, &c);
            let rows = set.options.iter().map(|&o| {
                let keys = set.keys_with(o);
                let used = pool.iter().find(|b| b.row != Some(row) && keys.contains(&b.key) && !b.kind.fixed());
                let blocked = used.filter(|_| Some(o) != mods);
                dropdown::pick_row(SharedString::from(format!("set-{row}-{o}")), Some(o) == mods)
                    .child(set_caps(o, set.shown))
                    .children(blocked.map(|b| div().ml(px(8.)).text_size(px(11.5)).text_color(TEXT_3).child(format!("Used by {}", b.label))))
                    .when(blocked.is_some(), |d| d.opacity(0.5))
                    .when(blocked.is_none(), |d| d.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick_mods(row, o, cx))))
            });
            let caption = div().px(px(8.)).py(px(4.)).text_size(px(11.5)).text_color(TEXT_3).child("Modifier for the whole set");
            let menu = ui::pop(div()).min_w(px(200.)).p(px(4.)).flex().flex_col().child(caption).children(rows);
            ui::dropdown_right(30., ui::menu_in(SharedString::from(format!("set-{row}-menu-in")), dropdown::shuts(menu, id, cx)))
        });
        div().relative().child(focus_ring(dropdown::trigger(button, id, open, cx))).children(menu)
    }

    fn shortcut_line(&self, row: usize, cx: &mut Context<Self>) -> Div {
        let (r, c) = (&SHORTCUTS[row], self.custom());
        let changed = r.changed(&c);
        let rec = self.keyboard.recording.as_ref().filter(|rec| rec.row == row);
        let title = div().flex().items_center().gap(px(8.)).child(r.label).children(r.place.map(ui::tag));
        let reset = changed.then(|| {
            let tip = format!("Reset to {}", r.default_shown());
            ui::icon_button_sized(SharedString::from(format!("key-reset-{row}")), "discard", 22., TEXT_3)
                .tab_index(0)
                .tooltip(move |window, cx| Tooltip::new(tip.clone()).build(window, cx))
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.reset_shortcut(row, cx)))
        });
        let keys: Vec<AnyElement> = if r.set.is_some() {
            vec![self.set_control(row, cx).into_any_element()]
        } else {
            let chips = r.chips(&c);
            let mut out: Vec<AnyElement> = Vec::new();
            for chip in &chips {
                if !out.is_empty() {
                    out.push(div().text_size(px(12.)).text_color(TEXT_4).child("or").into_any_element());
                }
                match rec.filter(|rec| rec.slot == chip.slot) {
                    Some(rec) => out.push(self.recording_field(rec, cx).into_any_element()),
                    None => out.push(self.chip(row, chip, cx).into_any_element()),
                }
            }
            match rec.filter(|rec| rec.slot == Slot::New) {
                Some(rec) => out.push(self.recording_field(rec, cx).into_any_element()),
                None if rec.is_some() => {}
                None if chips.is_empty() => out.push(
                    focus_ring(
                        div()
                            .id(SharedString::from(format!("key-add-{row}")))
                            .h(px(26.))
                            .px(px(8.))
                            .flex()
                            .items_center()
                            .gap(px(4.))
                            .rounded(px(7.))
                            .cursor_pointer()
                            .tab_index(0)
                            .text_size(px(12.5))
                            .text_color(TEXT_2)
                            .hover(|d| d.bg(FILL_2).text_color(TEXT))
                            .child(icon("plus", 11., TEXT_3))
                            .child("Add shortcut"),
                    )
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_recording(row, Slot::New, window, cx)))
                    .into_any_element(),
                ),
                None => out.push(
                    focus_ring(ui::icon_button_sized(SharedString::from(format!("key-add-{row}")), "plus", 24., TEXT_3).tab_index(0))
                        .tooltip(|window, cx| Tooltip::new("Add another shortcut").build(window, cx))
                        .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.start_recording(row, Slot::New, window, cx)))
                        .into_any_element(),
                ),
            }
            out
        };
        let line = shortcut_row(title, div().flex().flex_wrap().justify_end().items_center().gap(px(6.)).children(reset).children(keys)).relative();
        let below = match (rec, &self.keyboard.taking) {
            (Some(rec), _) => Some(match &rec.pending {
                Some(p) => self.pending_note(p, cx),
                None => {
                    let hint = rec.refused.clone().unwrap_or_else(|| {
                        let removes = if rec.slot == Slot::New { "" } else { " · ⌫ removes it" };
                        format!("Press the new shortcut · Esc cancels{removes}")
                    });
                    let color = if rec.refused.is_some() { FAILED_TEXT } else { TEXT_3 };
                    div().mx(px(14.)).mb(px(8.)).mt(px(-4.)).flex().justify_end().text_size(px(12.)).text_color(color).child(hint)
                }
            }),
            (None, Some((taking, blockers))) if *taking == row => {
                let (text, can) = conflicts::blocked_text(blockers);
                let blockers = blockers.clone();
                let cancel = small_button("key-take-cancel", "Cancel", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    this.keyboard.taking = None;
                    cx.notify();
                }));
                let take = can.then(|| {
                    small_button("key-take-back", "Take back", true)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.take_back(row, blockers.clone(), cx)))
                });
                Some(note_box(Level::Clash, rich(&text), [Some(cancel), take].into_iter().flatten().collect()))
            }
            _ => None,
        };
        div().flex().flex_col().child(line).children(below)
    }

    fn pending_note(&self, p: &Pending, cx: &mut Context<Self>) -> Div {
        let Some(first) = p.notes.first() else { return div() };
        let cancel =
            |label| small_button("key-note-cancel", label, false).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.resolve(false, window, cx)));
        let take = |label| small_button("key-note-take", label, true).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.resolve(true, window, cx)));
        let actions = match first.level {
            Level::Refuse => vec![cancel("Pick another")],
            Level::Clash if first.other.is_some() => vec![cancel("Cancel"), take("Replace")],
            Level::Clash => vec![cancel("Cancel")],
            Level::Warn => vec![cancel("Cancel"), take("Use anyway")],
            Level::Shared => vec![cancel("Cancel"), take("Keep both")],
        };
        let more =
            p.notes[1..].iter().filter(|n| n.level != Level::Refuse).map(|n| div().mt(px(3.)).text_size(px(12.)).text_color(TEXT_2).child(rich(&n.text)));
        note_box(first.level, div().flex().flex_col().child(rich(&first.text)).children(more), actions)
    }

    fn keyboard_toolbar(&self, rows: (usize, usize, usize), cx: &mut Context<Self>) -> Div {
        let finding = self.keyboard.find.is_some();
        let find = focus_ring(
            div()
                .id("keys-find")
                .h(px(22.))
                .px(px(7.))
                .flex()
                .items_center()
                .gap(px(5.))
                .rounded(px(6.))
                .cursor_pointer()
                .tab_index(0)
                .text_size(px(12.))
                .text_color(if finding { TEXT } else { TEXT_3 })
                .when(finding, |d| d.bg(SURFACE).shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]))
                .hover(|d| d.bg(SURFACE).text_color(TEXT))
                .child(icon("keyboard", 13., TEXT_3))
                .child("Find by keys")
                .tooltip(|window, cx| Tooltip::new("Press keys to see what they do").build(window, cx))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                    let find = if this.keyboard.find.is_some() { None } else { Some(None) };
                    this.keyboard.stop();
                    this.keyboard.find = find;
                    cx.notify();
                })),
        );
        let search = div()
            .h(px(34.))
            .pl(px(11.))
            .pr(px(5.))
            .flex()
            .items_center()
            .gap(px(8.))
            .rounded(px(8.))
            .bg(FILL_2)
            .child(icon("search", 14., TEXT_3))
            .child(div().flex_1().min_w_0().child(Input::new(&self.keyboard.search).appearance(false).p_0().text_size(px(13.))))
            .child(find);
        let (all, changed, unassigned) = rows;
        let filters = [Filter::All, Filter::Changed, Filter::Unassigned];
        let labels = vec![format!("All {all}").into(), format!("Changed {changed}").into(), format!("Unassigned {unassigned}").into()];
        let chosen = filters.iter().position(|f| *f == self.keyboard.filter).unwrap_or(0);
        let filter = super::segmented_control(
            "keys-filter",
            labels,
            chosen,
            move |this, i, cx| {
                this.keyboard.filter = filters[i];
                cx.notify();
            },
            cx,
        );
        let reset = super::action("keys-reset-all", "")
            .gap(px(6.))
            .child(icon("discard", 12., TEXT_2))
            .child("Reset all…")
            .when(changed == 0, |d| d.opacity(0.5))
            .when(changed > 0, |d| d.on_click(cx.listener(|_, _: &ClickEvent, _, cx| cx.emit(SettingsEvent::Ask(Confirm::ResetSection(Section::Keyboard))))));
        let controls = div().flex().items_center().justify_between().gap(px(8.)).child(filter).child(reset);
        div().flex().flex_col().gap(px(10.)).child(search).child(controls)
    }

    fn find_bar(&self, cx: &mut Context<Self>) -> Option<Div> {
        let find = self.keyboard.find.as_ref()?;
        let text: AnyElement = match find {
            None => "Press the keys you want to look up · Esc cancels".into_any_element(),
            Some(key) => div().flex().items_center().gap(px(6.)).child("Showing what").child(keycaps(&shown(key))).child("does").into_any_element(),
        };
        let clear = small_button("keys-find-clear", "Clear", false).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.keyboard.find = None;
            cx.notify();
        }));
        Some(
            div()
                .px(px(12.))
                .py(px(8.))
                .flex()
                .items_center()
                .gap(px(10.))
                .rounded(px(9.))
                .bg(ACCENT_TINT)
                .text_size(px(12.5))
                .text_color(TEXT)
                .child(div().flex_1().child(text))
                .child(clear),
        )
    }

    pub(crate) fn keybinding_settings(&self, cx: &mut Context<Self>) -> Div {
        let c = self.custom();
        let words = super::catalog::words(&self.keyboard.search.read(cx).value());
        let found = self.keyboard.find.clone().flatten();
        let filter = self.keyboard.filter;
        let counts =
            (SHORTCUTS.len(), model::changed_rows(&SHORTCUTS, &c).len(), SHORTCUTS.iter().filter(|r| r.set.is_none() && r.keys(&c).is_empty()).count());
        let listed: Vec<usize> = find(&SHORTCUTS, &c, &words)
            .into_iter()
            .filter_map(|r| SHORTCUTS.iter().position(|x| std::ptr::eq(x, r)))
            .filter(|&i| {
                let r = &SHORTCUTS[i];
                let keys = r.keys(&c);
                let by_keys = found.as_ref().is_none_or(|k| keys.contains(k));
                let by_filter = match filter {
                    Filter::All => true,
                    Filter::Changed => r.changed(&c),
                    Filter::Unassigned => r.set.is_none() && keys.is_empty(),
                };
                by_keys && by_filter
            })
            .collect();
        let mut groups: Vec<Div> = TOPICS
            .iter()
            .filter_map(|&(topic, title)| {
                let rows: Vec<Div> = listed.iter().filter(|&&i| SHORTCUTS[i].topic == topic).map(|&i| self.shortcut_line(i, cx)).collect();
                (!rows.is_empty()).then(|| group(title, rows))
            })
            .collect();
        if let Some(key) = &found {
            let elsewhere = conflicts::elsewhere(&SHORTCUTS, &c, key);
            if !elsewhere.is_empty() {
                let rows = elsewhere
                    .into_iter()
                    .map(|(label, place)| {
                        let title = div().flex().items_center().gap(px(8.)).child(label).child(ui::tag(place));
                        shortcut_row(title, div().flex().items_center().gap(px(6.)).child(icon("lock", 12., TEXT_4)).child(keycaps(&shown(key))))
                    })
                    .collect();
                groups.push(group("Built in elsewhere · can\u{2019}t change here", rows));
            }
        }
        let empty = groups.is_empty().then(|| {
            let text = match (&found, filter) {
                (Some(key), _) => format!("Nothing uses {} yet.", compact(key)),
                (None, Filter::Changed) if words.is_empty() => "No shortcut is changed yet.".to_string(),
                (None, Filter::Unassigned) if words.is_empty() => "Every action has a shortcut.".to_string(),
                _ => "No shortcut matches.".to_string(),
            };
            div().py(px(24.)).flex().justify_center().text_size(px(13.)).text_color(TEXT_3).child(text)
        });
        let open = self.fixed_open;
        let fixed_title = div()
            .id("settings-fixed-keys")
            .mb(px(7.))
            .ml(px(2.))
            .flex()
            .items_center()
            .gap(px(4.))
            .cursor_pointer()
            .text_size(px(13.))
            .line_height(px(18.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT_2)
            .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_3))
            .child("Fixed keys")
            .child(div().font_weight(FontWeight::NORMAL).text_color(TEXT_3).child("· handled before the keymap, so they can\u{2019}t change"))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.fixed_open = !this.fixed_open;
                cx.notify();
            }));
        let fixed = open.then(|| super::card(FIXED.iter().map(|&(label, keys)| shortcut_row(label, fixed_keys(keys))).collect()));
        let fixed =
            (filter == Filter::All && words.is_empty() && self.keyboard.find.is_none()).then(|| div().flex().flex_col().child(fixed_title).children(fixed));
        div()
            .flex()
            .flex_col()
            .gap(px(22.))
            .child(self.keyboard_toolbar(counts, cx))
            .children(self.find_bar(cx))
            .children(groups)
            .children(empty)
            .children(fixed)
    }
}

/// A set's keys as caps: its modifiers, then "1…9".
fn set_caps(mods: &str, keys: &str) -> Div {
    let mut caps: Vec<String> = shown(&model::canon(&actions::native(&format!("{mods}-x"), None))).split_whitespace().map(String::from).collect();
    caps.pop();
    caps.push(keys.to_string());
    keycaps(&caps.join(" "))
}

/// Fixed keys are glyphs already, so they skip `shown`.
fn fixed_keys(keys: &[&str]) -> Stateful<Div> {
    div()
        .id(SharedString::from(format!("fixed-{}", keys.join("-"))))
        .flex()
        .flex_none()
        .items_center()
        .gap(px(6.))
        .text_size(px(12.))
        .text_color(TEXT_3)
        .child(icon("lock", 12., TEXT_4))
        .children(keys.iter().enumerate().flat_map(|(i, k)| {
            let sep = (i > 0).then(|| div().child("or").into_any_element());
            sep.into_iter().chain([keycaps(k).into_any_element()])
        }))
        .tooltip(|window, cx| Tooltip::new("Fixed: handled before the keymap, so it can\u{2019}t change").build(window, cx))
}

#[cfg(test)]
mod tests {
    use super::model::{Custom, SHORTCUTS, Slot, canon, put};
    use super::{find, glyphs_on, refused, shown, takes};
    use crate::actions::native;
    use crate::settings::catalog::words;
    use gpui_kit::Keystroke;

    #[test]
    fn shortcuts_print_as_macos_glyphs() {
        let g = |k: &str| glyphs_on(&Keystroke::parse_with_os(k, keys::OS).unwrap(), true);
        assert_eq!([g("cmd-shift-j"), g("cmd-alt-left"), g("ctrl-shift-tab"), g("cmd-,")], ["⇧ ⌘ J", "⌥ ⌘ ←", "⌃ ⇧ ⇥", "⌘ ,"]);
    }

    #[test]
    fn shortcuts_off_macos_print_their_modifiers_by_name() {
        let g = |k: &str| glyphs_on(&Keystroke::parse_with_os(k, keys::OS).unwrap(), false);
        assert_eq!(
            [g("ctrl-shift-j"), g("ctrl-alt-left"), g("ctrl-shift-tab"), g("ctrl-enter")],
            ["Ctrl Shift J", "Ctrl Alt ←", "Ctrl Shift Tab", "Ctrl Enter"]
        );
    }

    #[test]
    fn search_finds_shortcuts_by_action_group_place_or_keys() {
        let mut c = Custom::default();
        let split = SHORTCUTS.iter().find(|r| r.label == "Split down").unwrap();
        put(&mut c, split, Slot::Spot, "cmd-shift-y");
        let labels = |q: &str| find(&SHORTCUTS, &c, &words(q)).iter().map(|r| r.label).collect::<Vec<_>>();
        assert!(labels("split").contains(&"Split right"));
        assert!(labels("panels").contains(&"Zoom pane"));
        assert!(labels("browser reload").contains(&"Reload"));
        assert_eq!(labels(&shown(&native("cmd-k", None)).replace(' ', "")), ["Command palette"]);
        assert_eq!(labels("cmd-shift-y"), ["Split down"]);
        assert!(labels("zzz").is_empty());
    }

    #[test]
    fn search_finds_an_added_key() {
        let mut c = Custom::default();
        let activity = SHORTCUTS.iter().find(|r| r.label == "Go to Activity").unwrap();
        put(&mut c, activity, Slot::New, &canon("cmd-shift-y"));
        assert_eq!(find(&SHORTCUTS, &c, &words("cmd-shift-y")).iter().map(|r| r.label).collect::<Vec<_>>(), ["Go to Activity"]);
    }

    #[test]
    fn only_keys_with_a_modifier_or_a_function_key_become_shortcuts() {
        let t = |k: &str| takes(&Keystroke::parse_with_os(k, keys::OS).unwrap());
        assert_eq!([t("cmd-k"), t("ctrl-1"), t("alt-x"), t("f5"), t("k"), t("shift-k"), t("f")], [true, true, true, true, false, false, false]);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn a_plain_letter_is_refused_with_what_to_add() {
        assert_eq!(refused(&Keystroke::parse("g").unwrap()), "G alone types text. Add ⌘, ⌃ or ⌥.");
    }
}
