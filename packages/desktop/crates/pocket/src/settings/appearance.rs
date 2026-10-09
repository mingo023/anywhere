use super::catalog::{Control, Look, Setting};
use super::{action, row};
use crate::desktop::Desktop;
use crate::git_ui::diff::{code, code_text};
use git::{Kind, Line};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::ops::Range;
use std::rc::Rc;
use store::{Appearance, DiffColors, Mode, Store, SyntaxTheme};
use theme::*;

/// The appearance to force on the app; `None` follows macOS.
pub(crate) fn forced(mode: Mode) -> Option<WindowAppearance> {
    match mode {
        Mode::System => None,
        Mode::Light => Some(WindowAppearance::Light),
        Mode::Dark => Some(WindowAppearance::Dark),
    }
}

/// Whether motion is reduced: the user's choice, else macOS's.
pub(crate) fn reduce_motion(pref: Option<bool>, system: impl FnOnce() -> bool) -> bool {
    pref.unwrap_or_else(system)
}

/// A window drawn small in the mode's colors; System is half light, half dark.
fn thumbnail(mode: Mode) -> Div {
    let window = |bg: u32, edge: u32, card: u32| {
        div()
            .size_full()
            .flex()
            .bg(rgb(bg))
            .child(div().w(px(22.)).h_full().border_r(px(0.5)).border_color(rgb(edge)))
            .child(div().flex_1().p(px(6.)).flex().flex_col().gap(px(4.)).child(div().h(px(5.)).w(relative(0.6)).rounded(px(2.)).bg(rgb(edge))).child(div().flex_1().rounded(px(3.)).bg(rgb(card))))
    };
    let inner = match mode {
        Mode::System => div().size_full().flex().child(div().flex_1().bg(rgb(0xf4f4f5))).child(div().flex_1().bg(rgb(0x161618))),
        Mode::Light => window(0xf4f4f5, 0xd4d4d8, 0xffffff),
        Mode::Dark => window(0x161618, 0x3b3b41, 0x232326),
    };
    div().w(px(76.)).h(px(48.)).rounded(px(8.)).overflow_hidden().child(inner)
}

const MODES: [Mode; 3] = [Mode::System, Mode::Light, Mode::Dark];
const MOTION: [Option<bool>; 3] = [None, Some(true), Some(false)];
const DIFF_COLORS: [DiffColors; 2] = [DiffColors::GreenRed, DiffColors::BlueOrange];
const SYNTAXES: [SyntaxTheme; 4] = [SyntaxTheme::Graphite, SyntaxTheme::Github, SyntaxTheme::One, SyntaxTheme::Solarized];

pub(super) const ROWS: &[Setting] = &[
    Setting::choice("mode", "Theme", "Appearance", &["System", "Light", "Dark"], Look::Thumbnails, |s| s.appearance.mode as usize, |s, i| s.appearance.mode = MODES[i])
        .hint("System follows macOS light and dark")
        .effect(apply_mode),
    Setting::custom("accent", "Theme", "Accent color", accent_row)
        .hint("Selection, focus rings and primary buttons")
        .resets(accent_name, |s, d| s.appearance.accent = d.appearance.accent)
        .effect(apply_accent),
    Setting::choice("reduce-motion", "Theme", "Reduce motion", &["System", "On", "Off"], Look::Segmented, |s| MOTION.iter().position(|m| *m == s.appearance.reduce_motion).unwrap_or(0), |s, i| {
        s.appearance.reduce_motion = MOTION[i]
    })
    .hint("System follows the macOS accessibility setting")
    .effect(apply_motion),
    Setting::custom("ui-font", "Fonts", "Interface font", |d, s, cx| font_row(d, s, &UI_FONT, cx))
        .resets(|s| UI_FONT.name(s), |s, d| s.appearance.ui_font = d.appearance.ui_font.clone())
        .effect(apply_fonts),
    Setting::custom("code-font", "Fonts", "Code font", |d, s, cx| font_row(d, s, &CODE_FONT, cx))
        .hint("Diffs, the editor and file previews")
        .resets(|s| CODE_FONT.name(s), |s, d| s.appearance.code_font = d.appearance.code_font.clone())
        .effect(apply_fonts),
    Setting::stepper("code-size", "Fonts", "Code size", (Appearance::CODE_SIZES.0 as i32, Appearance::CODE_SIZES.1 as i32, 1), " pt", |s| s.appearance.code_size() as i32, |s, n| {
        s.appearance.code_size = Some(n as u32)
    })
    .effect(apply_fonts),
    Setting::custom("font-preview", "Fonts", "Preview", |d, s, _| preview(d, s)),
    Setting::choice("syntax-theme", "Code colors", "Syntax theme", &["Graphite (matches app)", "GitHub", "One", "Solarized"], Look::Dropdown, |s| s.appearance.syntax as usize, |s, i| {
        s.appearance.syntax = SYNTAXES[i]
    })
    .effect(apply_syntax),
    Setting::choice("diff-colors", "Code colors", "Diff colors", &["Green & red", "Blue & orange"], Look::Segmented, |s| s.appearance.diff_colors as usize, |s, i| s.appearance.diff_colors = DIFF_COLORS[i])
        .hint("Blue and orange are easier to tell apart with red\u{2013}green color blindness")
        .effect(apply_diff_colors),
    Setting::custom("custom-diff-colors", "Code colors", "Custom diff colors", custom_diff_colors)
        .hint("Override the added and removed colors")
        .advanced()
        .resets(custom_colors, |s, d| (s.appearance.added, s.appearance.removed) = (d.appearance.added, d.appearance.removed))
        .effect(apply_diff_colors),
];

