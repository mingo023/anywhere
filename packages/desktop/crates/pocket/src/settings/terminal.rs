use super::appearance::{FontMenu, font_row, monospaced};
use super::catalog::{Look, Setting};
use super::{action, field_box, row};
use crate::desktop::Desktop;
use crate::terminal_view::surface::Metrics;
use crate::terminals::sessions::config;
use gpui_kit::component::input::{Input, InputEvent, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::prefs::terminal::{Cursor, Meta, Scheme, Shell, runs};
use theme::*;

const CURSORS: [Cursor; 3] = [Cursor::Block, Cursor::Bar, Cursor::Underline];
const SHELLS: [Shell; 2] = [Shell::Login, Shell::Custom];

/// Geist Mono ships with the app; the rest every Mac has.
const FONT: FontMenu = FontMenu {
    default: "Same as code font",
    curated: &["Geist Mono", "SF Mono", "Menlo", "Monaco", "Courier"],
    offers: monospaced,
    get: |s| s.terminal.font.as_deref(),
    set: |s, f| s.terminal.font = f,
};

const ANSI: [&str; 8] = ["Black", "Red", "Green", "Yellow", "Blue", "Magenta", "Cyan", "White"];

pub(super) const ROWS: &[Setting] = &[
    Setting::custom("terminal-font", "Text", "Font", |d, s, cx| font_row(d, s, &FONT, cx)).resets(|s| FONT.name(s), |s, d| s.terminal.font = d.terminal.font.clone()),
    Setting::stepper("terminal-size", "Text", "Size", (9, 24, 1), " pt", |s| s.terminal.size, |s, n| s.terminal.size = n),
    Setting::stepper("line-height", "Text", "Line height", (100, 200, 10), "%", |s| s.terminal.line_height, |s, n| s.terminal.line_height = n).advanced(),
    Setting::switch("ligatures", "Text", "Ligatures", |s| s.terminal.ligatures, |s, on| s.terminal.ligatures = on).hint("Joins characters like => and != into one glyph").advanced(),
    Setting::stepper("padding", "Text", "Padding", (0, 32, 1), " px", |s| s.terminal.padding, |s, n| s.terminal.padding = n).hint("Space between the text and the pane edge").advanced(),
    Setting::custom("preview", "Text", "Preview", preview),
    Setting::choice("scheme", "Colors", "Color scheme", &["Default", "Graphite", "Solarized", "Custom"], Look::Dropdown, |s| s.terminal.scheme as usize, |s, i| s.terminal.scheme = Scheme::ALL[i])
        .hint("16 ANSI colors. Click one to change it")
        .effect(configure),
    Setting::custom("swatches", "Colors", "Colors", swatches).effect(configure),
    Setting::choice("cursor", "Cursor", "Cursor shape", &["Block", "Bar", "Underline"], Look::Segmented, |s| s.terminal.cursor as usize, |s, i| s.terminal.cursor = CURSORS[i]).effect(configure),
    Setting::switch("blink", "Cursor", "Blink", |s| s.terminal.blink, |s, on| s.terminal.blink = on).effect(configure),
    Setting::choice("option-as-meta", "Keyboard & mouse", "Use Option as Meta", &["Off", "Both keys", "Left Option only", "Right Option only"], Look::Dropdown, |s| s.terminal.option_as_meta as usize, |s, i| {
        s.terminal.option_as_meta = Meta::ALL[i]
    })
        .hint("Leave off if you type characters like @ or € with Option"),
    Setting::switch("copy-on-select", "Keyboard & mouse", "Copy on select", |s| s.terminal.copy_on_select, |s, on| s.terminal.copy_on_select = on),
    Setting::switch("confirm-paste", "Keyboard & mouse", "Confirm multi-line paste", |s| s.terminal.confirm_paste, |s, on| s.terminal.confirm_paste = on)
        .hint("Asks before pasting text that would run several commands"),
    Setting::choice("link-click", "Keyboard & mouse", "Open links with", &["⌘-click", "Click"], Look::Segmented, |s| s.terminal.click_opens_links as usize, |s, i| s.terminal.click_opens_links = i == 1),
    Setting::choice("link-browser", "Keyboard & mouse", "Open links in", &["In-app browser", "System browser"], Look::Segmented, |s| s.terminal.system_browser as usize, |s, i| {
        s.terminal.system_browser = i == 1
    }),
    Setting::choice("shell", "Shell", "Shell", &["Login shell", "Custom"], Look::Segmented, |s| s.terminal.shell as usize, |s, i| s.terminal.shell = SHELLS[i])
        .note(|_| Some(format!("Login shell is {}, from your macOS account", daemon::login_shell()))),
    Setting::custom("shell-path", "Shell", "Shell path", shell_path)
        .under(|s| s.terminal.shell == Shell::Custom)
        .resets(|s| if s.terminal.shell_path.is_empty() { "Not set".into() } else { crate::util::tilde(&s.terminal.shell_path) }, |s, _| s.terminal.shell_path.clear()),
    Setting::text("shell-args", "Shell", "Arguments", "--login", |s| s.terminal.shell_args.clone(), |s, v| s.terminal.shell_args = v)
        .hint("Split at spaces")
        .under(|s| s.terminal.shell == Shell::Custom),
    Setting::switch("confirm-close", "Shell", "Confirm before closing a terminal in use", |s| s.terminal.confirm_close, |s, on| s.terminal.confirm_close = on)
        .hint("When a command other than the shell has it"),
    Setting::stepper("scrollback", "Shell", "Scrollback", (1_000, 100_000, 1_000), " lines", |s| s.terminal.scrollback, |s, n| s.terminal.scrollback = n).advanced().effect(configure),
];

fn configure(d: &mut Desktop, _: &mut Context<Desktop>) {
    d.terminals.sessions.configure(config(&d.store.terminal));
}

fn hex(c: u32) -> Hsla {
    rgb(c).into()
}

/// A prompt, a command and its result in the chosen font and colors, ANSI-coloured as a shell would.
fn preview(d: &mut Desktop, setting: &'static Setting, _: &mut Context<Desktop>) -> Div {
    let t = &d.store.terminal;
    let m = Metrics::of(t);
    let ansi = t.colors().map(hex);
    let ink: Hsla = TEXT.into();
    let lines: [&[(&str, Hsla)]; 4] = [
        &[("you ", ansi[2]), ("~/code/app ", ansi[4]), ("fix/restore-handoff ", ansi[5]), ("±", ansi[3])],
        &[("$ pnpm test", ink)],
        &[("✓ ", ansi[2]), ("42 passed ", ink), ("· ", ansi[8]), ("1 failed ", ansi[1]), ("in 3.2s", ansi[8])],
        &[("$ ", ink)],
    ];
    let (w, h) = match t.cursor {
        Cursor::Block => (m.size * 0.6, m.line),
        Cursor::Bar => (2., m.line),
        Cursor::Underline => (m.size * 0.6, 2.),
    };
    let line = m.line;
    let caret = move || div().h(px(line)).flex().items_end().child(div().w(px(w)).h(px(h)).bg(TERM_CURSOR));
    let rows = lines.into_iter().enumerate().map(|(i, spans)| {
        let text: String = spans.iter().map(|(s, _)| *s).collect();
        let runs = spans.iter().map(|(s, color)| TextRun { len: s.len(), font: m.font.clone(), color: *color, background_color: None, underline: None, strikethrough: None }).collect();
        div().h(px(m.line)).flex().whitespace_nowrap().child(StyledText::new(text).with_runs(runs)).when(i == 3, |d| d.child(caret()))
    });
    let screen = div().py(px(m.pad / 2. + 8.)).px(px(m.pad + 8.)).rounded(px(8.)).bg(CUTOUT).shadow(vec![ui::ring(HAIRLINE, 0.5)]).overflow_hidden().text_size(px(m.size)).line_height(px(m.line)).children(rows);
    div()
        .px(px(14.))
        .py(px(10.))
        .flex()
        .flex_col()
        .gap(px(8.))
        .child(div().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(setting.label))
        .child(screen)
}

/// The colour being edited and its hex field.
pub(super) struct Swatch {
    index: usize,
    field: Entity<InputState>,
    _changes: Subscription,
}

/// `#RRGGBB` or `RRGGBB` as 0xRRGGBB.
fn parse_hex(text: &str) -> Option<u32> {
    let hex = text.trim().trim_start_matches('#');
    (hex.len() == 6).then(|| u32::from_str_radix(hex, 16).ok()).flatten()
}

/// "Bright Red", the ANSI name of colour `i`.
fn ansi_name(i: usize) -> String {
    if i < 8 { ANSI[i].into() } else { format!("Bright {}", ANSI[i - 8]) }
}

/// Opens colour `i` in a hex field that recolours the terminals as it's typed.
fn edit_swatch(d: &mut Desktop, setting: &'static Setting, i: usize, window: &mut Window, cx: &mut Context<Desktop>) {
    let value = format!("#{:06X}", d.store.terminal.colors()[i]);
    let field = cx.new(|cx| InputState::new(window, cx).placeholder("#RRGGBB").default_value(value));
    let changes = cx.subscribe_in(&field, window, move |this, field, ev: &InputEvent, _, cx| match ev {
        InputEvent::Change => {
            if let Some(c) = parse_hex(&field.read(cx).value())
                && this.store.terminal.colors()[i] != c
            {
                this.store.terminal.set_color(i, c);
                this.changed(setting, cx);
            }
        }
        InputEvent::PressEnter { .. } => {
            this.settings.swatch = None;
            cx.notify();
        }
        _ => {}
    });
    field.update(cx, |f, cx| f.focus(window, cx));
    d.settings.swatch = Some(Swatch { index: i, field, _changes: changes });
    cx.notify();
}

impl Desktop {
    pub(crate) fn edit_terminal_color(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let setting = ROWS.iter().find(|s| s.id == "swatches").expect("the swatches row");
        edit_swatch(self, setting, i, window, cx);
    }
}

/// The scheme's 16 colors, normal above and bright below; clicking one opens it for editing.
fn swatches(d: &mut Desktop, setting: &'static Setting, cx: &mut Context<Desktop>) -> Div {
    let colors = d.store.terminal.colors();
    let editing = d.settings.swatch.as_ref().map(|s| s.index);
    let cell = |i: usize| {
        div()
            .id(("terminal-swatch", i))
            .flex_1()
            .h(px(20.))
            .rounded(px(4.))
            .cursor_pointer()
            .bg(hex(colors[i]))
            .shadow(vec![if editing == Some(i) { ui::ring(TEXT, 1.5) } else { ui::ring(SEPARATOR, 0.5) }])
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| edit_swatch(this, setting, i, window, cx)))
    };
    let editor = d.settings.swatch.as_ref().map(|s| {
        let field = field_box().w(px(110.)).child(div().flex_1().min_w_0().font_family(MONO).child(Input::new(&s.field).appearance(false).p_0().text_size(px(12.))));
        let done = action("terminal-swatch-done", "Done").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.settings.swatch = None;
            cx.notify();
        }));
        div()
            .flex()
            .items_center()
            .gap(px(8.))
            .pt(px(4.))
            .child(div().size(px(16.)).rounded(px(4.)).bg(hex(colors[s.index])).shadow(vec![ui::ring(SEPARATOR, 0.5)]))
            .child(div().flex_1().text_size(px(12.)).text_color(TEXT_2).child(ansi_name(s.index)))
            .child(field)
            .child(done)
    });
    let half = |from: usize| div().flex().gap(px(4.)).children((from..from + 8).map(cell));
    div().px(px(14.)).py(px(10.)).flex().flex_col().gap(px(4.)).child(half(0)).child(half(8)).children(editor)
}

