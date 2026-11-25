use crate::actions::{FormatBold, FormatCode, FormatItalic, FormatLink};
use chat::rich::{self, Edit, Style};
use gpui_kit::component::input::{Enter, IndentInline, InputEditsEvent, OutdentInline, TextareaState};
use gpui_kit::*;
use std::ops::Range;
use theme::*;

/// How a prompt's markdown shows, in the input and in the bubble it is sent as.
pub(crate) fn highlight(style: Style) -> HighlightStyle {
    let tint = |color: Token| HighlightStyle { color: Some(color.into()), ..Default::default() };
    match style {
        Style::Bold => HighlightStyle { font_weight: Some(FontWeight::SEMIBOLD), ..Default::default() },
        Style::Italic => HighlightStyle { font_style: Some(FontStyle::Italic), ..Default::default() },
        Style::Strike => HighlightStyle { strikethrough: Some(StrikethroughStyle { thickness: px(1.), color: None }), ..tint(TEXT_3) },
        Style::Link => HighlightStyle {
            underline: Some(UnderlineStyle { thickness: px(1.), color: Some(Hsla::from(MD_LINK).opacity(0.4)), wavy: false }),
            ..tint(MD_LINK)
        },
        Style::Syntax => tint(TEXT_4),
        Style::Code => tint(TEXT_2),
    }
}

fn rich_edit(input: &Entity<TextareaState>, find: impl FnOnce(&str, Range<usize>) -> Option<Edit>, window: &mut Window, cx: &mut App) -> bool {
    let state = input.read(cx);
    let Some(edit) = find(&state.value(), state.selected_range()) else { return false };
    input.update(cx, |s, cx| {
        s.set_selected_range(edit.range, cx);
        s.replace_text_in_range(None, &edit.text, window, cx);
        s.set_selected_range(edit.select, cx);
    });
    true
}

/// After an edit to the prompt: a list it just started steps in.
pub(crate) fn rich_typed(input: &Entity<TextareaState>, edits: &InputEditsEvent, window: &mut Window, cx: &mut App) {
    let Some((edit, _)) = edits.edits().last() else { return };
    let typed = edit.start_byte..edit.new_end_byte;
    rich_edit(input, |text, selection| rich::list_started(text, typed, selection), window, cx);
}

/// A format key acts on the selection; with none it stays the app's, as ⌘B and ⌘K are.
fn rich_format(input: &Entity<TextareaState>, format: impl FnOnce(&str, Range<usize>) -> Edit, window: &mut Window, cx: &mut App) {
    if !rich_edit(input, |text, selection| (!selection.is_empty()).then(|| format(text, selection)), window, cx) {
        cx.propagate();
    }
}

/// `d` keeping a list going as `input`'s prompt is typed, and formatting its selection. After the suggestions' keys, so
/// an open list takes Tab first.
pub(crate) fn rich_keys<V: 'static>(input: &Entity<TextareaState>, d: Div, cx: &mut Context<V>) -> Div {
    let stop = |cx: &mut App, taken: bool| {
        if taken {
            cx.stop_propagation();
        }
    };
    d.capture_action(cx.listener({
        let input = input.clone();
        move |_, e: &Enter, window, cx| {
            let taken = e.shift && !e.secondary && rich_edit(&input, rich::newline, window, cx);
            stop(cx, taken);
        }
    }))
    .capture_action(cx.listener({
        let input = input.clone();
        move |_, _: &IndentInline, window, cx| {
            let taken = rich_edit(&input, |text, selection| rich::indent(text, selection, false), window, cx);
            stop(cx, taken);
        }
    }))
    .capture_action(cx.listener({
        let input = input.clone();
        move |_, _: &OutdentInline, window, cx| {
            let taken = rich_edit(&input, |text, selection| rich::indent(text, selection, true), window, cx);
            stop(cx, taken);
        }
    }))
    .on_action(cx.listener({
        let input = input.clone();
        move |_, _: &FormatBold, window, cx| rich_format(&input, |text, selection| rich::toggle(text, selection, "**"), window, cx)
    }))
    .on_action(cx.listener({
        let input = input.clone();
        move |_, _: &FormatItalic, window, cx| rich_format(&input, |text, selection| rich::toggle(text, selection, "_"), window, cx)
    }))
    .on_action(cx.listener({
        let input = input.clone();
        move |_, _: &FormatCode, window, cx| rich_format(&input, |text, selection| rich::toggle(text, selection, "`"), window, cx)
    }))
    .on_action(cx.listener({
        let input = input.clone();
        move |_, _: &FormatLink, window, cx| rich_format(&input, rich::link, window, cx)
    }))
}
