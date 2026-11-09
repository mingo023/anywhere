use super::Layout;
use super::logic::Order;
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn display_menu(&self, cx: &mut Context<Self>) -> Div {
        let layout = [(Layout::Board, "Board"), (Layout::List, "List")].into_iter().enumerate().map(|(i, (l, label))| {
            ui::menu_pick(("board-layout", i), div(), label, self.board.layout == l).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.board.layout = l;
                cx.notify();
            }))
        }).collect::<Vec<_>>();
        let order = [(Order::Activity, "Last activity"), (Order::Created, "Created"), (Order::Name, "Name")].into_iter().enumerate().map(|(i, (o, label))| {
            ui::menu_pick(("board-order", i), div(), label, self.board.order == o).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.board.order = o;
                this.board_changed();
                cx.notify();
            }))
        }).collect::<Vec<_>>();
        let limit = self.agents.board.limit;
        let step = |id: &'static str, name: &'static str, next: usize| {
            ui::icon_button_sized(id, name, 22., TEXT_2).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.outbox.local.board_limit(next.clamp(1, 8));
                cx.notify();
            }))
        };
        let (less, more) = (step("board-limit-less", "minus", limit.saturating_sub(1)), step("board-limit-more", "plus", limit + 1));
        let stepper = div().h(px(32.)).px(px(10.)).flex().items_center().text_size(px(13.5)).child(div().flex_1().child("Work on at once")).child(less).child(div().w(px(20.)).text_center().child(limit.to_string())).child(more);
        let closed = ui::menu_pick("board-show-closed", div(), "Show closed", self.board.show_closed).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.board.show_closed = !this.board.show_closed;
            this.board_changed();
            cx.notify();
        }));
        ui::dropdown_right(30., ui::menu_in("board-display-in", self.popup("board-display-menu", 230., cx).child(ui::menu_label("Layout")).children(layout).child(ui::menu_divider()).child(ui::menu_label("Ordering")).children(order).child(ui::menu_divider()).child(stepper).child(closed)))
    }
}
