use super::parts::key_chip;
use super::{Board, BoardEvent};
use gpui_kit::component::input::Input;
use gpui_kit::*;
use theme::*;

impl Board {
    /// The bar's title and the search beside it.
    pub(super) fn board_bar(&self, cx: &mut Context<Self>) -> (Div, Div) {
        let searching = self.searching || !self.filters.query.is_empty();
        let field = searching.then(|| {
            div()
                .w(px(240.))
                .h(px(26.))
                .px(px(8.))
                .flex()
                .items_center()
                .gap(px(6.))
                .rounded(px(7.))
                .bg(FILL_1)
                .child(icon("search", 12., TEXT_3))
                .child(div().flex_1().min_w_0().text_size(px(12.5)).child(Input::new(&self.search).appearance(false).p_0()))
                .child(key_chip("/", FILL_2))
        });
        let toggle = ui::icon_button_sized("board-search", "search", 26., TEXT_2)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_board_search(window, cx)));
        let right = div().flex().items_center().gap(px(4.)).children(field).child(toggle);
        let title = div()
            .ml(px(2.))
            .flex_1()
            .min_w_0()
            .flex()
            .items_center()
            .gap(px(7.))
            .text_size(px(13.))
            .font_weight(FontWeight::MEDIUM)
            .child(icon("blocks", 14., TEXT_3))
            .child("Board");
        (title, right)
    }

    pub(super) fn toggle_board_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.set_search(!self.searching, window, cx);
    }

    pub(super) fn set_search(&mut self, open: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.searching = open;
        if open {
            self.search.update(cx, |s, cx| s.focus(window, cx));
        } else {
            self.filters.query.clear();
            self.search.update(cx, |s, cx| s.set_value("", window, cx));
            cx.emit(BoardEvent::Changed);
            cx.emit(BoardEvent::FocusRoot);
        }
        cx.notify();
    }
}
