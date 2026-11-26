mod permission;
mod sounds;

use super::catalog::{Control, Look, Setting};
use super::{DECOR, Decor, Section, Settings, action, focus_ring, group, row};
use crate::desktop::Desktop;
use crate::desktop::sounds::sound_name;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::prefs::terminal::Bell;
use store::{DockBadge, Store};
use theme::*;

const BADGES: [DockBadge; 3] = [DockBadge::Off, DockBadge::NeedsYou, DockBadge::Inbox];
const SOUND: &str = "Notification sound";

fn show_badge(d: &mut Desktop, _: &mut Context<Desktop>) {
    if !d.capturing {
        d.badge.show(&d.agents.list, d.store.notifications.badge);
    }
}

/// By urgency, as the subtitle says.
pub(super) const ROWS: &[Setting] = &[
    Setting::switch("banners", "Banners", "Show banners", |s| s.notifications.banners, |s, on| s.notifications.banners = on)
        .hint("A macOS banner when a session needs you, fails or is done"),
    Setting::switch("banner-needs-you", "Banners", "Needs you", |s| s.notifications.needs_you, |s, on| s.notifications.needs_you = on)
        .hint("An agent asks for an approval or an answer")
        .under(|s| s.notifications.banners),
    Setting::switch("banner-failed", "Banners", "Failed", |s| s.notifications.failed, |s, on| s.notifications.failed = on)
        .hint("An agent exited with an error")
        .under(|s| s.notifications.banners),
    Setting::switch("banner-done", "Banners", "Done", |s| s.notifications.done, |s, on| s.notifications.done = on)
        .hint("An agent completed its turn and is idle")
        .under(|s| s.notifications.banners),
    Setting::switch("on-screen", "Banners", "Alert even when the session is on screen", |s| s.notifications.on_screen, |s, on| s.notifications.on_screen = on)
        .hint("Off: no banner or sound for the pane you are looking at"),
    Setting::switch("sounds", "Sounds", "Play sounds", |s| s.sounds.all, |s, on| s.sounds.all = on),
    Setting::switch("sound-needs-you", "Sounds", "Needs you", |s| s.sounds.needs_you, |s, on| s.sounds.needs_you = on).under(|s| s.sounds.all),
    Setting::switch("sound-failed", "Sounds", "Failed", |s| s.sounds.failed, |s, on| s.sounds.failed = on).under(|s| s.sounds.all),
    Setting::switch("sound-done", "Sounds", "Done", |s| s.sounds.done, |s, on| s.sounds.done = on).under(|s| s.sounds.all),
    Setting::stepper("volume", "Sounds", "Volume", (10, 100, 10), "%", |s| s.sounds.volume as i32, |s, n| s.sounds.volume = n as u32).under(|s| s.sounds.all),
    Setting::custom("notification-sound", SOUND, SOUND, |d, s, cx| d.sound_row(s, cx))
        .hint("Pick a sound or add your own. Custom audio supports .mp3, .wav, and .ogg.")
        .under(|s| s.sounds.all)
        .resets(|s| sound_name(&s.sounds.sound), |s, d| s.sounds.sound = d.sounds.sound.clone()),
    Setting::choice(
        "dock-badge",
        "Dock & bell",
        "Dock badge",
        &["Off", "Needs you count", "Inbox count"],
        Look::Dropdown,
        |s| BADGES.iter().position(|b| *b == s.notifications.badge).unwrap_or(2),
        |s, i| s.notifications.badge = BADGES[i],
    )
    .hint("The number on the app icon")
    .effect(show_badge),
    Setting::choice(
        "terminal-bell",
        "Dock & bell",
        "Terminal bell",
        &["Off", "Sound", "Flash"],
        Look::Segmented,
        |s| s.terminal.bell as usize,
        |s, i| s.terminal.bell = Bell::ALL[i],
    )
    .hint("When a program in a terminal rings the bell"),
];

