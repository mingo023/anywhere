use super::catalog::Setting;
use super::{Section, card, group, row};
use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay, Screen};
use crate::{actions, browser, inbox, settings};
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::BTreeMap;
use theme::*;
use ui::Segment;

/// Only here so Reset puts every shortcut back; the page draws its own rows.
pub(super) const ROWS: &[Setting] = &[Setting::custom("shortcuts", "Shortcuts", "Changed shortcuts", |d, _, _| row("Changed shortcuts", None, div().text_size(px(12.5)).text_color(TEXT_2).child(changed(&d.store))))
    .resets(changed, |s, _| s.keys.clear())
    .effect(|d, cx| actions::rebind(&d.store.keys, cx))];

fn changed(s: &store::Store) -> String {
    match s.keys.len() {
        0 => "None".into(),
        1 => "1 shortcut".into(),
        n => format!("{n} shortcuts"),
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Topic {
    Sessions,
    Navigation,
    Panels,
    Browser,
    App,
}

const TOPICS: [(Topic, &str); 5] = [(Topic::Sessions, "Sessions"), (Topic::Navigation, "Navigation"), (Topic::Panels, "Panels & tabs"), (Topic::Browser, "Browser"), (Topic::App, "App")];

/// Keys handled outside the keymap, so no binding lists them.
const FIXED: [(&str, &[&str]); 5] = [
    ("Close a menu, dialog or Settings", &["Esc"]),
    ("Move in the palette", &["↑", "↓"]),
    ("Open from the palette", &["↩"]),
    ("Next in Inbox", &["J", "↓"]),
    ("Previous in Inbox", &["K", "↑"]),
];

/// What an action does and the group listing it; `None` leaves it out.
fn label(action: &str) -> Option<(&'static str, Topic)> {
    use Topic::*;
    Some(match action.strip_prefix("desktop::")? {
        "StartSession" => ("New session", Sessions),
        "NewWorktree" => ("New worktree", Sessions),
        "OpenSession" => ("Open session", Sessions),
        "NextNeedsYou" => ("Next Needs you", Sessions),
        "GoToUpNext" => ("Go to Up next", Sessions),
        "NextSession" => ("Next session", Sessions),
        "PrevSession" => ("Previous session", Sessions),
        "JumpTo" => ("Jump to session", Sessions),
        "AddToChat" => ("Add selection to chat", Sessions),
        "OpenPalette" => ("Command palette", Navigation),
        "GoToFile" => ("Go to file", Navigation),
        "OpenAutomations" => ("Go to Automations", Navigation),
        "ToggleSidebar" => ("Toggle sidebar", Navigation),
        "ToggleRail" => ("Toggle rail", Navigation),
        "ToggleFocus" => ("Focus mode", Navigation),
        "NewTab" => ("New tab", Panels),
        "CloseTab" => ("Close tab", Panels),
        "PrevTab" => ("Previous tab", Panels),
        "NextTab" => ("Next tab", Panels),
        "JumpToTab" => ("Jump to tab", Panels),
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
        "FocusSearch" => ("Search settings", App),
        "Save" => ("Save", App),
        "Quit" => ("Quit", App),
        "CopySelection" => ("Copy", App),
        "SelectAll" => ("Select all", App),
        "Paste" => ("Paste", App),
        _ => return None,
    })
}

/// Where `b` works: `Some(None)` everywhere, `Some(Some(place))` in a terminal, browser or the Inbox, `None` somewhere the list leaves out.
fn place(b: &KeyBinding) -> Option<Option<&'static str>> {
    match b.predicate().map(|p| p.to_string()).as_deref() {
        None => Some(None),
        Some(keys::CONTEXT) => Some(Some("Terminal")),
        Some(browser::CONTEXT) => Some(Some("Browser")),
        Some(inbox::CONTEXT) => Some(Some("Inbox")),
        Some(settings::CONTEXT) => Some(Some("Settings")),
        Some(_) => None,
    }
}

/// `k` as macOS menus print it, one glyph per key: "⌘ ⇧ J".
fn glyphs(k: &Keystroke) -> String {
    let m = k.modifiers;
    let key = match k.key.as_str() {
        "enter" => "↩".to_string(),
        "tab" => "⇥".to_string(),
        "left" => "←".to_string(),
        "right" => "→".to_string(),
        "up" => "↑".to_string(),
        "down" => "↓".to_string(),
        "backspace" => "⌫".to_string(),
        "space" => "Space".to_string(),
        key => key.to_uppercase(),
    };
    [(m.control, "⌃"), (m.alt, "⌥"), (m.shift, "⇧"), (m.platform, "⌘")].into_iter().filter(|(on, _)| *on).map(|(_, g)| g.to_string()).chain([key]).collect::<Vec<_>>().join(" ")
}

/// Keys as `Store.keys` holds them ("cmd-shift-j"), in glyphs.
fn shown(keys: &str) -> String {
    keys.split_whitespace().map(|k| Keystroke::parse(k).map_or_else(|_| k.to_string(), |k| glyphs(&k))).collect::<Vec<_>>().join("  ")
}

#[derive(Debug)]
struct Shortcut {
    /// The binding's id in `Store.keys`; one row can only be changed when it has a single binding.
    id: String,
    label: &'static str,
    topic: Topic,
    place: Option<&'static str>,
    /// The default keys of each binding, as `Store.keys` holds them.
    keys: Vec<String>,
}

impl Shortcut {
    fn editable(&self) -> bool {
        self.keys.len() == 1
    }

    /// The keys it has now: the ones it was changed to, else its defaults.
    fn keys_in<'a>(&'a self, custom: &'a BTreeMap<String, String>) -> Vec<&'a str> {
        match custom.get(&self.id) {
            Some(k) if k.is_empty() => Vec::new(),
            Some(k) => vec![k.as_str()],
            None => self.keys.iter().map(String::as_str).collect(),
        }
    }
}

/// Each labelled action, in binding order, with every shortcut it has in each place.
fn shortcut_rows(bindings: &[KeyBinding]) -> Vec<Shortcut> {
    let mut rows: Vec<Shortcut> = Vec::new();
    for b in bindings {
        let (Some(place), Some((label, topic))) = (place(b), label(b.action().name())) else { continue };
        let keys = b.keystrokes().iter().map(|k| k.inner().unparse()).collect::<Vec<_>>().join(" ");
        match rows.iter_mut().find(|r| r.label == label && r.place == place) {
            Some(r) => r.keys.push(keys),
            None => rows.push(Shortcut { id: actions::binding_id(b), label, topic, place, keys: vec![keys] }),
        }
    }
    rows
}

/// Built once: render runs on every poll, and parsing the keymap each time is wasted.
static SHORTCUTS: std::sync::LazyLock<Vec<Shortcut>> = std::sync::LazyLock::new(|| shortcut_rows(&actions::bindings()));

/// Shortcuts whose action, group, place or keys hold every word.
fn find<'a>(rows: &'a [Shortcut], custom: &BTreeMap<String, String>, words: &[String]) -> Vec<&'a Shortcut> {
    rows.iter()
        .filter(|r| {
            let topic = TOPICS.iter().find(|(t, _)| *t == r.topic).map_or("", |(_, title)| title);
            let keys = r.keys_in(custom);
            let glyphs: Vec<String> = keys.iter().map(|k| shown(k).replace(' ', "")).collect();
            let fields: Vec<&str> = [r.label, topic, r.place.unwrap_or_default()].into_iter().chain(keys).chain(glyphs.iter().map(String::as_str)).collect();
            crate::palette::matches(words, &fields)
        })
        .collect()
}

