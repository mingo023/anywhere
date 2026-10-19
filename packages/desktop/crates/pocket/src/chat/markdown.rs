use gpui_kit::base::text::{InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin, TextView, TextViewMotion, TextViewStyle, markdown_ast};
use gpui_kit::component::highlighter::SyntaxHighlighter;
use gpui_kit::component::input::Rope;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use theme::*;

pub(super) type Styles = Arc<Vec<(Range<usize>, HighlightStyle)>>;

/// A fenced code block's language and code; `None` for any other markdown. An unclosed fence (still streaming) runs to the end.
pub(crate) fn fence(text: &str) -> Option<(&str, &str)> {
    let text = text.trim();
    let marker = ["```", "~~~"].into_iter().find(|m| text.starts_with(m))?;
    let (info, body) = text.split_once('\n').unwrap_or((text, ""));
    let lang = info.trim_start_matches(marker.chars().next()?).split_whitespace().next().unwrap_or("");
    let code = match body.rfind('\n') {
        Some(i) if body[i + 1..].trim().starts_with(marker) => &body[..i],
        _ if body.trim().starts_with(marker) => "",
        _ => body,
    };
    Some((lang, code))
}

/// Code-block highlights by row id, for the code length and appearance they were computed for.
#[derive(Default)]
pub(super) struct Highlights(HashMap<String, ((usize, bool), Option<Styles>)>);

impl Highlights {
    /// The styles for a row's `code` in a light or dark appearance; true when it needs a new run.
    /// Code that changed length (a block still streaming) keeps the old run's colours until its own run lands.
    pub(super) fn get(&mut self, row: &str, code: &str, dark: bool) -> (Option<Styles>, bool) {
        let old = match self.0.get(row) {
            Some((of, styles)) if *of == (code.len(), dark) => return (styles.clone(), false),
            Some(((_, was_dark), Some(styles))) if *was_dark == dark => Some(kept(styles, code)),
            _ => None,
        };
        self.0.insert(row.to_string(), ((code.len(), dark), old.clone()));
        (old, true)
    }

    pub(super) fn set(&mut self, row: &str, len: usize, dark: bool, styles: Styles) {
        if let Some(entry) = self.0.get_mut(row).filter(|(of, _)| *of == (len, dark)) {
            entry.1 = Some(styles);
        }
    }

    pub(super) fn retain(&mut self, keep: impl Fn(&String) -> bool) {
        self.0.retain(|row, _| keep(row));
    }
}

/// The ranges of a run made for other code that still fit `code`: cut at its end, dropping any that would split a character.
fn kept(styles: &Styles, code: &str) -> Styles {
    let fits = |r: &Range<usize>| r.start < r.end && code.is_char_boundary(r.start) && code.is_char_boundary(r.end);
    Arc::new(styles.iter().map(|(r, s)| (r.start..r.end.min(code.len()), *s)).filter(|(r, _)| fits(r)).collect())
}

/// Runs off the UI thread: tree-sitter parsing a long block takes milliseconds.
pub(super) fn highlight(lang: &str, code: &str, dark: bool) -> Styles {
    let lang = match lang.to_lowercase().as_str() {
        "sh" | "shell" | "zsh" | "console" => "bash".to_string(),
        lang => lang.to_string(),
    };
    let mut hl = SyntaxHighlighter::new(&lang);
    hl.update(None, &Rope::from(code), None);
    Arc::new(hl.styles(&(0..code.len()), &*markdown_highlight_theme(dark)))
}

/// Whether inline `code` names a path, and if so whether a folder: no spaces, and a slash or a name with its own file icon.
fn path_in(code: &str) -> Option<bool> {
    let word = !code.contains(char::is_whitespace) && !code.contains("://") && code.contains(char::is_alphabetic);
    (word && (code.contains('/') || has_file_icon(code))).then(|| code.ends_with('/'))
}

/// Opens what a click in a markdown block points at: a link's URL, or a path as written.
pub(super) type Open = Arc<dyn Fn(&str, &mut Window, &mut App) + Send + Sync>;

