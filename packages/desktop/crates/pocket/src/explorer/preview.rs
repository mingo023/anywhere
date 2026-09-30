mod code;
mod header;
mod markdown;

use crate::desktop::Desktop;
use crate::desktop::chrome::empty;
use crate::explorer::mermaid::Diagrams;
use crate::syntax::language_for;
use code::{code_pane, decorations, gutter};
use git::Line;
use gpui_kit::component::input::{EditorState, TextDecoration, TextDecorationCollection};
use gpui_kit::component::text::TextViewState;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use theme::*;

const MAX_BYTES: u64 = 512 * 1024;
const MAX_IMAGE_BYTES: u64 = 20 * 1024 * 1024;
const IMAGES: &[&str] = &["png", "jpg", "jpeg", "gif", "webp", "bmp", "ico", "svg"];

#[derive(Clone, Debug, PartialEq)]
pub enum Preview {
    Text(String),
    Image,
    Binary(u64),
    TooLarge(u64),
    Unreadable,
}

/// Text unless the first 8 KB hold a NUL byte or the bytes aren't UTF-8.
fn decode(bytes: Vec<u8>) -> Preview {
    let len = bytes.len() as u64;
    if bytes[..bytes.len().min(8192)].contains(&0) {
        return Preview::Binary(len);
    }
    String::from_utf8(bytes).map_or(Preview::Binary(len), Preview::Text)
}

/// What the name and size of a `len`-byte file at `path` decide it shows as; `None` when its bytes must be read.
fn classify(path: &str, len: u64) -> Option<Preview> {
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let image = IMAGES.contains(&ext.as_str());
    if len > if image { MAX_IMAGE_BYTES } else { MAX_BYTES } {
        return Some(Preview::TooLarge(len));
    }
    image.then_some(Preview::Image)
}

/// What Explore can show for the file at `path`.
pub fn preview(path: &str) -> Preview {
    let Ok(len) = std::fs::metadata(path).map(|m| m.len()) else { return Preview::Unreadable };
    classify(path, len).unwrap_or_else(|| std::fs::read(path).map_or(Preview::Unreadable, decode))
}

/// What a file shows as and its diff against HEAD when git reports it changed.
pub fn load(path: &str, changed: bool) -> (String, Preview, Vec<Line>) {
    let preview = preview(path);
    let dir = Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let diff = if changed { git::file_diff(&dir, path) } else { Vec::new() };
    (path.to_string(), preview, diff)
}

pub fn language(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, e)| e) {
        Some("ts" | "tsx") => "TypeScript",
        Some("js" | "jsx" | "mjs" | "cjs") => "JavaScript",
        Some("rs") => "Rust",
        Some("py") => "Python",
        Some("go") => "Go",
        Some("swift") => "Swift",
        Some("kt" | "kts") => "Kotlin",
        Some("java") => "Java",
        Some("json") => "JSON",
        Some("md") => "Markdown",
        Some("toml") => "TOML",
        Some("yml" | "yaml") => "YAML",
        Some("sh" | "zsh" | "bash") => "Shell",
        Some("css") => "CSS",
        Some("html") => "HTML",
        _ => "Plain text",
    }
}

pub fn size(bytes: usize) -> String {
    match bytes {
        b if b < 1024 => format!("{b} B"),
        b if b < 1024 * 1024 => format!("{:.1} KB", b as f32 / 1024.),
        b => format!("{:.1} MB", b as f32 / (1024. * 1024.)),
    }
}

fn pane() -> Div {
    div().flex_1().min_h_0().border_t(px(0.5)).border_color(rgba(SEPARATOR))
}

/// The editors a file previews in.
pub struct Views {
    pub(crate) code: Entity<EditorState>,
    pub(crate) marks: TextDecorationCollection,
    pub(crate) md: Entity<TextViewState>,
    pub(crate) diagrams: Entity<Diagrams>,
}

#[derive(Debug, PartialEq)]
pub enum Body {
    Markdown,
    Code,
    Image,
    Note(String),
    Blank,
}