/// The other row already on `keys` where `row` works: the same place, or everywhere.
fn clash<'a>(rows: &'a [Shortcut], custom: &BTreeMap<String, String>, row: &Shortcut, keys: &str) -> Option<&'a Shortcut> {
    rows.iter().find(|r| r.id != row.id && (r.place.is_none() || row.place.is_none() || r.place == row.place) && r.keys_in(custom).contains(&keys))
}

/// Puts `row` on `keys`, forgetting the change when they're its defaults.
fn set_keys(custom: &mut BTreeMap<String, String>, row: &Shortcut, keys: &str) {
    if row.keys == [keys] {
        custom.remove(&row.id);
    } else {
        custom.insert(row.id.clone(), keys.to_string());
    }
}

/// Moves the clashing keys to `row`, leaving the row that had them with none. A row with several keys keeps them all, since clearing it would drop the rest.
fn replace(custom: &mut BTreeMap<String, String>, row: &Shortcut, keys: &str) -> bool {
    if let Some(other) = clash(&SHORTCUTS, custom, row, keys) {
        if !other.editable() {
            return false;
        }
        custom.insert(other.id.clone(), String::new());
    }
    set_keys(custom, row, keys);
    true
}

/// Whether `k` can be a shortcut: it holds ⌘, ⌃ or ⌥, or is a function key, so typing never triggers one.
fn takes(k: &Keystroke) -> bool {
    let m = k.modifiers;
    m.platform || m.control || m.alt || k.key.strip_prefix('f').is_some_and(|n| n.parse::<u8>().is_ok())
}

