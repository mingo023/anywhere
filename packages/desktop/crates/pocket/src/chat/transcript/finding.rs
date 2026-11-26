//! The transcript's side of the find bar: the text each row is searched in, its matches painted, the current one shown.

use super::{View, message};
use crate::chat::find::Find;
use crate::chat::{markdown, user_bubble};
use chat::{Row, Transcript};
use gpui_kit::base::text::{RangeHighlight, RenderedText, TextViewState};
use gpui_kit::*;
use std::cell::RefCell;
use std::collections::HashMap;
use std::ops::Range;
use std::time::Instant;
use theme::*;

const HIT: Token = Token::new(0xffd40061, 0xffd40040);
const HIT_ON: Token = Token::new(0xffc53dff, 0xffc53d8c);
/// The current match at the height of its pop.
const HIT_POP: Token = Token::new(0xff9f0aff, 0xff9f0acc);

#[derive(Default)]
pub(super) struct Finding {
    pub(super) find: Find,
    /// Each document's matches as last painted, by row id: what was found, and the text it was painted on.
    painted: HashMap<String, (u64, RenderedText)>,
    /// The search the current match was last scrolled to for.
    revealed: Option<u64>,
    /// Whether the current match stays still when it lands: typing that keeps it in the same row doesn't pop it.
    quiet: bool,
    /// Each searched row's text length, to place a match within its row.
    lens: HashMap<usize, usize>,
    /// The ticks last placed, for the search and the chat height they were placed at.
    ticks: RefCell<Option<(u64, Pixels, Vec<f32>)>>,
}

/// Where on the chat's height a match sits, 0 at the top and 1 at the end: `frac` of the way down its row, placed by the
/// row's laid-out `place` (top, height) when it has one, else by the row count.
fn tick_at(row: usize, frac: f32, place: Option<(f32, f32)>, total: f32, rows: usize) -> f32 {
    match place {
        Some((top, height)) if total > 0. => (top + height * frac) / total,
        _ => (row as f32 + frac) / rows.max(1) as f32,
    }
}

/// What `row` is searched in: a message's prose or code, or the text its prompt's bubble shows.
pub(super) fn searched(t: &Transcript, row: &Row) -> Option<String> {
    match row {
        Row::Prompt { turn } => t.turns.get(*turn)?.prompt.as_ref().map(|p| user_bubble::shown_text(&p.text)),
        Row::Text { turn, block, range } => {
            let text = message(t, *turn, *block)?.text.get(range.clone())?;
            match markdown::fence(text) {
                Some(("mermaid", _)) => None,
                Some((_, code)) => Some(code.into_owned()),
                None => Some(text.to_string()),
            }
        }
        _ => None,
    }
}

/// The colour of a match: the current one stronger, and brighter while it pops.
pub(super) fn shade(current: bool, pop: f32) -> Hsla {
    match current {
        true => crate::composer::effort_slider::style::mix(HIT_ON.into(), HIT_POP.into(), pop),
        false => HIT.into(),
    }
}

impl View {
    /// Looks for `query` across the chat and goes to its first match.
    pub(crate) fn find(&mut self, query: &str, cx: &mut Context<Self>) {
        // Searching the same text again would go back to the first match, and the input can report a change that leaves its text as it was.
        if self.finding.as_ref().is_some_and(|f| f.find.holds(query)) {
            return;
        }
        let t = &self.session.transcript;
        let texts: Vec<(usize, String)> = self.session.rows.iter().enumerate().filter_map(|(i, r)| Some((i, searched(t, r)?))).collect();
        let f = self.finding.get_or_insert_default();
        f.lens = texts.iter().map(|(i, s)| (*i, s.len())).collect();
        let was = f.find.current().map(|h| h.row);
        f.find.search(query, texts.iter().map(|(i, s)| (*i, s.as_str())));
        // While typing, the first match pops only when it lands in another row; otherwise each key would flash it.
        let quiet = f.find.current().map(|h| h.row) == was;
        self.go(quiet, cx);
    }

    /// Looks again for what the bar holds, after the rows changed under it.
    pub(super) fn find_again(&mut self) {
        let Some(f) = &mut self.finding else { return };
        let t = &self.session.transcript;
        let texts: Vec<(usize, String)> = self.session.rows.iter().enumerate().filter_map(|(i, r)| Some((i, searched(t, r)?))).collect();
        f.lens = texts.iter().map(|(i, s)| (*i, s.len())).collect();
        f.find.research(texts.iter().map(|(i, s)| (*i, s.as_str())));
    }

