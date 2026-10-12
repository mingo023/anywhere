use crate::desktop::Desktop;
use crate::status::Status;
use crate::util::now_ms;
use agents::Summary;
use gpui_kit::*;
use std::collections::HashMap;
use std::time::Duration;
use std::path::PathBuf;
use store::{Sound, Sounds};

/// A sound, ordered by priority: when several land in one window, the first wins.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub enum Cue {
    Failed,
    NeedsYou,
    Done,
}

pub const COALESCE_MS: u64 = 250;
/// Older Done turns are news the user already missed; they stay silent.
pub const FRESH_MS: i64 = 45_000;

impl Cue {
    pub const ALL: [Cue; 3] = [Cue::NeedsYou, Cue::Done, Cue::Failed];

    fn of(status: Status) -> Option<Cue> {
        match status {
            Status::NeedsYou => Some(Cue::NeedsYou),
            Status::Done => Some(Cue::Done),
            Status::Failed => Some(Cue::Failed),
            Status::Working | Status::Idle => None,
        }
    }

    pub fn on(self, s: &Sounds) -> bool {
        match self {
            Cue::NeedsYou => s.needs_you,
            Cue::Done => s.done,
            Cue::Failed => s.failed,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            Cue::NeedsYou => "Needs you",
            Cue::Done => "Done",
            Cue::Failed => "Failed",
        }
    }

    pub fn flip(self, s: &mut Sounds) {
        let on = match self {
            Cue::NeedsYou => &mut s.needs_you,
            Cue::Done => &mut s.done,
            Cue::Failed => &mut s.failed,
        };
        *on = !*on;
    }
}

/// A sound the app ships.
pub struct Builtin {
    pub sound: Sound,
    pub name: &'static str,
    pub hint: &'static str,
    pub emoji: &'static str,
    pub length: &'static str,
    file: &'static str,
    bytes: &'static [u8],
}

pub const BUILT_IN: &[Builtin] = &[
    Builtin { sound: Sound::Chime, name: "Chime", hint: "Light notification", emoji: "\u{1f390}", length: "0.8s", file: "chime.mp3", bytes: include_bytes!("../../assets/sounds/chime.mp3") },
    Builtin { sound: Sound::Ping, name: "Ping", hint: "Short alert tone", emoji: "\u{1f4cd}", length: "0.7s", file: "ping.mp3", bytes: include_bytes!("../../assets/sounds/ping.mp3") },
    Builtin { sound: Sound::Gasp, name: "Gasp", hint: "Bright UI alert", emoji: "\u{1f4ab}", length: "1.1s", file: "gasp.mp3", bytes: include_bytes!("../../assets/sounds/gasp.mp3") },
    Builtin { sound: Sound::HighDing, name: "High Ding", hint: "Soft bell", emoji: "\u{1f514}", length: "0.8s", file: "high-ding.mp3", bytes: include_bytes!("../../assets/sounds/high-ding.mp3") },
    Builtin { sound: Sound::Shimmer, name: "Shimmer", hint: "Quick sparkle", emoji: "\u{2728}", length: "0.3s", file: "shimmer.mp3", bytes: include_bytes!("../../assets/sounds/shimmer.mp3") },
    Builtin { sound: Sound::DingDong, name: "Ding Dong", hint: "Soft two-note bell", emoji: "\u{1f6ce}\u{fe0f}", length: "0.9s", file: "ding-dong.mp3", bytes: include_bytes!("../../assets/sounds/ding-dong.mp3") },
];

/// What the settings call `sound`: a built-in's name, else the file's.
pub fn sound_name(sound: &Sound) -> String {
    match sound {
        Sound::Custom(path) => file_name(path),
        s => BUILT_IN.iter().find(|b| b.sound == *s).map_or_else(String::new, |b| b.name.into()),
    }
}

/// A custom sound's name: its file's, without the extension.
pub fn file_name(path: &str) -> String {
    std::path::Path::new(path).file_stem().map_or_else(|| path.into(), |n| n.to_string_lossy().into_owned())
}

/// Turns status changes into at most one sound per `COALESCE_MS` window.
pub struct Chime {
    statuses: HashMap<String, Status>,
    primed: bool,
    pending: Option<Cue>,
}

impl Chime {
    pub fn new() -> Self {
        Self { statuses: HashMap::new(), primed: false, pending: None }
    }

    /// Makes the next snapshot a silent baseline, as after a reconnect.
    pub fn rebase(&mut self) {
        self.primed = false;
    }

    /// Takes a snapshot; returns true when it opened a window, which the caller closes with `flush` after `COALESCE_MS`.
    pub fn heard(&mut self, agents: &[Summary], seen: &[String], on: &Sounds, now_ms: i64) -> bool {
        let now: HashMap<String, Status> = agents.iter().filter_map(|a| Some((a.id.clone(), Status::of(a)?))).collect();
        let before = &self.statuses;
        let cue = agents
            .iter()
            .filter(|a| self.primed && !seen.contains(&a.id))
            .filter_map(|a| {
                let s = now.get(&a.id)?;
                (before.get(&a.id)? != s).then_some((Cue::of(*s)?, a.updated_at))
            })
            .filter(|(cue, at)| on.all && cue.on(on) && (*cue != Cue::Done || now_ms - at <= FRESH_MS))
            .map(|(cue, _)| cue)
            .min();
        self.statuses = now;
        self.primed = true;
        let Some(cue) = cue else { return false };
        let opened = self.pending.is_none();
        self.pending = Some(self.pending.map_or(cue, |p| p.min(cue)));
        opened
    }

    pub fn flush(&mut self) -> Option<Cue> {
        self.pending.take()
    }
}

