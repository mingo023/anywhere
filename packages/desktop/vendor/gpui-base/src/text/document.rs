use gpui::{
    AnyElement, App, IntoElement, ListState, ParentElement as _, Pixels, SharedString, Styled as _,
    Window, div, prelude::FluentBuilder as _, px,
};

use std::{ops::RangeInclusive, sync::Arc};

use crate::text::{
    SelectionFormat,
    node::{BlockNode, NodeContext, margin_top},
};

/// The parsed document AST.
#[derive(Debug, Clone, PartialEq, Default)]
pub(crate) struct ParsedDocument {
    pub(crate) source: SharedString,
    pub(crate) blocks: Arc<Vec<BlockNode>>,
}

#[derive(Default, Clone, Copy)]
pub(crate) struct NodeRenderOptions {
    pub(crate) ix: usize,
}

impl ParsedDocument {
    pub(super) fn text(&self) -> String {
        let mut text = String::new();
        for block in self.blocks.iter() {
            text.push_str(&block.text());
        }
        text
    }

    /// The selected text across all blocks, in `format`.
    ///
    /// In [`SelectionFormat::Source`] each block reconstructs its own Markdown
    /// source (inline markup, and block prefixes for headings and lists), and
    /// top-level blocks are joined with a blank line so the result re-renders
    /// with the same block structure.
    ///
    /// A block only learns its selection when it is painted, so in a scrollable
    /// (virtualized) view every block the user scrolled past reports nothing.
    /// The selection is one continuous range, so blocks it spans that came up
    /// empty are inside it and are emitted whole rather than dropped. `blocks`
    /// bounds that span; it comes from the selection endpoints, which hold on to
    /// their block index even after it scrolls out of view (the painted blocks
    /// alone cannot bound the span, because the press that starts a drag leaves
    /// an empty selection that never reaches paint). Without it, fall back to
    /// the painted blocks. (Source mode is only used by non-scrollable views,
    /// where `blocks` is `None` and every block paints.)
    ///
    /// A standalone image (a paragraph that is only an image) has no selectable
    /// text run, so it never carries a selection of its own. It is therefore
    /// included when it is *enclosed* by the selection — some block before and
    /// some block after it are selected — mirroring how an inline image is
    /// emitted when the selection runs into it. (Select-all returns the source
    /// verbatim, so a leading or trailing image is still copied there.)
    pub(super) fn selected_text(
        &self,
        format: SelectionFormat,
        blocks: Option<RangeInclusive<usize>>,
    ) -> String {
        let requested_blocks = blocks.clone();
        let painted = self
            .blocks
            .iter()
            .map(|block| block.has_selection())
            .collect::<Vec<_>>();
        let (Some(painted_first), Some(painted_last)) = (
            painted.iter().position(|painted| *painted),
            painted.iter().rposition(|painted| *painted),
        ) else {
            return String::new();
        };

        let last_ix = self.blocks.len().saturating_sub(1);
        let (first, last) = match blocks {
            Some(blocks) => (*blocks.start().min(&last_ix), *blocks.end().min(&last_ix)),
            None => (painted_first, painted_last),
        };

        if format == SelectionFormat::Plain {
            let mut text = String::new();
            for (ix, block) in self.blocks.iter().enumerate().take(last + 1).skip(first) {
                let selected = block.selected_text(format);
                let is_virtual_endpoint = requested_blocks
                    .as_ref()
                    .is_some_and(|blocks| ix == *blocks.start() || ix == *blocks.end());
                if requested_blocks.is_some() && !is_virtual_endpoint {
                    text.push_str(&block.text());
                } else if !selected.is_empty() {
                    text.push_str(&selected);
                } else if !painted[ix] {
                    // Never painted, so it cannot report a selection of its own
                    // even though the span covers it. A painted block that came
                    // up empty really has nothing selected, and stays empty.
                    text.push_str(&block.text());
                }
            }
            return text;
        }

        let mut out: Vec<String> = Vec::new();
        for (ix, block) in self.blocks.iter().enumerate().take(last + 1).skip(first) {
            // The selection is one continuous range, so only the block it
            // starts in and the block it ends in can be partly selected.
            // Everything between them is covered whole, and so is any block
            // that reports nothing — it either scrolled past without painting,
            // or renders no selectable text run at all (a rule, a break, a
            // custom node, a standalone image).
            let source = if (ix == first || ix == last) && painted[ix] {
                block.selected_text(format)
            } else {
                self.whole_source(block)
            };

            let trimmed = source.trim_end_matches('\n');
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
        out.join("\n\n")
    }

    /// The whole source of a block the selection covers.
    ///
    /// Copied straight out of the original text, which the Markdown parser
    /// locates per block. That keeps whatever the author wrote — `_italic_`
    /// stays `_italic_`, a reference link keeps its `[ref]` form, a table keeps
    /// its column padding — and it needs no rule of its own per block type.
    /// Blocks the parser could not locate fall back to reconstruction.
    fn whole_source(&self, block: &BlockNode) -> String {
        if let Some(span) = block.span()
            && let Some(source) = self.source.get(span.start..span.end)
        {
            return source.to_string();
        }

        block.selected_text(SelectionFormat::Source)
    }

    /// Synchronously clear the selection stored in every inline state.
    ///
    /// This mirrors the [`selected_text`](Self::selected_text) traversal so the
    /// stored selection can be cleared without relying on a repaint. Offscreen
    /// (virtualized) views do not repaint, so their `InlineState.selection`
    /// would otherwise retain stale values from the last painted frame.
    pub(super) fn clear_selection(&self) {
        for block in self.blocks.iter() {
            block.clear_selection();
        }
    }

    /// Converts the node to markdown format.
    ///
    /// This is used to generate markdown for test.
    #[allow(dead_code)]
    pub(crate) fn to_markdown(&self) -> String {
        self.blocks
            .iter()
            .map(|child| child.to_markdown())
            .collect::<Vec<_>>()
            .join("\n\n")
    }

    pub(super) fn render_root(
        &self,
        list_state: Option<ListState>,
        node_cx: &NodeContext,
        window: &mut Window,
        cx: &mut App,
    ) -> impl IntoElement {
        let Some(list_state) = list_state else {
            return div().w_full().children(
                (0..self.blocks.len()).map(|ix| {
                    render_page_block(&self.blocks, &self.source, ix, node_cx, window, cx)
                }),
            );
        };

        let blocks = &self.blocks;
        if list_state.item_count() != blocks.len() {
            list_state.reset(blocks.len());
        }

        div().size_full().child(
            gpui::list(list_state, {
                let node_cx = node_cx.clone();
                let blocks = blocks.clone();
                let source = self.source.clone();
                move |ix, window, cx| render_page_block(&blocks, &source, ix, &node_cx, window, cx)
            })
            .size_full(),
        )
    }
}

/// Top-level block `ix` in the page: in the content column, below the
/// space that keeps the blank lines written above it.
fn render_page_block(
    blocks: &[BlockNode],
    source: &str,
    ix: usize,
    node_cx: &NodeContext,
    window: &mut Window,
    cx: &mut App,
) -> AnyElement {
    let block = &blocks[ix];
    let style = &node_cx.style;
    let rem_size = window.rem_size();
    let margin = margin_top(
        block,
        style,
        window.text_style().font_size.to_pixels(rem_size),
        style.paragraph_gap().to_pixels(rem_size),
    );
    let space = space_above(
        margin,
        ix == 0,
        matches!(block, BlockNode::Heading { .. }),
        block
            .span()
            .map_or(0, |span| blank_lines_before(source, span.start)),
        window.line_height(),
    );
    let padding = style.content_padding();
    div()
        .w_full()
        .pl(padding.left)
        .pr(padding.right)
        .pt(if ix == 0 { space + padding.top } else { space })
        .when(ix + 1 == blocks.len(), |this| this.pb(padding.bottom))
        .child(
            div()
                .w_full()
                .when_some(style.content_max_width(), |this, width| {
                    this.max_w(width).mx_auto()
                })
                .child(block.render_block(
                    NodeRenderOptions {
                        ix,
                        ..Default::default()
                    },
                    node_cx,
                    window,
                    cx,
                )),
        )
        .into_any_element()
}

/// The space above a top-level block with `margin`, keeping the blank lines
/// written above it: each a line high but the last, which shrinks to a small
/// gap. A heading's margin after a blank line is a line high.
fn space_above(
    margin: Pixels,
    first: bool,
    heading: bool,
    blank_lines: usize,
    line_height: Pixels,
) -> Pixels {
    const LAST_BLANK_LINE: Pixels = px(4.);
    match blank_lines {
        0 if first => Pixels::ZERO,
        0 => margin,
        n => {
            line_height * (n - 1) as f32
                + LAST_BLANK_LINE
                + if heading { line_height } else { margin }
        }
    }
}

/// How many blank lines are right above the line holding `start`.
fn blank_lines_before(source: &str, start: usize) -> usize {
    let source = source.get(..start).unwrap_or_default();
    let line_start = source.rfind('\n').map_or(0, |ix| ix + 1);
    source[..line_start]
        .lines()
        .rev()
        .take_while(|line| line.trim().is_empty())
        .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blank_lines_are_counted_back_to_the_previous_text() {
        let source = "# Title\n\n  \n\nBody";
        assert_eq!(blank_lines_before(source, source.find("Body").unwrap()), 3);
        assert_eq!(blank_lines_before(source, 0), 0);
        assert_eq!(blank_lines_before("a\nb", 2), 0);
    }

    #[test]
    fn blank_lines_add_a_line_each_but_the_last_adds_four_pixels() {
        let (margin, lh) = (px(10.), px(21.));
        assert_eq!(space_above(margin, true, false, 0, lh), px(0.));
        assert_eq!(space_above(margin, false, false, 0, lh), margin);
        assert_eq!(space_above(margin, false, false, 1, lh), px(14.));
        assert_eq!(space_above(margin, false, false, 3, lh), px(56.));
    }

    #[test]
    fn a_heading_after_a_blank_line_is_spaced_by_a_line_instead_of_its_margin() {
        assert_eq!(space_above(px(30.), false, true, 1, px(21.)), px(25.));
    }
}
