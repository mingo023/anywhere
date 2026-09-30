pub(crate) mod add_project;
mod confirm;
pub(crate) mod form;
mod more;
pub(crate) mod new_session;

use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use gpui_kit::*;
use std::time::Duration;

impl Desktop {
    pub fn open(&mut self, o: Overlay, window: &mut Window, cx: &mut Context<Self>) {
        self.close_menus();
        self.overlay = Some(o);
        match o {
            Overlay::Palette => self.open_palette(window, cx),
            Overlay::NewSession => self.reset_new_form(None, false, window, cx),
            Overlay::AddRepo => self.reset_repo_form(None, window, cx),
            Overlay::More | Overlay::Confirm => {}
        }
        cx.notify();
    }

    pub fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = None;
        window.focus(&self.root, cx);
        cx.notify();
    }

    pub fn overlay_view(&mut self, window: &mut Window, cx: &mut Context<Self>) -> Option<AnyElement> {
        let o = self.overlay?;
        // Entrance as (ms, rise). The palette and new-session sheet open from shortcuts many times a day; motion would only slow them.
        let (body, alpha, entrance) = match o {
            Overlay::Palette => (self.palette(cx), 0x1f, None),
            Overlay::NewSession => (self.new_session_view(window, cx), 0x2e, None),
            Overlay::AddRepo => (self.repo_view(window, cx), 0x40, Some((200, 8.))),
            Overlay::More => (self.more_menu(cx), 0, Some((150, -4.))),
            Overlay::Confirm => (self.confirm_view(cx), 0x2e, Some((200, 8.))),
        };
        let backdrop = ui::backdrop("backdrop", alpha).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.close_overlay(window, cx)));
        let layer = div().absolute().inset_0();
        let Some((ms, rise)) = entrance else {
            return Some(layer.child(backdrop).child(body).into_any_element());
        };
        let enter = Animation::new(Duration::from_millis(ms)).with_easing(ease_out_quint());
        Some(
            layer
                .child(backdrop.with_animation("backdrop-in", enter.clone(), |d, t| d.opacity(t)))
                .child(body.with_animation("overlay-in", enter, move |d, t| d.opacity(t).mt(px(rise * (1. - t)))))
                .into_any_element(),
        )
    }
}