/// A font dropdown: what it calls the app's own font, the families worth listing first, and which installed ones it offers.
pub(crate) struct FontMenu {
    pub default: &'static str,
    pub curated: &'static [&'static str],
    pub offers: fn(&str) -> bool,
    pub get: fn(&Store) -> Option<&str>,
    pub set: fn(&mut Store, Option<String>),
}

const UI_FONT: FontMenu = FontMenu { default: "System (SF Pro)", curated: &["Geist", "Inter"], offers: |_| true, get: |s| s.appearance.ui_font.as_deref(), set: |s, f| s.appearance.ui_font = f };
const CODE_FONT: FontMenu = FontMenu { default: "Geist Mono", curated: &["SF Mono", "JetBrains Mono", "Menlo"], offers: monospaced, get: |s| s.appearance.code_font.as_deref(), set: |s, f| s.appearance.code_font = f };

impl FontMenu {
    pub fn name(&self, s: &Store) -> String {
        (self.get)(s).unwrap_or(self.default).to_string()
    }

    /// The installed families offered after the default: the curated ones, then the rest, leaving out macOS's hidden faces.
    fn choices(&self, installed: &[String]) -> Vec<String> {
        let shown = |f: &&String| !f.starts_with('.') && f.as_str() != self.default && (self.offers)(f);
        let curated = self.curated.iter().filter(|c| installed.iter().any(|f| f == *c)).map(|c| c.to_string());
        let rest = installed.iter().filter(shown).filter(|f| !self.curated.contains(&f.as_str())).cloned();
        curated.chain(rest).collect()
    }
}

/// Whether a family's name says it's monospaced; fonts don't report it without loading each one.
pub(super) fn monospaced(family: &str) -> bool {
    let f = family.to_lowercase();
    ["mono", "code", "courier", "menlo", "monaco", "consol", "iosevka", "fixed", "terminal"].iter().any(|w| f.contains(w))
}

/// `saved` if the system has it, else `None` for the app's own; trusted until the font list loads.
fn installed<'a>(saved: Option<&'a str>, families: &[String]) -> Option<&'a str> {
    saved.filter(|f| families.is_empty() || families.iter().any(|n| n == f))
}

/// Lists the installed fonts off the UI thread, then drops a saved font that's gone.
pub(crate) fn load_fonts(cx: &mut Context<Desktop>) {
    let text = cx.text_system().clone();
    let names = cx.background_spawn(async move { text.all_font_names() });
    cx.spawn(async move |this, cx| {
        let names = names.await;
        this.update(cx, |d, cx| {
            d.settings.fonts = names;
            let a = &d.store.appearance;
            if a.ui_font.is_some() || a.code_font.is_some() {
                apply_fonts(d, cx);
            }
        })
    })
    .detach();
}

fn apply_accent(d: &mut Desktop, cx: &mut Context<Desktop>) {
    theme::set_accent(d.store.appearance.accent, cx);
}

// Rows keep the heights they were measured at.
fn apply_fonts(d: &mut Desktop, cx: &mut Context<Desktop>) {
    let (a, fonts) = (&d.store.appearance, &d.settings.fonts);
    theme::set_fonts(installed(a.ui_font.as_deref(), fonts), installed(a.code_font.as_deref(), fonts), cx);
    d.diff.relayout(true);
    d.recolor_commit(cx);
}

