use super::Settings;
use gpui_kit::component::input::{Input, InputState};
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::time::{Duration, Instant};
use theme::*;

/// How long the "Saved" tick shows after a field commits.
pub(crate) const SAVED_FOR: Duration = Duration::from_millis(1600);

/// What a text field holds before it commits: its text, and why that text can't be saved.
#[derive(Clone, Debug, Default, PartialEq)]
pub(crate) struct Draft {
    pub text: String,
    pub error: Option<String>,
}

/// What committing a draft does.
#[derive(Debug, PartialEq)]
pub(crate) enum Commit {
    /// The text is what's stored; nothing to write.
    Same,
    Save(String),
    /// The text stays, with its error shown.
    Invalid,
}

impl Draft {
    pub(crate) fn new(text: impl Into<String>) -> Self {
        Self { text: text.into(), error: None }
    }

    /// Commits on Enter or blur: a valid text that differs from `stored` saves; an invalid one stays with its error.
    pub(crate) fn commit(&mut self, stored: &str, valid: impl Fn(&str) -> Result<(), String>) -> Commit {
        match valid(&self.text) {
            Err(error) => {
                self.error = Some(error);
                Commit::Invalid
            }
            Ok(()) => {
                self.error = None;
                if self.text == stored { Commit::Same } else { Commit::Save(self.text.clone()) }
            }
        }
    }

    /// Follows the store, unless the field is being typed in or holds an invalid draft the user hasn't given up on.
    pub(crate) fn follows(&self, stored: &str, focused: bool) -> bool {
        !focused && self.error.is_none() && self.text != stored
    }
}

pub(crate) fn any(_: &str) -> Result<(), String> {
    Ok(())
}

/// A text field that commits through a `Draft`, and when it last saved, for the tick.
pub(crate) struct TextField {
    pub input: Entity<InputState>,
    pub draft: Draft,
    pub saved: Option<Instant>,
}

impl TextField {
    pub(crate) fn new(input: Entity<InputState>, text: String) -> Self {
        Self { input, draft: Draft::new(text), saved: None }
    }

    /// Commits the input's text; the text to write when it saves.
    pub(crate) fn commit(&mut self, stored: &str, valid: impl Fn(&str) -> Result<(), String>, cx: &App) -> Commit {
        self.draft.text = self.input.read(cx).value().to_string();
        let commit = self.draft.commit(stored, valid);
        if matches!(commit, Commit::Save(_)) {
            self.saved = Some(Instant::now());
        }
        commit
    }

    /// Puts the stored text back in the field when the draft `follows` it.
    pub(crate) fn sync(&mut self, stored: &str, window: &mut Window, cx: &mut App) {
        let focused = self.input.read(cx).focus_handle(cx).is_focused(window);
        if self.draft.follows(stored, focused) {
            self.draft = Draft::new(stored);
            let stored = stored.to_string();
            self.input.update(cx, |f, cx| f.set_value(stored, window, cx));
        }
    }

    pub(crate) fn just_saved(&self, now: Instant) -> bool {
        self.saved.is_some_and(|at| now.duration_since(at) < SAVED_FOR)
    }

    /// For a field a page draws itself: puts its reason under the row while invalid, and the Saved tick beside it.
    pub(crate) fn decorate(&self) {
        super::DECOR.with_borrow_mut(|d| {
            let Some(d) = d else { return };
            if let Some(error) = self.draft.error.clone() {
                d.hint = Some(div().text_size(px(11.5)).text_color(FAILED_TEXT).child(error).into_any_element());
            }
            if self.just_saved(Instant::now()) {
                let saved = div().flex().items_center().gap(px(3.)).text_size(px(11.5)).font_weight(FontWeight::MEDIUM).text_color(SUCCESS);
                d.tags.push(saved.child(super::icon("check", 11., SUCCESS)).child("Saved").into_any_element());
            }
        });
    }
}

/// A unit a number can be typed in, and how many of the stored unit it holds: minutes in an hour is 60.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Unit {
    pub label: &'static str,
    pub factor: i64,
}

/// A stored number's bounds and step, in its stored unit.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Bounds {
    pub min: i64,
    pub max: i64,
    pub step: i64,
}

impl Bounds {
    fn round(self, n: f64) -> i64 {
        let step = self.step.max(1) as f64;
        ((n / step).round() * step) as i64
    }
}

/// `text` typed in `unit`, as the stored number rounded to the step; out of bounds or not a number is an error naming the range.
pub(crate) fn parse_amount(text: &str, unit: Unit, bounds: Bounds, units: &[Unit]) -> Result<i64, String> {
    let range = || format!("Between {} and {}", spoken(bounds.min, units), spoken(bounds.max, units));
    let n: f64 = text.trim().parse().map_err(|_| range())?;
    let n = bounds.round(n * unit.factor as f64);
    if n < bounds.min || n > bounds.max { Err(range()) } else { Ok(n) }
}

