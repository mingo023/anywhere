use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::*;
use std::borrow::Cow;
use std::sync::{Arc, OnceLock, RwLock, atomic::{AtomicBool, AtomicU32, AtomicUsize, Ordering}};

pub const SANS: &str = ".SystemUIFont";
pub const MONO: &str = "Geist Mono";
pub const SYMBOLS: &str = "GeistMono Nerd Font Mono";
pub const R_POPOVER: f32 = 12.;
pub const R_DIALOG: f32 = 16.;
pub const MENU_IN: std::time::Duration = std::time::Duration::from_millis(150);

pub mod contrast;

static DARK: AtomicBool = AtomicBool::new(false);
static BLUE_ORANGE: AtomicBool = AtomicBool::new(false);
static SYNTAX: AtomicUsize = AtomicUsize::new(0);
/// The user's added and removed colours as `0xRRGGBBff`; 0 keeps the theme's.
static ADDED: AtomicU32 = AtomicU32::new(0);
static REMOVED: AtomicU32 = AtomicU32::new(0);
/// The user's accent as `0xRRGGBBff`; 0 is Graphite.
static ACCENT_RGB: AtomicU32 = AtomicU32::new(0);
/// The interface and code fonts the user picked; `None` is `SANS` and `MONO`.
static CHOSEN_FONTS: RwLock<(Option<SharedString>, Option<SharedString>)> = RwLock::new((None, None));

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// Paints diffs blue and orange instead of green and red, from the next frame.
pub fn set_blue_orange(on: bool) {
    BLUE_ORANGE.store(on, Ordering::Relaxed);
}

/// Paints added and removed diff lines in these `0xRRGGBB` colours from the next frame; `None` keeps the theme's.
pub fn set_diff_colors(added: Option<u32>, removed: Option<u32>) {
    let opaque = |c: Option<u32>| c.map_or(0, |c| (c << 8) | 0xff);
    ADDED.store(opaque(added), Ordering::Relaxed);
    REMOVED.store(opaque(removed), Ordering::Relaxed);
}

/// Paints selection, focus rings and primary buttons in this `0xRRGGBB` colour; `None`, or anything past `0xffffff`, is Graphite.
pub fn set_accent(rgb: Option<u32>, cx: &mut App) {
    ACCENT_RGB.store(accent_rgba(rgb), Ordering::Relaxed);
    set_appearance(if is_dark() { WindowAppearance::Dark } else { WindowAppearance::Light }, cx);
}

fn accent_rgba(rgb: Option<u32>) -> u32 {
    rgb.filter(|c| *c <= 0xffffff).map_or(0, |c| (c << 8) | 0xff)
}

/// Sets the interface and code font families; `None` keeps `SANS` and `MONO`.
pub fn set_fonts(ui: Option<&str>, code: Option<&str>, cx: &mut App) {
    *CHOSEN_FONTS.write().unwrap_or_else(|e| e.into_inner()) = (ui.map(|f| f.to_string().into()), code.map(|f| f.to_string().into()));
    let t = gpui_kit::component::Theme::global_mut(cx);
    (t.font_family, t.mono_font_family) = (ui_font(), code_font());
}

pub fn ui_font() -> SharedString {
    CHOSEN_FONTS.read().unwrap_or_else(|e| e.into_inner()).0.clone().unwrap_or(SharedString::new_static(SANS))
}

/// The font of diffs, the editor and file previews.
pub fn code_font() -> SharedString {
    CHOSEN_FONTS.read().unwrap_or_else(|e| e.into_inner()).1.clone().unwrap_or(SharedString::new_static(MONO))
}

/// What a custom accent paints a token in that role.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Accent {
    /// The accent at this alpha.
    Fill(u8),
    /// Text on an opaque accent fill.
    On,
}

/// A colour with a value per appearance, resolved when painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    light: u32,
    dark: u32,
    /// A diff colour's light and dark values when diffs are blue and orange.
    blue_orange: Option<(u32, u32)>,
    /// Whether it's an added (`true`) or removed line's colour, and its alpha when the user picked that colour.
    diff: Option<(bool, u8)>,
    accent: Option<Accent>,
}

impl Token {
    pub const fn new(light: u32, dark: u32) -> Self {
        Self { light, dark, blue_orange: None, diff: None, accent: None }
    }

    const fn or_blue_orange(mut self, light: u32, dark: u32) -> Self {
        self.blue_orange = Some((light, dark));
        self
    }

    const fn diff(mut self, added: bool, alpha: u8) -> Self {
        self.diff = Some((added, alpha));
        self
    }

    const fn accent(mut self, role: Accent) -> Self {
        self.accent = Some(role);
        self
    }

    /// The colour a custom `accent` (`0xRRGGBBff`, 0 for Graphite) paints this token, if it's in an accent role.
    fn accented(self, accent: u32) -> Option<u32> {
        match self.accent? {
            _ if accent == 0 => None,
            Accent::Fill(alpha) => Some((accent & 0xffffff00) | alpha as u32),
            Accent::On => Some(on(accent)),
        }
    }

