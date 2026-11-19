use super::{Reach, Space, offline_note, offline_title};
use crate::desktop::Desktop;
use crate::modals::host_finder::Opener;
use gpui_kit::*;
use theme::*;

impl Desktop {
    /// Under Projects, in a host's space it can't reach: what its link is doing, and Reconnect.
    pub(crate) fn offline_card(&self, space: &Space, cx: &mut Context<Self>) -> Option<AnyElement> {
        let (id, reach) = (space.host()?, self.space_reach(space));
        if reach == Reach::Online {
            return None;
        }
        let name = self.space_name(space);
        let link = self.fleet.hosts.links.get(id);
        let target = id.to_string();
        let reconnect = (reach == Reach::Offline).then(|| {
            div().flex().child(
                ui::button("space-offline-reconnect", ui::Variant::Secondary, Some("reload"), "Reconnect")
                    .h(px(26.))
                    .text_size(px(12.5))
                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                        this.retry_host(&target);
                        cx.notify();
                    })),
            )
        });
        Some(
            div()
                .mt(px(4.))
                .mb(px(10.))
                .px(px(12.))
                .py(px(10.))
                .flex()
                .flex_col()
                .gap(px(8.))
                .rounded(px(10.))
                .bg(FILL_2)
                .text_size(px(12.))
                .text_color(TEXT_3)
                .child(div().flex().items_center().gap(px(6.)).text_size(px(12.5)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(icon("server", 13., TEXT_3)).child(offline_title(reach, &name)))
                .child(offline_note(link, &name))
                .children(reconnect)
                .into_any_element(),
        )
    }

    /// A reachable space with no project yet.
    pub(crate) fn empty_space(&self, space: &Space, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.space_reach(space) != Reach::Online {
            return None;
        }
        let name = self.space_name(space);
        let open = ui::button("space-empty-open", ui::Variant::Secondary, None, "Open folder\u{2026}").h(px(26.)).text_size(px(12.5)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open_folder(Opener::Sidebar, window, cx)));
        let clone = ui::button("space-empty-clone", ui::Variant::Secondary, None, "Clone\u{2026}").h(px(26.)).text_size(px(12.5)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.clone_project(window, cx)));
        Some(
            div()
                .mx(px(8.))
                .my(px(20.))
                .flex()
                .flex_col()
                .items_center()
                .gap(px(8.))
                .text_center()
                .text_size(px(12.5))
                .text_color(TEXT_3)
                .child(icon("folder", 22., TEXT_4))
                .child(div().font_weight(FontWeight::MEDIUM).text_color(TEXT).child(format!("No projects on {name} yet")))
                .child("Open a folder that's already there, or clone a repository onto it.")
                .child(div().flex().gap(px(6.)).child(open).child(clone))
                .into_any_element(),
        )
    }
}
