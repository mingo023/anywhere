use super::markdown::Press;
use crate::add_to_chat::blockquote;
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use gpui_kit::base::TextSelection;
use gpui_kit::*;
use theme::*;

#[derive(Default)]
pub(crate) struct Select {
    pub(crate) offer: Option<Offer>,
    drag_from: Point<Pixels>,
}

impl Select {
    pub(crate) fn menu_open(&self) -> bool {
        self.offer.as_ref().is_some_and(|o| matches!(o.kind, Kind::Menu { .. }))
    }

    pub(crate) fn close_menu(&mut self) {
        if self.menu_open() {
            self.offer = None;
        }
    }
}

pub(crate) struct Offer {
    agent: String,
    at: Point<Pixels>,
    /// The selection as it was when the offer opened: a press on the offer clears the window's selection before its click runs.
    text: String,
    kind: Kind,
}

enum Kind {
    Pill,
    Menu { link: Option<String> },
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Item {
    OpenLink,
    CopyLink,
    Copy,
    AddToChat,
}

pub(crate) fn items(selected: bool, link: bool, writable: bool) -> Vec<Item> {
    let mut items = Vec::new();
    if link {
        items.extend([Item::OpenLink, Item::CopyLink]);
    }
    if selected {
        items.push(Item::Copy);
        if writable {
            items.push(Item::AddToChat);
        }
    }
    items
}

/// Lifted 16px so the pill clears the line the higher end of the drag is on.
pub(crate) fn pill_at(down: Point<Pixels>, up: Point<Pixels>) -> Point<Pixels> {
    let top = if (down.y, down.x) <= (up.y, up.x) { down } else { up };
    top - point(px(0.), px(16.))
}

pub(crate) fn quoted(draft: &str, text: &str) -> String {
    let lead = if draft.is_empty() || draft.ends_with('\n') { "" } else { "\n" };
    format!("{lead}{}\n\n", blockquote(text.trim()))
}

impl Desktop {
    pub(super) fn offers_selection(&self, agent: &str, transcript: AnyElement, cx: &mut Context<Self>) -> Div {
        let (up, right) = (agent.to_string(), agent.to_string());
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .capture_any_mouse_down(cx.listener(|this, ev: &MouseDownEvent, _, _| {
                if ev.button == MouseButton::Left {
                    this.chats.select.drag_from = ev.position;
                }
            }))
            .capture_any_mouse_up(cx.listener(move |_, ev: &MouseUpEvent, window, cx| {
                if ev.button != MouseButton::Left {
                    return;
                }
                let (agent, at) = (up.clone(), ev.position);
                // The window settles its selection in its own mouse-up, which runs after this capture.
                cx.defer_in(window, move |this, window, cx| this.offer_pill(agent, at, window, cx));
            }))
            // On release, so a link under the pointer, whose handler stops the event, opens its own menu instead.
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(move |this, ev: &MouseUpEvent, window, cx| this.offer_menu(right.clone(), ev.position, None, window, cx)),
            )
            .on_scroll(cx.listener(|this, _: &ScrollEvent, _, cx| {
                if this.chats.select.offer.take().is_some() {
                    cx.notify();
                }
            }))
            .child(transcript)
    }

    fn offer_pill(&mut self, agent: String, up: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        if self.chats.select.menu_open() {
            return;
        }
        let text = TextSelection::selected_text(window, cx).trim().to_string();
        let shown = self.chats.select.offer.is_some();
        self.chats.select.offer =
            (!text.is_empty() && !self.agents.observe_only()).then(|| Offer { agent, at: pill_at(self.chats.select.drag_from, up), text, kind: Kind::Pill });
        if shown || self.chats.select.offer.is_some() {
            cx.notify();
        }
    }

    pub(super) fn chat_link(&mut self, agent: &str, link: &str, cwd: &str, press: Press, window: &mut Window, cx: &mut Context<Self>) {
        match press {
            Press::Menu => self.offer_menu(agent.to_string(), window.mouse_position(), Some(link.to_string()), window, cx),
            Press::Follow => self.chat_open(agent, link, cwd, window, cx),
        }
    }

    fn offer_menu(&mut self, agent: String, at: Point<Pixels>, link: Option<String>, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus(cx);
        let text = TextSelection::selected_text(window, cx).trim().to_string();
        let shown = !items(!text.is_empty(), link.is_some(), !self.agents.observe_only()).is_empty();
        self.chats.select.offer = shown.then_some(Offer { agent, at, text, kind: Kind::Menu { link } });
        cx.notify();
    }

    fn quote_in_chat(&mut self, agent: &str, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.chats.select.offer = None;
        if text.trim().is_empty() || self.agents.observe_only() {
            return cx.notify();
        }
        TextSelection::clear(window, cx);
        self.chat_focus_input(agent, window, cx);
        let (draft, _) = self.chat_draft(agent, cx);
        self.chat_add_text(agent, &quoted(&draft, text), window, cx);
    }

