use super::{ROWS, SOUND};
use crate::desktop::Desktop;
use crate::desktop::sounds::{BUILT_IN, play, sound_name};
use crate::settings::card;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::{Path, PathBuf};
use store::{Sound, Sounds, Store};
use theme::*;

/// A row of the sound list.
struct Choice {
    sound: Sound,
    emoji: &'static str,
    name: String,
    length: Option<&'static str>,
    hint: &'static str,
}

/// The app's sounds, then the user's.
fn choices(sounds: &Sounds) -> Vec<Choice> {
    let built = BUILT_IN.iter().map(|b| Choice { sound: b.sound.clone(), emoji: b.emoji, name: b.name.into(), length: Some(b.length), hint: b.hint });
    let custom = sounds.custom.iter().map(|p| {
        let sound = Sound::Custom(p.clone());
        Choice { name: sound_name(&sound), sound, emoji: "\u{1f3b5}", length: None, hint: "Custom audio" }
    });
    built.chain(custom).collect()
}

/// Where added sounds are copied.
fn sounds_dir(store: &Store) -> PathBuf {
    store.home().join("sounds")
}

/// Whether `afplay` reads it, going by its extension.
fn playable(path: &Path) -> bool {
    path.extension().and_then(|e| e.to_str()).is_some_and(|e| ["mp3", "wav", "ogg"].contains(&e.to_lowercase().as_str()))
}

/// `from`'s name in `dir`, numbered past any other file already there.
fn free_name(dir: &Path, from: &Path) -> PathBuf {
    let stem = from.file_stem().unwrap_or_default().to_string_lossy();
    let ext = from.extension().map(|e| format!(".{}", e.to_string_lossy())).unwrap_or_default();
    let mut to = dir.join(from.file_name().unwrap_or_default());
    let mut n = 1;
    while to != from && to.exists() {
        n += 1;
        to = dir.join(format!("{stem} {n}{ext}"));
    }
    to
}

/// Copies each file into `dir`, returning the copies' paths or the names of files that failed.
fn copy_into(dir: &Path, files: Vec<PathBuf>) -> Vec<Result<String, String>> {
    std::fs::create_dir_all(dir).ok();
    files
        .into_iter()
        .map(|f| {
            let to = free_name(dir, &f);
            // Copying a file onto itself empties it.
            if f != to {
                std::fs::copy(&f, &to).map_err(|_| f.file_name().unwrap_or_default().to_string_lossy().into_owned())?;
            }
            Ok(to.to_string_lossy().into_owned())
        })
        .collect()
}

/// What to tell the user about files that weren't added.
fn not_added(skipped: &[String], failed: &[String]) -> Option<String> {
    let skipped = (!skipped.is_empty()).then(|| format!("{} isn\u{2019}t .mp3, .wav or .ogg", skipped.join(", ")));
    let failed = (!failed.is_empty()).then(|| format!("Couldn\u{2019}t add {}", failed.join(", ")));
    let parts: Vec<String> = skipped.into_iter().chain(failed).collect();
    (!parts.is_empty()).then(|| parts.join(". "))
}

impl Desktop {
    /// The sound list under a header holding its hint and the add button, as the group title can't.
    pub(super) fn sound_group(&mut self, cx: &mut Context<Self>) -> Option<Div> {
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

    pub(super) fn sound_list(&mut self, cx: &mut Context<Self>) -> Div {
        let rows: Vec<_> = choices(&self.store.sounds).into_iter().enumerate().map(|(i, c)| self.sound_row(i, c, cx)).collect();
        div().flex().flex_col().children(rows)
    }

    fn sound_row(&mut self, i: usize, choice: Choice, cx: &mut Context<Self>) -> Stateful<Div> {
        let Choice { sound, emoji, name, length, hint } = choice;
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
        let tile = div().size(px(32.)).flex_none().flex().items_center().justify_center().rounded(px(8.)).bg(FILL_1).text_size(px(18.)).child(emoji);
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
            .child(tile)
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
            let skipped: Vec<String> = other.iter().filter_map(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).collect();
            let dir = sounds_dir(&d.store);
            cx.spawn(async move |this, cx| {
                let copied = cx.background_executor().spawn(async move { copy_into(&dir, audio) }).await;
                this.update(cx, |d, cx| {
                    let mut failed = vec![];
                    for c in copied {
                        match c {
                            Ok(path) => d.store.sounds.add(path),
                            Err(name) => failed.push(name),
                        }
                    }
                    if let Some(error) = not_added(&skipped, &failed) {
                        d.error = Some(error);
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
        if Path::new(path).starts_with(sounds_dir(&self.store)) {
            let path = path.to_string();
            cx.background_executor().spawn(async move { std::fs::remove_file(path).ok() }).detach();
        }
        self.save_soon(cx);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{choices, copy_into, not_added, playable};
    use std::path::{Path, PathBuf};
    use store::Sounds;

    fn temp_dir(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("pocket-sound-{name}-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn only_mp3_wav_and_ogg_are_playable() {
        assert_eq!(["a.mp3", "b.WAV", "c.ogg", "d.flac", "e"].map(|p| playable(Path::new(p))), [true, true, true, false, false]);
    }

    #[test]
    fn added_sounds_follow_the_built_ins_by_file_name() {
        let sounds = Sounds { custom: vec!["/s/Chime.mp3".into()], ..Sounds::default() };
        let names: Vec<String> = choices(&sounds).into_iter().map(|c| c.name).collect();
        assert_eq!(names, ["Chime", "Ping", "Gasp", "High Ding", "Shimmer", "Ding Dong", "Chime.mp3"]);
    }

    #[test]
    fn adding_a_sound_already_in_the_folder_keeps_it_whole() {
        let dir = temp_dir("copy");
        let file = dir.join("a.mp3");
        std::fs::write(&file, b"abc").unwrap();
        assert_eq!(copy_into(&dir, vec![file.clone()]), [Ok(file.to_string_lossy().into_owned())]);
        assert_eq!(std::fs::read(&file).unwrap(), b"abc");
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn adding_a_second_file_of_the_same_name_keeps_both() {
        let (dir, from) = (temp_dir("same-name"), temp_dir("same-name-from"));
        std::fs::write(dir.join("a.mp3"), b"old").unwrap();
        std::fs::write(from.join("a.mp3"), b"new").unwrap();
        assert_eq!(copy_into(&dir, vec![from.join("a.mp3")]), [Ok(dir.join("a 2.mp3").to_string_lossy().into_owned())]);
        assert_eq!([std::fs::read(dir.join("a.mp3")).unwrap(), std::fs::read(dir.join("a 2.mp3")).unwrap()], [b"old", b"new"]);
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&from).unwrap();
    }

    #[test]
    fn every_file_not_added_is_named() {
        let s = |v: &[&str]| v.iter().map(|n| n.to_string()).collect::<Vec<_>>();
        assert_eq!(not_added(&[], &[]), None);
        assert_eq!(not_added(&s(&["a.flac", "b.txt"]), &s(&["c.mp3"])).as_deref(), Some("a.flac, b.txt isn\u{2019}t .mp3, .wav or .ogg. Couldn\u{2019}t add c.mp3"));
    }
}
