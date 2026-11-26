//! The transcript's side of the find bar: the text each row is searched in, its matches painted, the current one shown.

use super::{View, message};
use crate::chat::find::Find;
use crate::chat::{markdown, user_bubble};
use chat::{Row, Transcript};
use gpui_kit::base::text::{RangeHighlight, RenderedText, TextViewState};
use gpui_kit::*;
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
        let rows = self.session.rows.len().max(1) as f32;
        let current = f.find.current().map(|h| h.row);
        let mut ticks: Vec<(f32, bool)> = Vec::new();
        for h in &f.find.hits {
            let tick = (h.row as f32 / rows, Some(h.row) == current);
            match ticks.last_mut() {
                Some(last) if last.0 == tick.0 => last.1 |= tick.1,
                _ => ticks.push(tick),
            }
        }
        ticks
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
        self.reveal(row, cx);
        cx.notify();
    }
}

/// Paints row `ix`'s matches on its document, and scrolls to the current one when it's here.
pub(super) fn paint(finding: &mut Option<Finding>, ix: usize, id: &str, code: bool, doc: &Entity<TextViewState>, window: &mut Window, cx: &mut App) {
    let Some(f) = finding else { return };
    let rendered = doc.read(cx).rendered_text();
    if rendered.is_empty() {
        // Its text hasn't parsed yet.
        return window.request_animation_frame();
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