    pub(crate) fn quote_chat_selection(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent) = self.summary(terminal).map(|a| a.id.clone()) else { return };
        let text = TextSelection::selected_text(window, cx);
        self.quote_in_chat(&agent, &text, window, cx);
    }

    fn pick_offer(&mut self, item: Item, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Offer { agent, text, kind, .. }) = self.chats.select.offer.take() else { return };
        let link = match kind {
            Kind::Menu { link } => link,
            Kind::Pill => None,
        };
        match (item, link) {
            (Item::OpenLink, Some(link)) => {
                let cwd = self.agents.get(&agent).map(|a| a.cwd.clone()).unwrap_or_default();
                self.chat_open(&agent, &link, &cwd, window, cx);
            }
            (Item::CopyLink, Some(link)) => cx.write_to_clipboard(ClipboardItem::new_string(link)),
            (Item::Copy, _) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            (Item::AddToChat, _) => return self.quote_in_chat(&agent, &text, window, cx),
            _ => {}
        }
        cx.notify();
    }

    pub(crate) fn chat_select_offer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // Nothing else drops the offer when a key or the app leaves its chat.
        let offer = self
            .chats
            .select
            .offer
            .as_ref()
            .filter(|o| self.screen == Screen::Sessions && self.overlay.is_none() && self.session() == Some(o.agent.as_str()))?;
        let close = cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.chats.select.offer = None;
            cx.notify();
        });
        let el = match &offer.kind {
            Kind::Pill => {
                let pill = div()
                    .id("chat-select-pill")
                    .h(px(32.))
                    .px(px(12.))
                    .flex()
                    .items_center()
                    .gap(px(8.))
                    .cursor_pointer()
                    .text_size(px(13.))
                    .font_weight(FontWeight::MEDIUM)
                    .text_color(TEXT)
                    .child("Add to chat")
                    .child(ui::kbd("⌘L"))
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.pick_offer(Item::AddToChat, window, cx)))
                    .on_mouse_down_out(close);
                anchored()
                    .anchor(Anchor::BottomLeft)
                    .position(offer.at)
                    .snap_to_window_with_margin(px(8.))
                    .child(ui::pop(pill).rounded(px(10.)))
                    .into_any_element()
            }
            Kind::Menu { link } => {
                let mut menu = ui::pop(div().id("chat-select-menu")).w(px(220.)).p(px(6.)).flex().flex_col().occlude().on_mouse_down_out(close);
                let mut prev = None;
                for item in items(!offer.text.is_empty(), link.is_some(), !self.agents.observe_only()) {
                    let group = matches!(item, Item::OpenLink | Item::CopyLink);
                    if prev.is_some_and(|p| p != group) {
                        menu = menu.child(ui::menu_divider());
                    }
                    prev = Some(group);
                    let row = match item {
                        Item::OpenLink => ui::menu_row("chat-select-open-link", "external", "Open link", None),
                        Item::CopyLink => ui::menu_row("chat-select-copy-link", "copy", "Copy link address", None),
                        Item::Copy => ui::menu_row("chat-select-copy", "copy", "Copy", Some("⌘C")),
                        Item::AddToChat => ui::menu_row("chat-select-add", "comment", "Add to chat", Some("⌘L")),
                    };
                    menu = menu.child(row.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.pick_offer(item, window, cx))));
                }
                anchored().position(offer.at).snap_to_window_with_margin(px(8.)).child(ui::menu_in("chat-select-menu-in", menu)).into_any_element()
            }
        };
        Some(deferred(el).with_priority(2).into_any_element())
    }
}

#[cfg(test)]
mod tests {
    use super::{Item, items, pill_at, quoted};
    use gpui_kit::{point, px};

    #[test]
    fn a_selection_offers_copy_and_add_to_chat() {
        assert_eq!(items(true, false, true), [Item::Copy, Item::AddToChat]);
    }

    #[test]
    fn a_link_offers_open_and_copy_address_before_the_selection() {
        assert_eq!(items(false, true, true), [Item::OpenLink, Item::CopyLink]);
        assert_eq!(items(true, true, true), [Item::OpenLink, Item::CopyLink, Item::Copy, Item::AddToChat]);
    }

    #[test]
    fn an_observer_can_copy_but_not_add_to_chat() {
        assert_eq!(items(true, false, false), [Item::Copy]);
    }

    #[test]
    fn nothing_selected_and_no_link_offers_nothing() {
        assert_eq!(items(false, false, true), []);
    }

    #[test]
    fn the_pill_sits_above_the_higher_end_of_the_drag() {
        let (a, b) = (point(px(40.), px(100.)), point(px(300.), px(148.)));
        assert_eq!(pill_at(a, b), point(px(40.), px(84.)));
        assert_eq!(pill_at(b, a), point(px(40.), px(84.)));
    }

    #[test]
    fn on_one_line_the_pill_starts_where_the_drag_is_leftmost() {
        assert_eq!(pill_at(point(px(300.), px(100.)), point(px(40.), px(100.))), point(px(40.), px(84.)));
    }

    #[test]
    fn a_quote_starts_its_own_line_and_leaves_a_blank_one_after() {
        assert_eq!(quoted("", " a\n\nb "), "> a\n>\n> b\n\n");
        assert_eq!(quoted("see", "a"), "\n> a\n\n");
        assert_eq!(quoted("see\n", "a"), "> a\n\n");
    }
}
