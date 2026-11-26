use super::setting;
use crate::desktop::Desktop;
use crate::desktop::sounds::{BUILT_IN, play, sound_name};
use crate::explorer::toast::unrestored;
use crate::settings::catalog::Setting;
use crate::settings::dropdown::{Choice as Pick, Row, dropdown, picks};
use crate::settings::toasts::{Toast, Undo};
use crate::settings::{Settings, SettingsEvent, action, deferred::with_window, group, row};
use gpui_kit::*;
use std::path::{Path, PathBuf};
use store::{Sound, Sounds, Store};
use theme::*;

/// The menu's last row: it opens a file picker instead of choosing a sound.
const ADD: &str = "add";

/// A sound the menu lists.
struct Choice {
    sound: Sound,
    name: String,
}

/// The app's sounds, then the user's.
fn choices(sounds: &Sounds) -> Vec<Choice> {
    let built = BUILT_IN.iter().map(|b| Choice { sound: b.sound.clone(), name: b.name.into() });
    let custom = sounds.custom.iter().map(|p| {
        let sound = Sound::Custom(p.clone());
        Choice { name: sound_name(&sound), sound }
    });
    built.chain(custom).collect()
}

/// The sound menu: each sound by its place in `choices`, then the way to add one.
fn menu(sounds: &Sounds) -> Vec<Row> {
    let list = choices(sounds);
    let built = BUILT_IN.len();
    let mut rows = picks(list.iter().take(built).enumerate().map(|(i, c)| (i.to_string(), c.name.clone())).collect());
    if list.len() > built {
        rows.push(Row::Heading("Custom".into()));
        rows.extend(picks(list.iter().enumerate().skip(built).map(|(i, c)| (i.to_string(), c.name.clone())).collect()));
    }
    rows.push(Row::Pick(Pick { value: ADD.into(), label: "Add custom audio\u{2026}".into() }));
    rows
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

impl Settings {
    /// The sound's dropdown with a preview beside it; custom audio is added from its menu.
    pub(super) fn sound_row(&mut self, setting: &'static Setting, cx: &mut Context<Self>) -> Div {
        let sounds = &self.store.sounds;
        let list = choices(sounds);
        let current = list.iter().position(|c| c.sound == sounds.sound).unwrap_or(0);
        let label = list.get(current).map(|c| c.name.clone()).unwrap_or_default();
        let trigger = action(setting.id, label).gap(px(6.)).child(icon("chevron-down", 12., TEXT_3));
        let snapshot = sounds.clone();
        let menu = dropdown(
            setting.id,
            trigger,
            &sounds.custom,
            move || menu(&snapshot),
            &current.to_string(),
            self,
            move |this, value, cx| {
                if value == ADD {
                    return with_window(cx, |s, window, cx| s.add_sounds(window, cx));
                }
                let picked = value.parse::<usize>().ok().and_then(|i| choices(&this.store.sounds).into_iter().nth(i)).map(|c| c.sound);
                if let Some(sound) = picked {
                    this.set(setting, cx, move |s| s.sounds.sound = sound.clone());
                }
            },
            cx,
        );
        let preview = self.store.sounds.sound.clone();
        let listen = preview_button("sound-play", 0, preview, cx);
        row(setting.label, setting.hint, div().flex().items_center().gap(px(8.)).child(menu).child(listen))
    }

    /// The sound and the volume.
    pub(super) fn sound_group(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let shown: Vec<&'static Setting> = ["notification-sound", "volume"].into_iter().filter_map(setting).filter(|s| s.allowed(&self.store)).collect();
        let rows: Vec<Div> = shown.into_iter().map(|s| self.setting_row(s, cx)).collect();
        (!rows.is_empty()).then(|| group("Sound & volume", rows))
    }

    /// The audio the user added, each with a preview and a way to delete it; nothing while there is none.
    pub(super) fn custom_sounds_group(&mut self, cx: &mut Context<Self>) -> Option<Div> {
        let sound = setting("notification-sound")?;
        if !sound.allowed(&self.store) || self.store.sounds.custom.is_empty() {
            return None;
        }
        let rows: Vec<Div> = self.store.sounds.custom.clone().into_iter().enumerate().map(|(i, path)| self.custom_row(i, path, cx)).collect();
        Some(group("Custom audio", rows))
    }

    fn custom_row(&mut self, i: usize, path: String, cx: &mut Context<Self>) -> Div {
        let name = sound_name(&Sound::Custom(path.clone()));
        let remove = ui::icon_button_sized(("sound-remove", i), "trash", 24., TEXT_3)
            .tooltip(|window, cx| gpui_kit::component::tooltip::Tooltip::new("Move to the Trash").build(window, cx));
        let remove = remove.on_click(cx.listener({
            let path = path.clone();
            move |this, _: &ClickEvent, _, cx| this.remove_sound(&path, cx)
        }));
        let controls = div().flex().items_center().gap(px(8.)).child(preview_button("custom-sound-play", i, Sound::Custom(path), cx)).child(remove);
        row(name, None, controls)
    }

    /// Copies the picked audio files into Pocket's folder, so moving the originals keeps them playing.
    fn add_sounds(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.pick_paths(false, window, cx, |s, paths, cx| {
            let (audio, other): (Vec<PathBuf>, Vec<PathBuf>) = paths.into_iter().partition(|p| playable(p));
            let skipped: Vec<String> = other.iter().filter_map(|p| p.file_name()).map(|n| n.to_string_lossy().into_owned()).collect();
            let dir = sounds_dir(&s.store);
            cx.spawn(async move |this, cx| {
                let copied = cx.background_executor().spawn(async move { copy_into(&dir, audio) }).await;
                this.update(cx, |s, cx| {
                    let (mut added, mut failed) = (vec![], vec![]);
                    for c in copied {
                        match c {
                            Ok(path) => added.push(path),
                            Err(name) => failed.push(name),
                        }
                    }
                    if let Some(error) = not_added(&skipped, &failed) {
                        cx.emit(SettingsEvent::Error(error));
                    }
                    s.write(setting("notification-sound"), cx, move |s, _| added.iter().for_each(|p| s.sounds.add(p.clone())));
                })
                .ok();
            })
            .detach();
        });
    }

    /// Moves the audio file to the Trash and forgets it, with a toast whose Undo brings both back.
    fn remove_sound(&mut self, path: &str, cx: &mut Context<Self>) {
        let sounds = &self.store.sounds;
        let held = Held { at: sounds.custom.iter().position(|p| p == path).unwrap_or(0), picked: sounds.sound == Sound::Custom(path.into()) };
        let (path, dir) = (path.to_string(), sounds_dir(&self.store));
        cx.spawn(async move |this, cx| {
            let file = path.clone();
            let moved = cx.background_executor().spawn(async move { trash_file(&dir, &file) }).await;
            this.update(cx, |s, cx| {
                let name = sound_name(&Sound::Custom(path.clone()));
                let Ok(landed) = moved else { return cx.emit(SettingsEvent::Error(format!("Couldn\u{2019}t move {name} to the Trash"))) };
                let gone = path.clone();
                s.write(None, cx, move |s, _| s.sounds.remove(&gone));
                let undo = Undo::new(move |d, _, cx| d.restore_sound(path.clone(), landed.clone(), held, cx));
                s.toast(Toast::undo(format!("Moved {name} to the Trash"), undo), cx);
            })
            .ok();
        })
        .detach();
    }
}

/// Where a removed sound stood, to put it back there.
#[derive(Clone, Copy, Debug, PartialEq)]
struct Held {
    at: usize,
    picked: bool,
}

/// Lists `path` again where it was, and picks it again if it was picked and nothing else has been picked since.
fn bring_back(sounds: &mut Sounds, path: &str, held: Held) {
    if !sounds.custom.iter().any(|p| p == path) {
        let at = held.at.min(sounds.custom.len());
        sounds.custom.insert(at, path.into());
    }
    if held.picked && sounds.sound == Sound::default() {
        sounds.sound = Sound::Custom(path.into());
    }
}

/// Moves the file at `path` to the Trash if it is one the app copied into `dir`: where it landed, `None` when it isn't ours
/// or is already gone, `Err` when the Trash refused it.
fn trash_file(dir: &Path, path: &str) -> Result<Option<PathBuf>, ()> {
    let path = Path::new(path);
    if !path.starts_with(dir) {
        return Ok(None);
    }
    crate::util::trash_for_undo(path)
}

impl Desktop {
    /// Brings a sound back from the Trash, unless something took its file's name meanwhile.
    fn restore_sound(&mut self, path: String, trashed: Option<PathBuf>, held: Held, cx: &mut Context<Self>) {
        cx.spawn(async move |this, cx| {
            let from = PathBuf::from(&path);
            let restored = match trashed {
                Some(to) => cx.background_spawn(async move { crate::explorer::files::restore(&to, &from) }).await,
                None => Ok(()),
            };
            this.update(cx, |d, cx| match restored {
                Ok(()) => {
                    bring_back(&mut d.store.sounds, &path, held);
                    d.changed(None, cx);
                }
                Err(e) => d.settings_toast(Toast::failed(unrestored(&e, &sound_name(&Sound::Custom(path.clone()))), None), cx),
            })
            .ok();
        })
        .detach();
    }
}

/// A round ▶ that plays `sound` at the saved volume.
fn preview_button(id: &'static str, i: usize, sound: Sound, cx: &mut Context<Settings>) -> Stateful<Div> {
    let button = div()
        .id((id, i))
        .size(px(28.))
        .flex()
        .flex_none()
        .items_center()
        .justify_center()
        .rounded_full()
        .bg(SURFACE)
        .shadow(vec![ui::ring(SEPARATOR_STRONG, 1.)])
        .cursor_pointer()
        .tab_index(0)
        .hover(|d| d.bg(FILL_3))
        .child(icon("play", 12., TEXT))
        .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| play(&sound, this.store.sounds.volume, cx)));
    crate::settings::focus_ring(button)
}

