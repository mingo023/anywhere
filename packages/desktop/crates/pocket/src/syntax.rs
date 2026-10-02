use gpui_kit::HighlightStyle;
use gpui_kit::component::highlighter::{LanguageRegistry, SyntaxHighlighter};
use gpui_kit::component::input::Rope;
use std::ops::Range;

pub type Spans = Vec<(Range<usize>, HighlightStyle)>;

/// tree-sitter-javascript's JSX query colours only lowercase tags, which leaves components like `<App>` and `<Foo.Bar>` bare.
const JSX_COMPONENTS: &str = r#"
(jsx_opening_element name: (identifier) @tag (#match? @tag "^[A-Z]"))
(jsx_closing_element name: (identifier) @tag (#match? @tag "^[A-Z]"))
(jsx_self_closing_element name: (identifier) @tag (#match? @tag "^[A-Z]"))
(jsx_opening_element name: (member_expression object: (identifier) @tag property: (property_identifier) @tag))
(jsx_closing_element name: (member_expression object: (identifier) @tag property: (property_identifier) @tag))
(jsx_self_closing_element name: (member_expression object: (identifier) @tag property: (property_identifier) @tag))
"#;

/// Gives tsx the full TypeScript highlights plus JSX tags; gpui-kit ships it with only tree-sitter-typescript's small TS add-on query.
pub fn init() {
    let registry = LanguageRegistry::singleton();
    let (Some(ts), Some(mut tsx)) = (registry.language("typescript"), registry.language("tsx")) else { return };
    tsx.highlights = format!("{}\n{}\n{JSX_COMPONENTS}", ts.highlights, tree_sitter_javascript::JSX_HIGHLIGHT_QUERY).into();
    tsx.injections = ts.injections;
    tsx.injection_languages = ts.injection_languages;
    registry.register("tsx", &tsx);
}

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
    for (r, style) in hl.styles(&(0..text.len()), &*theme::highlight_theme(theme::is_dark())) {
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
    use std::ops::Range;
    use theme::{SYN_COMMENT, SYN_FN, SYN_KEYWORD, SYN_STRING, Token};

    #[test]
    fn colours_each_line_by_its_own_offsets() {
        let spans = line_spans("rust", "fn main() {\n    // hi\n}\n");
        assert_eq!(spans.len(), 3);
        let has = |line: usize, range: Range<usize>, c: Token| spans[line].iter().any(|(r, s)| *r == range && s.color == Some(c.into()));
        assert!(has(0, 0..2, SYN_KEYWORD));
        assert!(has(0, 3..7, SYN_FN));
        assert!(has(1, 4..9, SYN_COMMENT));
    }

    #[test]
    fn colours_tsx_keywords_strings_and_jsx_tags() {
        super::init();
        let spans = line_spans("tsx", "const a = \"x\";\nreturn <div className=\"y\">{'z'}<App /></div>;\n<Foo.Bar />;\n");
        let has = |line: usize, range: Range<usize>, c: Token| spans[line].iter().any(|(r, s)| *r == range && s.color == Some(c.into()));
        assert!(has(0, 0..5, SYN_KEYWORD) && has(1, 0..6, SYN_KEYWORD), "const, return");
        assert!(has(0, 10..13, SYN_STRING) && has(1, 22..25, SYN_STRING) && has(1, 27..30, SYN_STRING), "strings");
        assert!(has(1, 8..11, SYN_FN) && has(1, 40..43, SYN_FN), "div tags");
        assert!(has(1, 12..21, SYN_KEYWORD), "className");
        assert!(has(1, 32..35, SYN_FN) && has(2, 1..4, SYN_FN) && has(2, 5..8, SYN_FN), "component tags");
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

