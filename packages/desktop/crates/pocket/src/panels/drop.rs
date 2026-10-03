use super::strip::GAP;
use std::collections::HashMap;
use workspace::tree::{Axis, Edge, MIN_H, MIN_W, PaneId, Rect, Target};

/// A strip's height: the top of every pane, where a held tab joins the pane's tabs.
pub(crate) const STRIP_H: f32 = 42.;

/// Each strip's tabs as laid out, as (left, width) in the window.
pub(crate) type Slots = HashMap<PaneId, Vec<(f32, f32)>>;

/// Where a held tab would go.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Aim {
    /// Back over the strip it came from, which places it itself.
    Home,
    To(Target),
}

/// Where a tab of pane `from`, which holds `tabs` tabs, drops at `(x, y)` with its middle at `mid`.
/// Over a strip it joins past the tabs whose middle it passed; near a pane's edge it splits that pane unless the pane is too small, else it joins the pane.
/// Over its own pane it drops only to split it while leaving a tab behind, and outside every pane nowhere.
pub(crate) fn aim(layout: &[(PaneId, Rect)], (x, y): (f32, f32), from: PaneId, tabs: usize, mid: f32, slots: &Slots) -> Option<Aim> {
    let &(pane, r) = layout.iter().find(|(_, r)| r.x <= x && x < r.x + r.w && r.y <= y && y < r.y + r.h)?;
    let slots = slots.get(&pane).map_or(&[][..], Vec::as_slice);
    if y - r.y < STRIP_H {
        let index = slots.iter().filter(|(l, w)| l + w / 2. < mid).count();
        return Some(if pane == from { Aim::Home } else { Aim::To(Target::Into { pane, index }) });
    }
    match zone(r, (x, y)) {
        Some(edge) if pane != from || tabs > 1 => Some(Aim::To(Target::Split { pane, edge })),
        None if pane != from => Some(Aim::To(Target::Into { pane, index: slots.len() })),
        _ => None,
    }
}

/// The edge of pane `r` whose outer third of the body `(x, y)` is in, the nearest if several; `None` in the middle, or when the pane is too small to split that way.
fn zone(r: Rect, (x, y): (f32, f32)) -> Option<Edge> {
    let fx = (x - r.x) / r.w;
    let fy = (y - r.y - STRIP_H) / (r.h - STRIP_H);
    let (edge, d) = [(Edge::Left, fx), (Edge::Right, 1. - fx), (Edge::Top, fy), (Edge::Bottom, 1. - fy)].into_iter().min_by(|a, b| a.1.total_cmp(&b.1))?;
    let room = match edge.axis() {
        Axis::Row => r.w >= 2. * MIN_W,
        Axis::Column => r.h >= 2. * MIN_H,
    };
    (d < 1. / 3. && room).then_some(edge)
}

/// What a drop lights up in pane `r`: the half of its body a split takes, else all of it, inset so the pane's edges show.
pub(crate) fn highlight(r: Rect, edge: Option<Edge>) -> Rect {
    let body = Rect { y: r.y + STRIP_H, h: r.h - STRIP_H, ..r };
    let part = match edge {
        Some(Edge::Left) => Rect { w: body.w / 2., ..body },
        Some(Edge::Right) => Rect { x: body.x + body.w / 2., w: body.w / 2., ..body },
        Some(Edge::Top) => Rect { h: body.h / 2., ..body },
        Some(Edge::Bottom) => Rect { y: body.y + body.h / 2., h: body.h / 2., ..body },
        None => body,
    };
    Rect { x: part.x + 4., y: part.y + 4., w: part.w - 8., h: part.h - 8. }
}

