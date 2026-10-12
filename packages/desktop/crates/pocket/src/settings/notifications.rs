mod permission;
mod sounds;

use super::catalog::{self, Look, Setting};
use super::{Section, action};
use crate::desktop::Desktop;
use crate::desktop::sounds::sound_name;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::prefs::terminal::Bell;
use store::DockBadge;
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
    Setting::switch("banners", "Banners", "Banners", |s| s.notifications.banners, |s, on| s.notifications.banners = on).hint("A macOS banner when a session needs you, fails or is done"),
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
    Setting::custom("notification-sound", SOUND, SOUND, |d, _, cx| d.sound_list(cx))
        .hint("Pick a sound or add your own. Custom audio supports .mp3, .wav, and .ogg.")
        .under(|s| s.sounds.all)
        .resets(|s| sound_name(&s.sounds.sound), |s, d| s.sounds.sound = d.sounds.sound.clone()),
    Setting::choice("dock-badge", "Dock & bell", "Dock badge", &["Off", "Needs you count", "Inbox count"], Look::Dropdown, |s| BADGES.iter().position(|b| *b == s.notifications.badge).unwrap_or(2), |s, i| {
        s.notifications.badge = BADGES[i]
    })
    .hint("The number on the app icon")
    .effect(show_badge),
    Setting::choice("terminal-bell", "Dock & bell", "Terminal bell", &["Off", "Sound", "Flash"], Look::Segmented, |s| s.terminal.bell as usize, |s, i| s.terminal.bell = Bell::ALL[i])
        .hint("When a program in a terminal rings the bell"),
];

impl Desktop {
    /// Re-reads whether macOS allows banners; it changes in System Settings.
    pub(crate) fn load_banner_permission(&mut self, cx: &mut Context<Self>) {
        let Some(answer) = permission::denied() else { return };
        cx.spawn(async |this, cx| {
            let Ok(denied) = answer.await else { return };
            this.update(cx, |d, cx| {
                if d.settings.banners_denied != denied {
                    d.settings.banners_denied = denied;
                    cx.notify();
                }
            })
            .ok();
        })
        .detach();
    }

    pub(super) fn notification_settings(&mut self, cx: &mut Context<Self>) -> Div {
        let groups: Vec<Div> = catalog::groups(ROWS).into_iter().filter_map(|g| if g == SOUND { self.sound_group(cx) } else { self.setting_group(Section::Notifications, g, cx) }).collect();
        div().flex().flex_col().gap(px(22.)).when(self.settings.banners_denied, |d| d.child(denied_banner())).children(groups)
    }
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
        .child(div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child("Banners below won\u{2019}t appear until you allow them. Sounds and the Dock badge still work."));
    div()
        .px(px(14.))
        .py(px(12.))
        .flex()
        .items_center()
        .gap(px(12.))
        .rounded(px(12.))
        .bg(WAITING_BG)
        .child(icon("bell", 16., WAITING))
        .child(text)
        .child(open)
}
