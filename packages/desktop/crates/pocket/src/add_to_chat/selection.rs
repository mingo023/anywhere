use super::Quote;
use crate::desktop::Desktop;
use crate::explorer::preview::{Body as View, relative};
use gpui_kit::*;
use theme::*;
use workspace::tree::PaneId;

/// Offers the pill where a mouse selection in `pane`'s file ends.
pub(crate) fn offers_chat(pane: PaneId, body: Div, cx: &Context<Desktop>) -> Div {
    body.capture_any_mouse_up(cx.listener(move |_, ev: &MouseUpEvent, window, cx| {
        if ev.button != MouseButton::Left {
            return;
        }
        let at = ev.position;
        // The view settles its selection in its own mouse-up, which runs after this capture.
        cx.defer_in(window, move |this, _, cx| this.offer_chat(pane, at, cx));
    }))
}

impl Desktop {
    fn offer_chat(&mut self, pane: PaneId, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.chat.file.is_some() {
            return;
        }
        let pill = self.selection_quote(pane, cx).map(|(quote, _)| (at, quote));
        if pill != self.chat.pill {
            self.chat.pill = pill;
            cx.notify();
        }
    }

    /// What's selected in `pane`'s file, and where on screen it ends when the view can tell.
    fn selection_quote(&self, pane: PaneId, cx: &App) -> Option<(Quote, Option<Point<Pixels>>)> {
        let f = self.preview.pane(pane)?;
        let (file, views) = (f.file.as_deref()?, f.views.as_ref()?);
        let path = || self.cwd().map_or_else(|| file.to_string(), |root| relative(file, &root));
        match self.preview.body(pane) {
            View::Code => {
                let code = views.code.read(cx);
                let range = code.selected_range();
                // Every click lands here, and `value` copies the whole file.
                if range.is_empty() {
                    return None;
                }
                let near = code.range_to_bounds(&range).map(|b| b.bottom_left());
                Some((Quote::code(&path(), &code.value(), range)?, near))
            }
            View::Markdown => Some((Quote::prose(&path(), f.text()?, &views.md.read(cx).selected_text())?, None)),
            _ => None,
        }
    }

    pub(crate) fn ask_about_text(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let pill = self.chat.pill.take();
        let Some((quote, near)) = self.selection_quote(pane, cx) else { return cx.notify() };
        let at = pill.map(|(at, _)| at).or(near).unwrap_or_else(|| window.mouse_position());
        self.ask_about(at, quote, window, cx);
    }

    fn ask_about(&mut self, at: Point<Pixels>, quote: Quote, window: &mut Window, cx: &mut Context<Self>) {
        self.diff.cancel_draft();
        (self.chat.file, self.chat.menu) = (Some((at, quote)), false);
        self.chat.input.update(cx, |s, cx| {
            s.set_value("", window, cx);
            s.focus(window, cx);
        });
        cx.notify();
    }

    pub(crate) fn chat_pill(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let at = self.chat.pill.as_ref()?.0;
        let pill = div()
            .id("chat-pill")
            .h(px(36.))
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .cursor_pointer()
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .text_color(TEXT)
            .child(icon("comment", 14., TEXT_2))
            .child("Add to chat")
            .child(ui::kbd("⌘L"))
            .occlude()
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
                if let Some((at, quote)) = this.chat.pill.take() {
                    this.ask_about(at, quote, window, cx);
                }
            }))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.chat.pill = None;
                cx.notify();
            }));
        Some(deferred(anchored().position(at + point(px(0.), px(8.))).snap_to_window_with_margin(px(8.)).child(ui::pop(pill).rounded(px(12.)))).with_priority(2))
    }

    pub(crate) fn chat_popover(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (at, quote) = self.chat.file.as_ref()?;
        let card = self.chat_card(quote, true, cx).w(px(420.)).occlude().on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, window, cx| {
            // The agent menu floats outside the card, so a press in it lands here too.
            if !this.chat.menu {
                this.cancel_chat(window, cx);
            }
        }));
        Some(deferred(anchored().position(*at + point(px(0.), px(8.))).snap_to_window_with_margin(px(8.)).child(card)).with_priority(2))
    }
}
