//! A card's mermaid source, shown in place of its diagram.

use super::Diagrams;
use gpui_kit::base::text::{TextView, TextViewStyle};
use gpui_kit::*;
use theme::*;

/// `source` as a code block framed as its card, with wrap, copy and a button back to the diagram.
pub(super) fn block(diagrams: &Entity<Diagrams>, place: u64, source: &str, cx: &App) -> TextView {
    let style = TextViewStyle::default()
        .with_foreground(TEXT.into())
        .with_muted_foreground(TEXT_2.into())
        .with_code_background(diagrams.read(cx).canvas.into())
        .with_code_border(SEPARATOR.into())
        .with_code_block(StyleRefinement::default().rounded(px(15.)).border(px(0.5)))
        .with_selection(SELECTION.into())
        .with_code_font(MONO)
        .with_dark(theme::is_dark());
    let d = diagrams.clone();
    TextView::markdown(("mermaid-source", place), crate::chat::markdown::fenced("mermaid", source))
        .style(style)
        .code_block_actions(move |_, _, _| {
            let d = d.clone();
            div()
                .id(("mermaid-diagram", place))
                .flex_none()
                .size(px(28.))
                .flex()
                .items_center()
                .justify_center()
                .rounded_full()
                .cursor_pointer()
                .hover(|s| s.bg(Hsla::from(TEXT).opacity(0.1)))
                .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                .on_click(move |_, _, cx| d.update(cx, |d, cx| d.toggle_source(place, cx)))
                .child(icon("flow", 16., TEXT_2))
        })
        .selectable(true)
        .scrollable(false)
        .w_full()
        .text_size(px(12.5))
        .text_color(TEXT)
}
