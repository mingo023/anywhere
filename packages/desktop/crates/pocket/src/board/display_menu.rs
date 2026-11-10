use super::Layout;
use super::logic::{Order, Props};
use super::parts::choice;
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

type PropField = fn(&mut Props) -> &mut bool;

const PROPS: [(&str, PropField); 6] = [("ID", |p| &mut p.id), ("Priority", |p| &mut p.priority), ("Labels", |p| &mut p.labels), ("Project", |p| &mut p.project), ("Agent", |p| &mut p.agent), ("Updated", |p| &mut p.updated)];

fn setting_row(label: &'static str) -> Div {
    div().h(px(32.)).flex().items_center().justify_between().text_size(px(13.)).text_color(TEXT_2).child(label)
}

fn section(label: &'static str) -> Div {
    div().flex().flex_col().gap(px(5.)).child(div().text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(TEXT_3).child(label))
}

impl Desktop {
    pub(super) fn display_menu(&self, cx: &mut Context<Self>) -> Div {
        let layouts = [(Layout::List, "list", "List"), (Layout::Board, "blocks", "Board")].into_iter().enumerate().map(|(i, (l, glyph, label))| {
            let on = self.board.layout == l;
            div()
                .id(("board-layout", i))
                .flex_1()
                .h(px(52.))
                .flex()
                .flex_col()
                .items_center()
                .justify_center()
                .gap(px(4.))
                .rounded(px(9.))
                .when(on, |d| d.bg(FILL_3))
                .when(!on, |d| d.border_1().border_color(SEPARATOR))
                .cursor_pointer()
                .text_size(px(12.))
                .text_color(if on { TEXT } else { TEXT_3 })
                .child(icon(glyph, 14., if on { TEXT } else { TEXT_3 }))
                .child(label)
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.board.layout = l;
                    this.store.board_list = l == Layout::List;
                    this.store.save();
                    this.board_changed();
                    cx.notify();
                }))
        }).collect::<Vec<_>>();
        let orders = [(Order::Activity, "Updated"), (Order::Created, "Created"), (Order::Priority, "Priority"), (Order::Name, "Name")].into_iter().enumerate().map(|(i, (o, label))| {
            choice(("board-order", i), label, self.board.order == o).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.board.order = o;
                this.board_changed();
                cx.notify();
            }))
        }).collect::<Vec<_>>();
        let props = PROPS.into_iter().enumerate().map(|(i, (label, pick))| {
            let mut current = self.board.props;
            choice(("board-prop", i), label, *pick(&mut current)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                let on = pick(&mut this.board.props);
                *on = !*on;
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
        let body = div()
            .p(px(7.))
            .flex()
            .flex_col()
            .gap(px(10.))
            .child(div().flex().gap(px(6.)).children(layouts))
            .child(section("Ordering").child(div().flex().flex_wrap().gap(px(4.)).children(orders)))
            .child(div().h(px(1.)).bg(HAIRLINE))
            .child(div().flex().flex_col().child(setting_row("Work on at once").child(stepper)).child(setting_row("Show closed tickets").child(closed)))
            .child(div().h(px(1.)).bg(HAIRLINE))
            .child(section("Display properties").child(div().flex().flex_wrap().gap(px(4.)).children(props)));
        ui::dropdown_right(30., ui::menu_in("board-display-in", self.popup("board-display-menu", 316., cx).child(body)))
    }
}