    /// The colour painted, given the user's added and removed colours (`0xRRGGBBff`, 0 for none).
    fn paint(self, dark: bool, blue_orange: bool, (added, removed): (u32, u32)) -> u32 {
        match self.diff.map(|(a, alpha)| (if a { added } else { removed }, alpha)) {
            Some((custom, alpha)) if custom != 0 => (custom & 0xffffff00) | alpha as u32,
            _ => self.pick_with(dark, blue_orange),
        }
    }

    pub fn pick_with(self, dark: bool, blue_orange: bool) -> u32 {
        match self.blue_orange {
            Some((light, night)) if blue_orange => {
                if dark {
                    night
                } else {
                    light
                }
            }
            _ => self.pick(dark),
        }
    }

    pub const fn fixed(c: u32) -> Self {
        Self::new(c, c)
    }

    pub fn pick(self, dark: bool) -> u32 {
        if dark { self.dark } else { self.light }
    }
}

impl From<Token> for Hsla {
    fn from(t: Token) -> Self {
        let painted = t.accented(ACCENT_RGB.load(Ordering::Relaxed)).unwrap_or_else(|| t.paint(is_dark(), BLUE_ORANGE.load(Ordering::Relaxed), (ADDED.load(Ordering::Relaxed), REMOVED.load(Ordering::Relaxed))));
        rgba(painted).into()
    }
}

impl From<Token> for Fill {
    fn from(t: Token) -> Self {
        Hsla::from(t).into()
    }
}

pub const TEXT: Token = Token::new(0x111113ff, 0xebebebff);
pub const TEXT_BODY: Token = Token::new(0x3f3f46ff, 0xbcbcbcff);
pub const TEXT_2: Token = Token::new(0x6f6f78ff, 0xa1a1a1ff);
pub const TEXT_3: Token = Token::new(0x8b8b94ff, 0x818181ff);
pub const TEXT_4: Token = Token::new(0xa1a1aaff, 0x6c6c6cff);
pub const TEXT_5: Token = Token::new(0xb4b4bcff, 0x616161ff);
pub const TEXT_6: Token = Token::new(0xd4d4d8ff, 0x414141ff);
pub const WHITE: Token = Token::fixed(0xffffffff);
/// Text and glyphs on a `TEXT` fill.
pub const ON_TEXT: Token = Token::new(0xffffffff, 0x171717ff);
/// Primary buttons: ink in Graphite, else the accent.
pub const PRIMARY: Token = TEXT.accent(Accent::Fill(0xff));
pub const ON_PRIMARY: Token = ON_TEXT.accent(Accent::On);

/// White or near-black, whichever reads better on `fill`.
fn on(fill: u32) -> u32 {
    let (white, ink) = (0xffffffff, 0x171717ff);
    if contrast::ratio(white, fill) >= contrast::ratio(ink, fill) { white } else { ink }
}

pub const WINDOW: Token = Token::new(0xf4f4f5ff, 0x171717d9);
/// The window colour where nothing shows through: floated panels and images.
pub const WINDOW_SOLID: Token = Token::new(0xf4f4f5ff, 0x171717ff);
pub const SURFACE: Token = Token::new(0xffffffff, 0xebebeb1a);
pub const SURFACE_SUNKEN: Token = Token::new(0xfafafaff, 0x00000000);
pub const PAGE: Token = Token::new(0xf7f7f7ff, 0x00000000);
/// A recessed block inside a page: tables, code blocks.
pub const WELL: Token = Token::new(0xf4f4f5ff, 0xebebeb0d);
pub const SIDE: Token = Token::new(0xfafafbb3, 0x00000000);
/// Opaque because GPUI has no backdrop blur.
pub const POPOVER: Token = Token::new(0xffffffff, 0x232323ff);
pub const GLASS: Token = Token::new(0xffffff9e, 0x2a2a2acc);
pub const HIGHLIGHT: Token = Token::new(0xfffffff2, 0xffffff0f);
/// Near-clear in dark: a wide black blur on a dark window reads as a smudge, so the ring alone lifts the surface.
pub const SHADOW: Token = Token::new(0x00000024, 0x0000000a);
/// The ring that cuts a badge out of whatever it overlaps.
pub const CUTOUT: Token = Token::new(0xffffffff, 0x171717ff);

