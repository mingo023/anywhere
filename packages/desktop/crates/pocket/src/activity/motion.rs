use super::logic::Places;
use gpui_kit::{Bounds, Pixels, Point, ease_out_quint, point, px};
use std::collections::HashMap;
use std::time::{Duration, Instant};

const ARRIVE: Duration = Duration::from_millis(220);
const SLIDE: Duration = Duration::from_millis(340);
const FLY: Duration = Duration::from_millis(520);

/// The sessions whose place changed, and those newly shown; a board that showed nothing has nothing arrive.
fn changes(before: &Places, after: &Places) -> (Vec<String>, Vec<String>) {
    if before.is_empty() {
        return (Vec::new(), Vec::new());
    }
    let moved = after.iter().filter(|(id, at)| before.get(*id).is_some_and(|was| was != *at)).map(|(id, _)| id.clone()).collect();
    let arrived = after.keys().filter(|id| !before.contains_key(*id)).cloned().collect();
    (moved, arrived)
}

fn progress(since: Instant, now: Instant, over: Duration) -> f32 {
    (now.saturating_duration_since(since).as_secs_f32() / over.as_secs_f32()).min(1.)
}

struct Move {
    from: Bounds<Pixels>,
    /// Where its card was drawn in its new place, and when the move set off.
    to: Option<(Bounds<Pixels>, Instant)>,
    /// A frame went by without its card drawn: it moved out of sight.
    waited: bool,
}

impl Move {
    fn over(&self, to: Bounds<Pixels>) -> Duration {
        if (self.from.origin.x - to.origin.x).abs() < px(4.) { SLIDE } else { FLY }
    }

    fn at(&self, now: Instant) -> Point<Pixels> {
        let Some((to, since)) = self.to else { return self.from.origin };
        let t = ease_out_quint()(progress(since, now, self.over(to)));
        point(self.from.origin.x + (to.origin.x - self.from.origin.x) * t, self.from.origin.y + (to.origin.y - self.from.origin.y) * t)
    }

    fn done(&self, now: Instant) -> bool {
        match self.to {
            Some((to, since)) => progress(since, now, self.over(to)) >= 1.,
            None => self.waited,
        }
    }
}

/// Cards going from where they were drawn to their new place: a copy flies there while the card waits hidden.
#[derive(Default)]
pub(super) struct Motion {
    pub reduce: bool,
    drawn: HashMap<String, Bounds<Pixels>>,
    moves: HashMap<String, Move>,
    arrivals: HashMap<String, Instant>,
}

impl Motion {
    /// The columns changed: cards that changed place set off from where they were drawn, new ones fade in.
    pub fn shift(&mut self, before: &Places, after: &Places, now: Instant) {
        if self.reduce {
            return;
        }
        let (moved, arrived) = changes(before, after);
        for id in moved {
            let Some(&drawn) = self.drawn.get(&id) else { continue };
            // A card still in flight sets off from where it is now, not where it was headed.
            let origin = self.moves.get(&id).map_or(drawn.origin, |m| m.at(now));
            self.moves.insert(id, Move { from: Bounds { origin, size: drawn.size }, to: None, waited: false });
        }
        self.arrivals.extend(arrived.into_iter().map(|id| (id, now)));
    }

    /// A card was drawn; true while it is still fading in and needs another frame.
    pub fn drew(&mut self, id: &str, bounds: Bounds<Pixels>, now: Instant) -> bool {
        self.drawn.insert(id.to_string(), bounds);
        if let Some(m) = self.moves.get_mut(id).filter(|m| m.to.is_none()) {
            if (m.from.origin.x - bounds.origin.x).abs() < px(1.) && (m.from.origin.y - bounds.origin.y).abs() < px(1.) {
                self.moves.remove(id);
            } else {
                m.to = Some((bounds, now));
            }
        }
        self.arrivals.contains_key(id)
    }

    /// A new frame: forgets where cards were drawn, as they are drawn again, and what has landed.
    pub fn frame(&mut self, now: Instant) {
        self.drawn.clear();
        self.moves.retain(|_, m| !m.done(now));
        for m in self.moves.values_mut().filter(|m| m.to.is_none()) {
            m.waited = true;
        }
        self.arrivals.retain(|_, since| progress(*since, now, ARRIVE) < 1.);
    }

