use super::{Views, frame};
use crate::explorer::frontmatter::Frontmatter;
use crate::explorer::mermaid::Mermaid;
use gpui_kit::base::text::{CodeBlock, CodeBlockHighlighterFn, MarkdownExtensions, TextView, TextViewStyle};
use gpui_kit::component::highlighter::SyntaxHighlighter;
use gpui_kit::component::input::Rope;
use gpui_kit::*;
use std::path::Path;
use std::sync::{Arc, LazyLock};
use theme::*;

// SF Mono, which macOS lists under this name only.
const CODE_FONT: &str = ".AppleSystemUIFontMonospaced";

/// Code blocks in Xcode's colours; one per scheme, kept so their highlights stay cached across frames.
static HIGHLIGHTERS: LazyLock<[Arc<CodeBlockHighlighterFn>; 2]> = LazyLock::new(|| [false, true].map(highlighter));

fn highlighter(dark: bool) -> Arc<CodeBlockHighlighterFn> {
    let theme = markdown_highlight_theme(dark);
    Arc::new(move |block: &CodeBlock| {
        let Some(lang) = block.lang() else { return Vec::new() };
        // The reference reads these fence names as bash too.
        let lang = match lang.to_lowercase().as_str() {
            "shell" | "zsh" | "console" => "bash".to_string(),
            lang => lang.to_string(),
        };
        let code = block.code();
        let mut hl = SyntaxHighlighter::new(&lang);
        hl.update(None, &Rope::from(code.as_ref()), None);
        hl.styles(&(0..code.len()), &*theme)
    })
}

fn markdown_style() -> TextViewStyle {
    let mut table = StyleRefinement::default();
    table.overflow.x = Some(Overflow::Scroll);
    TextViewStyle::default()
        .with_foreground(MD_TEXT.into())
        .with_muted_foreground(MD_TEXT_2.into())
        .with_link(MD_LINK.into())
        .with_border(MD_GRID.into())
        .with_code_background(MD_CODE_BG.into())
        .with_code_border(MD_CODE_BORDER.into())
        .with_aside_background(MD_ASIDE.into())
        .with_inline_code(HighlightStyle::default())
        .with_code_font(CODE_FONT)
        .with_content_max_width(px(820.))
        .with_content_padding(Edges { top: px(32.), right: px(40.), bottom: px(48.), left: px(40.) })
        // 0.8em of the 14px body; GPUI's rem is 16px.
        .with_paragraph_gap(rems(0.7))
        .with_table(table)
        .with_dark(is_dark())
}

/// The directory a markdown file's relative images load from; none for a file on a host, which `img` can't read.
fn image_base(path: &str) -> Option<&Path> {
    if hosts::split(path).0.is_some() {
        return None;
    }
    Path::new(path).parent()
}

/// A markdown file rendered like pluk-inc/markdown-preview, with its mermaid fences drawn as diagrams.
pub(super) fn markdown_pane(views: &Views, path: &str) -> Div {
    let text = TextView::new(&views.md);
    let text = match image_base(path) {
        Some(dir) => text.image_base(dir),
        None => text,
    };
    frame().bg(MD_PAGE).child(
        text
            .markdown_extensions(MarkdownExtensions::default().frontmatter())
            .plugin(Frontmatter)
            .plugin(Mermaid(views.diagrams.clone()))
            .shared_code_block_highlighter(HIGHLIGHTERS[is_dark() as usize].clone())
            .selectable(true)
            .scrollable(true)
            .style(markdown_style())
            .size_full()
            .font_family(SANS)
            .text_size(px(14.))
            .line_height(relative(1.5))
            .text_color(MD_TEXT),
    )
}

#[cfg(test)]
mod tests {
    use super::image_base;
    use std::path::Path;

    #[test]
    fn images_load_beside_a_local_markdown_file_but_not_one_on_a_host() {
        assert_eq!(image_base("/r/docs/README.md"), Some(Path::new("/r/docs")));
        assert_eq!(image_base("//mac-mini/r/docs/README.md"), None);
    }
}
