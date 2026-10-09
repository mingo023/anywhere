use super::{catalog, nav_groups};
use gpui_kit::component::input::Input;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, HEADER, drag_area};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(crate) fn settings_nav(&self, cx: &mut Context<Self>) -> Div {
        let words = self.settings.query(cx);
        let hits = catalog::search(&words);
        let searching = !words.is_empty();
        let groups = nav_groups().into_iter().map(|(title, sections)| {
            let items = sections.into_iter().map(|s| {
                let count = hits.iter().find(|(h, _)| *h == s).map_or(0, |(_, rows)| rows.len());
                let selected = !searching && self.settings.section.nav() == s;
                item(("settings-section", s as usize), s.icon(), s.label(), selected)
                    .when(searching && count == 0, |d| d.opacity(0.45))
                    .when(count > 0, |d| d.child(div().ml_auto().min_w(px(18.)).h(px(16.)).px(px(5.)).flex().items_center().justify_center().rounded(px(8.)).bg(FILL_3).text_size(px(11.)).font_weight(FontWeight::MEDIUM).text_color(TEXT_2).child(count.to_string())))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                        this.show_section(s, cx);
                        this.settings.search.update(cx, |q, cx| q.set_value("", window, cx));
                        cx.notify();
                    }))
            });
            div()
                .flex()
                .flex_col()
                .gap(px(1.))
                .child(div().px(px(8.)).pb(px(4.)).text_size(px(11.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_4).child(title))
                .children(items)
        });
        let empty = self.settings.search.read(cx).value().is_empty();
        let search = div()
            .h(px(30.))
            .px(px(9.))
            .flex()
            .items_center()
            .gap(px(7.))
            .rounded(px(8.))
            .bg(FILL_3)
            .child(icon("search", 14., TEXT_3))
            .child(div().flex_1().min_w_0().text_size(px(13.)).child(Input::new(&self.settings.search).appearance(false).p_0().text_size(px(13.))))
            .when(empty, |d| d.child(div().flex_none().text_size(px(11.5)).text_color(TEXT_4).child("⌘F")));
        ui::side(div())
            .w(px(self.width(Column::Projects, 272.)))
            .flex_none()
            .h_full()
            .flex()
            .flex_col()
            .child(drag_area(div()).h(px(HEADER)).flex_none())
            .child(div().px(px(12.)).pb(px(10.)).child(search))
            .child(div().pt(px(2.)).px(px(10.)).pb(px(12.)).flex().flex_col().gap(px(14.)).children(groups))
    }
}

fn item(id: impl Into<ElementId>, icon_name: &str, label: &'static str, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(30.))
        .px(px(8.))
        .flex()
        .items_center()
        .gap(px(9.))
        .rounded(px(7.))
        .cursor_pointer()
        .text_size(px(13.))
        .when(selected, |d| d.bg(FILL_4).text_color(TEXT).font_weight(FontWeight::SEMIBOLD))
        .when(!selected, |d| d.text_color(TEXT_BODY).font_weight(FontWeight(450.)).hover(|h| h.bg(FILL_2)))
        .child(icon(icon_name, 17., if selected { TEXT } else { TEXT_3 }))
        .child(label)
}