fn accent_name(s: &Store) -> String {
    s.appearance.accent.map_or("Graphite".into(), |c| format!("#{c:06X}"))
}

/// A colour to pick, ringed when it's the one chosen.
fn swatch(id: impl Into<ElementId>, fill: u32, chosen: bool) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(22.))
        .flex()
        .items_center()
        .justify_center()
        .rounded_full()
        .cursor_pointer()
        .when(chosen, |d| d.shadow(vec![ui::ring(TEXT_2, 1.5)]))
        .child(ui::swatch(fill, 16., 8.))
}

/// Graphite, then `ACCENT_SWATCHES`.
fn accent_row(d: &mut Desktop, setting: &'static Setting, cx: &mut Context<Desktop>) -> Div {
    let chosen = d.store.appearance.accent;
    let dots = std::iter::once(None).chain(ACCENT_SWATCHES.map(Some)).enumerate().map(|(i, c)| {
        swatch(("accent", i), c.map_or(0x71717aff, |c| (c << 8) | 0xff), c == chosen).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.store.appearance.accent = c;
            this.changed(setting, cx);
        }))
    });
    row(setting.label, setting.hint, div().flex().items_center().gap(px(2.)).children(dots))
}

/// A dropdown of the default font then `menu`'s installed choices, scrolled rather than all drawn.
pub(super) fn font_row(d: &mut Desktop, setting: &'static Setting, menu: &'static FontMenu, cx: &mut Context<Desktop>) -> Div {
    let chosen = (menu.get)(&d.store).map(str::to_string);
    let open = d.settings.menu == Some(setting.id);
    let button = action(setting.id, "").gap(px(6.)).child(menu.name(&d.store)).child(icon("chevron-down", 12., TEXT_3)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
        this.settings.menu = (!open).then_some(setting.id);
        cx.notify();
    }));
    if !open {
        return row(setting.label, setting.hint, button);
    }
    let options: Rc<Vec<Option<String>>> = Rc::new(std::iter::once(None).chain(menu.choices(&d.settings.fonts).into_iter().map(Some)).collect());
    let count = options.len();
    let rows = uniform_list(
        SharedString::from(format!("{}-options", setting.id)),
        count,
        cx.processor(move |_, range: Range<usize>, _, cx| {
            range
                .map(|i| {
                    let font = options[i].clone();
                    div()
                        .id((setting.id, i))
                        .w_full()
                        .h(px(26.))
                        .px(px(8.))
                        .flex()
                        .items_center()
                        .gap(px(6.))
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|d| d.bg(FILL_2))
                        .text_size(px(13.))
                        .text_color(TEXT)
                        .child(div().w(px(14.)).flex_none().when(font == chosen, |d| d.child(icon("check", 13., TEXT))))
                        .child(div().truncate().child(font.clone().unwrap_or(menu.default.to_string())))
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                            this.settings.menu = None;
                            (menu.set)(&mut this.store, font.clone());
                            this.changed(setting, cx);
                        }))
                })
                .collect::<Vec<_>>()
        }),
    )
    .w(px(220.))
    .h(px(26. * count.min(12) as f32));
    let list = ui::menu_in(SharedString::from(format!("{}-menu-in", setting.id)), ui::pop(div()).p(px(4.)).child(rows));
    row(setting.label, setting.hint, div().relative().child(button).child(ui::dropdown_right(30., list)))
}

/// The sample the Preview row shows: a file name, then a removed, an added and an unchanged line.
const SAMPLE: [(Kind, &str); 3] = [(Kind::Del, "const ready = isRestored;"), (Kind::Add, "const ready = useTerminalSettled(session.id);"), (Kind::Context, "return ready ? 'reveal' : 'wait';")];

/// Byte ranges of the sample's keywords, functions and strings, coloured by hand so the preview needs no parser.
fn sample_spans(text: &str, palette: &Syntax) -> Vec<(Range<usize>, HighlightStyle)> {
    let mut spans: Vec<(Range<usize>, Token)> = Vec::new();
    for (word, color) in [("const", palette.keyword), ("return", palette.keyword), ("useTerminalSettled", palette.function), ("'reveal'", palette.string), ("'wait'", palette.string)] {
        spans.extend(text.match_indices(word).map(|(i, _)| (i..i + word.len(), color)));
    }
    spans.sort_by_key(|(r, _)| r.start);
    spans.into_iter().map(|(r, c)| (r, HighlightStyle { color: Some(c.into()), ..Default::default() })).collect()
}