impl Desktop {
    /// Hears the agents' latest statuses; after a reconnect, only takes them as the new baseline.
    pub(crate) fn chime(&mut self, connected: bool, cx: &mut Context<Self>) {
        if connected {
            self.chime.rebase();
            return;
        }
        let seen = if self.store.notifications.on_screen { Vec::new() } else { self.alerts.viewing.clone().unwrap_or_default() };
        if !self.chime.heard(&self.agents.list, &seen, &self.store.sounds, now_ms()) {
            return;
        }
        cx.spawn(async move |this, cx| {
            cx.background_executor().timer(Duration::from_millis(COALESCE_MS)).await;
            this.update(cx, |d, cx| d.chime.flush().map(|_| play(&d.store.sounds.sound, d.store.sounds.volume, cx))).ok();
        })
        .detach();
    }
}

/// `afplay -v`'s gain for a volume in percent.
fn gain(volume: u32) -> String {
    format!("{}", f64::from(volume.min(100)) / 100.)
}

/// Plays `sound` with `afplay`, writing a built-in to the temp dir the first time.
pub(crate) fn play(sound: &Sound, volume: u32, cx: &App) {
    let sound = sound.clone();
    let gain = gain(volume);
    cx.background_executor()
        .spawn(async move {
            let path = match sound {
                Sound::Custom(path) => PathBuf::from(path),
                s => {
                    let b = BUILT_IN.iter().find(|b| b.sound == s).unwrap_or(&BUILT_IN[0]);
                    let dir = std::env::temp_dir().join("pocket-sounds");
                    let path = dir.join(b.file);
                    if std::fs::metadata(&path).map(|m| m.len()).ok() != Some(b.bytes.len() as u64) {
                        std::fs::create_dir_all(&dir).ok();
                        std::fs::write(&path, b.bytes).ok();
                    }
                    path
                }
            };
            std::process::Command::new("afplay").arg("-v").arg(gain).arg(&path).status().ok();
        })
        .detach();
}

#[cfg(test)]
mod tests {
    use super::{BUILT_IN, Chime, Cue, FRESH_MS, gain, sound_name};
    use agents::Summary;
    use store::{Sound, Sounds};

    const NOW: i64 = 1_000_000;

    fn agent(id: &str, status: &str) -> Summary {
        Summary { id: id.into(), status: status.into(), attached: true, updated_at: NOW, ..Default::default() }
    }

    fn primed(agents: &[Summary]) -> Chime {
        let mut c = Chime::new();
        c.heard(agents, &[], &Sounds::default(), NOW);
        c
    }

    #[test]
    fn the_first_snapshot_is_silent() {
        let mut c = Chime::new();
        assert!(!c.heard(&[agent("a", "needsYou")], &[], &Sounds::default(), NOW));
        assert_eq!(c.flush(), None);
    }

    #[test]
    fn a_reconnect_is_silent() {
        let mut c = primed(&[agent("a", "working")]);
        c.rebase();
        assert!(!c.heard(&[agent("a", "needsYou")], &[], &Sounds::default(), NOW));
        assert!(c.heard(&[agent("a", "done")], &[], &Sounds::default(), NOW));
    }

    #[test]
    fn the_highest_cue_wins_a_window() {
        let mut c = primed(&[agent("a", "working"), agent("b", "working"), agent("f", "working")]);
        assert!(c.heard(&[agent("a", "done"), agent("b", "working"), agent("f", "working")], &[], &Sounds::default(), NOW));
        let failed = Summary { failed: true, ..agent("f", "done") };
        assert!(!c.heard(&[agent("a", "done"), agent("b", "needsYou"), failed], &[], &Sounds::default(), NOW));
        assert_eq!((c.flush(), c.flush()), (Some(Cue::Failed), None));
    }

    #[test]
    fn seen_sessions_stay_silent() {
        let mut c = primed(&[agent("a", "working")]);
        assert!(!c.heard(&[agent("a", "done")], &["a".into()], &Sounds::default(), NOW));
    }

    #[test]
    fn a_stale_done_is_silent() {
        let mut c = primed(&[agent("a", "working")]);
        let stale = Summary { updated_at: NOW - FRESH_MS - 1, ..agent("a", "done") };
        assert!(!c.heard(&[stale], &[], &Sounds::default(), NOW));
    }

    #[test]
    fn a_muted_cue_is_silent() {
        let mut c = primed(&[agent("a", "working"), agent("b", "working")]);
        let on = Sounds { needs_you: false, ..Sounds::default() };
        assert!(!c.heard(&[agent("a", "needsYou"), agent("b", "working")], &[], &on, NOW));
        assert!(c.heard(&[agent("a", "needsYou"), agent("b", "done")], &[], &on, NOW));
        assert_eq!(c.flush(), Some(Cue::Done));
    }

    #[test]
    fn full_volume_plays_at_afplays_normal_gain() {
        assert_eq!([100, 70, 0, 250].map(gain), ["1", "0.7", "0", "1"]);
    }

    #[test]
    fn the_default_sound_is_the_first_built_in() {
        assert_eq!(BUILT_IN[0].sound, Sound::default());
    }

    #[test]
    fn a_custom_sound_is_named_by_its_file() {
        assert_eq!([sound_name(&Sound::HighDing), sound_name(&Sound::Custom("/s/Bloop.wav".into()))], ["High Ding", "Bloop"]);
    }

    #[test]
    fn muting_all_sounds_silences_every_cue() {
        let mut c = primed(&[agent("a", "working"), agent("b", "working"), agent("c", "working")]);
        let on = Sounds { all: false, ..Sounds::default() };
        assert!(!c.heard(&[agent("a", "needsYou"), agent("b", "done"), agent("c", "failed")], &[], &on, NOW));
    }
}
