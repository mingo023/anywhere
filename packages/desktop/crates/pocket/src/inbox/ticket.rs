use super::FactoryRow;
use crate::desktop::Desktop;
use crate::desktop::chrome::drag_area;
use gpui_kit::*;
use theme::*;
use ui::{self, Variant, kbd};

impl Desktop {
    /// A Factory row's detail: the Ticket's panel and the same footer bar the Factory ticket page draws.
    pub(super) fn inbox_ticket(&mut self, f: &FactoryRow, cx: &mut Context<Self>) -> Div {
        let (Some(t), Some(page)) = (self.agents.board.get(&f.ticket).cloned(), self.inbox.page.clone()) else { return nothing_waiting() };
        let key = self.ticket_key(&t);
        let id = t.id.clone();
        let header = drag_area(div())
            .h(px(44.))
            .flex_none()
            .px(px(20.))
            .flex()
            .items_center()
            .gap(px(6.))
            .text_size(px(12.))
            .text_color(TEXT_2)
            .child(div().flex_1())
            .child(kbd("J"))
            .child(kbd("K"))
            .child("move")
            .child(kbd("⌘↵"))
            .child("decide");
        let open = ui::button("open-ticket", Variant::Ghost, None, "Open ticket").child(icon("chevron-right", 14., TEXT)).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
            this.open_board(window, cx);
            this.open_factory_ticket(&id, window, cx);
        }));
        let above = div().flex().items_center().text_size(px(12.)).text_color(TEXT_3).children(key).child(div().flex_1()).child(open);
        let pane = self.factory_pane(&t, &page, Some(above), cx);
        div().flex_1().min_h_0().flex().flex_col().child(header).child(pane)
    }
}

/// The detail pane with nothing selected.
pub(super) fn nothing_waiting() -> Div {
    drag_area(div()).flex_1().flex().items_center().justify_center().text_size(px(14.)).text_color(TEXT_3).child("You're all caught up.")
}
