use crate::desktop::Desktop;
use gpui_kit::component::tooltip::Tooltip;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;

impl Desktop {
    /// The title, the path bar, the dot-file toggle, the host switch and close.
    pub(super) fn finder_header(&self, name: &str, online: bool, cx: &mut Context<Self>) -> Div {
        let Some(f) = self.host_finder.finder.as_ref() else { return div() };
        let (hidden, crumbs) = (f.hidden, f.crumbs());
        let close = ui::icon_button("finder-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
            this.close_host_finder(window, cx);
        }));
        let eye = ui::icon_button("finder-hidden", if hidden { "eye" } else { "eye-off" })
            .tooltip(move |window, cx| Tooltip::new(if hidden { "Hide dot files" } else { "Show dot files" }).build(window, cx))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.finder_toggle_hidden(cx)));
        let crumbs = div().flex_1().min_w_0().flex().items_center().overflow_hidden().font_family(MONO).text_size(px(12.5)).text_color(TEXT_2).children(
            crumbs.into_iter().enumerate().map(|(i, (label, dir))| {
                div().flex().items_center().when(i > 0, |d| d.child(div().px(px(2.)).text_color(TEXT_5).child("/"))).child(
                    div()
                        .id(("finder-crumb", i))
                        .px(px(5.))
                        .py(px(2.))
                        .rounded(px(5.))
                        .cursor_pointer()
                        .hover(|s| s.bg(FILL_3).text_color(TEXT))
                        .child(label)
                        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.finder_go(dir.clone(), cx))),
                )
            }),
        );
        div()
            .px(px(14.))
            .h(px(52.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(10.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(
                div()
                    .flex_none()
                    .text_size(px(13.))
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_color(TEXT_3)
                    .child(self.finder_ask(cx).map_or("Choose a folder", |a| a.title)),
            )
            .child(crumbs)
            .child(eye)
            .child(self.finder_host_switch(name, online, cx))
            .child(close)
    }
}
