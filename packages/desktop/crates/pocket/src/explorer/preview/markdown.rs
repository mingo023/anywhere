use super::pane;
use crate::desktop::Desktop;
use crate::explorer::mermaid::Mermaid;
use gpui_kit::base::text::CodeBlock;
use gpui_kit::component::ActiveTheme;
use gpui_kit::component::clipboard::Clipboard;
use gpui_kit::component::highlighter::HighlightTheme;
use gpui_kit::component::text::{TextView, TextViewStyle};
use gpui_kit::*;
use std::sync::Arc;
use theme::*;

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
        inline_code: HighlightStyle { background_color: Some(HAIRLINE.into()), ..Default::default() },
        code_block: StyleRefinement::default().border_1().border_color(SEPARATOR).rounded(px(10.)).pt(px(36.)).px(px(12.)).pb(px(10.)).text_size(px(12.)),
        table: StyleRefinement::default().bg(WELL).border_color(SEPARATOR).rounded(px(10.)),
        table_head: StyleRefinement::default().bg(transparent_black()).text_color(TEXT).font_weight(FontWeight::SEMIBOLD),
        table_cell: StyleRefinement::default().px(px(10.)).py(px(8.)).text_size(px(12.)),
        ..Default::default()
    }
}

fn code_actions(block: &CodeBlock) -> Div {
    div()
        .flex()
        .items_center()
        .gap(px(6.))
        .children(block.lang().map(|lang| div().font_family(MONO).text_size(px(12.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(lang.to_lowercase())))
        .child(Clipboard::new("copy").value(block.code()))
}

impl Desktop {
    /// A markdown file rendered, with its mermaid fences drawn as diagrams.
    pub(super) fn markdown_pane(&self, cx: &App) -> Div {
        pane().px(px(40.)).py(px(32.)).bg(PAGE).child(
            TextView::new(&self.preview.views.md)
                .plugin(Mermaid(self.preview.views.diagrams.clone()))
                .code_block_actions(|block, _, _| code_actions(block))
                .selectable(true)
                .scrollable(true)
                .style(markdown_style(cx.theme().highlight_theme.clone()))
                .size_full(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::heading_size;
    use gpui_kit::px;

    #[test]
    fn headings_shrink_with_depth() {
        assert_eq!((1..=6).map(heading_size).collect::<Vec<_>>(), [22., 18., 16., 14., 14., 14.].map(px));
    }
}