pub const FILL_1: Token = Token::new(0x11111308, 0xebebeb0d);
pub const FILL_2: Token = Token::new(0x1111130b, 0xebebeb14);
pub const FILL_3: Token = Token::new(0x1111130e, 0xebebeb1a);
pub const FILL_4: Token = Token::new(0x11111311, 0xebebeb1f);
pub const HAIRLINE: Token = Token::new(0x11111312, 0xebebeb12);
pub const SEPARATOR: Token = Token::new(0x11111317, 0xebebeb14);
pub const SEPARATOR_STRONG: Token = Token::new(0x1111131f, 0xebebeb24);
pub const SELECTION: Token = Token::new(0x11111324, 0xebebeb33).accent(Accent::Fill(0x40));
/// Translucent so the character under a block cursor stays readable.
pub const TERM_CURSOR: Token = Token::new(0x3030358c, 0xebebeb66);

/// No hue: the accent is ink, so links and focus read as text weight rather than colour.
pub const ACCENT: Token = Token::new(0x3f3f46ff, 0xd4d4d4ff);
pub const ACCENT_BG: Token = Token::new(0x1111131c, 0xebebeb33).accent(Accent::Fill(0x33));
pub const ACCENT_RING: Token = Token::new(0x11111333, 0xebebeb66).accent(Accent::Fill(0x80));
pub const ACCENT_TINT: Token = Token::new(0x11111317, 0xebebeb29).accent(Accent::Fill(0x24));
pub const ACCENT_GLOW: Token = Token::new(0x11111399, 0xebebeb99).accent(Accent::Fill(0x99));
/// Accent buttons: `ACCENT` in Graphite, else the accent.
pub const ACCENT_FILL: Token = ACCENT.accent(Accent::Fill(0xff));
/// What Accent color offers after Graphite: blue, purple, pink, red, orange, yellow and green.
pub const ACCENT_SWATCHES: [u32; 7] = [0x3e8ef7, 0x8e4ec6, 0xc2298a, 0xe5484d, 0xf76b15, 0xffb224, 0x30a46c];

/// The amber of dots and badges, which stays bright in light mode where `WAITING` darkens for text.
pub const WAITING_DOT: Token = Token::fixed(0xffb224ff);
pub const ON_WAITING: Token = Token::fixed(0x1a1306ff);
/// Underlined wherever it is used: colour alone doesn't mark a link.
pub const ACCENT_LINK: Token = Token::new(0x111113ff, 0xebebebff);
pub const WAITING: Token = Token::new(0xad5700ff, 0xffb224ff);
pub const WAITING_TEXT: Token = Token::new(0xad5700ff, 0xffca16ff);
pub const WAITING_BG: Token = Token::fixed(0xffb2242e);
pub const FAILED: Token = Token::new(0xcd2b31ff, 0xe5484dff);
pub const FAILED_BG: Token = Token::fixed(0xe5484d1c);
pub const FAILED_TEXT: Token = Token::new(0xcd2b31ff, 0xff9592ff);
pub const SUCCESS: Token = Token::new(0x2b9a66ff, 0x30a46cff);
pub const SUCCESS_TEXT: Token = Token::new(0x18794eff, 0x3dd68cff);
pub const SUCCESS_BG: Token = Token::fixed(0x30a46c21);
pub const WORKING: Token = Token::new(0xcc4e00ff, 0xf76b15ff);
pub const MERGED: Token = Token::new(0x8250dfff, 0xa371f7ff);

pub const AGENT_CLAUDE: Token = Token::fixed(0xd97757ff);
pub const AGENT_CODEX: Token = Token::fixed(0x0f9d8aff);

pub const DIFF_ADD_BG: Token = Token::fixed(0x30a46c1c).or_blue_orange(0x0090ff1c, 0x0090ff1c).diff(true, 0x1c);
pub const DIFF_ADD_TEXT: Token = Token::new(0x18794eff, 0x3dd68cff).or_blue_orange(0x0b5fb5ff, 0x70b8ffff).diff(true, 0xff);
pub const DIFF_DEL_BG: Token = Token::fixed(0xe5484d17).or_blue_orange(0xf76b1517, 0xf76b1517).diff(false, 0x17);
pub const DIFF_DEL_TEXT: Token = Token::new(0xcd2b31ff, 0xff9592ff).or_blue_orange(0xc24400ff, 0xff9e57ff).diff(false, 0xff);
pub const DIFF_ADD_WORD: Token = Token::fixed(0x30a46c40).or_blue_orange(0x0090ff40, 0x0090ff40).diff(true, 0x40);
pub const DIFF_DEL_WORD: Token = Token::fixed(0xe5484d38).or_blue_orange(0xf76b1538, 0xf76b1538).diff(false, 0x38);
/// What Custom diff colors offers for added and removed lines.
pub const DIFF_SWATCHES: [u32; 8] = [0x30a46c, 0x0090ff, 0x12a594, 0x8e4ec6, 0xe5484d, 0xf76b15, 0xd6409f, 0xffb224];

pub const MODIFIED: Token = Token::new(0xad5700ff, 0xffca16ff);
pub const TEAL: Token = Token::new(0x0e7c86ff, 0x0bd8b6ff);
pub const TEAL_BG: Token = Token::fixed(0x0f9d8a14);