    pub fn clear(&mut self) {
        let reduce = self.reduce;
        *self = Self { reduce, ..Self::default() };
    }

    /// How much of the card shows: none while its copy flies, rising as it arrives.
    pub fn opacity(&self, id: &str, now: Instant) -> f32 {
        if self.moves.contains_key(id) {
            return 0.;
        }
        self.arrivals.get(id).map_or(1., |since| ease_out_quint()(progress(*since, now, ARRIVE)))
    }

    /// The copies in flight: whose, where now, and how wide they land.
    pub fn flights(&self, now: Instant) -> Vec<(String, Point<Pixels>, Pixels)> {
        self.moves.iter().map(|(id, m)| (id.clone(), m.at(now), m.to.map_or(m.from, |(to, _)| to).size.width)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::{FLY, Motion, Places, SLIDE, changes};
    use gpui_kit::{Bounds, point, px, size};
    use std::time::Instant;

    fn places(at: &[(&str, usize, usize)]) -> Places {
        at.iter().map(|&(id, col, ix)| (id.to_string(), (col, ix))).collect()
    }

    fn at(x: f32, y: f32) -> Bounds<gpui_kit::Pixels> {
        Bounds::new(point(px(x), px(y)), size(px(200.), px(100.)))
    }

    #[test]
    fn a_card_moves_when_its_column_or_place_changes_and_arrives_when_new() {
        let before = places(&[("a", 0, 0), ("b", 0, 1), ("c", 1, 0)]);
        let after = places(&[("a", 0, 0), ("b", 2, 0), ("c", 1, 1), ("d", 1, 0)]);
        let (mut moved, arrived) = changes(&before, &after);
        moved.sort();
        assert_eq!((moved, arrived), (vec!["b".to_string(), "c".to_string()], vec!["d".to_string()]));
    }

    #[test]
    fn nothing_arrives_on_a_board_that_showed_nothing() {
        assert_eq!(changes(&Places::new(), &places(&[("a", 0, 0)])), (vec![], vec![]));
    }

    #[test]
    fn a_card_flies_from_where_it_was_drawn_to_where_it_lands() {
        let (mut m, now) = (Motion::default(), Instant::now());
        m.drew("b", at(0., 300.), now);
        m.shift(&places(&[("b", 0, 1)]), &places(&[("b", 2, 0)]), now);
        m.frame(now);
        assert_eq!(m.flights(now)[0].1, point(px(0.), px(300.)));
        assert_eq!(m.opacity("b", now), 0.);
        m.drew("b", at(400., 100.), now);
        assert_eq!(m.flights(now + FLY)[0].1, point(px(400.), px(100.)));
        m.frame(now + FLY);
        assert!(m.flights(now + FLY).is_empty());
        assert_eq!(m.opacity("b", now + FLY), 1.);
    }

    #[test]
    fn a_card_sliding_in_its_column_lands_sooner() {
        let (mut m, now) = (Motion::default(), Instant::now());
        m.drew("a", at(0., 300.), now);
        m.shift(&places(&[("a", 0, 1)]), &places(&[("a", 0, 0)]), now);
        m.frame(now);
        m.drew("a", at(0., 100.), now);
        m.frame(now + SLIDE);
        assert!(m.flights(now + SLIDE).is_empty());
    }

    #[test]
    fn a_card_whose_new_place_is_out_of_sight_stops_flying() {
        let (mut m, now) = (Motion::default(), Instant::now());
        m.drew("b", at(0., 300.), now);
        m.shift(&places(&[("b", 0, 1)]), &places(&[("b", 2, 9)]), now);
        m.frame(now);
        m.frame(now);
        assert!(m.flights(now).is_empty());
    }

    #[test]
    fn reduced_motion_moves_nothing() {
        let (mut m, now) = (Motion { reduce: true, ..Motion::default() }, Instant::now());
        m.drew("b", at(0., 300.), now);
        m.shift(&places(&[("b", 0, 1), ("x", 0, 0)]), &places(&[("b", 2, 0), ("c", 0, 0)]), now);
        assert!(m.flights(now).is_empty());
        assert_eq!(m.opacity("c", now), 1.);
    }
}
