//! The chat's find bar: what it looks for, where that shows, and which place is current.

use std::ops::Range;
use std::time::{Duration, Instant};

/// How long the current match takes to settle from its pop to its resting shade.
pub(super) const POP: Duration = Duration::from_millis(260);

/// One place the query shows: the row, and the range of the text that row searches.
#[derive(Clone, Debug, PartialEq)]
pub(super) struct Hit {
    pub(super) row: usize,
    pub(super) range: Range<usize>,
}

#[derive(Default)]
pub(super) struct Find {
    pub(super) query: String,
    pub(super) hits: Vec<Hit>,
    current: usize,
    /// Folds opened to show a match, shut again when the bar closes.
    pub(super) opened: Vec<String>,
    /// When the current match was first painted: it pops once from then.
    pub(super) landed: Option<Instant>,
    /// Counts each search and step, so what was painted for an older one shows as stale.
    pub(super) version: u64,
}

impl Find {
    /// Looks for `query` in each row's text and makes the first match current.
    pub(super) fn search<'a>(&mut self, query: &str, texts: impl IntoIterator<Item = (usize, &'a str)>) {
        self.query = query.to_string();
        let needle = query.trim();
        self.hits = texts.into_iter().flat_map(|(row, text)| matches(text, needle).into_iter().map(move |range| Hit { row, range })).collect();
        self.current = 0;
        self.landed = None;
        self.version += 1;
    }

    /// Looks for the same query again in rows that changed, keeping the current match's place where it can.
    pub(super) fn research<'a>(&mut self, texts: impl IntoIterator<Item = (usize, &'a str)>) {
        let needle = self.query.trim().to_string();
        self.hits = texts.into_iter().flat_map(|(row, text)| matches(text, &needle).into_iter().map(move |range| Hit { row, range })).collect();
        self.current = self.current.min(self.hits.len().saturating_sub(1));
        self.version += 1;
    }

    /// Moves to the next match, or the previous one, wrapping at either end.
    pub(super) fn step(&mut self, down: bool) {
        let n = self.hits.len();
        if n == 0 {
            return;
        }
        self.current = if down { (self.current + 1) % n } else { (self.current + n - 1) % n };
        self.landed = None;
        self.version += 1;
    }

    /// Whether `query` is what was last searched for.
    pub(super) fn holds(&self, query: &str) -> bool {
        self.version > 0 && self.query == query
    }

    pub(super) fn current(&self) -> Option<&Hit> {
        self.hits.get(self.current)
    }

    /// The matches in `row`, each with whether it's the current one.
    pub(super) fn in_row(&self, row: usize) -> impl Iterator<Item = (Range<usize>, bool)> + '_ {
        let current = self.current;
        self.hits.iter().enumerate().filter(move |(_, h)| h.row == row).map(move |(i, h)| (h.range.clone(), i == current))
    }

    /// "3 of 12", "No results" once something is typed, or nothing.
    pub(super) fn count(&self) -> Option<String> {
        match self.hits.len() {
            _ if self.query.trim().is_empty() => None,
            0 => Some("No results".into()),
            n => Some(format!("{} of {n}", self.current + 1)),
        }
    }

    /// How far the current match's pop has to go, 1 when it starts and 0 once it's over.
    pub(super) fn pop(&self, now: Instant) -> f32 {
        self.landed.map_or(0., |at| 1. - (now.saturating_duration_since(at).as_secs_f32() / POP.as_secs_f32()).min(1.))
    }
}

/// Each place `needle` shows in `hay`, ignoring case, as byte ranges of `hay`.
pub(super) fn matches(hay: &str, needle: &str) -> Vec<Range<usize>> {
    let mut out = Vec::new();
    if needle.is_empty() {
        return out;
    }
    let mut at = 0;
    while at < hay.len() {
        match starts_with_folded(&hay[at..], needle) {
            Some(len) => {
                out.push(at..at + len);
                at += len;
            }
            None => at += hay[at..].chars().next().map_or(1, char::len_utf8),
        }
    }
    out
}

/// The length of `hay`'s start that reads as `needle` with case ignored, if it does.
fn starts_with_folded(hay: &str, needle: &str) -> Option<usize> {
    let mut chars = hay.char_indices();
    for n in needle.chars() {
        let (_, h) = chars.next()?;
        if !h.to_lowercase().eq(n.to_lowercase()) {
            return None;
        }
    }
    Some(chars.next().map_or(hay.len(), |(i, _)| i))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn find(query: &str, rows: &[&str]) -> Find {
        let mut f = Find::default();
        f.search(query, rows.iter().enumerate().map(|(i, t)| (i, *t)));
        f
    }

    #[test]
    fn a_match_ignores_case_and_reports_where_it_shows() {
        assert_eq!(matches("The Payload and the payload", "payload"), [4..11, 20..27]);
    }

    #[test]
    fn a_match_after_wide_characters_keeps_byte_offsets() {
        assert_eq!(matches("Đã gửi payload", "PAYLOAD"), vec![11..18]);
    }

    #[test]
    fn matches_dont_overlap() {
        assert_eq!(matches("aaaa", "aa"), [0..2, 2..4]);
    }

    #[test]
    fn stepping_wraps_at_either_end() {
        let mut f = find("x", &["x x", "x"]);
        assert_eq!(f.count().as_deref(), Some("1 of 3"));
        f.step(false);
        assert_eq!(f.current(), Some(&Hit { row: 1, range: 0..1 }));
        f.step(true);
        assert_eq!(f.count().as_deref(), Some("1 of 3"));
    }

    #[test]
    fn a_query_found_nowhere_says_so_and_an_empty_one_says_nothing() {
        assert_eq!(find("zzz", &["abc"]).count().as_deref(), Some("No results"));
        assert_eq!(find("  ", &["abc"]).count(), None);
    }

    #[test]
    fn a_row_lists_its_own_matches_and_marks_the_current_one() {
        let mut f = find("a", &["a", "ba a"]);
        f.step(true);
        assert_eq!(f.in_row(1).collect::<Vec<_>>(), [(1..2, true), (3..4, false)]);
    }

    #[test]
    fn looking_again_keeps_the_current_match_while_it_still_exists() {
        let mut f = find("a", &["a a a"]);
        f.step(true);
        f.step(true);
        f.research([(0, "a a")]);
        assert_eq!(f.count().as_deref(), Some("2 of 2"));
    }

    #[test]
    fn the_pop_fades_from_full_to_nothing() {
        let mut f = Find::default();
        let now = Instant::now();
        assert_eq!(f.pop(now), 0.);
        f.landed = Some(now);
        assert_eq!(f.pop(now), 1.);
        assert_eq!(f.pop(now + POP), 0.);
    }

    #[test]
    fn the_same_query_is_held_after_a_step_and_another_one_isnt() {
        let mut f = Find::default();
        assert!(!f.holds(""));
        f.search("ab", [(0, "ab ab")]);
        f.step(true);
        assert!(f.holds("ab") && !f.holds("abc"));
    }
}