/// Each matching shortcut's action, and its place and keys as the Keyboard page shows them, for Settings search.
pub(super) fn search(words: &[String], custom: &BTreeMap<String, String>) -> Vec<(&'static str, Div)> {
    find(&SHORTCUTS, custom, words).into_iter().map(|r| (r.label, div().flex().items_center().gap(px(8.)).children(r.place.map(ui::tag)).child(shortcuts(&r.keys_in(custom))))).collect()
}

/// The Keyboard page's filter, and the row taking new keys.
pub(crate) struct Keyboard {
    /// The row being changed, and keys pressed for it that another row already has.
    pub(crate) recording: Option<(String, Option<String>)>,
    customized: bool,
    search: Entity<InputState>,
}

impl Keyboard {
    pub(super) fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let search = cx.new(|cx| InputState::new(window, cx).placeholder("Search by name or keys"));
        let desktop = cx.entity().downgrade();
        let subs = vec![
            cx.subscribe(&search, |_, _, ev: &InputEvent, cx| {
                if let InputEvent::Change = ev {
                    cx.notify();
                }
            }),
            // Ahead of the keymap, so ⌘K becomes the new keys instead of opening the palette.
            cx.intercept_keystrokes(move |ev, _, cx| {
                if desktop.update(cx, |d, cx| d.record_shortcut(&ev.keystroke, cx)).unwrap_or(false) {
                    cx.stop_propagation();
                }
            }),
        ];
        (Self { recording: None, customized: false, search }, subs)
    }

    pub(super) fn stop(&mut self) {
        self.recording = None;
    }
}

fn shortcuts<S: AsRef<str>>(keys: &[S]) -> Div {
    // Some rows have nine; the first and last say it.
    let keys: Vec<String> = keys.iter().map(|k| shown(k.as_ref())).collect();
    let shown: Vec<&String> = if keys.len() > 4 { vec![&keys[0], &keys[keys.len() - 1]] } else { keys.iter().collect() };
    let sep = if keys.len() > 4 { "–" } else { "or" };
    div().flex().flex_none().items_center().gap(px(6.)).text_size(px(12.)).text_color(TEXT_3).when(keys.is_empty(), |d| d.child("None")).children(shown.into_iter().enumerate().flat_map(|(i, k)| {
        let sep = (i > 0).then(|| div().child(sep).into_any_element());
        sep.into_iter().chain([keycaps(k).into_any_element()])
    }))
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
    div().min_h(px(40.)).px(px(14.)).py(px(7.)).flex().items_center().gap(px(10.)).child(div().flex_1().flex().items_center().gap(px(8.)).text_size(px(13.)).text_color(TEXT).child(label)).child(control)
}

impl Desktop {
    /// Takes `k` as the new keys of the row being changed; true when it was for that row.
    fn record_shortcut(&mut self, k: &Keystroke, cx: &mut Context<Self>) -> bool {
        let Some((id, _)) = self.settings.keyboard.recording.clone() else { return false };
        if self.screen != Screen::Settings || self.settings.section != Section::Keyboard {
            self.settings.keyboard.stop();
            return false;
        }
        if k.key == "escape" {
            self.settings.keyboard.stop();
        } else if takes(k) {
            let keys = k.unparse();
            let Some(row) = SHORTCUTS.iter().find(|r| r.id == id) else { return false };
            if clash(&SHORTCUTS, &self.store.keys, row, &keys).is_some() {
                self.settings.keyboard.recording = Some((id, Some(keys)));
            } else {
                set_keys(&mut self.store.keys, row, &keys);
                self.settings.keyboard.stop();
                self.shortcuts_changed(cx);
            }
        }
        cx.notify();
        true
    }