/// The custom shell, chosen as a file; one that can't run is refused, so new terminals never fail to open.
fn shell_path(d: &mut Desktop, setting: &'static Setting, cx: &mut Context<Desktop>) -> Div {
    let path = d.store.terminal.shell_path.clone();
    let shown = if path.is_empty() { "Not set".to_string() } else { crate::util::tilde(&path) };
    let field = field_box().child(icon("terminal", 13., TEXT_3)).child(div().flex_1().min_w_0().truncate().font_family(MONO).text_size(px(12.)).text_color(if path.is_empty() { TEXT_3 } else { TEXT }).child(shown));
    let choose = action("shell-path-choose", "Choose…").on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
        this.pick_path(false, window, cx, move |_, paths, _, cx| {
            let Some(p) = paths.into_iter().next() else { return };
            let p = p.to_string_lossy().into_owned();
            let check = p.clone();
            cx.spawn(async move |this, cx| {
                let problem = cx.background_executor().spawn(async move { shell_problem(&check, runs) }).await;
                this.update(cx, |d, cx| match problem {
                    Some(problem) => {
                        d.error = Some(problem);
                        cx.notify();
                    }
                    None => {
                        d.store.terminal.shell_path = p;
                        d.changed(setting, cx);
                    }
                })
                .ok();
            })
            .detach();
        });
    }));
    let title = div().flex().items_center().gap(px(10.)).child(div().w(px(16.)).text_color(TEXT_4).child("\u{21b3}")).child(setting.label);
    row(title, None, div().flex().items_center().gap(px(6.)).child(field).child(choose))
}

