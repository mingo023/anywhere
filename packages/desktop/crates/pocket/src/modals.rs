pub(crate) mod add_project;
mod confirm;
pub(crate) mod form;
mod more;
pub(crate) mod new_session;
pub(crate) mod pair_phone;
mod phone_access;

use crate::desktop::Desktop;
use crate::desktop::chrome::Overlay;
use agents::Agents;
use gpui_kit::*;
use std::time::Duration;

pub(crate) fn may_open(o: Overlay, agents: &Agents) -> bool {
    match o {
        Overlay::NewSession => !agents.observe_only(),
        Overlay::PairPhone => agents.owner() && agents.pairing(),
        Overlay::PhoneAccess => agents.owner(),
        Overlay::Palette | Overlay::AddRepo | Overlay::More | Overlay::Confirm => true,
    }
}

impl Desktop {
    pub fn open(&mut self, o: Overlay, window: &mut Window, cx: &mut Context<Self>) {
        if !may_open(o, &self.agents) {
            return;
        }
        self.close_menus();
        self.overlay = Some(o);
        match o {
            Overlay::Palette => self.open_palette(window, cx),
            Overlay::NewSession => self.reset_new_form(None, false, window, cx),
            Overlay::AddRepo => self.reset_repo_form(None, window, cx),
            Overlay::PairPhone => self.begin_pairing(window, cx),
            Overlay::More | Overlay::Confirm | Overlay::PhoneAccess => {}
        }
        cx.notify();
    }

    pub fn close_overlay(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.overlay = None;
        if self.web_tab(self.focused_pane()).is_some() {
            window.focus(&self.browsers.focus, cx);
        } else if !self.terminal.focus.is_focused(window) {
            window.focus(&self.root, cx);
        }
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
            Overlay::PairPhone => (self.pair_view(cx), 0x2e, Some((200, 8.))),
            Overlay::PhoneAccess => (self.phone_access_view(cx), 0x2e, Some((200, 8.))),
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

#[cfg(test)]
mod tests {
    use super::may_open;
    use crate::desktop::chrome::Overlay;
    use agents::{Agents, Event};

    fn connected(scopes: &[&str], caps: &[&str]) -> Agents {
        let mut a = Agents::default();
        let strings = |v: &[&str]| v.iter().map(|s| s.to_string()).collect();
        a.apply(Event::Connected { scopes: strings(scopes), caps: strings(caps), version: String::new() });
        a
    }

    const OWNER: [&str; 5] = ["observe", "drive", "approve", "spawn", "owner"];

    #[test]
    fn an_observe_only_desktop_cannot_start_a_session() {
        assert!(!may_open(Overlay::NewSession, &connected(&["observe"], &["pair.v1"])));
        assert!(may_open(Overlay::NewSession, &connected(&OWNER, &[])));
        assert!(may_open(Overlay::NewSession, &Agents::default()));
    }

    #[test]
    fn only_the_owner_of_a_pocketd_that_pairs_can_pair_a_phone() {
        assert!(may_open(Overlay::PairPhone, &connected(&OWNER, &["pair.v1", "scopes.v1"])));
        assert!(!may_open(Overlay::PairPhone, &connected(&OWNER, &["scopes.v1"])));
        assert!(!may_open(Overlay::PairPhone, &connected(&["observe"], &["pair.v1"])));
        assert!(!may_open(Overlay::PairPhone, &Agents::default()));
    }
}
