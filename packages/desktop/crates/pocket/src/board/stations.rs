use super::logic::{Slot, column as status_column};
use super::parts::station_glyph;
use crate::desktop::Desktop;
use crate::factory::station::{COLUMNS, Column, GAP, PAD, RAIL_W, widths};
use agents::tickets::Gate;
use gpui_kit::*;
use theme::*;

const COL_RADIUS: f32 = 10.;
const LETTER_H: f32 = 13.;

impl Desktop {
    /// A column's mark, name and count, and a quiet "You" where you decide.
    pub(super) fn station_heading(&self, column: Column) -> Div {
        let count = self.factory.columns[column as usize].len().to_string();
        let you = column.yours().then(|| div().ml_auto().text_size(px(12.)).font_weight(FontWeight::NORMAL).text_color(TEXT_3).child("You"));
        div()
            .h(px(38.))
            .flex_none()
            .px(px(12.))
            .flex()
            .items_center()
            .gap(px(8.))
            .text_size(px(12.5))
            .font_weight(FontWeight::MEDIUM)
            .child(station_glyph(column, Gate::None, false))
            .child(column.name())
            .child(div().font_weight(FontWeight::NORMAL).text_color(TEXT_3).child(count))
            .children(you)
    }

    /// A closed rail: the mark, the name one letter per line, the count. A click opens it.
    pub(super) fn station_rail(&self, column: Column, cx: &mut Context<Self>) -> Stateful<Div> {
        let letters = column.name().chars().map(|c| div().h(px(LETTER_H)).child(c.to_string()));
        let name = div().flex().flex_col().items_center().line_height(px(LETTER_H)).font_weight(FontWeight::MEDIUM).text_color(TEXT).children(letters);
        let count = self.factory.columns[column as usize].len().to_string();
        div()
            .id(("board-rail", column as usize))
            .w(px(RAIL_W))
            .h_full()
            .pt(px(12.))
            .flex()
            .flex_none()
            .flex_col()
            .items_center()
            .gap(px(8.))
            .rounded(px(COL_RADIUS))
            .bg(FILL_1)
            .cursor_pointer()
            .hover(|s| s.bg(FILL_2))
            .text_size(px(12.5))
            .text_color(TEXT_3)
            .child(station_glyph(column, Gate::None, false))
            .child(name)
            .child(count)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_station_rail(column, cx)))
    }

    fn toggle_station_rail(&mut self, column: Column, cx: &mut Context<Self>) {
        self.factory.toggle(column);
        if self.factory.open[column as usize] {
            self.factory.scroll.scroll_to_item(column as usize);
        }
        self.board_changed();
        cx.notify();
    }

    fn station_column(&mut self, column: Column, width: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let cards = list(self.factory.lists[column as usize].clone(), cx.processor(move |this, ix, _, cx| {
            let index = this.factory.columns[column as usize].get(ix).copied();
            let col = index.and_then(|i| this.agents.board.tickets.get(i)).map(status_column);
            match (index, col) {
                (Some(index), Some(col)) => this.ticket_card(Slot { col, pos: ix, index }, cx),
                _ => div().into_any_element(),
            }
        }))
        .flex_1()
        .w_full();
        let heading = self.station_heading(column);
        let heading = if column.rail() {
            div().id(("board-rail-close", column as usize)).cursor_pointer().child(heading).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.toggle_station_rail(column, cx))).into_any_element()
        } else {
            heading.into_any_element()
        };
        div().id(("board-station", column as usize)).w(px(width)).h_full().flex().flex_none().flex_col().rounded(px(COL_RADIUS)).bg(FILL_1).child(heading).child(cards)
    }

    /// The Factory layout: seven columns on a page `available` wide, scrolling sideways when they don't fit.
    pub(super) fn station_columns(&mut self, available: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let widths = widths(available, &self.factory.open);
        let columns: Vec<AnyElement> = COLUMNS
            .into_iter()
            .map(|c| {
                if c.rail() && !self.factory.open[c as usize] {
                    self.station_rail(c, cx).into_any_element()
                } else {
                    self.station_column(c, widths[c as usize], cx).into_any_element()
                }
            })
            .collect();
        div().id("board-stations").flex_1().min_h_0().overflow_x_scroll().track_scroll(&self.factory.scroll).px(px(PAD)).pb(px(8.)).flex().gap(px(GAP)).children(columns)
    }
}
