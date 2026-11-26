use super::pane::{Look, PaneEvent, TerminalPane};
use gpui_kit::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Item {
    Copy,
    Paste,
    Clear,
    Close,
    Kill,
}

/// What the pane under the menu allows.
#[derive(Clone, Copy)]
pub struct Pane {
    pub selected: bool,
    pub writable: bool,
    pub closable: bool,
    /// A full-screen program or an agent's TUI draws the whole screen, so clearing it would leave a blank one.
    pub owns_screen: bool,
}

/// The rows a pane's menu offers. Only a terminal that takes input can be pasted into, cleared or killed.
pub fn items(p: Pane) -> Vec<Item> {
    let mut items = Vec::new();
    if p.selected {
        items.push(Item::Copy);
    }
    if p.writable {
        items.push(Item::Paste);
        if !p.owns_screen {
            items.push(Item::Clear);
        }
    }
    if p.closable {
        items.push(Item::Close);
    }
    if p.writable {
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

impl TerminalPane {
    pub(super) fn menu(at: Point<Pixels>, look: &Look, cx: &mut Context<Self>) -> AnyElement {
        let term = look.grid.as_ref();
        let facts = Pane {
            selected: term.is_some_and(|t| t.get().selection_text().is_some()),
            writable: look.known && look.may_send,
            closable: look.may_close,
            owns_screen: term.is_some_and(|t| t.get().alt_screen()) || look.has_agent,
        };
        let mut menu = ui::pop(div().id("term-menu"))
            .w(px(200.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|_, _: &MouseDownEvent, _, cx| cx.emit(PaneEvent::CloseMenu)));
        let mut prev = None;
        for item in items(facts) {
            if prev.is_some_and(|p| p != group(item)) {
                menu = menu.child(ui::menu_divider());
            }
            prev = Some(group(item));
            let row = match item {
                Item::Copy => ui::menu_row("term-menu-copy", "copy", "Copy", crate::settings::hint(&crate::actions::CopySelection, cx).as_deref()),
                Item::Paste => ui::menu_row("term-menu-paste", "compose", "Paste", crate::settings::hint(&crate::actions::Paste, cx).as_deref()),
                Item::Clear => ui::menu_row("term-menu-clear", "discard", "Clear", None),
                Item::Close => ui::menu_row("term-menu-close", "x", "Close terminal", crate::settings::hint(&crate::actions::CloseTab, cx).as_deref()),
                Item::Kill => ui::danger_row("term-menu-kill", "stop", "Kill session"),
            };
            menu = menu.child(row.on_click(cx.listener(move |_, _: &ClickEvent, _, cx| cx.emit(PaneEvent::Pick(item)))));
        }
        deferred(anchored().position(at).snap_to_window_with_margin(px(8.)).child(ui::menu_in("term-menu-in", menu))).with_priority(1).into_any_element()
    }
}

#[cfg(test)]
mod tests {
    use super::{Item, Pane, items};

    const SHELL: Pane = Pane { selected: false, writable: true, closable: true, owns_screen: false };

    #[test]
    fn a_running_shell_offers_every_row_but_copy_until_text_is_selected() {
        assert_eq!(items(SHELL), [Item::Paste, Item::Clear, Item::Close, Item::Kill]);
        assert_eq!(items(Pane { selected: true, ..SHELL }), [Item::Copy, Item::Paste, Item::Clear, Item::Close, Item::Kill]);
    }

    #[test]
    fn a_program_that_owns_the_screen_keeps_it_from_clear() {
        assert_eq!(items(Pane { owns_screen: true, ..SHELL }), [Item::Paste, Item::Close, Item::Kill]);
    }

    #[test]
    fn a_terminal_that_takes_no_input_can_only_be_copied_from_and_closed() {
        assert_eq!(items(Pane { selected: true, writable: false, ..SHELL }), [Item::Copy, Item::Close]);
    }

    #[test]
    fn a_terminal_that_may_not_be_closed_offers_no_close() {
        assert_eq!(items(Pane { selected: true, writable: false, closable: false, owns_screen: false }), [Item::Copy]);
    }
}
