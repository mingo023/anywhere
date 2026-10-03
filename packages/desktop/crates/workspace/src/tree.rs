use serde::{Deserialize, Serialize};

pub type PaneId = u64;

/// `Row` lays children side by side, `Column` one under another.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Axis {
    Row,
    Column,
}

impl Axis {
    fn min(self) -> f32 {
        match self {
            Axis::Row => MIN_W,
            Axis::Column => MIN_H,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Edge {
    Left,
    Right,
    Top,
    Bottom,
}

impl Edge {
    pub fn axis(self) -> Axis {
        match self {
            Edge::Left | Edge::Right => Axis::Row,
            Edge::Top | Edge::Bottom => Axis::Column,
        }
    }

    fn after(self) -> bool {
        matches!(self, Edge::Right | Edge::Bottom)
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pane<T> {
    pub id: PaneId,
    pub tabs: Vec<T>,
    pub active: usize,
}

impl<T> Pane<T> {
    pub fn active(&self) -> Option<&T> {
        self.tabs.get(self.active)
    }
}

/// A split's `sizes` are its children's shares of its length, summing to 1.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Node<T> {
    Pane(Pane<T>),
    Split { axis: Axis, sizes: Vec<f32>, children: Vec<Node<T>> },
}

/// Where a moved tab goes: into a pane's strip at `index`, or into a new pane at `edge` of a pane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Into { pane: PaneId, index: usize },
    Split { pane: PaneId, edge: Edge },
}

/// The line between child `i` and `i + 1` of the split reached from the root by `path`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Divider {
    pub path: Vec<usize>,
    pub i: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub const UNIT: Rect = Rect { x: 0., y: 0., w: 1., h: 1. };

/// The smallest a pane may be drawn; a split or resize that would go below it doesn't happen.
pub const MIN_W: f32 = 260.;

pub const MIN_H: f32 = 150.;

const EPS: f32 = 1e-4;

impl Rect {
    fn centre(&self) -> (f32, f32) {
        (self.x + self.w / 2., self.y + self.h / 2.)
    }

    fn contains(&self, (x, y): (f32, f32)) -> bool {
        self.x <= x && x <= self.x + self.w && self.y <= y && y <= self.y + self.h
    }

    fn len(&self, axis: Axis) -> f32 {
        match axis {
            Axis::Row => self.w,
            Axis::Column => self.h,
        }
    }
}

impl<T> Node<T> {
    fn collect<'a>(&'a self, out: &mut Vec<&'a Pane<T>>) {
        match self {
            Node::Pane(p) => out.push(p),
            Node::Split { children, .. } => children.iter().for_each(|c| c.collect(out)),
        }
    }

    fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane<T>> {
        match self {
            Node::Pane(p) => (p.id == id).then_some(p),
            Node::Split { children, .. } => children.iter_mut().find_map(|c| c.pane_mut(id)),
        }
    }

    fn each_pane_mut(&mut self, f: &mut impl FnMut(&mut Pane<T>)) {
        match self {
            Node::Pane(p) => f(p),
            Node::Split { children, .. } => children.iter_mut().for_each(|c| c.each_pane_mut(f)),
        }
    }

    fn at_mut(&mut self, path: &[usize]) -> Option<&mut Node<T>> {
        match (path.split_first(), self) {
            (None, node) => Some(node),
            (Some((&i, rest)), Node::Split { children, .. }) => children.get_mut(i)?.at_mut(rest),
            (Some(_), Node::Pane(_)) => None,
        }
    }

    /// The least length along `axis` this node can be drawn at without a pane going below its minimum.
    fn min_len(&self, axis: Axis) -> f32 {
        match self {
            Node::Pane(_) => axis.min(),
            Node::Split { axis: a, children, .. } if *a == axis => children.iter().map(|c| c.min_len(axis)).sum(),
            Node::Split { children, .. } => children.iter().map(|c| c.min_len(axis)).fold(0., f32::max),
        }
    }

    fn equalize_all(&mut self) {
        if let Node::Split { sizes, children, .. } = self {
            let n = sizes.len() as f32;
            sizes.iter_mut().for_each(|s| *s = 1. / n);
            children.iter_mut().for_each(Node::equalize_all);
        }
    }

    fn layout(&self, r: Rect, out: &mut Vec<(PaneId, Rect)>) {
        match self {
            Node::Pane(p) => out.push((p.id, r)),
            Node::Split { axis, sizes, children } => {
                let mut at = 0.;
                for (s, c) in sizes.iter().zip(children) {
                    let part = match axis {
                        Axis::Row => Rect { x: r.x + at * r.w, w: s * r.w, ..r },
                        Axis::Column => Rect { y: r.y + at * r.h, h: s * r.h, ..r },
                    };
                    c.layout(part, out);
                    at += s;
                }
            }
        }
    }

    /// Puts `new` at `edge` of pane `target`, handing `new` back when no pane is `target`.
    fn split(&mut self, target: PaneId, edge: Edge, new: Node<T>) -> Result<(), Node<T>> {
        match self {
            Node::Pane(p) if p.id == target => {
                let old = std::mem::replace(self, Node::Split { axis: edge.axis(), sizes: Vec::new(), children: Vec::new() });
                let children = if edge.after() { vec![old, new] } else { vec![new, old] };
                *self = Node::Split { axis: edge.axis(), sizes: vec![0.5, 0.5], children };
                Ok(())
            }
            Node::Pane(_) => Err(new),
            Node::Split { children, .. } => {
                let mut new = new;
                for c in children {
                    match c.split(target, edge, new) {
                        Ok(()) => return Ok(()),
                        Err(back) => new = back,
                    }
                }
                Err(new)
            }
        }
    }

    /// Mends what a saved tree may get wrong and edits rely on: sizes that don't fit the children become equal shares, active tabs come into range, and panes are numbered from `next` in reading order. `focused` becomes the new id of the first pane that had id `was`.
    fn repair(&mut self, next: &mut PaneId, focused: &mut Option<PaneId>, was: PaneId) {
        match self {
            Node::Pane(p) => {
                p.active = p.active.min(p.tabs.len().saturating_sub(1));
                if p.id == was && focused.is_none() {
                    *focused = Some(*next);
                }
                p.id = *next;
                *next += 1;
            }
            Node::Split { sizes, children, .. } => {
                if sizes.len() != children.len() || sizes.iter().any(|s| !s.is_finite() || *s <= 0.) || !sizes.iter().sum::<f32>().is_finite() {
                    *sizes = vec![1. / children.len() as f32; children.len()];
                }
                children.iter_mut().for_each(|c| c.repair(next, focused, was));
            }
        }
    }

    /// Drops empty panes, lets a split left with one child give way to it, and merges a split into a parent along the same axis.
    fn prune(self) -> Option<Node<T>> {
        match self {
            Node::Pane(p) => (!p.tabs.is_empty()).then_some(Node::Pane(p)),
            Node::Split { axis, sizes, children } => {
                let mut kept: Vec<(f32, Node<T>)> = Vec::new();
                for (s, c) in sizes.into_iter().zip(children) {
                    match c.prune() {
                        Some(Node::Split { axis: a, sizes: inner, children: grand }) if a == axis => kept.extend(inner.into_iter().map(|x| x * s).zip(grand)),
                        Some(c) => kept.push((s, c)),
                        None => {}
                    }
                }
                match kept.len() {
                    0 => None,
                    1 => kept.pop().map(|(_, c)| c),
                    _ => {
                        let total: f32 = kept.iter().map(|(s, _)| s).sum();
                        let (sizes, children) = kept.into_iter().map(|(s, c)| (s / total, c)).unzip();
                        Some(Node::Split { axis, sizes, children })
                    }
                }
            }
        }
    }
}

/// A worktree's panes: a tree of splits whose leaves each hold a strip of tabs.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(from = "Saved<T>")]
pub struct Tree<T> {
    pub root: Node<T>,
    /// Always a pane in `root`.
    pub focused: PaneId,
    /// The pane drawn alone over the rest, while there are others.
    #[serde(skip_serializing)]
    pub zoomed: Option<PaneId>,
    #[serde(skip_serializing)]
    next: PaneId,
}

/// What a tree is written as; reading one back goes through `Tree::from_root`, so a hand-edited or stale file still yields a sound tree.
#[derive(Deserialize)]
struct Saved<T> {
    root: Node<T>,
    focused: PaneId,
}

impl<T> From<Saved<T>> for Tree<T> {
    fn from(s: Saved<T>) -> Self {
        Self::from_root(s.root, s.focused)
    }
}

impl<T> Default for Tree<T> {
    fn default() -> Self {
        Self { root: Node::Pane(Pane { id: 0, tabs: Vec::new(), active: 0 }), focused: 0, zoomed: None, next: 1 }
    }
}

impl<T> Tree<T> {
    /// The tree around `root`: panes renumbered in reading order, so saved ids can neither repeat nor overflow, empty panes closed, and `focused` kept if it is still a pane.
    pub fn from_root(mut root: Node<T>, focused: PaneId) -> Self {
        let (mut next, mut found) = (0, None);
        root.repair(&mut next, &mut found, focused);
        let mut t = Self { root, focused: found.unwrap_or(next), zoomed: None, next: 0 };
        t.tidy();
        t.next = t.panes().iter().map(|p| p.id + 1).max().unwrap_or(0);
        t
    }

    /// In reading order: left to right, then top to bottom within each split.
    pub fn panes(&self) -> Vec<&Pane<T>> {
        let mut out = Vec::new();
        self.root.collect(&mut out);
        out
    }

    /// The panes on screen: the zoomed one alone, else all of them.
    pub fn drawn(&self) -> Vec<&Pane<T>> {
        match self.zoomed {
            Some(z) => self.panes().into_iter().filter(|p| p.id == z).collect(),
            None => self.panes(),
        }
    }

    pub fn pane(&self, id: PaneId) -> Option<&Pane<T>> {
        self.panes().into_iter().find(|p| p.id == id)
    }

    pub fn pane_mut(&mut self, id: PaneId) -> Option<&mut Pane<T>> {
        self.root.pane_mut(id)
    }

    pub fn focused(&self) -> &Pane<T> {
        self.pane(self.focused).expect("the focused pane is in the tree")
    }

    /// The first tab `pick` accepts, as its pane and index.
    pub fn find(&self, mut pick: impl FnMut(&T) -> bool) -> Option<(PaneId, usize)> {
        self.panes().into_iter().find_map(|p| Some((p.id, p.tabs.iter().position(&mut pick)?)))
    }

    /// Focuses `pane`; a zoom on another pane ends, since that pane would hide this one.
    pub fn focus(&mut self, pane: PaneId) {
        if self.pane(pane).is_some() {
            if self.zoomed.is_some_and(|z| z != pane) {
                self.zoomed = None;
            }
            self.focused = pane;
        }
    }

    /// Shows tab `i` of `pane` and focuses the pane.
    pub fn select(&mut self, pane: PaneId, i: usize) {
        let Some(p) = self.pane_mut(pane) else { return };
        if i < p.tabs.len() {
            p.active = i;
            self.focus(pane);
        }
    }

    /// Adds `tab` at the end of `pane`, leaving what the pane shows alone; returns its index.
    pub fn push(&mut self, pane: PaneId, tab: T) -> Option<usize> {
        let p = self.pane_mut(pane)?;
        p.tabs.push(tab);
        Some(p.tabs.len() - 1)
    }

    /// Opens a pane holding `tab` at `edge` of `pane` and focuses it.
    pub fn split(&mut self, pane: PaneId, edge: Edge, tab: T) -> Option<PaneId> {
        let id = self.next;
        self.root.split(pane, edge, Node::Pane(Pane { id, tabs: vec![tab], active: 0 })).ok()?;
        self.next += 1;
        self.focused = id;
        self.zoomed = None;
        self.tidy();
        Some(id)
    }

    /// Takes tab `i` out of `pane`, keeping the shown tab shown; a pane left empty closes unless it is the last.
    pub fn take(&mut self, pane: PaneId, i: usize) -> Option<T> {
        let p = self.pane_mut(pane).filter(|p| i < p.tabs.len())?;
        let tab = p.tabs.remove(i);
        if p.active > i || p.active == p.tabs.len() {
            p.active = p.active.saturating_sub(1);
        }
        self.tidy();
        Some(tab)
    }

    /// Drops every tab `keep` refuses, keeping each pane's shown tab shown while it stays, and closes panes left empty.
    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        self.root.each_pane_mut(&mut |p| {
            let kept: Vec<bool> = p.tabs.iter().map(&mut keep).collect();
            let before = kept[..p.active.min(kept.len())].iter().filter(|k| !**k).count();
            let mut flags = kept.iter();
            p.tabs.retain(|_| *flags.next().expect("one flag per tab"));
            p.active = p.active.saturating_sub(before).min(p.tabs.len().saturating_sub(1));
        });
        self.tidy();
    }

    /// Each pane's rect within `bounds`, in reading order.
    pub fn layout(&self, bounds: Rect) -> Vec<(PaneId, Rect)> {
        let mut out = Vec::new();
        self.root.layout(bounds, &mut out);
        out
    }

    fn rect(&self, pane: PaneId, bounds: Rect) -> Option<Rect> {
        self.layout(bounds).into_iter().find(|(id, _)| *id == pane).map(|(_, r)| r)
    }

    /// Moves tab `i` of `from` to `to`. Within its own strip the shown tab stays shown; anywhere else the moved tab is shown and its pane focused.
    pub fn move_tab(&mut self, from: PaneId, i: usize, to: Target) {
        let Some(len) = self.pane(from).map(|p| p.tabs.len()).filter(|&n| i < n) else { return };
        match to {
            Target::Into { pane, index } if pane == from => {
                let p = self.pane_mut(from).expect("checked above");
                let to = index.min(len - 1);
                let tab = p.tabs.remove(i);
                p.tabs.insert(to, tab);
                p.active = match p.active {
                    a if a == i => to,
                    a if i < a && a <= to => a - 1,
                    a if to <= a && a < i => a + 1,
                    a => a,
                };
            }
            Target::Into { pane, index } => {
                if self.pane(pane).is_none() {
                    return;
                }
                let tab = self.take(from, i).expect("checked above");
                let p = self.pane_mut(pane).expect("checked above");
                let index = index.min(p.tabs.len());
                p.tabs.insert(index, tab);
                p.active = index;
                self.focused = pane;
            }
            Target::Split { pane, .. } if pane == from && len == 1 => {}
            Target::Split { pane, edge } => {
                if self.pane(pane).is_none() {
                    return;
                }
                let tab = self.take(from, i).expect("checked above");
                self.split(pane, edge, tab);
            }
        }
    }

    /// Whether `pane`, drawn within `bounds`, has room for two panes along `edge`'s axis.
    pub fn can_split(&self, pane: PaneId, edge: Edge, bounds: Rect) -> bool {
        let axis = edge.axis();
        self.rect(pane, bounds).is_some_and(|r| r.len(axis) >= 2. * axis.min())
    }

    /// The pane touching `edge` of `pane` most in line with it.
    pub fn neighbour(&self, pane: PaneId, edge: Edge) -> Option<PaneId> {
        let rects = self.layout(UNIT);
        let from = self.rect(pane, UNIT)?;
        let (cx, cy) = from.centre();
        let touches = |r: &Rect| match edge {
            Edge::Left => (r.x + r.w - from.x).abs() < EPS,
            Edge::Right => (from.x + from.w - r.x).abs() < EPS,
            Edge::Top => (r.y + r.h - from.y).abs() < EPS,
            Edge::Bottom => (from.y + from.h - r.y).abs() < EPS,
        };
        let across = |r: &Rect| match edge.axis() {
            Axis::Row => r.y < from.y + from.h - EPS && from.y < r.y + r.h - EPS,
            Axis::Column => r.x < from.x + from.w - EPS && from.x < r.x + r.w - EPS,
        };
        let off = |r: &Rect| match edge.axis() {
            Axis::Row => (r.centre().1 - cy).abs(),
            Axis::Column => (r.centre().0 - cx).abs(),
        };
        rects.iter().filter(|(id, r)| *id != pane && touches(r) && across(r)).min_by(|a, b| off(&a.1).total_cmp(&off(&b.1))).map(|(id, _)| *id)
    }

    /// The pane nearest `from` that `pick` accepts, `from` itself first.
    pub fn nearest(&self, from: PaneId, pick: impl Fn(&Pane<T>) -> bool) -> Option<PaneId> {
        let (fx, fy) = self.rect(from, UNIT)?.centre();
        let dist = |r: &Rect| {
            let (x, y) = r.centre();
            (x - fx).powi(2) + (y - fy).powi(2)
        };
        let rects = self.layout(UNIT);
        rects.iter().zip(self.panes()).filter(|(_, p)| pick(p)).min_by(|(a, _), (b, _)| dist(&a.1).total_cmp(&dist(&b.1))).map(|(_, p)| p.id)
    }

    /// Moves `d` to `at`, measured from the start of its split, which is `len` long; only the two children beside it change, and neither goes below its minimum.
    pub fn resize(&mut self, d: &Divider, at: f32, len: f32) {
        let Some(Node::Split { axis, sizes, children }) = self.root.at_mut(&d.path) else { return };
        if d.i + 1 >= sizes.len() || len <= 0. {
            return;
        }
        let start = sizes[..d.i].iter().sum::<f32>() * len;
        let pair = (sizes[d.i] + sizes[d.i + 1]) * len;
        let (lo, hi) = (children[d.i].min_len(*axis), children[d.i + 1].min_len(*axis));
        if lo + hi > pair {
            return;
        }
        let first = (at - start).clamp(lo, pair - hi);
        sizes[d.i] = first / len;
        sizes[d.i + 1] = (pair - first) / len;
    }

    /// Gives the children of the split at `path` equal shares.
    pub fn equalize(&mut self, path: &[usize]) {
        if let Some(Node::Split { sizes, .. }) = self.root.at_mut(path) {
            let n = sizes.len() as f32;
            sizes.iter_mut().for_each(|s| *s = 1. / n);
        }
    }

    pub fn equalize_all(&mut self) {
        self.root.equalize_all();
    }

    /// The tab beside the focused pane's shown one, after it or before it, wrapping around.
    pub fn step(&self, forward: bool) -> Option<(PaneId, usize)> {
        let p = self.focused();
        let n = p.tabs.len();
        (n > 0).then(|| (p.id, if forward { (p.active + 1) % n } else { (p.active + n - 1) % n }))
    }

    /// Each drawn pane's rect within `bounds`: the zoomed pane alone over all of it, else every pane's.
    pub fn drawn_layout(&self, bounds: Rect) -> Vec<(PaneId, Rect)> {
        match self.zoomed {
            Some(z) => vec![(z, bounds)],
            None => self.layout(bounds),
        }
    }

    /// Zooms the focused pane, or unzooms; a lone pane has nothing to zoom over.
    pub fn toggle_zoom(&mut self) {
        self.zoomed = match self.zoomed {
            None if self.panes().len() > 1 => Some(self.focused),
            _ => None,
        };
    }

    /// Restores the invariants after tabs left: no empty pane but a lone root, the focus on a pane that exists, the zoom on one too.
    fn tidy(&mut self) {
        let was = self.rect(self.focused, UNIT).map(|r| r.centre());
        let root = std::mem::replace(&mut self.root, Node::Split { axis: Axis::Row, sizes: Vec::new(), children: Vec::new() });
        self.root = root.prune().unwrap_or_else(|| Node::Pane(Pane { id: self.focused, tabs: Vec::new(), active: 0 }));
        if self.pane(self.focused).is_none() {
            let rects = self.layout(UNIT);
            self.focused = was.and_then(|c| rects.iter().find(|(_, r)| r.contains(c))).unwrap_or(&rects[0]).0;
        }
        if self.zoomed.is_some_and(|z| self.pane(z).is_none() || self.panes().len() < 2) {
            self.zoomed = None;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A tree holding `tabs` in its first pane.
    fn with(tabs: &[&'static str]) -> Tree<&'static str> {
        let mut t = Tree::default();
        tabs.iter().for_each(|tab| _ = t.push(0, *tab));
        t
    }

    fn tabs(t: &Tree<&'static str>) -> Vec<Vec<&'static str>> {
        t.panes().iter().map(|p| p.tabs.clone()).collect()
    }

    fn sizes(t: &Tree<&'static str>) -> Vec<f32> {
        match &t.root {
            Node::Split { sizes, .. } => sizes.clone(),
            Node::Pane(_) => vec![1.],
        }
    }

    const WINDOW: Rect = Rect { x: 0., y: 0., w: 1200., h: 800. };

    #[test]
    fn a_new_tree_is_one_empty_focused_pane() {
        let t: Tree<&str> = Tree::default();
        assert_eq!((t.panes().len(), t.focused().tabs.len(), t.focused), (1, 0, 0));
    }

    #[test]
    fn splitting_puts_the_new_pane_at_the_edge_and_focuses_it() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Left, "b").unwrap();
        assert_eq!((tabs(&t), t.focused), (vec![vec!["b"], vec!["a"]], b));
        let c = t.split(0, Edge::Bottom, "c").unwrap();
        assert_eq!((tabs(&t), t.focused), (vec![vec!["b"], vec!["a"], vec!["c"]], c));
        assert_eq!(t.layout(UNIT)[2].1, Rect { x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
    }

    #[test]
    fn splitting_along_the_parents_axis_shares_the_split_panes_space() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        t.split(b, Edge::Right, "c");
        assert_eq!((tabs(&t), sizes(&t)), (vec![vec!["a"], vec!["b"], vec!["c"]], vec![0.5, 0.25, 0.25]));
    }

    #[test]
    fn splitting_a_missing_pane_does_nothing() {
        let mut t = with(&["a"]);
        assert_eq!(t.split(9, Edge::Right, "b"), None);
        assert_eq!(tabs(&t), vec![vec!["a"]]);
    }

    #[test]
    fn taking_a_tab_keeps_the_shown_tab_shown() {
        let mut t = with(&["a", "b", "c"]);
        t.select(0, 2);
        assert_eq!(t.take(0, 0), Some("a"));
        assert_eq!(t.focused().active(), Some(&"c"));
        assert_eq!(t.take(0, 1), Some("c"));
        assert_eq!(t.focused().active(), Some(&"b"));
    }

    #[test]
    fn taking_a_panes_last_tab_closes_it_and_focuses_the_pane_that_took_its_place() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        let c = t.split(b, Edge::Bottom, "c").unwrap();
        t.take(c, 0);
        assert_eq!((tabs(&t), t.focused), (vec![vec!["a"], vec!["b"]], b));
        t.take(b, 0);
        assert_eq!((tabs(&t), t.focused), (vec![vec!["a"]], 0));
    }

    #[test]
    fn the_last_pane_stays_when_emptied() {
        let mut t = with(&["a"]);
        t.take(0, 0);
        assert_eq!((t.panes().len(), t.focused().tabs.len()), (1, 0));
    }

    #[test]
    fn a_split_left_with_one_child_gives_way_to_it() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        let c = t.split(b, Edge::Bottom, "c").unwrap();
        t.split(c, Edge::Right, "d");
        t.take(b, 0);
        assert_eq!(tabs(&t), vec![vec!["a"], vec!["c"], vec!["d"]]);
        assert!(matches!(&t.root, Node::Split { axis: Axis::Row, sizes, .. } if *sizes == vec![0.5, 0.25, 0.25]));
    }

    #[test]
    fn retaining_drops_refused_tabs_everywhere_and_closes_emptied_panes() {
        let mut t = with(&["a", "x", "b"]);
        t.select(0, 2);
        t.split(0, Edge::Right, "x");
        t.retain(|tab| *tab != "x");
        assert_eq!(tabs(&t), vec![vec!["a", "b"]]);
        assert_eq!(t.focused().active(), Some(&"b"));
    }

    #[test]
    fn moving_a_tab_within_its_strip_keeps_the_shown_tab_shown() {
        for (active, from, to) in [(0, 0, 2), (1, 0, 2), (1, 2, 0), (2, 0, 1), (0, 1, 2)] {
            let mut t = with(&["a", "b", "c"]);
            t.select(0, active);
            let shown = t.focused().active().copied();
            t.move_tab(0, from, Target::Into { pane: 0, index: to });
            assert_eq!(t.focused().active().copied(), shown, "showing {active}, moving {from} to {to}");
        }
    }

    #[test]
    fn a_moved_tab_takes_the_place_it_was_dropped_on() {
        let mut t = with(&["a", "b", "c"]);
        t.move_tab(0, 0, Target::Into { pane: 0, index: 2 });
        assert_eq!(tabs(&t), vec![vec!["b", "c", "a"]]);
        t.move_tab(0, 2, Target::Into { pane: 0, index: 9 });
        assert_eq!(tabs(&t), vec![vec!["b", "c", "a"]]);
    }

    #[test]
    fn a_tab_moved_into_another_pane_is_shown_there_and_focused() {
        let mut t = with(&["a", "b"]);
        let c = t.split(0, Edge::Right, "c").unwrap();
        t.focus(0);
        t.move_tab(0, 1, Target::Into { pane: c, index: 0 });
        assert_eq!(tabs(&t), vec![vec!["a"], vec!["b", "c"]]);
        assert_eq!((t.focused, t.focused().active()), (c, Some(&"b")));
    }

    #[test]
    fn moving_a_panes_only_tab_away_closes_the_pane() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        t.move_tab(b, 0, Target::Into { pane: 0, index: 9 });
        assert_eq!((tabs(&t), t.focused), (vec![vec!["a", "b"]], 0));
    }

    #[test]
    fn dropping_a_tab_on_an_edge_splits_that_pane() {
        let mut t = with(&["a", "b"]);
        t.move_tab(0, 1, Target::Split { pane: 0, edge: Edge::Bottom });
        assert_eq!(tabs(&t), vec![vec!["a"], vec!["b"]]);
        assert!(matches!(t.root, Node::Split { axis: Axis::Column, .. }));
        assert_eq!(t.focused().tabs, vec!["b"]);
    }

    #[test]
    fn a_pane_is_not_split_by_its_only_tab() {
        let mut t = with(&["a"]);
        t.move_tab(0, 0, Target::Split { pane: 0, edge: Edge::Right });
        assert_eq!(tabs(&t), vec![vec!["a"]]);
    }

    #[test]
    fn a_move_to_a_missing_pane_loses_nothing() {
        let mut t = with(&["a", "b"]);
        t.move_tab(0, 1, Target::Into { pane: 9, index: 0 });
        t.move_tab(0, 1, Target::Split { pane: 9, edge: Edge::Left });
        assert_eq!(tabs(&t), vec![vec!["a", "b"]]);
    }

    #[test]
    fn a_pane_too_small_to_halve_can_not_be_split() {
        let t = with(&["a"]);
        assert!(t.can_split(0, Edge::Right, WINDOW));
        assert!(!t.can_split(0, Edge::Right, Rect { w: 500., ..WINDOW }));
        assert!(!t.can_split(0, Edge::Bottom, Rect { h: 290., ..WINDOW }));
    }

    #[test]
    fn the_neighbour_is_the_touching_pane_most_in_line() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        let c = t.split(b, Edge::Bottom, "c").unwrap();
        assert_eq!(t.neighbour(0, Edge::Right), Some(b));
        assert_eq!(t.neighbour(c, Edge::Left), Some(0));
        assert_eq!(t.neighbour(b, Edge::Bottom), Some(c));
        assert_eq!(t.neighbour(0, Edge::Left), None);
    }

    #[test]
    fn the_nearest_accepted_pane_is_the_pane_itself_then_the_closest() {
        let mut t = with(&["doc"]);
        let b = t.split(0, Edge::Right, "term").unwrap();
        let c = t.split(b, Edge::Right, "doc").unwrap();
        let docs = |p: &Pane<&str>| p.tabs.contains(&"doc");
        assert_eq!((t.nearest(0, docs), t.nearest(b, docs)), (Some(0), Some(c)));
        assert_eq!(t.nearest(b, |p| p.tabs.contains(&"none")), None);
    }

    #[test]
    fn resizing_moves_only_the_two_panes_beside_the_divider() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        t.split(b, Edge::Right, "c");
        t.resize(&Divider { path: vec![], i: 1 }, 900., 1200.);
        assert_eq!(sizes(&t), vec![0.5, 0.25, 0.25]);
        t.resize(&Divider { path: vec![], i: 1 }, 880., 1200.);
        let s = sizes(&t);
        assert!((s[0] - 0.5).abs() < EPS && (s[1] - 280. / 1200.).abs() < EPS && (s[2] - 320. / 1200.).abs() < EPS, "{s:?}");
    }

