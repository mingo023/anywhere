use super::{group, row};
use crate::desktop::Desktop;
use crate::desktop::sounds::Cue;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::Store;

impl Desktop {
    pub(crate) fn general_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let (banners, sounds) = (self.store.notifications.banners, self.store.sounds);
        let notifications = group(
            "Notifications",
            vec![row("Banners", Some("A macOS banner when a session needs you, finishes or fails."), pref_switch("settings-banners", banners, true, |s| s.notifications.banners = !s.notifications.banners, cx))],
        );
        let mut rows = vec![row("Play sounds", None, pref_switch("settings-sounds", sounds.all, true, |s| s.sounds.all = !s.sounds.all, cx))];
        rows.extend(Cue::ALL.into_iter().map(|cue| {
            let id = SharedString::from(format!("settings-sound-{}", cue.name()));
            row(cue.name(), None, pref_switch(id, cue.on(sounds), sounds.all, move |s| cue.flip(&mut s.sounds), cx)).pl(px(32.)).when(!sounds.all, |d| d.opacity(0.5))
        }));
        div().flex().flex_col().gap(px(24.)).child(notifications).child(group("Sounds", rows))
    }
}

fn pref_switch(id: impl Into<ElementId>, on: bool, enabled: bool, flip: impl Fn(&mut Store) + 'static, cx: &mut Context<Desktop>) -> Stateful<Div> {
    div().id(id).flex_none().child(ui::switch(on)).when(enabled, |d| {
        d.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            flip(&mut this.store);
            this.save_soon(cx);
            cx.notify();
        }))
    })
}