/// A stepper's typed value: kept in bounds and on the step; text that isn't a number keeps `current`.
pub(crate) fn typed_step(text: &str, current: i64, bounds: Bounds) -> i64 {
    text.trim().parse::<f64>().map_or(current, |n| bounds.round(n).clamp(bounds.min, bounds.max))
}

/// The largest unit `value` is a whole number of, so 120 minutes shows as 2 h.
pub(crate) fn best_unit(value: i64, units: &[Unit]) -> usize {
    units.iter().enumerate().rev().find(|(_, u)| u.factor > 0 && value % u.factor == 0).map_or(0, |(i, _)| i)
}

/// `value` written in `unit`, with up to two decimals and no trailing zeros.
pub(crate) fn amount(value: i64, unit: Unit) -> String {
    let n = value as f64 / unit.factor.max(1) as f64;
    let text = format!("{n:.2}");
    text.trim_end_matches('0').trim_end_matches('.').to_string()
}

/// `value` in its best unit, as an error or the reset sheet says it: "7 days", "5 min".
pub(crate) fn spoken(value: i64, units: &[Unit]) -> String {
    match units.get(best_unit(value, units)) {
        Some(unit) => format!("{} {}", amount(value, *unit), unit.label),
        None => value.to_string(),
    }
}

/// A typed number with a unit segment: an automation's interval in min, h or days. The caller owns it and its input's subscription.
pub(crate) struct NumberField {
    pub field: TextField,
    pub unit: usize,
    pub units: &'static [Unit],
    pub bounds: Bounds,
}

impl NumberField {
    pub(crate) fn new(value: i64, units: &'static [Unit], bounds: Bounds, window: &mut Window, cx: &mut App) -> Self {
        let unit = best_unit(value, units);
        let text = amount(value, units[unit]);
        let input = cx.new(|cx| InputState::new(window, cx).default_value(text.clone()));
        Self { field: TextField::new(input, text), unit, units, bounds }
    }

    /// Commits the typed amount in the chosen unit; the stored number when it changed.
    pub(crate) fn commit(&mut self, stored: i64, cx: &App) -> Option<i64> {
        let (unit, bounds, units) = (self.units[self.unit], self.bounds, self.units);
        let shown = amount(stored, unit);
        match self.field.commit(&shown, |t| parse_amount(t, unit, bounds, units).map(|_| ()), cx) {
            Commit::Save(text) => parse_amount(&text, unit, bounds, units).ok().filter(|n| *n != stored),
            Commit::Same | Commit::Invalid => None,
        }
    }

    /// Switches the unit the amount is typed in, rewriting the stored value in it.
    pub(crate) fn set_unit(&mut self, unit: usize, stored: i64, window: &mut Window, cx: &mut App) {
        self.unit = unit.min(self.units.len() - 1);
        let text = amount(stored, self.units[self.unit]);
        self.field.draft = Draft::new(text.clone());
        self.field.input.update(cx, |f, cx| f.set_value(text, window, cx));
    }

    /// Follows the store, in the unit that fits its value, unless typed in or holding an invalid draft; see `Draft::follows`.
    pub(crate) fn sync(&mut self, stored: i64, window: &mut Window, cx: &mut App) {
        let focused = self.field.input.read(cx).focus_handle(cx).is_focused(window);
        if self.field.draft.follows(&amount(stored, self.units[self.unit]), focused) {
            self.set_unit(best_unit(stored, self.units), stored, window, cx);
        }
    }

    /// The field and its unit segment; `pick` gets the unit chosen.
    pub(crate) fn view(&self, id: &'static str, pick: impl Fn(&mut Settings, usize, &mut Context<Settings>) + 'static, cx: &mut Context<Settings>) -> Div {
        let bad = self.field.draft.error.is_some();
        let input = super::field_box()
            .w(px(64.))
            .when(bad, |d| d.shadow(vec![ui::ring(FAILED, 1.)]))
            .child(div().flex_1().min_w_0().child(Input::new(&self.field.input).appearance(false).p_0().text_size(px(12.5))));
        let labels = self.units.iter().map(|u| SharedString::from(u.label)).collect();
        let units = super::segmented_control(SharedString::from(format!("{id}-unit")), labels, self.unit, pick, cx);
        div().flex().items_center().gap(px(8.)).child(input).child(units)
    }
}