#[cfg(test)]
mod tests {
    use super::{Held, Row, bring_back, choices, copy_into, menu, not_added, playable, trash_file};
    use std::path::{Path, PathBuf};
    use store::{Sound, Sounds};

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
        assert_eq!(
            not_added(&s(&["a.flac", "b.txt"]), &s(&["c.mp3"])).as_deref(),
            Some("a.flac, b.txt isn\u{2019}t .mp3, .wav or .ogg. Couldn\u{2019}t add c.mp3")
        );
    }

    fn labels(rows: &[Row]) -> Vec<String> {
        rows.iter()
            .map(|r| match r {
                Row::Heading(h) => format!("# {h}"),
                Row::Pick(c) => format!("{} = {}", c.label, c.value),
            })
            .collect()
    }

    #[test]
    fn the_menu_ends_with_the_way_to_add_audio_and_heads_the_custom_ones() {
        let none = labels(&menu(&Sounds::default()));
        assert_eq!(
            (none[0].as_str(), none[none.len() - 2].as_str(), none[none.len() - 1].as_str()),
            ("Chime = 0", "Ding Dong = 5", "Add custom audio\u{2026} = add")
        );
        assert!(!none.iter().any(|l| l.starts_with('#')));
        let sounds = Sounds { custom: vec!["/s/Gong.mp3".into()], ..Sounds::default() };
        assert_eq!(labels(&menu(&sounds))[6..], ["# Custom", "Gong.mp3 = 6", "Add custom audio\u{2026} = add"]);
    }

    #[test]
    fn undoing_a_removal_lists_the_sound_where_it_stood_and_picks_it_if_it_was_picked() {
        let mut sounds = Sounds { custom: vec!["/s/a.mp3".into(), "/s/c.mp3".into()], ..Sounds::default() };
        bring_back(&mut sounds, "/s/b.mp3", Held { at: 1, picked: true });
        assert_eq!(
            (sounds.custom.clone(), sounds.sound.clone()),
            (vec!["/s/a.mp3".into(), "/s/b.mp3".into(), "/s/c.mp3".into()], Sound::Custom("/s/b.mp3".into()))
        );
        bring_back(&mut sounds, "/s/b.mp3", Held { at: 0, picked: false });
        assert_eq!(sounds.custom.len(), 3, "already listed");
    }

    #[test]
    fn undoing_a_removal_leaves_a_sound_picked_since() {
        let mut sounds = Sounds { sound: Sound::Ping, ..Sounds::default() };
        bring_back(&mut sounds, "/s/a.mp3", Held { at: 9, picked: true });
        assert_eq!((sounds.custom, sounds.sound), (vec!["/s/a.mp3".to_string()], Sound::Ping));
    }

    #[test]
    fn only_a_file_the_app_copied_goes_to_the_trash() {
        let (dir, elsewhere) = (temp_dir("trash-own"), temp_dir("trash-other"));
        let theirs = elsewhere.join("a.mp3");
        std::fs::write(&theirs, b"a").unwrap();
        assert_eq!(trash_file(&dir, &theirs.to_string_lossy()), Ok(None));
        assert!(theirs.exists(), "a file outside the folder is left alone");
        assert_eq!(trash_file(&dir, &dir.join("gone.mp3").to_string_lossy()), Ok(None));
        std::fs::remove_dir_all(&dir).unwrap();
        std::fs::remove_dir_all(&elsewhere).unwrap();
    }
}