impl Settings {
    /// Re-reads whether macOS allows banners; it changes in System Settings.
    pub(crate) fn load_banner_permission(&mut self, cx: &mut Context<Self>) {
        let Some(answer) = permission::denied() else { return };
        cx.spawn(async |this, cx| {
            let Ok(denied) = answer.await else { return };
            this.update(cx, |s, cx| {
                if s.banners_denied != denied {
                    s.banners_denied = denied;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn notification_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let groups: Vec<Div> =
            [self.alerts_group(cx), self.sound_group(cx), self.custom_sounds_group(cx), self.setting_group(Section::Notifications, "Dock & bell", cx)]
                .into_iter()
                .flatten()
                .collect();
        div().flex().flex_col().gap(px(22.)).when(self.banners_denied, |d| d.child(denied_banner())).children(groups)
    }

    /// The two master switches, then one row per event with a Banner and a Sound toggle, then the on-screen switch.
    fn alerts_group(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let (banners, sounds, on_screen) = (setting("banners")?, setting("sounds")?, setting("on-screen")?);
        let heading =
            div().px(px(14.)).pt(px(12.)).pb(px(2.)).text_size(px(12.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child("When an agent\u{2026}");
        let mut rows = vec![self.setting_row(banners, cx), self.setting_row(sounds, cx), heading];
        for event in EVENTS {
            rows.push(self.event_row(event, cx));
        }
        rows.push(self.setting_row(on_screen, cx));
        Some(group("Alerts", rows))
    }

    fn event_row(&mut self, event: Event, cx: &mut Context<Self>) -> Div {
        let (banner, sound) = (setting(event.banner).expect("event rows are in ROWS"), setting(event.sound).expect("event rows are in ROWS"));
        let [(banner_on, banner_free), (sound_on, sound_free)] = toggles(&self.store, event);
        let pills = div()
            .flex()
            .items_center()
            .gap(px(8.))
            .child(self.toggle_pill(banner, "bell", "Banner", banner_on, banner_free, cx))
            .child(self.toggle_pill(sound, "volume", "Sound", sound_on, sound_free, cx));
        let found = self.found.is_some_and(|f| f == event.banner || f == event.sound);
        if let Some(failed) = [banner, sound].into_iter().find(|s| self.saves.error(s.id).is_some()) {
            let hint = self.decor(failed, cx).hint;
            DECOR.with_borrow_mut(|d| *d = Some(Decor { hint, ..Decor::default() }));
        }
        row(banner.label, banner.hint, pills).relative().when(found, |d| d.bg(ACCENT_TINT))
    }

    /// A labelled toggle: filled while on, and taking no press or focus while its master switch is off.
    fn toggle_pill(
        &mut self,
        setting: &'static Setting,
        glyph: &'static str,
        label: &'static str,
        on: bool,
        free: bool,
        cx: &mut Context<Self>,
    ) -> Stateful<Div> {
        let focus = self.control_focus(setting.id, cx).tab_stop(free);
        let (face, ink) = if on { (TEXT, ON_TEXT) } else { (SURFACE, TEXT_3) };
        let pill = div()
            .id(setting.id)
            .track_focus(&focus)
            .h(px(26.))
            .px(px(10.))
            .flex()
            .flex_none()
            .items_center()
            .gap(px(5.))
            .rounded_full()
            .bg(face)
            .text_color(ink)
            .text_size(px(12.5))
            .font_weight(FontWeight::MEDIUM)
            .when(!on, |d| d.shadow(vec![ui::ring(SEPARATOR_STRONG, 0.5)]))
            .child(icon(glyph, 12., ink))
            .child(label);
        if !free {
            return pill.opacity(0.45);
        }
        focus_ring(pill.cursor_pointer().on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.set_switch(setting, !on, cx))))
    }
}

/// One event and the two rows that decide how it reaches you.
#[derive(Clone, Copy)]
struct Event {
    banner: &'static str,
    sound: &'static str,
}

/// By urgency, as the page says.
const EVENTS: [Event; 3] = [
    Event { banner: "banner-needs-you", sound: "sound-needs-you" },
    Event { banner: "banner-failed", sound: "sound-failed" },
    Event { banner: "banner-done", sound: "sound-done" },
];

fn setting(id: &str) -> Option<&'static Setting> {
    ROWS.iter().find(|r| r.id == id)
}

/// An event's Banner and Sound toggles as (on, free to press): each is free only while its master switch is on.
fn toggles(store: &Store, event: Event) -> [(bool, bool); 2] {
    let state = |id: &str| match setting(id).map(|s| s.control) {
        Some(Control::Switch { get, .. }) => get(store),
        _ => false,
    };
    [(state(event.banner), state("banners")), (state(event.sound), state("sounds"))]
}

fn denied_banner() -> Div {
    let open = action("settings-open-notifications", "Open System Settings").on_click(|_, _, cx| {
        if let Some(id) = permission::bundle_id() {
            cx.open_url(&permission::settings_url(&id));
        }
    });
    let text = div()
        .flex_1()
        .min_w_0()
        .flex()
        .flex_col()
        .gap(px(2.))
        .child(div().text_size(px(13.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child("Notifications are off for Anywhere in macOS"))
        .child(
            div()
                .text_size(px(12.))
                .line_height(px(16.))
                .text_color(TEXT_3)
                .child("Banners below won\u{2019}t appear until you allow them. Sounds and the Dock badge still work."),
        );
    div().px(px(14.)).py(px(12.)).flex().items_center().gap(px(12.)).rounded(px(12.)).bg(WAITING_BG).child(icon("bell", 16., WAITING)).child(text).child(open)
}

#[cfg(test)]
mod tests {
    use super::{EVENTS, ROWS, setting, toggles};
    use store::Store;

    #[test]
    fn each_event_has_a_banner_and_a_sound_row() {
        for e in EVENTS {
            assert!(setting(e.banner).is_some() && setting(e.sound).is_some());
        }
        assert_eq!(EVENTS.map(|e| setting(e.banner).unwrap().label), ["Needs you", "Failed", "Done"]);
        assert!(ROWS.iter().any(|r| r.id == "banners") && ROWS.iter().any(|r| r.id == "sounds"));
    }

    #[test]
    fn an_event_s_toggles_are_free_only_while_their_master_switch_is_on() {
        let mut store = Store::default();
        assert_eq!(toggles(&store, EVENTS[2]), [(true, true), (true, true)]);
        store.notifications.banners = false;
        store.sounds.done = false;
        assert_eq!(toggles(&store, EVENTS[2]), [(true, false), (false, true)]);
        store.sounds.all = false;
        assert_eq!(toggles(&store, EVENTS[0]), [(true, false), (true, false)]);
    }
}
