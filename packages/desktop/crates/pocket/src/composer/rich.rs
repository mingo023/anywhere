use crate::actions::{FormatBold, FormatCode, FormatItalic, FormatLink};
use crate::chat::composer::suggest::Prompt;
use crate::desktop::Desktop;
use chat::rich::{self, Edit, Style};
use gpui_kit::component::input::{Enter, IndentInline, InputEditsEvent, OutdentInline};
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
        Style::Link => HighlightStyle { underline: Some(UnderlineStyle { thickness: px(1.), color: Some(Hsla::from(MD_LINK).opacity(0.4)), wavy: false }), ..tint(MD_LINK) },
        Style::Syntax => tint(TEXT_4),
        Style::Code => tint(TEXT_2),
    }
}

impl Desktop {
    /// Makes the edit `find` has for the prompt's text and selection; false when it has none.
    fn rich_edit(&mut self, p: Prompt, find: impl FnOnce(&str, Range<usize>) -> Option<Edit>, window: &mut Window, cx: &mut Context<Self>) -> bool {
        let input = self.suggest_input(p).clone();
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
    pub(crate) fn rich_typed(&mut self, p: Prompt, edits: &InputEditsEvent, window: &mut Window, cx: &mut Context<Self>) {
        let Some((edit, _)) = edits.edits().last() else { return };
        let typed = edit.start_byte..edit.new_end_byte;
        self.rich_edit(p, |text, selection| rich::list_started(text, typed, selection), window, cx);
    }

    /// A format key acts on the selection; with none it stays the app's, as ⌘B and ⌘K are.
    fn rich_format(&mut self, p: Prompt, format: impl FnOnce(&str, Range<usize>) -> Edit, window: &mut Window, cx: &mut Context<Self>) {
        if !self.rich_edit(p, |text, selection| (!selection.is_empty()).then(|| format(text, selection)), window, cx) {
            cx.propagate();
        }
    }

    /// `d` keeping a list going as the prompt is typed, and formatting its selection. After `suggest_keys`, so an open
    /// list of suggestions takes Tab first.
    pub(crate) fn rich_keys(&self, p: Prompt, d: Div, cx: &mut Context<Self>) -> Div {
        let stop = |cx: &mut Context<Self>, taken: bool| {
            if taken {
                cx.stop_propagation();
            }
        };
        d.capture_action(cx.listener(move |this, e: &Enter, window, cx| {
            let taken = e.shift && !e.secondary && this.rich_edit(p, rich::newline, window, cx);
            stop(cx, taken);
        }))
        .capture_action(cx.listener(move |this, _: &IndentInline, window, cx| {
            let taken = this.rich_edit(p, |text, selection| rich::indent(text, selection, false), window, cx);
            stop(cx, taken);
        }))
        .capture_action(cx.listener(move |this, _: &OutdentInline, window, cx| {
            let taken = this.rich_edit(p, |text, selection| rich::indent(text, selection, true), window, cx);
            stop(cx, taken);
        }))
        .on_action(cx.listener(move |this, _: &FormatBold, window, cx| this.rich_format(p, |text, selection| rich::toggle(text, selection, "**"), window, cx)))
        .on_action(cx.listener(move |this, _: &FormatItalic, window, cx| this.rich_format(p, |text, selection| rich::toggle(text, selection, "_"), window, cx)))
        .on_action(cx.listener(move |this, _: &FormatCode, window, cx| this.rich_format(p, |text, selection| rich::toggle(text, selection, "`"), window, cx)))
        .on_action(cx.listener(move |this, _: &FormatLink, window, cx| this.rich_format(p, rich::link, window, cx)))
    }
}
