use std::time::Instant;

/// How quickly a sliding item closes on where it's going: about 90% of the way in 80ms.
const SLIDE: f32 = 0.035;

/// An item held in a list, a strip's tabs or the sidebar's rows, and the others sliding out of its way; once released, every item sliding home.
/// Positions run along the list's one axis.
pub(crate) struct Slide {
    pub(crate) from: usize,
    /// Where the pointer holds the item, from its leading edge.
    pub(crate) grab: f32,
    pub(crate) pointer: f32,
    /// Each item's leading edge and size as laid out, before any slide.
    slots: Vec<(f32, f32)>,
    /// How far each item is drawn from its slot.
    pub(crate) offsets: Vec<f32>,
    /// The last frame, while any item is on its way.
    last_frame: Option<Instant>,
    pub(crate) released: bool,
    /// Held off its list: the other items slide home and a release here moves nothing.
    pub(crate) away: bool,
    /// The space between two items.
    gap: f32,
}

impl Slide {
    pub(crate) fn new(from: usize, grab: f32, pointer: f32, gap: f32) -> Self {
        Self { from, grab, pointer, slots: Vec::new(), offsets: Vec::new(), last_frame: None, released: false, away: false, gap }
    }

    /// Takes where the list laid its items out; false once it has gained or lost one.
    pub(crate) fn lay(&mut self, slots: Vec<(f32, f32)>) -> bool {
        if self.offsets.is_empty() {
            self.offsets = vec![0.; slots.len()];
        } else if slots.len() != self.offsets.len() {
            return false;
        }
        self.slots = slots;
        true
    }

    /// The held item's leading edge, under the pointer but within the list, and where it would land: past every item whose middle its leading edge has crossed.
    fn aim(&self) -> Option<(f32, usize)> {
        let (n, from) = (self.slots.len(), self.from);
        let &(_, w) = self.slots.get(from)?;
        let (first, last) = (self.slots[0], self.slots[n - 1]);
        let left = (self.pointer - self.grab).min(last.0 + last.1 - w).max(first.0);
        let crossed = |j: usize| {
            let mid = self.slots[j].0 + self.slots[j].1 / 2.;
            if j > from { left + w > mid } else { left < mid }
        };
        Some((left, from + (from + 1..n).filter(|&j| crossed(j)).count() - (0..from).filter(|&j| crossed(j)).count()))
    }

    /// How far each item should be drawn from its slot: the held one under the pointer, the ones it passed moved over to fill its place.
    fn targets(&self) -> Vec<f32> {
        let n = self.slots.len();
        let Some((left, to)) = self.aim().filter(|_| !self.released && !self.away) else { return vec![0.; n] };
        let step = self.slots[self.from].1 + self.gap;
        (0..n)
            .map(|i| match i {
                i if i == self.from => left - self.slots[i].0,
                i if self.from < i && i <= to => -step,
                i if to <= i && i < self.from => step,
                _ => 0.,
            })
            .collect()
    }

    pub(crate) fn held(&self) -> Option<usize> {
        (!self.released && !self.away).then_some(self.from)
    }

    /// Moves each item toward its target as of `now`, the held one straight there; true while any is still on its way.
    pub(crate) fn ease(&mut self, now: Instant, reduce_motion: bool) -> bool {
        let dt = self.last_frame.map_or(0., |t| now.duration_since(t).as_secs_f32());
        let k = if reduce_motion { 1. } else { 1. - (-dt / SLIDE).exp() };
        let (targets, held) = (self.targets(), self.held());
        for (i, (o, &t)) in self.offsets.iter_mut().zip(&targets).enumerate() {
            *o = if Some(i) == held || (t - *o).abs() < 0.5 { t } else { *o + (t - *o) * k };
        }
        let moving = self.offsets != targets;
        self.last_frame = moving.then_some(now);
        moving
    }

    /// The held item's middle under the pointer, unclamped, so it follows the pointer past the list.
    pub(crate) fn mid(&self) -> Option<f32> {
        let &(_, w) = self.slots.get(self.from)?;
        Some(self.pointer - self.grab + w / 2.)
    }

    /// Where the items run, from the first's leading edge to the last's trailing one.
    pub(crate) fn span(&self) -> Option<(f32, f32)> {
        let (first, last) = (self.slots.first()?, self.slots.last()?);
        Some((first.0, last.0 + last.1))
    }

    /// Lets the held item go, once, and returns where it moves from and to; each item then slides home from where it's drawn, in the new order.
    pub(crate) fn release(&mut self) -> Option<(usize, usize)> {
        if self.released {
            return None;
        }
        let targets = self.targets();
        self.released = true;
        let (_, to) = self.aim().filter(|_| !self.away)?;
        let (from, w) = (self.from, self.slots[self.from].1);
        let (l, lw) = self.slots[to];
        let landing = if to > from { l + lw - w } else { l };
        let mut offsets: Vec<f32> = self.offsets.iter().zip(targets).map(|(o, t)| o - t).collect();
        offsets.remove(from);
        offsets.insert(to, self.slots[from].0 + self.offsets[from] - landing);
        self.offsets = offsets;
        Some((from, to))
    }
}

