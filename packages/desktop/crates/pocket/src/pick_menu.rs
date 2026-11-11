//! A menu of picks under headings, which an agent's models can make a thousand lines long.

use crate::desktop::Desktop;
use gpui_kit::component::scroll::ScrollableElement as _;
use gpui_kit::*;
use std::cell::RefCell;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::ops::Range;
use std::rc::Rc;

/// A line of a menu: something to pick, or the name of the group under it.
#[derive(Debug, PartialEq)]
pub(crate) enum Line<P> {
    Heading(String),
    Pick(P),
}

/// The first line whose pick `is` holds: the one the menu checks.
pub(crate) fn checked<P>(lines: &[Line<P>], is: impl Fn(&P) -> bool) -> Option<usize> {
    lines.iter().position(|l| matches!(l, Line::Pick(p) if is(p)))
}

/// What a menu lists, built again only when what it was built from changes: every notify redraws the window, and an
/// agent can list a thousand models.
pub(crate) struct Memo<T>(RefCell<Option<(u64, Rc<T>)>>);

impl<T> Default for Memo<T> {
    fn default() -> Self {
        Self(RefCell::new(None))
    }
}

impl<T> Memo<T> {
    /// The value last built from `from`, else what `build` makes of it.
    pub(crate) fn get(&self, from: impl Hash, build: impl FnOnce() -> T) -> Rc<T> {
        let mut hasher = DefaultHasher::new();
        from.hash(&mut hasher);
        let key = hasher.finish();
        let mut kept = self.0.borrow_mut();
        match &*kept {
            Some((built_from, value)) if *built_from == key => value.clone(),
            _ => {
                let value = Rc::new(build());
                *kept = Some((key, value.clone()));
                value
            }
        }
    }
}

/// `count` lines in `frame`, with its scrollbar, of which `line` builds only those in view; `fit` sizes the list.
pub(crate) fn pick_list<R: IntoElement>(
    id: impl Into<ElementId>,
    count: usize,
    scroll: &UniformListScrollHandle,
    frame: Div,
    fit: impl FnOnce(UniformList) -> UniformList,
    line: impl Fn(&mut Desktop, usize, &mut Context<Desktop>) -> R + 'static,
    cx: &mut Context<Desktop>,
) -> Div {
    let list = uniform_list(id, count, cx.processor(move |this, range: Range<usize>, _, cx| range.map(|i| line(this, i, cx)).collect::<Vec<_>>())).track_scroll(scroll);
    frame.relative().child(fit(list)).vertical_scrollbar(scroll)
}

#[cfg(test)]
mod tests {
    use super::{Line, Memo, checked};

    #[test]
    fn a_kept_value_is_built_again_only_when_what_it_was_built_from_changes() {
        let (kept, built) = (Memo::default(), std::cell::Cell::new(0));
        let get = |from: (&str, u64)| {
            let value = kept.get(from, || {
                built.set(built.get() + 1);
                from.0.to_uppercase()
            });
            (*value).clone()
        };
        assert_eq!([get(("pi", 1)), get(("pi", 1)), get(("pi", 1))], ["PI", "PI", "PI"]);
        assert_eq!(built.get(), 1);
        assert_eq!([get(("pi", 2)), get(("omp", 2)), get(("omp", 2))], ["PI", "OMP", "OMP"]);
        assert_eq!(built.get(), 3);
    }

    #[test]
    fn the_line_checked_is_the_first_pick_that_holds_and_never_a_heading() {
        let lines = [Line::Heading("opus".to_string()), Line::Pick("haiku"), Line::Pick("opus"), Line::Pick("opus")];
        assert_eq!(checked(&lines, |p| *p == "opus"), Some(2));
        assert_eq!(checked(&lines, |p| *p == "sonnet"), None);
    }
}
