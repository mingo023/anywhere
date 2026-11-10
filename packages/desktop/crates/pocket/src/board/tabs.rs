use super::logic::{Filters, Tab};
use super::views::view_is_on;
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

fn tab_chip(id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
    div().id(id).h(px(26.)).px(px(if on { 10. } else { 9. })).flex().flex_none().items_center().gap(px(6.)).rounded(px(13.)).cursor_pointer().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).whitespace_nowrap().text_color(if on { TEXT } else { TEXT_3 }).when(on, |d| d.bg(FILL_3)).when(!on, |d| d.border_1().border_color(SEPARATOR).hover(|s| s.bg(FILL_1).text_color(TEXT)))
}

impl Desktop {
    pub(super) fn board_tabs(&self, cx: &mut Context<Self>) -> Div {
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
        let row = div().id("board-tab-row").flex_1().min_w_0().overflow_x_scroll().flex().items_center().gap(px(5.)).children(tabs).children(views);
        div().h(px(42.)).flex_none().pl(px(14.)).pr(px(12.)).flex().items_center().gap(px(8.)).child(row).child(self.board_tools(cx))
    }
}
