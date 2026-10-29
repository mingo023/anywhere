use gpui_kit::base::text::{InlineElement, InlineRenderContext, MarkdownNode, MarkdownParseContext, MarkdownPlugin, TextView, TextViewMotion, TextViewState, TextViewStyle, markdown_ast};
use gpui_kit::component::highlighter::SyntaxHighlighter;
use gpui_kit::component::input::Rope;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::borrow::Cow;
use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;
use theme::*;

pub(super) type Styles = Arc<Vec<(Range<usize>, HighlightStyle)>>;

/// How far a list item's text sits from its marker's edge.
pub(super) const LIST_INDENT: f32 = 14. * 1.9;

/// A fenced code block's language and code; `None` for any other markdown. An unclosed fence (still streaming) runs to the end.
/// A fence indented under a list item takes that indent off each code line, as CommonMark does.
pub(crate) fn fence(text: &str) -> Option<(&str, Cow<'_, str>)> {
    let first = text.lines().find(|l| !l.trim().is_empty())?;
    let indent = first.len() - first.trim_start_matches(' ').len();
    let text = text.trim();
    let marker = ["```", "~~~"].into_iter().find(|m| text.starts_with(m))?;
    let (info, body) = text.split_once('\n').unwrap_or((text, ""));
    let lang = info.trim_start_matches(marker.chars().next()?).split_whitespace().next().unwrap_or("");
    let code = match body.rfind('\n') {
        Some(i) if body[i + 1..].trim().starts_with(marker) => &body[..i],
        _ if body.trim().starts_with(marker) => "",
        _ => body,
    };
    if indent == 0 {
        return Some((lang, Cow::Borrowed(code)));
    }
    let dedent = |l: &str| l[(l.len() - l.trim_start_matches(' ').len()).min(indent)..].to_string();
    Some((lang, Cow::Owned(code.split('\n').map(dedent).collect::<Vec<_>>().join("\n"))))
}

/// Whether a fenced block has its closing fence, as long as its opening one or longer; one still streaming hasn't.
pub(crate) fn closed(text: &str) -> bool {
    let text = text.trim();
    let Some(c) = ['`', '~'].into_iter().find(|c| text.starts_with(*c)) else { return false };
    let run = |l: &str| l.len() - l.trim_start_matches(c).len();
    let opened = run(text);
    let closing = |l: &str| run(l.trim()) >= opened && l.trim().trim_start_matches(c).is_empty();
    opened >= 3 && text.split_once('\n').and_then(|(_, body)| body.lines().last()).is_some_and(closing)
}

/// Parsed markdown by key, kept while its row is off screen: a `TextView` keeps its own only while drawn every frame, so a
/// row scrolled back into view would parse and lay out its text all over again. Code keeps its own, so a paragraph that
/// turns out to open a fence doesn't hand its code the stream fade.
#[derive(Default)]
pub(crate) struct Docs {
    text: HashMap<String, Entity<TextViewState>>,
    code: HashMap<String, Entity<TextViewState>>,
}

impl Docs {
    /// `key`'s document, holding `text`; a `live` one fades in what streams onto its end.
    pub(super) fn get(&mut self, key: &str, text: &str, live: bool, cx: &mut App) -> Entity<TextViewState> {
        let doc = held(&mut self.text, key, text, cx);
        if live {
            doc.update(cx, |d, _| d.set_motion(TextViewMotion::default().with_stream_fade(super::reveal::FADE)));
        }
        doc.update(cx, |d, cx| d.set_text(text, cx));
        doc
    }

    /// `key`'s document, holding `code` fenced as `lang`.
    pub(super) fn code(&mut self, key: &str, lang: &str, code: &str, cx: &mut App) -> Entity<TextViewState> {
        let text = fenced(lang, code);
        let doc = held(&mut self.code, key, &text, cx);
        doc.update(cx, |d, cx| d.set_text(&text, cx));
        doc
    }

    pub(super) fn retain(&mut self, keep: impl Fn(&str) -> bool) {
        self.text.retain(|key, _| keep(key));
        self.code.retain(|key, _| keep(key));
    }
}