pub const GRAPH_HEAD: Token = Token::new(0x1a5cffff, 0x57a3f8ff);
pub const GRAPH_UPSTREAM: Token = Token::new(0x652d90ff, 0xad80d7ff);
pub const GRAPH_BASE: Token = Token::fixed(0xea5c00ff);
pub const GRAPH_LANES: [Token; 5] = [Token::fixed(0xffb000ff), Token::fixed(0xdc267fff), Token::fixed(0x994f00ff), Token::fixed(0x40b0a6ff), Token::fixed(0xb66dffff)];

pub const SYN_KEYWORD: Token = Token::new(0x8e4ec6ff, 0xd19dffff);
pub const SYN_FN: Token = Token::new(0x3e63ddff, 0x9eb1ffff);
pub const SYN_STRING: Token = Token::new(0x18794eff, 0x3dd68cff);
pub const SYN_COMMENT: Token = Token::new(0xa1a1aaff, 0x818181ff);

/// A syntax theme: the colours of keywords, functions and types, strings and numbers, and comments.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Syntax {
    pub keyword: Token,
    pub function: Token,
    pub string: Token,
    pub comment: Token,
}

/// Graphite, GitHub, One and Solarized, in the order Settings lists them.
pub const SYNTAX_THEMES: [Syntax; 4] = [
    Syntax { keyword: SYN_KEYWORD, function: SYN_FN, string: SYN_STRING, comment: SYN_COMMENT },
    Syntax { keyword: Token::new(0xcf222eff, 0xff7b72ff), function: Token::new(0x8250dfff, 0xd2a8ffff), string: Token::new(0x0a3069ff, 0xa5d6ffff), comment: Token::new(0x6e7781ff, 0x8b949eff) },
    Syntax { keyword: Token::new(0xa626a4ff, 0xc678ddff), function: Token::new(0x4078f2ff, 0x61afefff), string: Token::new(0x50a14fff, 0x98c379ff), comment: Token::new(0xa0a1a7ff, 0x7f848eff) },
    Syntax { keyword: Token::new(0x859900ff, 0x859900ff), function: Token::new(0x268bd2ff, 0x268bd2ff), string: Token::new(0x2aa198ff, 0x2aa198ff), comment: Token::new(0x93a1a1ff, 0x657b83ff) },
];

/// Colours code with `SYNTAX_THEMES[i]`; past the end means Graphite. Highlights already worked out keep their colours.
pub fn set_syntax(i: usize, cx: &mut App) {
    SYNTAX.store(i, Ordering::Relaxed);
    gpui_kit::component::Theme::global_mut(cx).highlight_theme = highlight_theme(is_dark());
}

/// The syntax theme code is coloured with.
pub fn syntax() -> Syntax {
    SYNTAX_THEMES.get(SYNTAX.load(Ordering::Relaxed)).copied().unwrap_or(SYNTAX_THEMES[0])
}

fn syntax_color(palette: &Syntax, name: &str) -> Token {
    match name.split('.').next().unwrap_or(name) {
        "keyword" | "boolean" | "preproc" | "attribute" => palette.keyword,
        "function" | "constructor" | "type" | "enum" | "tag" => palette.function,
        "string" | "number" | "constant" => palette.string,
        "comment" => palette.comment,
        _ => TEXT_BODY,
    }
}

/// gpui-kit's highlight theme recoloured with the chosen syntax theme, for the code editor, markdown and diff rows.
pub fn highlight_theme(dark: bool) -> Arc<HighlightTheme> {
    highlight_theme_with(dark, &syntax())
}

fn highlight_theme_with(dark: bool, palette: &Syntax) -> Arc<HighlightTheme> {
    let hsla = |c: Token| serde_json::to_value(Hsla::from(rgba(c.pick(dark)))).expect("colour serializes");
    let base = if dark { HighlightTheme::default_dark() } else { HighlightTheme::default_light() };
    let mut v = serde_json::to_value(&*base).expect("theme serializes");
    if let Some(syntax) = v["style"]["syntax"].as_object_mut() {
        for (name, style) in syntax.iter_mut() {
            *style = serde_json::json!({ "color": hsla(syntax_color(palette, name)) });
        }
    }
    for (key, c) in [
        ("editor.background", SURFACE_SUNKEN),
        ("editor.gutter.background", SURFACE_SUNKEN),
        ("editor.foreground", TEXT_BODY),
        ("editor.line_number", TEXT_5),
        ("editor.active_line_number", TEXT_2),
        ("editor.active_line.background", FILL_1),
    ] {
        v["style"][key] = hsla(c);
    }
    Arc::new(serde_json::from_value(v).expect("theme deserializes"))
}

/// Colours a repository can take in the rail, the menu and the new-session form.
pub const PALETTE: [u32; 6] = [0xd97757ff, 0x0f9d8aff, 0x7b61ffff, 0xe8a317ff, 0x3b82f6ff, 0x8b8b94ff];

