use gpui_kit::HighlightStyle;
use gpui_kit::component::highlighter::SyntaxHighlighter;
use gpui_kit::component::input::Rope;
use std::ops::Range;

pub type Spans = Vec<(Range<usize>, HighlightStyle)>;

/// gpui-kit's grammar name for a file, or "text" when it has none.
pub fn language_for(path: &str) -> &'static str {
    match path.rsplit_once('.').map(|(_, e)| e) {
        Some("rs") => "rust",
        Some("ts" | "mts" | "cts") => "typescript",
        Some("tsx") => "tsx",
        Some("js" | "jsx" | "mjs" | "cjs") => "javascript",
        Some("py") => "python",
        Some("go") => "go",
        Some("swift") => "swift",
        Some("kt" | "kts") => "kotlin",
        Some("java") => "java",
        Some("json") => "json",
        Some("md") => "markdown",
        Some("toml") => "toml",
        Some("yml" | "yaml") => "yaml",
        Some("sh" | "zsh" | "bash") => "bash",
        Some("css" | "scss") => "css",
        Some("html") => "html",
        _ => "text",
    }
}

/// Syntax colours for each line of `text`, as byte ranges within that line. Parses the whole text, so run it off the UI thread.
pub fn line_spans(language: &str, text: &str) -> Vec<Spans> {
    let lines: Vec<&str> = text.lines().collect();
    let starts: Vec<usize> = lines.iter().map(|l| l.as_ptr() as usize - text.as_ptr() as usize).collect();
    let mut hl = SyntaxHighlighter::new(language);
    hl.update(None, &Rope::from(text), None);
    let mut out = vec![Vec::new(); lines.len()];
    for (r, style) in hl.styles(&(0..text.len()), &*theme::highlight_theme()) {
        if style.color.is_none() {
            continue;
        }
        let mut i = starts.partition_point(|&s| s <= r.start).saturating_sub(1);
        while i < lines.len() && starts[i] < r.end {
            let (a, b) = (r.start.max(starts[i]) - starts[i], r.end.min(starts[i] + lines[i].len()).saturating_sub(starts[i]));
            if a < b {
                out[i].push((a..b, style));
            }
            i += 1;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{language_for, line_spans};
    use gpui_kit::rgba;
    use std::ops::Range;
    use theme::{SYN_COMMENT, SYN_FN, SYN_KEYWORD};

    #[test]
    fn colours_each_line_by_its_own_offsets() {
        let spans = line_spans("rust", "fn main() {\n    // hi\n}\n");
        assert_eq!(spans.len(), 3);
        let has = |line: usize, range: Range<usize>, c: u32| spans[line].iter().any(|(r, s)| *r == range && s.color == Some(rgba(c).into()));
        assert!(has(0, 0..2, SYN_KEYWORD));
        assert!(has(0, 3..7, SYN_FN));
        assert!(has(1, 4..9, SYN_COMMENT));
    }

    #[test]
    fn keeps_ranges_inside_crlf_lines_and_plain_text_bare() {
        let spans = line_spans("rust", "fn a() {}\r\nfn b() {}\r\n");
        assert!(spans.iter().flatten().all(|(r, _)| r.end <= 9));
        assert_eq!(line_spans("text", "a\nb"), vec![vec![], vec![]]);
    }

    #[test]
    fn maps_extensions_to_grammars() {
        assert_eq!(language_for("src/a.tsx"), "tsx");
        assert_eq!(language_for("README.md"), "markdown");
        assert_eq!(language_for("Makefile"), "text");
    }
}