/// Where a link points.
#[derive(Debug, PartialEq)]
pub(super) enum Target {
    Web(String),
    File(String),
}

/// A URL opens on the web; anything else is a file, under `cwd` unless absolute, without its `:line:col` or `#L` suffix.
pub(super) fn target(link: &str, cwd: &str) -> Target {
    if link.contains("://") && !link.starts_with("file://") {
        return Target::Web(link.to_string());
    }
    let mut path = link.trim_start_matches("file://").split('#').next().unwrap_or_default();
    for _ in 0..2 {
        if let Some((p, n)) = path.rsplit_once(':')
            && !n.is_empty()
            && n.bytes().all(|b| b.is_ascii_digit())
        {
            path = p;
        }
    }
    Target::File(std::path::Path::new(cwd).join(path).to_string_lossy().into_owned())
}

/// Inline code naming a file or folder, as a chip led by its icon; a file's opens it.
struct Paths(Open);

impl MarkdownPlugin for Paths {
    fn name(&self) -> &str {
        "path"
    }

    fn parse(&self, node: &markdown_ast::Node, _: &MarkdownParseContext<'_>) -> Option<MarkdownNode> {
        let markdown_ast::Node::InlineCode(code) = node else { return None };
        Some(MarkdownNode::new("path", path_in(&code.value)?).text(code.value.clone()))
    }

    fn render_inline(&self, node: &MarkdownNode, cx: &InlineRenderContext, window: &mut Window, _: &mut App) -> Option<InlineElement> {
        let (path, folder) = (node.as_text(), *node.data::<bool>()?);
        // Sized and padded like TextView's own inline code: 0.9em, `padding: 0.25em 0.5em`.
        let size = cx.font_size() * 0.9;
        let text = window.text_system();
        let mono = text.resolve_font(&font(code_font()));
        let (ascent, descent) = (text.ascent(mono, size), px(f32::from(text.descent(mono, size)).abs()));
        let chip = div()
            .flex()
            .items_center()
            .gap(px(4.))
            .px(size * 0.5)
            .py(size * 0.25)
            .rounded(px(5.))
            .bg(FILL_4)
            .font_family(code_font())
            .text_size(size)
            .line_height(ascent + descent)
            .text_color(TEXT)
            .whitespace_nowrap()
            .child(file_icon(path.trim_end_matches('/'), folder, false, 13.))
            .child(path.to_string());
        let chip = chip.id(SharedString::from(format!("path-{path}"))).when(!folder, |c| {
            let (open, path) = (self.0.clone(), path.to_string());
            c.cursor_pointer().hover(|s| s.bg(FILL_3)).on_click(move |_, window, cx| open(&path, window, cx))
        });
        Some(InlineElement::new(chip).with_baseline(size * 0.25 + ascent))
    }
}

fn style() -> TextViewStyle {
    let accent = Hsla::from(ACCENT_FILL);
    // Columns sized to their measured content, so a short one isn't squeezed letter by letter.
    let mut table = StyleRefinement::default();
    table.overflow.x = Some(Overflow::Scroll);
    let quote = StyleRefinement::default().border_l_2().border_color(accent.opacity(0.6)).bg(accent.opacity(0.05)).rounded_r(px(6.)).pl(px(12.)).pr(px(10.)).py(px(6.)).text_color(TEXT);
    TextViewStyle::default()
        .with_foreground(TEXT_BODY.into())
        .with_muted_foreground(TEXT_2.into())
        .with_link(TEXT.into())
        .with_strong(TEXT.into())
        .with_link_underline(TEXT_2.into())
        .with_list_marker(accent)
        .with_list_indent(px(14. * 1.9))
        .with_list_item_gap(px(8.))
        .with_selection(SELECTION.into())
        .with_border(SEPARATOR.into())
        .with_code_background(ink(0.035))
        .with_code_border(SEPARATOR.into())
        .with_aside_background(ink(0.035))
        .with_inline_code(HighlightStyle { color: Some(TEXT.into()), background_color: Some(FILL_4.into()), ..Default::default() })
        .with_blockquote(quote)
        .with_table(table)
        .with_table_cell(StyleRefinement::default().p(px(12.)))
        .with_table_head(StyleRefinement::default().text_color(TEXT))
        .with_table_inner_rules()
        .with_code_font(code_font())
        // 16px between blocks; GPUI's rem is 16px.
        .with_paragraph_gap(rems(1.))
        .with_heading_font_size(|level, _| px(match level {
            1 => 19.,
            2 => 16.,
            3 => 15.,
            _ => 14.,
        }))
        .with_heading_line_height(|level| px(match level {
            1 => 27.,
            2 => 24.,
            _ => 22.,
        }))
        .with_dark(is_dark())
}