pub fn provider_color(provider: &str) -> Token {
    match provider {
        "codex" => AGENT_CODEX,
        "claude" => AGENT_CLAUDE,
        _ => TEXT_3,
    }
}

/// Claude's spark in its orange; OpenAI's knot, or a terminal for anything else, in `ink`.
pub fn provider_icon(provider: &str, size: f32, ink: impl Into<Hsla>) -> Svg {
    match provider {
        "claude" => icon("claude", size, AGENT_CLAUDE),
        "codex" => icon("openai", size, ink),
        _ => icon("terminal", size, ink),
    }
}

pub fn provider_name(provider: &str) -> &'static str {
    match provider {
        "codex" => "Codex",
        "claude" => "Claude Code",
        _ => "Shell",
    }
}

pub fn icon(name: &str, size: f32, color: impl Into<Hsla>) -> Svg {
    svg().path(format!("icons/{name}.svg")).size(px(size)).flex_none().text_color(color)
}

fn material() -> &'static serde_json::Value {
    static MANIFEST: OnceLock<serde_json::Value> = OnceLock::new();
    MANIFEST.get_or_init(|| serde_json::from_str(include_str!("../assets/material/icons.json")).expect("material manifest parses"))
}

/// Material Icon Theme's icon for the last component of `path`, matched like VS Code: the whole name, then its extensions longest first.
fn material_icon(path: &str, folder: bool, open: bool) -> &'static str {
    let m = material();
    let name = path.rsplit('/').next().unwrap_or(path).to_lowercase();
    match (folder, open) {
        (true, true) => m["foldersOpen"][&name].as_str().unwrap_or("folder-open"),
        (true, false) => m["folders"][&name].as_str().unwrap_or("folder"),
        _ => m["names"][&name]
            .as_str()
            .or_else(|| name.match_indices('.').find_map(|(i, _)| m["extensions"][&name[i + 1..]].as_str()))
            .unwrap_or("file"),
    }
}

pub fn file_icon(path: &str, folder: bool, open: bool, size: f32) -> Img {
    img(format!("icons/material/{}.svg", material_icon(path, folder, open))).size(px(size)).flex_none()
}

pub fn spinner(id: impl Into<ElementId>, size: f32, color: impl Into<Hsla>) -> impl IntoElement {
    icon("spinner", size, color).with_animation(id, Animation::new(std::time::Duration::from_secs(1)).repeat(), |s, t| {
        s.with_transformation(Transformation::rotate(percentage(t)))
    })
}

pub fn dot_spinner(id: impl Into<ElementId>, size: f32, color: impl Into<Hsla>) -> impl IntoElement {
    const FRAMES: [&str; 10] = ["⠋", "⠙", "⠹", "⠸", "⠼", "⠴", "⠦", "⠧", "⠇", "⠏"];
    div()
        .size(px(size))
        .flex_none()
        .flex()
        .items_center()
        .justify_center()
        // A braille glyph's dots cover a fraction of its em box.
        .text_size(px(size * 1.6))
        .line_height(px(size))
        .text_color(color.into())
        // Uncapped, a repeating animation redraws the whole window every vsync.
        .with_animation(id, Animation::new(std::time::Duration::from_secs(1)).repeat().with_max_fps(FRAMES.len() as f32), |d, t| {
            d.child(FRAMES[(t * FRAMES.len() as f32) as usize % FRAMES.len()])
        })
}

pub const FONTS: [&[u8]; 9] = [
    include_bytes!("../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Geist-Bold.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Regular.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Medium.ttf"),
    include_bytes!("../assets/fonts/GeistMono-SemiBold.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Bold.ttf"),
    include_bytes!("../assets/fonts/GeistMonoNerdFontMono-Regular.otf"),
];

pub struct Assets;

macro_rules! embed {
    ($($name:literal),* $(,)?) => {
        &[$((concat!("icons/", $name, ".svg"), include_bytes!(concat!("../assets/icons/", $name, ".svg")))),*]
    };
}

const ICONS: &[(&str, &[u8])] = embed!(
    "appearance", "arrow-right", "arrow-up", "back", "bell", "bolt", "branch", "check", "chevron-down", "chevron-right", "claude", "clock", "cloud", "comment", "compose", "copy", "diff-multiple", "diff-split",
    "discard", "external", "file", "filter", "flow", "folder", "forward", "globe", "grip", "inbox", "keyboard", "laptop", "list-flat", "list-tree", "merge", "mic", "minus", "more", "openai", "pencil", "phone", "pin", "play", "plus", "prompt", "pull-request", "reload", "run-cancelled", "run-skipped", "search", "send", "settings", "shield",
    "sidebar", "sidebar-collapse", "sidebar-expand", "sliders", "sparkle", "spinner", "split-down", "split-right", "stop", "terminal", "trash", "unfold", "warning", "worktree", "x", "x-bold",
);

