use super::{card, group, row};
use crate::desktop::Desktop;
use crate::{actions, browser, inbox};
use gpui_kit::*;
use theme::*;

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
        key => key.to_uppercase(),
    };
    [(m.control, "⌃"), (m.alt, "⌥"), (m.shift, "⇧"), (m.platform, "⌘")].into_iter().filter(|(on, _)| *on).map(|(_, g)| g.to_string()).chain([key]).collect::<Vec<_>>().join(" ")
}

#[derive(Debug)]
struct Shortcut {
    label: &'static str,
    topic: Topic,
    place: Option<&'static str>,
    keys: Vec<String>,
}

/// Each labelled action, in binding order, with every shortcut it has in each place.
fn shortcut_rows(bindings: &[KeyBinding]) -> Vec<Shortcut> {
    let mut rows: Vec<Shortcut> = Vec::new();
    for b in bindings {
        let (Some(place), Some((label, topic))) = (place(b), label(b.action().name())) else { continue };
        let keys = b.keystrokes().iter().map(|k| glyphs(k.inner())).collect::<Vec<_>>().join("  ");
        match rows.iter_mut().find(|r| r.label == label && r.place == place) {
            Some(r) => r.keys.push(keys),
            None => rows.push(Shortcut { label, topic, place, keys: vec![keys] }),
        }
    }
    rows
}

/// Built once: render runs on every poll, and parsing the keymap each time is wasted.
static SHORTCUTS: std::sync::LazyLock<Vec<Shortcut>> = std::sync::LazyLock::new(|| shortcut_rows(&actions::bindings()));

fn shortcuts(keys: &[String]) -> Div {
    // Some rows have nine; the first and last say it.
    let shown: Vec<&String> = if keys.len() > 4 { vec![&keys[0], &keys[keys.len() - 1]] } else { keys.iter().collect() };
    let sep = if keys.len() > 4 { "–" } else { "or" };
    div().flex().flex_none().items_center().gap(px(6.)).text_size(px(12.)).text_color(TEXT_3).children(shown.into_iter().enumerate().flat_map(|(i, k)| {
        let sep = (i > 0).then(|| div().child(sep).into_any_element());
        sep.into_iter().chain([ui::kbd(k).into_any_element()])
    }))
}

impl Desktop {
    pub(crate) fn keybinding_settings(&self, cx: &mut Context<Self>) -> Div {
        let groups = TOPICS.map(|(topic, title)| {
            let rows = SHORTCUTS.iter().filter(|r| r.topic == topic).map(|r| row(r.label, None, div().flex().items_center().gap(px(8.)).children(r.place.map(ui::tag)).child(shortcuts(&r.keys))).min_h(px(36.)).py(px(4.))).collect();
            group(title, rows)
        });
        let open = self.settings.fixed_open;
        let fixed_title = div()
            .id("settings-fixed-keys")
            .px(px(14.))
            .flex()
            .items_center()
            .gap(px(4.))
            .cursor_pointer()
            .text_size(px(12.))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT_2)
            .child(icon(if open { "chevron-down" } else { "chevron-right" }, 12., TEXT_3))
            .child("Fixed keys")
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.settings.fixed_open = !this.settings.fixed_open;
                cx.notify();
            }));
        let fixed = open.then(|| card(FIXED.iter().map(|&(label, keys)| row(label, None, shortcuts(&keys.iter().map(|k| k.to_string()).collect::<Vec<_>>())).min_h(px(36.)).py(px(4.))).collect()));
        div()
            .flex()
            .flex_col()
            .gap(px(24.))
            .child(div().px(px(14.)).text_size(px(12.)).text_color(TEXT_2).child("Shortcuts work everywhere unless marked with where they work."))
            .children(groups)
            .child(div().flex().flex_col().gap(px(6.)).child(fixed_title).children(fixed))
    }
}

#[cfg(test)]
mod tests {
    use super::{Topic, glyphs, label, place, shortcut_rows};
    use crate::actions::bindings;
    use gpui_kit::Keystroke;

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
    fn an_action_bound_to_several_shortcuts_is_one_row() {
        let rows = shortcut_rows(&bindings());
        let focus: Vec<_> = rows.iter().filter(|r| r.label == "Focus pane").collect();
        assert_eq!((focus.len(), focus[0].keys.len(), focus[0].topic), (1, 4, Topic::Panels));
    }

    #[test]
    fn terminal_browser_and_inbox_shortcuts_are_marked_with_where_they_work() {
        let rows = shortcut_rows(&bindings());
        let at = |l: &str| rows.iter().find(|r| r.label == l).unwrap().place;
        assert_eq!([at("Copy"), at("Reload"), at("Open session"), at("Settings")], [Some("Terminal"), Some("Browser"), Some("Inbox"), None]);
    }
}
