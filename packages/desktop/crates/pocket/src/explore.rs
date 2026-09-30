use crate::view::{ago_long, empty, id, list_dir, now_ms};
use crate::{Desktop, Overlay, Side};
use git::{Kind, Line};
use crate::mermaid::Mermaid;
use crate::syntax::language_for;
use gpui_kit::base::text::CodeBlock;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::component::input::{Editor, TextDecoration};
use gpui_kit::component::text::{TextView, TextViewStyle};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use theme::*;
use ui::{self, Segment, Variant, dot};

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

/// What Explore can show for the file at `path`.
pub fn preview(path: &str) -> Preview {
    let Ok(len) = std::fs::metadata(path).map(|m| m.len()) else { return Preview::Unreadable };
    let ext = path.rsplit_once('.').map(|(_, e)| e.to_ascii_lowercase()).unwrap_or_default();
    let image = IMAGES.contains(&ext.as_str());
    if len > if image { MAX_IMAGE_BYTES } else { MAX_BYTES } {
        return Preview::TooLarge(len);
    }
    if image {
        return Preview::Image;
    }
    std::fs::read(path).map_or(Preview::Unreadable, decode)
}

/// What a file shows as and its diff against HEAD when git reports it changed.
pub fn load(path: &str, changed: bool) -> (String, Preview, Vec<Line>) {
    let preview = preview(path);
    let dir = Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let diff = if changed { git::file_diff(&dir, path) } else { Vec::new() };
    (path.to_string(), preview, diff)
}

pub fn status_word(status: Option<char>) -> (u32, &'static str) {
    match status {
        Some('A') => (RUNNING, "Added"),
        Some('D') => (FAILED, "Deleted"),
        Some(_) => (WAITING, "Modified"),
        None => (TEXT_5, "Unchanged"),
    }
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

/// New-side line numbers that git marks as changed: true where a line replaced another, false where it was only added.
pub fn gutter(lines: &[Line]) -> HashMap<usize, bool> {
    git::split(lines)
        .into_iter()
        .filter_map(|pair| match pair {
            (Some(l), Some(r)) if l != r => Some((lines[r].new?, true)),
            (None, Some(r)) if lines[r].kind == Kind::Add => Some((lines[r].new?, false)),
            _ => None,
        })
        .collect()
}

/// Background tints for the lines git marks as changed: amber where a line was modified, green where it was only added.
pub fn decorations(text: &str, marks: &HashMap<usize, bool>) -> Vec<TextDecoration> {
    text.lines()
        .enumerate()
        .filter_map(|(i, line)| {
            let modified = *marks.get(&(i + 1))?;
            let start = line.as_ptr() as usize - text.as_ptr() as usize;
            let bg = HighlightStyle { background_color: Some(rgba(if modified { WAITING_BG } else { RUNNING_BG }).into()), ..Default::default() };
            Some(TextDecoration::new(start..start + line.len(), bg))
        })
        .collect()
}

fn pane() -> Div {
    div().flex_1().min_h_0().border_t(px(0.5)).border_color(rgba(SEPARATOR))
}

fn heading_size(level: u8) -> Pixels {
    px(match level {
        1 => 22.,
        2 => 18.,
        3 => 16.,
        _ => 14.,
    })
}

fn markdown_style(highlight_theme: Arc<HighlightTheme>) -> TextViewStyle {
    TextViewStyle {
        highlight_theme,
        heading_font_size: Some(Arc::new(|level, _| heading_size(level))),
        inline_code: HighlightStyle { background_color: Some(rgba(HAIRLINE).into()), ..Default::default() },
        code_block: StyleRefinement::default().border_1().border_color(rgba(SEPARATOR)).rounded(px(10.)).pt(px(36.)).px(px(12.)).pb(px(10.)).text_size(px(12.)),
        table: StyleRefinement::default().bg(rgba(WINDOW)).border_color(rgba(SEPARATOR)).rounded(px(10.)),
        table_head: StyleRefinement::default().bg(transparent_black()).text_color(rgba(TEXT)).font_weight(FontWeight::SEMIBOLD),
        table_cell: StyleRefinement::default().px(px(10.)).py(px(8.)).text_size(px(12.)),
        ..Default::default()
    }
}

fn code_actions(block: &CodeBlock) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .children(block.lang().map(|lang| div().font_family(MONO).text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(rgba(TEXT_2)).child(lang.to_lowercase())))
        .child(Clipboard::new("copy").value(block.code()))
}

impl Desktop {
    /// The folder Explore browses: the chosen worktree, else the project.
    pub fn explore_root(&self) -> Option<String> {
        self.worktree.clone().or_else(|| self.project.clone())
    }

    /// Git's letter for a file under the explore root, if it has changed.
    pub fn file_status(&self, path: &str) -> Option<char> {
        let root = self.explore_root()?;
        let rel = path.strip_prefix(&root)?.trim_start_matches('/');
        self.repos.get(&root)?.files.iter().find(|f| f.path == rel).map(|f| f.status)
    }

