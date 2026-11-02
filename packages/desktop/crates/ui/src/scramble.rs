use gpui_kit::*;
use std::time::{Duration, Instant};
use theme::*;

/// How long noise takes to reach one letter further into the old name, in ms.
const WAVE: f32 = 20.;
/// When the new name's first letter locks in, in ms.
const HOLD: f32 = 180.;
/// The most time between two letters locking in, in ms.
const STEP: f32 = 30.;
/// About what a sidebar row shows of a name. Its first `SPAN` letters lock in within `LOCKS` ms; the rest land with the last of them.
const SPAN: usize = 32;
const LOCKS: f32 = 560.;
/// How long a locked letter takes to fade from its flash to the label's colour, in ms.
const FLASH: f32 = 240.;
/// How often the noise reshuffles, in ms; every frame would read as flicker.
const RESHUFFLE: f32 = 50.;
const NOISE: &[u8] = b"abcdeghknopqsuvxyz0123456789";
/// Characters that keep their place through the decode, so the new name's shape shows before its letters.
const SEPARATORS: &[char] = &['-', ' ', '/', '_', '.', ':'];

#[derive(Clone, Copy, PartialEq, Debug)]
enum Letter {
    /// Drawn plainly: an old letter not yet reached, or a separator.
    Plain,
    Noise,
    /// How far through its flash, 0 to 1.
    Locked(f32),
}

fn lock_at(i: usize, n: usize) -> f32 {
    let span = n.clamp(1, SPAN);
    HOLD + i.min(span - 1) as f32 * STEP.min(LOCKS / span as f32)
}

/// How long `to` takes to decode; after it, the label reads `to` plainly.
pub fn scramble_duration(to: &str) -> Duration {
    let n = to.chars().count().max(1);
    Duration::from_secs_f32((lock_at(n - 1, n) + FLASH) / 1000.)
}

fn noise(i: usize, step: usize) -> char {
    let h = (i as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) ^ (step as u64).wrapping_mul(0xC2B2_AE3D_27D4_EB4F);
    NOISE[((h ^ (h >> 31)) % NOISE.len() as u64) as usize] as char
}

/// What a label reads `ms` into decoding `to` out of `from`: noise spreads over the old name from its start, then the new one locks in left to right.
fn scrambled(from: &str, to: &str, ms: f32) -> Vec<(char, Letter)> {
    let (from, to): (Vec<char>, Vec<char>) = (from.chars().collect(), to.chars().collect());
    let step = (ms / RESHUFFLE) as usize;
    let settled = lock_at(to.len().saturating_sub(1), to.len());
    (0..from.len().max(to.len()))
        .filter_map(|i| {
            let reached = ms >= (i + 1) as f32 * WAVE;
            let old = || from.get(i).map(|&c| (c, Letter::Plain));
            match to.get(i) {
                Some(&c) => {
                    let lock = lock_at(i, to.len());
                    if SEPARATORS.contains(&c) && (reached || ms >= lock) {
                        Some((c, Letter::Plain))
                    } else if ms >= lock {
                        Some((c, Letter::Locked(((ms - lock) / FLASH).min(1.))))
                    } else if reached {
                        Some((noise(i, step), Letter::Noise))
                    } else {
                        old()
                    }
                }
                None if ms >= settled => None,
                None if reached => Some((noise(i, step), Letter::Noise)),
                None => old(),
            }
        })
        .collect()
}

/// `to` in `base`, decoding out of `from` since a rename at `since`: the old name scrambles into noise, then the new one locks in left to right, each letter flashing as it lands.
pub fn scramble(id: impl Into<ElementId>, from: SharedString, to: SharedString, base: Token, since: Instant) -> AnimationElement<Div> {
    let (base, peak, dim): (Hsla, Hsla, Hsla) = (base.into(), SHIMMER.into(), TEXT_4.into());
    let length = scramble_duration(&to);
    // Only a ticker: each frame is read off `since`, so a row first drawn mid-decode joins it instead of starting over.
    // Uncapped, it would redraw the whole window every vsync.
    div().with_animation(id, Animation::new(length).repeat().with_max_fps(30.), move |d, _| {
        let letters = scrambled(&from, &to, since.elapsed().min(length).as_secs_f32() * 1000.);
        let mut at = 0;
        let runs: Vec<_> = letters
            .iter()
            .map(|&(c, letter)| {
                let color = match letter {
                    Letter::Plain => base,
                    Letter::Noise => dim,
                    Letter::Locked(f) => base.blend(Hsla { a: 1. - f, ..peak }),
                };
                at += c.len_utf8();
                (at - c.len_utf8()..at, HighlightStyle { color: Some(color), ..Default::default() })
            })
            .collect();
        d.child(StyledText::new(letters.iter().map(|(c, _)| c).collect::<String>()).with_highlights(runs))
    })
}

#[cfg(test)]
mod tests {
    use super::{Letter, scramble_duration, scrambled};

    fn read(letters: &[(char, Letter)]) -> String {
        letters.iter().map(|(c, _)| c).collect()
    }

    fn end(to: &str) -> f32 {
        scramble_duration(to).as_secs_f32() * 1000.
    }

    #[test]
    fn a_scrambled_name_starts_as_the_old_one_and_settles_as_the_new_one() {
        assert_eq!(read(&scrambled("calm-otter", "fix-login-redirect", 0.)), "calm-otter");
        let settled = scrambled("calm-otter", "fix-login-redirect", end("fix-login-redirect"));
        assert_eq!(read(&settled), "fix-login-redirect");
        assert!(settled.iter().all(|&(_, l)| l == Letter::Plain || l == Letter::Locked(1.)));
    }

    #[test]
    fn a_scrambling_name_keeps_its_separators_still_while_its_letters_are_noise() {
        let mid = scrambled("calm-otter", "fix-login", 150.);
        assert_eq!(mid[3], ('-', Letter::Plain));
        assert!(mid[..3].iter().all(|&(_, l)| l == Letter::Noise));
    }

    #[test]
    fn letters_the_new_name_has_no_room_for_turn_to_noise_before_they_go() {
        assert_eq!(scrambled("calm-otter", "fix", 220.)[9].1, Letter::Noise);
        assert_eq!(read(&scrambled("calm-otter", "fix", end("fix"))), "fix");
    }

    #[test]
    fn a_long_name_settles_whole_in_no_more_time_than_what_a_row_shows_of_it() {
        let long = "a".repeat(150);
        assert_eq!(read(&scrambled("calm-otter", &long, end(&long))), long);
        assert_eq!(scramble_duration(&long), scramble_duration(&"a".repeat(32)));
        assert!(scramble_duration(&long).as_millis() <= 1000);
    }
}
