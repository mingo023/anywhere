use std::time::{Duration, Instant};

/// Bytes a reply lets out each second, until a big chunk needs it faster.
const PACE: u64 = 320;
/// However much arrives at once, it shows within this…
const QUICKEST: Duration = Duration::from_millis(300);
/// …and this.
const SLOWEST: Duration = Duration::from_millis(1400);
/// How long a word takes to fade in once it shows.
pub(super) const FADE: Duration = Duration::from_millis(240);

/// How much of a streaming message shows. What arrives is let out at an even pace,
/// so a whole paragraph landing at once still reads as being written.
#[derive(Clone, Copy, Debug)]
pub(super) struct Reveal {
    from: usize,
    to: usize,
    start: Instant,
    took: Duration,
}

impl Reveal {
    /// A message `len` bytes long, all of it showing.
    pub(super) fn shown(len: usize, now: Instant) -> Self {
        Self { from: len, to: len, start: now, took: Duration::ZERO }
    }

    /// The message is now `len` bytes: let out what's new from wherever the reveal is.
    pub(super) fn arrive(&mut self, len: usize, now: Instant) {
        if len <= self.to {
            if len < self.to {
                *self = Self::shown(len, now);
            }
            return;
        }
        let from = self.at(now);
        let took = Duration::from_millis((len - from) as u64 * 1000 / PACE).clamp(QUICKEST, SLOWEST);
        *self = Self { from, to: len, start: now, took };
    }

    fn at(&self, now: Instant) -> usize {
        if self.took.is_zero() {
            return self.to;
        }
        let t = (now.saturating_duration_since(self.start).as_secs_f32() / self.took.as_secs_f32()).min(1.);
        self.from + ((self.to - self.from) as f32 * t) as usize
    }

    /// How much of `text` shows at `now`, in whole words: a word shows once the reveal reaches it.
    pub(super) fn cut(&self, text: &str, now: Instant) -> usize {
        let mut at = self.at(now).min(text.len());
        if at == 0 || at == text.len() {
            return at;
        }
        while !text.is_char_boundary(at) {
            at -= 1;
        }
        text[at..].find(char::is_whitespace).map_or(text.len(), |i| at + i)
    }

    /// Whether it's still letting text out, or fading in the last of it.
    pub(super) fn moving(&self, now: Instant) -> bool {
        now < self.start + self.took + FADE
    }
}

#[cfg(test)]
mod tests {
    use super::{FADE, Reveal};
    use std::time::{Duration, Instant};

    fn ms(n: u64) -> Duration {
        Duration::from_millis(n)
    }

    fn words(n: usize) -> String {
        "word ".repeat(n)
    }

    #[test]
    fn a_chunk_landing_at_once_shows_word_by_word_within_a_second_and_a_half() {
        let (text, t0) = (words(128), Instant::now());
        let mut r = Reveal::shown(0, t0);
        r.arrive(text.len(), t0);
        assert_eq!(r.cut(&text, t0), 0);
        let half = r.cut(&text, t0 + ms(700));
        assert!((300..340).contains(&half), "{half}");
        assert_eq!(&text[half - 4..half], "word", "ends on a whole word");
        assert_eq!(r.cut(&text, t0 + ms(1400)), text.len());
    }

    #[test]
    fn a_few_words_still_take_the_quickest_reveal() {
        let (text, t0) = (words(2), Instant::now());
        let mut r = Reveal::shown(0, t0);
        r.arrive(text.len(), t0);
        assert_eq!(r.cut(&text, t0 + ms(100)), 4);
        assert_eq!(r.cut(&text, t0 + ms(300)), text.len());
    }

    #[test]
    fn text_arriving_mid_reveal_carries_on_from_what_shows() {
        let (text, t0) = (words(128), Instant::now());
        let mut r = Reveal::shown(0, t0);
        r.arrive(320, t0);
        let before = r.cut(&text, t0 + ms(500));
        r.arrive(text.len(), t0 + ms(500));
        assert_eq!(r.cut(&text, t0 + ms(500)), before, "no jump");
        assert_eq!(r.cut(&text, t0 + ms(500) + ms(1400)), text.len());
    }

    #[test]
    fn a_reveal_moves_until_its_last_word_has_faded_in() {
        let t0 = Instant::now();
        let mut r = Reveal::shown(0, t0);
        assert!(!r.moving(t0 + FADE));
        r.arrive(10, t0);
        assert!(r.moving(t0 + ms(300) + FADE - ms(1)));
        assert!(!r.moving(t0 + ms(300) + FADE));
    }

    #[test]
    fn text_already_there_or_rewritten_shows_at_once() {
        let t0 = Instant::now();
        let text = "héllo wörld, ünïcode";
        assert_eq!(Reveal::shown(text.len(), t0).cut(text, t0), text.len());
        let mut r = Reveal::shown(0, t0);
        r.arrive(text.len(), t0);
        r.arrive(5, t0 + ms(10));
        assert_eq!(r.cut(&text[..5], t0 + ms(10)), 5);
    }

    #[test]
    fn a_cut_inside_a_wide_character_still_ends_on_a_word() {
        let t0 = Instant::now();
        let text = "héllo wörld";
        let mut r = Reveal::shown(0, t0);
        r.arrive(text.len(), t0);
        for n in 0..=300 {
            let cut = r.cut(text, t0 + ms(n));
            assert!(text.is_char_boundary(cut) && [0, 6, text.len()].contains(&cut), "{n}ms: {cut}");
        }
    }
}