    fn touched(&self) -> Vec<String> {
        let Some(root) = self.explore_root() else { return Vec::new() };
        let files = self.repos.get(&root).map(|r| r.files.clone()).unwrap_or_default();
        files.into_iter().map(|f| format!("{root}/{}", f.path)).filter(|p| self.agents.last_edit(p).is_some()).collect()
    }

    pub fn open_file(&mut self, path: String, cx: &mut Context<Self>) {
        if self.file.as_ref() != Some(&path) {
            self.file = Some(path);
            self.file_preview = None;
            self.file_diff.clear();
            self.code_stale = true;
            self.md_source = false;
        }
        self.side = Side::Explorer;
        self.refresh_git(cx);
        cx.notify();
    }

    pub fn go_to_file(&mut self, _: &crate::GoToFile, window: &mut Window, cx: &mut Context<Self>) {
        self.palette_all = false;
        self.open(Overlay::Palette, window, cx);
    }

    pub fn explorer(&mut self, cx: &mut Context<Self>) -> Stateful<Div> {
        let touched = self.touched();
        let go = ui::trigger_field("go-to-file", "search", "Go to file…", "⌘P")
            .mx(px(8.))
            .mt(px(8.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.go_to_file(&crate::GoToFile, window, cx)));
        let mut rows = Vec::new();
        if let Some(root) = self.explore_root() {
            self.tree(Path::new(&root), 0, &touched, &mut rows, cx);
        }
        let list = div().id("explorer").flex_1().min_h_0().px(px(8.)).pt(px(8.)).pb(px(8.)).overflow_y_scroll().flex().flex_col().gap(px(1.)).children(rows);
        div().id("explore-side").flex_1().min_h_0().flex().flex_col().child(go).child(list)
    }

