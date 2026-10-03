use super::CONTEXT;
use crate::actions::{Back, Forward, Reload};
use crate::desktop::Desktop;
use gpui_kit::component::input::Input;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use workspace::tree::PaneId;

impl Desktop {
    pub(crate) fn browser_view(&mut self, pane: PaneId, id: u64, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if !self.browsers.tabs.contains_key(&id) {
            return div().flex_1();
        }
        let address = self.browsers.address(pane, window, cx);
        let b = &self.browsers.tabs[&id];
        if !address.focus_handle(cx).is_focused(window) && *address.read(cx).value() != *b.url {
            address.update(cx, |s, cx| s.set_value(b.url.clone(), window, cx));
        }
        let back = b.page.as_ref().is_some_and(|p| p.can_go_back());
        let forward = b.page.as_ref().is_some_and(|p| p.can_go_forward());
        let nav = |id: &'static str, name: &str, on: bool| ui::icon_button(id, name).when(!on, |d| d.opacity(0.35));
        let bar = div()
            .h(px(40.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(2.))
            .px(px(8.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .child(nav("browser-back", "back", back).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.back(&Back, window, cx))))
            .child(nav("browser-forward", "forward", forward).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.forward(&Forward, window, cx))))
            .child(ui::icon_button("browser-reload", "reload").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.reload(&Reload, window, cx))))
            .child(
                div()
                    .flex_1()
                    .min_w_0()
                    .h(px(28.))
                    .mx(px(6.))
                    .px(px(10.))
                    .flex()
                    .items_center()
                    .rounded(px(7.))
                    .bg(FILL_2)
                    .text_size(px(13.))
                    .child(Input::new(&address).appearance(false).p_0().text_size(px(13.))),
            )
            .child(ui::icon_button("browser-external", "external").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                if let Some(b) = this.browsers.tabs.get(&id).filter(|b| !b.url.is_empty()) {
                    cx.open_url(&b.url);
                }
            })));
        let page = b.page.clone().filter(|_| self.browsers.shown.contains(&id));
        let shot = b.shot.clone();
        let focused = pane == self.focused_pane();
        let focus = self.browsers.focus.clone();
        let place = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let rect = web::Rect { x: bounds.origin.x.into(), y: bounds.origin.y.into(), width: bounds.size.width.into(), height: bounds.size.height.into() };
                if let Some(page) = &page
                    && page.place(rect)
                    && focused
                    && focus.is_focused(window)
                {
                    page.take_keys();
                }
            },
        )
        .absolute()
        .size_full();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(SURFACE)
            .key_context(CONTEXT)
            .when(focused, |d| d.track_focus(&self.browsers.focus))
            .on_action(cx.listener(Self::focus_address))
            .on_action(cx.listener(Self::reload))
            .on_action(cx.listener(Self::back))
            .on_action(cx.listener(Self::forward))
            .on_action(cx.listener(Self::page_edit))
            .child(bar)
            .child(div().flex_1().min_h_0().relative().child(place).when_some(shot, |d, shot| d.child(img(shot).absolute().inset_0().size_full().opacity(0.6))))
    }
}