/// One markdown block of an assistant message. Its parsed state lives only while the row renders;
/// a `live` one fades in what streams onto its end. Its links and file chips go to `open`.
pub(super) fn block(id: impl Into<ElementId>, text: SharedString, live: bool, open: Open) -> TextView {
    TextView::markdown(id, text)
        .style(style())
        .plugin(Paths(open.clone()))
        .on_link_click(move |url, _, window, cx| open(url, window, cx))
        .selectable(true)
        .scrollable(false)
        .when(live, |v| v.motion(TextViewMotion::default().with_stream_fade(super::reveal::FADE)))
        .w_full()
        .text_size(px(14.))
        .line_height(px(24.))
        .text_color(TEXT_BODY)
}

/// A fenced code block: language, wrap and copy in a header over the code, which scrolls sideways unless `wrapped`.
pub(super) fn code_block(
    id: &str,
    lang: &str,
    code: &str,
    styles: Option<Styles>,
    (copied, on_copy): (bool, impl Fn(&ClickEvent, &mut Window, &mut App) + 'static),
    (wrapped, on_wrap): (bool, impl Fn(&ClickEvent, &mut Window, &mut App) + 'static),
) -> Div {
    let wrap = div()
        .id(SharedString::from(format!("wrap-{id}")))
        .size(px(22.))
        .flex()
        .items_center()
        .justify_center()
        .rounded(px(5.))
        .cursor_pointer()
        .when(wrapped, |d| d.bg(ink(0.06)))
        .hover(|d| d.bg(ink(0.08)))
        .tooltip(move |window, cx| Tooltip::new(if wrapped { "Scroll long lines" } else { "Wrap long lines" }).build(window, cx))
        .on_click(on_wrap)
        .child(icon("text-wrap", 13., if wrapped { TEXT } else { TEXT_2 }));
    let copy = div()
        .id(SharedString::from(format!("copy-{id}")))
        .h(px(22.))
        .px(px(6.))
        .flex()
        .items_center()
        .gap(px(4.))
        .rounded(px(5.))
        .cursor_pointer()
        .hover(|d| d.bg(ink(0.08)))
        .text_size(px(10.5))
        .text_color(TEXT_2)
        .tooltip(|window, cx| Tooltip::new("Copy code").build(window, cx))
        .on_click(on_copy)
        .child(icon(if copied { "check" } else { "copy" }, 12., TEXT_2))
        .when(copied, |d| d.child("Copied"));
    let header = div()
        .h(px(28.))
        .flex_none()
        .pl(px(12.))
        .pr(px(5.))
        .border_b_1()
        .border_color(SEPARATOR)
        .bg(ink(0.02))
        .flex()
        .items_center()
        .justify_between()
        .child(div().min_w_0().text_size(px(11.)).text_color(TEXT_2).child(lang.to_string()))
        .child(div().flex().items_center().gap(px(2.)).child(wrap).child(copy));
    let text = StyledText::new(code.to_string()).with_highlights(styles.iter().flat_map(|s| s.iter().cloned()));
    let body = div()
        .id(SharedString::from(format!("code-{id}")))
        .when(!wrapped, |d| d.overflow_x_scroll().restrict_scroll_to_axis())
        .px(px(12.))
        .py(px(10.))
        .font_family(code_font())
        .text_size(px(12.5))
        .line_height(px(18.))
        .text_color(TEXT)
        .child(div().min_w_full().map(|d| if wrapped { d.whitespace_normal() } else { d.flex_none().whitespace_nowrap() }).child(text));
    div().w_full().min_w_0().flex().flex_col().rounded(px(10.)).bg(ink(0.035)).border_1().border_color(SEPARATOR).overflow_hidden().child(header).child(body)
}

#[cfg(test)]
mod tests {
    use super::{Highlights, Target, fence, path_in, target};
    use gpui_kit::HighlightStyle;
    use std::sync::Arc;

    #[test]
    fn a_fenced_block_reads_its_language_and_code() {
        assert_eq!(fence("```rust\nfn main() {}\n```\n"), Some(("rust", "fn main() {}")));
        assert_eq!(fence("~~~\na\nb"), Some(("", "a\nb")));
        assert_eq!(fence("Some *text*"), None);
    }

    #[test]
    fn inline_code_naming_a_file_or_folder_is_a_path() {
        assert_eq!(path_in("crates/pocket/src/desktop.rs"), Some(false));
        assert_eq!(path_in("CHANGELOG.md"), Some(false));
        assert_eq!(path_in(".anywhere/"), Some(true));
        for code in ["make app", "v*", "type:", "Desktop", "self.items", "0.1/5", "https://x.dev/a"] {
            assert_eq!(path_in(code), None, "{code}");
        }
    }

    #[test]
    fn a_link_opens_a_web_page_or_a_file_under_the_agents_folder() {
        assert_eq!(target("https://x.dev/a#b", "/r"), Target::Web("https://x.dev/a#b".into()));
        assert_eq!(target("src/a.rs", "/r"), Target::File("/r/src/a.rs".into()));
        assert_eq!(target("src/a.rs:42:7", "/r"), Target::File("/r/src/a.rs".into()));
        assert_eq!(target("a.rs#L42", "/r"), Target::File("/r/a.rs".into()));
        assert_eq!(target("file:///etc/hosts", "/r"), Target::File("/etc/hosts".into()));
    }

    #[test]
    fn a_code_block_is_highlighted_again_when_the_appearance_flips() {
        let mut h = Highlights::default();
        assert!(h.get("r", "a();", false).1);
        h.set("r", 4, false, Arc::new(vec![]));
        assert_eq!(h.get("r", "a();", false), (Some(Arc::new(vec![])), false));
        assert_eq!(h.get("r", "a();", true), (None, true));
        h.set("r", 4, false, Arc::new(vec![]));
        assert_eq!(h.get("r", "a();", true), (None, false), "a light run landing late doesn't fill the dark slot");
    }

    #[test]
    fn a_growing_code_block_keeps_its_old_colours_until_the_new_run_lands() {
        let style = HighlightStyle { font_weight: Some(gpui_kit::FontWeight::BOLD), ..Default::default() };
        let mut h = Highlights::default();
        h.get("r", "let a", false);
        h.set("r", 5, false, Arc::new(vec![(0..3, style)]));
        assert_eq!(h.get("r", "let a = 1;", false), (Some(Arc::new(vec![(0..3, style)])), true));
        assert_eq!(h.get("r", "let a = 1;", false), (Some(Arc::new(vec![(0..3, style)])), false), "asked once per length");
        h.set("r", 5, false, Arc::new(vec![]));
        assert_eq!(h.get("r", "let a = 1;", false).0, Some(Arc::new(vec![(0..3, style)])), "a stale run doesn't replace them");
        h.set("r", 10, false, Arc::new(vec![(0..3, style), (8..9, style)]));
        assert_eq!(h.get("r", "let a = 1;", false), (Some(Arc::new(vec![(0..3, style), (8..9, style)])), false));
    }

    #[test]
    fn kept_colours_stop_at_the_code_and_never_split_a_character() {
        let style = HighlightStyle::default();
        let mut h = Highlights::default();
        h.get("r", "aé b ``", false);
        h.set("r", 8, false, Arc::new(vec![(0..2, style), (4..6, style), (7..8, style)]));
        assert_eq!(h.get("r", "aé b", false).0, Some(Arc::new(vec![(4..5, style)])));
    }
}