    fn file_row(&self, path: String, label: String, depth: usize, touched: bool, cx: &mut Context<Self>) -> Stateful<Div> {
        let selected = self.file.as_ref() == Some(&path);
        let git = self.file_status(&path);
        ui::tree_row(id(format!("tree-{path}")), label, false, false, depth, selected, touched, git)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_file(path.clone(), cx)))
    }

    fn tree(&self, dir: &Path, depth: usize, touched: &[String], rows: &mut Vec<Stateful<Div>>, cx: &mut Context<Self>) {
        for (is_dir, path) in self.tree.get(dir).cloned().unwrap_or_default() {
            let label = path.file_name().unwrap_or_default().to_string_lossy().to_string();
            let key = path.to_string_lossy().to_string();
            if !is_dir {
                rows.push(self.file_row(key.clone(), label, depth, touched.contains(&key), cx));
                continue;
            }
            let open = self.tree.contains_key(&path);
            let target = path.clone();
            rows.push(ui::tree_row(id(format!("tree-{key}")), label, true, open, depth, false, false, None).on_click(cx.listener(
                move |this, _: &ClickEvent, _, cx| {
                    if this.tree.remove(&target).is_none() {
                        this.tree.insert(target.clone(), list_dir(&target));
                    }
                    cx.notify();
                },
            )));
            if open {
                self.tree(&path, depth + 1, touched, rows, cx);
            }
        }
    }

    fn copy_path(&mut self, path: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
        let reset = cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(1500)).await;
            this.update(cx, |this, cx| {
                this.path_copied = None;
                cx.notify();
            })
            .ok();
        });
        self.path_copied = Some((path.to_string(), reset));
        cx.notify();
    }

    pub fn file_view(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(path) = self.file.clone() else { return div() };
        let root = self.explore_root().unwrap_or_default();
        let rel = path.strip_prefix(&root).map(|r| r.trim_start_matches('/').to_string()).unwrap_or_else(|| path.clone());
        let project = self.project.as_deref().map(|p| self.repo_name(p)).unwrap_or_default();
        let crumbs = std::iter::once(project).chain(rel.split('/').map(str::to_string)).collect();
        let prompt = format!("About {rel}: ");
        let copy = rel.clone();
        let opened = path.clone();
        let text = match &self.file_preview {
            Some(Preview::Text(t)) => Some(t.as_str()),
            _ => None,
        };
        let markdown = text.is_some() && language_for(&path) == "markdown";
        let right = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .when(markdown, |d| {
                d.child(div().id("md-mode").child(ui::segmented(
                    vec![Segment { icon: None, value: false, label: "Preview".into(), badge: None }, Segment { icon: None, value: true, label: "Source".into(), badge: None }],
                    self.md_source,
                    true,
                    false,
                    |this, v, cx| {
                        this.md_source = v;
                        cx.notify();
                    },
                    cx,
                )))
            })
            .child(
                ui::button("ask-file", Variant::Ghost, Some("sparkle"), "Ask about this file").text_color(rgba(TEXT)).on_click(cx.listener(
                    move |this, _: &ClickEvent, window, cx| {
                        this.open(Overlay::NewSession, window, cx);
                        this.reset_new_form(Some(prompt.clone()), false, window, cx);
                    },
                )),
            )
            .child(ui::icon_group([
                ui::group_button("open-editor", "external").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_with_system(Path::new(&opened)))),
                match self.path_copied.as_ref().is_some_and(|(p, _)| *p == copy) {
                    true => ui::group_button_swapped("copy-path", "check"),
                    false => ui::group_button("copy-path", "copy"),
                }
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.copy_path(&copy, cx))),
                ui::group_button("file-more", "more").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::More, window, cx))),
            ]));
        let (status_color, status_label) = status_word(self.file_status(&path));
        let mut meta = vec![ui::meta_item().child(icon("file", 13., TEXT_2)).child(language(&path)).into_any_element()];
        if let Some(t) = text {
            meta.push(ui::meta_item().child(ui::meta_value(format!("{} lines · {}", t.lines().count(), size(t.len())))).into_any_element());
        }
        let status = ui::meta_item().id("meta-status").child(dot(7., status_color)).child(ui::meta_value(status_label));
        meta.push(match self.agents.last_edit(&path) {
            Some((a, ts)) => status
                .cursor_pointer()
                .child(format!("by {} · {}", provider_name(&a.provider), ago_long(ts, now_ms())))
                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_changes(None, cx)))
                .into_any_element(),
            None => status.into_any_element(),
        });
        let code = match text {
            Some(_) if markdown && !self.md_source => pane()
                .px(px(40.))
                .py(px(32.))
                .bg(rgba(SURFACE))
                .child(
                    TextView::new(&self.md)
                        .plugin(Mermaid(self.diagrams.clone()))
                        .code_block_actions(|block, _, _| code_actions(block))
                        .selectable(true)
                        .scrollable(true)
                        .style(markdown_style(cx.theme().highlight_theme.clone()))
                        .size_full(),
                )
                .into_any_element(),
            Some(_) => pane()
                .bg(rgba(SURFACE_SUNKEN))
                .child(Editor::new(&self.code).readonly(true).bordered(false).size_full().font_family(MONO).text_size(px(13.)).line_height(px(22.)))
                .into_any_element(),
            None => match &self.file_preview {
                Some(Preview::Image) => pane()
                    .p(px(24.))
                    .flex()
                    .items_center()
                    .justify_center()
                    .bg(rgba(SURFACE_SUNKEN))
                    .child(img(PathBuf::from(&path)).max_w_full().max_h_full().object_fit(ObjectFit::Contain))
                    .into_any_element(),
                Some(Preview::Binary(n)) => empty(format!("Binary file · {}", size(*n as usize))).into_any_element(),
                Some(Preview::TooLarge(n)) => empty(format!("Too large to preview · {}", size(*n as usize))).into_any_element(),
                Some(Preview::Unreadable) => empty("This file can't be shown.").into_any_element(),
                Some(Preview::Text(_)) | None => div().flex_1().into_any_element(),
            },
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(self.page_bar(crumbs, meta, right, cx))
            .child(code)
    }

    /// Loads the open file into the code editor after it changed, keeping the scroll position when the same file refreshes.
    pub fn sync_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !std::mem::take(&mut self.code_stale) {
            return;
        }
        let text = match &self.file_preview {
            Some(Preview::Text(t)) => SharedString::from(t.clone()),
            _ => SharedString::default(),
        };
        let same = self.code_file == self.file;
        let reload = !same || text != self.code_text;
        let language = language_for(self.file.as_deref().unwrap_or_default());
        let marks = decorations(&text, &gutter(&self.file_diff));
        self.code_file = self.file.clone();
        self.code_text = text.clone();
        self.code.update(cx, |s, cx| {
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
        self.code_marks.set(marks, cx);
        if reload {
            self.md.update(cx, |md, cx| {
                md.set_text(&self.code_text, cx);
                if !same {
                    md.list_state().scroll_to(ListOffset::default());
                }
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Preview, decode, decorations, gutter, heading_size, language, preview, size};
    use git::{Kind, Line};
    use gpui_kit::{px, rgba};
    use std::collections::HashMap;
    use theme::{RUNNING_BG, WAITING_BG};

    #[test]
    fn tints_changed_lines() {
        let d = decorations("a\nbb\nccc\n", &HashMap::from([(2, true), (3, false)]));
        assert_eq!(d.iter().map(|d| d.range.clone()).collect::<Vec<_>>(), vec![2..4, 5..8]);
        assert_eq!(d[0].style.background_color, Some(rgba(WAITING_BG).into()));
        assert_eq!(d[1].style.background_color, Some(rgba(RUNNING_BG).into()));
    }

    #[test]
    fn gutter_tells_modified_from_added() {
        let l = |kind, old, new| Line { kind, old, new, text: String::new() };
        let lines = vec![
            l(Kind::Hunk, None, None),
            l(Kind::Context, Some(1), Some(1)),
            l(Kind::Del, Some(2), None),
            l(Kind::Add, None, Some(2)),
            l(Kind::Add, None, Some(3)),
        ];
        let marks = gutter(&lines);
        assert_eq!(marks.get(&2), Some(&true));
        assert_eq!(marks.get(&3), Some(&false));
        assert_eq!(marks.get(&1), None);
    }

    #[test]
    fn headings_shrink_with_depth() {
        assert_eq!((1..=6).map(heading_size).collect::<Vec<_>>(), [22., 18., 16., 14., 14., 14.].map(px));
    }

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
}