    pub(crate) fn find_step(&mut self, down: bool, cx: &mut Context<Self>) {
        let Some(f) = &mut self.finding else { return };
        f.find.step(down);
        self.go(false, cx);
    }

    /// Clears the matches and shuts the folds opened to show them.
    pub(crate) fn end_find(&mut self, cx: &mut Context<Self>) {
        let Some(f) = self.finding.take() else { return };
        let opened: Vec<String> = f.find.opened.iter().filter(|fold| self.session.open(fold, false)).cloned().collect();
        for fold in &opened {
            self.toggle_row(fold, false, cx);
        }
        for id in f.painted.keys() {
            if let Some(doc) = self.docs.held(id) {
                doc.update(cx, |d, cx| d.clear_range_highlights(cx));
            }
        }
        cx.notify();
    }

    pub(crate) fn find_count(&self) -> Option<(Option<String>, bool)> {
        self.finding.as_ref().map(|f| (f.find.count(), !f.find.hits.is_empty()))
    }

    /// Where on the chat's height each match sits, 0 at the top and 1 at the end, and whether it's the current one.
    pub(super) fn find_ticks(&self) -> Vec<(f32, bool)> {
        let Some(f) = &self.finding else { return Vec::new() };
        let total = self.list.laid_out_height_from(0);
        let mut cache = f.ticks.borrow_mut();
        if cache.as_ref().is_none_or(|(version, at, _)| *version != f.find.version || *at != total) {
            let mut places: HashMap<usize, Option<(f32, f32)>> = HashMap::new();
            let ticks = f
                .find
                .hits
                .iter()
                .map(|h| {
                    let place = *places.entry(h.row).or_insert_with(|| self.row_place(h.row, total));
                    let frac = h.range.start as f32 / f.lens.get(&h.row).copied().unwrap_or(1).max(1) as f32;
                    tick_at(h.row, frac, place, f32::from(total), self.session.rows.len())
                })
                .collect();
            *cache = Some((f.find.version, total, ticks));
        }
        let current = f.find.current();
        let mut ticks: Vec<(f32, bool)> = Vec::new();
        for (h, at) in f.find.hits.iter().zip(cache.as_ref().map(|(_, _, t)| t.as_slice()).unwrap_or_default()) {
            let tick = (*at, current.is_some_and(|c| std::ptr::eq(c, h)));
            // Ticks closer than a pixel or so on the rail read as one.
            match ticks.last_mut() {
                Some(last) if (tick.0 - last.0).abs() < 0.002 => last.1 |= tick.1,
                _ => ticks.push(tick),
            }
        }
        ticks
    }

    /// Row `ix`'s top and height in the list as last laid out, when it was.
    fn row_place(&self, ix: usize, total: Pixels) -> Option<(f32, f32)> {
        self.list.laid_out(ix).then(|| {
            let from = self.list.laid_out_height_from(ix);
            (f32::from(total - from), f32::from(from - self.list.laid_out_height_from(ix + 1)))
        })
    }

    /// Scrolls so the current match sits mid-chat, as far as its row was laid out.
    fn centre(&self, row: usize) {
        let Some(f) = &self.finding else { return };
        let Some(h) = f.find.current() else { return };
        let total = self.list.laid_out_height_from(0);
        let Some((top, height)) = self.row_place(row, total) else { return };
        let frac = h.range.start as f32 / f.lens.get(&row).copied().unwrap_or(1).max(1) as f32;
        let half = f32::from(self.list.viewport_bounds().size.height) / 2.;
        let goal = (top + height * frac - half).max(0.);
        let top_of = |i: usize| f32::from(total - self.list.laid_out_height_from(i));
        let mut ix = row;
        while ix > 0 && top_of(ix) > goal {
            ix -= 1;
        }
        self.list.scroll_to(ListOffset { item_ix: ix, offset_in_item: px(goal - top_of(ix)) });
    }

    fn go(&mut self, quiet: bool, cx: &mut Context<Self>) {
        let Some(f) = &mut self.finding else { return };
        (f.quiet, f.revealed, f.find.landed) = (quiet, None, None);
        let Some(row) = f.find.current().map(|h| h.row) else { return cx.notify() };
        if let Some(fold) = self.session.shut_fold_over(row) {
            f.find.opened.push(fold);
        }
        // A prompt held at the top, or a glide to the end, would scroll the list off the match.
        (self.pin, self.glide, self.seek) = (None, None, None);
        // A long prompt shows its first lines only, which may not hold the match.
        let prompt = matches!(self.session.rows.get(row), Some(Row::Prompt { .. }));
        if let Some(id) = self.session.ids.get(row).filter(|id| prompt && !self.expanded.contains(*id)).cloned() {
            self.toggle_expanded(&id, cx);
        }
        self.reveal(row, cx);
        self.centre(row);
        cx.notify();
    }
}