/// What the code and markdown views load after the shown file changed.
pub struct CodeSync {
    pub text: SharedString,
    pub language: &'static str,
    /// The views already hold this file, so its scroll position stays.
    pub same: bool,
    pub reload: bool,
    pub marks: Vec<TextDecoration>,
}

/// The file Explore shows. The editors are `V` so the rules over the plain state test without GPUI.
pub struct PreviewState<V = Views> {
    pub(crate) file: Option<String>,
    pub(crate) preview: Option<Preview>,
    pub(crate) diff: Vec<git::Line>,
    pub(crate) stale: bool,
    pub(crate) code_file: Option<String>,
    pub(crate) code_text: SharedString,
    pub(crate) md_source: bool,
    /// The copied path and the timer that turns its check back; a new copy replaces, and so cancels, the old one.
    pub(crate) path_copied: Option<(String, Task<()>)>,
    pub(crate) views: V,
}

impl PreviewState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> Self {
        let code = cx.new(|cx| EditorState::new(window, cx).line_number(true).searchable(true).soft_wrap(false));
        let marks = code.update(cx, |s, cx| s.create_decorations_collection(Vec::new(), cx));
        let md = cx.new(|cx| TextViewState::markdown("", cx));
        let diagrams = cx.new(|_| Diagrams::new(&md));
        Self::with(Views { code, marks, md, diagrams })
    }
}

impl<V> PreviewState<V> {
    pub fn with(views: V) -> Self {
        Self {
            file: None,
            preview: None,
            diff: Vec::new(),
            stale: false,
            code_file: None,
            code_text: SharedString::default(),
            md_source: false,
            path_copied: None,
            views,
        }
    }

    /// Shows `path`, clearing what another file left behind until it loads.
    pub fn open(&mut self, path: String) {
        if self.file.as_ref() != Some(&path) {
            self.file = Some(path);
            self.preview = None;
            self.diff.clear();
            self.stale = true;
            self.md_source = false;
        }
    }

    /// Shows `file` unless another file was opened while it loaded; returns whether anything changed.
    pub fn apply(&mut self, (path, preview, lines): (String, Preview, Vec<Line>)) -> bool {
        if self.file.as_ref() != Some(&path) {
            return false;
        }
        let preview = Some(preview);
        let fresh = preview != self.preview || lines != self.diff;
        self.stale |= fresh;
        (self.preview, self.diff) = (preview, lines);
        fresh
    }

    pub fn text(&self) -> Option<&str> {
        match &self.preview {
            Some(Preview::Text(t)) => Some(t.as_str()),
            _ => None,
        }
    }

    /// Whether the file is markdown text, which renders unless its source is asked for.
    pub fn markdown(&self) -> bool {
        self.text().is_some() && language_for(self.file.as_deref().unwrap_or_default()) == "markdown"
    }

    pub fn body(&self) -> Body {
        match &self.preview {
            Some(Preview::Text(_)) if self.markdown() && !self.md_source => Body::Markdown,
            Some(Preview::Text(_)) => Body::Code,
            Some(Preview::Image) => Body::Image,
            Some(Preview::Binary(n)) => Body::Note(format!("Binary file · {}", size(*n as usize))),
            Some(Preview::TooLarge(n)) => Body::Note(format!("Too large to preview · {}", size(*n as usize))),
            Some(Preview::Unreadable) => Body::Note("This file can't be shown.".into()),
            None => Body::Blank,
        }
    }

    /// What the views must load since the shown file last changed, once per change.
    pub fn take_sync(&mut self) -> Option<CodeSync> {
        if !std::mem::take(&mut self.stale) {
            return None;
        }
        let text = self.text().map(|t| SharedString::from(t.to_string())).unwrap_or_default();
        let same = self.code_file == self.file;
        let reload = !same || text != self.code_text;
        let language = language_for(self.file.as_deref().unwrap_or_default());
        let marks = decorations(&text, &gutter(&self.diff));
        self.code_file = self.file.clone();
        self.code_text = text.clone();
        Some(CodeSync { text, language, same, reload, marks })
    }
}