    fn replace_shortcut(&mut self, cx: &mut Context<Self>) {
        let Some((id, Some(keys))) = self.settings.keyboard.recording.take() else { return };
        let Some(row) = SHORTCUTS.iter().find(|r| r.id == id) else { return };
        if replace(&mut self.store.keys, row, &keys) {
            self.shortcuts_changed(cx);
        }
    }

    fn shortcuts_changed(&mut self, cx: &mut Context<Self>) {
        actions::rebind(&self.store.keys, cx);
        self.save_soon(cx);
        cx.notify();
    }

    fn shortcut_line(&self, r: &'static Shortcut, cx: &mut Context<Self>) -> Div {
        let custom = &self.store.keys;
        let changed = custom.contains_key(&r.id);
        let recording = self.settings.keyboard.recording.as_ref().filter(|(id, _)| *id == r.id).map(|(_, pressed)| pressed.clone());
        let title = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .when(changed, |d| d.font_weight(FontWeight::SEMIBOLD).child(div().size(px(5.)).rounded_full().bg(TEXT)))
            .child(r.label);
        let keys = r.keys_in(custom);
        let control = match &recording {
            Some(pressed) => div()
                .flex()
                .items_center()
                .gap(px(10.))
                .child(div().text_size(px(12.)).text_color(TEXT_3).child("Press a shortcut · Esc cancels"))
                .child(
                    div()
                        .min_w(px(72.))
                        .h(px(28.))
                        .px(px(6.))
                        .flex()
                        .items_center()
                        .justify_center()
                        .rounded(px(7.))
                        .border_1()
                        .border_color(if pressed.is_some() { FAILED } else { ACCENT })
                        .children(pressed.as_deref().map(|k| keycaps(&shown(k)))),
                ),
            None => {
                let id = r.id.clone();
                let reset = changed.then(|| {
                    let id = id.clone();
                    div()
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .child(div().text_size(px(12.)).text_color(TEXT_4).child(format!("Default {}", shown(&r.keys[0]).replace(' ', ""))))
                        .child(ui::icon_button_sized(SharedString::from(format!("key-reset-{id}")), "discard", 22., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.store.keys.remove(&id);
                            this.shortcuts_changed(cx);
                        })))
                });
                let keys = div().id(SharedString::from(format!("key-{id}"))).rounded(px(6.)).cursor_pointer().hover(|d| d.bg(FILL_2)).child(shortcuts(&keys)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.settings.keyboard.recording = Some((id.clone(), None));
                    cx.notify();
                }));
                div().flex().items_center().gap(px(8.)).children(reset).children(r.place.map(ui::tag)).child(keys)
            }
        };
        let line = shortcut_row(title, control);
        let other = recording.flatten().and_then(|pressed| clash(&SHORTCUTS, custom, r, &pressed).map(|o| (shown(&pressed).replace(' ', ""), o)));
        let Some((pressed, other)) = other else { return line };
        let cancel = ui::button("key-cancel", ui::Variant::Ghost, None, "Cancel").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.settings.keyboard.stop();
            cx.notify();
        }));
        let replace = ui::button("key-replace", ui::Variant::Primary, None, "Replace").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.replace_shortcut(cx)));
        let banner = div()
            .mx(px(10.))
            .mb(px(10.))
            .px(px(12.))
            .py(px(9.))
            .flex()
            .items_center()
            .gap(px(10.))
            .rounded(px(8.))
            .bg(FAILED_BG)
            .child(icon("warning", 14., FAILED))
            .child(div().flex_1().min_w_0().text_size(px(12.5)).line_height(px(17.)).text_color(TEXT).child(clash_text(&pressed, other.label, other.editable())))
            .child(div().flex_none().flex().gap(px(6.)).child(cancel).when(other.editable(), |d| d.child(replace)));
        div().flex().flex_col().child(line).child(banner)
    }

    fn keyboard_toolbar(&self, cx: &mut Context<Self>) -> Div {
        let n = self.store.keys.len();
        let search = div()
            .flex_1()
            .h(px(28.))
            .px(px(9.))
            .flex()
            .items_center()
            .gap(px(6.))
            .rounded(px(7.))
            .bg(FILL_2)
            .child(icon("search", 13., TEXT_3))
            .child(div().flex_1().min_w_0().child(Input::new(&self.settings.keyboard.search).appearance(false).p_0().text_size(px(12.5))));
        let filter = ui::segmented(
            vec![Segment { icon: None, value: false, label: "All".into(), badge: None }, Segment { icon: None, value: true, label: format!("Customized · {n}").into(), badge: None }],
            self.settings.keyboard.customized,
            true,
            false,
            |this: &mut Self, on, cx| {
                this.settings.keyboard.customized = on;
                cx.notify();
            },
            cx,
        );
        let reset = super::action("keys-reset-all", "").gap(px(6.)).child(icon("discard", 12., TEXT_2)).child("Reset all…").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            (this.confirm, this.overlay) = (Some(Confirm::ResetSection(Section::Keyboard)), Some(Overlay::Confirm));
            cx.notify();
        }));
        div().flex().items_center().gap(px(8.)).child(search).child(filter).child(reset)
    }

    pub(crate) fn keybinding_settings(&self, cx: &mut Context<Self>) -> Div {
        let words = super::catalog::words(&self.settings.keyboard.search.read(cx).value());
        let customized = self.settings.keyboard.customized;
        let shown: Vec<&'static Shortcut> = find(&SHORTCUTS, &self.store.keys, &words).into_iter().filter(|r| !customized || self.store.keys.contains_key(&r.id)).collect();
        let groups: Vec<Div> = TOPICS
            .iter()
            .filter_map(|&(topic, title)| {
                let rows: Vec<Div> = shown.iter().filter(|r| r.topic == topic).map(|r| if r.editable() { self.shortcut_line(r, cx) } else { shortcut_row(r.label, div().flex().items_center().gap(px(8.)).children(r.place.map(ui::tag)).child(shortcuts(&r.keys))) }).collect();
                (!rows.is_empty()).then(|| group(title, rows))
            })
            .collect();
        let empty = groups.is_empty().then(|| div().py(px(24.)).flex().justify_center().text_size(px(13.)).text_color(TEXT_3).child(if customized && words.is_empty() { "No shortcut is changed yet." } else { "No shortcut matches." }));
        let open = self.settings.fixed_open;
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
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.settings.fixed_open = !this.settings.fixed_open;
                cx.notify();
            }));
        let fixed = open.then(|| card(FIXED.iter().map(|&(label, keys)| shortcut_row(label, fixed_keys(keys))).collect()));
        let fixed = (!customized && words.is_empty()).then(|| div().flex().flex_col().child(fixed_title).children(fixed));
        div().flex().flex_col().gap(px(22.)).child(self.keyboard_toolbar(cx)).children(groups).children(empty).children(fixed)
    }
}