/// Paints row `ix`'s matches on its document, and scrolls to the current one when it's here.
pub(super) fn paint(finding: &mut Option<Finding>, ix: usize, id: &str, code: bool, doc: &Entity<TextViewState>, window: &mut Window, cx: &mut App) {
    let Some(f) = finding else { return };
    let rendered = doc.read(cx).rendered_text();
    if rendered.is_empty() {
        // Not parsed yet, unless it parsed to nothing, like a lone rule.
        if rendered.source().is_empty() {
            window.request_animation_frame();
        }
        return;
    }
    let now = Instant::now();
    let here = f.find.current().is_some_and(|h| h.row == ix);
    let arriving = here && f.revealed != Some(f.find.version);
    if arriving && !f.quiet {
        f.find.landed = Some(now);
    }
    let pop = if here { f.find.pop(now) } else { 0. };
    let key = (f.find.version, rendered.clone());
    if !arriving && pop == 0. && f.painted.get(id) == Some(&key) {
        return;
    }
    // A code block's document holds it fenced, so its code starts after the opening fence.
    let offset = if code { rendered.source().find('\n').map_or(0, |i| i + 1) } else { 0 };
    let found: Vec<(Range<usize>, bool)> =
        f.find.in_row(ix).filter_map(|(r, current)| Some((rendered.range_for_source(r.start + offset..r.end + offset)?, current))).collect();
    let current = found.iter().find(|(_, c)| *c).map(|(r, _)| r.clone());
    doc.update(cx, |d, cx| {
        _ = d.set_range_highlights(found.into_iter().map(|(r, current)| RangeHighlight::new(r, shade(current, pop))), cx);
        if let Some(r) = current.filter(|_| arriving) {
            _ = d.reveal_range(r, cx);
        }
    });
    if arriving {
        f.revealed = Some(f.find.version);
    }
    f.painted.insert(id.to_string(), key);
    if pop > 0. {
        window.request_animation_frame();
    }
}

/// Row `ix`'s matches, for a bubble to paint itself.
pub(super) fn found_in(finding: &mut Option<Finding>, ix: usize, window: &mut Window) -> Vec<(Range<usize>, Hsla)> {
    let Some(f) = finding else { return Vec::new() };
    let now = Instant::now();
    let here = f.find.current().is_some_and(|h| h.row == ix);
    if here && f.revealed != Some(f.find.version) {
        f.revealed = Some(f.find.version);
        if !f.quiet {
            f.find.landed = Some(now);
        }
    }
    let pop = if here { f.find.pop(now) } else { 0. };
    if pop > 0. {
        window.request_animation_frame();
    }
    f.find.in_row(ix).map(|(r, current)| (r, shade(current, pop))).collect()
}

/// A tick on the chat's right edge for each place a match sits, the current one longer and solid.
pub(super) fn rail(ticks: Vec<(f32, bool)>) -> Option<Div> {
    (!ticks.is_empty()).then(|| {
        div().absolute().top(px(4.)).bottom(px(4.)).right(px(3.)).w(px(6.)).children(ticks.into_iter().map(|(at, current)| {
            div().absolute().top(relative(at)).left_0().w(px(6.)).h(px(if current { 3. } else { 2. })).rounded(px(1.)).bg(HIT_ON).opacity(if current {
                1.
            } else {
                0.55
            })
        }))
    })
}

#[cfg(test)]
mod tests {
    use super::tick_at;

    #[test]
    fn two_matches_in_one_row_get_their_own_ticks() {
        let place = Some((100., 200.));
        let (a, b) = (tick_at(3, 0.1, place, 1000., 10), tick_at(3, 0.9, place, 1000., 10));
        assert!((a - 0.12).abs() < 1e-4 && (b - 0.28).abs() < 1e-4);
    }

    #[test]
    fn a_row_not_laid_out_yet_is_placed_by_the_row_count() {
        assert!((tick_at(5, 0.5, None, 1000., 10) - 0.55).abs() < 1e-4);
    }
}
