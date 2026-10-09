mod code;
mod header;
mod markdown;

pub(crate) use header::relative;

use crate::add_to_chat::offers_chat;
use crate::desktop::Desktop;
use crate::desktop::chrome::empty;
use crate::explorer::mermaid::Diagrams;
use crate::syntax::language_for;
use crate::util::basename;
use code::{Mark, code_pane, gutter};
use markdown::markdown_pane;
use git::Line;
use gpui_kit::component::input::{EditorState, InputEvent, TabSize};
use gpui_kit::component::text::TextViewState;
use gpui_kit::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;
use store::prefs::files::{Autosave, Files};
use theme::*;
use workspace::Doc;
use workspace::tree::PaneId;

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

fn frame() -> Div {
    div().flex_1().min_h_0().border_t(px(0.5)).border_color(SEPARATOR)
}

/// Soft wrap, line numbers and tab width, as the Files settings ask of the code editor.
type Look = (bool, bool, usize);

fn look(files: &Files) -> Look {
    (files.soft_wrap, files.line_numbers, files.tab_width as usize)
}

/// The editors a file previews in.
pub struct Views {
    pub(crate) code: Entity<EditorState>,
    pub(crate) md: Entity<TextViewState>,
    pub(crate) diagrams: Entity<Diagrams>,
    look: Look,
    _edits: Subscription,
    _blur: Subscription,
}

impl Views {
    fn new(pane: PaneId, look @ (wrap, numbers, tab): Look, window: &mut Window, cx: &mut Context<Desktop>) -> Self {
        let code = cx.new(|cx| {
            EditorState::new(window, cx).line_number(numbers).searchable(true).soft_wrap(wrap).tab_size(TabSize { tab_size: tab, hard_tabs: false })
        });
        let md = cx.new(|cx| TextViewState::markdown("", cx));
        let diagrams = cx.new(|_| Diagrams::new(&md));
        let _edits = cx.subscribe(&code, move |this, code, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                let text = code.read(cx).value();
                this.edited(pane, text, cx);
                if this.chat.pill.take().is_some() {
                    cx.notify();
                }
            }
        });
        let focus = code.focus_handle(cx);
        let _blur = cx.on_blur(&focus, window, move |this, _, cx| {
            if let (Autosave::OnBlur, Some(file)) = (this.store.files.autosave, this.preview.unsaved(pane)) {
                this.save_file(file, false, cx);
            }
        });
        Self { code, md, diagrams, look, _edits, _blur }
    }
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
}

/// The file one pane shows and what its editors hold.
pub struct FilePane<V = Views> {
    pub(crate) file: Option<String>,
    pub(crate) preview: Option<Preview>,
    pub(crate) diff: Vec<git::Line>,
    pub(crate) stale: bool,
    pub(crate) code_file: Option<String>,
    pub(crate) code_text: SharedString,
    /// What git changed in the text the editor holds.
    pub(crate) marks: Rc<[(usize, Mark)]>,
    pub(crate) md_source: bool,
    /// Built on the pane's first render, since editors need the window.
    pub(crate) views: Option<V>,
}

impl<V> Default for FilePane<V> {
    fn default() -> Self {
        Self {
            file: None,
            preview: None,
            diff: Vec::new(),
            stale: false,
            code_file: None,
            code_text: SharedString::default(),
            marks: Rc::default(),
            md_source: false,
            views: None,
        }
    }
}