/// `key`'s document in `docs`, made from `text` the first time.
fn held(docs: &mut HashMap<String, Entity<TextViewState>>, key: &str, text: &str, cx: &mut App) -> Entity<TextViewState> {
    if let Some(doc) = docs.get(key) {
        return doc.clone();
    }
    docs.entry(key.to_string()).or_insert_with(|| cx.new(|cx| TextViewState::markdown(text, cx))).clone()
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
    // Columns sized to their measured content, so a short one isn't squeezed letter by letter. Framed as a code block
    // is, so a table too wide to fit ends at the frame, not mid-column on the page; a shade stronger, having no fill.
    let mut table = StyleRefinement::default().rounded(px(10.)).border_1().border_color(SEPARATOR_STRONG);
    table.overflow.x = Some(Overflow::Scroll);
    let quote = StyleRefinement::default().border_l_2().border_color(accent.opacity(0.6)).bg(accent.opacity(0.05)).rounded_r(px(6.)).pl(px(12.)).pr(px(10.)).py(px(6.)).text_color(TEXT);
    TextViewStyle::default()
        .with_foreground(TEXT_BODY.into())
        .with_muted_foreground(TEXT_2.into())
        .with_link(TEXT.into())
        .with_strong(TEXT.into())
        .with_link_underline(TEXT_2.into())
        .with_list_marker(accent)
        .with_list_indent(px(LIST_INDENT))
        .with_list_item_gap(px(8.))
        .with_selection(SELECTION.into())
        .with_border(SEPARATOR.into())
        .with_code_background(ink(0.035))
        .with_code_border(SEPARATOR.into())
        .with_code_block(StyleRefinement::default().rounded(px(10.)).border_1())
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

/// One markdown block of an assistant message. Its links and file chips go to `open`.
pub(super) fn block(view: TextView, open: Open) -> TextView {
    view.style(style())
        .plugin(Paths(open.clone()))
        .on_link_click(move |url, _, window, cx| open(url, window, cx))
        .selectable(true)
        .scrollable(false)
        .w_full()
        .text_size(px(14.))
        .line_height(px(24.))
        .text_color(TEXT_BODY)
}

/// A fenced code block, selectable: language, wrap and copy over code that scrolls sideways, coloured once `styles` land.
pub(super) fn code_block(doc: &Entity<TextViewState>, styles: Option<Styles>) -> TextView {
    TextView::new(doc)
        .style(style())
        .code_block_highlighter(move |_| styles.iter().flat_map(|s| s.iter().cloned()).collect())
        .selectable(true)
        .scrollable(false)
        .w_full()
        .text_size(px(12.5))
        .text_color(TEXT)
}

// A code block's header as gpui-base lays it out, which a diagram card's copies; the switch sits over both by them.
const HEADER_TOP: f32 = 8.;
const HEADER_RIGHT: f32 = 10.;
const BUTTON: f32 = 28.;
const BUTTON_GAP: f32 = 4.;
/// A small `ui::segmented`'s height: 24 and its padding.
const SWITCH_HEIGHT: f32 = 30.;

/// `switch` over a card's header, left of its two buttons as a code block's action would sit, inside its 1px border.
pub(super) fn header_switch(switch: Div) -> Div {
    let top = 1. + HEADER_TOP + (BUTTON - SWITCH_HEIGHT) / 2.;
    div().absolute().top(px(top)).right(px(1. + HEADER_RIGHT + 2. * (BUTTON + BUTTON_GAP))).child(switch)
}

/// A diagram card's header, laid out as a code block's so the switch over both stays put: its name, then fit and copy.
pub(super) fn diagram_header(id: &str, copied: bool, on_fit: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static, on_copy: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static) -> Div {
    let button = |name: &str, glyph: &str| {
        let group = SharedString::from(format!("{name}-{id}"));
        div()
            .id(group.clone())
            .group(group.clone())
            .flex_none()
            .size(px(BUTTON))
            .flex()
            .items_center()
            .justify_center()
            .rounded_full()
            .cursor_pointer()
            .hover(|s| s.bg(Hsla::from(TEXT_BODY).opacity(0.1)))
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .child(icon(glyph, 16., TEXT_2).group_hover(group, |s| s.text_color(TEXT_BODY)))
    };
    div()
        .flex()
        .items_center()
        .gap(px(BUTTON_GAP))
        .pt(px(HEADER_TOP))
        .pr(px(HEADER_RIGHT))
        .pl(px(16.))
        .child(div().flex_1().text_size(px(11.)).line_height(px(14.)).text_color(TEXT_2).child("mermaid"))
        .child(button("fit", "fit").on_click(on_fit))
        .child(button("copy", if copied { "block-check" } else { "block-copy" }).on_click(on_copy))
}

/// `code` refenced at column 0 with a fence longer than any backtick run inside it, so it parses back whole.
fn fenced(lang: &str, code: &str) -> String {
    let longest = code.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let ticks = "`".repeat(longest.max(2) + 1);
    format!("{ticks}{lang}\n{code}\n{ticks}")
}

#[cfg(test)]
mod tests {
    use super::{Highlights, Target, closed, fence, fenced, path_in, target};
    use gpui_kit::HighlightStyle;
    use std::sync::Arc;

    #[test]
    fn a_fenced_block_reads_its_language_and_code() {
        assert_eq!(fence("```rust\nfn main() {}\n```\n"), Some(("rust", "fn main() {}".into())));
        assert_eq!(fence("~~~\na\nb"), Some(("", "a\nb".into())));
        assert_eq!(fence("Some *text*"), None);
    }

    #[test]
    fn a_fence_is_closed_only_once_its_closing_fence_arrives() {
        assert!(closed("```mermaid\nflowchart LR\n  A --> B\n```\n"));
        assert!(closed("   ~~~\n   a\n   ~~~"));
        assert!(!closed("```mermaid\nflowchart LR\n  A --> B"));
        assert!(!closed("```mermaid"));
        assert!(!closed("Some *text*"));
        assert!(closed("````md\n```\nx\n```\n````"));
        assert!(!closed("````md\n```\nx\n```"), "a shorter fence inside is code");
        assert!(!closed("```mermaid\nA\n```js"), "a closing fence has nothing after it");
    }

    #[test]
    fn a_fence_under_a_list_item_loses_its_indent_from_each_line() {
        assert_eq!(fence("   ```sh\n   brew install\n     --cask x\n  y\n   ```"), Some(("sh", "brew install\n  --cask x\ny".into())));
    }

    #[test]
    fn a_refenced_block_parses_back_to_its_code() {
        assert_eq!(fenced("rust", "fn main() {}"), "```rust\nfn main() {}\n```");
        let code = "~~~md\n````\nx\n````";
        assert_eq!(fence(&fenced("", code)), Some(("", code.into())));
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
