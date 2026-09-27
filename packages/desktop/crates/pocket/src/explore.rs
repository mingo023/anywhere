use crate::view::{ago_long, basename, empty, id, list_dir, now_ms};
use crate::{Desktop, Overlay, Side};
use git::{Kind, Line};
use gpui_kit::*;
use std::collections::HashMap;
use std::ops::Range;
use std::path::Path;
use theme::*;
use ui::{self, Variant, dot};

const MAX_BYTES: u64 = 512 * 1024;
const MAX_LINES: usize = 3000;

const KEYWORDS: &[&str] = &[
    "as", "async", "await", "break", "case", "catch", "class", "const", "continue", "def", "default", "else", "enum", "export", "extends", "false",
    "fn", "for", "from", "function", "if", "impl", "import", "in", "interface", "let", "loop", "match", "mod", "mut", "new", "null", "of", "pub",
    "return", "self", "static", "struct", "switch", "throw", "trait", "true", "try", "type", "undefined", "use", "var", "where", "while",
];

/// A file's text (None when too big or not UTF-8) and its diff against HEAD when git reports it changed.
pub fn load(path: &str, changed: bool) -> (String, Option<String>, Vec<Line>) {
    let small = std::fs::metadata(path).is_ok_and(|m| m.len() <= MAX_BYTES);
    let text = small.then(|| std::fs::read_to_string(path).ok()).flatten();
    let dir = Path::new(path).parent().map(|p| p.to_string_lossy().into_owned()).unwrap_or_default();
    let diff = if changed { git::file_diff(&dir, path) } else { Vec::new() };
    (path.to_string(), text, diff)
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

/// Colour ranges for one line of code: keywords, strings, comments, numbers and called functions.
pub fn highlight(line: &str) -> Vec<(Range<usize>, u32)> {
    let b = line.as_bytes();
    let word = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$';
    let mut out = Vec::new();
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if line[i..].starts_with("//") {
            out.push((i..b.len(), SYN_COMMENT));
            break;
        }
        if matches!(c, b'"' | b'\'' | b'`') {
            let mut j = i + 1;
            while j < b.len() && b[j] != c {
                j += if b[j] == b'\\' { 2 } else { 1 };
            }
            let end = (j + 1).min(b.len());
            out.push((i..end, SYN_STRING));
            i = end;
            continue;
        }
        if word(c) {
            let start = i;
            while i < b.len() && word(b[i]) {
                i += 1;
            }
            let token = &line[start..i];
            let next = line[i..].trim_start().bytes().next();
            if c.is_ascii_digit() {
                out.push((start..i, SYN_FN));
            } else if KEYWORDS.contains(&token) {
                out.push((start..i, SYN_KEYWORD));
            } else if next == Some(b'(') || b.get(i) == Some(&b'<') {
                out.push((start..i, SYN_FN));
            }
            continue;
        }
        i += 1;
    }
    out
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
            self.file_text = None;
            self.file_diff.clear();
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
            .mx(px(14.))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.go_to_file(&crate::GoToFile, window, cx)));
        let chips = div()
            .px(px(14.))
            .pt(px(10.))
            .pb(px(4.))
            .flex()
            .gap(px(4.))
            .child(ui::chip("all-files", !self.touched_only).child("All files").on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.touched_only = false;
                cx.notify();
            })))
            .child(
                ui::chip("touched-files", self.touched_only)
                    .child(dot(6., AGENT_CODEX))
                    .child(format!("Touched by agents · {}", touched.len()))
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                        this.touched_only = true;
                        cx.notify();
                    })),
            );
        let mut rows = Vec::new();
        match self.explore_root() {
            Some(root) if self.touched_only => {
                for path in &touched {
                    let label = path.strip_prefix(&root).unwrap_or(path).trim_start_matches('/').to_string();
                    rows.push(self.file_row(path.clone(), label, 0, true, cx));
                }
            }
            Some(root) => self.tree(Path::new(&root), 0, &touched, &mut rows, cx),
            None => {}
        }
        let list = div().id("explorer").flex_1().min_h_0().px(px(8.)).pb(px(8.)).overflow_y_scroll().flex().flex_col().gap(px(1.));
        let list = if rows.is_empty() && self.touched_only { list.child(empty("No files touched by agents yet.")) } else { list.children(rows) };
        div().id("explore-side").flex_1().min_h_0().flex().flex_col().child(go).child(chips).child(list)
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

    pub fn file_view(&mut self, cx: &mut Context<Self>) -> Div {
        let Some(path) = self.file.clone() else { return div() };
        let root = self.explore_root().unwrap_or_default();
        let rel = path.strip_prefix(&root).map(|r| r.trim_start_matches('/').to_string()).unwrap_or_else(|| path.clone());
        let project = self.project.as_deref().map(|p| self.repo_name(p)).unwrap_or_default();
        let crumbs = std::iter::once(project).chain(rel.split('/').map(str::to_string)).collect();
        let name = basename(&path);
        let prompt = format!("About {rel}: ");
        let copy = rel.clone();
        let opened = path.clone();
        let right = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .child(
                ui::button("ask-file", Variant::Ghost, Some("sparkle"), "Ask about this file").text_color(rgba(TEXT)).on_click(cx.listener(
                    move |this, _: &ClickEvent, window, cx| {
                        this.open(Overlay::NewSession, window, cx);
                        this.reset_new_form(Some(prompt.clone()), window, cx);
                    },
                )),
            )
            .child(
                ui::button("open-editor", Variant::Ghost, Some("external"), "Open in editor")
                    .text_color(rgba(TEXT))
                    .on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.open_with_system(Path::new(&opened)))),
            )
            .child(ui::icon_group([
                ui::group_button("copy-path", "copy").on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.write_to_clipboard(ClipboardItem::new_string(copy.clone())))),
                ui::group_button("file-more", "more").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::More, window, cx))),
            ]));
        let text = self.file_text.clone();
        let (status_color, status_label) = status_word(self.file_status(&path));
        let mut meta = div()
            .flex()
            .items_center()
            .child(ui::meta_item().child(icon("file", 13., TEXT_3)).child(language(&path)));
        if let Some(t) = &text {
            meta = meta.child(ui::meta_item().child(ui::meta_value(format!("{} lines · {}", t.lines().count(), size(t.len())))));
        }
        meta = meta.child(ui::meta_item().child(dot(7., status_color)).child(ui::meta_value(status_label)));
        let banner = self.agents.last_edit(&path).map(|(a, ts)| {
            let title = if a.title.is_empty() { "a session".to_string() } else { a.title.clone() };
            div()
                .mx(px(40.))
                .mb(px(14.))
                .h(px(50.))
                .px(px(14.))
                .flex()
                .flex_none()
                .items_center()
                .gap(px(12.))
                .rounded(px(12.))
                .bg(rgba(TEAL_BG))
                .text_size(px(13.5))
                .text_color(rgba(TEXT_BODY))
                .child(div().size(px(26.)).flex().flex_none().items_center().justify_center().rounded(px(8.)).bg(rgba(0xffffffcc)).child(icon("sparkle", 13., TEAL)))
                .child(
                    div()
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .flex()
                        .gap(px(4.))
                        .child(div().font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT)).child(provider_name(&a.provider)))
                        .child(format!("edited this file {} in", ago_long(ts, now_ms())))
                        .child(div().font_weight(FontWeight::SEMIBOLD).text_color(rgba(TEXT)).child(format!("{title}."))),
                )
                .child(ui::link("view-changes", "View changes").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.open_changes(None, cx))))
        });
        let code = match text {
            Some(t) => self.code_box(&t).into_any_element(),
            None => empty("This file can't be shown.").into_any_element(),
        };
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .child(Self::page_bar(crumbs, right))
            .child(Self::title_block(name, if self.wide { 24. } else { 28. }, meta))
            .children(banner)
            .child(code)
    }

    fn code_box(&self, text: &str) -> Stateful<Div> {
        let marks = gutter(&self.file_diff);
        let rows = text.lines().take(MAX_LINES).enumerate().map(|(i, line)| {
            let n = i + 1;
            let bar = marks.get(&n).map(|&modified| if modified { WAITING } else { RUNNING });
            let styled = StyledText::new(SharedString::from(line.to_string()))
                .with_highlights(highlight(line).into_iter().map(|(r, c)| (r, HighlightStyle { color: Some(rgba(c).into()), ..Default::default() })));
            div()
                .relative()
                .h(px(22.))
                .flex()
                .items_center()
                .whitespace_nowrap()
                .children(bar.map(|c| div().absolute().left_0().top_0().bottom_0().w(px(3.)).bg(rgba(c))))
                .child(div().w(px(46.)).flex_none().pr(px(18.)).flex().justify_end().text_color(rgba(TEXT_5)).child(n.to_string()))
                .child(styled)
        });
        div()
            .id("code")
            .flex_1()
            .min_h_0()
            .mx(px(20.))
            .mb(px(20.))
            .py(px(14.))
            .overflow_y_scroll()
            .rounded(px(16.))
            .bg(rgba(SURFACE_SUNKEN))
            .shadow(vec![BoxShadow { inset: true, ..ui::ring(SEPARATOR, 0.5) }])
            .font_family(MONO)
            .text_size(px(13.))
            .text_color(rgba(TEXT))
            .children(rows)
    }
}