const MATERIAL: &[(&str, &[u8])] = include!(concat!(env!("OUT_DIR"), "/material.rs"));

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ICONS.iter().chain(MATERIAL).find(|(p, _)| *p == path) {
            return Ok(Some(Cow::Borrowed(bytes)));
        }
        gpui_kit::assets::Assets.load(path)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        gpui_kit::assets::Assets.list(path)
    }
}

/// Loads the bundled fonts and applies `appearance`. Call after `gpui_kit::init`.
pub fn init(appearance: WindowAppearance, cx: &mut App) {
    cx.text_system().add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect()).expect("bundled fonts load");
    set_appearance(appearance, cx);
}

/// Switches every token to `appearance` and points gpui-kit's theme at them.
pub fn set_appearance(appearance: WindowAppearance, cx: &mut App) {
    let dark = matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark);
    DARK.store(dark, Ordering::Relaxed);
    /* `change` keeps fonts its config leaves unset and builds markdown's defaults from them, so
    start each switch from gpui-kit's fonts, as at launch, rather than ours. */
    let kit = gpui_kit::component::Theme::default();
    let t = gpui_kit::component::Theme::global_mut(cx);
    (t.font_family, t.font_size, t.mono_font_family, t.mono_font_size) = (kit.font_family, kit.font_size, kit.mono_font_family, kit.mono_font_size);
    gpui_kit::component::Theme::change(appearance, None, cx);
    let t = gpui_kit::component::Theme::global_mut(cx);
    t.font_family = ui_font();
    t.font_size = px(14.);
    t.foreground = TEXT.into();
    t.muted_foreground = TEXT_2.into();
    t.background = WINDOW_SOLID.into();
    // gpui-kit paints markdown code blocks and their copy-button backdrop with `muted`.
    t.muted = WELL.into();
    t.caret = TEXT.into();
    t.mono_font_family = code_font();
    t.mono_font_size = px(13.);
    t.link = ACCENT_LINK.into();
    t.highlight_theme = highlight_theme(dark);
    // Graphite keeps gpui-kit's own focus ring and text selection.
    if ACCENT_RGB.load(Ordering::Relaxed) != 0 {
        (t.primary, t.primary_foreground, t.ring, t.selection) = (PRIMARY.into(), ON_PRIMARY.into(), PRIMARY.into(), SELECTION.into());
    }
}

/// Glass in dark, as monocode does; pale desktops make translucent light chrome illegible.
/// Reads the last `set_appearance`, so a window opened before `init` stays opaque.
pub fn window_background() -> WindowBackgroundAppearance {
    if is_dark() { WindowBackgroundAppearance::Blurred } else { WindowBackgroundAppearance::Opaque }
}

#[cfg(test)]
mod tests {
    use super::contrast::{over, ratio};
    use super::{
        ACCENT, ACCENT_LINK, DIFF_ADD_TEXT, DIFF_DEL_TEXT, FAILED, FAILED_TEXT, FILL_1, FILL_2, FILL_3, FILL_4, HAIRLINE, MATERIAL, MODIFIED, PAGE, POPOVER, SEPARATOR, SEPARATOR_STRONG, SIDE, SUCCESS, SUCCESS_TEXT, SURFACE, SURFACE_SUNKEN, SYN_COMMENT, SYN_FN, SYN_KEYWORD,
        ON_TEXT, SYN_STRING, TEXT, TEXT_2, TEXT_3, TEXT_BODY, Token, WAITING, WAITING_TEXT, WHITE, WINDOW, WINDOW_SOLID, DIFF_ADD_BG, DIFF_DEL_WORD, SYNTAX_THEMES, highlight_theme_with, material, material_icon,
        ACCENT_BG, ACCENT_SWATCHES, ON_PRIMARY, PRIMARY, SELECTION, WORKING, accent_rgba,
    };
    use gpui_kit::component::input::HighlightStyleResolver;
    use gpui_kit::{Hsla, rgba};

    const TEXT_ROLES: [(&str, Token); 11] = [
        ("TEXT", TEXT),
        ("TEXT_BODY", TEXT_BODY),
        ("TEXT_2", TEXT_2),
        ("WAITING_TEXT", WAITING_TEXT),
        ("SUCCESS_TEXT", SUCCESS_TEXT),
        ("FAILED_TEXT", FAILED_TEXT),
        ("ACCENT", ACCENT),
        ("ACCENT_LINK", ACCENT_LINK),
        ("DIFF_ADD_TEXT", DIFF_ADD_TEXT),
        ("DIFF_DEL_TEXT", DIFF_DEL_TEXT),
        ("MODIFIED", MODIFIED),
    ];
    const GLYPHS: [(&str, Token); 6] = [("WAITING", WAITING), ("SUCCESS", SUCCESS), ("FAILED", FAILED), ("ACCENT", ACCENT), ("WORKING", WORKING), ("TEXT_3", TEXT_3)];
    const FILLS: [(&str, Token); 2] = [("FAILED", FAILED), ("WAITING", WAITING)];
    /// FAILED and WAITING keep WHITE in both schemes (the owner's dark-theme plan), so these dark pairs stay below AA.
    const DARK_EXEMPT: [&str; 2] = ["WHITE on FAILED", "WHITE on WAITING"];