fn preview(d: &Desktop, setting: &'static Setting) -> Div {
    let palette = theme::syntax();
    let lines = SAMPLE.map(|(kind, text)| (Line { kind, old: None, new: None, text: text.into() }, sample_spans(text, &palette)));
    div()
        .px(px(14.))
        .py(px(10.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(div().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(setting.label))
        .child(
            code_text(div(), d.store.appearance.code_size())
                .py(px(4.))
                .rounded(px(8.))
                .overflow_hidden()
                .bg(WINDOW_SOLID)
                .child(div().pl(px(18.)).text_color(TEXT_3).child("hooks/use-restore-preview.ts"))
                .children(lines.iter().map(|(l, spans)| code(l, Some(spans), Vec::new(), false))),
        )
}

// The window appearance observer repaints the theme once AppKit applies this.
fn apply_mode(d: &mut Desktop, cx: &mut Context<Desktop>) {
    cx.set_window_appearance(forced(d.store.appearance.mode));
}

fn apply_motion(d: &mut Desktop, cx: &mut Context<Desktop>) {
    crate::desktop::follow_reduce_motion(d.store.appearance.reduce_motion, cx);
}

// Word highlights hold resolved colours, so the diff and commit views redo them.
fn apply_diff_colors(d: &mut Desktop, cx: &mut Context<Desktop>) {
    let a = &d.store.appearance;
    theme::set_blue_orange(a.diff_colors == DiffColors::BlueOrange);
    theme::set_diff_colors(a.added, a.removed);
    d.recolor_diff(cx);
    d.recolor_commit(cx);
}

// Diff and commit rows hold the colours they were highlighted with.
fn apply_syntax(d: &mut Desktop, cx: &mut Context<Desktop>) {
    theme::set_syntax(d.store.appearance.syntax as usize, cx);
    d.recolor_diff(cx);
    d.recolor_commit(cx);
}

fn custom_colors(s: &Store) -> String {
    let name = |c: Option<u32>| c.map_or("from theme".to_string(), |c| format!("#{c:06X}"));
    match (s.appearance.added, s.appearance.removed) {
        (None, None) => "From theme".into(),
        (added, removed) => format!("Added {}, removed {}", name(added), name(removed)),
    }
}

/// Added and removed, each a row of swatches led by the theme's own colour.
fn custom_diff_colors(d: &mut Desktop, setting: &'static Setting, cx: &mut Context<Desktop>) -> Div {
    let a = d.store.appearance.clone();
    let blue_orange = a.diff_colors == DiffColors::BlueOrange;
    let picker = |label: &'static str, theme_color: Token, chosen: Option<u32>, set: fn(&mut Store, Option<u32>)| {
        let options = std::iter::once(None).chain(DIFF_SWATCHES.map(Some));
        let dots = options.enumerate().map(|(i, c)| {
            let fill = c.map_or(theme_color.pick_with(is_dark(), blue_orange), |c| (c << 8) | 0xff);
            swatch((label, i), fill, c == chosen).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                set(&mut this.store, c);
                this.changed(setting, cx);
            }))
        });
        div().flex().items_center().gap(px(2.)).child(div().w(px(64.)).text_size(px(12.)).text_color(TEXT_2).child(label)).children(dots)
    };
    let added = picker("Added", DIFF_ADD_TEXT, a.added, |s, c| s.appearance.added = c);
    let removed = picker("Removed", DIFF_DEL_TEXT, a.removed, |s, c| s.appearance.removed = c);
    row(setting.label, setting.hint, div().flex().flex_col().gap(px(4.)).child(added).child(removed))
}

/// The three modes as small windows; `setting` is the row choosing between them.
pub(super) fn thumbnails(setting: &'static Setting, chosen: usize, cx: &mut Context<Desktop>) -> Div {
    let Control::Choice { options, .. } = setting.control else { return div() };
    let tiles = MODES.into_iter().zip(options).enumerate().map(|(i, (m, label))| {
        let selected = chosen == i;
        // The selection ring sits 2px off the tile, so a border on a padded frame draws it; a translucent surface can't mask a box-shadow ring.
        let frame = div()
            .p(px(2.))
            .rounded(px(12.))
            .border_2()
            .border_color(transparent_black())
            .when(selected, |d| d.border_color(TEXT_2))
            .child(thumbnail(m).when(!selected, |t| t.shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)])));
        div()
            .id(("settings-mode", i))
            .flex()
            .flex_col()
            .items_center()
            .gap(px(2.))
            .cursor_pointer()
            .text_size(px(12.))
            .when(selected, |d| d.text_color(TEXT).font_weight(FontWeight::SEMIBOLD))
            .when(!selected, |d| d.text_color(TEXT_2).font_weight(FontWeight(450.)))
            .child(frame)
            .child(*label)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.choose(setting, i, cx)))
    });
    div().flex().gap(px(6.)).children(tiles)
}

