use crate::actions::{CopySelection, Paste};
use crate::desktop::Desktop;
use gpui_kit::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Item {
    Copy,
    Paste,
    Clear,
    Close,
    Kill,
}

/// The rows a pane's menu offers. Only a terminal that takes input can be pasted into, cleared or killed, and a full-screen program owns its screen.
pub fn items(selected: bool, writable: bool, alt_screen: bool) -> Vec<Item> {
    let mut items = Vec::new();
    if selected {
        items.push(Item::Copy);
    }
    if writable {
        items.push(Item::Paste);
        if !alt_screen {
            items.push(Item::Clear);
        }
    }
    items.push(Item::Close);
    if writable {
        items.push(Item::Kill);
    }
    items
}

/// The menu's dividers fall between groups.
fn group(item: Item) -> u8 {
    match item {
        Item::Copy | Item::Paste => 0,
        Item::Clear => 1,
        Item::Close | Item::Kill => 2,
    }
}

impl Desktop {
    pub(crate) fn open_term_menu(&mut self, pane: &str, at: Point<Pixels>, cx: &mut Context<Self>) {
        self.close_menus();
        self.terminal.menu = Some((pane.to_string(), at));
        cx.notify();
    }

    /// `pane`'s right-click menu, while it is open there.
    pub(crate) fn term_menu(&self, pane: &str, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (_, at) = self.terminal.menu.as_ref().filter(|(p, _)| p == pane)?;
        let session = self.terminals.sessions.get(pane);
        let term = session.and_then(|s| s.term.as_ref());
        let selected = term.is_some_and(|t| t.selection_text().is_some());
        let writable = session.is_some() && self.terminals.may_send(pane, self.agents.observe_only());
        let alt_screen = term.is_some_and(|t| t.alt_screen());
        let mut menu = ui::pop(div().id("term-menu"))
            .w(px(200.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.terminal.menu = None;
                cx.notify();
            }));
        let mut prev = None;
        for item in items(selected, writable, alt_screen) {
            if prev.is_some_and(|p| p != group(item)) {
                menu = menu.child(ui::menu_divider());
            }
            prev = Some(group(item));
            let row = match item {
                Item::Copy => ui::menu_row("term-menu-copy", "copy", "Copy", Some("⌘C")),
                Item::Paste => ui::menu_row("term-menu-paste", "compose", "Paste", Some("⌘V")),
                Item::Clear => ui::menu_row("term-menu-clear", "discard", "Clear", None),
                Item::Close => ui::menu_row("term-menu-close", "x", "Close terminal", Some("⌘W")),
                Item::Kill => ui::danger_row("term-menu-kill", "stop", "Kill session"),
            };
            let pane = pane.to_string();
            menu = menu.child(row.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                this.terminal.menu = None;
                match item {
                    Item::Copy => this.copy_selection(&CopySelection, window, cx),
                    Item::Paste => this.paste(&Paste, window, cx),
                    Item::Clear => this.clear_pane(&pane, cx),
                    Item::Close => {
                        if this.terminals.may_close(&pane, this.agents.observe_only()) && !this.ask_close(vec![pane.clone()], cx) {
                            this.close_pane(&pane, cx);
                        }
                    }
                    Item::Kill => this.kill_pane(&pane),
                }
                cx.notify();
            })));
        }
        Some(deferred(anchored().position(*at).snap_to_window_with_margin(px(8.)).child(ui::menu_in("term-menu-in", menu))).with_priority(1))
    }
}

#[cfg(test)]
mod tests {
    use super::{Item, items};

    #[test]
    fn a_running_shell_offers_every_row_but_copy_until_text_is_selected() {
        assert_eq!(items(false, true, false), [Item::Paste, Item::Clear, Item::Close, Item::Kill]);
        assert_eq!(items(true, true, false), [Item::Copy, Item::Paste, Item::Clear, Item::Close, Item::Kill]);
    }

    #[test]
    fn a_full_screen_program_keeps_its_screen_from_clear() {
        assert_eq!(items(false, true, true), [Item::Paste, Item::Close, Item::Kill]);
    }

    #[test]
    fn a_terminal_that_takes_no_input_can_only_be_copied_from_and_closed() {
        assert_eq!(items(true, false, false), [Item::Copy, Item::Close]);
    }
}
