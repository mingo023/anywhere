use super::{ChatComposer, ChatEvent, Quote};
use crate::desktop::Desktop;
use crate::explorer::preview::{Body as View, relative};
use gpui_kit::*;
use theme::*;
use workspace::tree::PaneId;

impl Desktop {
    pub(crate) fn offer_chat(&mut self, pane: PaneId, at: Point<Pixels>, cx: &mut Context<Self>) {
        if self.chat.read(cx).file.is_some() {
            return;
        }
        let pill = self.selection_quote(pane, cx).map(|(quote, _)| (at, quote));
        self.chat.update(cx, |c, cx| c.offer(pill, cx));
    }

    /// What's selected in `pane`'s file, and where on screen it ends when the view can tell.
    fn selection_quote(&self, pane: PaneId, cx: &App) -> Option<(Quote, Option<Point<Pixels>>)> {
        let preview = self.preview.read(cx);
        let f = preview.pane(pane)?;
        let (file, views) = (f.file.as_deref()?, f.views.as_ref()?);
        let path = || self.cwd(cx).map_or_else(|| file.to_string(), |root| relative(file, &root));
        match preview.body(pane) {
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
        let pill = self.chat.update(cx, |c, _| c.pill.take());
        let Some((quote, near)) = self.selection_quote(pane, cx) else { return cx.notify() };
        let at = pill.map(|(at, _)| at).or(near).unwrap_or_else(|| window.mouse_position());
        self.chat.update(cx, |c, cx| c.ask(at, quote, window, cx));
    }

    pub(crate) fn chat_offers(&self, cx: &mut Context<Self>) -> Entity<ChatComposer> {
        if self.chat.read(cx).file.is_some() {
            let side = self.chat_side(cx);
            self.chat.update(cx, |c, cx| c.hand(side, cx));
        }
        self.chat.clone()
    }
}

impl Render for ChatComposer {
    fn render(&mut self, _: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div().absolute().children(self.pill_view(cx)).children(self.popover(cx))
    }
}

impl ChatComposer {
    fn pill_view(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let at = self.pill.as_ref()?.0;
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
                if let Some((at, quote)) = this.pill.take() {
                    this.ask(at, quote, window, cx);
                }
            }))
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.pill = None;
                cx.notify();
            }));
        Some(
            deferred(anchored().position(at + point(px(0.), px(8.))).snap_to_window_with_margin(px(8.)).child(ui::pop(pill).rounded(px(12.)))).with_priority(2),
        )
    }

    fn popover(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let (at, quote) = self.file.as_ref()?;
        let card = self.card(quote, true, cx).w(px(420.)).occlude().on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
            // The agent menu floats outside the card, so a press in it lands here too.
            if !this.menu {
                cx.emit(ChatEvent::Cancel);
            }
        }));
        Some(deferred(anchored().position(*at + point(px(0.), px(8.))).snap_to_window_with_margin(px(8.)).child(card)).with_priority(2))
    }
}