/// Where a caret marks `index` among a strip's tabs: in the gap before that tab, else after the last; `None` in an empty strip.
pub(crate) fn caret(slots: &[(f32, f32)], index: usize) -> Option<f32> {
    match slots.get(index) {
        Some(&(l, _)) => Some(l - GAP / 2.),
        None => slots.last().map(|&(l, w)| l + w + GAP / 2.),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Two 600x400 panes side by side, `0` holding three 100px tabs, `1` holding two.
    fn two() -> (Vec<(PaneId, Rect)>, Slots) {
        let layout = vec![(0, Rect { x: 0., y: 0., w: 600., h: 400. }), (1, Rect { x: 600., y: 0., w: 600., h: 400. })];
        let slots = HashMap::from([(0, vec![(10., 100.), (112., 100.), (214., 100.)]), (1, vec![(610., 100.), (712., 100.)])]);
        (layout, slots)
    }

    fn at(pane: PaneId, index: usize) -> Option<Aim> {
        Some(Aim::To(Target::Into { pane, index }))
    }

    fn split(pane: PaneId, edge: Edge) -> Option<Aim> {
        Some(Aim::To(Target::Split { pane, edge }))
    }

    #[test]
    fn a_tab_over_another_strip_lands_past_the_tabs_whose_middle_it_passed() {
        let (layout, slots) = two();
        assert_eq!(aim(&layout, (650., 20.), 0, 3, 640., &slots), at(1, 0));
        assert_eq!(aim(&layout, (700., 20.), 0, 3, 700., &slots), at(1, 1));
        assert_eq!(aim(&layout, (1100., 20.), 0, 3, 1100., &slots), at(1, 2));
    }

    #[test]
    fn a_tab_over_its_own_strip_is_left_to_the_strip() {
        let (layout, slots) = two();
        assert_eq!(aim(&layout, (300., 20.), 0, 3, 300., &slots), Some(Aim::Home));
    }

    #[test]
    fn near_a_pane_edge_a_tab_splits_that_way_and_in_the_middle_joins_the_pane() {
        let (layout, slots) = two();
        assert_eq!(aim(&layout, (620., 220.), 0, 3, 620., &slots), split(1, Edge::Left));
        assert_eq!(aim(&layout, (1180., 220.), 0, 3, 1180., &slots), split(1, Edge::Right));
        assert_eq!(aim(&layout, (900., 50.), 0, 3, 900., &slots), split(1, Edge::Top));
        assert_eq!(aim(&layout, (900., 390.), 0, 3, 900., &slots), split(1, Edge::Bottom));
        assert_eq!(aim(&layout, (900., 220.), 0, 3, 900., &slots), at(1, 2));
    }

    #[test]
    fn a_pane_too_small_to_split_takes_the_tab_instead() {
        let layout = vec![(0, Rect { x: 0., y: 0., w: 400., h: 250. })];
        assert_eq!(aim(&layout, (10., 150.), 1, 1, 10., &HashMap::new()), at(0, 0));
        assert_eq!(aim(&layout, (200., 245.), 1, 1, 200., &HashMap::new()), at(0, 0));
    }

    #[test]
    fn over_its_own_pane_a_tab_drops_only_to_split_it_and_only_if_it_leaves_a_tab() {
        let (layout, slots) = two();
        assert_eq!(aim(&layout, (300., 220.), 0, 3, 300., &slots), None);
        assert_eq!(aim(&layout, (580., 220.), 0, 3, 580., &slots), split(0, Edge::Right));
        assert_eq!(aim(&layout, (580., 220.), 0, 1, 580., &slots), None);
    }

    #[test]
    fn a_tab_outside_every_pane_drops_nowhere() {
        let (layout, slots) = two();
        assert_eq!(aim(&layout, (1300., 220.), 0, 3, 1300., &slots), None);
    }

    #[test]
    fn the_highlight_fills_the_half_a_split_takes_or_the_whole_body() {
        let r = Rect { x: 600., y: 0., w: 600., h: 442. };
        assert_eq!(highlight(r, Some(Edge::Right)), Rect { x: 904., y: 46., w: 292., h: 392. });
        assert_eq!(highlight(r, Some(Edge::Bottom)), Rect { x: 604., y: 246., w: 592., h: 192. });
        assert_eq!(highlight(r, None), Rect { x: 604., y: 46., w: 592., h: 392. });
    }

    #[test]
    fn the_caret_sits_in_the_gap_before_its_tab_or_after_the_last() {
        let slots = [(10., 100.), (112., 100.)];
        assert_eq!((caret(&slots, 0), caret(&slots, 1), caret(&slots, 2), caret(&[], 0)), (Some(9.), Some(111.), Some(213.), None));
    }
}