impl<V> FilePane<V> {
    /// Shows `path`, clearing what another file left behind until it loads.
    pub fn open(&mut self, path: String, source: bool) {
        if self.file.as_ref() != Some(&path) {
            self.file = Some(path);
            self.preview = None;
            self.diff.clear();
            self.stale = true;
            self.md_source = source;
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
}

/// The file each pane shows, and the unsaved edits all panes share. The editors are `V` so the rules over the plain state test without GPUI.
pub struct PreviewState<V = Views> {
    pub(crate) panes: HashMap<PaneId, FilePane<V>>,
    /// The copied path and the timer that turns its check back; a new copy replaces, and so cancels, the old one.
    pub(crate) path_copied: Option<(String, Task<()>)>,
    /// Saves every draft once typing pauses; a new edit replaces, and so restarts, it.
    autosave: Option<Task<()>>,
    /// Unsaved text by file path. Panes showing one file share its draft, so edits live here, not in an editor.
    pub(crate) drafts: HashMap<String, SharedString>,
}

impl<V> Default for PreviewState<V> {
    fn default() -> Self {
        Self { panes: HashMap::new(), path_copied: None, autosave: None, drafts: HashMap::new() }
    }
}

impl<V> PreviewState<V> {
    pub fn pane(&self, pane: PaneId) -> Option<&FilePane<V>> {
        self.panes.get(&pane)
    }

    pub fn file(&self, pane: PaneId) -> Option<&str> {
        self.pane(pane)?.file.as_deref()
    }

    pub fn open(&mut self, pane: PaneId, path: String, source: bool) {
        self.panes.entry(pane).or_default().open(path, source);
    }

    pub fn apply(&mut self, pane: PaneId, file: (String, Preview, Vec<Line>)) -> bool {
        self.panes.get_mut(&pane).is_some_and(|f| f.apply(file))
    }

    pub fn body(&self, pane: PaneId) -> Body {
        let Some(f) = self.pane(pane) else { return Body::Blank };
        match &f.preview {
            Some(Preview::Text(_)) if f.markdown() && !f.md_source => Body::Markdown,
            Some(Preview::Text(_)) => Body::Code,
            _ if f.file.as_deref().is_some_and(|p| self.dirty(p)) => Body::Code,
            Some(Preview::Image) => Body::Image,
            Some(Preview::Binary(n)) => Body::Note(format!("Binary file · {}", size(*n as usize))),
            Some(Preview::TooLarge(n)) => Body::Note(format!("Too large to preview · {}", size(*n as usize))),
            Some(Preview::Unreadable) => Body::Note("This file can't be shown.".into()),
            None => Body::Blank,
        }
    }

    /// What `pane`'s views must load since its file last changed, once per change.
    pub fn take_sync(&mut self, pane: PaneId) -> Option<CodeSync> {
        let f = self.panes.get_mut(&pane)?;
        if !std::mem::take(&mut f.stale) {
            return None;
        }
        let draft = f.file.as_ref().and_then(|p| self.drafts.get(p)).cloned();
        let text = draft.unwrap_or_else(|| f.text().map(|t| SharedString::from(t.to_string())).unwrap_or_default());
        let same = f.code_file == f.file;
        let reload = !same || text != f.code_text;
        let language = language_for(f.file.as_deref().unwrap_or_default());
        f.code_file = f.file.clone();
        f.code_text = text.clone();
        f.marks = gutter(&f.diff).into();
        Some(CodeSync { text, language, same, reload })
    }

    /// Keeps `pane`'s editor text as the draft of the file it holds, dropping the draft once the text matches the file on disk, and reloads other panes holding that file. Returns the file when that flipped whether it's dirty.
    pub fn edit(&mut self, pane: PaneId, text: SharedString) -> Option<String> {
        let f = self.panes.get_mut(&pane)?;
        let file = f.code_file.clone()?;
        let clean = f.file == f.code_file && f.text() == Some(text.as_ref());
        let was_dirty = if clean { self.drafts.remove(&file).is_some() } else { self.drafts.insert(file.clone(), text.clone()).is_some() };
        f.code_text = text;
        for (_, other) in self.panes.iter_mut().filter(|(p, o)| **p != pane && o.code_file.as_ref() == Some(&file)) {
            other.stale = true;
        }
        (was_dirty == clean).then_some(file)
    }

    pub fn dirty(&self, path: &str) -> bool {
        self.drafts.contains_key(path)
    }

    /// The file `pane`'s editor holds, when it has edits not yet saved.
    pub fn unsaved(&self, pane: PaneId) -> Option<String> {
        self.pane(pane)?.code_file.clone().filter(|f| self.dirty(f))
    }

    /// Takes `text` as what `path` now holds on disk; its draft goes unless edited since.
    pub fn saved(&mut self, path: &str, text: &SharedString) {
        if self.drafts.get(path) == Some(text) {
            self.drafts.remove(path);
        }
        for f in self.panes.values_mut().filter(|f| f.file.as_deref() == Some(path)) {
            f.preview = Some(Preview::Text(text.to_string()));
        }
    }

    /// Drops `path`'s draft, so the editors show the file on disk again.
    pub fn discard(&mut self, path: &str) {
        self.drafts.remove(path);
        for f in self.panes.values_mut() {
            f.stale = true;
        }
    }

    /// Forgets the files of panes that closed; their unsaved edits stay.
    pub fn retain_panes(&mut self, panes: &[PaneId]) {
        self.panes.retain(|p, _| panes.contains(p));
    }
}

impl Desktop {
    fn edited(&mut self, pane: PaneId, text: SharedString, cx: &mut Context<Self>) {
        if let Some(file) = self.preview.edit(pane, text) {
            self.pin_doc(&Doc::File(file), cx);
            cx.notify();
        }
        if self.store.files.autosave == Autosave::AfterDelay && self.preview.unsaved(pane).is_some() {
            self.preview.autosave = Some(cx.spawn(async move |this, cx| {
                cx.background_executor().timer(Duration::from_secs(1)).await;
                this.update(cx, |d, cx| {
                    let files: Vec<String> = d.preview.drafts.keys().cloned().collect();
                    for file in files {
                        d.save_file(file, false, cx);
                    }
                })
                .ok();
            }));
        }
    }

    pub fn save(&mut self, _: &crate::actions::Save, _: &mut Window, cx: &mut Context<Self>) {
        let Some(Doc::File(path)) = self.active_doc() else { return };
        self.pin_doc(&Doc::File(path.clone()), cx);
        self.save_file(path, false, cx);
        cx.notify();
    }

    /// Writes `path`'s draft off the UI thread, then closes its tab if `close`. A failed write keeps both.
    pub(crate) fn save_file(&mut self, path: String, close: bool, cx: &mut Context<Self>) {
        let Some(text) = self.preview.drafts.get(&path).cloned() else {
            if close {
                self.close_doc(&Doc::File(path), cx);
            }
            return;
        };
        let (dest, bytes) = (path.clone(), text.clone());
        let task = cx.background_executor().spawn(async move { std::fs::write(dest, bytes.as_bytes()) });
        cx.spawn(async move |this, cx| {
            let res = task.await;
            this.update(cx, |d, cx| {
                match res {
                    Ok(()) => {
                        d.preview.saved(&path, &text);
                        if close {
                            d.close_doc(&Doc::File(path), cx);
                        }
                        d.refresh_git(cx);
                    }
                    Err(e) => d.error = Some(format!("Couldn't save {}: {e}", basename(&path))),
                }
                cx.notify();
            })
            .ok();
        })
        .detach();
    }

    pub fn load_file(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        let Some(path) = self.preview.file(pane).map(str::to_string) else { return };
        let changed = self.file_status(&path).is_some();
        let task = cx.background_executor().spawn(async move { load(&path, changed) });
        cx.spawn(async move |this, cx| {
            let file = task.await;
            this.update(cx, |d, cx| {
                if d.preview.apply(pane, file) {
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub fn file_view(&mut self, pane: PaneId, cx: &mut Context<Self>) -> Div {
        let Some(path) = self.preview.file(pane).map(str::to_string) else { return div() };
        let header = self.file_header(pane, &path, cx);
        let Some(FilePane { views: Some(views), marks, .. }) = self.preview.pane(pane) else { return div() };
        let body = match self.preview.body(pane) {
            Body::Markdown => offers_chat(pane, markdown_pane(views, cx), cx).into_any_element(),
            Body::Code => offers_chat(pane, code_pane(&views.code, marks.clone(), self.store.files.font_size as f32), cx).into_any_element(),
            Body::Image => frame()
                .p(px(24.))
                .flex()
                .items_center()
                .justify_center()
                .bg(SURFACE_SUNKEN)
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

    /// Loads each pane's file into its code editor after it changed, keeping the scroll position when the same file refreshes.
    pub fn sync_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panes: Vec<PaneId> = self.preview.panes.keys().copied().collect();
        let look @ (wrap, numbers, tab) = look(&self.store.files);
        for pane in panes {
            if self.preview.panes.get(&pane).is_some_and(|f| f.views.is_none()) {
                let views = Views::new(pane, look, window, cx);
                self.preview.panes.entry(pane).or_default().views = Some(views);
            }
            if let Some(FilePane { views: Some(views), .. }) = self.preview.panes.get_mut(&pane)
                && views.look != look
            {
                views.look = look;
                views.code.update(cx, |s, cx| {
                    s.set_soft_wrap(wrap, window, cx);
                    s.set_line_number(numbers, window, cx);
                    s.set_tab_size(TabSize { tab_size: tab, hard_tabs: false }, cx);
                });
            }
            let Some(CodeSync { text, language, same, reload }) = self.preview.take_sync(pane) else { continue };
            let Some(FilePane { views: Some(views), code_text, .. }) = self.preview.pane(pane) else { continue };
            views.code.update(cx, |s, cx| {
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
            if reload {
                views.md.update(cx, |md, cx| {
                    md.set_text(code_text, cx);
                    if !same {
                        md.list_state().scroll_to(ListOffset::default());
                    }
                });
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Body, FilePane, Mark, Preview, PreviewState, classify, decode, language, preview, size};
    use crate::desktop::MAIN;
    use git::{Kind, Line};
    use workspace::tree::PaneId;

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

    const OTHER: PaneId = 1;

    fn opened(path: &str) -> PreviewState<()> {
        let mut state = PreviewState::default();
        state.open(MAIN, path.into(), false);
        state
    }

    fn main(state: &mut PreviewState<()>) -> &mut FilePane<()> {
        state.panes.get_mut(&MAIN).unwrap()
    }

    fn text(t: &str) -> Preview {
        Preview::Text(t.into())
    }

    #[test]
    fn a_load_for_a_file_no_longer_shown_is_dropped() {
        let mut state = opened("/r/b.rs");
        assert!(!state.apply(MAIN, ("/r/a.rs".into(), text("a"), Vec::new())));
        let f = main(&mut state);
        assert_eq!((f.file.as_deref(), f.preview.clone()), (Some("/r/b.rs"), None));
    }

    #[test]
    fn a_load_for_the_shown_file_changes_it_only_when_its_content_differs() {
        let mut state = opened("/r/a.rs");
        main(&mut state).stale = false;
        assert!(state.apply(MAIN, ("/r/a.rs".into(), text("a"), Vec::new())));
        let f = main(&mut state);
        assert_eq!((f.preview.clone(), f.stale), (Some(text("a")), true));
        f.stale = false;
        assert!(!state.apply(MAIN, ("/r/a.rs".into(), text("a"), Vec::new())));
        assert!(!main(&mut state).stale);
    }

    #[test]
    fn opening_another_file_clears_the_last_one_but_reopening_keeps_it() {
        let mut state = opened("/r/a.md");
        state.apply(MAIN, ("/r/a.md".into(), text("# a"), Vec::new()));
        let f = main(&mut state);
        (f.md_source, f.stale) = (true, false);
        f.open("/r/a.md".into(), false);
        assert_eq!((f.preview.clone(), f.md_source, f.stale), (Some(text("# a")), true, false));
        f.open("/r/b.md".into(), false);
        assert_eq!((f.file.as_deref(), f.preview.clone(), f.md_source, f.stale), (Some("/r/b.md"), None, false, true));
    }

    #[test]
    fn markdown_opens_as_source_when_the_settings_ask() {
        let mut f = FilePane::<()>::default();
        f.open("/r/a.md".into(), true);
        assert!(f.md_source);
    }

    fn body_of(path: &str, preview: Preview) -> Body {
        let mut state = opened(path);
        state.apply(MAIN, (path.into(), preview, Vec::new()));
        state.body(MAIN)
    }

    #[test]
    fn markdown_text_renders_unless_its_source_is_asked_for() {
        let mut state = opened("/r/README.md");
        state.apply(MAIN, ("/r/README.md".into(), text("# hi"), Vec::new()));
        assert_eq!((main(&mut state).markdown(), state.body(MAIN)), (true, Body::Markdown));
        main(&mut state).md_source = true;
        assert_eq!(state.body(MAIN), Body::Code);
        assert_eq!(body_of("/r/a.rs", text("fn a() {}")), Body::Code);
        assert_eq!(body_of("/r/x.md", Preview::Binary(4)), Body::Note("Binary file · 4 B".into()));
    }

    #[test]
    fn files_without_text_show_the_image_or_say_why_not_and_nothing_while_loading() {
        assert_eq!(body_of("/r/logo.png", Preview::Image), Body::Image);
        assert_eq!(body_of("/r/big.log", Preview::TooLarge(3 * 1024 * 1024)), Body::Note("Too large to preview · 3.0 MB".into()));
        assert_eq!(body_of("/r/gone.rs", Preview::Unreadable), Body::Note("This file can't be shown.".into()));
        assert_eq!(opened("/r/a.rs").body(MAIN), Body::Blank);
    }

    #[test]
    fn a_newly_shown_file_syncs_once_with_its_grammar_and_from_the_top() {
        let mut state = opened("/r/a.rs");
        state.apply(MAIN, ("/r/a.rs".into(), text("fn a() {}"), Vec::new()));
        let sync = state.take_sync(MAIN).unwrap();
        assert_eq!((sync.text.as_ref(), sync.language, sync.same, sync.reload), ("fn a() {}", "rust", false, true));
        assert!(state.take_sync(MAIN).is_none());
    }

    #[test]
    fn a_refreshed_file_keeps_its_place_and_reloads_only_when_its_text_changed() {
        let mut state = opened("/r/a.rs");
        state.apply(MAIN, ("/r/a.rs".into(), text("a\nb\n"), Vec::new()));
        state.take_sync(MAIN);
        let added = vec![Line { kind: Kind::Add, old: None, new: Some(2), text: "b".into() }];
        state.apply(MAIN, ("/r/a.rs".into(), text("a\nb\n"), added.clone()));
        let sync = state.take_sync(MAIN).unwrap();
        assert_eq!((sync.same, sync.reload), (true, false));
        assert_eq!(*main(&mut state).marks, [(2, Mark::Added)]);
        state.apply(MAIN, ("/r/a.rs".into(), text("a\nc\n"), added));
        let sync = state.take_sync(MAIN).unwrap();
        assert_eq!((sync.text.as_ref(), sync.same, sync.reload), ("a\nc\n", true, true));
    }

    fn show(state: &mut PreviewState<()>, pane: PaneId, path: &str, disk: &str) {
        state.open(pane, path.into(), false);
        state.apply(pane, (path.into(), text(disk), Vec::new()));
        state.take_sync(pane);
    }

    fn shown(path: &str, disk: &str) -> PreviewState<()> {
        let mut state = PreviewState::default();
        show(&mut state, MAIN, path, disk);
        state
    }

    #[test]
    fn an_edit_turns_the_file_dirty_once_and_undoing_it_turns_it_clean() {
        let mut state = shown("/r/a.rs", "a\n");
        assert_eq!(state.edit(MAIN, "ab\n".into()), Some("/r/a.rs".into()));
        assert_eq!(state.edit(MAIN, "abc\n".into()), None);
        assert!(state.dirty("/r/a.rs"));
        assert_eq!(state.edit(MAIN, "a\n".into()), Some("/r/a.rs".into()));
        assert!(!state.dirty("/r/a.rs"));
    }

    #[test]
    fn unsaved_edits_outlive_refreshes_and_switching_files() {
        let mut state = shown("/r/a.rs", "a\n");
        state.edit(MAIN, "ab\n".into());
        state.apply(MAIN, ("/r/a.rs".into(), text("changed on disk\n"), Vec::new()));
        let sync = state.take_sync(MAIN).unwrap();
        assert_eq!((sync.text.as_ref(), sync.reload), ("ab\n", false));
        state.open(MAIN, "/r/b.rs".into(), false);
        state.apply(MAIN, ("/r/b.rs".into(), text("b\n"), Vec::new()));
        assert_eq!(state.take_sync(MAIN).unwrap().text.as_ref(), "b\n");
        state.open(MAIN, "/r/a.rs".into(), false);
        let sync = state.take_sync(MAIN).unwrap();
        assert_eq!((sync.text.as_ref(), sync.reload), ("ab\n", true));
        assert!(state.dirty("/r/a.rs") && !state.dirty("/r/b.rs"));
    }

    #[test]
    fn an_unsaved_edit_stays_shown_when_its_file_turns_unreadable() {
        let mut state = shown("/r/a.rs", "a\n");
        state.edit(MAIN, "ab\n".into());
        state.apply(MAIN, ("/r/a.rs".into(), Preview::Unreadable, Vec::new()));
        assert_eq!(state.body(MAIN), Body::Code);
        assert_eq!(state.take_sync(MAIN).unwrap().text.as_ref(), "ab\n");
    }

    #[test]
    fn a_save_cleans_the_file_unless_it_was_edited_while_writing() {
        let mut state = shown("/r/a.rs", "a\n");
        state.edit(MAIN, "ab\n".into());
        state.saved("/r/a.rs", &"ab\n".into());
        assert!(!state.dirty("/r/a.rs"));
        state.edit(MAIN, "abc\n".into());
        state.edit(MAIN, "abcd\n".into());
        state.saved("/r/a.rs", &"abc\n".into());
        assert!(state.dirty("/r/a.rs"));
        assert_eq!(state.edit(MAIN, "abc\n".into()), Some("/r/a.rs".into()));
    }

    #[test]
    fn autosave_finds_a_pane_unsaved_only_while_its_file_differs_from_disk() {
        let mut state = shown("/r/a.rs", "a\n");
        assert_eq!(state.unsaved(MAIN), None);
        state.edit(MAIN, "ab\n".into());
        assert_eq!(state.unsaved(MAIN).as_deref(), Some("/r/a.rs"));
        state.edit(MAIN, "a\n".into());
        assert_eq!(state.unsaved(MAIN), None);
    }

    #[test]
    fn discarding_shows_the_file_on_disk_again() {
        let mut state = shown("/r/a.rs", "a\n");
        state.edit(MAIN, "ab\n".into());
        state.discard("/r/a.rs");
        let sync = state.take_sync(MAIN).unwrap();
        assert_eq!((sync.text.as_ref(), sync.reload), ("a\n", true));
        assert!(!state.dirty("/r/a.rs"));
    }

    #[test]
    fn two_panes_show_two_files_but_share_one_draft_per_file() {
        let mut state = shown("/r/a.rs", "a\n");
        show(&mut state, OTHER, "/r/b.rs", "b\n");
        state.edit(MAIN, "ab\n".into());
        assert_eq!((state.file(MAIN), state.file(OTHER)), (Some("/r/a.rs"), Some("/r/b.rs")));
        state.open(OTHER, "/r/a.rs".into(), false);
        state.apply(OTHER, ("/r/a.rs".into(), text("a\n"), Vec::new()));
        assert_eq!(state.take_sync(OTHER).unwrap().text.as_ref(), "ab\n");
    }

    #[test]
    fn a_load_lands_only_in_the_pane_showing_its_file() {
        let mut state = opened("/r/a.rs");
        state.open(OTHER, "/r/b.rs".into(), false);
        assert!(!state.apply(OTHER, ("/r/a.rs".into(), text("a"), Vec::new())));
        assert!(state.apply(MAIN, ("/r/a.rs".into(), text("a"), Vec::new())));
        assert_eq!(state.pane(OTHER).unwrap().preview, None);
    }

    #[test]
    fn an_edit_in_one_pane_reloads_another_holding_that_file() {
        let mut state = shown("/r/a.rs", "a\n");
        show(&mut state, OTHER, "/r/a.rs", "a\n");
        state.edit(MAIN, "ab\n".into());
        assert!(state.take_sync(MAIN).is_none());
        let sync = state.take_sync(OTHER).unwrap();
        assert_eq!((sync.text.as_ref(), sync.same, sync.reload), ("ab\n", true, true));
    }

    #[test]
    fn a_closed_panes_file_goes_but_its_unsaved_edit_stays() {
        let mut state = shown("/r/a.rs", "a\n");
        show(&mut state, OTHER, "/r/b.rs", "b\n");
        state.edit(OTHER, "bc\n".into());
        state.retain_panes(&[MAIN]);
        assert_eq!(state.file(OTHER), None);
        assert!(state.dirty("/r/b.rs"));
    }
}