/// A filter field for a long list; the caller owns `input` and narrows its rows with `filter_matches`.
pub(crate) fn filter_field(input: &Entity<InputState>) -> Div {
    div()
        .h(px(30.))
        .mx(px(12.))
        .my(px(10.))
        .px(px(9.))
        .flex()
        .items_center()
        .gap(px(7.))
        .rounded(px(8.))
        .bg(FILL_2)
        .child(icon("search", 13., TEXT_3))
        .child(div().flex_1().min_w_0().child(Input::new(input).appearance(false).p_0().text_size(px(13.))))
}

/// Whether every word of `query` is in one of `fields`, ignoring case; an empty query keeps everything.
pub(crate) fn filter_matches(query: &str, fields: &[&str]) -> bool {
    let fields: Vec<String> = fields.iter().map(|f| f.to_lowercase()).collect();
    query.split_whitespace().map(str::to_lowercase).all(|w| fields.iter().any(|f| f.contains(&w)))
}

#[cfg(test)]
mod tests {
    use super::{Bounds, Commit, Draft, Unit, amount, any, best_unit, filter_matches, parse_amount, spoken, typed_step};

    const MINUTES: &[Unit] = &[Unit { label: "min", factor: 1 }, Unit { label: "h", factor: 60 }, Unit { label: "days", factor: 1440 }];
    const EVERY: Bounds = Bounds { min: 5, max: 10_080, step: 5 };

    fn port(t: &str) -> Result<(), String> {
        t.parse::<u16>().map(|_| ()).map_err(|_| "Use a port from 1 to 65535".into())
    }

    #[test]
    fn a_draft_saves_on_commit_only_when_its_text_changed() {
        let mut draft = Draft::new("zsh");
        assert_eq!(draft.commit("zsh", any), Commit::Same);
        draft.text = "fish".into();
        assert_eq!(draft.commit("zsh", any), Commit::Save("fish".into()));
    }

    #[test]
    fn an_invalid_draft_keeps_its_text_and_shows_why() {
        let mut draft = Draft::new("80a");
        assert_eq!(draft.commit("8080", port), Commit::Invalid);
        assert_eq!(draft, Draft { text: "80a".into(), error: Some("Use a port from 1 to 65535".into()) });
        draft.text = "8081".into();
        assert_eq!(draft.commit("8080", port), Commit::Save("8081".into()));
        assert_eq!(draft.error, None);
    }

    #[test]
    fn a_draft_follows_the_store_unless_typed_in_or_invalid() {
        let draft = Draft::new("zsh");
        assert!(draft.follows("fish", false));
        assert!(!draft.follows("fish", true), "the field has focus");
        assert!(!draft.follows("zsh", false), "already the same");
        assert!(!Draft { text: "80a".into(), error: Some("bad".into()) }.follows("8080", false));
    }

    #[test]
    fn an_amount_is_typed_in_its_unit_and_rounded_to_the_step() {
        assert_eq!(parse_amount("2", MINUTES[1], EVERY, MINUTES), Ok(120));
        assert_eq!(parse_amount(" 1.5 ", MINUTES[2], EVERY, MINUTES), Ok(2160));
        assert_eq!(parse_amount("12", MINUTES[0], EVERY, MINUTES), Ok(10));
    }

    #[test]
    fn an_amount_out_of_range_or_not_a_number_names_the_range() {
        let range = Err("Between 5 min and 7 days".to_string());
        assert_eq!(parse_amount("8", MINUTES[2], EVERY, MINUTES), range);
        assert_eq!(parse_amount("1", MINUTES[0], EVERY, MINUTES), range);
        assert_eq!(parse_amount("soon", MINUTES[1], EVERY, MINUTES), range);
    }

    #[test]
    fn a_value_shows_in_the_largest_unit_it_fills() {
        assert_eq!([best_unit(90, MINUTES), best_unit(120, MINUTES), best_unit(2880, MINUTES)], [0, 1, 2]);
        assert_eq!([amount(90, MINUTES[1]), amount(120, MINUTES[1]), amount(100, MINUTES[1])], ["1.5", "2", "1.67"]);
        assert_eq!(spoken(10_080, MINUTES), "7 days");
    }

    #[test]
    fn a_typed_stepper_value_stays_in_range_and_on_the_step() {
        let size = Bounds { min: 9, max: 24, step: 1 };
        assert_eq!([typed_step("13", 12, size), typed_step("40", 12, size), typed_step("big", 12, size)], [13, 24, 12]);
        assert_eq!(typed_step("12", 900, Bounds { min: 100, max: 900, step: 100 }), 100);
        assert_eq!(typed_step("260", 900, Bounds { min: 100, max: 900, step: 100 }), 300);
    }

    #[test]
    fn a_filter_keeps_rows_holding_every_word_in_any_field() {
        let row = ["anywhere", "~/srv/anywhere", "build-box"];
        assert!(filter_matches("", &row));
        assert!(filter_matches("ANY build", &row));
        assert!(!filter_matches("any mac-mini", &row));
    }
}