#[cfg(test)]
mod tests {
    use super::{gutter, highlight, language, size};
    use git::{Kind, Line};
    use theme::*;

    #[test]
    fn highlights_keywords_strings_calls_and_comments() {
        let line = "const x = useState<Phase>('idle') // note";
        let spans: Vec<(&str, u32)> = highlight(line).into_iter().map(|(r, c)| (&line[r], c)).collect();
        assert_eq!(spans, vec![("const", SYN_KEYWORD), ("useState", SYN_FN), ("'idle'", SYN_STRING), ("// note", SYN_COMMENT)]);
        let line = "  timeout(id, 5000)";
        let spans: Vec<&str> = highlight(line).into_iter().map(|(r, _)| &line[r]).collect();
        assert_eq!(spans, vec!["timeout", "5000"]);
    }

    #[test]
    fn highlight_survives_unterminated_strings_and_unicode() {
        let line = "let s = \"héllo";
        let spans: Vec<&str> = highlight(line).into_iter().map(|(r, _)| &line[r]).collect();
        assert_eq!(spans, vec!["let", "\"héllo"]);
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
    fn names_languages_and_sizes() {
        assert_eq!(language("src/a.tsx"), "TypeScript");
        assert_eq!(language("Makefile"), "Plain text");
        assert_eq!(size(612), "612 B");
        assert_eq!(size(2048), "2.0 KB");
    }
}
