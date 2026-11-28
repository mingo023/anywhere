use super::markdown::{self, Press};
use crate::add_to_chat::Quote;
use crate::desktop::Desktop;
use crate::desktop::chrome::Screen;
use gpui_kit::base::TextSelection;
use gpui_kit::*;
use theme::*;

#[derive(Default)]
pub(crate) struct Select {
    offer: Option<Offer>,
    drag_from: Point<Pixels>,
}

pub(crate) enum SelectEvent {
    OpenLink { agent: String, link: String },
    OpenIn { link: String, browser: Browser },
    Quote { agent: String, text: String },
    Menu { agent: String, at: Point<Pixels> },
}

impl EventEmitter<SelectEvent> for Select {}

impl Select {
    pub(crate) fn menu_open(&self) -> bool {
        self.offer.as_ref().is_some_and(|o| matches!(o.kind, Kind::Menu { .. }))
    }

    pub(crate) fn close_menu(&mut self) {
        if self.menu_open() {
            self.offer = None;
        }
    }

    pub(crate) fn dismiss(&mut self) {
        self.offer = None;
    }

    pub(crate) fn pressed(&mut self, at: Point<Pixels>) {
        self.drag_from = at;
    }

    pub(crate) fn scrolled(&mut self, cx: &mut Context<Self>) {
        if self.offer.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn released(&mut self, agent: String, up: Point<Pixels>, writable: bool, window: &mut Window, cx: &mut Context<Self>) {
        if self.menu_open() {
            return;
        }
        let text = TextSelection::selected_text(window, cx).trim().to_string();
        let shown = self.offer.is_some();
        self.offer = (!text.is_empty() && writable).then(|| Offer { agent, at: pill_at(self.drag_from, up), text, kind: Kind::Pill });
        if shown || self.offer.is_some() {
            cx.notify();
        }
    }

    pub(crate) fn menu(&mut self, agent: String, at: Point<Pixels>, link: Option<Link>, writable: bool, window: &mut Window, cx: &mut Context<Self>) {
        let text = TextSelection::selected_text(window, cx).trim().to_string();
        let shown = !items(!text.is_empty(), link.as_ref().map(|l| l.other_browser), writable).is_empty();
        self.offer = shown.then_some(Offer { agent, at, text, kind: Kind::Menu { link } });
        cx.notify();
    }

    fn pick(&mut self, item: Item, cx: &mut Context<Self>) {
        let Some(Offer { agent, text, kind, .. }) = self.offer.take() else { return };
        let link = match kind {
            Kind::Menu { link } => link.map(|l| l.url),
            Kind::Pill => None,
        };
        match (item, link) {
            (Item::OpenLink, Some(link)) => cx.emit(SelectEvent::OpenLink { agent, link }),
            (Item::OpenIn(browser), Some(link)) => cx.emit(SelectEvent::OpenIn { link, browser }),
            (Item::CopyLink, Some(link)) => cx.write_to_clipboard(ClipboardItem::new_string(link)),
            (Item::Copy, _) => cx.write_to_clipboard(ClipboardItem::new_string(text)),
            (Item::AddToChat, _) => cx.emit(SelectEvent::Quote { agent, text }),
            _ => {}
        }
        cx.notify();
    }

    pub(crate) fn offers(&self, agent: &str, transcript: AnyElement, writable: bool, cx: &mut Context<Self>) -> Div {
        let (up, right) = (agent.to_string(), agent.to_string());
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .capture_any_mouse_down(cx.listener(|this, ev: &MouseDownEvent, _, _| {
                if ev.button == MouseButton::Left {
                    this.pressed(ev.position);
                }
            }))
            .capture_any_mouse_up(cx.listener(move |_, ev: &MouseUpEvent, window, cx| {
                if ev.button != MouseButton::Left {
                    return;
                }
                let (agent, at) = (up.clone(), ev.position);
                // The window settles its selection in its own mouse-up, which runs after this capture.
                cx.defer_in(window, move |this, window, cx| this.released(agent, at, writable, window, cx));
            }))
            // On release, so a link under the pointer, whose handler stops the event, opens its own menu instead.
            .on_mouse_up(
                MouseButton::Right,
                cx.listener(move |_, ev: &MouseUpEvent, _, cx| cx.emit(SelectEvent::Menu { agent: right.clone(), at: ev.position })),
            )
            .on_scroll(cx.listener(|this, _: &ScrollEvent, _, cx| this.scrolled(cx)))
            .child(transcript)
    }

    pub(crate) fn view(&self, shown: &str, writable: bool, cx: &mut Context<Self>) -> Option<AnyElement> {
        let offer = self.offer.as_ref().filter(|o| o.agent == shown)?;
        let close = cx.listener(|this, _: &MouseDownEvent, _, cx| {
            this.offer = None;
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
                    .children(crate::settings::hint(&crate::actions::AddToChat, cx).map(|k| ui::kbd(&k)))
                    .occlude()
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.pick(Item::AddToChat, cx)))
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
                for item in items(!offer.text.is_empty(), link.as_ref().map(|l| l.other_browser), writable) {
                    let group = matches!(item, Item::OpenLink | Item::OpenIn(_) | Item::CopyLink);
                    if prev.is_some_and(|p| p != group) {
                        menu = menu.child(ui::menu_divider());
                    }
                    prev = Some(group);
                    let row = match item {
                        Item::OpenLink => ui::menu_row("chat-select-open-link", "external", "Open link", None),
                        Item::OpenIn(Browser::InApp) => ui::menu_row("chat-select-open-in-app", "globe", "Open in in-app browser", None),
                        Item::OpenIn(Browser::System) => ui::menu_row("chat-select-open-in-system", "laptop", "Open in system browser", None),
                        Item::CopyLink => ui::menu_row("chat-select-copy-link", "copy", "Copy link address", None),
                        Item::Copy => ui::menu_row("chat-select-copy", "copy", "Copy", Some("⌘C")),
                        Item::AddToChat => {
                            ui::menu_row("chat-select-add", "comment", "Add to chat", crate::settings::hint(&crate::actions::AddToChat, cx).as_deref())
                        }
                    };
                    menu = menu.child(row.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.pick(item, cx))));
                }
                anchored().position(offer.at).snap_to_window_with_margin(px(8.)).child(ui::menu_in("chat-select-menu-in", menu)).into_any_element()
            }
        };
        Some(deferred(el).with_priority(2).into_any_element())
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
    Menu { link: Option<Link> },
}

