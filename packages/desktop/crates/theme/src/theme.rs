use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::*;
use std::borrow::Cow;
use std::sync::{Arc, OnceLock};

pub const SANS: &str = ".SystemUIFont";
pub const MONO: &str = "Geist Mono";

pub const TEXT: u32 = 0x111113ff;
pub const TEXT_BODY: u32 = 0x3f3f46ff;
pub const TEXT_2: u32 = 0x6f6f78ff;
pub const TEXT_3: u32 = 0x8b8b94ff;
pub const TEXT_4: u32 = 0xa1a1aaff;
pub const TEXT_5: u32 = 0xb4b4bcff;
pub const TEXT_6: u32 = 0xd4d4d8ff;
pub const WHITE: u32 = 0xffffffff;

pub const WINDOW: u32 = 0xf4f4f5ff;
pub const SURFACE: u32 = 0xffffffff;
pub const SURFACE_SUNKEN: u32 = 0xfafafaff;
pub const PAGE: u32 = 0xf7f7f7ff;

pub const FILL_1: u32 = 0x11111308;
pub const FILL_2: u32 = 0x1111130b;
pub const FILL_3: u32 = 0x1111130e;
pub const FILL_4: u32 = 0x11111311;
pub const HAIRLINE: u32 = 0x11111312;
pub const SEPARATOR: u32 = 0x11111317;
pub const SEPARATOR_STRONG: u32 = 0x1111131f;

pub const ACCENT: u32 = 0x5b5bd6ff;
pub const ACCENT_BG: u32 = 0x5b5bd61c;
pub const ACCENT_RING: u32 = 0x5b5bd633;
pub const ACCENT_TINT: u32 = 0x5b5bd612;
pub const ACCENT_GLOW: u32 = 0x5b5bd666;

pub const WAITING: u32 = 0xffb224ff;
pub const WAITING_TEXT: u32 = 0xad5700ff;
pub const WAITING_BG: u32 = 0xffb2242e;
pub const RUNNING: u32 = 0x30a46cff;
pub const RUNNING_TEXT: u32 = 0x18794eff;
pub const RUNNING_BG: u32 = 0x30a46c21;
pub const FAILED: u32 = 0xe5484dff;
pub const FAILED_BG: u32 = 0xe5484d1c;

pub const AGENT_CLAUDE: u32 = 0xd97757ff;
pub const AGENT_CODEX: u32 = 0x0f9d8aff;

pub const DIFF_ADD_BG: u32 = 0x30a46c1c;
pub const DIFF_ADD_TEXT: u32 = 0x18794eff;
pub const DIFF_DEL_BG: u32 = 0xe5484d17;
pub const DIFF_DEL_TEXT: u32 = 0xcd2b31ff;
pub const DIFF_ADD_WORD: u32 = 0x30a46c40;
pub const DIFF_DEL_WORD: u32 = 0xe5484d38;

pub const MODIFIED: u32 = 0xad5700ff;
pub const TEAL: u32 = 0x0e7c86ff;
pub const TEAL_BG: u32 = 0x0f9d8a14;

pub const SYN_KEYWORD: u32 = 0x8e4ec6ff;
pub const SYN_FN: u32 = 0x3e63ddff;
pub const SYN_STRING: u32 = 0x18794eff;
pub const SYN_COMMENT: u32 = 0xa1a1aaff;

fn syntax_color(name: &str) -> u32 {
    match name.split('.').next().unwrap_or(name) {
        "keyword" | "boolean" | "preproc" | "attribute" => SYN_KEYWORD,
        "function" | "constructor" | "type" | "enum" | "tag" => SYN_FN,
        "string" | "number" | "constant" => SYN_STRING,
        "comment" => SYN_COMMENT,
        _ => TEXT_BODY,
    }
}

/// gpui-kit's light highlight theme recoloured with the `SYN_*` tokens, for the code editor, markdown and diff rows.
pub fn highlight_theme() -> Arc<HighlightTheme> {
    let hsla = |c: u32| serde_json::to_value(Hsla::from(rgba(c))).expect("colour serializes");
    let mut v = serde_json::to_value(&*HighlightTheme::default_light()).expect("theme serializes");
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

pub fn provider_color(provider: &str) -> u32 {
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

pub fn icon(name: &str, size: f32, color: u32) -> Svg {
    svg().path(format!("icons/{name}.svg")).size(px(size)).flex_none().text_color(rgba(color))
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

pub fn spinner(id: impl Into<ElementId>, size: f32, color: u32) -> impl IntoElement {
    icon("spinner", size, color).with_animation(id, Animation::new(std::time::Duration::from_secs(1)).repeat(), |s, t| {
        s.with_transformation(Transformation::rotate(percentage(t)))
    })
}

pub const FONTS: [&[u8]; 8] = [
    include_bytes!("../assets/fonts/Geist-Regular.ttf"),
    include_bytes!("../assets/fonts/Geist-Medium.ttf"),
    include_bytes!("../assets/fonts/Geist-SemiBold.ttf"),
    include_bytes!("../assets/fonts/Geist-Bold.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Regular.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Medium.ttf"),
    include_bytes!("../assets/fonts/GeistMono-SemiBold.ttf"),
    include_bytes!("../assets/fonts/GeistMono-Bold.ttf"),
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

/// Loads the bundled fonts and points gpui-kit's theme at these tokens. Call after `gpui_kit::init`.
pub fn init(cx: &mut App) {
    cx.text_system().add_fonts(FONTS.iter().map(|f| Cow::Borrowed(*f)).collect()).expect("bundled fonts load");
    let t = gpui_kit::component::Theme::global_mut(cx);
    t.font_family = SANS.into();
    t.font_size = px(14.);
    t.foreground = rgba(TEXT).into();
    t.muted_foreground = rgba(TEXT_2).into();
    t.background = rgba(WINDOW).into();
    // gpui-kit paints markdown code blocks and their copy-button backdrop with `muted`.
    t.muted = rgba(WINDOW).into();
    t.caret = rgba(TEXT).into();
    t.mono_font_family = MONO.into();
    t.mono_font_size = px(13.);
    t.link = rgba(ACCENT).into();
    t.highlight_theme = highlight_theme();
}

#[cfg(test)]
mod tests {
    use super::{MATERIAL, SYN_COMMENT, SYN_FN, SYN_KEYWORD, SYN_STRING, highlight_theme, material, material_icon};
    use gpui_kit::component::input::HighlightStyleResolver;
    use gpui_kit::rgba;

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
    fn colours_syntax_with_our_tokens() {
        let t = highlight_theme();
        let color = |name: &str| t.style(name).and_then(|s| s.color);
        assert_eq!(color("keyword"), Some(rgba(SYN_KEYWORD).into()));
        assert_eq!(color("function"), Some(rgba(SYN_FN).into()));
        assert_eq!(color("string"), Some(rgba(SYN_STRING).into()));
        assert_eq!(color("comment"), Some(rgba(SYN_COMMENT).into()));
    }
}
