use super::Layout;
use super::logic::Order;
use super::parts::choice;
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

fn setting_row(label: &'static str) -> Div {
    div().h(px(32.)).flex().items_center().justify_between().text_size(px(13.)).text_color(TEXT_2).child(label)
}

impl Desktop {
    pub(super) fn display_menu(&self, cx: &mut Context<Self>) -> Div {
        let layouts = [(Layout::List, "list", "List"), (Layout::Board, "blocks", "Board")].into_iter().enumerate().map(|(i, (l, glyph, label))| {
            let on = self.board.layout == l;
            div()
                .id(("board-layout", i))
                .flex_1()
                .h(px(56.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(5.))
                .rounded(px(8.))
                .border_1()
                .border_color(if on { SEPARATOR_STRONG } else { SEPARATOR })
                .cursor_pointer()
                .when(on, |d| d.bg(FILL_2))
                .text_size(px(12.5))
                .font_weight(FontWeight::MEDIUM)
                .text_color(if on { TEXT } else { TEXT_2 })
                .child(icon(glyph, 15., if on { TEXT } else { TEXT_3 }))
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.board.layout = l;
                    cx.notify();
                }))
        }).collect::<Vec<_>>();
        let orders = [(Order::Activity, "Last activity"), (Order::Created, "Created"), (Order::Name, "Name")].into_iter().enumerate().map(|(i, (o, label))| {
            choice(("board-order", i), label, self.board.order == o).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
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
        let stepper = div().flex().items_center().text_color(TEXT).child(less).child(div().w(px(20.)).text_center().child(limit.to_string())).child(more);
        let closed = div().id("board-show-closed").cursor_pointer().child(ui::toggle("board-show-closed-switch", self.board.show_closed)).on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.board.show_closed = !this.board.show_closed;
            this.board_changed();
            cx.notify();
        }));
        let body = div().p(px(7.)).flex().flex_col().gap(px(8.)).child(div().flex().gap(px(6.)).children(layouts)).child(div().h(px(1.)).bg(HAIRLINE)).child(setting_row("Ordering").child(div().flex().gap(px(2.)).children(orders))).child(setting_row("Work on at once").child(stepper)).child(setting_row("Show closed").child(closed));
        ui::dropdown_right(30., ui::menu_in("board-display-in", self.popup("board-display-menu", 300., cx).child(body)))
    }
}