    fn surfaces(dark: bool) -> [(&'static str, u32); 6] {
        let window = WINDOW_SOLID.pick(dark);
        let on_window = |t: Token| over(t.pick(dark), window);
        [("WINDOW", window), ("SURFACE", on_window(SURFACE)), ("SURFACE_SUNKEN", on_window(SURFACE_SUNKEN)), ("PAGE", on_window(PAGE)), ("POPOVER", on_window(POPOVER)), ("SIDE", on_window(SIDE))]
    }

    fn below(dark: bool, roles: &[(&str, Token)], min: f32) -> Vec<String> {
        surfaces(dark).into_iter().flat_map(|(bg_name, bg)| roles.iter().filter(move |(_, t)| ratio(t.pick(dark), bg) < min).map(move |(name, _)| format!("{name} on {bg_name}"))).collect()
    }

    fn white_below(dark: bool) -> Vec<String> {
        FILLS.iter().filter(|(_, fill)| ratio(WHITE.pick(dark), fill.pick(dark)) < 4.5).map(|(name, _)| format!("WHITE on {name}")).collect()
    }

    #[test]
    fn light_text_roles_clear_4_5_on_every_surface() {
        assert_eq!(below(false, &TEXT_ROLES, 4.5), Vec::<String>::new());
    }

    #[test]
    fn blue_and_orange_diff_text_clears_4_5_on_every_surface_in_both_schemes() {
        for dark in [false, true] {
            let low: Vec<String> = surfaces(dark)
                .into_iter()
                .flat_map(|(bg_name, bg)| [("DIFF_ADD_TEXT", DIFF_ADD_TEXT), ("DIFF_DEL_TEXT", DIFF_DEL_TEXT)].into_iter().filter(move |(_, t)| ratio(t.pick_with(dark, true), bg) < 4.5).map(move |(n, _)| format!("{n} on {bg_name}")))
                .collect();
            assert_eq!((dark, low), (dark, Vec::<String>::new()));
        }
    }

    #[test]
    fn only_diff_colours_change_with_blue_and_orange() {
        assert_ne!(DIFF_ADD_TEXT.pick_with(true, true), DIFF_ADD_TEXT.pick(true));
        assert_eq!(TEXT.pick_with(true, true), TEXT.pick(true));
    }

    #[test]
    fn light_status_glyphs_clear_3() {
        assert_eq!(below(false, &GLYPHS, 3.), Vec::<String>::new());
    }

    #[test]
    fn white_labels_clear_4_5_on_failed_and_waiting_fills() {
        assert_eq!(white_below(false), Vec::<String>::new());
    }

    #[test]
    fn labels_on_the_accent_fill_clear_4_5_in_both_schemes() {
        for dark in [false, true] {
            assert!(ratio(ON_TEXT.pick(dark), ACCENT.pick(dark)) >= 4.5, "dark: {dark}");
        }
    }

    #[test]
    fn dark_pairs_clear_aa_except_the_listed_exemptions() {
        let text = below(true, &TEXT_ROLES, 4.5).into_iter().map(|s| format!("text {s}"));
        let glyphs = below(true, &GLYPHS, 3.).into_iter().map(|s| format!("glyph {s}"));
        let mut failing: Vec<String> = text.chain(glyphs).chain(white_below(true)).collect();
        failing.sort();
        let mut exempt = DARK_EXEMPT.map(String::from).to_vec();
        exempt.sort();
        assert_eq!(failing, exempt);
    }

    #[test]
    fn matches_material_icons_like_vscode() {
        assert_eq!(material_icon("CHANGELOG.md", false, false), "changelog");
        assert_eq!(material_icon("src/.gitignore", false, false), "git");
        assert_eq!(material_icon("a.test.ts", false, false), "test-ts");
        assert_eq!(material_icon("main.rs", false, false), "rust");
        assert_eq!(material_icon("pnpm-lock.yaml", false, false), "pnpm_light");
        assert_eq!(material_icon("Makefile.unknown-ext", false, false), "file");
        assert_eq!(material_icon("src", true, false), "folder-src");
        assert_eq!(material_icon("src", true, true), "folder-src-open");
        assert_eq!(material_icon("nothing-special", true, true), "folder-open");
    }

    #[test]
    fn embeds_every_material_icon_the_manifest_names() {
        let m = material().as_object().unwrap();
        let named = m.values().flat_map(|s| s.as_object().unwrap().values()).filter_map(|v| v.as_str());
        for name in named.chain(["file", "folder", "folder-open"]) {
            let path = format!("icons/material/{name}.svg");
            assert!(MATERIAL.iter().any(|(p, _)| *p == path), "{path} is not embedded");
        }
    }

    #[test]
    fn each_syntax_theme_colours_keywords_functions_strings_and_comments_its_own_way() {
        let github = &SYNTAX_THEMES[1];
        let t = highlight_theme_with(true, github);
        let color = |name: &str| t.style(name).and_then(|s| s.color);
        for (name, token) in [("keyword", github.keyword), ("function", github.function), ("string", github.string), ("comment", github.comment)] {
            assert_eq!(color(name), Some(rgba(token.pick(true)).into()), "{name}");
        }
        let keywords: std::collections::HashSet<u32> = SYNTAX_THEMES.iter().map(|p| p.keyword.pick(true)).collect();
        assert_eq!(keywords.len(), SYNTAX_THEMES.len());
    }

    #[test]
    fn a_picked_diff_colour_keeps_each_tokens_alpha_and_leaves_other_colours_alone() {
        let green = (0x30a46cff, 0);
        assert_eq!(DIFF_ADD_BG.paint(true, false, (0x8e4ec6ff, 0)), 0x8e4ec61c);
        assert_eq!(DIFF_ADD_TEXT.paint(false, true, (0x8e4ec6ff, 0)), 0x8e4ec6ff);
        assert_eq!(DIFF_DEL_WORD.paint(true, false, green), DIFF_DEL_WORD.pick(true));
        assert_eq!(DIFF_DEL_TEXT.paint(true, true, (0, 0xffb224ff)), 0xffb224ff);
        assert_eq!(TEXT.paint(true, false, (0x8e4ec6ff, 0x8e4ec6ff)), TEXT.pick(true));
    }

    #[test]
    fn a_custom_accent_paints_only_the_accent_roles_keeping_their_alpha() {
        let blue = accent_rgba(Some(0x3e8ef7));
        assert_eq!([PRIMARY, SELECTION, ACCENT_BG].map(|t| t.accented(blue)), [Some(0x3e8ef7ff), Some(0x3e8ef740), Some(0x3e8ef733)]);
        assert_eq!([ACCENT, TEXT, DIFF_ADD_BG].map(|t| t.accented(blue)), [None; 3]);
        assert_eq!(PRIMARY.accented(0), None);
    }

    #[test]
    fn graphite_is_no_accent_and_so_is_a_colour_past_rgb() {
        assert_eq!([accent_rgba(None), accent_rgba(Some(0x1000000)), accent_rgba(Some(0xffb224))], [0, 0, 0xffb224ff]);
    }

    #[test]
    fn labels_on_every_accent_swatch_clear_4_5() {
        for c in ACCENT_SWATCHES {
            let fill = accent_rgba(Some(c));
            let label = ON_PRIMARY.accented(fill).unwrap();
            assert!(ratio(label, fill) >= 4.5, "{c:06x}");
        }
    }

    #[test]
    fn colours_syntax_with_our_tokens_in_either_scheme() {
        for dark in [false, true] {
            let t = highlight_theme_with(dark, &SYNTAX_THEMES[0]);
            let color = |name: &str| t.style(name).and_then(|s| s.color);
            for (name, token) in [("keyword", SYN_KEYWORD), ("function", SYN_FN), ("string", SYN_STRING), ("comment", SYN_COMMENT)] {
                assert_eq!(color(name), Some(rgba(token.pick(dark)).into()), "{name} dark={dark}");
            }
        }
    }

    #[test]
    fn a_token_picks_its_light_or_dark_value() {
        let t = Token::new(0x111111ff, 0xeeeeeeff);
        assert_eq!((t.pick(false), t.pick(true)), (0x111111ff, 0xeeeeeeff));
        assert_eq!(Token::fixed(0x5b5bd6ff).pick(true), 0x5b5bd6ff);
    }

    #[test]
    fn a_token_paints_light_until_told_otherwise() {
        let t = Token::new(0x111111ff, 0xeeeeeeff);
        assert_eq!(Hsla::from(t), Hsla::from(rgba(0x111111ff)));
    }

    #[test]
    fn dark_is_glass_over_the_desktop_and_light_is_opaque() {
        assert_eq!(WINDOW.pick(false) & 0xff, 0xff);
        assert_eq!(WINDOW.pick(true), 0x171717d9);
        assert_eq!(WINDOW_SOLID.pick(true), 0x171717ff);
    }

    #[test]
    fn dark_fills_are_ink_over_transparent() {
        for t in [FILL_1, FILL_2, FILL_3, FILL_4, HAIRLINE, SEPARATOR, SEPARATOR_STRONG, SURFACE] {
            assert_eq!(t.pick(true) >> 8, 0xebebeb);
        }
    }
}