pub(crate) struct Link {
    url: String,
    /// The browser "Open link" doesn't use, offered beside it; None when the link isn't a web page.
    other_browser: Option<Browser>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Browser {
    InApp,
    System,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Item {
    OpenLink,
    OpenIn(Browser),
    CopyLink,
    Copy,
    AddToChat,
}

/// The browser a link may also open in: the one the "Open links in" setting doesn't pick, for a web page only.
fn other_browser(web: bool, system_browser: bool) -> Option<Browser> {
    web.then_some(if system_browser { Browser::InApp } else { Browser::System })
}

/// `link` is None without a link under the pointer, else the browser it may also open in.
pub(crate) fn items(selected: bool, link: Option<Option<Browser>>, writable: bool) -> Vec<Item> {
    let mut items = Vec::new();
    if let Some(other_browser) = link {
        items.push(Item::OpenLink);
        items.extend(other_browser.map(Item::OpenIn));
        items.push(Item::CopyLink);
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

impl Desktop {
    pub(super) fn offers_selection(&self, agent: &str, transcript: AnyElement, cx: &mut Context<Self>) -> Div {
        let writable = !self.agents.observe_only();
        self.chats.select.update(cx, |s, cx| s.offers(agent, transcript, writable, cx))
    }

    pub(super) fn chat_link(&mut self, agent: &str, link: &str, cwd: &str, press: Press, window: &mut Window, cx: &mut Context<Self>) {
        match press {
            Press::Menu => {
                let web = matches!(markdown::target(link, cwd, &self.host_agents.home_of(cwd)), markdown::Target::Web(_));
                let other_browser = other_browser(web, self.store.terminal.system_browser);
                self.offer_menu(agent.to_string(), window.mouse_position(), Some(Link { url: link.to_string(), other_browser }), window, cx)
            }
            Press::Follow => self.chat_open(agent, link, cwd, window, cx),
        }
    }

    fn offer_menu(&mut self, agent: String, at: Point<Pixels>, link: Option<Link>, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus(cx);
        let writable = !self.agents.observe_only();
        self.chats.select.update(cx, |s, cx| s.menu(agent, at, link, writable, window, cx));
    }

    pub(super) fn on_select_event(&mut self, ev: &SelectEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            SelectEvent::OpenLink { agent, link } => {
                let cwd = self.agents.get(agent).map(|a| a.cwd.clone()).unwrap_or_default();
                self.chat_open(agent, link, &cwd, window, cx);
            }
            SelectEvent::OpenIn { link, browser: Browser::InApp } => self.open_browser(Some(link.clone()), window, cx),
            SelectEvent::OpenIn { link, browser: Browser::System } => cx.open_url(link),
            SelectEvent::Quote { agent, text } => self.quote_in_chat(agent, text, window, cx),
            SelectEvent::Menu { agent, at } => self.offer_menu(agent.clone(), *at, None, window, cx),
        }
    }

    fn quote_in_chat(&mut self, agent: &str, text: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.chats.select.update(cx, |s, _| s.dismiss());
        let Some(quote) = Quote::chat(text).filter(|_| !self.agents.observe_only()) else { return cx.notify() };
        TextSelection::clear(window, cx);
        self.chat_focus_input(agent, window, cx);
        self.chat_draft_edit(agent, cx, |d| d.add_quote(&quote, ""));
        cx.notify();
    }

    pub(crate) fn quote_chat_selection(&mut self, terminal: &str, window: &mut Window, cx: &mut Context<Self>) {
        let Some(agent) = self.summary(terminal).map(|a| a.id.clone()) else { return };
        let text = TextSelection::selected_text(window, cx);
        self.quote_in_chat(&agent, &text, window, cx);
    }

    pub(crate) fn chat_select_offer(&self, cx: &mut Context<Self>) -> Option<AnyElement> {
        // Nothing else drops the offer when a key or the app leaves its chat.
        if self.screen != Screen::Sessions || self.overlay.is_some() {
            return None;
        }
        let (shown, writable) = (self.session()?.to_string(), !self.agents.observe_only());
        self.chats.select.update(cx, |s, cx| s.view(&shown, writable, cx))
    }
}

#[cfg(test)]
mod tests {
    use super::{Browser, Item, items, other_browser, pill_at};
    use gpui_kit::{point, px};

    #[test]
    fn a_selection_offers_copy_and_add_to_chat() {
        assert_eq!(items(true, None, true), [Item::Copy, Item::AddToChat]);
    }

    #[test]
    fn a_link_offers_open_and_copy_address_before_the_selection() {
        assert_eq!(items(false, Some(None), true), [Item::OpenLink, Item::CopyLink]);
        assert_eq!(items(true, Some(None), true), [Item::OpenLink, Item::CopyLink, Item::Copy, Item::AddToChat]);
    }

    #[test]
    fn a_web_link_offers_the_browser_the_setting_doesnt_pick_and_another_link_offers_none() {
        assert_eq!(other_browser(true, false), Some(Browser::System));
        assert_eq!(other_browser(true, true), Some(Browser::InApp));
        assert_eq!(other_browser(false, true), None);
    }

    #[test]
    fn a_web_link_also_offers_the_browser_open_link_does_not_use() {
        assert_eq!(items(false, Some(Some(Browser::InApp)), true), [Item::OpenLink, Item::OpenIn(Browser::InApp), Item::CopyLink]);
        assert_eq!(items(false, Some(Some(Browser::System)), true), [Item::OpenLink, Item::OpenIn(Browser::System), Item::CopyLink]);
    }

    #[test]
    fn an_observer_can_copy_but_not_add_to_chat() {
        assert_eq!(items(true, None, false), [Item::Copy]);
    }

    #[test]
    fn nothing_selected_and_no_link_offers_nothing() {
        assert_eq!(items(false, None, true), []);
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
}
