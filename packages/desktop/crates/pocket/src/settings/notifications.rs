mod permission;

use super::catalog::{Look, Setting};
use super::{Section, action};
use crate::desktop::Desktop;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use store::prefs::terminal::Bell;
use store::{DockBadge, Store, Tone};
use theme::*;

/// "Off", then every tone a cue can play.
const TONES: &[&str] = &["Off", "Anywhere", "Basso", "Blow", "Bottle", "Frog", "Funk", "Glass", "Hero", "Morse", "Ping", "Pop", "Purr", "Sosumi", "Submarine", "Tink"];
const BADGES: [DockBadge; 3] = [DockBadge::Off, DockBadge::NeedsYou, DockBadge::Inbox];

/// A cue's dropdown index: 0 when it is off, else its tone's place after "Off".
fn tone_index(on: bool, tone: Tone) -> usize {
    if on { Tone::ALL.iter().position(|t| *t == tone).map_or(1, |i| i + 1) } else { 0 }
}

/// Turns the cue off at 0, else on in the chosen tone; an off cue keeps its tone.
fn pick_tone(on: &mut bool, tone: &mut Tone, i: usize) {
    *on = i > 0;
    if let Some(t) = i.checked_sub(1).and_then(|i| Tone::ALL.get(i)) {
        *tone = *t;
    }
}

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
    Setting::choice("sound-needs-you", "Sounds", "Needs you", TONES, Look::Dropdown, |s| tone_index(s.sounds.needs_you, s.sounds.needs_you_tone), |s: &mut Store, i| {
        pick_tone(&mut s.sounds.needs_you, &mut s.sounds.needs_you_tone, i)
    })
    .under(|s| s.sounds.all),
    Setting::choice("sound-failed", "Sounds", "Failed", TONES, Look::Dropdown, |s| tone_index(s.sounds.failed, s.sounds.failed_tone), |s: &mut Store, i| {
        pick_tone(&mut s.sounds.failed, &mut s.sounds.failed_tone, i)
    })
    .under(|s| s.sounds.all),
    Setting::choice("sound-done", "Sounds", "Done", TONES, Look::Dropdown, |s| tone_index(s.sounds.done, s.sounds.done_tone), |s: &mut Store, i| pick_tone(&mut s.sounds.done, &mut s.sounds.done_tone, i))
        .under(|s| s.sounds.all),
    Setting::stepper("volume", "Sounds", "Volume", (10, 100, 10), "%", |s| s.sounds.volume as i32, |s, n| s.sounds.volume = n as u32).under(|s| s.sounds.all),
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
        let groups = self.setting_groups(Section::Notifications, cx);
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

#[cfg(test)]
mod tests {
    use super::{TONES, pick_tone, tone_index};
    use store::Tone;

    #[test]
    fn the_dropdown_names_every_tone_in_order() {
        assert_eq!(TONES[1..].to_vec(), Tone::ALL.map(|t| format!("{t:?}")));
    }

    #[test]
    fn a_cue_reads_off_or_its_tone_after_off() {
        assert_eq!([tone_index(false, Tone::Glass), tone_index(true, Tone::Anywhere), tone_index(true, Tone::Glass)], [0, 1, 7]);
    }

    #[test]
    fn turning_a_cue_off_keeps_its_tone_for_when_it_comes_back() {
        let (mut on, mut tone) = (true, Tone::Glass);
        pick_tone(&mut on, &mut tone, 0);
        assert_eq!((on, tone), (false, Tone::Glass));
        pick_tone(&mut on, &mut tone, 2);
        assert_eq!((on, tone), (true, Tone::Basso));
    }
}
