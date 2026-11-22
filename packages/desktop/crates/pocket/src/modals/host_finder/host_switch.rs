use crate::desktop::Desktop;
use crate::spaces::Space;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use hosts::remote;
use theme::*;
use ui::Variant;

impl Desktop {
    pub(super) fn finder_host_switch(&self, name: &str, online: bool, cx: &mut Context<Self>) -> Div {
        let open = self.host_finder.host_menu;
        let glyph = self.host_finder.finder.as_ref().map_or("server", |f| self.space_glyph(&Space::Host(f.host.clone())));
        let button = ui::button(
            "finder-host",
            Variant::Secondary,
            Some(glyph),
            div().flex().items_center().gap(px(6.)).child(name.to_string()).child(ui::dot(6., if online { SUCCESS } else { TEXT_5 })).child(icon(
                "chevron-down",
                12.,
                TEXT_3,
            )),
        )
        .h(px(28.))
        .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
            this.host_finder.host_menu = !this.host_finder.host_menu;
            cx.notify();
        }));
        let picked = self.host_finder.finder.as_ref().map(|f| f.host.clone());
        let opener = self.host_finder.opener;
        let menu = open.then(|| {
            let rows = self.fleet.hosts.list.iter().enumerate().map(|(i, h)| {
                let up = remote::connected(&h.id).is_some();
                let id = h.id.clone();
                div()
                    .id(("finder-host-row", i))
                    .h(px(32.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .gap(px(9.))
                    .rounded(px(7.))
                    .text_size(px(13.))
                    .cursor_pointer()
                    .hover(|s| s.bg(FILL_3))
                    .child(div().w(px(14.)).when(picked.as_deref() == Some(h.id.as_str()), |d| d.child(icon("check", 14., TEXT))))
                    .child(icon(self.space_glyph(&Space::Host(h.id.clone())), 14., TEXT_3))
                    .child(div().flex_1().truncate().when(!up, |d| d.text_color(TEXT_3)).child(self.host_name(&h.id)))
                    .child(div().text_size(px(11.5)).text_color(TEXT_3).child(if up { "" } else { "Offline" }))
                    .child(ui::dot(6., if up { SUCCESS } else { TEXT_5 }))
                    .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.open_host_finder(id.clone(), None, opener, window, cx)))
            });
            ui::dropdown(34., ui::pop(div().w(px(240.)).p(px(5.)).flex().flex_col()).occlude().children(rows))
        });
        div().relative().child(button).children(menu)
    }
}
