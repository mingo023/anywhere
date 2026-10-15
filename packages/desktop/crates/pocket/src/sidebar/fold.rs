use gpui_kit::*;
use std::collections::HashMap;
use std::time::{Duration, Instant};
use theme::SPRING;

/// Long enough for `SPRING` to settle, so a fold's rows stay drawn until it has closed over them.
const SHUTTING: Duration = Duration::from_millis(500);

/// A sidebar fold: a project's trees, a group's projects, or Pinned.
#[derive(Clone, PartialEq, Eq, Hash)]
pub(crate) enum Fold {
    Project(String),
    Group(u32),
    Pinned,
}

/// Folds shut a moment ago.
#[derive(Default)]
pub(crate) struct Closing(HashMap<Fold, Instant>);

impl Closing {
    pub(crate) fn start(&mut self, fold: Fold) {
        self.0.retain(|_, at| at.elapsed() < SHUTTING);
        self.0.insert(fold, Instant::now());
    }

    /// Whether `fold`'s rows are drawn: it is open, or still shutting.
    pub(crate) fn drawn(&self, fold: &Fold, open: bool) -> bool {
        open || self.0.get(fold).is_some_and(|at| at.elapsed() < SHUTTING)
    }
}

/// The height of `heights` stacked `gap` apart; nothing when empty.
pub(super) fn stacked(heights: impl IntoIterator<Item = f32>, gap: f32) -> f32 {
    let (n, sum) = heights.into_iter().fold((0, 0.), |(n, sum), h| (n + 1, sum + h));
    if n == 0 { 0. } else { sum + gap * (n - 1) as f32 }
}

/// `rows` springing open to `height` and shut to nothing, clipped while they move; wrapped, since a height can't clip its own padding.
pub(super) fn springing(id: impl Into<ElementId>, open: bool, height: f32, rows: Div) -> SpringAnimationElement<Div> {
    div().flex().flex_col().child(rows).with_spring(id, SpringAnimation::new(SPRING).to(open), move |d, t| if t.0 < 1. { d.h(px(height * t.0)).overflow_hidden() } else { d })
}

#[cfg(test)]
mod tests {
    use super::{Closing, Fold, stacked};

    #[test]
    fn a_fold_just_shut_keeps_its_rows_drawn_and_others_stay_hidden() {
        let mut closing = Closing::default();
        closing.start(Fold::Group(1));
        assert!(closing.drawn(&Fold::Group(1), false));
        assert!(!closing.drawn(&Fold::Group(2), false));
        assert!(closing.drawn(&Fold::Pinned, true));
    }

    #[test]
    fn stacked_rows_add_a_gap_between_each_pair() {
        assert_eq!(stacked([28., 28., 28.], 1.), 86.);
        assert_eq!(stacked([], 1.), 0.);
    }
}
