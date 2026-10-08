use crate::desktop::Desktop;
use crate::modals::new_session::Access;
use crate::modals::new_session::picker::access_row;
use gpui_kit::*;
use theme::*;

impl Desktop {
    pub(super) fn phone_access_view(&mut self, cx: &mut Context<Self>) -> Div {
        let rows = [Access::Ask, Access::Edits, Access::Auto].map(|a| {
            access_row(a.wire(), self.agents.phone_max == a.wire(), None, a.label(), a.hint(), TEXT)
                .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.outbox.set_phone_access(a.wire());
                    this.close_overlay(window, cx);
                }))
                .into_any_element()
        });
        let close = ui::icon_button("phone-access-close", "x").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        ui::modal("Phone access level", 400., 160., close, rows)
    }
}