/// Why `path` can't be a shell, or `None` when it can.
fn shell_problem(path: &str, runs: impl Fn(&str) -> bool) -> Option<String> {
    (!runs(path)).then(|| format!("{} isn't a program this Mac can run", crate::util::tilde(path)))
}

#[cfg(test)]
mod tests {
    use super::super::catalog::Control;
    use super::{FONT, ROWS, ansi_name, parse_hex, shell_problem};
    use store::Store;
    use store::prefs::terminal::{Cursor, Meta, Scheme, Shell};

    #[test]
    fn every_choice_row_reads_back_what_it_writes() {
        let mut store = Store::default();
        for row in ROWS {
            if let Control::Choice { options, get, set, .. } = row.control {
                let last = options.len() - 1;
                set(&mut store, last);
                assert_eq!(get(&store), last, "{}", row.id);
            }
        }
        let t = &store.terminal;
        assert_eq!((t.scheme, t.cursor, t.option_as_meta, t.click_opens_links, t.system_browser, t.shell), (Scheme::Custom, Cursor::Underline, Meta::Right, true, true, Shell::Custom));
    }

    #[test]
    fn a_hex_color_reads_with_or_without_its_hash() {
        assert_eq!((parse_hex(" #ff00Aa"), parse_hex("00ff00")), (Some(0xff00aa), Some(0x00ff00)));
        assert_eq!((parse_hex("#fff"), parse_hex("#gg0000"), parse_hex("")), (None, None, None));
    }

    #[test]
    fn bright_colors_are_named_after_their_normal_ones() {
        assert_eq!((ansi_name(1), ansi_name(15)), ("Red".to_string(), "Bright White".to_string()));
    }

    #[test]
    fn a_shell_that_cannot_run_is_refused_by_name() {
        assert_eq!(shell_problem("/bin/zsh", |_| true), None);
        assert!(shell_problem("/etc/hosts", |_| false).unwrap().contains("/etc/hosts"));
    }

    #[test]
    fn the_terminal_font_follows_the_code_font_until_one_is_picked() {
        let mut store = Store::default();
        store.terminal.font = None;
        assert_eq!(FONT.name(&store), "Same as code font");
        (FONT.set)(&mut store, Some("Menlo".into()));
        assert_eq!(FONT.name(&store), "Menlo");
    }
}
