use gpui_kit::*;
use std::borrow::Cow;

pub const SANS: &str = "Geist";
pub const MONO: &str = "Geist Mono";

pub const APP: u32 = 0xf6f5f2;
pub const RAIL: u32 = 0xeeece7;
pub const MAIN: u32 = 0xf8f7f4;
pub const WHITE: u32 = 0xffffff;
pub const LINE: u32 = 0xe0ded8;
pub const FIELD: u32 = 0xdedcd6;
pub const HAIR: u32 = 0xe6e4df;
pub const TAB_LINE: u32 = 0xe2e0da;
pub const CARD: u32 = 0xe4e2dd;
pub const PANE_LINE: u32 = 0xeceae5;
pub const TERM_LINE: u32 = 0xcfcbc3;
pub const SEGMENT: u32 = 0xe9e7e2;
pub const SELECTED: u32 = 0xe6e4df;
pub const PROJECT_SELECTED: u32 = 0xe1dfd9;
pub const HOVER: u32 = 0xe8e6e1;

pub const INK: u32 = 0x1c1b19;
pub const TEXT: u32 = 0x3e3c38;
pub const MUTED: u32 = 0x6b6964;
pub const FAINT: u32 = 0xa09d97;
pub const TERM_TEXT: u32 = 0x2b2a27;

pub const GREEN: u32 = 0x1e8a4c;
pub const LIVE: u32 = 0x3faf6c;
pub const RED: u32 = 0xb93a2a;
pub const AMBER: u32 = 0xd99a2b;
pub const AMBER_TEXT: u32 = 0x9a620a;
pub const CODEX: u32 = 0x1b8378;
pub const CLAUDE: u32 = 0xc15f3c;

pub const HUNK: u32 = 0xf0efeb;
pub const DEL_BG: u32 = 0xfbeae7;
pub const DEL_TEXT: u32 = 0x8f2a1e;
pub const ADD_BG: u32 = 0xe7f4ec;
pub const ADD_TEXT: u32 = 0x15603a;
pub const PICK: u32 = 0x3b6fd1;
pub const PICK_BG: u32 = 0xd6e7f0;
pub const PICK_CONTEXT: u32 = 0xe1e9f5;
pub const COMPOSER_LINE: u32 = 0xd3dbe8;

const PROJECTS: [u32; 5] = [0x1b8378, 0xc15f3c, 0x6b5cc4, 0xb58a1b, 0x4f7cbf];

pub fn project_color(index: usize) -> u32 {
    PROJECTS[index % PROJECTS.len()]
}

pub fn provider_color(provider: &str) -> u32 {
    match provider {
        "codex" => CODEX,
        "claude" => CLAUDE,
        _ => MUTED,
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
    svg().path(format!("icons/{name}.svg")).size(px(size)).flex_none().text_color(rgb(color))
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
    "arrow-right", "back", "bolt", "branch", "check", "chevron-down", "external", "file", "filter", "folder", "forward", "inbox", "plus",
    "search", "send", "settings", "shield", "sidebar", "spinner", "split-down", "split-right", "terminal", "x",
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