fn clash_text(pressed: &str, other: &str, replaceable: bool) -> StyledText {
    let lead = format!("{pressed} is already used by ");
    let bold = HighlightStyle { font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() };
    let ask = if replaceable { "Replace it there, or pick another shortcut." } else { "Pick another shortcut." };
    StyledText::new(format!("{lead}{other}. {ask}")).with_highlights([(lead.len()..lead.len() + other.len(), bold)])
}

/// Fixed keys are glyphs already, so they skip `shown`.
fn fixed_keys(keys: &[&str]) -> Div {
    div().flex().flex_none().items_center().gap(px(6.)).text_size(px(12.)).text_color(TEXT_3).children(keys.iter().enumerate().flat_map(|(i, k)| {
        let sep = (i > 0).then(|| div().child("or").into_any_element());
        sep.into_iter().chain([keycaps(k).into_any_element()])
    }))
}

#[cfg(test)]
mod tests {
    use super::{SHORTCUTS, Topic, clash, find, glyphs, label, place, replace, set_keys, shortcut_rows, takes};
    use crate::actions::bindings;
    use crate::settings::catalog::words;
    use gpui_kit::Keystroke;
    use std::collections::BTreeMap;

    fn row(label: &str) -> &'static super::Shortcut {
        SHORTCUTS.iter().find(|r| r.label == label).unwrap()
    }

    #[test]
    fn every_binding_appears_in_the_shortcut_table() {
        // Copy, paste and undo inside a web page are the page's own text editing.
        for b in bindings().iter().filter(|b| place(b).is_some() && b.action().name() != "desktop::PageEdit") {
            assert!(label(b.action().name()).is_some(), "{} has no label", b.action().name());
        }
    }

    #[test]
    fn shortcuts_print_as_macos_glyphs() {
        let g = |k: &str| glyphs(&Keystroke::parse(k).unwrap());
        assert_eq!([g("cmd-shift-j"), g("cmd-alt-left"), g("ctrl-shift-tab"), g("cmd-,")], ["⇧ ⌘ J", "⌥ ⌘ ←", "⌃ ⇧ ⇥", "⌘ ,"]);
    }

    #[test]
    fn an_action_bound_to_several_shortcuts_is_one_row_that_cant_be_changed() {
        let rows = shortcut_rows(&bindings());
        let focus: Vec<_> = rows.iter().filter(|r| r.label == "Focus pane").collect();
        assert_eq!((focus.len(), focus[0].keys.len(), focus[0].topic, focus[0].editable()), (1, 4, Topic::Panels, false));
        assert!(row("Split right").editable());
    }

    #[test]
    fn terminal_browser_and_inbox_shortcuts_are_marked_with_where_they_work() {
        let rows = shortcut_rows(&bindings());
        let at = |l: &str| rows.iter().find(|r| r.label == l).unwrap().place;
        assert_eq!([at("Copy"), at("Reload"), at("Open session"), at("Settings")], [Some("Terminal"), Some("Browser"), Some("Inbox"), None]);
    }

    #[test]
    fn search_finds_shortcuts_by_action_group_place_or_keys() {
        let custom = BTreeMap::from([("desktop::SplitDown".to_string(), "cmd-shift-y".to_string())]);
        let labels = |q: &str| find(&SHORTCUTS, &custom, &words(q)).iter().map(|r| r.label).collect::<Vec<_>>();
        assert!(labels("split").contains(&"Split right"));
        assert!(labels("panels").contains(&"Zoom pane"));
        assert!(labels("browser reload").contains(&"Reload"));
        assert_eq!(labels("⌘K"), ["Command palette"]);
        assert_eq!(labels("cmd-shift-y"), ["Split down"]);
        assert!(labels("zzz").is_empty());
    }

    #[test]
    fn a_shortcut_set_back_to_its_default_is_no_longer_changed() {
        let mut custom = BTreeMap::new();
        set_keys(&mut custom, row("Split right"), "cmd-shift-y");
        assert_eq!(row("Split right").keys_in(&custom), ["cmd-shift-y"]);
        set_keys(&mut custom, row("Split right"), "cmd-d");
        assert!(custom.is_empty());
    }

    #[test]
    fn keys_clash_with_a_shortcut_that_works_in_the_same_place_or_everywhere() {
        let none = BTreeMap::new();
        let other = |r: &str, keys: &str| clash(&SHORTCUTS, &none, row(r), keys).map(|o| o.label);
        assert_eq!(other("Split right", "cmd-k"), Some("Command palette"));
        assert_eq!(other("Reload", "cmd-k"), Some("Command palette"));
        assert_eq!(other("Reload", "cmd-c"), None);
        assert_eq!(other("Split right", "cmd-d"), None);
        let moved = BTreeMap::from([("desktop::OpenPalette".to_string(), String::new())]);
        assert_eq!(clash(&SHORTCUTS, &moved, row("Split right"), "cmd-k").map(|o| o.label), None);
    }

    #[test]
    fn replacing_takes_the_keys_from_a_single_key_shortcut_but_not_from_one_with_several() {
        let mut custom = BTreeMap::new();
        assert!(replace(&mut custom, row("Split right"), "cmd-k"));
        assert_eq!([row("Split right").keys_in(&custom), row("Command palette").keys_in(&custom)], [vec!["cmd-k"], vec![]]);
        let mut custom = BTreeMap::new();
        assert!(!replace(&mut custom, row("Split right"), "cmd-1"));
        assert!(custom.is_empty());
    }

    #[test]
    fn only_keys_with_a_modifier_or_a_function_key_become_shortcuts() {
        let t = |k: &str| takes(&Keystroke::parse(k).unwrap());
        assert_eq!([t("cmd-k"), t("ctrl-1"), t("alt-x"), t("f5"), t("k"), t("shift-k"), t("f")], [true, true, true, true, false, false, false]);
    }
}