#[cfg(test)]
mod tests {
    use super::{CODE_FONT, Control, UI_FONT, forced, installed, monospaced, reduce_motion};
    use gpui_kit::WindowAppearance;
    use store::{DiffColors, Mode, SyntaxTheme};

    #[test]
    fn system_mode_forces_nothing() {
        assert_eq!(forced(Mode::System), None);
    }

    #[test]
    fn light_and_dark_force_the_window() {
        assert_eq!([Mode::Light, Mode::Dark].map(forced), [Some(WindowAppearance::Light), Some(WindowAppearance::Dark)]);
    }

    #[test]
    fn the_rows_read_back_what_they_write() {
        let mut store = store::Store::default();
        let id = |id: &str| super::ROWS.iter().find(|r| r.id == id).unwrap();
        for (row, i) in [(id("mode"), 2), (id("reduce-motion"), 1), (id("diff-colors"), 1), (id("syntax-theme"), 3)] {
            let Control::Choice { get, set, .. } = row.control else { panic!() };
            set(&mut store, i);
            assert_eq!(get(&store), i);
        }
        assert_eq!((store.appearance.mode, store.appearance.reduce_motion, store.appearance.diff_colors), (Mode::Dark, Some(true), DiffColors::BlueOrange));
        assert_eq!(store.appearance.syntax, SyntaxTheme::Solarized);
    }

    #[test]
    fn custom_diff_colors_name_what_changed_and_reset_to_the_themes() {
        let row = super::ROWS.iter().find(|r| r.id == "custom-diff-colors").unwrap();
        let mut store = store::Store::default();
        assert_eq!(row.value(&store).as_deref(), Some("From theme"));
        store.appearance.added = Some(0x8e4ec6);
        assert_eq!(row.value(&store).as_deref(), Some("Added #8E4EC6, removed from theme"));
        super::super::catalog::reset(super::ROWS, &mut store);
        assert_eq!((store.appearance.added, store.appearance.removed), (None, None));
    }

    #[test]
    fn a_reduce_motion_override_ignores_macos() {
        assert_eq!([reduce_motion(Some(false), || true), reduce_motion(Some(true), || false), reduce_motion(None, || true)], [false, true, true]);
    }

    #[test]
    fn a_saved_font_the_system_lacks_falls_back_once_the_list_loads() {
        let families = vec!["Inter".to_string(), "Menlo".to_string()];
        assert_eq!([installed(Some("Inter"), &families), installed(Some("Gone"), &families), installed(Some("Gone"), &[])], [Some("Inter"), None, Some("Gone")]);
    }

    #[test]
    fn the_code_font_menu_offers_monospaced_families_with_the_curated_first() {
        assert!(monospaced("JetBrains Mono") && monospaced("Fira Code") && !monospaced("Helvetica"));
        let families = ["Andale Mono", "Geist Mono", "Helvetica", "Menlo", ".SF NS Mono", "Inter"].map(String::from);
        assert_eq!(CODE_FONT.choices(&families), ["Menlo", "Andale Mono"]);
        assert_eq!(UI_FONT.choices(&families), ["Inter", "Andale Mono", "Geist Mono", "Helvetica", "Menlo"]);
    }

    #[test]
    fn accent_and_fonts_name_their_choice_and_reset_to_the_defaults() {
        let row = |id| super::ROWS.iter().find(|r| r.id == id).unwrap();
        let mut store = store::Store::default();
        assert_eq!(["accent", "ui-font", "code-font", "code-size"].map(|id| row(id).value(&store).unwrap()), ["Graphite", "System (SF Pro)", "Geist Mono", "12 pt"]);
        (store.appearance.accent, store.appearance.code_font, store.appearance.code_size) = (Some(0x3e8ef7), Some("Menlo".into()), Some(14));
        assert_eq!(["accent", "code-font", "code-size"].map(|id| row(id).value(&store).unwrap()), ["#3E8EF7", "Menlo", "14 pt"]);
        super::super::catalog::reset(super::ROWS, &mut store);
        assert_eq!((store.appearance.accent, store.appearance.code_font.as_deref(), store.appearance.code_size()), (None, None, 12));
    }
}