impl Desktop {
    pub fn load_file(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.preview.file.clone() else { return };
        let changed = self.file_status(&path).is_some();
        let task = cx.background_executor().spawn(async move { load(&path, changed) });
        cx.spawn(async move |this, cx| {
            let file = task.await;
            this.update(cx, |d, cx| {
                if d.apply_file(file) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn apply_file(&mut self, file: (String, Preview, Vec<Line>)) -> bool {
        self.preview.apply(file)
    }

    pub fn file_view(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(path) = self.preview.file.clone() else { return div() };
        let header = self.file_header(&path, cx);
        let body = match self.preview.body() {
            Body::Markdown => self.markdown_pane(cx).into_any_element(),
            Body::Code => code_pane(&self.preview.views.code).into_any_element(),
            Body::Image => pane()
                .p(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(SURFACE_SUNKEN))
                .child(img(PathBuf::from(&path)).max_w_full().max_h_full().object_fit(ObjectFit::Contain))
                .into_any_element(),
            Body::Note(note) => empty(note).into_any_element(),
            Body::Blank => div().flex_1().into_any_element(),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(header)
            .child(body)
    }

    /// Loads the open file into the code editor after it changed, keeping the scroll position when the same file refreshes.
    pub fn sync_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(CodeSync { text, language, same, reload, marks }) = self.preview.take_sync() else { return };
        self.preview.views.code.update(cx, |s, cx| {
            if !same {
                s.set_highlighter(language, cx);
            }
            if reload {
                let scroll = s.scroll_offset();
                s.set_value(text, window, cx);
                if same {
                    s.set_scroll_offset(scroll, cx);
                }
            }
        });
        self.preview.views.marks.set(marks, cx);
        if reload {
            self.preview.views.md.update(cx, |md, cx| {
                md.set_text(&self.preview.code_text, cx);
                if !same {
                    md.list_state().scroll_to(ListOffset::default());
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Body, Preview, PreviewState, classify, decode, language, preview, size};
    use git::{Kind, Line};

    #[test]
    fn names_languages_and_sizes() {
        assert_eq!(language("src/a.tsx"), "TypeScript");
        assert_eq!(language("Makefile"), "Plain text");
        assert_eq!(size(612), "612 B");
        assert_eq!(size(2048), "2.0 KB");
    }

    #[test]
    fn tells_text_from_binary() {
        assert_eq!(decode(b"fn main() {}\n".to_vec()), Preview::Text("fn main() {}\n".into()));
        assert_eq!(decode(b"PK\x03\x04\0\0".to_vec()), Preview::Binary(6));
        assert_eq!(decode(vec![0xff, 0xfe, b'a']), Preview::Binary(3));
    }

    #[test]
    fn images_may_be_larger_than_text_before_they_are_too_large() {
        assert_eq!(classify("a/logo.png", 20 * 1024 * 1024), Some(Preview::Image));
        assert_eq!(classify("a/logo.png", 20 * 1024 * 1024 + 1), Some(Preview::TooLarge(20 * 1024 * 1024 + 1)));
        assert_eq!(classify("a/notes.txt", 512 * 1024), None);
        assert_eq!(classify("a/notes.txt", 512 * 1024 + 1), Some(Preview::TooLarge(512 * 1024 + 1)));
    }

    #[test]
    fn previews_files_by_kind_and_size() {
        let dir = std::env::temp_dir().join(format!("pocket-preview-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = |name: &str, bytes: &[u8]| {
            let p = dir.join(name);
            std::fs::write(&p, bytes).unwrap();
            p.to_string_lossy().into_owned()
        };
        assert_eq!(preview(&file("a.rs", b"x\n")), Preview::Text("x\n".into()));
        assert_eq!(preview(&file("logo.PNG", b"\x89PNG\0")), Preview::Image);
        assert_eq!(preview(&file("big.txt", &vec![b'a'; 600 * 1024])), Preview::TooLarge(600 * 1024));
        assert_eq!(preview(&dir.join("missing.rs").to_string_lossy()), Preview::Unreadable);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    fn opened(path: &str) -> PreviewState<()> {
        let mut state = PreviewState::with(());
        state.open(path.into());
        state
    }

    fn text(t: &str) -> Preview {
        Preview::Text(t.into())
    }

    #[test]
    fn a_load_for_a_file_no_longer_shown_is_dropped() {
        let mut state = opened("/r/b.rs");
        assert!(!state.apply(("/r/a.rs".into(), text("a"), Vec::new())));
        assert_eq!((state.file.as_deref(), state.preview), (Some("/r/b.rs"), None));
    }

    #[test]
    fn a_load_for_the_shown_file_changes_it_only_when_its_content_differs() {
        let mut state = opened("/r/a.rs");
        state.stale = false;
        assert!(state.apply(("/r/a.rs".into(), text("a"), Vec::new())));
        assert_eq!((state.preview.clone(), state.stale), (Some(text("a")), true));
        state.stale = false;
        assert!(!state.apply(("/r/a.rs".into(), text("a"), Vec::new())));
        assert!(!state.stale);
    }

    #[test]
    fn opening_another_file_clears_the_last_one_but_reopening_keeps_it() {
        let mut state = opened("/r/a.md");
        state.apply(("/r/a.md".into(), text("# a"), Vec::new()));
        state.md_source = true;
        state.stale = false;
        state.open("/r/a.md".into());
        assert_eq!((state.preview.clone(), state.md_source, state.stale), (Some(text("# a")), true, false));
        state.open("/r/b.md".into());
        assert_eq!((state.file.as_deref(), state.preview.clone(), state.md_source, state.stale), (Some("/r/b.md"), None, false, true));
    }

    fn body_of(path: &str, preview: Preview) -> Body {
        let mut state = opened(path);
        state.apply((path.into(), preview, Vec::new()));
        state.body()
    }

    #[test]
    fn markdown_text_renders_unless_its_source_is_asked_for() {
        let mut state = opened("/r/README.md");
        state.apply(("/r/README.md".into(), text("# hi"), Vec::new()));
        assert_eq!((state.markdown(), state.body()), (true, Body::Markdown));
        state.md_source = true;
        assert_eq!(state.body(), Body::Code);
        assert_eq!(body_of("/r/a.rs", text("fn a() {}")), Body::Code);
        assert_eq!(body_of("/r/x.md", Preview::Binary(4)), Body::Note("Binary file · 4 B".into()));
    }

    #[test]
    fn files_without_text_show_the_image_or_say_why_not_and_nothing_while_loading() {
        assert_eq!(body_of("/r/logo.png", Preview::Image), Body::Image);
        assert_eq!(body_of("/r/big.log", Preview::TooLarge(3 * 1024 * 1024)), Body::Note("Too large to preview · 3.0 MB".into()));
        assert_eq!(body_of("/r/gone.rs", Preview::Unreadable), Body::Note("This file can't be shown.".into()));
        assert_eq!(opened("/r/a.rs").body(), Body::Blank);
    }

    #[test]
    fn a_newly_shown_file_syncs_once_with_its_grammar_and_from_the_top() {
        let mut state = opened("/r/a.rs");
        state.apply(("/r/a.rs".into(), text("fn a() {}"), Vec::new()));
        let sync = state.take_sync().unwrap();
        assert_eq!((sync.text.as_ref(), sync.language, sync.same, sync.reload), ("fn a() {}", "rust", false, true));
        assert!(state.take_sync().is_none());
    }

    #[test]
    fn a_refreshed_file_keeps_its_place_and_reloads_only_when_its_text_changed() {
        let mut state = opened("/r/a.rs");
        state.apply(("/r/a.rs".into(), text("a\nb\n"), Vec::new()));
        state.take_sync();
        let added = vec![Line { kind: Kind::Add, old: None, new: Some(2), text: "b".into() }];
        state.apply(("/r/a.rs".into(), text("a\nb\n"), added.clone()));
        let sync = state.take_sync().unwrap();
        assert_eq!((sync.same, sync.reload), (true, false));
        assert_eq!(sync.marks.iter().map(|m| (m.range.start, m.range.end)).collect::<Vec<_>>(), [(2, 3)]);
        state.apply(("/r/a.rs".into(), text("a\nc\n"), added));
        let sync = state.take_sync().unwrap();
        assert_eq!((sync.text.as_ref(), sync.same, sync.reload), ("a\nc\n", true, true));
    }
}
