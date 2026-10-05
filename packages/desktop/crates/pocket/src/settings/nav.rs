use super::Section;
use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, HEADER, drag_area};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(crate) fn settings_nav(&self, cx: &mut Context<Self>) -> Div {
        let items = Section::ALL.into_iter().enumerate().map(|(i, s)| {
            let selected = self.settings.section == s;
            item(("settings-section", i), s.icon(), s.label(), selected).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.settings.section = s;
                cx.notify();
            }))
        });
        let back = ui::icon_button("settings-back", "back").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_settings(window, cx)));
        let title = div().h(px(36.)).mb(px(8.)).flex().items_center().gap(px(4.)).child(back).child(div().text_size(px(15.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT).child("Settings"));
        ui::side(div())
            .w(px(self.width(Column::Projects, 272.)))
            .flex_none()
            .h_full()
            .px(px(8.))
            .flex()
            .flex_col()
            .child(drag_area(div()).h(px(HEADER)).flex_none())
            .child(title)
            .child(div().flex().flex_col().gap(px(2.)).children(items))
    }
}

fn item(id: impl Into<ElementId>, icon_name: &str, label: &'static str, selected: bool) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(32.))
        .px(px(10.))
        .flex()
        .items_center()
        .gap(px(8.))
        .rounded(px(8.))
        .cursor_pointer()
        .text_size(px(13.5))
        .when(selected, |d| d.bg(FILL_4).text_color(TEXT).font_weight(FontWeight::SEMIBOLD))
        .when(!selected, |d| d.text_color(TEXT_2).font_weight(FontWeight::MEDIUM).hover(|h| h.bg(FILL_2)))
        .child(icon(icon_name, 15., if selected { TEXT } else { TEXT_2 }))
        .child(label)
}