    #[test]
    fn resizing_stops_where_a_pane_would_get_too_small() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        t.split(b, Edge::Bottom, "c");
        let d = Divider { path: vec![], i: 0 };
        t.resize(&d, 0., 1200.);
        assert_eq!(t.layout(WINDOW)[0].1.w, MIN_W);
        t.resize(&d, 1200., 1200.);
        assert_eq!(t.layout(WINDOW)[1].1.w, MIN_W);
    }

    #[test]
    fn a_split_too_small_for_its_panes_keeps_its_sizes() {
        let mut t = with(&["a"]);
        t.split(0, Edge::Right, "b");
        t.resize(&Divider { path: vec![], i: 0 }, 100., 400.);
        assert_eq!(sizes(&t), vec![0.5, 0.5]);
    }

    #[test]
    fn equalizing_gives_a_splits_children_equal_shares() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        t.split(b, Edge::Right, "c");
        t.equalize(&[]);
        assert!(sizes(&t).iter().all(|s| (s - 1. / 3.).abs() < EPS));
    }

    #[test]
    fn equalizing_all_reaches_nested_splits() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        t.split(b, Edge::Bottom, "c");
        t.resize(&Divider { path: vec![1], i: 0 }, 300., 800.);
        t.equalize_all();
        assert_eq!(t.layout(WINDOW)[1].1.h, 400.);
    }

    #[test]
    fn only_a_pane_among_others_zooms_and_closing_it_unzooms() {
        let mut t = with(&["a"]);
        t.toggle_zoom();
        assert_eq!(t.zoomed, None);
        let b = t.split(0, Edge::Right, "b").unwrap();
        t.toggle_zoom();
        assert_eq!(t.zoomed, Some(b));
        t.take(b, 0);
        assert_eq!(t.zoomed, None);
    }

    #[test]
    fn a_rebuilt_tree_numbers_new_panes_after_its_own() {
        let mut saved = with(&["a"]);
        let b = saved.split(0, Edge::Right, "b").unwrap();
        let mut t = Tree::from_root(saved.root, b);
        assert_eq!(t.focused, b);
        assert_eq!(t.split(b, Edge::Bottom, "c"), Some(b + 1));
    }

    #[test]
    fn a_rebuilt_tree_focuses_its_first_pane_when_the_saved_focus_is_gone() {
        let mut saved = with(&["a"]);
        saved.split(0, Edge::Right, "b");
        assert_eq!(Tree::from_root(saved.root, 9).focused, 0);
    }

    #[test]
    fn a_rebuilt_tree_closes_its_empty_panes() {
        let mut saved = with(&["a"]);
        let b = saved.split(0, Edge::Right, "b").unwrap();
        saved.pane_mut(b).unwrap().tabs.clear();
        let t = Tree::from_root(saved.root, b);
        assert_eq!((tabs(&t), t.focused), (vec![vec!["a"]], 0));
    }

    #[test]
    fn a_rebuilt_tree_with_no_tabs_is_one_empty_pane_numbered_below_new_ones() {
        let mut t: Tree<&str> = Tree::from_root(Node::Pane(Pane { id: 4, tabs: Vec::new(), active: 0 }), 7);
        assert_eq!((t.panes().len(), t.focused().tabs.len()), (1, 0));
        t.push(t.focused, "a");
        t.split(t.focused, Edge::Right, "b");
        let ids: Vec<PaneId> = t.panes().iter().map(|p| p.id).collect();
        assert_ne!(ids[0], ids[1]);
    }

    #[test]
    fn a_tree_reads_back_as_written_unzoomed() {
        let mut t = with(&["a", "b"]);
        let c = t.split(0, Edge::Bottom, "c").unwrap();
        t.toggle_zoom();
        let back: Tree<String> = serde_json::from_str(&serde_json::to_string(&t).unwrap()).unwrap();
        assert_eq!((back.panes().iter().map(|p| p.tabs.clone()).collect::<Vec<_>>(), back.focused, back.zoomed), (vec![vec!["a".to_string(), "b".into()], vec!["c".into()]], c, None));
    }

    fn pane(id: PaneId, tabs: &[&'static str]) -> Node<&'static str> {
        Node::Pane(Pane { id, tabs: tabs.to_vec(), active: 0 })
    }

    #[test]
    fn a_rebuilt_tree_shares_space_equally_when_its_saved_sizes_do_not_fit() {
        for bad in [vec![1.], vec![0.5, 0.25, 0.25], vec![f32::NAN, 0.5], vec![f32::INFINITY, 0.5], vec![0., 1.], vec![-0.5, 1.5], vec![f32::MAX, f32::MAX]] {
            let root = Node::Split { axis: Axis::Row, sizes: bad.clone(), children: vec![pane(0, &["a"]), pane(1, &["b"])] };
            let t = Tree::from_root(root, 0);
            assert_eq!((tabs(&t), sizes(&t)), (vec![vec!["a"], vec!["b"]], vec![0.5, 0.5]), "saved sizes {bad:?}");
        }
    }

    #[test]
    fn a_rebuilt_tree_keeps_each_active_tab_in_range() {
        let t = Tree::from_root(Node::Pane(Pane { id: 0, tabs: vec!["a", "b"], active: 5 }), 0);
        assert_eq!(t.focused().active(), Some(&"b"));
    }

    #[test]
    fn a_rebuilt_tree_renumbers_repeated_panes() {
        let root = Node::Split { axis: Axis::Row, sizes: vec![0.25, 0.25, 0.5], children: vec![pane(3, &["a"]), pane(3, &["b"]), pane(1, &["c"])] };
        let mut t = Tree::from_root(root, 3);
        assert_eq!(t.focused().tabs, vec!["a"]);
        let d = t.split(t.focused, Edge::Bottom, "d").unwrap();
        let mut ids: Vec<PaneId> = t.panes().iter().map(|p| p.id).collect();
        ids.sort();
        ids.dedup();
        assert_eq!((ids.len(), t.pane(d).map(|p| p.tabs.clone())), (4, Some(vec!["d"])));
    }

    #[test]
    fn a_rebuilt_tree_takes_the_largest_pane_ids() {
        let root = Node::Split { axis: Axis::Row, sizes: vec![0.5, 0.5], children: vec![pane(PaneId::MAX - 1, &["a"]), pane(PaneId::MAX, &["b"])] };
        let mut t = Tree::from_root(root, PaneId::MAX);
        assert_eq!(t.focused().tabs, vec!["b"]);
        assert!(t.split(t.focused, Edge::Right, "c").is_some());
    }

    #[test]
    fn stepping_wraps_around_the_focused_panes_tabs() {
        let mut t = with(&["a", "b", "c"]);
        assert_eq!((t.step(false), t.step(true)), (Some((0, 2)), Some((0, 1))));
        t.select(0, 2);
        assert_eq!(t.step(true), Some((0, 0)));
    }

    #[test]
    fn stepping_only_moves_within_the_focused_pane() {
        let mut t = with(&["a", "b"]);
        let c = t.split(0, Edge::Right, "c").unwrap();
        assert_eq!(t.step(true), Some((c, 0)));
    }

    #[test]
    fn stepping_an_empty_pane_shows_nothing() {
        let t: Tree<&str> = Tree::default();
        assert_eq!(t.step(true), None);
    }

    #[test]
    fn focusing_another_pane_ends_the_zoom() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        t.toggle_zoom();
        t.focus(b);
        assert_eq!(t.zoomed, Some(b));
        t.focus(0);
        assert_eq!(t.zoomed, None);
        t.toggle_zoom();
        t.select(b, 0);
        assert_eq!(t.zoomed, None);
    }

    #[test]
    fn a_zoomed_tree_draws_only_its_zoomed_pane() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        assert_eq!(t.drawn().len(), 2);
        t.toggle_zoom();
        assert_eq!(t.drawn().iter().map(|p| p.id).collect::<Vec<_>>(), vec![b]);
    }

    #[test]
    fn a_zoomed_pane_is_laid_over_the_whole_area() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        assert_eq!(t.drawn_layout(WINDOW), t.layout(WINDOW));
        t.toggle_zoom();
        assert_eq!(t.drawn_layout(WINDOW), vec![(b, WINDOW)]);
    }
}
