use gpui_kit::*;
use std::borrow::Cow;

pub const SANS: &str = "Geist";
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

pub const FILL_1: u32 = 0x11111308;
pub const FILL_2: u32 = 0x1111130b;
pub const FILL_3: u32 = 0x1111130e;
pub const FILL_4: u32 = 0x11111311;
pub const HAIRLINE: u32 = 0x11111312;
pub const SEPARATOR: u32 = 0x11111317;
pub const SEPARATOR_STRONG: u32 = 0x1111131f;
pub const ROW_SELECTED: u32 = 0xffffffeb;

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

pub const MODIFIED: u32 = 0xad5700ff;
pub const MERGED: u32 = 0x8250dfff;
pub const MERGED_BG: u32 = 0x8250df1f;
pub const TEAL: u32 = 0x0e7c86ff;
pub const TEAL_BG: u32 = 0x0f9d8a14;

pub const SYN_KEYWORD: u32 = 0x8e4ec6ff;
pub const SYN_FN: u32 = 0x3e63ddff;
pub const SYN_STRING: u32 = 0x18794eff;
pub const SYN_COMMENT: u32 = 0xa1a1aaff;

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

pub fn spinner(id: impl Into<ElementId>, size: f32, color: u32) -> impl IntoElement {
    icon("spinner", size, color).with_animation(id, Animation::new(std::time::Duration::from_secs(1)).repeat(), |s, t| {
        s.with_transformation(Transformation::rotate(percentage(t)))
    })
}

const FONTS: [&[u8]; 8] = [
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
    "arrow-right", "back", "bell", "bolt", "branch", "check", "chevron-down", "chevron-right", "clock", "comment", "compose", "copy",
    "external", "file", "filter", "folder", "forward", "inbox", "merge", "mic", "more", "plus", "prompt", "search", "send", "settings", "shield",
    "sidebar", "sparkle", "spinner", "split-down", "split-right", "terminal", "unfold", "worktree", "x", "x-bold",
);

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if let Some((_, bytes)) = ICONS.iter().find(|(p, _)| *p == path) {
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
    t.caret = rgba(TEXT).into();
}
