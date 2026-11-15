use super::Layout;
use super::layouts;
use super::logic::{Filters, Tab};
use super::views::view_is_on;
use crate::desktop::Desktop;
use crate::factory::station::Needs;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

fn row_divider() -> Div {
    div().mx(px(3.)).w(px(1.)).h(px(16.)).flex_none().bg(SEPARATOR)
}

fn tab_chip(id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
    div().id(id).h(px(26.)).px(px(if on { 10. } else { 9. })).flex().flex_none().items_center().gap(px(6.)).rounded(px(13.)).cursor_pointer().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).whitespace_nowrap().text_color(if on { TEXT } else { TEXT_3 }).when(on, |d| d.bg(FILL_3)).when(!on, |d| d.border_1().border_color(SEPARATOR).hover(|s| s.bg(FILL_1).text_color(TEXT)))
}

impl Desktop {
    pub(super) fn board_tabs(&self, cx: &mut Context<Self>) -> Div {
        let switch = self.layout_switch(cx);
        if self.board.layout == Layout::Factory {
            let row = div().id("board-tab-row").flex_1().min_w_0().overflow_x_scroll().flex().items_center().gap(px(5.)).child(switch).child(row_divider()).children(self.needs_chips(cx)).child(self.slots_text());
            return div().h(px(42.)).flex_none().pl(px(14.)).pr(px(12.)).flex().items_center().gap(px(8.)).child(row).child(self.board_tools(cx));
        }
        let view_on = self.store.board_views.iter().position(|v| view_is_on(v, &self.board.filters, self.board.tab));
        let tabs: Vec<_> = Tab::ALL
            .into_iter()
            .enumerate()
            .map(|(i, tab)| {
                tab_chip(("board-tab", i), self.board.tab == tab && view_on.is_none()).child(tab.name()).child(div().font_weight(FontWeight::NORMAL).text_color(TEXT_4).child(self.board.counts[i].to_string())).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    if view_on.is_some() {
                        this.board.filters = Filters { query: std::mem::take(&mut this.board.filters.query), ..Default::default() };
                    }
                    this.board.tab = tab;
                    this.board_changed();
                    cx.notify();
                }))
            })
            .collect();
        let views: Vec<_> = self
            .store
            .board_views
            .iter()
            .enumerate()
            .map(|(i, view)| {
                let on = view_on == Some(i);
                let remove = on.then(|| {
                    ui::icon_button_sized(("board-view-x", i), "x", 16., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        cx.stop_propagation();
                        this.remove_board_view(i, cx);
                    }))
                });
                tab_chip(("board-view", i), on).child(icon("list", 12., TEXT_3)).child(div().max_w(px(160.)).truncate().child(view.name.clone())).children(remove).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.apply_board_view(i, cx)))
            })
            .collect();
        let row = div().id("board-tab-row").flex_1().min_w_0().overflow_x_scroll().flex().items_center().gap(px(5.)).child(switch).child(row_divider()).children(tabs).children(views);
        div().h(px(42.)).flex_none().pl(px(14.)).pr(px(12.)).flex().items_center().gap(px(8.)).child(row).child(self.board_tools(cx))
    }

    /// The Factory, Status and List switch, first in the row.
    fn layout_switch(&self, cx: &mut Context<Self>) -> Stateful<Div> {
        let segments = layouts(self.agents.factory_offered()).into_iter().map(|l| ui::Segment { icon: None, value: l, label: l.name().into(), badge: None }).collect();
        div().id("board-layout").flex_none().child(ui::segmented(segments, self.board.layout, true, false, |this: &mut Self, l, cx| this.set_board_layout(l, cx), cx))
    }

    /// `needs you`, `ask` and `failed`, each with its count. Only the active "needs you" chip is tinted.
    fn needs_chips(&self, cx: &mut Context<Self>) -> Vec<Stateful<Div>> {
        let counts = self.factory.counts;
        Needs::ALL
            .into_iter()
            .enumerate()
            .map(|(i, needs)| {
                let on = self.board.filters.needs == Some(needs);
                let tinted = on && needs == Needs::You;
                tab_chip(("board-needs", i), on)
                    .when(tinted, |d| d.bg(WAITING_BG).text_color(WAITING_TEXT))
                    .child(div().font_weight(FontWeight::NORMAL).text_color(if tinted { WAITING_TEXT } else { TEXT_4 }).child(counts.of(needs).to_string()))
                    .child(needs.name())
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.board.filters.needs = Needs::toggled(this.board.filters.needs, needs);
                        this.board_changed();
                        cx.notify();
                    }))
            })
            .collect()
    }

    /// `3/4 slots`: the tickets that hold a slot, and the Board's limit.
    fn slots_text(&self) -> Div {
        let used = div().font_family(MONO).child(format!("{}/{}", self.factory.counts.slots, self.agents.board.limit));
        div().ml(px(6.)).flex().flex_none().items_center().gap(px(4.)).text_size(px(12.5)).text_color(TEXT_3).whitespace_nowrap().child(used).child("slots")
    }
}
