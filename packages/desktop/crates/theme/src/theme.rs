use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::*;
use std::borrow::Cow;
use std::sync::{Arc, OnceLock, atomic::{AtomicBool, Ordering}};

pub const SANS: &str = ".SystemUIFont";
pub const MONO: &str = "Geist Mono";
pub const SYMBOLS: &str = "GeistMono Nerd Font Mono";

static DARK: AtomicBool = AtomicBool::new(false);

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

/// A colour with a value per appearance, resolved when painted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Token {
    light: u32,
    dark: u32,
}

impl Token {
    pub const fn new(light: u32, dark: u32) -> Self {
        Self { light, dark }
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
        rgba(t.pick(is_dark())).into()
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
/// The ring that cuts a badge out of whatever it overlaps.
pub const CUTOUT: Token = Token::new(0xffffffff, 0x171717ff);

pub const FILL_1: Token = Token::new(0x11111308, 0xebebeb0d);
pub const FILL_2: Token = Token::new(0x1111130b, 0xebebeb14);
pub const FILL_3: Token = Token::new(0x1111130e, 0xebebeb1a);
pub const FILL_4: Token = Token::new(0x11111311, 0xebebeb1f);
pub const HAIRLINE: Token = Token::new(0x11111312, 0xebebeb12);
pub const SEPARATOR: Token = Token::new(0x11111317, 0xebebeb14);
pub const SEPARATOR_STRONG: Token = Token::new(0x1111131f, 0xebebeb24);
pub const SELECTION: Token = Token::new(0x11111324, 0xebebeb33);
/// Translucent so the character under a block cursor stays readable.
pub const TERM_CURSOR: Token = Token::new(0x3030358c, 0xebebeb66);

pub const ACCENT: Token = Token::fixed(0x5b5bd6ff);
pub const ACCENT_BG: Token = Token::fixed(0x5b5bd61c);
pub const ACCENT_RING: Token = Token::fixed(0x5b5bd633);
pub const ACCENT_TINT: Token = Token::fixed(0x5b5bd612);
pub const ACCENT_GLOW: Token = Token::fixed(0x5b5bd666);

pub const WAITING: Token = Token::fixed(0xffb224ff);
pub const WAITING_TEXT: Token = Token::new(0xad5700ff, 0xffca16ff);
pub const WAITING_BG: Token = Token::fixed(0xffb2242e);
pub const FAILED: Token = Token::fixed(0xe5484dff);
pub const FAILED_BG: Token = Token::fixed(0xe5484d1c);
pub const FAILED_TEXT: Token = Token::new(0xcd2b31ff, 0xff9592ff);
pub const SUCCESS: Token = Token::new(0x2b9a66ff, 0x30a46cff);
pub const SUCCESS_TEXT: Token = Token::new(0x18794eff, 0x3dd68cff);
pub const SUCCESS_BG: Token = Token::fixed(0x30a46c21);

pub const AGENT_CLAUDE: Token = Token::fixed(0xd97757ff);
pub const AGENT_CODEX: Token = Token::fixed(0x0f9d8aff);

pub const DIFF_ADD_BG: Token = Token::fixed(0x30a46c1c);
pub const DIFF_ADD_TEXT: Token = Token::new(0x18794eff, 0x3dd68cff);
pub const DIFF_DEL_BG: Token = Token::fixed(0xe5484d17);
pub const DIFF_DEL_TEXT: Token = Token::new(0xcd2b31ff, 0xff9592ff);
pub const DIFF_ADD_WORD: Token = Token::fixed(0x30a46c40);
pub const DIFF_DEL_WORD: Token = Token::fixed(0xe5484d38);

pub const MODIFIED: Token = Token::new(0xad5700ff, 0xffca16ff);
pub const TEAL: Token = Token::new(0x0e7c86ff, 0x0bd8b6ff);
pub const TEAL_BG: Token = Token::fixed(0x0f9d8a14);

pub const SYN_KEYWORD: Token = Token::new(0x8e4ec6ff, 0xd19dffff);
pub const SYN_FN: Token = Token::new(0x3e63ddff, 0x9eb1ffff);
pub const SYN_STRING: Token = Token::new(0x18794eff, 0x3dd68cff);
pub const SYN_COMMENT: Token = Token::new(0xa1a1aaff, 0x818181ff);

fn syntax_color(name: &str) -> Token {
    match name.split('.').next().unwrap_or(name) {
        "keyword" | "boolean" | "preproc" | "attribute" => SYN_KEYWORD,
        "function" | "constructor" | "type" | "enum" | "tag" => SYN_FN,
        "string" | "number" | "constant" => SYN_STRING,
        "comment" => SYN_COMMENT,
        _ => TEXT_BODY,
    }
}

/// gpui-kit's highlight theme recoloured with the `SYN_*` tokens, for the code editor, markdown and diff rows.
pub fn highlight_theme(dark: bool) -> Arc<HighlightTheme> {
    let hsla = |c: Token| serde_json::to_value(Hsla::from(rgba(c.pick(dark)))).expect("colour serializes");
    let base = if dark { HighlightTheme::default_dark() } else { HighlightTheme::default_light() };
    let mut v = serde_json::to_value(&*base).expect("theme serializes");
    if let Some(syntax) = v["style"]["syntax"].as_object_mut() {
        for (name, style) in syntax.iter_mut() {
            *style = serde_json::json!({ "color": hsla(syntax_color(name)) });
        }
    }
    for (key, c) in [
        ("editor.background", SURFACE_SUNKEN),
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
    "arrow-right", "arrow-up", "back", "bell", "bolt", "branch", "check", "chevron-down", "chevron-right", "clock", "comment", "compose", "copy",
    "discard", "external", "file", "filter", "folder", "forward", "inbox", "list-flat", "list-tree", "mic", "minus", "more", "plus", "prompt", "search", "send", "settings", "shield",
    "sidebar", "sidebar-collapse", "sidebar-expand", "sparkle", "spinner", "split-down", "split-right", "terminal", "trash", "unfold", "worktree", "x", "x-bold",
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

/// Loads the bundled fonts and follows the system appearance. Call after `gpui_kit::init`.
pub fn init(cx: &mut App) {
    cx.text_system().add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect()).expect("bundled fonts load");
    set_appearance(cx.window_appearance(), cx);
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
    t.font_family = SANS.into();
    t.font_size = px(14.);
    t.foreground = TEXT.into();
    t.muted_foreground = TEXT_2.into();
    t.background = WINDOW_SOLID.into();
    // gpui-kit paints markdown code blocks and their copy-button backdrop with `muted`.
    t.muted = WELL.into();
    t.caret = TEXT.into();
    t.mono_font_family = MONO.into();
    t.mono_font_size = px(13.);
    t.link = ACCENT.into();
    t.highlight_theme = highlight_theme(dark);
}

/// Glass in dark, as monocode does; pale desktops make translucent light chrome illegible.
/// Reads the last `set_appearance`, so a window opened before `init` stays opaque.
pub fn window_background() -> WindowBackgroundAppearance {
    if is_dark() { WindowBackgroundAppearance::Blurred } else { WindowBackgroundAppearance::Opaque }
}

#[cfg(test)]
mod tests {
    use super::{FILL_1, FILL_2, FILL_3, FILL_4, HAIRLINE, MATERIAL, SEPARATOR, SEPARATOR_STRONG, SURFACE, SYN_COMMENT, SYN_FN, SYN_KEYWORD, SYN_STRING, Token, WINDOW, WINDOW_SOLID, highlight_theme, material, material_icon};
    use gpui_kit::component::input::HighlightStyleResolver;
    use gpui_kit::{Hsla, rgba};

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
    fn colours_syntax_with_our_tokens_in_either_scheme() {
        for dark in [false, true] {
            let t = highlight_theme(dark);
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
