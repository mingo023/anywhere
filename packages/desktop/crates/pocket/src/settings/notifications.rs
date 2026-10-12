mod permission;

use super::catalog::{self, Look, Setting};
use super::{Section, action, card};
use crate::desktop::Desktop;
use crate::desktop::sounds::{BUILT_IN, file_name, play, sound_name};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use store::prefs::terminal::Bell;
use store::{DockBadge, Sound};
use theme::*;

const BADGES: [DockBadge; 3] = [DockBadge::Off, DockBadge::NeedsYou, DockBadge::Inbox];
const SOUND: &str = "Notification sound";

/// Whether `afplay` reads it, going by its extension.
fn playable(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["mp3", "wav", "ogg"].contains(&e.to_lowercase().as_str()))
}

/// Copies each file into `dir`, returning the copies' paths or the names of files that failed.
fn copy_into(dir: &Path, files: Vec<PathBuf>) -> Vec<Result<String, String>> {
    std::fs::create_dir_all(dir).ok();
    files
        .into_iter()
        .map(|f| {
            let name = f.file_name().unwrap_or_default();
            let to = dir.join(name);
            // Copying a file onto itself empties it.
            if f != to {
                std::fs::copy(&f, &to).map_err(|_| name.to_string_lossy().into_owned())?;
            }
            Ok(to.to_string_lossy().into_owned())
        })
        .collect()
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

    /// The sound list under a header holding its hint and the add button, as the group title can't.
    fn sound_group(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let setting = ROWS.iter().find(|r| r.group == SOUND)?;
        if !setting.allowed(&self.store) {
            return None;
        }
        let add = ui::button("sound-add", ui::Variant::Secondary, Some("plus"), "Add custom audio").h(px(28.)).on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.add_sounds(window, cx)));
        let text = div()
            .flex_1()
            .min_w_0()
            .flex()
            .flex_col()
            .gap(px(1.))
            .child(div().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::SEMIBOLD).text_color(TEXT_2).child(SOUND))
            .children(setting.hint.map(|h| div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(h)));
        let head = div().mb(px(7.)).ml(px(2.)).flex().items_end().gap(px(12.)).child(text).child(add);
        Some(div().flex().flex_col().child(head).child(card(vec![self.setting_row(setting, cx)])))
    }

    fn sound_list(&mut self, cx: &mut Context<Self>) -> Div {
        let built = BUILT_IN.iter().map(|b| (b.sound.clone(), b.emoji, b.name.to_string(), Some(b.length), b.hint));
        let custom = self.store.sounds.custom.iter().map(|p| (Sound::Custom(p.clone()), "\u{1f3b5}", file_name(p), None, "Custom audio"));
        let all: Vec<_> = built.chain(custom).collect();
        let rows: Vec<_> = all.into_iter().enumerate().map(|(i, (sound, emoji, name, length, hint))| self.sound_row(i, sound, emoji, name, length, hint, cx)).collect();
        div().flex().flex_col().children(rows)
    }

    #[allow(clippy::too_many_arguments)]
    fn sound_row(&mut self, i: usize, sound: Sound, emoji: &'static str, name: String, length: Option<&'static str>, hint: &'static str, cx: &mut Context<Self>) -> Stateful<Div> {
        let picked = self.store.sounds.sound == sound;
        let title = div()
            .flex()
            .items_baseline()
            .gap(px(6.))
            .child(div().truncate().text_size(px(13.)).line_height(px(18.)).font_weight(FontWeight::MEDIUM).text_color(TEXT).child(name))
            .children(length.map(|l| div().flex_none().text_size(px(12.)).text_color(TEXT_4).child(l)));
        let text = div().flex_1().min_w_0().flex().flex_col().gap(px(1.)).child(title).child(div().text_size(px(12.)).line_height(px(16.)).text_color(TEXT_3).child(hint));
        let remove = match &sound {
            Sound::Custom(path) => {
                let path = path.clone();
                Some(ui::icon_button_sized(("sound-remove", i), "trash", 24., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    cx.stop_propagation();
                    this.remove_sound(&path, cx);
                })))
            }
            _ => None,
        };
        let preview = sound.clone();
        let play_button = div()
            .id(("sound-play", i))
            .size(px(28.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded_full()
            .bg(SURFACE)
            .shadow(vec![ui::ring(SEPARATOR_STRONG, 1.)])
            .cursor_pointer()
            .hover(|d| d.bg(FILL_3))
            .child(icon("play", 12., TEXT))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                play(&preview, this.store.sounds.volume, cx);
            }));
        div()
            .id(("sound", i))
            .px(px(14.))
            .py(px(10.))
            .flex()
            .items_center()
            .gap(px(12.))
            .cursor_pointer()
            .when(i > 0, |r| r.border_t(px(0.5)).border_color(SEPARATOR))
            .map(|r| if picked { r.bg(FILL_2) } else { r.hover(|d| d.bg(FILL_1)) })
            .child(div().w(px(24.)).flex_none().flex().justify_center().text_size(px(18.)).child(emoji))
            .child(text)
            .when(picked, |r| r.child(icon("check", 14., ACCENT_FILL)))
            .children(remove)
            .child(play_button)
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.store.sounds.sound = sound.clone();
                this.save_soon(cx);
                cx.notify();
            }))
    }

    /// Copies the picked audio files into Pocket's folder, so moving the originals keeps them playing.
    fn add_sounds(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_path(false, window, cx, |d, paths, _, cx| {
            let (audio, other): (Vec<PathBuf>, Vec<PathBuf>) = paths.into_iter().partition(|p| playable(p));
            if let Some(name) = other.first().and_then(|p| p.file_name()) {
                d.error = Some(format!("{} isn\u{2019}t .mp3, .wav or .ogg", name.to_string_lossy()));
            }
            let dir = d.store.home().join("sounds");
            cx.spawn(async move |this, cx| {
                let copied = cx.background_executor().spawn(async move { copy_into(&dir, audio) }).await;
                this.update(cx, |d, cx| {
                    for c in copied {
                        match c {
                            Ok(path) => d.store.sounds.add(path),
                            Err(name) => d.error = Some(format!("Couldn\u{2019}t add {name}")),
                        }
                    }
                    d.save_soon(cx);
                    cx.notify();
                })
                .ok();
            })
            .detach();
        });
    }

    fn remove_sound(&mut self, path: &str, cx: &mut Context<Self>) {
        self.store.sounds.remove(path);
        if Path::new(path).starts_with(self.store.home().join("sounds")) {
            let path = path.to_string();
            cx.background_executor().spawn(async move { std::fs::remove_file(path).ok() }).detach();
        }
        self.save_soon(cx);
        cx.notify();
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
    use super::{copy_into, playable};
    use std::path::Path;

    #[test]
    fn only_mp3_wav_and_ogg_are_playable() {
        assert_eq!(["a.mp3", "b.WAV", "c.ogg", "d.flac", "e"].map(|p| playable(Path::new(p))), [true, true, true, false, false]);
    }

    #[test]
    fn adding_a_sound_already_in_the_folder_keeps_it_whole() {
        let dir = std::env::temp_dir().join(format!("pocket-sound-copy-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("a.mp3");
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(copy_into(&dir, vec![file.clone()]), [Ok(file.to_string_lossy().into_owned())]);
        assert_eq!(std::fs::read(&file).unwrap(), b"abc");
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
