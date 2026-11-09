use super::logic::Tab;
use super::parts::choice;
use crate::desktop::Desktop;
use gpui_kit::*;

impl Desktop {
    pub(super) fn board_tabs(&self, cx: &mut Context<Self>) -> Div {
        let tabs: Vec<_> = Tab::ALL
            .into_iter()
            .enumerate()
            .map(|(i, tab)| {
                choice(("board-tab", i), tab.name(), self.board.tab == tab).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.board.tab = tab;
                    this.board_changed();
                    cx.notify();
                }))
            })
            .collect();
        div().h(px(40.)).flex_none().px(px(16.)).flex().items_center().gap(px(2.)).children(tabs).child(div().flex_1()).child(self.board_tools(cx))
    }
}
