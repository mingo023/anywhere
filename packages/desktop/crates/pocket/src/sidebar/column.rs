use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, Overlay, Screen, Side, column, drag_area};
use crate::util::basename;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, icon_button_sized};

impl Desktop {
    pub(crate) fn column_view(&mut self, cx: &mut Context<Self>) -> Div {
        let body = match (self.screen, self.side) {
            (Screen::Inbox, _) => {
                let list = self.inbox_list(cx).w(px(self.width(Column::Sessions, 348.)));
                return self.resizable(list, Column::Sessions, cx);
            }
            (_, Side::Sessions) => self.session_list(cx).into_any_element(),
            (_, Side::Explorer) => self.explorer(cx).into_any_element(),
            (_, Side::Changes) => self.changes_list(cx).into_any_element(),
        };
        let totals = self.repo().filter(|r| !r.files.is_empty()).map(|r| r.totals());
        let tabs = [(Side::Sessions, "Sessions"), (Side::Explorer, "Explorer"), (Side::Changes, "Changes")].into_iter().enumerate().map(|(i, (side, label))| {
            let selected = self.side == side;
            div()
                .id(("column-tab", i))
                .flex_1()
                .h(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .rounded(px(8.))
                .cursor_pointer()
                .text_size(px(13.))
                .when(selected, |d| d.bg(FILL_4).text_color(TEXT).font_weight(FontWeight::SEMIBOLD))
                .when(!selected, |d| d.text_color(TEXT_2).font_weight(FontWeight::MEDIUM).hover(|s| s.bg(FILL_2)))
                .map(|d| match (side, totals) {
                    (Side::Changes, Some((added, removed))) => d.child(ui::meta_diff(added, removed, 12.5)),
                    _ => d.child(label),
                })
                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.side = side;
                    this.refresh_graph(cx);
                    cx.notify();
                }))
        });
        let tabs = div().p(px(8.)).flex().flex_none().gap(px(4.)).border_b(px(0.5)).border_color(SEPARATOR).children(tabs);
        let column = column()
            .w(px(self.width(Column::Sessions, 334.)))
            .child(drag_area(self.column_header(cx)).h(px(42.)).flex_none().border_b(px(0.5)).border_color(SEPARATOR))
            .child(tabs)
            .child(body);
        self.resizable(column, Column::Sessions, cx)
    }

    fn column_header(&self, cx: &mut Context<Self>) -> Div {
        let title = self.cwd().map(|t| basename(&t)).or_else(|| self.project.as_deref().map(|p| self.repo_name(p))).unwrap_or_default();
        let add = icon_button_sized("column-add", "plus", 28., TEXT_2)
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::NewSession, window, cx)));
        div()
            .pl(px(18.))
            .pr(px(12.))
            .flex()
            .items_center()
            .gap(px(4.))
            .child(div().flex_1().text_size(px(16.)).font_weight(FontWeight::BOLD).truncate().child(title))
            .child(add)
    }
}