#[cfg(test)]
mod tests {
    use super::Slide;
    use std::time::{Duration, Instant};

    /// Three 100px tabs 2px apart, tab `from` held 10px from its left edge.
    fn holding(from: usize, pointer: f32) -> Slide {
        let mut d = Slide::new(from, 10., pointer, 2.);
        d.lay(vec![(0., 100.), (102., 100.), (204., 100.)]);
        d
    }

    #[test]
    fn a_held_tab_lands_past_the_tabs_whose_middle_its_leading_edge_crossed() {
        assert_eq!(holding(0, 10.).release(), Some((0, 0)));
        assert_eq!(holding(0, 160.).release(), Some((0, 1)));
        assert_eq!(holding(2, 100.).release(), Some((2, 1)));
        assert_eq!(holding(2, 10.).release(), Some((2, 0)));
    }

    #[test]
    fn a_held_tab_stays_within_the_strip_yet_reaches_either_end() {
        let mut d = holding(0, 1000.);
        d.ease(Instant::now(), true);
        assert_eq!(d.offsets[0], 204.);
        assert_eq!(d.release(), Some((0, 2)));
        let mut d = holding(2, -500.);
        d.ease(Instant::now(), true);
        assert_eq!(d.offsets[2], -204.);
        assert_eq!(d.release(), Some((2, 0)));
    }

    #[test]
    fn the_tabs_a_held_tab_passes_move_over_to_fill_its_place() {
        let mut d = holding(0, 1000.);
        d.ease(Instant::now(), true);
        assert_eq!(d.offsets, [204., -102., -102.]);
        let mut d = holding(2, -500.);
        d.ease(Instant::now(), true);
        assert_eq!(d.offsets, [102., 102., -204.]);
    }

    #[test]
    fn passed_tabs_slide_while_the_held_one_stays_under_the_pointer() {
        let (mut d, t) = (holding(0, 1000.), Instant::now());
        assert!(d.ease(t, false));
        assert_eq!(d.offsets, [204., 0., 0.]);
        assert!(d.ease(t + Duration::from_millis(16), false));
        assert!(-102. < d.offsets[1] && d.offsets[1] < 0.);
        assert!(!d.ease(t + Duration::from_secs(1), false));
        assert_eq!(d.offsets, [204., -102., -102.]);
    }

    #[test]
    fn with_reduced_motion_tabs_jump_straight_to_their_place() {
        let mut d = holding(0, 1000.);
        assert!(!d.ease(Instant::now(), true));
        assert_eq!(d.offsets, [204., -102., -102.]);
    }

    #[test]
    fn a_released_tab_slides_home_from_where_it_was_let_go() {
        let mut d = holding(0, 160.);
        d.ease(Instant::now(), true);
        assert_eq!(d.release(), Some((0, 1)));
        assert_eq!(d.offsets, [0., 48., 0.]);
        assert_eq!(d.held(), None);
        assert!(!d.ease(Instant::now(), true));
        assert_eq!(d.offsets, [0., 0., 0.]);
    }

    #[test]
    fn a_tab_is_let_go_only_once() {
        let mut d = holding(0, 160.);
        assert_eq!(d.release(), Some((0, 1)));
        assert_eq!(d.release(), None);
    }

    #[test]
    fn a_release_before_the_strip_is_measured_moves_nothing() {
        assert_eq!(Slide::new(0, 10., 500., 2.).release(), None);
    }

    #[test]
    fn a_hold_ends_once_the_strip_gains_or_loses_a_tab() {
        let mut d = holding(0, 160.);
        assert!(d.lay(vec![(0., 100.), (102., 100.), (204., 100.)]));
        assert!(!d.lay(vec![(0., 100.), (102., 100.)]));
    }

    #[test]
    fn a_tab_held_off_its_strip_lets_the_others_slide_home_and_drops_nowhere() {
        let mut d = holding(0, 160.);
        d.away = true;
        d.ease(Instant::now(), true);
        assert_eq!((d.offsets.clone(), d.held()), (vec![0.; 3], None));
        assert_eq!(d.release(), None);
    }

    #[test]
    fn the_held_tab_s_middle_follows_the_pointer_past_the_strip() {
        assert_eq!(holding(1, 500.).mid(), Some(540.));
    }

    #[test]
    fn items_make_room_for_the_held_one_by_its_own_size_and_their_gap() {
        let mut d = Slide::new(0, 5., 1000., 1.);
        d.lay(vec![(0., 28.), (29., 60.), (90., 28.)]);
        d.ease(Instant::now(), true);
        assert_eq!(d.offsets, [90., -29., -29.]);
        assert_eq!(d.span(), Some((0., 118.)));
        assert_eq!(d.release(), Some((0, 2)));
    }
}
