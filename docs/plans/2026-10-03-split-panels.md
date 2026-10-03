# Split Panels Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** The main area becomes a tree of resizable panels, each with its own tab strip. Tabs drag between strips, and onto a panel's edge to split it. The layout survives restarts per worktree.

**Architecture:** A generic pane tree, `workspace::tree::Tree<T>`, holds splits, sizes, focus and zoom as plain data with no GPUI. `Workspace` becomes a `Tree<Tab>` plus the preview doc, with one terminal per tab. Doc views (diff, file, commit) and browser address bars move from singletons to per-pane state keyed by `PaneId`, so two panels can show two docs. `pocket/src/panels.rs` renders the tree recursively (strips, dividers, bodies) and owns the drag state. The top row of strips doubles as the title bar.

**Toolset** (run everything from `/Users/mingo/Developer/self/coding-pocket/packages/desktop`):
- One workspace test: `cargo test -p workspace <test_name>`
- One pocket test: `cargo test -p pocket <test_name>`
- One store test: `cargo test -p store <test_name>`
- Gate before calling a PR done: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
- Clippy baseline (must not grow): 5× "a `Vec` of `Range` that is only one element", 2× "field assignment outside of initializer", 1× "`Iterator::last` on a `DoubleEndedIterator`", plus the `block v0.1.6` future-incompat note.
- Screens: `.ui-review/fixture/capture.sh <dir> <name>=<step>,<step>…` (from the repo root).
- Judge speed in release: `cargo run --release -p pocket`.
- Do NOT run `cargo fmt`. There is no rustfmt.toml, and lines run to ~200 chars by hand. Do not commit.

**Read first:**
- `CLAUDE.md` (repo root): layout rules, performance rules (1000+ changed files is the normal case; every notify redraws the window) and the pre-commit gate.
- `docs/adr/0003-desktop-code-layout.md`: model crates hold logic and `pocket` holds views; one state struct per feature; tests named as sentences at the bottom of the file.
- `.ui-review/prototypes/panels.html`: the agreed look. The strips are the title bar, split/⋯/ring sit at each strip's end, drop zones highlight, and an empty panel shows a CTA.
- `packages/desktop/crates/workspace/src/workspace.rs`: today's flat tab list, which this plan replaces.
- `packages/desktop/crates/pocket/src/terminal_view/tabs.rs`: today's strip and the `TabDrag` slide animation, which move into `panels/strip.rs`.

Rules for every task: no comments that restate code (only `///` docs on items, and comments for a non-obvious WHY). Match the surrounding style and touch nothing else.

**Shipping order.** It differs from the grill's phases:
- PR 1 (the pane tree, pure logic) and PR 2 (header B) are small and independent of each other.
- PR 3 (per-pane doc state) is a refactor with no behaviour change. It must land before PR 4, which switches the UI to panels in one go.
- PR 5 adds drag-and-drop, PR 6 persistence and PR 7 shortcuts.
- Header B ships before the panels because once there are many strips, no single bar is left to hold the diff totals and the error.

**Assumptions to confirm in review:**
- Doc state is per *pane*, not per tab. A pane shows one doc at a time, and switching tabs within a pane reloads that doc, as it does today.
- There is one preview tab per worktree. An unpinned open replaces it only when it sits in the pane the doc opens into. Otherwise the doc opens as a new preview tab in that pane, and the old one becomes a normal tab.

---

## PR 1: A pane tree in the `workspace` crate

**Scope:** `crates/workspace/src/tree.rs`: `Tree<T>` with panes, splits, focus, moves, layout, neighbours, resizing and zoom. It is pure data with no GPUI. Nothing calls it yet, so it stays inert until PR 4.
**Depends on:** nothing
**Done when:** `cargo test -p workspace tree::` runs 25 passing tests, and the gate is green with the clippy baseline unchanged.

### Task 1.1: Panes, splits and taking tabs out

**Files:**
- Create: `packages/desktop/crates/workspace/src/tree.rs`
- Modify: `packages/desktop/crates/workspace/src/workspace.rs:1` (add `pub mod tree;` and a blank line before the first line)

**Context:** A worktree's main area is a tree.
- **Leaves** are `Pane`s: an id, a strip of tabs, and the shown tab.
- **Inner nodes** are `Split`s. They lay their children side by side (`Axis::Row`) or stacked (`Axis::Column`). Each child has a share of the length, and the `sizes` sum to 1.
- **The tab type is generic**, so here the tree is tested with `&str` tabs. From PR 4 it holds `workspace::Tab`.

After every removal, the private `tidy` restores these invariants:
- No pane is empty unless it is the lone root.
- A split never has one child; it gives way to that child.
- A split never directly holds a split along the same axis. The inner split is merged in and its sizes scaled.
- `focused` always names a pane in the tree. When the focused pane closes, focus goes to the pane now covering its old centre.
- `zoomed` names a pane in a tree of 2+ panes, or is `None`.

`Tree::layout(bounds)` gives each pane's rect. `tidy` runs it on `UNIT` (the 1×1 square) to find the old centre.

**Step 1: Write the failing tests**

Create `packages/desktop/crates/workspace/src/tree.rs` containing only the test module below, and add `pub mod tree;` to `workspace.rs`. The helpers build a tree of `&str` tabs, and each test's name says what it proves.

```rust
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
}
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p workspace tree::`
Expected: FAIL to compile with "cannot find type `Tree`" (and similar for `Node`, `Edge`, `Pane`).

**Step 3: Write the implementation**

Put this above `#[cfg(test)]` in `tree.rs`:

```rust
pub type PaneId = u64;

/// `Row` lays children side by side, `Column` one under another.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Axis {
    Row,
    Column,
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

#[derive(Clone, Debug, PartialEq)]
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
#[derive(Clone, Debug, PartialEq)]
pub enum Node<T> {
    Pane(Pane<T>),
    Split { axis: Axis, sizes: Vec<f32>, children: Vec<Node<T>> },
}

#[derive(Clone, Copy, Debug, PartialEq, Default)]
pub struct Rect {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
}

pub const UNIT: Rect = Rect { x: 0., y: 0., w: 1., h: 1. };

impl Rect {
    fn centre(&self) -> (f32, f32) {
        (self.x + self.w / 2., self.y + self.h / 2.)
    }

    fn contains(&self, (x, y): (f32, f32)) -> bool {
        self.x <= x && x <= self.x + self.w && self.y <= y && y <= self.y + self.h
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
#[derive(Clone, Debug, PartialEq)]
pub struct Tree<T> {
    pub root: Node<T>,
    /// Always a pane in `root`.
    pub focused: PaneId,
    /// The pane drawn alone over the rest, while there are others.
    pub zoomed: Option<PaneId>,
    next: PaneId,
}

impl<T> Default for Tree<T> {
    fn default() -> Self {
        Self { root: Node::Pane(Pane { id: 0, tabs: Vec::new(), active: 0 }), focused: 0, zoomed: None, next: 1 }
    }
}

impl<T> Tree<T> {
    /// In reading order: left to right, then top to bottom within each split.
    pub fn panes(&self) -> Vec<&Pane<T>> {
        let mut out = Vec::new();
        self.root.collect(&mut out);
        out
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

    pub fn focus(&mut self, pane: PaneId) {
        if self.pane(pane).is_some() {
            self.focused = pane;
        }
    }

    /// Shows tab `i` of `pane` and focuses the pane.
    pub fn select(&mut self, pane: PaneId, i: usize) {
        let Some(p) = self.pane_mut(pane) else { return };
        if i < p.tabs.len() {
            p.active = i;
            self.focused = pane;
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
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p workspace tree::`
Expected: PASS with `test result: ok. 9 passed`. `cargo clippy -p workspace --all-targets` prints no warning.

### Task 1.2: Moving a tab within its strip, into another pane, or onto an edge

**Files:**
- Modify: `packages/desktop/crates/workspace/src/tree.rs`

**Context:** `move_tab(from, i, Target)` is the single entry point that drag-and-drop (PR 5) calls. Its rules:
- **Within its own strip**, the shown tab stays shown. Today's `Workspace::move_tab` follows the same rule; its tests `moving_tabs_keeps_the_shown_tab_shown` and `a_moved_tab_takes_the_place_it_was_dropped_on` are the model.
- **Into another pane**, the moved tab is shown and that pane is focused.
- **A pane's only tab can't split that pane**, because nothing would be left behind.
- **A target pane that doesn't exist** leaves everything untouched. Never take the tab out before checking that the target exists.

**Step 1: Write the failing tests**

Append inside `mod tests`:

```rust
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
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p workspace tree::`
Expected: FAIL to compile with "use of undeclared type `Target`" and "no method named `move_tab`".

**Step 3: Write the implementation**

At the top level, next to the other types:

```rust
/// Where a moved tab goes: into a pane's strip at `index`, or into a new pane at `edge` of a pane.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Target {
    Into { pane: PaneId, index: usize },
    Split { pane: PaneId, edge: Edge },
}
```

Inside `impl<T> Tree<T>`, before `fn tidy`:

```rust
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
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p workspace tree::`
Expected: PASS with `test result: ok. 16 passed`.

### Task 1.3: Room to split, neighbours and the nearest matching pane

**Files:**
- Modify: `packages/desktop/crates/workspace/src/tree.rs`

**Context:** No pane may be drawn smaller than `MIN_W`×`MIN_H` (260×150 px).
- **`can_split`** says whether a pane, drawn in the given window bounds, has room for two. PR 4 uses it to disable the split buttons and PR 5 to hide the edge zones.
- **`neighbour`** serves ⌘⌥+arrow (PR 7). It returns the pane across `edge` that touches and overlaps this one; the most in line wins.
- **`nearest`** decides where things open: docs go to the nearest pane holding a doc, terminals to the nearest pane holding a terminal (PR 4). Distance is measured between centres on the `UNIT` layout. Ties go to reading order, because `min_by` keeps the first.
- **`EPS`** absorbs float error when comparing edges.

**Step 1: Write the failing tests**

Add this constant next to the helpers, after `fn sizes`:

```rust
    const WINDOW: Rect = Rect { x: 0., y: 0., w: 1200., h: 800. };
```

Then append inside `mod tests`:

```rust
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
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p workspace tree::`
Expected: FAIL to compile with "no method named `can_split`" (and `neighbour`, `nearest`).

**Step 3: Write the implementation**

At the top level, next to the other types:

```rust
/// The smallest a pane may be drawn; a split or resize that would go below it doesn't happen.
pub const MIN_W: f32 = 260.;

pub const MIN_H: f32 = 150.;

const EPS: f32 = 1e-4;
```

A new `impl Axis` block right after `enum Axis`:

```rust
impl Axis {
    fn min(self) -> f32 {
        match self {
            Axis::Row => MIN_W,
            Axis::Column => MIN_H,
        }
    }
}
```

Inside `impl Rect`:

```rust
    fn len(&self, axis: Axis) -> f32 {
        match axis {
            Axis::Row => self.w,
            Axis::Column => self.h,
        }
    }
```

Inside `impl<T> Tree<T>`, before `fn tidy`:

```rust
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
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p workspace tree::`
Expected: PASS with `test result: ok. 19 passed`.

### Task 1.4: Resizing, equalizing and zoom

**Files:**
- Modify: `packages/desktop/crates/workspace/src/tree.rs`

**Context:**
- **A `Divider`** is addressed by `path`, the child indices from the root down to its split, and `i`, the child it follows.
- **`resize(d, at, len)`** takes two pixel values from the view in PR 4: `at`, the pointer's offset from the split's start, and `len`, the split's length. It moves only the two children beside the divider, and clamps so neither goes below its minimum length.
- **`min_len`** is a node's minimum length. For a pane it is the pane minimum. Along the split's own axis, a split needs the sum of its children's minimums; across it, the largest of them.
- **When the pair can't fit both minimums** (the window is too small), the sizes stay as they are.
- **`equalize(path)`** serves a double-click on a divider, and `equalize_all` the ⌘⌃= shortcut.
- **`toggle_zoom`** serves ⌘⇧↩ (PR 7). It only does something among 2+ panes, and `tidy` already clears the zoom when the zoomed pane closes.

**Step 1: Write the failing tests**

Append inside `mod tests`:

```rust
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
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p workspace tree::`
Expected: FAIL to compile with "cannot find struct `Divider`" and "no method named `resize`" (and `equalize`, `equalize_all`, `toggle_zoom`).

**Step 3: Write the implementation**

At the top level, next to the other types:

```rust
/// The line between child `i` and `i + 1` of the split reached from the root by `path`.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Divider {
    pub path: Vec<usize>,
    pub i: usize,
}
```

Inside `impl<T> Node<T>`:

```rust
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
```

Inside `impl<T> Tree<T>`, before `fn tidy`:

```rust
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

    /// Zooms the focused pane, or unzooms; a lone pane has nothing to zoom over.
    pub fn toggle_zoom(&mut self) {
        self.zoomed = match self.zoomed {
            None if self.panes().len() > 1 => Some(self.focused),
            _ => None,
        };
    }
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p workspace tree::`
Expected: PASS with `test result: ok. 25 passed`. Then run the gate, `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`: it is green, with clippy at the baseline.

## PR 2: Move diff totals and the error out of the session bar

**Scope:**
- The `bar-diff` totals leave the session bar. The sidebar's Changes tab already carries them; it changes to the "Changes" label plus a totals pill, with counts past a thousand shortened so 1000+ file repos still fit.
- When the sidebar is hidden (Compact rail, Focus toggle), its toggle gets an accent dot while the worktree has changes.
- The bar's red error text becomes a dismissable toast at the bottom-right of the main area.
- The context ring stays in the bar; PR 4 moves it into each strip.
- Nothing about panels lands here.

**Depends on:** nothing.

**Done when:**
- The gate is green with no new clippy warnings.
- The session bar shows tabs, ring, split buttons and ⋯ only.
- Totals show on the Changes tab, and a dot shows on the hidden-sidebar toggle.
- An error shows as a toast, and × dismisses it.

Before Task 2.1, capture the "before" screens from the repo root (`/Users/mingo/Developer/self/coding-pocket`):

```sh
.ui-review/fixture/capture.sh /tmp/pr2-before session=session compact=session,focus focus-mode=session,focus,focus
```

Expected: three paths printed, `/tmp/pr2-before/impl-session.png`, `impl-compact.png` and `impl-focus-mode.png`.

### Task 2.1: Totals pill on the Changes tab

**Files:**
- Modify: `packages/desktop/crates/pocket/src/sidebar/column.rs:1-8` (imports), `:20-38` (tab row), append functions and tests at the end of the file (after line 67).

**Context:**
- `column_view` already swaps the Changes tab's label for `ui::meta_diff(+N −M)` while the worktree has changed files (`column.rs:20,35-38`).
- Variant B of `.ui-review/prototypes/panels.html` (`renderSessions`, CSS `.sbadge`) keeps the "Changes" label and puts the totals in a pill:
  - 17px high, 5px horizontal padding, 8px radius;
  - `HAIRLINE` fill, semibold, mono 11px;
  - 6px gap after the label.
- Each tab is about 103px wide (334px column, 8px padding, three tabs). Treat repos with 1000+ changed files as the normal case: `+40213 −30987` would overflow, so counts past 999 shorten (`1.2k`, `12k`).
- The badge shows whenever the worktree has a changed file, even if every change is binary (+0 −0). That matches today's `!r.files.is_empty()` rule.
- The logic is free functions over `git::Repo`, so tests need no `Desktop`.
- `changes_dot` is used by Task 2.2.

**Step 1: Write the failing tests**

Append to the end of `column.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::{changes_badge, short};
    use git::{FileStat, Repo};

    fn changed(added: usize, removed: usize) -> FileStat {
        FileStat { path: "a.rs".into(), added, removed, staged: false, unstaged: true, status: 'M' }
    }

    #[test]
    fn a_clean_worktree_has_no_changes_badge() {
        assert_eq!(changes_badge(Some(&Repo::default())), None);
        assert_eq!(changes_badge(None), None);
    }

    #[test]
    fn the_changes_badge_totals_every_changed_file() {
        let repo = Repo { files: vec![changed(3, 1), changed(4, 0)], ..Default::default() };
        assert_eq!(changes_badge(Some(&repo)), Some(("7".into(), "1".into())));
    }

    #[test]
    fn binary_changes_alone_still_badge_the_worktree() {
        let repo = Repo { files: vec![changed(0, 0)], ..Default::default() };
        assert_eq!(changes_badge(Some(&repo)), Some(("0".into(), "0".into())));
    }

    #[test]
    fn counts_past_a_thousand_are_shortened_to_fit_the_toggle() {
        assert_eq!([987, 1_000, 1_234, 9_960, 12_345].map(short), ["987", "1k", "1.2k", "10k", "12k"]);
    }
}
```

What each test proves:
- A clean worktree, or none, shows no badge.
- The badge sums across files.
- A binary-only change still marks the worktree.
- Large counts fit in a tab.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p pocket changes_badge counts_past_a_thousand`

Expected: FAIL to compile with `unresolved imports super::changes_badge, super::short`.

(Cargo takes one filter; if it rejects two, run `cargo test -p pocket column::tests`.)

**Step 3: Write the implementation**

Replace `column.rs:1-8` with:

```rust
use crate::desktop::Desktop;
use crate::desktop::chrome::{Column, Overlay, Screen, Side, column, drag_area};
use crate::util::basename;
use git::Repo;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use theme::*;
use ui::{self, icon_button_sized};

/// The Changes toggle's added and removed counts, while the worktree has any changed file.
pub(crate) fn changes_badge(repo: Option<&Repo>) -> Option<(String, String)> {
    let (added, removed) = repo.filter(|r| !r.files.is_empty())?.totals();
    Some((short(added), short(removed)))
}

fn short(n: usize) -> String {
    match n {
        ..1_000 => n.to_string(),
        ..10_000 => format!("{:.1}k", n as f32 / 1000.).replace(".0k", "k"),
        _ => format!("{}k", n / 1000),
    }
}

/// Marks a sidebar toggle while the hidden Changes list has something in it.
pub(crate) fn changes_dot() -> Div {
    ui::dot(6., ACCENT).absolute().top(px(5.)).right(px(5.)).shadow(vec![ui::ring(Token::new(0xfafafaff, 0x171717ff), 1.5)])
}
```

Replace `column.rs:20-38`, from the `let totals = …` line through the `.map(|d| match (side, totals) { … })` block, with:

```rust
        let totals = changes_badge(self.repo());
        let tabs = [(Side::Sessions, "Sessions"), (Side::Explorer, "Explorer"), (Side::Changes, "Changes")].into_iter().enumerate().map(|(i, (side, label))| {
            let selected = self.side == side;
            let badge = totals.clone().filter(|_| side == Side::Changes).map(|(added, removed)| {
                div()
                    .h(px(17.))
                    .px(px(5.))
                    .flex()
                    .items_center()
                    .gap(px(6.6))
                    .rounded(px(8.))
                    .bg(HAIRLINE)
                    .font_family(MONO)
                    .font_weight(FontWeight::SEMIBOLD)
                    .text_size(px(11.))
                    .child(div().text_color(SUCCESS_TEXT).child(format!("+{added}")))
                    .child(div().text_color(FAILED).child(format!("−{removed}")))
            });
            div()
                .id(("column-tab", i))
                .flex_1()
                .h(px(30.))
                .flex()
                .items_center()
                .justify_center()
                .gap(px(6.))
                .rounded(px(8.))
                .cursor_pointer()
                .whitespace_nowrap()
                .text_size(px(13.))
                .when(selected, |d| d.bg(FILL_4).text_color(TEXT).font_weight(FontWeight::SEMIBOLD))
                .when(!selected, |d| d.text_color(TEXT_2).font_weight(FontWeight::MEDIUM).hover(|s| s.bg(FILL_2)))
                .child(label)
                .children(badge)
```

The `.on_click(…)` that follows stays as it is.

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p pocket column::tests`

Expected: PASS, 4 tests.

### Task 2.2: Dot on the hidden-sidebar toggles; drop `bar-diff`

**Files:**
- Modify: `packages/desktop/crates/pocket/src/desktop/chrome.rs:1-7` (imports), `:215-225` (`bar_start`).
- Modify: `packages/desktop/crates/pocket/src/sidebar/rail.rs:1-9` (imports), `:23-36` (`nav-panel` toggle).
- Modify: `packages/desktop/crates/pocket/src/terminal_view.rs:292-305`, `:333` (`session_page`).

**Context:**
- In `Layout::Compact` the Changes tab lives behind the rail's `nav-panel` toggle. In `Layout::Focus` it lives behind the `sidebar-expand` toggle that `bar_start` puts at the left of every top bar.
- Variant B (`.tdot` in the prototype) puts a 6px accent dot on those toggles while there are changes, so the totals the session bar used to show are still hinted.
- Clicking either toggle already opens the sidebar.
- `open_changes` keeps its other callers (`explorer/preview/header.rs:89`, `git_ui/changes/rows.rs:197`, `git_ui/changes/notes.rs:29`, `capture.rs:30`), so nothing is orphaned.
- This task is view-only; Task 2.1's tests cover the logic.

**Step 1: Add the dot to the Focus toggle**

In `chrome.rs`, add an import below line 4 (`use crate::terminals::close::Busy;`):

```rust
use crate::sidebar::column::{changes_badge, changes_dot};
```

Replace `chrome.rs:220-224` (the `match self.layout { … }` in `bar_start`) with:

```rust
        match self.layout {
            // Leaves room for the window's traffic lights once the sidebars are hidden.
            Layout::Focus => (91., Some(toggle("sidebar-expand", cx).relative().when(changes_badge(self.repo()).is_some(), |d| d.child(changes_dot())))),
            Layout::Compact | Layout::Sidebars => (24., None),
        }
```

`chrome.rs` has no `FluentBuilder` import, so also add after `use gpui_kit::*;` (line 5):

```rust
use gpui_kit::prelude::FluentBuilder as _;
```

**Step 2: Add the dot to the rail toggle**

In `rail.rs`, add an import after line 2:

```rust
use crate::sidebar::column::{changes_badge, changes_dot};
```

In the `toggle` builder (`rail.rs:23-36`), insert two lines:
- `.relative()` after `.id("nav-panel")`;
- `.when(changes_badge(self.repo()).is_some(), |d| d.child(changes_dot()))` after `.child(icon("sidebar", 18., TEXT_2))`.

The result reads:

```rust
        let toggle = div()
            .id("nav-panel")
            .relative()
            .w(px(36.))
            .h(px(32.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(8.))
            .cursor_pointer()
            .when(self.panel, |d| d.bg(FILL_3))
            .hover(|s| s.bg(FILL_3))
            .child(icon("sidebar", 18., TEXT_2))
            .when(changes_badge(self.repo()).is_some(), |d| d.child(changes_dot()))
            .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.toggle_rail(&crate::actions::ToggleRail, window, cx)));
```

**Step 3: Drop `bar-diff` from the session bar**

In `terminal_view.rs`:
- Delete lines 293-302: the `let (added, removed) = …` line and the whole `let diff = (added + removed > 0).then(|| { … });` block.
- Delete the `.children(diff)` line (line 333 before the deletions).

Leave the `error` line (303) for Task 2.3.

**Step 4: Build and run the related tests**

Run: `cargo build -p pocket && cargo test -p pocket column::tests`

Expected: builds with no new warnings; PASS, 4 tests.

### Task 2.3: The error as a toast

**Files:**
- Create: `packages/desktop/crates/pocket/src/desktop/toast.rs`
- Modify: `packages/desktop/crates/pocket/src/desktop.rs:1-7` (module list), `:334-343` (`main_view`).
- Modify: `packages/desktop/crates/pocket/src/terminal_view.rs` (`session_page`: the `let error = …` and `let status = …` lines).
- Modify: `packages/desktop/crates/pocket/src/capture.rs:16`, `:75-80` (an `error` step for screens).

**Context:**
- `Desktop.error: Option<String>` (`desktop.rs:72`) is set by five places:
  - pocketd `"error"` (`terminals.rs:202`);
  - a failed page open (`browser.rs:135`);
  - a failed save (`explorer/preview.rs:305`);
  - a worktree delete (`desktop/project.rs:232`);
  - an agents config failure (`desktop/alerts.rs:119`).
- It is cleared on a successful spawn (`terminals.rs:194,243`).
- Today only `session_page` draws it, as truncated red text in the bar, so it never shows on the blank, link or inbox pages.
- The toast sits in `main_view`, so it shows over every main page: bottom-right, 16px inset, 300px wide.
- Prototype `#toast`: an 18px `FAILED` circle with a white `x-bold`, the message, and a 20px `x` dismiss.
- Dismiss sets `error = None`. A new error replaces the text in place, because the field is replaced.
- Use `ui::pop` (opaque `POPOVER`), not `ui::glass`. GPUI has no backdrop blur (see the comment in `sidebar/panel.rs`), so translucent glass over terminal text would be unreadable.
- `occlude` keeps clicks on the toast from reaching the terminal below.
- A shown web page is a native WKWebView above GPUI, so it hides the toast while the browser tab is shown.
- `observe_banner` (`chrome.rs:116`) is a full-width in-flow strip, so it doesn't fit a floating toast. Only its tokens are reused.
- This task is view-only; there is no unit test. Task 2.4's capture checks it.

**Step 1: Write the toast**

Create `packages/desktop/crates/pocket/src/desktop/toast.rs`:

```rust
use crate::desktop::Desktop;
use gpui_kit::*;
use theme::*;
use ui;

impl Desktop {
    /// The last error, floated over the main area's bottom-right corner until dismissed or replaced.
    pub(crate) fn error_toast(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let error = self.error.clone()?;
        let mark = div().mt(px(1.)).size(px(18.)).flex().flex_none().items_center().justify_center().rounded(px(9.)).bg(FAILED).child(icon("x-bold", 9., WHITE));
        let dismiss = div()
            .id("error-toast-dismiss")
            .size(px(20.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(5.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_3))
            .child(icon("x", 12., TEXT_3))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                this.error = None;
                cx.notify();
            }));
        let toast = ui::pop(div())
            .id("error-toast")
            .absolute()
            .right(px(16.))
            .bottom(px(16.))
            .w(px(300.))
            .pl(px(12.))
            .pr(px(10.))
            .py(px(11.))
            .flex()
            .items_start()
            .gap(px(10.))
            .rounded(px(14.))
            .occlude()
            .child(mark)
            .child(div().flex_1().min_w_0().text_size(px(13.)).text_color(TEXT).child(error))
            .child(dismiss);
        Some(toast.with_animation("error-toast-in", Animation::new(MENU_IN).with_easing(ease_out_quint()), |d, t| d.opacity(t)))
    }
}
```

In `desktop.rs`, add after `pub(crate) mod sounds;` (line 7):

```rust
pub(crate) mod toast;
```

Replace the last line of `main_view` (`desktop.rs:342`) with:

```rust
        ui::page(div()).relative().flex_1().min_w_0().h_full().flex().flex_col().overflow_hidden().child(body).children(self.error_toast(cx))
```

**Step 2: Drop the error from the session bar**

In `terminal_view.rs` `session_page`:
- Delete the line `let error = self.error.clone().map(|e| div().min_w_0().truncate().mr(px(6.)).text_size(px(12.5)).text_color(FAILED_TEXT).child(e));`.
- Replace the `let status = …` line with:

```rust
        let status = div().ml_auto().pl(px(8.)).min_w_0().flex().items_center().children(ring);
```

**Step 3: Add a capture step that raises an error**

In `capture.rs`, change line 16 to `const STEPS: [(&str, Step); 24] = [`. After the `("fail", …)` entry (it ends at line 79, just before `];`), add:

```rust
    ("error", |d, _, _| d.error = Some("Couldn't save placeholder.tsx: Permission denied (os error 13)".into())),
```

**Step 4: Build**

Run: `cargo build -p pocket && cargo build -p pocket --features capture`

Expected: both build with no new warnings.

### Task 2.4: Gate and screens

**Files:** none changed.

**Context:**
- The "before" screens were captured before Task 2.1.
- Compare by eye. Before → after:
  - `session`: the bar loses `+N −M` (and any red text); the Changes tab reads "Changes" plus a pill.
  - `compact`: the rail's sidebar toggle has an accent dot.
  - `focus-mode`: the `sidebar-expand` toggle at the top-left has an accent dot.
  - `error` (after only): the toast sits at the bottom-right of the main area.

**Step 1: Run the gate**

Run, from `packages/desktop`:

```sh
cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace
```

Expected:
- Everything passes.
- Clippy shows only the baseline: 5× "a `Vec` of `Range` that is only one element", 2× "field assignment outside of initializer", 1× "`Iterator::last` on a `DoubleEndedIterator`", plus the `block v0.1.6` future-incompat note.

**Step 2: Capture the "after" screens**

Run, from the repo root:

```sh
.ui-review/fixture/capture.sh /tmp/pr2-after session=session compact=session,focus focus-mode=session,focus,focus error=session,error
```

Expected: four PNG paths printed under `/tmp/pr2-after/`. Compare each with its `/tmp/pr2-before/` twin, as described in Context.

## PR 3: Keep doc and browser state per pane

**Scope:** Each per-pane state is keyed by `workspace::tree::PaneId`:
- `DiffState` keeps the split toggle, comment input, target, comments and viewed set shared, and holds each pane's diff in `panes: HashMap<PaneId, DiffView>`.
- `PreviewState` keeps `drafts` and `path_copied` shared, and holds each pane's file in `panes: HashMap<PaneId, FilePane>`.
- The commit state becomes `Commits` (one `CommitState` per pane).
- `Browsers` gets one address bar per pane and a set of shown tabs.

The doc API takes a pane: `show_doc(pane, …)`, `loaded(pane, …)`, `diff_view` / `file_view` / `commit_view(pane, cx)`, `browser_view(pane, id, …)`, `load_diff` / `load_file` / `load_commit(pane, …)`. `refresh_git` updates every pane. Page actions go to the focused pane. Every per-pane state gets `retain_panes(&[PaneId])`.

This is a refactor only. There is one pane, `MAIN = 0`, and the three seams `focused_pane`, `pane_ids` and `pane_tab` (which PR 4 rewrites) only ever return it.

**Depends on:** PR 1 (`workspace::tree::PaneId`, declared by `pub mod tree;` in `workspace.rs`)

**Done when:** the gate passes, `cargo test -p pocket` shows 333 tests (323 + 10), and the before/after screens match.

Before Task 3.1, capture the screens at HEAD. From the repo root:

Run: `.ui-review/fixture/capture.sh /tmp/split-pr3-before changes=session,changes comment=session,changes,comment file=session,explore,file graph-commit=session,changes,graph-expand,graph-commit browser=session,browser`

### Task 3.1: One pane, and a commit per pane

**Files:**
- Modify: `packages/desktop/crates/pocket/src/desktop.rs:20` (import), `:39` (import), `:41` (const before the struct), `:64` (field), `:147` (init), `:244`, `:251-257` (`active_doc`), `:264`, `:279-313` (`show_doc`, `loaded`, `load_active`)
- Modify: `packages/desktop/crates/pocket/src/git_ui/commit.rs:8-10` (imports), `:102-227` (`impl Desktop`), `:231` (test imports), end of `mod tests`
- Modify: `packages/desktop/crates/pocket/src/terminal_view.rs:12`, `:338-339`
- Modify: `packages/desktop/crates/pocket/src/capture.rs:159`

**Context:**
- There is still one pane. `MAIN` is its id.
- Three seams stand for what PR 4 builds: `focused_pane()` (where keys and actions go), `pane_ids()` (the panes on screen) and `pane_tab(pane)` (the tab a pane shows).
- `CommitState` (35–100) is left as is. A new `Commits` wraps one `CommitState` per pane. `select` returns `None` when the pane already shows that sha, which replaces the early return in `show_commit`.
- `show_doc` and `loaded` take the pane now. Only their commit arms use it; Tasks 3.2 and 3.3 convert the diff and file arms.

**Step 1: Write the failing tests**

In `commit.rs`, change the test imports at line 231:

```rust
    use super::{CommitState, Commits, FileDiff, Row};
    use crate::desktop::MAIN;
```

Append inside `mod tests`:

```rust
    #[test]
    fn showing_a_commit_in_one_pane_leaves_another_alone() {
        let mut commits = Commits::default();
        let run = commits.select(MAIN, "a".into()).unwrap();
        commits.panes.get_mut(&MAIN).unwrap().set_files(run, vec![file("x")]);
        assert!(commits.select(1, "b".into()).is_some());
        assert!(commits.select(MAIN, "a".into()).is_none());
        assert_eq!(commits.get(MAIN).unwrap().rows, [Row::Header(0)]);
        commits.retain_panes(&[MAIN]);
        assert!(commits.get(1).is_none());
    }
```

**Step 2: Run to verify it fails**

Run: `cargo test -p pocket git_ui::commit::`
Expected: FAIL to compile — no `Commits` in `super`, no `MAIN` in `crate::desktop`.

**Step 3: Implement**

**`desktop.rs` imports.** Line 20 becomes `use crate::git_ui::commit::Commits;`. Add `use workspace::tree::PaneId;` before line 39 (`use workspace::{Doc, Tab, Workspace};`).

**`MAIN`.** Add this just above `pub struct Desktop` (line 41):

```rust
/// The one pane every tab shows in.
pub(crate) const MAIN: PaneId = 0;
```

**Field and init.** Line 64 becomes `pub(crate) commit: Commits,` and line 147 becomes `commit: Commits::default(),`.

**Callers of `show_doc`.** Line 244 becomes `Some(Tab::Doc(doc)) => self.show_doc(self.focused_pane(), doc, cx),` and line 264 becomes `self.show_doc(self.focused_pane(), doc, cx);`.

**Seams.** Replace `active_doc` (251–257) with:

```rust
    /// The pane keys and actions go to.
    pub(crate) fn focused_pane(&self) -> PaneId {
        MAIN
    }

    pub(crate) fn pane_ids(&self) -> Vec<PaneId> {
        vec![MAIN]
    }

    /// The tab `pane` shows in the worktree on screen.
    pub(crate) fn pane_tab(&self, pane: PaneId) -> Option<Tab> {
        self.workspaces.get(&self.cwd()?)?.active().filter(|_| pane == MAIN).cloned()
    }

    /// The file or changes the focused pane shows.
    pub fn active_doc(&self) -> Option<Doc> {
        match self.pane_tab(self.focused_pane())? {
            Tab::Doc(doc) => Some(doc),
            Tab::Term(_) | Tab::Web(_) => None,
        }
    }
```

**`show_doc`, `loaded`, `load_active`.** Replace 279–313 with:

```rust
    fn show_doc(&mut self, pane: PaneId, doc: Doc, cx: &mut Context<Self>) {
        match doc {
            Doc::File(path) => {
                self.preview.open(path);
                self.load_file(cx);
            }
            Doc::Diff(path) => {
                self.diff.select(path, None);
                self.load_diff(cx);
            }
            Doc::CommitFile { sha, path } => {
                self.diff.select(path, Some(sha));
                self.load_diff(cx);
            }
            Doc::Commit(sha) => self.show_commit(pane, sha, cx),
        }
        cx.notify();
    }

    pub fn loaded(&self, pane: PaneId, doc: &Doc) -> bool {
        match doc {
            Doc::File(p) => self.preview.file.as_ref() == Some(p),
            Doc::Diff(p) => self.diff.file.as_ref() == Some(p) && self.diff.at.is_none(),
            Doc::CommitFile { sha, path } => self.diff.file.as_ref() == Some(path) && self.diff.at.as_ref() == Some(sha),
            Doc::Commit(sha) => self.commit.get(pane).is_some_and(|c| c.sha.as_ref() == Some(sha)),
        }
    }

    /// Drops what closed panes showed, and loads the docs that tabs revealed by closing or switching worktrees show, unless they are loaded already.
    pub(crate) fn load_active(&mut self, cx: &mut Context<Self>) {
        let panes = self.pane_ids();
        self.commit.retain_panes(&panes);
        for pane in panes {
            if let Some(Tab::Doc(doc)) = self.pane_tab(pane)
                && !self.loaded(pane, &doc)
            {
                self.show_doc(pane, doc, cx);
            }
        }
    }
```

**`commit.rs` imports.** Line 8 becomes `use std::collections::{HashMap, HashSet};`. Add `use workspace::tree::PaneId;` after `use workspace::Doc;`.

**`Commits`.** Insert above `impl Desktop` (line 102):

```rust
/// The commit each pane shows.
#[derive(Default)]
pub struct Commits {
    pub(crate) panes: HashMap<PaneId, CommitState>,
}

impl Commits {
    pub fn get(&self, pane: PaneId) -> Option<&CommitState> {
        self.panes.get(&pane)
    }

    /// Shows commit `sha` in `pane`; the run tags the reads for it, none when the pane shows it already.
    pub fn select(&mut self, pane: PaneId, sha: String) -> Option<u64> {
        let c = self.panes.entry(pane).or_default();
        (c.sha.as_ref() != Some(&sha)).then(|| c.select(sha))
    }

    pub fn retain_panes(&mut self, panes: &[PaneId]) {
        self.panes.retain(|p, _| panes.contains(p));
    }
}
```

**`commit.rs` `impl Desktop`.** Replace `show_commit`, `recolor_commit` and the head of `load_commit` (103–120) with:

```rust
    pub(crate) fn show_commit(&mut self, pane: PaneId, sha: String, cx: &mut Context<Self>) {
        if let Some(run) = self.commit.select(pane, sha) {
            self.load_commit(pane, run, cx);
        }
    }

    /// Colours every pane's commit again, for a new appearance.
    pub(crate) fn recolor_commit(&mut self, cx: &mut Context<Self>) {
        let shown: Vec<(PaneId, u64)> = self.commit.panes.iter_mut().filter(|(_, c)| c.sha.is_some()).map(|(p, c)| (*p, c.restart())).collect();
        for (pane, run) in shown {
            self.load_commit(pane, run, cx);
        }
    }

    fn load_commit(&mut self, pane: PaneId, run: u64, cx: &mut Context<Self>) {
        let Some((cwd, sha)) = self.cwd().zip(self.commit.get(pane).and_then(|c| c.sha.clone())) else { return };
```

In the rest of `load_commit`:
- Line 135 becomes `let ok = d.commit.panes.get_mut(&pane).is_some_and(|c| c.set_files(run, files.clone()));`.
- Line 149 becomes `let ok = d.commit.panes.get_mut(&pane).is_some_and(|c| c.set_diffs(run, start, diffs));`.

**`commit_view`.** Its first lines (163–167) become:

```rust
    pub fn commit_view(&mut self, pane: PaneId, cx: &mut Context<Self>) -> Div {
        let Some((sha, n, state)) = self.commit.get(pane).and_then(|c| Some((c.sha.clone()?, c.files.len(), c.list.clone()))) else { return empty("No commit.") };
        let meta = vec![ui::meta_item().child(ui::meta_value(format!("{n} file{}", if n == 1 { "" } else { "s" }))).into_any_element()];
        let crumbs = std::iter::once(git::short_sha(&sha).to_string()).chain(self.graph.commit(&sha).map(|c| c.subject.clone())).collect();
        let rows = list(state, cx.processor(move |this, ix, _, cx| this.commit_diff_row(pane, ix, cx))).pb(px(8.));
```

**`commit_diff_row` and `commit_file_header`.** Their heads (184–188 and 198–201) become:

```rust
    fn commit_diff_row(&self, pane: PaneId, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some(c) = self.commit.get(pane) else { return Empty.into_any_element() };
        match c.rows.get(ix).copied() {
            Some(Row::Header(f)) => self.commit_file_header(c, f, cx).into_any_element(),
            Some(Row::Code(f, j)) => {
                let d = &c.diffs[f];
```

```rust
    fn commit_file_header(&self, c: &CommitState, f: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let file = &c.files[f];
        let (dir, name) = file.path.rsplit_once('/').unwrap_or(("", &file.path));
        let doc = c.sha.clone().map(|sha| Doc::CommitFile { sha, path: file.path.clone() });
```

**`terminal_view.rs`.** Line 12 becomes `use crate::desktop::{Desktop, MAIN};`. Lines 338–339 become:

```rust
            Some(Tab::Doc(doc @ (Doc::Diff(_) | Doc::CommitFile { .. }))) if self.loaded(MAIN, &doc) => self.diff_view(cx),
            Some(Tab::Doc(doc @ Doc::Commit(_))) if self.loaded(MAIN, &doc) => self.commit_view(MAIN, cx),
```

**`capture.rs`.** Line 159 (`d.commit.sha = None;`) becomes:

```rust
    for c in d.commit.panes.values_mut() {
        c.sha = None;
    }
```

**Step 4: Run to verify it passes**

Run: `cargo test -p pocket git_ui::commit::`
Expected: PASS, 4 tests.

### Task 3.2: A diff per pane

**Files:**
- Modify: `packages/desktop/crates/pocket/src/git_ui/diff.rs:11` (import), `:19-21` (imports), `:288-396` (`DiffState` and its impl), `:398-609` (`impl Desktop`), `:612-616` (test imports), end of `mod tests`
- Modify: `packages/desktop/crates/pocket/src/git_ui/diff/row.rs:1-8`, `:97-144`
- Modify: `packages/desktop/crates/pocket/src/git_ui/diff/composer.rs:11`
- Modify: `packages/desktop/crates/pocket/src/git_ui/changes/notes.rs:40-41`
- Modify: `packages/desktop/crates/pocket/src/git_ui/changes/rows.rs:118`
- Modify: `packages/desktop/crates/pocket/src/git_ui/graph/row.rs:156`
- Modify: `packages/desktop/crates/pocket/src/desktop.rs` (the `Doc::Diff` and `Doc::CommitFile` arms of `show_doc` and `loaded`; `set_project` line 221)
- Modify: `packages/desktop/crates/pocket/src/terminal_view.rs:338`
- Modify: `packages/desktop/crates/pocket/src/capture.rs:4`, `:52-53`, `:157-158`
- Modify: `packages/desktop/crates/pocket/src/desktop/project.rs:100-102`, `:113`, `:124-127`, `:139-141`

**Context:**
- **What each pane owns.** A pane's `DiffView` holds the file it shows, its lines, rows, list, highlights, folds and pick.
- **What stays shared.** The split toggle, comment input, agent target, sent comments and viewed set stay on `DiffState`.
- **One comment at a time.** Only one comment is written at a time. `drafting` names the pane it belongs to, and starting a pick in another pane (`draft_in`) drops the old pane's pick.
- **Re-laying out after a change.** A sent or resolved comment, or the split toggle, re-lays out every pane (`relayout`).
- **Tests without GPUI.** `DiffState` is generic over its input (`I = Entity<TextareaState>`), so the tests use `DiffState::with(())`. `ListState::new` works without a window.
- **Pane-free callers.** `cancel_comment`, `submit_comment` and `composer` take no pane; they use `draft()`.

**Step 1: Write the failing tests**

In `diff.rs` `mod tests`, the imports (612–616) become:

```rust
    use super::{Comment, DiffLoad, DiffState, Pick, Row, changed, comment_target, highlights, label, notes, remap, rows};
    use crate::desktop::MAIN;
    use git::parse;
    use gpui_kit::HighlightStyle;
    use std::collections::HashSet;
    use theme::{DIFF_ADD_WORD, DIFF_DEL_WORD, SYN_FN, SYN_KEYWORD, SYN_STRING, Token};
    use workspace::tree::PaneId;
```

After `const TWO_HUNKS…` add `const OTHER: PaneId = 1;`. Then append at the end of `mod tests`:

```rust
    fn load(path: &str, text: &str) -> DiffLoad {
        DiffLoad { path: path.into(), at: None, open: HashSet::new(), lines: parse(text), source: Default::default(), colors: None, dark: false }
    }

    fn showing(panes: &[(PaneId, &str)]) -> DiffState<()> {
        let mut state = DiffState::with(());
        for &(pane, path) in panes {
            state.select(pane, path.into(), None);
            state.apply(pane, load(path, DIFF));
        }
        state
    }

    #[test]
    fn a_diff_load_lands_only_in_a_pane_still_showing_its_file() {
        let mut state = showing(&[(MAIN, "a.rs")]);
        state.select(OTHER, "b.rs".into(), None);
        assert!(!state.apply(OTHER, load("a.rs", DIFF)));
        assert!(state.apply(OTHER, load("b.rs", DIFF)));
        assert_eq!(state.view(MAIN).unwrap().lines, parse(DIFF));
        assert!(state.view(OTHER).unwrap().shows("b.rs", None));
    }

    #[test]
    fn starting_a_comment_in_one_pane_drops_the_pick_in_another() {
        let mut state = showing(&[(MAIN, "a.rs"), (OTHER, "a.rs")]);
        state.draft_in(MAIN);
        state.panes.get_mut(&MAIN).unwrap().pick.open(&parse(DIFF), 2);
        state.layout(MAIN, false);
        assert!(state.view(MAIN).unwrap().rows.contains(&Row::Composer));
        state.draft_in(OTHER);
        assert_eq!(state.view(MAIN).unwrap().pick, Pick::default());
        assert!(!state.view(MAIN).unwrap().rows.contains(&Row::Composer));
        assert_eq!(state.drafting, OTHER);
    }

    #[test]
    fn switching_to_split_lays_out_every_pane() {
        let mut state = showing(&[(MAIN, "a.rs"), (OTHER, "b.rs")]);
        state.split = true;
        state.relayout(true);
        assert!(state.panes.values().all(|v| v.rows.iter().all(|r| matches!(r, Row::Split(..)))));
    }

    #[test]
    fn a_sent_comment_shows_in_every_pane_showing_its_file() {
        let mut state = showing(&[(MAIN, "a.rs"), (OTHER, "a.rs"), (2, "b.rs")]);
        state.add_comment(sent("a.rs", (3, 3), false));
        assert!(state.view(MAIN).unwrap().rows.contains(&Row::Comment(0)));
        assert!(state.view(OTHER).unwrap().rows.contains(&Row::Comment(0)));
        assert!(!state.view(2).unwrap().rows.contains(&Row::Comment(0)));
    }

    #[test]
    fn a_closed_panes_diff_is_dropped() {
        let mut state = showing(&[(MAIN, "a.rs"), (OTHER, "b.rs")]);
        state.retain_panes(&[MAIN]);
        assert!(state.view(OTHER).is_none() && state.view(MAIN).is_some());
    }
```

**Step 2: Run to verify it fails**

Run: `cargo test -p pocket git_ui::diff::`
Expected: FAIL to compile — `DiffState` takes no type argument, and has no `with`, `view`, `draft_in`, `relayout`, `add_comment`, `retain_panes` or `panes`.

**Step 3: Implement**

**`diff.rs` imports.**
- Line 11 becomes `use crate::desktop::{Desktop, MAIN};`.
- Line 19 becomes `use std::collections::{HashMap, HashSet};`.
- Add `use workspace::tree::PaneId;` after `use workspace::Doc;`.

**`DiffView` and `DiffState`.** Replace 288–396 (`DiffState` and its impl) with:

```rust
/// One pane's diff: the file it shows, its lines, and the lines picked there for a comment.
pub struct DiffView {
    pub(crate) file: Option<String>,
    pub(crate) at: Option<String>,
    pub(crate) lines: Vec<git::Line>,
    pub(crate) rows: Vec<Row>,
    pub(crate) list: ListState,
    pub(crate) hl: Vec<Spans>,
    pub(crate) open: HashSet<usize>,
    pub(crate) source: (String, String),
    pub(crate) syntax: (Vec<Spans>, Vec<Spans>),
    pub(crate) pick: Pick,
}

impl Default for DiffView {
    fn default() -> Self {
        Self {
            file: None,
            at: None,
            lines: Vec::new(),
            rows: Vec::new(),
            list: ListState::new(0, ListAlignment::Top, px(400.)),
            hl: Vec::new(),
            open: HashSet::new(),
            source: Default::default(),
            syntax: Default::default(),
            pick: Pick::default(),
        }
    }
}

impl DiffView {
    /// Whether this shows `path` in the working tree, or with `at` in that commit.
    pub fn shows(&self, path: &str, at: Option<&str>) -> bool {
        self.file.as_deref() == Some(path) && self.at.as_deref() == at
    }

    pub fn working_file(&self) -> Option<&str> {
        self.file.as_deref().filter(|_| self.at.is_none())
    }

    fn set_lines(&mut self, lines: Vec<Line>) -> bool {
        if lines == self.lines {
            return false;
        }
        self.pick.remap(&self.lines, &lines);
        self.lines = lines;
        true
    }

    /// Rebuilds the rows; without `reset` only the rows that changed are remeasured and the scroll position stays.
    fn layout(&mut self, reset: bool, split: bool, comments: &[Comment]) {
        let notes = notes(comments, self.working_file(), &self.lines);
        let rows = rows(&self.lines, split, self.pick.composer_line(), ¬es);
        if reset {
            self.list.reset(rows.len());
        } else {
            let (range, count) = changed(&self.rows, &rows);
            self.list.splice(range, count);
        }
        self.rows = rows;
    }
}

/// The diff each pane shows, and the comments, one of them being written, shared by all. The input is `I` so the rules test without GPUI.
pub struct DiffState<I = Entity<TextareaState>> {
    pub(crate) panes: HashMap<PaneId, DiffView>,
    pub(crate) split: bool,
    pub(crate) input: I,
    /// The pane whose pick the comment being written is about.
    pub(crate) drafting: PaneId,
    pub(crate) target: Option<String>,
    pub(crate) target_menu: bool,
    pub(crate) comments: Vec<Comment>,
    pub(crate) viewed: HashSet<String>,
}

impl DiffState {
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> (Self, Vec<Subscription>) {
        let input = cx.new(|cx| TextareaState::new(window, cx).placeholder("Ask the agent about these lines…").rows(3));
        let subs = vec![cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, window, cx| match ev {
            InputEvent::PressEnter { secondary: true, .. } => this.submit_comment(window, cx),
            InputEvent::Change => cx.notify(),
            _ => {}
        })];
        (Self::with(input), subs)
    }
}

impl<I> DiffState<I> {
    pub fn with(input: I) -> Self {
        Self { panes: HashMap::new(), split: false, input, drafting: MAIN, target: None, target_menu: false, comments: Vec::new(), viewed: HashSet::new() }
    }

    pub fn view(&self, pane: PaneId) -> Option<&DiffView> {
        self.panes.get(&pane)
    }

    /// The pane the comment is being written in.
    pub fn draft(&self) -> Option<&DiffView> {
        self.panes.get(&self.drafting)
    }

    /// Shows `load` in `pane` unless another file, fold state or appearance was picked there while it ran.
    pub fn apply(&mut self, pane: PaneId, load: DiffLoad) -> bool {
        let Some(v) = self.panes.get_mut(&pane) else { return false };
        if v.file.as_ref() != Some(&load.path) || v.at != load.at || load.open != v.open || load.colors.is_some() && load.dark != theme::is_dark() {
            return false;
        }
        v.source = load.source;
        let recolored = load.colors.map(|(syntax, hl)| (v.syntax, v.hl) = (syntax, hl)).is_some();
        let changed = v.set_lines(load.lines);
        if changed {
            v.layout(true, self.split, &self.comments);
        }
        changed | recolored
    }

    /// Shows `path` in `pane`, in the working tree or with `at` in that commit, dropping the pick, folds and lines of another file.
    pub fn select(&mut self, pane: PaneId, path: String, at: Option<String>) {
        let v = self.panes.entry(pane).or_default();
        if v.shows(&path, at.as_deref()) {
            return;
        }
        v.pick.range = None;
        v.open.clear();
        if v.set_lines(Vec::new()) {
            v.layout(true, self.split, &self.comments);
        }
        (v.file, v.at) = (Some(path), at);
    }

    /// Shows the unchanged lines folded away from `start` on in `pane`, keeping the scroll position.
    fn expand(&mut self, pane: PaneId, start: usize) {
        let Some(v) = self.panes.get_mut(&pane) else { return };
        v.open.insert(start);
        let lines = git::diff_texts(&v.source.0, &v.source.1, &v.open);
        v.hl = highlights(&lines, &v.syntax.0, &v.syntax.1);
        if v.set_lines(lines) {
            v.layout(false, self.split, &self.comments);
        }
    }

    pub fn layout(&mut self, pane: PaneId, reset: bool) {
        if let Some(v) = self.panes.get_mut(&pane) {
            v.layout(reset, self.split, &self.comments);
        }
    }

    pub fn relayout(&mut self, reset: bool) {
        for v in self.panes.values_mut() {
            v.layout(reset, self.split, &self.comments);
        }
    }

    /// Moves the comment being written to `pane`, dropping the pick another pane held.
    pub fn draft_in(&mut self, pane: PaneId) {
        self.drafting = pane;
        for (_, v) in self.panes.iter_mut().filter(|(p, v)| **p != pane && v.pick != Pick::default()) {
            v.pick = Pick::default();
            v.layout(false, self.split, &self.comments);
        }
    }

    pub fn cancel_draft(&mut self) {
        if let Some(v) = self.panes.get_mut(&self.drafting) {
            v.pick.cancel();
            v.layout(false, self.split, &self.comments);
        }
    }

    pub fn add_comment(&mut self, comment: Comment) {
        self.comments.push(comment);
        self.relayout(false);
    }

    fn resolve_comment(&mut self, i: usize) -> bool {
        let found = i < self.comments.len();
        if found {
            self.comments.remove(i);
            self.relayout(false);
        }
        found
    }

    pub fn retain_panes(&mut self, panes: &[PaneId]) {
        self.panes.retain(|p, _| panes.contains(p));
    }
}
```

**`diff.rs` `impl Desktop` (398–609).** `resolve_comment` and `comment_target` are unchanged; everything else changes as follows.

`diff_view` (399–403):

```rust
    pub fn diff_view(&mut self, pane: PaneId, cx: &mut Context<Self>) -> Div {
        let Some((path, at)) = self.diff.view(pane).and_then(|v| Some((v.file.clone()?, v.at.clone()))) else {
            return empty("No changes.");
        };
```

In the split toggle (418), `this.diff.layout(true);` becomes `this.diff.relayout(true);`. The last child (475) becomes `.child(self.diff_box(pane, cx))`.

`diff_box` (478–493) and `expand` (495–498):

```rust
    pub fn diff_box(&mut self, pane: PaneId, cx: &mut Context<Self>) -> Div {
        let Some(state) = self.diff.view(pane).map(|v| v.list.clone()) else { return div().flex_1() };
        let rows = list(state, cx.processor(move |this, ix, _, cx| this.diff_row(pane, ix, cx))).pb(px(8.));
        div()
            .flex_1()
            .min_h_0()
            .bg(PAGE)
            .border_t(px(0.5))
            .border_color(SEPARATOR)
            .overflow_hidden()
            .font_family(MONO)
            .text_size(px(12.5))
            .line_height(px(ROW))
            .child(rows.size_full())
            .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| this.end_drag(pane, window, cx)))
            .on_mouse_up_out(MouseButton::Left, cx.listener(move |this, _, window, cx| this.end_drag(pane, window, cx)))
    }

    fn expand(&mut self, pane: PaneId, start: usize, cx: &mut Context<Self>) {
        self.diff.expand(pane, start);
        cx.notify();
    }
```

In `open_changes`, line 507 becomes:

```rust
        let shown = self.diff.view(self.focused_pane()).and_then(|v| v.working_file()).map(str::to_string);
        let path = path.or(shown).or_else(|| self.repo()?.files.first().map(|f| f.path.clone()));
```

`load_diff` through the head of `read_diff_in_background` (515–527):

```rust
    pub(crate) fn load_diff(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        let shown = self.diff.view(pane).map(|v| v.lines.clone()).unwrap_or_default();
        self.read_diff_in_background(pane, shown, cx);
    }

    /// Colours every pane's diff again, for a new appearance.
    pub(crate) fn recolor_diff(&mut self, cx: &mut Context<Self>) {
        for pane in self.diff.panes.keys().copied().collect::<Vec<_>>() {
            self.read_diff_in_background(pane, Vec::new(), cx);
        }
    }

    fn read_diff_in_background(&mut self, pane: PaneId, shown: Vec<Line>, cx: &mut Context<Self>) {
        let Some((cwd, (path, at, open))) = self.cwd().zip(self.diff.view(pane).and_then(|v| Some((v.file.clone()?, v.at.clone(), v.open.clone())))) else { return };
```

In the same function, `if d.diff.apply(load) {` becomes `if d.diff.apply(pane, load) {`.

`select_line` through `cancel_comment` (541–586):

```rust
    /// Starts a comment on line `i` of `pane`'s diff, or with `extend` stretches the open one to it.
    pub fn select_line(&mut self, pane: PaneId, i: usize, extend: bool, window: &mut Window, cx: &mut Context<Self>) {
        self.diff.draft_in(pane);
        let Some(v) = self.diff.panes.get_mut(&pane) else { return };
        if v.pick.press(&v.lines, i, extend) {
            self.diff.input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.diff.layout(pane, false);
        cx.notify();
    }

    pub fn drag_to(&mut self, pane: PaneId, i: usize, pressed: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(v) = self.diff.panes.get_mut(&pane) else { return };
        if !pressed && v.pick.held() {
            return self.end_drag(pane, window, cx);
        }
        if v.pick.drag(&v.lines, i) {
            cx.notify();
        }
    }

    pub fn end_drag(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        if !self.diff.panes.get_mut(&pane).is_some_and(|v| v.pick.release()) {
            return;
        }
        self.diff.layout(pane, false);
        if self.diff.view(pane).is_some_and(|v| v.pick.composing) {
            self.diff.input.update(cx, |s, cx| s.focus(window, cx));
        }
        cx.notify();
    }

    pub fn open_comment(&mut self, pane: PaneId, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        self.diff.draft_in(pane);
        let Some(v) = self.diff.panes.get_mut(&pane) else { return };
        if v.pick.open(&v.lines, i) {
            self.diff.input.update(cx, |s, cx| s.set_value("", window, cx));
        }
        self.diff.layout(pane, false);
        self.diff.input.update(cx, |s, cx| s.focus(window, cx));
        cx.notify();
    }

    pub fn cancel_comment(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.diff.cancel_draft();
        self.diff.target_menu = false;
        self.diff.input.update(cx, |s, cx| s.set_value("", window, cx));
        window.focus(&self.root, cx);
        cx.notify();
    }
```

In `submit_comment`, line 593 becomes:

```rust
        let Some(v) = self.diff.draft() else { return };
        let (Some(target), Some(path), Some(lines)) = (self.comment_target(), v.file.clone(), v.pick.label(&v.lines)) else { return };
```

Lines 599–601 of `submit_comment` become:

```rust
        if let Some(comment) = self.diff.draft().and_then(|v| v.pick.comment(&v.lines, path, lines, text, now_ms())) {
            self.diff.add_comment(comment);
        }
```

**`diff/row.rs`.** Add `use workspace::tree::PaneId;` after `use theme::*;`. Replace 97–144 (`cell`, `fold`, the head of `diff_row`) with the code below. `cell` and `fold` look the view up themselves, because passing it too would push `cell` past clippy's 7-argument limit.

```rust
    /// One side of a row: pressing picks its line, dragging or shift-clicking stretches the pick; "+" or a drag opens the composer.
    fn cell(&self, pane: PaneId, id: &'static str, i: usize, numbers: Vec<Option<usize>>, add_at: f32, cx: &mut Context<Self>) -> Stateful<Div> {
        let v = &self.diff.panes[&pane];
        if v.lines[i].kind == Kind::Hunk {
            return self.fold(pane, id, i, cx);
        }
        let row = code(&v.lines[i], v.hl.get(i), numbers, v.pick.picked(&v.lines, i)).id((id, i));
        if v.at.is_some() {
            return row;
        }
        let last = v.pick.last() == Some(i);
        row.group("diff-line")
            .cursor_pointer()
            .child(
                add_button(add_at)
                    .when(!last, |b| b.opacity(0.).group_hover("diff-line", |s| s.opacity(1.)))
                    .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                    .on_mouse_up(MouseButton::Left, cx.listener(move |this, _, window, cx| this.open_comment(pane, i, window, cx))),
            )
            .on_mouse_down(MouseButton::Left, cx.listener(move |this, ev: &MouseDownEvent, window, cx| this.select_line(pane, i, ev.modifiers.shift, window, cx)))
            .on_mouse_move(cx.listener(move |this, ev: &MouseMoveEvent, window, cx| this.drag_to(pane, i, ev.dragging(), window, cx)))
    }

    /// A hunk header; clicking it unfolds the lines hidden above it.
    fn fold(&self, pane: PaneId, id: &'static str, i: usize, cx: &mut Context<Self>) -> Stateful<Div> {
        let v = &self.diff.panes[&pane];
        let row = hunk(&v.lines, i).id((id, i));
        match fold_start(&v.lines, i) {
            Some(start) => row.cursor_pointer().hover(|s| s.bg(FILL_2)).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.expand(pane, start, cx))),
            None => row,
        }
    }

    pub(super) fn diff_row(&self, pane: PaneId, ix: usize, cx: &mut Context<Self>) -> AnyElement {
        let Some((v, &row)) = self.diff.view(pane).and_then(|v| Some((v, v.rows.get(ix)?))) else { return Empty.into_any_element() };
        match row {
            Row::Unified(i) => self.cell(pane, "line", i, vec![v.lines[i].old, v.lines[i].new], 2. * NUM - 10., cx).w_full().into_any_element(),
            Row::Split(Some(i), _) | Row::Split(None, Some(i)) if v.lines[i].kind == Kind::Hunk => self.fold(pane, "fold", i, cx).w_full().into_any_element(),
            Row::Split(l, r) => {
                let mut side = |id, i: Option<usize>, n: fn(&Line) -> Option<usize>| match i {
                    Some(i) => self.cell(pane, id, i, vec![n(&v.lines[i])], NUM - 10., cx).flex_1().min_w_0().into_any_element(),
```

**Other call sites.**
- `diff/composer.rs:11` becomes `let lines = self.diff.draft().and_then(|v| v.pick.label(&v.lines)).unwrap_or_default();`.
- `changes/notes.rs:40-41` becomes:

  ```rust
        if let Some(v) = self.diff.draft()
            && let (true, false, Some(path), Some((lo, hi, _))) = (v.pick.composing, draft.is_empty(), v.file.clone(), v.pick.range.and_then(|s| span(&v.lines, ordered(s))))
  ```

- `changes/rows.rs:118` becomes `let selected = self.diff.view(self.focused_pane()).is_some_and(|v| v.shows(&f.path, None));`.
- `graph/row.rs:156` becomes `let selected = self.diff.view(self.focused_pane()).is_some_and(|v| v.shows(&f.path, Some(commit.sha.as_str())));`.

**`desktop.rs`.**
- In `set_project`, line 221 (`self.diff.file = None;`) becomes:

  ```rust
        for v in self.diff.panes.values_mut() {
            v.file = None;
        }
  ```

- In `show_doc`, the diff arms become `self.diff.select(pane, path, None); self.load_diff(pane, cx);` and `self.diff.select(pane, path, Some(sha)); self.load_diff(pane, cx);`.
- In `loaded`, add `let diff = self.diff.view(pane);` above the `match`, and the diff arms become:

  ```rust
            Doc::Diff(p) => diff.is_some_and(|v| v.shows(p, None)),
            Doc::CommitFile { sha, path } => diff.is_some_and(|v| v.shows(path, Some(sha.as_str()))),
  ```

- In `load_active`, add `self.diff.retain_panes(&panes);` before the commit line.

**`terminal_view.rs:338`.** The diff arm ends `=> self.diff_view(MAIN, cx),`.

**`capture.rs`.**
- Line 4 becomes `use crate::desktop::{Desktop, MAIN};`.
- Lines 52–53 become:

  ```rust
        if let Some(i) = d.diff.view(MAIN).and_then(|v| v.lines.iter().position(|l| l.kind == Kind::Add)) {
            d.open_comment(MAIN, i, window, cx);
  ```

- Lines 157–158 become:

  ```rust
    (d.session, d.worktree, d.terminal.focused, d.preview.file) = (None, None, None, None);
    for v in d.diff.panes.values_mut() {
        (v.file, v.at) = (None, None);
    }
  ```

**`desktop/project.rs` `refresh_git`.**
- Lines 100–102 become:

  ```rust
        let root = self.cwd();
        let diffs: Vec<_> = self.diff.panes.iter().filter_map(|(p, v)| Some((*p, v.working_file()?.to_string(), v.open.clone(), v.lines.clone()))).collect();
  ```

- Line 113 becomes:

  ```rust
            let diffs: Vec<_> = root.map(|cwd| diffs.into_iter().map(|(p, path, open, shown)| (p, diff::read_diff(&cwd, path, None, open, &shown))).collect()).unwrap_or_default();
  ```

- Rename `diff` to `diffs` in the task's result tuple (line 124) and in its destructuring (line 127).
- Lines 139–141 become:

  ```rust
                for (pane, load) in diffs {
                    changed |= d.diff.apply(pane, load);
                }
  ```

**Step 4: Run to verify it passes**

Run: `cargo test -p pocket git_ui::diff::`
Expected: PASS, 39 tests.

### Task 3.3: A file per pane, drafts shared

**Files:**
- Modify: `packages/desktop/crates/pocket/src/explorer/preview.rs:10`, `:19` (imports), `:96-98` (`pane` becomes `frame`), `:100-105` (`Views`), `:125-267` (`PreviewState`), `:270-275` (`edited`), `:331-371` (`load_file`, `file_view`), `:373-396` (`sync_code`), `:400-401` (test imports), `:443-595` (tests)
- Modify: `packages/desktop/crates/pocket/src/explorer/preview/code.rs:1`, `:89`
- Modify: `packages/desktop/crates/pocket/src/explorer/preview/markdown.rs:1-2`, `:44-60`
- Modify: `packages/desktop/crates/pocket/src/explorer/preview/header.rs:11`, `:34`, `:42-58`
- Modify: `packages/desktop/crates/pocket/src/explorer.rs:130`
- Modify: `packages/desktop/crates/pocket/src/desktop.rs:98`, `:119`, `:165`, plus the `set_project`, `show_doc`, `loaded` and `load_active` lines named below
- Modify: `packages/desktop/crates/pocket/src/terminal_view.rs:337`
- Modify: `packages/desktop/crates/pocket/src/capture.rs:157`
- Modify: `packages/desktop/crates/pocket/src/desktop/project.rs:97-99`, `:120-121`, `:124`, `:127`, `:135-137`

**Context:**
- **What each pane owns.** A pane's `FilePane` holds the file, its load, sync bookkeeping, `md_source`, and its own editors (`views`).
- **Editors are built lazily.** The editors need a `Window`, so `sync_code` builds them on the pane's first render. Separate editor entities per pane also keep GPUI element ids apart.
- **What stays shared.** `drafts` stays shared, so two panes showing one file edit one draft. An edit in one pane marks every other pane that holds that file stale, so it reloads.
- **Tests without GPUI.** `PreviewState<V>` stays generic, so tests use `PreviewState<()>`. `new` and `with` give way to `Default`, and the editor subscription moves into `Views::new`.
- **Rename.** The free fn `pane()` would be shadowed by the new `pane` parameters, so it becomes `frame()`.
- **Moved function.** `markdown_pane` becomes a free fn over `&Views`.

**Step 1: Write the failing tests**

The test imports (400–401) become:

```rust
    use super::{Body, FilePane, Mark, Preview, PreviewState, classify, decode, language, preview, size};
    use crate::desktop::MAIN;
    use git::{Kind, Line};
    use workspace::tree::PaneId;
```

Replace everything from `fn opened` (443) to the end of `mod tests` with:

```rust
    const OTHER: PaneId = 1;

    fn opened(path: &str) -> PreviewState<()> {
        let mut state = PreviewState::default();
        state.open(MAIN, path.into());
        state
    }

    fn main(state: &mut PreviewState<()>) -> &mut FilePane<()> {
        state.panes.get_mut(&MAIN).unwrap()
    }

    fn text(t: &str) -> Preview {
        Preview::Text(t.into())
    }

    #[test]
    fn a_load_for_a_file_no_longer_shown_is_dropped() {
        let mut state = opened("/r/b.rs");
        assert!(!state.apply(MAIN, ("/r/a.rs".into(), text("a"), Vec::new())));
        let f = main(&mut state);
        assert_eq!((f.file.as_deref(), f.preview.clone()), (Some("/r/b.rs"), None));
    }

    #[test]
    fn a_load_for_the_shown_file_changes_it_only_when_its_content_differs() {
        let mut state = opened("/r/a.rs");
        main(&mut state).stale = false;
        assert!(state.apply(MAIN, ("/r/a.rs".into(), text("a"), Vec::new())));
        let f = main(&mut state);
        assert_eq!((f.preview.clone(), f.stale), (Some(text("a")), true));
        f.stale = false;
        assert!(!state.apply(MAIN, ("/r/a.rs".into(), text("a"), Vec::new())));
        assert!(!main(&mut state).stale);
    }

    #[test]
    fn opening_another_file_clears_the_last_one_but_reopening_keeps_it() {
        let mut state = opened("/r/a.md");
        state.apply(MAIN, ("/r/a.md".into(), text("# a"), Vec::new()));
        let f = main(&mut state);
        (f.md_source, f.stale) = (true, false);
        f.open("/r/a.md".into());
        assert_eq!((f.preview.clone(), f.md_source, f.stale), (Some(text("# a")), true, false));
        f.open("/r/b.md".into());
        assert_eq!((f.file.as_deref(), f.preview.clone(), f.md_source, f.stale), (Some("/r/b.md"), None, false, true));
    }

    fn body_of(path: &str, preview: Preview) -> Body {
        let mut state = opened(path);
        state.apply(MAIN, (path.into(), preview, Vec::new()));
        state.body(MAIN)
    }

    #[test]
    fn markdown_text_renders_unless_its_source_is_asked_for() {
        let mut state = opened("/r/README.md");
        state.apply(MAIN, ("/r/README.md".into(), text("# hi"), Vec::new()));
        assert_eq!((main(&mut state).markdown(), state.body(MAIN)), (true, Body::Markdown));
        main(&mut state).md_source = true;
        assert_eq!(state.body(MAIN), Body::Code);
        assert_eq!(body_of("/r/a.rs", text("fn a() {}")), Body::Code);
        assert_eq!(body_of("/r/x.md", Preview::Binary(4)), Body::Note("Binary file · 4 B".into()));
    }

    #[test]
    fn files_without_text_show_the_image_or_say_why_not_and_nothing_while_loading() {
        assert_eq!(body_of("/r/logo.png", Preview::Image), Body::Image);
        assert_eq!(body_of("/r/big.log", Preview::TooLarge(3 * 1024 * 1024)), Body::Note("Too large to preview · 3.0 MB".into()));
        assert_eq!(body_of("/r/gone.rs", Preview::Unreadable), Body::Note("This file can't be shown.".into()));
        assert_eq!(opened("/r/a.rs").body(MAIN), Body::Blank);
    }

    #[test]
    fn a_newly_shown_file_syncs_once_with_its_grammar_and_from_the_top() {
        let mut state = opened("/r/a.rs");
        state.apply(MAIN, ("/r/a.rs".into(), text("fn a() {}"), Vec::new()));
        let sync = state.take_sync(MAIN).unwrap();
        assert_eq!((sync.text.as_ref(), sync.language, sync.same, sync.reload), ("fn a() {}", "rust", false, true));
        assert!(state.take_sync(MAIN).is_none());
    }

    #[test]
    fn a_refreshed_file_keeps_its_place_and_reloads_only_when_its_text_changed() {
        let mut state = opened("/r/a.rs");
        state.apply(MAIN, ("/r/a.rs".into(), text("a\nb\n"), Vec::new()));
        state.take_sync(MAIN);
        let added = vec![Line { kind: Kind::Add, old: None, new: Some(2), text: "b".into() }];
        state.apply(MAIN, ("/r/a.rs".into(), text("a\nb\n"), added.clone()));
        let sync = state.take_sync(MAIN).unwrap();
        assert_eq!((sync.same, sync.reload), (true, false));
        assert_eq!(*main(&mut state).marks, [(2, Mark::Added)]);
        state.apply(MAIN, ("/r/a.rs".into(), text("a\nc\n"), added));
        let sync = state.take_sync(MAIN).unwrap();
        assert_eq!((sync.text.as_ref(), sync.same, sync.reload), ("a\nc\n", true, true));
    }

    fn show(state: &mut PreviewState<()>, pane: PaneId, path: &str, disk: &str) {
        state.open(pane, path.into());
        state.apply(pane, (path.into(), text(disk), Vec::new()));
        state.take_sync(pane);
    }

    fn shown(path: &str, disk: &str) -> PreviewState<()> {
        let mut state = PreviewState::default();
        show(&mut state, MAIN, path, disk);
        state
    }

    #[test]
    fn an_edit_turns_the_file_dirty_once_and_undoing_it_turns_it_clean() {
        let mut state = shown("/r/a.rs", "a\n");
        assert_eq!(state.edit(MAIN, "ab\n".into()), Some("/r/a.rs".into()));
        assert_eq!(state.edit(MAIN, "abc\n".into()), None);
        assert!(state.dirty("/r/a.rs"));
        assert_eq!(state.edit(MAIN, "a\n".into()), Some("/r/a.rs".into()));
        assert!(!state.dirty("/r/a.rs"));
    }

    #[test]
    fn unsaved_edits_outlive_refreshes_and_switching_files() {
        let mut state = shown("/r/a.rs", "a\n");
        state.edit(MAIN, "ab\n".into());
        state.apply(MAIN, ("/r/a.rs".into(), text("changed on disk\n"), Vec::new()));
        let sync = state.take_sync(MAIN).unwrap();
        assert_eq!((sync.text.as_ref(), sync.reload), ("ab\n", false));
        state.open(MAIN, "/r/b.rs".into());
        state.apply(MAIN, ("/r/b.rs".into(), text("b\n"), Vec::new()));
        assert_eq!(state.take_sync(MAIN).unwrap().text.as_ref(), "b\n");
        state.open(MAIN, "/r/a.rs".into());
        let sync = state.take_sync(MAIN).unwrap();
        assert_eq!((sync.text.as_ref(), sync.reload), ("ab\n", true));
        assert!(state.dirty("/r/a.rs") && !state.dirty("/r/b.rs"));
    }

    #[test]
    fn an_unsaved_edit_stays_shown_when_its_file_turns_unreadable() {
        let mut state = shown("/r/a.rs", "a\n");
        state.edit(MAIN, "ab\n".into());
        state.apply(MAIN, ("/r/a.rs".into(), Preview::Unreadable, Vec::new()));
        assert_eq!(state.body(MAIN), Body::Code);
        assert_eq!(state.take_sync(MAIN).unwrap().text.as_ref(), "ab\n");
    }

    #[test]
    fn a_save_cleans_the_file_unless_it_was_edited_while_writing() {
        let mut state = shown("/r/a.rs", "a\n");
        state.edit(MAIN, "ab\n".into());
        state.saved("/r/a.rs", &"ab\n".into());
        assert!(!state.dirty("/r/a.rs"));
        state.edit(MAIN, "abc\n".into());
        state.edit(MAIN, "abcd\n".into());
        state.saved("/r/a.rs", &"abc\n".into());
        assert!(state.dirty("/r/a.rs"));
        assert_eq!(state.edit(MAIN, "abc\n".into()), Some("/r/a.rs".into()));
    }

    #[test]
    fn discarding_shows_the_file_on_disk_again() {
        let mut state = shown("/r/a.rs", "a\n");
        state.edit(MAIN, "ab\n".into());
        state.discard("/r/a.rs");
        let sync = state.take_sync(MAIN).unwrap();
        assert_eq!((sync.text.as_ref(), sync.reload), ("a\n", true));
        assert!(!state.dirty("/r/a.rs"));
    }

    #[test]
    fn two_panes_show_two_files_but_share_one_draft_per_file() {
        let mut state = shown("/r/a.rs", "a\n");
        show(&mut state, OTHER, "/r/b.rs", "b\n");
        state.edit(MAIN, "ab\n".into());
        assert_eq!((state.file(MAIN), state.file(OTHER)), (Some("/r/a.rs"), Some("/r/b.rs")));
        state.open(OTHER, "/r/a.rs".into());
        state.apply(OTHER, ("/r/a.rs".into(), text("a\n"), Vec::new()));
        assert_eq!(state.take_sync(OTHER).unwrap().text.as_ref(), "ab\n");
    }

    #[test]
    fn a_load_lands_only_in_the_pane_showing_its_file() {
        let mut state = opened("/r/a.rs");
        state.open(OTHER, "/r/b.rs".into());
        assert!(!state.apply(OTHER, ("/r/a.rs".into(), text("a"), Vec::new())));
        assert!(state.apply(MAIN, ("/r/a.rs".into(), text("a"), Vec::new())));
        assert_eq!(state.pane(OTHER).unwrap().preview, None);
    }

    #[test]
    fn an_edit_in_one_pane_reloads_another_holding_that_file() {
        let mut state = shown("/r/a.rs", "a\n");
        show(&mut state, OTHER, "/r/a.rs", "a\n");
        state.edit(MAIN, "ab\n".into());
        assert!(state.take_sync(MAIN).is_none());
        let sync = state.take_sync(OTHER).unwrap();
        assert_eq!((sync.text.as_ref(), sync.same, sync.reload), ("ab\n", true, true));
    }

    #[test]
    fn a_closed_panes_file_goes_but_its_unsaved_edit_stays() {
        let mut state = shown("/r/a.rs", "a\n");
        show(&mut state, OTHER, "/r/b.rs", "b\n");
        state.edit(OTHER, "bc\n".into());
        state.retain_panes(&[MAIN]);
        assert_eq!(state.file(OTHER), None);
        assert!(state.dirty("/r/b.rs"));
    }
```

**Step 2: Run to verify it fails**

Run: `cargo test -p pocket explorer::preview::`
Expected: FAIL to compile — no `FilePane` in `super`, and `PreviewState` has no `default`, `panes` or `pane`, and its methods don't take a pane.

**Step 3: Implement**

**`preview.rs` imports.** After line 10 add `use markdown::markdown_pane;`. After line 19 add `use workspace::tree::PaneId;`.

**`frame`.** Lines 96–98: `fn pane() -> Div {` becomes `fn frame() -> Div {`. In `code.rs`, line 1 becomes `use super::frame;` and line 89 becomes `    frame()`.

**`Views`.** Replace 100–105 with:

```rust
/// The editors a file previews in.
pub struct Views {
    pub(crate) code: Entity<EditorState>,
    pub(crate) md: Entity<TextViewState>,
    pub(crate) diagrams: Entity<Diagrams>,
    _edits: Subscription,
}

impl Views {
    fn new(pane: PaneId, window: &mut Window, cx: &mut Context<Desktop>) -> Self {
        let code = cx.new(|cx| EditorState::new(window, cx).line_number(true).searchable(true).soft_wrap(false));
        let md = cx.new(|cx| TextViewState::markdown("", cx));
        let diagrams = cx.new(|_| Diagrams::new(&md));
        let _edits = cx.subscribe(&code, move |this, code, ev: &InputEvent, cx| {
            if let InputEvent::Change = ev {
                let text = code.read(cx).value();
                this.edited(pane, text, cx);
            }
        });
        Self { code, md, diagrams, _edits }
    }
}
```

**`FilePane` and `PreviewState`.** Replace 125–267 (`PreviewState`, `impl PreviewState`, `impl<V> PreviewState<V>`) with:

```rust
/// The file one pane shows and what its editors hold.
pub struct FilePane<V = Views> {
    pub(crate) file: Option<String>,
    pub(crate) preview: Option<Preview>,
    pub(crate) diff: Vec<git::Line>,
    pub(crate) stale: bool,
    pub(crate) code_file: Option<String>,
    pub(crate) code_text: SharedString,
    /// What git changed in the text the editor holds.
    pub(crate) marks: Rc<[(usize, Mark)]>,
    pub(crate) md_source: bool,
    /// Built on the pane's first render, since editors need the window.
    pub(crate) views: Option<V>,
}

impl<V> Default for FilePane<V> {
    fn default() -> Self {
        Self {
            file: None,
            preview: None,
            diff: Vec::new(),
            stale: false,
            code_file: None,
            code_text: SharedString::default(),
            marks: Rc::default(),
            md_source: false,
            views: None,
        }
    }
}

impl<V> FilePane<V> {
    /// Shows `path`, clearing what another file left behind until it loads.
    pub fn open(&mut self, path: String) {
        if self.file.as_ref() != Some(&path) {
            self.file = Some(path);
            self.preview = None;
            self.diff.clear();
            self.stale = true;
            self.md_source = false;
        }
    }

    /// Shows `file` unless another file was opened while it loaded; returns whether anything changed.
    pub fn apply(&mut self, (path, preview, lines): (String, Preview, Vec<Line>)) -> bool {
        if self.file.as_ref() != Some(&path) {
            return false;
        }
        let preview = Some(preview);
        let fresh = preview != self.preview || lines != self.diff;
        self.stale |= fresh;
        (self.preview, self.diff) = (preview, lines);
        fresh
    }

    pub fn text(&self) -> Option<&str> {
        match &self.preview {
            Some(Preview::Text(t)) => Some(t.as_str()),
            _ => None,
        }
    }

    /// Whether the file is markdown text, which renders unless its source is asked for.
    pub fn markdown(&self) -> bool {
        self.text().is_some() && language_for(self.file.as_deref().unwrap_or_default()) == "markdown"
    }
}

/// The file each pane shows, and the unsaved edits all panes share. The editors are `V` so the rules over the plain state test without GPUI.
pub struct PreviewState<V = Views> {
    pub(crate) panes: HashMap<PaneId, FilePane<V>>,
    /// The copied path and the timer that turns its check back; a new copy replaces, and so cancels, the old one.
    pub(crate) path_copied: Option<(String, Task<()>)>,
    /// Unsaved text by file path. Panes showing one file share its draft, so edits live here, not in an editor.
    pub(crate) drafts: HashMap<String, SharedString>,
}

impl<V> Default for PreviewState<V> {
    fn default() -> Self {
        Self { panes: HashMap::new(), path_copied: None, drafts: HashMap::new() }
    }
}

impl<V> PreviewState<V> {
    pub fn pane(&self, pane: PaneId) -> Option<&FilePane<V>> {
        self.panes.get(&pane)
    }

    pub fn file(&self, pane: PaneId) -> Option<&str> {
        self.pane(pane)?.file.as_deref()
    }

    pub fn open(&mut self, pane: PaneId, path: String) {
        self.panes.entry(pane).or_default().open(path);
    }

    pub fn apply(&mut self, pane: PaneId, file: (String, Preview, Vec<Line>)) -> bool {
        self.panes.get_mut(&pane).is_some_and(|f| f.apply(file))
    }

    pub fn body(&self, pane: PaneId) -> Body {
        let Some(f) = self.pane(pane) else { return Body::Blank };
        match &f.preview {
            Some(Preview::Text(_)) if f.markdown() && !f.md_source => Body::Markdown,
            Some(Preview::Text(_)) => Body::Code,
            _ if f.file.as_deref().is_some_and(|p| self.dirty(p)) => Body::Code,
            Some(Preview::Image) => Body::Image,
            Some(Preview::Binary(n)) => Body::Note(format!("Binary file · {}", size(*n as usize))),
            Some(Preview::TooLarge(n)) => Body::Note(format!("Too large to preview · {}", size(*n as usize))),
            Some(Preview::Unreadable) => Body::Note("This file can't be shown.".into()),
            None => Body::Blank,
        }
    }

    /// What `pane`'s views must load since its file last changed, once per change.
    pub fn take_sync(&mut self, pane: PaneId) -> Option<CodeSync> {
        let f = self.panes.get_mut(&pane)?;
        if !std::mem::take(&mut f.stale) {
            return None;
        }
        let draft = f.file.as_ref().and_then(|p| self.drafts.get(p)).cloned();
        let text = draft.unwrap_or_else(|| f.text().map(|t| SharedString::from(t.to_string())).unwrap_or_default());
        let same = f.code_file == f.file;
        let reload = !same || text != f.code_text;
        let language = language_for(f.file.as_deref().unwrap_or_default());
        f.code_file = f.file.clone();
        f.code_text = text.clone();
        f.marks = gutter(&f.diff).into();
        Some(CodeSync { text, language, same, reload })
    }

    /// Keeps `pane`'s editor text as the draft of the file it holds, dropping the draft once the text matches the file on disk, and reloads other panes holding that file. Returns the file when that flipped whether it's dirty.
    pub fn edit(&mut self, pane: PaneId, text: SharedString) -> Option<String> {
        let f = self.panes.get_mut(&pane)?;
        let file = f.code_file.clone()?;
        let clean = f.file == f.code_file && f.text() == Some(text.as_ref());
        let was_dirty = if clean { self.drafts.remove(&file).is_some() } else { self.drafts.insert(file.clone(), text.clone()).is_some() };
        f.code_text = text;
        for (_, other) in self.panes.iter_mut().filter(|(p, o)| **p != pane && o.code_file.as_ref() == Some(&file)) {
            other.stale = true;
        }
        (was_dirty == clean).then_some(file)
    }

    pub fn dirty(&self, path: &str) -> bool {
        self.drafts.contains_key(path)
    }

    /// Takes `text` as what `path` now holds on disk; its draft goes unless edited since.
    pub fn saved(&mut self, path: &str, text: &SharedString) {
        if self.drafts.get(path) == Some(text) {
            self.drafts.remove(path);
        }
        for f in self.panes.values_mut().filter(|f| f.file.as_deref() == Some(path)) {
            f.preview = Some(Preview::Text(text.to_string()));
        }
    }

    /// Drops `path`'s draft, so the editors show the file on disk again.
    pub fn discard(&mut self, path: &str) {
        self.drafts.remove(path);
        for f in self.panes.values_mut() {
            f.stale = true;
        }
    }

    /// Forgets the files of panes that closed; their unsaved edits stay.
    pub fn retain_panes(&mut self, panes: &[PaneId]) {
        self.panes.retain(|p, _| panes.contains(p));
    }
}
```

**`edited`.** Lines 270–271 become:

```rust
    fn edited(&mut self, pane: PaneId, text: SharedString, cx: &mut Context<Self>) {
        if let Some(file) = self.preview.edit(pane, text) {
```

**`load_file`.** Lines 331–332 become:

```rust
    pub fn load_file(&mut self, pane: PaneId, cx: &mut Context<Self>) {
        let Some(path) = self.preview.file(pane).map(str::to_string) else { return };
```

In the same function, `if d.preview.apply(file) {` becomes `if d.preview.apply(pane, file) {`.

**`file_view`.** Lines 347–353 become:

```rust
    pub fn file_view(&mut self, pane: PaneId, cx: &mut Context<Self>) -> Div {
        let Some(path) = self.preview.file(pane).map(str::to_string) else { return div() };
        let header = self.file_header(pane, &path, cx);
        let Some(FilePane { views: Some(views), marks, .. }) = self.preview.pane(pane) else { return div() };
        let body = match self.preview.body(pane) {
            Body::Markdown => markdown_pane(views, cx).into_any_element(),
            Body::Code => code_pane(&views.code, marks.clone()).into_any_element(),
            Body::Image => frame()
```

**`sync_code`.** Replace 373–396 with:

```rust
    /// Loads each pane's file into its code editor after it changed, keeping the scroll position when the same file refreshes.
    pub fn sync_code(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let panes: Vec<PaneId> = self.preview.panes.keys().copied().collect();
        for pane in panes {
            if self.preview.panes.get(&pane).is_some_and(|f| f.views.is_none()) {
                let views = Views::new(pane, window, cx);
                self.preview.panes.entry(pane).or_default().views = Some(views);
            }
            let Some(CodeSync { text, language, same, reload }) = self.preview.take_sync(pane) else { continue };
            let Some(FilePane { views: Some(views), code_text, .. }) = self.preview.pane(pane) else { continue };
            views.code.update(cx, |s, cx| {
                if !same {
                    s.set_highlighter(language, cx);
                }
                if reload {
                    let scroll = s.scroll_offset();
                    s.set_value(text, window, cx);
                    if same {
                        s.set_scroll_offset(scroll, cx);
                    }
                }
            });
            if reload {
                views.md.update(cx, |md, cx| {
                    md.set_text(code_text, cx);
                    if !same {
                        md.list_state().scroll_to(ListOffset::default());
                    }
                });
            }
        }
    }
```

**`markdown.rs`.** Lines 1–2 become `use super::{Views, frame};`. Replace the `impl Desktop` block (44–60) with:

```rust
/// A markdown file rendered, with its mermaid fences drawn as diagrams.
pub(super) fn markdown_pane(views: &Views, cx: &App) -> Div {
    // Side padding sits on the TextView so its scrollbar reaches the edge; vertical padding there would skew the scrollbar's thumb.
    frame().py(px(32.)).bg(PAGE).child(
        TextView::new(&views.md)
            .plugin(Mermaid(views.diagrams.clone()))
            .code_block_actions(|block, _, _| code_actions(block))
            .selectable(true)
            .scrollable(true)
            .style(markdown_style(cx.theme().highlight_theme.clone()))
            .size_full()
            .px(px(40.)),
    )
}
```

**`header.rs`.** Add `use workspace::tree::PaneId;` after line 11.
- Line 34 becomes `pub(super) fn file_header(&mut self, pane: PaneId, path: &str, cx: &mut Context<Self>) -> Div {`.
- Line 42 becomes:

  ```rust
        let file = self.preview.pane(pane);
        let text = file.and_then(|f| f.text());
  ```

- Line 47 becomes `.when(file.is_some_and(|f| f.markdown()), |d| {`.
- Line 50 becomes `file.is_some_and(|f| f.md_source),`.
- The toggle closure (53–58) becomes:

  ```rust
                    move |this, v, cx| {
                        let Some(f) = this.preview.panes.get_mut(&pane) else { return };
                        f.md_source = v;
                        if let Some(views) = &f.views {
                            views.md.update(cx, |md, cx| md.set_text(&f.code_text, cx));
                        }
                        cx.notify();
                    },
  ```

**`explorer.rs:130`** becomes `let selected = self.preview.file(self.focused_pane()) == Some(key.as_str());`.

**`desktop.rs`.**
- Delete line 98 (`let (preview, preview_subs) = …`) and line 119 (`_subs.extend(preview_subs);`).
- Line 165 becomes `preview: PreviewState::default(),`.
- In `set_project`, `self.preview.file = None;` becomes:

  ```rust
        for p in self.preview.panes.values_mut() {
            p.file = None;
        }
  ```

- In `show_doc`, the `File` arm becomes `self.preview.open(pane, path); self.load_file(pane, cx);`.
- In `loaded`, the `File` arm becomes `Doc::File(p) => self.preview.file(pane) == Some(p.as_str()),`.
- In `load_active`, add `self.preview.retain_panes(&panes);`.

**`terminal_view.rs:337`** becomes `Some(Tab::Doc(doc @ Doc::File(_))) if self.loaded(MAIN, &doc) => self.file_view(MAIN, cx),`.

**`capture.rs:157`** becomes `(d.session, d.worktree, d.terminal.focused) = (None, None, None);`, followed by:

```rust
    for p in d.preview.panes.values_mut() {
        p.file = None;
    }
```

**`desktop/project.rs` `refresh_git`.**
- Lines 97–99 become:

  ```rust
        let files: Vec<_> = self.preview.panes.iter().filter_map(|(p, f)| {
            let f = f.file.clone()?;
            let changed = self.file_status(&f).is_some();
            Some((*p, f, changed))
        }).collect();
  ```

- Line 120 becomes `let files: Vec<_> = files.into_iter().map(|(p, f, changed)| (p, preview::load(&f, changed))).collect();`.
- Rename `file` to `files` in the result tuple (lines 121 and 124).
- Lines 135–137 become:

  ```rust
                for (pane, file) in files {
                    changed |= d.preview.apply(pane, file);
                }
  ```

**Step 4: Run to verify it passes**

Run: `cargo test -p pocket explorer::preview::`
Expected: PASS, 25 tests.

### Task 3.4: Browsers per pane

**Files:**
- Modify: `packages/desktop/crates/pocket/src/browser.rs:12` (import), `:31-41` (`Browsers`), `:44-65` (`new`), `:74-87` (`show`, `page`), `:91-107` (`active_web_tab`, `sync_browser`), `:171-206` (`focus_address` and the page actions)
- Modify: `packages/desktop/crates/pocket/src/browser/view.rs:7`, `:10-15`, `:50`, `:56-59`, `:73`
- Modify: `packages/desktop/crates/pocket/src/modals.rs:43`
- Modify: `packages/desktop/crates/pocket/src/desktop.rs:102`, `:123`, `load_active`
- Modify: `packages/desktop/crates/pocket/src/terminal_view.rs:340`

**Context:**
- **Address bars.** Each pane gets its own address bar, built the first time the pane renders a web tab, because it needs a `Window`. Its subscription browses the tab its pane shows.
- **Shown pages.** `shown` becomes the set of tabs whose page may be placed. Every other page is parked.
- **Focus.** Only the focused pane takes `Browsers::focus` and gives keys to its page.
- **Page actions.** Reload, Back, Forward, the page edit actions and FocusAddress target the focused pane's tab.
- **Seam.** `active_web_tab()` becomes `web_tab(pane)`.
- **No new tests.** `Browsers` holds entities and a `FocusHandle`, so it can't be built in a test. The `browser` capture covers this task.

**Step 1: Implement**

**`browser.rs` imports.** Add `use workspace::tree::PaneId;` after line 12.

**`Browsers` fields.** In `Browsers`, replace the `address` field and its doc (35–36) with:

```rust
    /// Each pane's address bar, built the first time it shows a tab, with the subscription that browses from it.
    addresses: HashMap<PaneId, (Entity<InputState>, Subscription)>,
```

Replace the `shown` field and its doc (39–40) with:

```rust
    /// The tabs whose pages may be placed: a page draws over GPUI, so it is parked under any menu or sheet.
    pub(crate) shown: HashSet<u64>,
```

**`new` and `address`.** `new` (44–65) becomes:

```rust
    pub fn new(window: &mut Window, cx: &mut Context<Desktop>) -> Self {
        let (events, mut rx) = unbounded();
        cx.spawn_in(window, async move |this, cx| {
            while let Some((id, ev)) = rx.next().await {
                if this.update_in(cx, |d: &mut Desktop, window, cx| d.on_page(id, ev, window, cx)).is_err() {
                    break;
                }
            }
        })
        .detach();
        Self { tabs: HashMap::new(), next: 0, addresses: HashMap::new(), focus: cx.focus_handle(), shown: HashSet::new(), events }
    }

    fn address(&mut self, pane: PaneId, window: &mut Window, cx: &mut Context<Desktop>) -> Entity<InputState> {
        let (address, _) = self.addresses.entry(pane).or_insert_with(|| {
            let address = cx.new(|cx| InputState::new(window, cx).placeholder("Search or enter address"));
            let sub = cx.subscribe_in(&address, window, move |this, address, ev: &InputEvent, window, cx| {
                let text = address.read(cx).value();
                if let InputEvent::PressEnter { secondary: false, .. } = ev
                    && !text.trim().is_empty()
                    && let Some(id) = this.web_tab(pane)
                {
                    this.browse(id, web::resolve(&text), window, cx);
                }
            });
            (address, sub)
        });
        address.clone()
    }
```

**`show`, `page`, `retain_panes`.** Replace `show` and `page` (74–87) with:

```rust
    /// Drops the tabs no workspace holds any more, and parks every page but `shown`'s.
    fn show(&mut self, shown: &[u64], live: &HashSet<u64>) {
        self.tabs.retain(|id, _| live.contains(id));
        for (_, b) in self.tabs.iter().filter(|(id, _)| !shown.contains(id)) {
            if let Some(page) = &b.page {
                page.park();
            }
        }
        self.shown = shown.iter().copied().collect();
    }

    fn page(&self, id: u64) -> Option<&web::Page> {
        self.tabs.get(&id).filter(|_| self.shown.contains(&id))?.page.as_deref()
    }

    /// Drops the address bars of panes that closed.
    pub fn retain_panes(&mut self, panes: &[PaneId]) {
        self.addresses.retain(|p, _| panes.contains(p));
    }
```

**`web_tab`.** Replace `active_web_tab` (91–97) with:

```rust
    pub(crate) fn web_tab(&mut self, pane: PaneId) -> Option<u64> {
        self.session_tree()?;
        match self.pane_tab(pane)? {
            Tab::Web(id) => Some(id),
            _ => None,
        }
    }
```

**`sync_browser`.** Line 104 becomes:

```rust
        let shown: Vec<u64> = if covered { Vec::new() } else { self.pane_ids().into_iter().filter_map(|p| self.web_tab(p)).collect() };
```

Line 106 becomes `self.browsers.show(&shown, &live);`.

**`focus_address`.** Its body's first lines (172–177) become:

```rust
        let pane = self.focused_pane();
        let Some(b) = self.web_tab(pane).and_then(|id| self.browsers.tabs.get(&id)) else { return };
        if let Some(page) = &b.page {
            page.give_keys();
        }
        let url = b.url.clone();
        self.browsers.address(pane, window, cx).update(cx, |s, cx| {
```

**Page actions.** In `reload`, `back`, `forward` and `page_edit` (184–206), each `if let Some(page) = self.browsers.page() {` becomes:

```rust
        if let Some(page) = self.web_tab(self.focused_pane()).and_then(|id| self.browsers.page(id)) {
```

**`browser/view.rs`.** Add `use workspace::tree::PaneId;` after line 7. Lines 10–12 become:

```rust
    pub(crate) fn browser_view(&mut self, pane: PaneId, id: u64, window: &mut Window, cx: &mut Context<Self>) -> Div {
        if !self.browsers.tabs.contains_key(&id) {
            return div().flex_1();
        }
        let address = self.browsers.address(pane, window, cx);
        let b = &self.browsers.tabs[&id];
```

Line 50 becomes:

```rust
        let page = b.page.clone().filter(|_| self.browsers.shown.contains(&id));
        let focused = pane == self.focused_pane();
```

In the canvas, add the `&& focused` condition before `&& focus.is_focused(window)`:

```rust
                if let Some(page) = &page
                    && page.place(rect)
                    && focused
                    && focus.is_focused(window)
```

Line 73 becomes `.when(focused, |d| d.track_focus(&self.browsers.focus))`.

**Callers.**
- `modals.rs:43` becomes `if self.web_tab(self.focused_pane()).is_some() {`.
- In `desktop.rs`, line 102 becomes `let browsers = Browsers::new(window, cx);`. Delete line 123 (`_subs.extend(browser_subs);`). In `load_active`, add `self.browsers.retain_panes(&panes);`, so it ends up as:

  ```rust
        let panes = self.pane_ids();
        self.diff.retain_panes(&panes);
        self.preview.retain_panes(&panes);
        self.commit.retain_panes(&panes);
        self.browsers.retain_panes(&panes);
  ```

- `terminal_view.rs:340` becomes `Some(Tab::Web(id)) => self.browser_view(MAIN, id, window, cx),`.

**Step 2: Run to verify it passes**

Run: `cargo build --workspace`
Expected: builds with no warnings beyond the `block v0.1.6` note.

### Task 3.5: Gate and screens

**Files:** none

**Step 1: Gate**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: builds; clippy shows only the baseline warnings; all tests pass (pocket: 333).

**Step 2: Screens after**

Run (repo root): `.ui-review/fixture/capture.sh /tmp/split-pr3-after changes=session,changes comment=session,changes,comment file=session,explore,file graph-commit=session,changes,graph-expand,graph-commit browser=session,browser`
Expected: each PNG matches its counterpart in `/tmp/split-pr3-before`. That includes the comment composer under the first added line, the file's code editor, the commit's stacked diff, and the browser's address bar focused.

## PR 4: Show the main area as panels

**Scope:**
- `Workspace` becomes a `Tree<Tab>` of panes. `Tab::Term(String)` is one terminal, so a terminal split is two panes.
- The main area draws the tree: resizable dividers, one tab bar per pane, and per-pane split and ⋯ buttons. The context ring moves into each pane's bar.
- Terminals open in the terminal pane nearest the focus. Docs open in the doc pane nearest the focus, else in a new pane to the right. Browsers open in the focused pane.
- The tab right-click menu adds "Move to new split right / down".
- New code goes in `pocket::panels`, with `Panels` as its state. The tab strip, + menu and tab actions move there from `terminal_view`.
- The PR 3 seams become real:
  - `focused_pane` → `tree.focused`
  - `pane_ids` → the ids from `tree.panes()`
  - `pane_tab` → that pane's `active()`

  `MAIN` stays as the fallback, and its doc becomes "The pane a worktree starts with."

**Not in this PR:**
- dragging tabs between panes (PR 5)
- persistence (PR 6)
- shortcuts, `Tree::drawn()`, and unzoom-on-focus (PR 7)

The render already draws only the zoomed pane when `tree.zoomed.is_some()`.

**Depends on:** PR 1 (`workspace::tree`), PR 2 (session bar trimmed to the ring), PR 3 (per-pane doc and browser state).

**Done when:**
- The gate passes: `cargo test -p workspace` shows 42 tests, and `cargo test -p pocket` shows the PR 3 count − 4 + 1.
- The screens show one pane where HEAD showed the session page, and a split where `split=` asks for one.

**How the code was checked:**
- Every task below was applied to a scratch copy of PR 1 + PR 3, without PR 2.
- In that copy, `cargo check`, `cargo clippy --all-targets` (baseline warnings only) and `cargo test -p pocket -p workspace` all pass: pocket 330, workspace 42.
- Edits are given as diffs against PR 3. PR 2 also edits `session_page`, which this PR deletes. If a hunk's context differs there, delete the whole function anyway.

Before Task 4.1, capture the screens. From the repo root:

Run: `.ui-review/fixture/capture.sh /tmp/split-pr4-before session=session split=session,explore,file tab-menu=session,tab-menu browser=session,browser focus=session,focus`

### Task 4.1: A workspace is a tree of panes

**Files:**
- Modify: `packages/desktop/crates/workspace/src/workspace.rs`: everything after `pub mod tree;`, tests included.

**Context:**
- `Tab::Term(String)` replaces `Tab::Term(Vec<Vec<String>>)`. A split terminal tab is now two panes.
- `Place` says where a new terminal goes:
  - `Pane(None)`: the terminal pane nearest the focus.
  - `Pane(Some(p))`: pane `p`.
  - `Split(p, edge)`: a new pane at `edge` of `p`.
- `sync` gives terminals it hasn't seen a tab in the top-left pane, without moving the focus.
- `open_doc(doc, pin, pane)`:
  - A doc already shown is selected wherever it is.
  - An unpinned open takes over the preview tab only if that tab is in `pane`.
  - With no `pane`, the doc opens in a new pane right of the focus.
- `shown()` lists the tab each drawn pane shows: only the zoomed pane's when zoomed.

**Step 1: Write the failing tests**

Replace `mod tests` at the bottom of `workspace.rs` with:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn term(id: &str) -> Tab {
        Tab::Term(id.into())
    }

    fn diff(path: &str) -> Doc {
        Doc::Diff(path.into())
    }

    fn file(path: &str) -> Doc {
        Doc::File(path.into())
    }

    fn ids(ids: &[&str]) -> Vec<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    fn with(terms: &[&str]) -> Workspace {
        let mut w = Workspace::default();
        w.sync(&ids(terms), &[]);
        w
    }

    fn tabs(w: &Workspace) -> Vec<Vec<Tab>> {
        w.tree.panes().iter().map(|p| p.tabs.clone()).collect()
    }

    fn shown(w: &Workspace) -> Option<&Tab> {
        w.tree.focused().active()
    }

    #[test]
    fn new_terminals_open_top_left_without_taking_the_focus() {
        let mut w = with(&["a"]);
        let b = w.tree.split(0, Edge::Right, term("b")).unwrap();
        w.sync(&ids(&["a", "b", "c"]), &[]);
        assert_eq!(tabs(&w), vec![vec![term("a"), term("c")], vec![term("b")]]);
        assert_eq!((w.tree.focused, w.tree.pane(0).unwrap().active), (b, 0));
    }

    #[test]
    fn sync_drops_other_worktrees_terminals_and_keeps_unlisted_ones() {
        let mut w = with(&["a"]);
        w.open_term("x".into(), Place::Pane(None));
        w.sync(&ids(&["b"]), &ids(&["a"]));
        assert_eq!(tabs(&w), vec![vec![term("x"), term("b")]]);
    }

    #[test]
    fn a_new_terminal_opens_in_the_terminal_pane_nearest_the_focus() {
        let mut w = with(&["a"]);
        w.open_doc(diff("x"), true, None);
        w.open_term("b".into(), Place::Pane(None));
        assert_eq!(tabs(&w), vec![vec![term("a"), term("b")], vec![Tab::Doc(diff("x"))]]);
        assert_eq!((w.tree.focused, shown(&w)), (0, Some(&term("b"))));
    }

    #[test]
    fn a_new_terminal_opens_where_asked() {
        let mut w = with(&["a"]);
        w.open_term("b".into(), Place::Split(0, Edge::Bottom));
        let b = w.tree.focused;
        w.open_term("c".into(), Place::Pane(Some(0)));
        w.open_term("d".into(), Place::Pane(Some(9)));
        assert_eq!(tabs(&w), vec![vec![term("a"), term("c"), term("d")], vec![term("b")]]);
        assert_ne!(b, 0);
    }

    #[test]
    fn opening_a_shown_terminal_selects_its_tab() {
        let mut w = with(&["a", "b"]);
        w.open_term("a".into(), Place::Split(0, Edge::Right));
        assert_eq!((tabs(&w), shown(&w)), (vec![vec![term("a"), term("b")]], Some(&term("a"))));
    }

    #[test]
    fn docs_open_in_an_empty_focused_pane_else_the_nearest_doc_pane() {
        let mut w = Workspace::default();
        assert_eq!(w.doc_pane(), Some(0));
        w.open_term("a".into(), Place::Pane(None));
        assert_eq!(w.doc_pane(), None);
        let (d, _) = w.open_doc(diff("x"), true, None);
        w.tree.focus(0);
        assert_eq!(w.doc_pane(), Some(d));
    }

    #[test]
    fn a_doc_with_no_pane_to_open_in_splits_right_of_the_focus() {
        let mut w = with(&["a"]);
        let at = w.open_doc(diff("x"), false, None);
        assert_eq!(tabs(&w), vec![vec![term("a")], vec![Tab::Doc(diff("x"))]]);
        assert_eq!((w.tree.focused, at.1), (at.0, 0));
    }

    #[test]
    fn each_doc_gets_one_tab_across_panes() {
        let mut w = with(&["a"]);
        let (d, _) = w.open_doc(file("x"), true, None);
        w.open_doc(diff("y"), true, Some(0));
        assert_eq!(w.open_doc(file("x"), true, Some(0)), (d, 0));
        assert_eq!(w.tree.focused, d);
        assert_eq!(tabs(&w), vec![vec![term("a"), Tab::Doc(diff("y"))], vec![Tab::Doc(file("x"))]]);
    }

    #[test]
    fn an_unpinned_open_takes_over_the_preview_tab_in_its_own_pane_only() {
        let mut w = with(&["a"]);
        let (d, _) = w.open_doc(file("x"), false, None);
        w.open_doc(diff("y"), false, Some(d));
        assert_eq!(tabs(&w)[1], vec![Tab::Doc(diff("y"))]);
        w.open_doc(diff("z"), false, Some(0));
        assert_eq!(tabs(&w), vec![vec![term("a"), Tab::Doc(diff("z"))], vec![Tab::Doc(diff("y"))]]);
        assert_eq!(w.preview, Some(diff("z")));
    }

    #[test]
    fn a_pinned_tab_stays_when_the_next_preview_opens() {
        let mut w = with(&["a"]);
        let (d, _) = w.open_doc(file("x"), false, None);
        w.pin(&file("x"));
        w.open_doc(diff("y"), false, Some(d));
        assert_eq!(tabs(&w)[1], vec![Tab::Doc(file("x")), Tab::Doc(diff("y"))]);
    }

    #[test]
    fn a_pinned_open_leaves_the_preview_tab_alone() {
        let mut w = with(&["a"]);
        let (d, _) = w.open_doc(file("x"), false, None);
        assert_eq!(w.open_doc(file("y"), true, Some(d)), (d, 1));
        assert_eq!((tabs(&w)[1].len(), w.preview.clone()), (2, Some(file("x"))));
        w.open_doc(file("x"), true, Some(d));
        assert_eq!((shown(&w), w.preview.clone()), (Some(&Tab::Doc(file("x"))), None));
    }

    #[test]
    fn reopening_a_shown_doc_shows_its_tab_without_pinning_or_replacing() {
        let mut w = with(&["a"]);
        let (d, _) = w.open_doc(file("x"), true, None);
        w.open_doc(diff("y"), false, Some(d));
        w.open_doc(file("x"), false, Some(d));
        assert_eq!((tabs(&w)[1].len(), shown(&w), w.preview.clone()), (2, Some(&Tab::Doc(file("x"))), Some(diff("y"))));
    }

    #[test]
    fn closing_the_preview_tab_leaves_no_preview() {
        let mut w = with(&["a"]);
        let (d, _) = w.open_doc(file("x"), false, None);
        assert_eq!(w.close(d, 0), Some(Tab::Doc(file("x"))));
        assert_eq!((w.preview.clone(), tabs(&w)), (None, vec![vec![term("a")]]));
    }

    #[test]
    fn a_web_tab_opens_shown_in_the_focused_pane() {
        let mut w = with(&["a"]);
        let (d, _) = w.open_doc(file("x"), true, None);
        w.open_web(7);
        assert_eq!((w.tree.focused, shown(&w)), (d, Some(&Tab::Web(7))));
    }

    #[test]
    fn removing_terminals_closes_the_panes_they_emptied() {
        let mut w = with(&["a", "b"]);
        w.open_term("c".into(), Place::Split(0, Edge::Right));
        w.tree.select(0, 1);
        w.remove(&ids(&["c", "a"]));
        assert_eq!((tabs(&w), w.tree.focused, shown(&w)), (vec![vec![term("b")]], 0, Some(&term("b"))));
    }

    #[test]
    fn only_the_zoomed_pane_shows() {
        let mut w = with(&["a"]);
        w.open_term("b".into(), Place::Split(0, Edge::Right));
        let b = w.tree.focused;
        assert_eq!(w.shown(), vec![(0, &term("a")), (b, &term("b"))]);
        w.tree.toggle_zoom();
        assert_eq!(w.shown(), vec![(b, &term("b"))]);
    }

    #[test]
    fn finds_the_tab_holding_a_terminal() {
        let mut w = with(&["a", "b"]);
        w.open_term("c".into(), Place::Split(0, Edge::Bottom));
        let c = w.tree.focused;
        assert_eq!((w.tab_of("c"), w.tab_of("b"), w.tab_of("x")), (Some((c, 0)), Some((0, 1)), None));
    }
}
```

**Step 2: Run them**

Run: `cargo test -p workspace workspace::tests`
Expected: FAIL to compile (`Place`, `open_term`, `tree` are unknown).

**Step 3: Implement**

Replace everything from `pub mod tree;` down to `#[cfg(test)]` with:

```rust
pub mod tree;

use tree::{Edge, PaneId, Tree};

/// What a doc tab shows: a file by its absolute path, a file's changes by its path in the worktree, a file's changes in commit `sha`, or every change of a commit.
#[derive(Clone, Debug, PartialEq)]
pub enum Doc {
    File(String),
    Diff(String),
    CommitFile { sha: String, path: String },
    Commit(String),
}

/// A tab of one worktree: a pocketd terminal by its id, a doc, or a browser page by the app's id for it.
#[derive(Clone, Debug, PartialEq)]
pub enum Tab {
    Term(String),
    Doc(Doc),
    Web(u64),
}

/// Where a new terminal opens: in a pane, the terminal pane nearest the focus when none is given or it is gone, or in a new pane at an edge of one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    Pane(Option<PaneId>),
    Split(PaneId, Edge),
}

#[derive(Debug, PartialEq, Default)]
pub struct Workspace {
    pub tree: Tree<Tab>,
    /// The doc whose tab the next unpinned open in its pane takes over.
    pub preview: Option<Doc>,
}

impl Workspace {
    /// Gives each of `mine` not yet shown a tab in the top-left pane, leaving the focus alone, and drops terminals of `theirs`; unknown terminals stay, since a just-spawned terminal is not listed yet.
    pub fn sync(&mut self, mine: &[String], theirs: &[String]) {
        self.remove(theirs);
        let first = self.tree.panes()[0].id;
        for id in mine {
            if self.tab_of(id).is_none() {
                self.tree.push(first, Tab::Term(id.clone()));
            }
        }
    }

    /// Selects the tab showing terminal `id`, giving it one at `place` if none does.
    pub fn open_term(&mut self, id: String, place: Place) {
        if let Some((pane, i)) = self.tab_of(&id) {
            return self.tree.select(pane, i);
        }
        let tab = Tab::Term(id);
        match place {
            Place::Split(pane, edge) if self.tree.pane(pane).is_some() => _ = self.tree.split(pane, edge, tab),
            Place::Pane(Some(pane)) if self.tree.pane(pane).is_some() => self.show(pane, tab),
            _ => self.show(self.term_pane(), tab),
        }
    }

    fn show(&mut self, pane: PaneId, tab: Tab) {
        if let Some(i) = self.tree.push(pane, tab) {
            self.tree.select(pane, i);
        }
    }

    /// The pane nearest the focus that holds a terminal, else the focused pane.
    pub fn term_pane(&self) -> PaneId {
        self.tree.nearest(self.tree.focused, |p| p.tabs.iter().any(|t| matches!(t, Tab::Term(_)))).unwrap_or(self.tree.focused)
    }

    /// The pane a doc opens in: the focused pane when empty, else the doc pane nearest the focus; `None` when no pane holds a doc.
    pub fn doc_pane(&self) -> Option<PaneId> {
        if self.tree.focused().tabs.is_empty() {
            return Some(self.tree.focused);
        }
        self.tree.nearest(self.tree.focused, |p| p.tabs.iter().any(|t| matches!(t, Tab::Doc(_))))
    }

    /// Shows `doc` and returns its tab. A doc shown in any pane is selected there. Otherwise it takes over the preview tab if that is in `pane` and `pin` is off, else gets a tab in `pane`, else, with no `pane`, a new pane right of the focused one.
    pub fn open_doc(&mut self, doc: Doc, pin: bool, pane: Option<PaneId>) -> (PaneId, usize) {
        let shown = self.doc_tab(&doc);
        let pane = pane.filter(|p| self.tree.pane(*p).is_some());
        let preview = self.preview.as_ref().and_then(|p| self.doc_tab(p)).filter(|(p, _)| !pin && Some(*p) == pane);
        let tab = Tab::Doc(doc.clone());
        let (p, i) = match (shown, preview, pane) {
            (Some(at), _, _) => at,
            (None, Some((p, i)), _) => {
                self.tree.pane_mut(p).expect("the preview's pane").tabs[i] = tab;
                (p, i)
            }
            (None, None, Some(p)) => (p, self.tree.push(p, tab).expect("checked above")),
            (None, None, None) => (self.tree.split(self.tree.focused, Edge::Right, tab).expect("the focused pane is in the tree"), 0),
        };
        self.tree.select(p, i);
        if pin {
            self.pin(&doc);
        } else if shown.is_none() {
            self.preview = Some(doc);
        }
        (p, i)
    }

    /// Keeps `doc`'s tab from being taken over by the next preview.
    pub fn pin(&mut self, doc: &Doc) {
        if self.preview.as_ref() == Some(doc) {
            self.preview = None;
        }
    }

    /// Gives page `id` a tab in the focused pane and shows it.
    pub fn open_web(&mut self, id: u64) {
        self.show(self.tree.focused, Tab::Web(id));
    }

    pub fn doc_tab(&self, doc: &Doc) -> Option<(PaneId, usize)> {
        self.tree.find(|t| matches!(t, Tab::Doc(d) if d == doc))
    }

    pub fn tab_of(&self, id: &str) -> Option<(PaneId, usize)> {
        self.tree.find(|t| matches!(t, Tab::Term(t) if t == id))
    }

    /// Drops the tabs of terminals `ids`, closing the panes they leave empty.
    pub fn remove(&mut self, ids: &[String]) {
        self.tree.retain(|t| !matches!(t, Tab::Term(id) if ids.contains(id)));
    }

    /// Takes tab `i` out of `pane`; a closed doc stops being the preview.
    pub fn close(&mut self, pane: PaneId, i: usize) -> Option<Tab> {
        let tab = self.tree.take(pane, i)?;
        if let Tab::Doc(doc) = &tab {
            self.pin(doc);
        }
        Some(tab)
    }

    /// The tab each drawn pane shows: every pane's, or only the zoomed pane's.
    pub fn shown(&self) -> Vec<(PaneId, &Tab)> {
        let drawn = |id: PaneId| self.tree.zoomed.is_none_or(|z| z == id);
        self.tree.panes().into_iter().filter(|p| drawn(p.id)).filter_map(|p| Some((p.id, p.active()?))).collect()
    }
}
```

**Step 4: Run them**

Run: `cargo test -p workspace`
Expected: PASS, 42 tests.

`pocket` no longer compiles at this point. It compiles again at the end of Task 4.6, so Tasks 4.2–4.5 have no test runs of their own.

### Task 4.2: Terminals open at a place

**Files:**
- Modify: `packages/desktop/crates/pocket/src/terminals.rs`. Changes:
  - imports
  - `Intent`
  - `spawned`
  - the arrived/spawned handlers
  - `adopt`
  - `new_shell`, `new_agent_tab`, `run_in_tree`
  - `close_pane`, `close_tab`
  - tests

**Context:**
- An intent now carries a `Place` instead of the old "split down?" flag.
- `adopt(id, tree, place, window, cx)` calls `Workspace::open_term`.
- `new_shell(Place, cx)` and `new_agent_tab(provider, Place, cx)` no longer touch menu state; their callers close the menu.
- `close_tab(pane, i, cx)` closes tab `i` of `pane`.

**Step 1: Apply**

```diff
diff --git a/packages/desktop/crates/pocket/src/terminals.rs b/packages/desktop/crates/pocket/src/terminals.rs
index 5831367..e38588b 100644
--- a/packages/desktop/crates/pocket/src/terminals.rs
+++ b/packages/desktop/crates/pocket/src/terminals.rs
@@ -12,10 +12,10 @@ use serde_json::json;
 use std::collections::{HashMap, HashSet, VecDeque};
 use std::time::{Duration, Instant};
-use workspace::{Doc, Tab};
+use workspace::tree::PaneId;
+use workspace::{Doc, Place, Tab};
 
-pub(crate) enum Intent {
-    Tab(String),
-    Split(String, bool),
-}
+/// A spawn awaiting its terminal: the worktree to adopt it in, and where its tab goes.
+#[derive(Debug, PartialEq)]
+pub(crate) struct Intent(pub(crate) String, pub(crate) Place);
 
 /// What a pane's measured size asks of pocketd.
@@ -89,10 +89,7 @@ impl Terminals {
     }
 
-    /// Matches a spawned terminal to the oldest pending intent; returns the worktree to adopt it in and the split direction, if any.
-    pub(crate) fn spawned(&mut self) -> Option<(String, Option<bool>)> {
-        match self.intents.pop_front()? {
-            Intent::Tab(tree) => Some((tree, None)),
-            Intent::Split(tree, down) => Some((tree, Some(down))),
-        }
+    /// Matches a spawned terminal to the oldest pending intent.
+    pub(crate) fn spawned(&mut self) -> Option<Intent> {
+        self.intents.pop_front()
     }
 
@@ -185,5 +182,5 @@ impl Desktop {
                 }
                 for (id, tree) in self.terminals.arrived() {
-                    self.adopt(id, tree, None, window, cx);
+                    self.adopt(id, tree, Place::Pane(None), window, cx);
                 }
                 if self.project.is_none() {
@@ -193,6 +190,6 @@ impl Desktop {
             "spawned" => {
                 self.error = None;
-                if let Some((tree, split)) = self.terminals.spawned() {
-                    self.adopt(m.id, tree, split, window, cx);
+                if let Some(Intent(tree, place)) = self.terminals.spawned() {
+                    self.adopt(m.id, tree, place, window, cx);
                 }
                 self.daemon.send(json!({"op": "list"}));
@@ -225,10 +222,6 @@ impl Desktop {
     }
 
-    fn adopt(&mut self, id: String, tree: String, split: Option<bool>, window: &mut Window, cx: &mut Context<Self>) {
-        let w = self.workspace(&tree);
-        match split {
-            Some(down) => w.split(id.clone(), down),
-            None => w.add_tab(id.clone()),
-        }
+    fn adopt(&mut self, id: String, tree: String, place: Place, window: &mut Window, cx: &mut Context<Self>) {
+        self.workspace(&tree).open_term(id.clone(), place);
         self.show_tree(&tree, &tree);
         self.focus_pane(id, window, cx);
@@ -245,22 +238,18 @@ impl Desktop {
     }
 
-    /// Opens a login shell in the worktree's folder, as a new tab or a split of the active one.
-    pub fn new_shell(&mut self, split: Option<bool>, cx: &mut Context<Self>) {
-        self.run_in_tree(split, daemon::shell_op, cx);
+    /// Opens a login shell in the worktree's folder, its tab at `place`.
+    pub fn new_shell(&mut self, place: Place, cx: &mut Context<Self>) {
+        self.run_in_tree(place, daemon::shell_op, cx);
     }
 
-    pub fn new_agent_tab(&mut self, provider: &str, cx: &mut Context<Self>) {
-        self.terminal.tab_menu = false;
+    pub fn new_agent_tab(&mut self, provider: &str, place: Place, cx: &mut Context<Self>) {
         let argv = [provider.to_string()];
-        self.run_in_tree(None, |cwd| daemon::agent_op(&argv, cwd), cx);
+        self.run_in_tree(place, |cwd| daemon::agent_op(&argv, cwd), cx);
     }
 
-    fn run_in_tree(&mut self, split: Option<bool>, op: impl FnOnce(&str) -> serde_json::Value, cx: &mut Context<Self>) {
+    fn run_in_tree(&mut self, place: Place, op: impl FnOnce(&str) -> serde_json::Value, cx: &mut Context<Self>) {
         let Some(tree) = self.cwd() else { return };
-        let intent = match split {
-            Some(down) => Intent::Split(tree.clone(), down),
-            None => Intent::Tab(tree.clone()),
-        };
-        self.send_spawn(op(&tree), intent, cx);
+        let op = op(&tree);
+        self.send_spawn(op, Intent(tree, place), cx);
     }
 
@@ -272,6 +261,7 @@ impl Desktop {
             self.daemon.send(json!({"op": "close", "id": id}));
         }
+        let ids = [id.to_string()];
         for w in self.workspaces.values_mut() {
-            w.remove(id);
+            w.remove(&ids);
         }
         self.load_active(cx);
@@ -291,8 +281,8 @@ impl Desktop {
     }
 
-    pub fn close_tab(&mut self, i: usize, cx: &mut Context<Self>) {
+    pub fn close_tab(&mut self, pane: PaneId, i: usize, cx: &mut Context<Self>) {
         let Some(tree) = self.cwd() else { return };
-        let ids = match self.workspace(&tree).tabs.get(i).cloned() {
-            Some(Tab::Term(rows)) => rows.concat(),
+        let ids = match self.workspace(&tree).tree.pane(pane).and_then(|p| p.tabs.get(i)).cloned() {
+            Some(Tab::Term(id)) => vec![id],
             Some(Tab::Doc(Doc::File(path))) if self.preview.dirty(&path) => {
                 self.confirm = Some(Confirm::CloseFile(path));
@@ -304,8 +294,9 @@ impl Desktop {
         };
         let observe = self.agents.observe_only();
-        if !ids.iter().all(|id| self.terminals.may_close(id, observe)) || self.ask_close(ids, cx) {
+        if !ids.iter().all(|id| self.terminals.may_close(id, observe)) || self.ask_close(ids.clone(), cx) {
             return;
         }
-        for id in self.workspace(&tree).close_tab(i) {
+        self.workspace(&tree).close(pane, i);
+        for id in ids {
             self.close_pane(&id, cx);
         }
@@ -344,4 +335,6 @@ mod tests {
     use daemon::{Info, Msg};
     use std::collections::HashMap;
+    use workspace::Place;
+    use workspace::tree::Edge;
 
     fn info(id: &str) -> Info {
@@ -360,6 +353,6 @@ mod tests {
     fn spawned_terminals_open_as_the_tab_asked_for() {
         let mut t = Terminals::new();
-        t.intents.push_back(Intent::Tab("/w".into()));
-        assert_eq!(t.spawned(), Some(("/w".to_string(), None)));
+        t.intents.push_back(Intent("/w".into(), Place::Pane(None)));
+        assert_eq!(t.spawned(), Some(Intent("/w".into(), Place::Pane(None))));
     }
 
@@ -367,8 +360,8 @@ mod tests {
     fn spawned_terminals_take_intents_in_the_order_they_were_sent() {
         let mut t = Terminals::new();
-        t.intents.push_back(Intent::Split("/w".into(), true));
-        t.intents.push_back(Intent::Split("/v".into(), false));
-        assert_eq!(t.spawned(), Some(("/w".to_string(), Some(true))));
-        assert_eq!(t.spawned(), Some(("/v".to_string(), Some(false))));
+        t.intents.push_back(Intent("/w".into(), Place::Split(0, Edge::Bottom)));
+        t.intents.push_back(Intent("/v".into(), Place::Split(0, Edge::Right)));
+        assert_eq!(t.spawned(), Some(Intent("/w".into(), Place::Split(0, Edge::Bottom))));
+        assert_eq!(t.spawned(), Some(Intent("/v".into(), Place::Split(0, Edge::Right))));
     }
 
@@ -383,9 +376,9 @@ mod tests {
     fn a_failed_spawn_drops_its_intent_but_a_terminal_error_does_not() {
         let mut t = Terminals::new();
-        t.intents.push_back(Intent::Tab("/w".into()));
-        t.intents.push_back(Intent::Tab("/v".into()));
+        t.intents.push_back(Intent("/w".into(), Place::Pane(None)));
+        t.intents.push_back(Intent("/v".into(), Place::Pane(None)));
         t.errored("a");
         t.errored("");
-        assert_eq!(t.spawned(), Some(("/v".to_string(), None)));
+        assert_eq!(t.spawned(), Some(Intent("/v".into(), Place::Pane(None))));
     }
 
@@ -526,5 +519,5 @@ mod tests {
     fn losing_pocketd_drops_spawns_it_never_answered() {
         let mut t = Terminals::new();
-        t.intents.push_back(Intent::Tab("/w".into()));
+        t.intents.push_back(Intent("/w".into(), Place::Pane(None)));
         t.disconnected(std::time::Instant::now());
         assert_eq!(t.spawned(), None);
@@ -535,7 +528,7 @@ mod tests {
         let mut t = Terminals::new();
         t.disconnected(std::time::Instant::now());
-        t.spawn_sent(true, Intent::Tab("/w".into()));
+        t.spawn_sent(true, Intent("/w".into(), Place::Pane(None)));
         t.reconnected();
-        assert_eq!(t.spawned(), Some(("/w".to_string(), None)));
+        assert_eq!(t.spawned(), Some(Intent("/w".into(), Place::Pane(None))));
     }
 
@@ -551,7 +544,7 @@ mod tests {
     fn failed_spawn_send_records_no_intent() {
         let mut t = Terminals::new();
-        t.spawn_sent(false, Intent::Tab("/w".into()));
-        t.spawn_sent(true, Intent::Tab("/v".into()));
-        assert_eq!(t.spawned(), Some(("/v".to_string(), None)));
+        t.spawn_sent(false, Intent("/w".into(), Place::Pane(None)));
+        t.spawn_sent(true, Intent("/v".into(), Place::Pane(None)));
+        assert_eq!(t.spawned(), Some(Intent("/v".into(), Place::Pane(None))));
     }
```

### Task 4.3: `Panels`, the main area's state and views

**Files:**
- Create: `packages/desktop/crates/pocket/src/panels.rs`
- Modify: `packages/desktop/crates/pocket/src/main.rs` (`mod panels;` after `mod palette;`)
- Modify: `packages/desktop/crates/pocket/src/desktop/chrome.rs:24` (`SEAM` becomes `pub(crate)`)

**Context:**
- `Panels` holds what the panes need besides the tree:
  - each strip's scroll
  - which pane's + menu or tab menu is open
  - the tab held on a strip
  - where ⋯ was clicked
  - the main area's bounds, which `can_split` needs
- `panels_view` draws the tree:
  - A `Split` is a flex row or column of cells sized by `flex_basis(relative(size))`.
  - Each cell after the first gets a 0.5px border and a deferred divider handle, which mirrors `Desktop::resizable` in `chrome.rs`.
  - A double click on a divider evens out its split.
- Each pane is `id("pane:{p}")`. That makes the ids inside it (`tab`, `new-tab`, `split-right`…) unique per pane.
- A pane on the top edge has a bar that moves the window. Only the top-left pane starts with the sidebar toggle and its traffic-light padding (`edges`).
- A capture-phase mouse-down focuses a pane, and the keys follow:
  - a terminal: `focus_pane`
  - a page: the browser focus
  - otherwise: the root
- `sync_panels` runs first in `render`. It points `terminal.focused` at the focused pane's terminal, and drops strips and the + menu of closed panes.
- `panels.bounds` is measured by a canvas, so it lags one frame. The split buttons are dimmed on the very first frame.

**Step 1: Write the failing test.** It is the `mod tests` at the bottom of the file in Step 2.

**Step 2: Create `panels.rs`**

```rust
mod strip;
mod tab_actions;
mod tab_menu;

use crate::desktop::Desktop;
use crate::desktop::chrome::{Overlay, SEAM, drag_area, id, observe_banner};
use crate::terminal_view::context;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::collections::HashMap;
use theme::*;
use workspace::tree::{Axis, Divider, Edge, Node, Pane, PaneId, Rect, Target, UNIT};
use workspace::{Doc, Place, Tab};

/// A pane's tab strip.
#[derive(Default)]
pub(crate) struct Strip {
    scroll: ScrollHandle,
    /// The worktree and tab last scrolled into view, so a tab is revealed once when it becomes active rather than every frame.
    revealed: Option<(String, usize)>,
}

/// The main area's panes: their strips, menus and the tab held on one.
#[derive(Default)]
pub(crate) struct Panels {
    strips: HashMap<PaneId, Strip>,
    /// The pane whose + menu is open.
    pub(crate) menu: Option<PaneId>,
    /// The tab whose right-click menu is open, its pane, and where the click was.
    pub(crate) actions: Option<(PaneId, Tab, Point<Pixels>)>,
    /// The tab held on a strip, and the strip's pane.
    drag: Option<(PaneId, strip::TabDrag)>,
    /// Where the click that opened the More menu was.
    pub(crate) more_at: Option<Point<Pixels>>,
    /// The main area as last drawn, in window pixels; which panes have room to split depends on it.
    pub(crate) bounds: Rect,
}

/// Whether `pane` touches the top of the main area, where its bar also moves the window, and its left, where the bar starts with the sidebar toggle.
pub(crate) fn edges(layout: &[(PaneId, Rect)], pane: PaneId) -> (bool, bool) {
    layout.iter().find(|(p, _)| *p == pane).map_or((true, true), |(_, r)| (r.y < 1e-4, r.x < 1e-4))
}

pub(crate) fn rect(b: Bounds<Pixels>) -> Rect {
    Rect { x: b.origin.x.into(), y: b.origin.y.into(), w: b.size.width.into(), h: b.size.height.into() }
}

fn empty_pane() -> Div {
    div()
        .flex_1()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(px(6.))
        .child(div().text_size(px(13.)).text_color(TEXT_2).child("No open tabs"))
        .child(div().text_size(px(12.5)).text_color(TEXT_3).child("Use + to start a shell or an agent"))
}

impl Desktop {
    /// Points keys at the focused pane's terminal, and forgets what closed panes held.
    pub(crate) fn sync_panels(&mut self) {
        let Some(tree) = self.session_tree() else { return };
        let w = self.workspace(&tree);
        let focused = match w.tree.focused().active() {
            Some(Tab::Term(id)) => Some(id.clone()),
            _ => None,
        };
        let live: Vec<PaneId> = w.tree.panes().iter().map(|p| p.id).collect();
        self.terminal.focused = focused;
        self.panels.strips.retain(|p, _| live.contains(p));
        if self.panels.menu.is_some_and(|p| !live.contains(&p)) {
            self.panels.menu = None;
        }
    }

    /// Worktree `tree`'s panes: all of them, or only the zoomed one.
    pub(crate) fn panels_view(&mut self, tree: &str, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let t = &self.workspace(tree).tree;
        let (root, zoomed) = (t.root.clone(), t.zoomed.and_then(|z| t.pane(z)).cloned());
        let layout = match &zoomed {
            Some(p) => vec![(p.id, UNIT)],
            None => t.layout(UNIT),
        };
        let body = match zoomed {
            Some(p) => self.pane_view(tree, &p, &layout, window, cx).into_any_element(),
            None => self.node_view(tree, &root, Vec::new(), &layout, window, cx),
        };
        let this = cx.weak_entity();
        let measure = canvas(
            move |b, _, cx| {
                this.update(cx, |this, _| this.panels.bounds = rect(b)).ok();
            },
            |_, _, _, _| {},
        )
        .absolute()
        .size_full();
        let observe = self.agents.observe_only();
        div()
            .flex_1()
            .min_h_0()
            .flex()
            .flex_col()
            .bg(SURFACE_SUNKEN)
            .child(div().relative().flex_1().min_h_0().flex().child(measure).child(body))
            .when(observe, |d| d.child(observe_banner()))
    }

    fn node_view(&mut self, tree: &str, node: &Node<Tab>, path: Vec<usize>, layout: &[(PaneId, Rect)], window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let (axis, sizes, children) = match node {
            Node::Pane(p) => return self.pane_view(tree, p, layout, window, cx).into_any_element(),
            Node::Split { axis, sizes, children } => (*axis, sizes, children),
        };
        let mut cells = Vec::new();
        for (j, (child, &size)) in children.iter().zip(sizes).enumerate() {
            let inner = self.node_view(tree, child, [path.as_slice(), &[j]].concat(), layout, window, cx);
            let cell = div().relative().flex().flex_none().flex_basis(relative(size)).min_w_0().min_h_0().child(inner);
            cells.push(match (j, axis) {
                (0, _) => cell,
                (_, Axis::Row) => cell.border_l(px(0.5)).border_color(SEPARATOR).children(self.divider(&path, j, axis, cx)),
                (_, Axis::Column) => cell.border_t(px(0.5)).border_color(SEPARATOR).children(self.divider(&path, j, axis, cx)),
            });
        }
        div()
            .size_full()
            .flex()
            .when(axis == Axis::Column, |d| d.flex_col())
            .children(cells)
            .on_drag_move(cx.listener(move |this, e: &DragMoveEvent<Divider>, _, cx| {
                let d = e.drag(cx).clone();
                if d.path != path {
                    return;
                }
                let (at, len) = match axis {
                    Axis::Row => (e.event.position.x - e.bounds.left(), e.bounds.size.width),
                    Axis::Column => (e.event.position.y - e.bounds.top(), e.bounds.size.height),
                };
                this.resize_split(&d, at.into(), len.into(), cx);
            }))
            .into_any_element()
    }

    /// The seam before cell `j` of the split at `path`: drag it to resize, double-click it to even the split out.
    fn divider(&self, path: &[usize], j: usize, axis: Axis, cx: &mut Context<Self>) -> Option<Deferred> {
        // The deferred handle paints above overlays, so it would steal their clicks.
        if self.overlay.is_some() {
            return None;
        }
        let line = div().absolute().group_hover("seam", |s| s.bg(SEPARATOR_STRONG));
        let handle = div().id(id(format!("divider:{path:?}:{j}"))).group("seam").absolute().occlude();
        let (line, handle) = match axis {
            Axis::Row => (line.top_0().bottom_0().left(px(SEAM / 2.)).w(px(1.)), handle.top_0().bottom_0().left(px(-SEAM / 2.)).w(px(SEAM)).cursor_col_resize()),
            Axis::Column => (line.left_0().right_0().top(px(SEAM / 2.)).h(px(1.)), handle.left_0().right_0().top(px(-SEAM / 2.)).h(px(SEAM)).cursor_row_resize()),
        };
        let at = path.to_vec();
        let handle = handle
            .child(line)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                    if e.click_count == 2 {
                        this.equalize_split(&at, cx);
                    }
                }),
            )
            .on_drag(Divider { path: path.to_vec(), i: j - 1 }, |_, _, _, cx| {
                cx.stop_propagation();
                cx.new(|_| EmptyView)
            });
        Some(deferred(handle))
    }

    fn resize_split(&mut self, d: &Divider, at: f32, len: f32, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        let t = &mut self.workspace(&tree).tree;
        let before = t.layout(UNIT);
        t.resize(d, at, len);
        // A drag move comes every frame, moved or not; notifying on each would redraw forever.
        if t.layout(UNIT) != before {
            cx.notify();
        }
    }

    fn equalize_split(&mut self, path: &[usize], cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        self.workspace(&tree).tree.equalize(path);
        cx.notify();
    }

    fn pane_view(&mut self, tree: &str, pane: &Pane<Tab>, layout: &[(PaneId, Rect)], window: &mut Window, cx: &mut Context<Self>) -> Stateful<Div> {
        let p = pane.id;
        let (top, left) = edges(layout, p);
        let (pad, toggle) = if top && left { self.bar_start(cx) } else { (10., None) };
        let ring = match pane.active() {
            Some(Tab::Term(t)) => self.summary(t).and_then(|a| a.context()).map(|(used, window)| context::ring(used, window)),
            _ => None,
        };
        let status = div().ml_auto().pl(px(8.)).min_w_0().flex().items_center().children(ring);
        let observe = self.agents.observe_only();
        let t = &self.workspaces[tree].tree;
        let focused = t.focused == p;
        let can = [Edge::Right, Edge::Bottom].map(|edge| t.can_split(p, edge, self.panels.bounds));
        let split = |name: &'static str, edge: Edge, can: bool, cx: &mut Context<Self>| {
            let b = ui::group_button(name, name);
            if can { b.on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.new_shell(Place::Split(p, edge), cx))) } else { b.opacity(0.35) }
        };
        let right = div()
            .flex()
            .flex_none()
            .items_center()
            .gap(px(8.))
            .when(!observe, |d| d.child(ui::icon_group([split("split-right", Edge::Right, can[0], cx), split("split-down", Edge::Bottom, can[1], cx)])))
            .child(ui::icon_group([ui::group_button("session-more", "more").on_click(cx.listener(|this, e: &ClickEvent, window, cx| this.open_more(e.position(), window, cx)))]));
        let bar = div()
            .h(px(42.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(pad))
            .pr(px(10.))
            .children(toggle)
            .child(self.strip_tabs(tree, pane, window, cx))
            .child(status)
            .child(right);
        let body = self.pane_body(p, focused, pane.active().cloned(), window, cx);
        div()
            .id(id(format!("pane:{p}")))
            .size_full()
            .min_w_0()
            .min_h_0()
            .flex()
            .flex_col()
            .overflow_hidden()
            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, window, cx| this.focus_panel(p, window, cx)))
            .child(if top { drag_area(bar) } else { bar })
            .child(body)
    }

    fn pane_body(&mut self, p: PaneId, focused: bool, tab: Option<Tab>, window: &mut Window, cx: &mut Context<Self>) -> Div {
        match tab {
            Some(Tab::Term(id)) => self.term_body(&id, focused, cx),
            Some(Tab::Doc(doc @ Doc::File(_))) if self.loaded(p, &doc) => self.file_view(p, cx),
            Some(Tab::Doc(doc @ (Doc::Diff(_) | Doc::CommitFile { .. }))) if self.loaded(p, &doc) => self.diff_view(p, cx),
            Some(Tab::Doc(doc @ Doc::Commit(_))) if self.loaded(p, &doc) => self.commit_view(p, cx),
            Some(Tab::Web(id)) => self.browser_view(p, id, window, cx),
            Some(Tab::Doc(_)) => div().flex_1(),
            None => empty_pane(),
        }
    }

    /// Opens the More menu by the click at `at`.
    pub(crate) fn open_more(&mut self, at: Point<Pixels>, window: &mut Window, cx: &mut Context<Self>) {
        self.panels.more_at = Some(at);
        self.open(Overlay::More, window, cx);
    }

    /// Moves the focus to pane `p`, and the keys with it.
    fn focus_panel(&mut self, p: PaneId, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        let t = &mut self.workspace(&tree).tree;
        if t.focused == p {
            return;
        }
        t.focus(p);
        match t.focused().active().cloned() {
            Some(Tab::Term(id)) => self.focus_pane(id, window, cx),
            Some(Tab::Web(_)) => window.focus(&self.browsers.focus, cx),
            Some(Tab::Doc(_)) | None => window.focus(&self.root, cx),
        }
        cx.notify();
    }

    /// Moves tab `i` of `pane` to `to`, then shows what the focused pane shows and loads what the moved tab needs.
    pub(crate) fn move_tab_to(&mut self, pane: PaneId, i: usize, to: Target, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        let t = &mut self.workspace(&tree).tree;
        t.move_tab(pane, i, to);
        let (focused, active) = (t.focused, t.focused().active);
        self.select_tab(focused, active, window, cx);
        self.load_active(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::edges;
    use workspace::tree::{Edge, Tree, UNIT};

    #[test]
    fn only_panes_on_the_top_edge_take_the_window_bar_and_only_the_top_left_one_its_controls() {
        let mut t = Tree::default();
        t.push(0, 1);
        let b = t.split(0, Edge::Right, 2).unwrap();
        let c = t.split(b, Edge::Bottom, 3).unwrap();
        let layout = t.layout(UNIT);
        assert_eq!([0, b, c].map(|p| edges(&layout, p)), [(true, true), (true, false), (false, false)]);
    }
}
```

```diff
diff --git a/packages/desktop/crates/pocket/src/main.rs b/packages/desktop/crates/pocket/src/main.rs
index edce883..c376a74 100644
--- a/packages/desktop/crates/pocket/src/main.rs
+++ b/packages/desktop/crates/pocket/src/main.rs
@@ -9,4 +9,5 @@ mod inbox;
 mod modals;
 mod palette;
+mod panels;
 mod sidebar;
 mod status;
diff --git a/packages/desktop/crates/pocket/src/desktop/chrome.rs b/packages/desktop/crates/pocket/src/desktop/chrome.rs
index 3852de8..2b4fc4c 100644
--- a/packages/desktop/crates/pocket/src/desktop/chrome.rs
+++ b/packages/desktop/crates/pocket/src/desktop/chrome.rs
@@ -22,5 +22,5 @@ pub enum Side {
 pub use store::Layout;
 
-const SEAM: f32 = 20.;
+pub(crate) const SEAM: f32 = 20.;
 
 /// A sidebar column whose right edge the user drags.
```

### Task 4.4: Each pane's tab strip

**Files:**
- Move (`git mv`):
  - `terminal_view/tabs.rs` → `panels/strip.rs`
  - `terminal_view/tab_menu.rs` → `panels/tab_menu.rs`
  - `terminal_view/tab_actions.rs` → `panels/tab_actions.rs`

  All are under `packages/desktop/crates/pocket/src/`.
- Modify all three files.

**Context:**
- `term_tabs(tree)` becomes `strip_tabs(tree, &Pane<Tab>)`.
- Its scroll and reveal come from `panels.strips[p]`. The in-strip drag is stored with its pane, and every click passes the pane.
- `tab_label` and its test go, since a tab no longer holds several panes.
- The selected tab of the focused pane keeps today's raised look. The selected tab of any other pane gets `FILL_2`, so you can see where the focus is.
- `new_tab_controls(pane)` opens that pane's + menu, and the menu's items open there (`Place::Pane(Some(pane))`).
- The right-click menu gains "Move to new split right / down" when the pane has room (`can_split`). It calls `move_tab_to` (in `panels.rs`).

**Step 1: Apply**

```diff
diff --git a/packages/desktop/crates/pocket/src/terminal_view/tabs.rs b/packages/desktop/crates/pocket/src/panels/strip.rs
similarity index 83%
rename from packages/desktop/crates/pocket/src/terminal_view/tabs.rs
rename to packages/desktop/crates/pocket/src/panels/strip.rs
index 501be46..142ec83 100644
--- a/packages/desktop/crates/pocket/src/terminal_view/tabs.rs
+++ b/packages/desktop/crates/pocket/src/panels/strip.rs
@@ -9,10 +9,7 @@ use std::time::Instant;
 use theme::*;
 use ui::{self, dot};
+use workspace::tree::{Pane, PaneId, Target};
 use workspace::{Doc, Tab};
 
-pub fn tab_label(text: String, panes: usize) -> String {
-    if panes > 1 { format!("{text} · {panes} panes") } else { text }
-}
-
 const GAP: f32 = 2.;
 /// How quickly a sliding tab closes on where it's going: about 90% of the way in 80ms.
@@ -121,6 +118,6 @@ impl Desktop {
         let row = div().flex().items_center().gap(px(7.));
         let label = |text: String| div().max_w(px(150.)).truncate().child(text);
-        let p = match tab {
-            Tab::Term(rows) => rows.concat(),
+        let term = match tab {
+            Tab::Term(term) => term,
             Tab::Web(id) => {
                 let b = self.browsers.tabs.get(id);
@@ -146,10 +143,9 @@ impl Desktop {
             }
         };
-        let count = |text: String| tab_label(text, p.len());
-        if let Some(a) = self.summary(&p[0]) {
+        if let Some(a) = self.summary(term) {
             let mark = ui::indicator(id(format!("tab-mark:{}", a.id)), Status::of(a).map(|s| state(s, 0, 0)));
-            return row.child(provider_icon(&a.provider, 13., ink)).child(label(count(provider_name(&a.provider).into()))).children(mark);
+            return row.child(provider_icon(&a.provider, 13., ink)).child(label(provider_name(&a.provider).into())).children(mark);
         }
-        let s = self.terminals.sessions.get(&p[0]);
+        let s = self.terminals.sessions.get(term);
         let busy = s.and_then(|s| s.busy());
         let mark = match s {
@@ -158,21 +154,23 @@ impl Desktop {
             _ => dot(6., TEXT_5).into_any_element(),
         };
-        let text = busy.map_or_else(|| self.pane_label(&p[0]), str::to_string);
-        row.child(icon("prompt", 13., TEXT_3)).child(label(count(text))).child(mark)
+        let text = busy.map_or_else(|| self.pane_label(term), str::to_string);
+        row.child(icon("prompt", 13., TEXT_3)).child(label(text)).child(mark)
     }
 
-    pub(crate) fn move_tab(&mut self, from: usize, to: usize, cx: &mut Context<Self>) {
+    /// Moves tab `from` of `pane` to `to` in the same strip.
+    pub(crate) fn move_tab(&mut self, pane: PaneId, from: usize, to: usize, cx: &mut Context<Self>) {
         let Some(tree) = self.cwd() else { return };
-        self.workspace(&tree).move_tab(from, to);
+        self.workspace(&tree).tree.move_tab(pane, from, Target::Into { pane, index: to });
         cx.notify();
     }
 
     fn release_tab(&mut self, cx: &mut Context<Self>) {
-        if let Some((from, to)) = self.terminal.tab_drag.as_mut().and_then(TabDrag::release) {
-            self.move_tab(from, to, cx);
+        if let Some((pane, (from, to))) = self.panels.drag.as_mut().and_then(|(p, d)| Some((*p, d.release()?))) {
+            self.move_tab(pane, from, to, cx);
         }
     }
 
-    pub(crate) fn new_tab_controls(&self, cx: &mut Context<Self>) -> Div {
+    fn new_tab_controls(&self, pane: PaneId, cx: &mut Context<Self>) -> Div {
+        let open = self.panels.menu == Some(pane);
         let plus = div()
             .id("new-tab")
@@ -185,41 +183,49 @@ impl Desktop {
             .rounded(px(7.))
             .cursor_pointer()
-            .when(self.terminal.tab_menu, |d| d.bg(FILL_3))
+            .when(open, |d| d.bg(FILL_3))
             .hover(|s| s.bg(FILL_3))
             .child(icon("plus", 15., TEXT_2))
             // Runs before the open menu's click-outside handler, which would otherwise close it only for this click to reopen it.
-            .capture_any_mouse_down(cx.listener(|this, _: &MouseDownEvent, _, cx| {
+            .capture_any_mouse_down(cx.listener(move |this, _: &MouseDownEvent, _, cx| {
                 cx.stop_propagation();
-                this.terminal.tab_menu = !this.terminal.tab_menu;
-                this.terminal.tab_actions = None;
+                this.panels.menu = if open { None } else { Some(pane) };
+                this.panels.actions = None;
                 this.row_menu = None;
                 cx.notify();
             }));
-        let menu = self.terminal.tab_menu.then(|| ui::dropdown(29., ui::menu_in("tab-menu-in", self.tab_menu_view(cx))));
+        let menu = open.then(|| ui::dropdown(29., ui::menu_in("tab-menu-in", self.tab_menu_view(pane, cx))));
         div().relative().flex().flex_none().items_center().child(plus).children(menu)
     }
 
-    pub(crate) fn term_tabs(&mut self, tree: &str, window: &mut Window, cx: &mut Context<Self>) -> Div {
-        let w = self.workspace(tree);
-        let active = w.active;
-        let tabs = w.tabs.clone();
+    /// `pane`'s tabs in worktree `tree`, with its + menu.
+    pub(super) fn strip_tabs(&mut self, tree: &str, pane: &Pane<Tab>, window: &mut Window, cx: &mut Context<Self>) -> Div {
+        let (p, active, tabs) = (pane.id, pane.active, &pane.tabs);
+        let w = &self.workspaces[tree];
+        let focused = w.tree.focused == p;
         let preview = w.preview.clone();
         let observe = self.agents.observe_only();
         let reduce_motion = cx.reduce_motion();
-        if let Some(d) = self.terminal.tab_drag.as_mut() {
-            let scroll = &self.terminal.tab_scroll;
+        let strip = self.panels.strips.entry(p).or_default();
+        let scroll = strip.scroll.clone();
+        let shown = Some((tree.to_string(), active));
+        if strip.revealed != shown {
+            scroll.scroll_to_item(active);
+            strip.revealed = shown;
+        }
+        if let Some((_, d)) = self.panels.drag.as_mut().filter(|(at, _)| *at == p) {
             let scrolled = f32::from(scroll.offset().x);
             let slots = (0..tabs.len()).map_while(|i| scroll.bounds_for_item(i)).map(|b| (f32::from(b.left()) + scrolled, f32::from(b.size.width))).collect();
             // gpui ends a drag on any mouse up, even one the strip never sees; a hold outliving it would keep its tab lifted.
             if !d.lay(slots) || !(d.released || cx.has_active_drag()) {
-                self.terminal.tab_drag = None;
+                self.panels.drag = None;
             } else if d.ease(Instant::now(), reduce_motion) {
                 window.request_animation_frame();
             } else if d.released {
-                self.terminal.tab_drag = None;
+                self.panels.drag = None;
             }
         }
-        let held = self.terminal.tab_drag.as_ref().and_then(TabDrag::held);
-        let offsets = self.terminal.tab_drag.as_ref().map(|d| d.offsets.clone()).unwrap_or_default();
+        let drag = self.panels.drag.as_ref().filter(|(at, _)| *at == p).map(|(_, d)| d);
+        let held = drag.and_then(TabDrag::held);
+        let offsets = drag.map(|d| d.offsets.clone()).unwrap_or_default();
         let items: Vec<_> = tabs
             .iter()
@@ -229,5 +235,5 @@ impl Desktop {
                 let selected = i == active;
                 let closable = match tab {
-                    Tab::Term(rows) => rows.iter().flatten().all(|id| self.terminals.may_close(id, observe)),
+                    Tab::Term(id) => self.terminals.may_close(id, observe),
                     Tab::Doc(_) | Tab::Web(_) => true,
                 };
@@ -248,5 +254,5 @@ impl Desktop {
                     .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                         cx.stop_propagation();
-                        this.close_tab(i, cx);
+                        this.close_tab(p, i, cx);
                     }));
                 let doc = match tab {
@@ -264,5 +270,5 @@ impl Desktop {
                     .child(self.tab_lead(tab, doc.is_some() && doc == preview, if selected { TEXT } else { TEXT_2 }))
                     .on_click(cx.listener(move |this, ev: &ClickEvent, window, cx| {
-                        this.select_tab(i, window, cx);
+                        this.select_tab(p, i, window, cx);
                         if ev.click_count() > 1
                             && let Some(doc) = &doc
@@ -276,6 +282,6 @@ impl Desktop {
                             let pointer = f32::from(window.mouse_position().x);
                             this.update(cx, |this, cx| {
-                                this.select_tab(i, window, cx);
-                                this.terminal.tab_drag = Some(TabDrag::new(i, grab.x.into(), pointer));
+                                this.select_tab(p, i, window, cx);
+                                this.panels.drag = Some((p, TabDrag::new(i, grab.x.into(), pointer)));
                             })
                             .ok();
@@ -289,5 +295,5 @@ impl Desktop {
                             cx.listener(move |this, e: &MouseDownEvent, _, cx| {
                                 this.close_menus();
-                                this.terminal.tab_actions = Some((tab.clone(), e.position));
+                                this.panels.actions = Some((p, tab.clone(), e.position));
                                 cx.notify();
                             }),
@@ -305,5 +311,6 @@ impl Desktop {
                     .text_size(px(12.5))
                     .whitespace_nowrap()
-                    .when(selected, |d| d.bg(SURFACE).shadow(ui::row_shadow()).font_weight(FontWeight::SEMIBOLD).text_color(TEXT))
+                    .when(selected && focused, |d| d.bg(SURFACE).shadow(ui::row_shadow()).font_weight(FontWeight::SEMIBOLD).text_color(TEXT))
+                    .when(selected && !focused, |d| d.bg(FILL_2).font_weight(FontWeight::MEDIUM).text_color(TEXT))
                     .when(!selected, |d| d.font_weight(FontWeight::MEDIUM).text_color(TEXT_2).hover(|s| s.bg(FILL_2)))
                     // Else the bar's drag_area moves the window, which takes the mouse before the tab's drag can start.
@@ -315,11 +322,6 @@ impl Desktop {
             })
             .collect();
-        let shown = Some((tree.to_string(), active));
-        if self.terminal.tab_revealed != shown {
-            self.terminal.tab_scroll.scroll_to_item(active);
-            self.terminal.tab_revealed = shown;
-        }
         // As f32: Pixels orders -0 below 0, so a strip that can't scroll would show its right fade.
-        let (offset, max) = (f32::from(self.terminal.tab_scroll.offset().x), f32::from(self.terminal.tab_scroll.max_offset().x));
+        let (offset, max) = (f32::from(scroll.offset().x), f32::from(scroll.max_offset().x));
         let fade = |left: bool| {
             let solid: Hsla = SURFACE_SUNKEN.into();
@@ -330,5 +332,5 @@ impl Desktop {
         let strip = div()
             .id("tab-strip")
-            .track_scroll(&self.terminal.tab_scroll)
+            .track_scroll(&scroll)
             .overflow_x_scroll()
             .flex()
@@ -336,8 +338,8 @@ impl Desktop {
             .items_center()
             .gap(px(GAP))
-            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<DragTab>, _, cx| {
+            .on_drag_move(cx.listener(move |this, e: &DragMoveEvent<DragTab>, _, cx| {
                 let pointer = f32::from(e.event.position.x);
                 // A drag move comes every frame, moved or not; notifying on each would redraw forever.
-                if let Some(d) = this.terminal.tab_drag.as_mut().filter(|d| d.pointer != pointer) {
+                if let Some((_, d)) = this.panels.drag.as_mut().filter(|(at, d)| *at == p && d.pointer != pointer) {
                     d.pointer = pointer;
                     cx.notify();
@@ -362,6 +364,6 @@ impl Desktop {
             .gap(px(2.))
             .child(div().relative().flex().min_w_0().child(strip).when(offset < 0., |d| d.child(fade(true))).when(offset > -max, |d| d.child(fade(false))))
-            .child(self.new_tab_controls(cx))
-            .children(self.tab_actions(&tabs, cx))
+            .child(self.new_tab_controls(p, cx))
+            .children(self.tab_actions(tree, p, tabs, cx))
     }
 }
@@ -369,13 +371,7 @@ impl Desktop {
 #[cfg(test)]
 mod tests {
-    use super::{TabDrag, tab_label};
+    use super::TabDrag;
     use std::time::{Duration, Instant};
 
-    #[test]
-    fn a_split_tab_counts_its_panes() {
-        assert_eq!(tab_label("claude".into(), 1), "claude");
-        assert_eq!(tab_label("claude".into(), 3), "claude · 3 panes");
-    }
-
     /// Three 100px tabs 2px apart, tab `from` held 10px from its left edge.
     fn holding(from: usize, pointer: f32) -> TabDrag {
```

```diff
diff --git a/packages/desktop/crates/pocket/src/terminal_view/tab_menu.rs b/packages/desktop/crates/pocket/src/panels/tab_menu.rs
similarity index 87%
rename from packages/desktop/crates/pocket/src/terminal_view/tab_menu.rs
rename to packages/desktop/crates/pocket/src/panels/tab_menu.rs
index 0e3df06..88c7206 100644
--- a/packages/desktop/crates/pocket/src/terminal_view/tab_menu.rs
+++ b/packages/desktop/crates/pocket/src/panels/tab_menu.rs
@@ -3,4 +3,6 @@ use agents::Summary;
 use gpui_kit::*;
 use theme::*;
+use workspace::Place;
+use workspace::tree::PaneId;
 
 /// The last model seen for `provider`, as a short label.
@@ -15,5 +17,5 @@ impl Desktop {
     }
 
-    pub(crate) fn tab_menu_view(&self, cx: &mut Context<Self>) -> Stateful<Div> {
+    pub(crate) fn tab_menu_view(&self, pane: PaneId, cx: &mut Context<Self>) -> Stateful<Div> {
         let branch = self.repo().map(|r| r.branch.clone()).unwrap_or_default();
         let item = |id: &'static str, lead: AnyElement, label: String, hint: Option<String>, keys: Option<&str>| {
@@ -36,5 +38,8 @@ impl Desktop {
         let agent = |id: &'static str, provider: &'static str, cx: &mut Context<Self>| {
             item(id, provider_icon(provider, 14., TEXT).into_any_element(), provider_name(provider).into(), self.model_hint(provider), None)
-                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.new_agent_tab(provider, cx)))
+                .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
+                    this.panels.menu = None;
+                    this.new_agent_tab(provider, Place::Pane(Some(pane)), cx);
+                }))
         };
         ui::pop(div().id("tab-menu"))
@@ -58,5 +63,8 @@ impl Desktop {
             .child(
                 item("tab-menu-shell", icon("prompt", 14., TEXT_2).into_any_element(), "New shell".into(), None, Some("⌘T"))
-                    .on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.new_tab(&crate::actions::NewTab, window, cx))),
+                    .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
+                        this.panels.menu = None;
+                        this.new_shell(Place::Pane(Some(pane)), cx);
+                    })),
             )
             .child(
@@ -68,5 +76,5 @@ impl Desktop {
             .child(agent("tab-menu-codex", "codex", cx))
             .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
-                this.terminal.tab_menu = false;
+                this.panels.menu = None;
                 cx.notify();
             }))
```

```diff
diff --git a/packages/desktop/crates/pocket/src/terminal_view/tab_actions.rs b/packages/desktop/crates/pocket/src/panels/tab_actions.rs
similarity index 51%
rename from packages/desktop/crates/pocket/src/terminal_view/tab_actions.rs
rename to packages/desktop/crates/pocket/src/panels/tab_actions.rs
index 723e7a2..d6d08d8 100644
--- a/packages/desktop/crates/pocket/src/terminal_view/tab_actions.rs
+++ b/packages/desktop/crates/pocket/src/panels/tab_actions.rs
@@ -2,4 +2,5 @@ use crate::desktop::Desktop;
 use gpui_kit::*;
 use workspace::Tab;
+use workspace::tree::{Edge, PaneId, Target};
 
 /// Where Move left and Move right take tab `i` of `len`, if it can go that way.
@@ -9,17 +10,26 @@ fn neighbours(i: usize, len: usize) -> (Option<usize>, Option<usize>) {
 
 impl Desktop {
-    /// The right-click menu, while the tab it was opened on is still among `tabs`.
-    pub(crate) fn tab_actions(&self, tabs: &[Tab], cx: &mut Context<Self>) -> Option<impl IntoElement> {
-        let (tab, at) = self.terminal.tab_actions.as_ref()?;
+    /// `pane`'s right-click menu, while the tab it was opened on is still among `tabs`.
+    pub(crate) fn tab_actions(&self, tree: &str, pane: PaneId, tabs: &[Tab], cx: &mut Context<Self>) -> Option<impl IntoElement> {
+        let (_, tab, at) = self.panels.actions.as_ref().filter(|(p, _, _)| *p == pane)?;
         let i = tabs.iter().position(|t| t == tab)?;
         let (left, right) = neighbours(i, tabs.len());
-        let row = |id: &'static str, icon: &str, label: &str, to: usize| {
+        let row = |id: &'static str, icon: &str, label: &str, to: usize, cx: &mut Context<Self>| {
             ui::menu_row(id, icon, label, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
-                this.terminal.tab_actions = None;
-                this.move_tab(i, to, cx);
+                this.panels.actions = None;
+                this.move_tab(pane, i, to, cx);
             }))
         };
+        let t = &self.workspaces[tree].tree;
+        let split = |id: &'static str, icon: &str, label: &str, edge: Edge, cx: &mut Context<Self>| {
+            t.can_split(pane, edge, self.panels.bounds).then(|| {
+                ui::menu_row(id, icon, label, None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
+                    this.panels.actions = None;
+                    this.move_tab_to(pane, i, Target::Split { pane, edge }, window, cx);
+                }))
+            })
+        };
         let menu = ui::pop(div().id("tab-actions"))
-            .w(px(180.))
+            .w(px(220.))
             .p(px(6.))
             .flex()
@@ -27,9 +37,11 @@ impl Desktop {
             .occlude()
             .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
-                this.terminal.tab_actions = None;
+                this.panels.actions = None;
                 cx.notify();
             }))
-            .children(left.map(|to| row("tab-move-left", "back", "Move left", to)))
-            .children(right.map(|to| row("tab-move-right", "forward", "Move right", to)));
+            .children(left.map(|to| row("tab-move-left", "back", "Move left", to, cx)))
+            .children(right.map(|to| row("tab-move-right", "forward", "Move right", to, cx)))
+            .children(split("tab-split-right", "split-right", "Move to new split right", Edge::Right, cx))
+            .children(split("tab-split-down", "split-down", "Move to new split down", Edge::Bottom, cx));
         Some(deferred(anchored().position(*at).snap_to_window_with_margin(px(8.)).child(ui::menu_in("tab-actions-in", menu))).with_priority(1))
     }
```

### Task 4.5: A terminal tab is one terminal

**Files:**
- Modify: `packages/desktop/crates/pocket/src/terminal_view.rs`. Changes:
  - remove `mod tab_actions`, `mod tab_menu` and `mod tabs`
  - remove the tab fields
  - change `new_tab` and `close_active_tab`
  - delete `session_page`
- Modify: `packages/desktop/crates/pocket/src/terminal_view/pane.rs`. Changes:
  - delete `pane_title`, `numbered`, `panes` and the pane header
  - add `term_body`
- Modify: `packages/desktop/crates/pocket/src/terminal_view/surface.rs` (`SMALL` is now unused)
- Modify: `packages/desktop/crates/pocket/src/inbox/detail.rs:41`

**Context:**
- `term_body(id, focused)` wraps one terminal.
- Only the focused pane's terminal tracks the terminal focus and takes keys. Two panes can't both `track_focus` the same handle.
- The focus ring and the numbered header that told split terminals apart are gone. The pane's tab bar does that job now.

**Step 1: Apply**

```diff
diff --git a/packages/desktop/crates/pocket/src/terminal_view.rs b/packages/desktop/crates/pocket/src/terminal_view.rs
index ab55b47..3bbcdd5 100644
--- a/packages/desktop/crates/pocket/src/terminal_view.rs
+++ b/packages/desktop/crates/pocket/src/terminal_view.rs
@@ -5,12 +5,8 @@ pub(crate) mod pane;
 pub(crate) mod scroll;
 pub(crate) mod surface;
-pub(crate) mod tab_actions;
-pub(crate) mod tab_menu;
-pub(crate) mod tabs;
 
 use crate::actions::{CloseTab, CopySelection, NewTab, Paste, SelectAll};
-use crate::desktop::{Desktop, MAIN};
-use crate::desktop::chrome::{Confirm, Overlay, drag_area, observe_banner};
-use gpui_kit::prelude::FluentBuilder as _;
+use crate::desktop::Desktop;
+use crate::desktop::chrome::{Confirm, Overlay};
 use gpui_kit::*;
 use scroll::{Wheel, WheelRows};
@@ -18,6 +14,5 @@ use std::ops::Range;
 use std::time::Duration;
 use term::{Autoscroll, Pointer, Scroll};
-use theme::*;
-use workspace::{Doc, Tab};
+use workspace::Place;
 
 pub struct TerminalViewState {
@@ -26,11 +21,4 @@ pub struct TerminalViewState {
     /// The IME's uncommitted text, drawn at the cursor until it commits.
     pub(crate) marked: Option<String>,
-    pub(crate) tab_scroll: ScrollHandle,
-    /// The worktree and tab last scrolled into view, so a tab is revealed once when it becomes active rather than every frame.
-    pub(crate) tab_revealed: Option<(String, usize)>,
-    pub(crate) tab_menu: bool,
-    /// The tab whose right-click menu is open, and where the click was.
-    pub(crate) tab_actions: Option<(Tab, Point<Pixels>)>,
-    pub(crate) tab_drag: Option<tabs::TabDrag>,
     pub(crate) selection: Option<Drag>,
     pub(crate) wheel: WheelRows,
@@ -48,5 +36,5 @@ pub struct TerminalViewState {
 impl TerminalViewState {
     pub fn new(cx: &mut Context<Desktop>) -> Self {
-        Self { focus: cx.focus_handle(), focused: None, marked: None, tab_scroll: ScrollHandle::new(), tab_revealed: None, tab_menu: false, tab_actions: None, tab_drag: None, selection: None, wheel: WheelRows::default(), autoscroll: None, blink: None, blink_on: true, keyboard: false, cursor_blinks: false, caret: None }
+        Self { focus: cx.focus_handle(), focused: None, marked: None, selection: None, wheel: WheelRows::default(), autoscroll: None, blink: None, blink_on: true, keyboard: false, cursor_blinks: false, caret: None }
     }
 }
@@ -267,6 +255,6 @@ impl Desktop {
 
     pub fn new_tab(&mut self, _: &NewTab, _: &mut Window, cx: &mut Context<Self>) {
-        self.terminal.tab_menu = false;
-        self.new_shell(None, cx);
+        self.panels.menu = None;
+        self.new_shell(Place::Pane(None), cx);
     }
 
@@ -276,6 +264,7 @@ impl Desktop {
         }
         let Some(tree) = self.session_tree() else { return };
-        let active = self.workspace(&tree).active;
-        self.close_tab(active, cx);
+        let p = self.workspace(&tree).tree.focused();
+        let (pane, i) = (p.id, p.active);
+        self.close_tab(pane, i, cx);
     }
 
@@ -289,58 +278,4 @@ impl Desktop {
         }
     }
-
-    pub(crate) fn session_page(&mut self, tree: &str, window: &mut Window, cx: &mut Context<Self>) -> Div {
-        let (added, removed) = self.repo().map(|r| r.totals()).unwrap_or_default();
-        let diff = (added + removed > 0).then(|| {
-            div()
-                .id("bar-diff")
-                .flex_none()
-                .mr(px(4.))
-                .cursor_pointer()
-                .child(ui::meta_diff(added, removed, 12.))
-                .on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.open_changes(None, false, cx)))
-        });
-        let error = self.error.clone().map(|e| div().min_w_0().truncate().mr(px(6.)).text_size(px(12.5)).text_color(FAILED_TEXT).child(e));
-        let ring = self.terminal.focused.as_deref().and_then(|t| self.summary(t)).and_then(|a| a.context()).map(|(used, window)| context::ring(used, window));
-        let status = div().ml_auto().pl(px(8.)).min_w_0().flex().items_center().children(error).children(ring);
-        let observe = self.agents.observe_only();
-        let right = div()
-            .flex()
-            .flex_none()
-            .items_center()
-            .gap(px(8.))
-            .when(!observe, |d| {
-                d.child(ui::icon_group([
-                    ui::group_button("split-right", "split-right").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(false), cx))),
-                    ui::group_button("split-down", "split-down").on_click(cx.listener(|this, _: &ClickEvent, _, cx| this.new_shell(Some(true), cx))),
-                ]))
-            })
-            .child(ui::icon_group([
-                ui::group_button("session-more", "more").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::More, window, cx))),
-            ]));
-        let (pad, toggle) = self.bar_start(cx);
-        let bar = drag_area(div())
-            .h(px(42.))
-            .flex_none()
-            .flex()
-            .items_center()
-            .gap(px(6.))
-            .pl(px(pad))
-            .pr(px(10.))
-            .children(toggle)
-            .child(self.term_tabs(tree, window, cx))
-            .child(status)
-            .children(diff)
-            .child(right);
-        let body = match self.workspace(tree).active().cloned() {
-            Some(Tab::Term(rows)) => self.panes(rows, cx),
-            Some(Tab::Doc(doc @ Doc::File(_))) if self.loaded(MAIN, &doc) => self.file_view(MAIN, cx),
-            Some(Tab::Doc(doc @ (Doc::Diff(_) | Doc::CommitFile { .. }))) if self.loaded(MAIN, &doc) => self.diff_view(MAIN, cx),
-            Some(Tab::Doc(doc @ Doc::Commit(_))) if self.loaded(MAIN, &doc) => self.commit_view(MAIN, cx),
-            Some(Tab::Web(id)) => self.browser_view(MAIN, id, window, cx),
-            Some(Tab::Doc(_)) | None => div().flex_1(),
-        };
-        div().flex_1().min_h_0().flex().flex_col().bg(SURFACE_SUNKEN).child(bar).when(observe, |d| d.child(observe_banner())).child(body)
-    }
 }
```

```diff
diff --git a/packages/desktop/crates/pocket/src/terminal_view/pane.rs b/packages/desktop/crates/pocket/src/terminal_view/pane.rs
index 9825ade..501b926 100644
--- a/packages/desktop/crates/pocket/src/terminal_view/pane.rs
+++ b/packages/desktop/crates/pocket/src/terminal_view/pane.rs
@@ -1,4 +1,3 @@
 use crate::desktop::Desktop;
-use crate::desktop::chrome::id;
 use crate::status;
 use crate::terminal_view::{cursor, scroll};
@@ -11,5 +10,4 @@ use gpui_kit::prelude::FluentBuilder as _;
 use gpui_kit::*;
 use theme::*;
-use ui::{self, icon_button_sized};
 
 /// "/bin/zsh -l" reads as "zsh": the login flag the desktop adds says nothing about the pane.
@@ -30,27 +28,4 @@ pub fn pane_label(agent: Option<&Summary>, session: Option<&Session>) -> String
 }
 
-pub fn pane_title(agent: Option<&Summary>, session: Option<&Session>) -> String {
-    match agent {
-        Some(a) => format!("{} — {}", a.provider, basename(&a.cwd)),
-        None => pane_label(None, session),
-    }
-}
-
-/// Numbers a split tab's panes in reading order, for their headers; a lone pane has no header.
-pub fn numbered(rows: Vec<Vec<String>>) -> Vec<Vec<(String, Option<usize>)>> {
-    let split = rows.len() > 1 || rows[0].len() > 1;
-    let mut n = 0;
-    rows.into_iter()
-        .map(|row| {
-            row.into_iter()
-                .map(|id| {
-                    n += 1;
-                    (id, split.then_some(n))
-                })
-                .collect()
-        })
-        .collect()
-}
-
 impl Desktop {
     pub fn pane_label(&self, id: &str) -> String {
@@ -58,20 +33,6 @@ impl Desktop {
     }
 
-    pub(crate) fn panes(&mut self, rows: Vec<Vec<String>>, cx: &mut Context<Self>) -> Div {
-        let mut out = Vec::new();
-        for (r, row) in numbered(rows).into_iter().enumerate() {
-            let m = if r == 0 { &surface::MAIN } else { &surface::SMALL };
-            if r > 0 {
-                out.push(div().h(px(0.5)).flex_none().bg(SEPARATOR));
-            }
-            let mut panes = Vec::new();
-            for (i, (id, n)) in row.into_iter().enumerate() {
-                if i > 0 {
-                    panes.push(div().w(px(0.5)).flex_none().bg(SEPARATOR));
-                }
-                panes.push(self.pane(&id, n, m, cx));
-            }
-            out.push(div().flex().min_h_0().when(r == 0, |d| d.flex_1()).when(r > 0, |d| d.h(px(250.)).flex_none()).children(panes));
-        }
+    /// The body of a terminal tab; only the focused pane's takes keys.
+    pub(crate) fn term_body(&mut self, id: &str, focused: bool, cx: &mut Context<Self>) -> Div {
         div()
             .flex_1()
@@ -81,18 +42,19 @@ impl Desktop {
             .border_t(px(0.5))
             .border_color(SEPARATOR)
-            .key_context(keys::CONTEXT)
-            .track_focus(&self.terminal.focus)
-            .on_key_down(cx.listener(Self::on_term_key))
-            .on_action(cx.listener(Self::copy_selection))
-            .on_action(cx.listener(Self::select_all))
-            .on_action(cx.listener(Self::paste))
-            .children(out)
+            .when(focused, |d| {
+                d.key_context(keys::CONTEXT)
+                    .track_focus(&self.terminal.focus)
+                    .on_key_down(cx.listener(Self::on_term_key))
+                    .on_action(cx.listener(Self::copy_selection))
+                    .on_action(cx.listener(Self::select_all))
+                    .on_action(cx.listener(Self::paste))
+            })
+            .child(self.pane(id, &surface::MAIN, cx))
     }
 
-    pub fn pane(&mut self, id: &str, n: Option<usize>, m: &'static Metrics, cx: &mut Context<Self>) -> Div {
+    pub fn pane(&mut self, id: &str, m: &'static Metrics, cx: &mut Context<Self>) -> Div {
         let focused = self.terminal.focused.as_deref() == Some(id);
         let exit = self.terminals.sessions.get(id).and_then(|s| s.exit);
         let known = self.terminals.sessions.get(id).is_some();
-        let title = pane_title(self.summary(id), self.terminals.sessions.get(id));
         let banner = self.summary(id).and_then(status::banner).map(|text| {
             div().flex_none().px(px(16.)).py(px(6.)).border_b(px(0.5)).border_color(SEPARATOR).bg(FILL_2).text_size(px(12.)).text_color(TEXT_2).child(text)
@@ -116,29 +78,4 @@ impl Desktop {
             cursor::overlay(focused.then(|| cx.entity()), at, rows, c, preedit, m)
         });
-        let close_id = id.to_string();
-        let closable = self.terminals.may_close(id, self.agents.observe_only());
-        let header = n.map(|n| {
-            div()
-                .h(px(30.))
-                .flex_none()
-                .pl(px(16.))
-                .pr(px(6.))
-                .flex()
-                .items_center()
-                .border_b(px(0.5))
-                .border_color(SEPARATOR)
-                .text_color(if focused { TEXT } else { TEXT_3 })
-                .font_family(MONO)
-                .text_size(px(11.5))
-                .child(div().flex_1().truncate().child(format!("{n} · {title}")))
-                .when(closable, |d| {
-                    d.child(icon_button_sized(self::id(format!("close-{close_id}")), "x", 22., TEXT_3).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
-                        cx.stop_propagation();
-                        if !this.ask_close(vec![close_id.clone()], cx) {
-                            this.close_pane(&close_id, cx);
-                        }
-                    })))
-                })
-        });
         let focus_id = id.to_string();
         let drop_id = id.to_string();
@@ -158,10 +95,8 @@ impl Desktop {
             .flex_col()
             .bg(SURFACE_SUNKEN)
-            .when(focused && n.is_some(), |d| d.shadow(vec![BoxShadow { inset: true, ..ui::ring(SEPARATOR_STRONG, 0.5) }]))
             .overflow_hidden()
             .on_mouse_down(MouseButton::Left, cx.listener(move |this, _: &MouseDownEvent, window, cx| this.focus_pane(focus_id.clone(), window, cx)))
             .drag_over::<ExternalPaths>(|s, _, _, _| s.shadow(vec![BoxShadow { inset: true, ..ui::ring(ACCENT, 1.5) }]))
             .on_drop(cx.listener(move |this, paths: &ExternalPaths, window, cx| this.drop_paths(drop_id.clone(), paths, window, cx)))
-            .children(header)
             .children(banner)
             .child(
@@ -185,5 +120,5 @@ impl Desktop {
 #[cfg(test)]
 mod tests {
-    use super::{Info, command_line, numbered, pane_label, pane_title};
+    use super::{Info, command_line, pane_label};
     use crate::terminals::sessions::Session;
     use agents::Summary;
@@ -212,27 +147,4 @@ mod tests {
     }
 
-    #[test]
-    fn an_agent_pane_title_names_its_folder_and_a_shell_one_its_label() {
-        let agent = Summary { provider: "codex".into(), cwd: "/code/pocket".into(), ..Default::default() };
-        assert_eq!(pane_title(Some(&agent), None), "codex — pocket");
-        assert_eq!(pane_title(None, Some(&shell("", None))), "zsh");
-    }
-
-    fn rows(rows: &[&[&str]]) -> Vec<Vec<String>> {
-        rows.iter().map(|r| r.iter().map(|id| id.to_string()).collect()).collect()
-    }
-
-    #[test]
-    fn a_lone_pane_goes_unnumbered() {
-        assert_eq!(numbered(rows(&[&["a"]])), vec![vec![("a".to_string(), None)]]);
-    }
-
-    #[test]
-    fn split_panes_are_numbered_in_reading_order() {
-        let n = |id: &str, n| (id.to_string(), Some(n));
-        assert_eq!(numbered(rows(&[&["a", "b"], &["c"]])), vec![vec![n("a", 1), n("b", 2)], vec![n("c", 3)]]);
-        assert_eq!(numbered(rows(&[&["a"], &["b"]])), vec![vec![n("a", 1)], vec![n("b", 2)]]);
-    }
-
     #[test]
     fn login_shells_show_as_their_name() {
diff --git a/packages/desktop/crates/pocket/src/terminal_view/surface.rs b/packages/desktop/crates/pocket/src/terminal_view/surface.rs
index 9fcfcf5..c93658f 100644
--- a/packages/desktop/crates/pocket/src/terminal_view/surface.rs
+++ b/packages/desktop/crates/pocket/src/terminal_view/surface.rs
@@ -11,5 +11,4 @@ pub struct Metrics {
 
 pub const MAIN: Metrics = Metrics { size: 13., line: 17. };
-pub const SMALL: Metrics = Metrics { size: 12., line: 15. };
 
 /// Geist Mono with ligatures off: a merged `--` would collapse two cells into one.
diff --git a/packages/desktop/crates/pocket/src/inbox/detail.rs b/packages/desktop/crates/pocket/src/inbox/detail.rs
index 8727453..59d47cb 100644
--- a/packages/desktop/crates/pocket/src/inbox/detail.rs
+++ b/packages/desktop/crates/pocket/src/inbox/detail.rs
@@ -39,5 +39,5 @@ impl Desktop {
                 this.focus_agent(&agent, window, cx)
             })));
-        let pane = self.pane(&n.terminal, None, &surface::MAIN, cx);
+        let pane = self.pane(&n.terminal, &surface::MAIN, cx);
         let hints = div()
             .h(px(36.))
```

### Task 4.6: Desktop draws the panels

**Files:**
- Modify: `packages/desktop/crates/pocket/src/desktop.rs`. Changes:
  - imports, `MAIN`'s doc, the `panels` field
  - `focus_agent`, `select_tab`
  - the three seams
  - `open_doc`, `close_doc`
  - `menu_open`, `close_menus`
  - `session_tree`, `main_view`, `blank_page`
  - `render`
- Modify: `browser.rs` (`sync_browser`, `Pressed`)
- Modify: `modals/more.rs`, `palette.rs`, `sidebar/row_menu.rs`, `creating.rs`, `desktop/alerts.rs` and `capture.rs`
- Modify: `explorer/preview/header.rs` and `git_ui/diff.rs`

**Context:**
- `session_tree` no longer requires tabs. An empty worktree shows one empty pane, with "No open tabs", instead of the blank page. The blank page remains for no project, and loses its + button.
- `select_tab(pane, i)` selects within a pane and focuses it.
- `open_doc` opens in the focused pane when no pane holds docs and the focused one has no room to split right.
- `sync_browser` shows only the pages that drawn panes show, so a zoom hides the others. `live` walks every pane's tabs.
- Clicking into a page focuses its pane.
- The More menu now opens at the click (`open_more`), because ⋯ can sit in any pane. The doc views' ⋯ buttons use it too.
- `visible_panes` (alerts) lists the terminals of every drawn pane.
- `capture.rs`: `tab-menu` opens the focused pane's menu.

**Step 1: Apply**

```diff
diff --git a/packages/desktop/crates/pocket/src/desktop.rs b/packages/desktop/crates/pocket/src/desktop.rs
index 9a3b819..fb5d42d 100644
--- a/packages/desktop/crates/pocket/src/desktop.rs
+++ b/packages/desktop/crates/pocket/src/desktop.rs
@@ -25,4 +25,5 @@ use crate::modals::pair_phone::PairPhone;
 use crate::modals::{add_project, new_session};
 use crate::palette::PaletteState;
+use crate::panels::Panels;
 use crate::sidebar::SidebarState;
 use crate::terminal_view::TerminalViewState;
@@ -37,8 +38,8 @@ use std::time::Instant;
 use store::Store;
 use theme::*;
-use workspace::tree::PaneId;
+use workspace::tree::{Edge, PaneId};
 use workspace::{Doc, Tab, Workspace};
 
-/// The one pane every tab shows in.
+/// The pane a worktree starts with.
 pub(crate) const MAIN: PaneId = 0;
 
@@ -68,4 +69,5 @@ pub struct Desktop {
     pub(crate) commit: Commits,
     pub(crate) terminal: TerminalViewState,
+    pub(crate) panels: Panels,
     pub(crate) browsers: Browsers,
     pub(crate) initials: String,
@@ -154,4 +156,5 @@ impl Desktop {
             root,
             terminal: TerminalViewState::new(cx),
+            panels: Panels::default(),
             browsers,
             inbox: InboxState::new(cx),
@@ -183,6 +186,6 @@ impl Desktop {
         self.session = Some(id.to_string());
         let w = self.workspace(&tree);
-        if let Some(i) = w.tab_of(&term) {
-            w.active = i;
+        if let Some((pane, i)) = w.tab_of(&term) {
+            w.tree.select(pane, i);
         }
         self.focus_pane(term, window, cx);
@@ -241,11 +244,11 @@ impl Desktop {
     }
 
-    pub fn select_tab(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
+    /// Shows tab `i` of `pane` and focuses the pane.
+    pub fn select_tab(&mut self, pane: PaneId, i: usize, window: &mut Window, cx: &mut Context<Self>) {
         let Some(tree) = self.cwd() else { return };
-        let w = self.workspace(&tree);
-        w.active = i;
-        match w.active().cloned() {
-            Some(Tab::Term(rows)) => self.focus_pane(rows[0][0].clone(), window, cx),
-            Some(Tab::Doc(doc)) => self.show_doc(self.focused_pane(), doc, cx),
+        self.workspace(&tree).tree.select(pane, i);
+        match self.pane_tab(pane) {
+            Some(Tab::Term(id)) => self.focus_pane(id, window, cx),
+            Some(Tab::Doc(doc)) => self.show_doc(pane, doc, cx),
             Some(Tab::Web(_)) => window.focus(&self.browsers.focus, cx),
             None => {}
@@ -256,14 +259,14 @@ impl Desktop {
     /// The pane keys and actions go to.
     pub(crate) fn focused_pane(&self) -> PaneId {
-        MAIN
+        self.cwd().and_then(|t| self.workspaces.get(&t)).map_or(MAIN, |w| w.tree.focused)
     }
 
     pub(crate) fn pane_ids(&self) -> Vec<PaneId> {
-        vec![MAIN]
+        self.cwd().and_then(|t| self.workspaces.get(&t)).map_or(vec![MAIN], |w| w.tree.panes().iter().map(|p| p.id).collect())
     }
 
     /// The tab `pane` shows in the worktree on screen.
     pub(crate) fn pane_tab(&self, pane: PaneId) -> Option<Tab> {
-        self.workspaces.get(&self.cwd()?)?.active().filter(|_| pane == MAIN).cloned()
+        self.workspaces.get(&self.cwd()?)?.tree.pane(pane)?.active().cloned()
     }
 
@@ -277,9 +280,14 @@ impl Desktop {
 
     /// Opens `doc` in its tab of the worktree on screen; unless `pin`, in the preview tab the next open takes over.
+    /// With no pane holding docs it opens in a new pane right of the focused one, or in the focused one when that has no room to split.
     pub fn open_doc(&mut self, doc: Doc, pin: bool, cx: &mut Context<Self>) {
         let Some(tree) = self.cwd() else { return };
         self.screen = Screen::Sessions;
-        self.workspace(&tree).open_doc(doc.clone(), pin);
-        self.show_doc(self.focused_pane(), doc, cx);
+        let bounds = self.panels.bounds;
+        let w = self.workspace(&tree);
+        let focused = w.tree.focused;
+        let pane = w.doc_pane().or_else(|| (!w.tree.can_split(focused, Edge::Right, bounds)).then_some(focused));
+        let (pane, _) = w.open_doc(doc.clone(), pin, pane);
+        self.show_doc(pane, doc, cx);
     }
 
@@ -291,6 +299,6 @@ impl Desktop {
 
     pub(crate) fn close_doc(&mut self, doc: &Doc, cx: &mut Context<Self>) {
-        if let Some(i) = self.cwd().and_then(|t| self.workspaces.get(&t)?.doc_tab(doc)) {
-            self.close_tab(i, cx);
+        if let Some((pane, i)) = self.cwd().and_then(|t| self.workspaces.get(&t)?.doc_tab(doc)) {
+            self.close_tab(pane, i, cx);
         }
     }
@@ -342,5 +350,5 @@ impl Desktop {
 
     pub(crate) fn menu_open(&self) -> bool {
-        self.terminal.tab_menu || self.terminal.tab_actions.is_some() || self.row_menu.is_some() || self.changes.commit_menu || self.changes.menu
+        self.panels.menu.is_some() || self.panels.actions.is_some() || self.row_menu.is_some() || self.changes.commit_menu || self.changes.menu
     }
 
@@ -348,15 +356,15 @@ impl Desktop {
     pub(crate) fn close_menus(&mut self) -> bool {
         let open = self.menu_open();
-        (self.terminal.tab_menu, self.terminal.tab_actions, self.row_menu, self.changes.commit_menu, self.changes.menu) = (false, None, None, false, false);
+        (self.panels.menu, self.panels.actions, self.row_menu, self.changes.commit_menu, self.changes.menu) = (None, None, None, false, false);
         self.sidebar.menu_at = None;
         open
     }
 
-    /// The worktree whose tabs the main view shows, if it shows any.
-    pub(crate) fn session_tree(&mut self) -> Option<String> {
+    /// The worktree whose panes the main view shows.
+    pub(crate) fn session_tree(&self) -> Option<String> {
         if self.terminals.link.is_down() || matches!(self.screen, Screen::Inbox) {
             return None;
         }
-        self.cwd().filter(|t| !self.workspace(t).tabs.is_empty())
+        self.cwd()
     }
 
@@ -364,5 +372,5 @@ impl Desktop {
         let body = match self.session_tree() {
             _ if self.shown_create().is_some() => self.creating_page(cx),
-            Some(tree) => self.session_page(&tree, window, cx),
+            Some(tree) => self.panels_view(&tree, window, cx),
             None if self.terminals.link.is_down() => self.link_page(cx),
             None if matches!(self.screen, Screen::Inbox) => self.inbox_detail(cx),
@@ -396,6 +404,5 @@ impl Desktop {
             .pl(px(pad))
             .children(toggle)
-            .child(ui::breadcrumb(vec!["Sessions".into()]))
-            .when(self.cwd().is_some() && !observe, |d| d.child(self.new_tab_controls(cx)));
+            .child(ui::breadcrumb(vec!["Sessions".into()]));
         div().flex_1().flex().flex_col().child(bar).when(observe, |d| d.child(chrome::observe_banner())).child(
             div()
@@ -422,4 +429,5 @@ impl Desktop {
 impl Render for Desktop {
     fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
+        self.sync_panels();
         self.sync_code(window, cx);
         self.sync_view(window, cx);
```

```diff
diff --git a/packages/desktop/crates/pocket/src/browser.rs b/packages/desktop/crates/pocket/src/browser.rs
index 9555aa4..d022565 100644
--- a/packages/desktop/crates/pocket/src/browser.rs
+++ b/packages/desktop/crates/pocket/src/browser.rs
@@ -115,6 +115,10 @@ impl Desktop {
         }
         let covered = self.overlay.is_some() || self.menu_open() || self.sidebar.menu_at.is_some() || (self.layout == Layout::Compact && self.panel) || self.shown_create().is_some();
-        let shown: Vec<u64> = if covered { Vec::new() } else { self.pane_ids().into_iter().filter_map(|p| self.web_tab(p)).collect() };
-        let live = self.workspaces.values().flat_map(|w| &w.tabs).filter_map(|t| if let Tab::Web(id) = t { Some(*id) } else { None }).collect();
+        let web = |t: &Tab| if let Tab::Web(id) = t { Some(*id) } else { None };
+        let shown: Vec<u64> = match self.session_tree().and_then(|t| self.workspaces.get(&t)) {
+            Some(w) if !covered => w.shown().into_iter().filter_map(|(_, t)| web(t)).collect(),
+            _ => Vec::new(),
+        };
+        let live = self.workspaces.values().flat_map(|w| w.tree.panes()).flat_map(|p| &p.tabs).filter_map(web).collect();
         self.browsers.show(&shown, &live);
     }
@@ -172,5 +176,12 @@ impl Desktop {
                 }
             }
-            web::Event::Pressed => window.focus(&self.browsers.focus, cx),
+            web::Event::Pressed => {
+                if let Some(w) = self.cwd().and_then(|t| self.workspaces.get_mut(&t))
+                    && let Some((pane, _)) = w.tree.find(|t| *t == Tab::Web(id))
+                {
+                    w.tree.focus(pane);
+                }
+                window.focus(&self.browsers.focus, cx);
+            }
         }
         cx.notify();
diff --git a/packages/desktop/crates/pocket/src/modals/more.rs b/packages/desktop/crates/pocket/src/modals/more.rs
index ba2d121..b71bdd9 100644
--- a/packages/desktop/crates/pocket/src/modals/more.rs
+++ b/packages/desktop/crates/pocket/src/modals/more.rs
@@ -5,9 +5,10 @@ use gpui_kit::*;
 use std::path::Path;
 use ui::{self, menu_row};
-use workspace::Doc;
+use workspace::{Doc, Place};
 
 impl Desktop {
     pub(super) fn more_menu(&mut self, cx: &mut Context<Self>) -> Div {
-        let menu = ui::pop(div().absolute().right(px(22.)).top(px(58.)).w(px(230.)).p(px(6.)).flex().flex_col()).occlude();
+        let at = self.panels.more_at.unwrap_or_default();
+        let menu = ui::pop(div().absolute().left(at.x - px(216.)).top(at.y + px(18.)).w(px(230.)).p(px(6.)).flex().flex_col()).occlude();
         let path = match self.active_doc() {
             Some(Doc::File(p)) => Some(p),
@@ -18,5 +19,5 @@ impl Desktop {
             return menu
                 .child(menu_row("more-tab", "terminal", "New terminal tab", None).on_click(cx.listener(|this, _: &ClickEvent, window, cx| {
-                    this.new_shell(None, cx);
+                    this.new_shell(Place::Pane(None), cx);
                     this.close_overlay(window, cx);
                 })))
diff --git a/packages/desktop/crates/pocket/src/explorer/preview/header.rs b/packages/desktop/crates/pocket/src/explorer/preview/header.rs
index e71a862..e5b5290 100644
--- a/packages/desktop/crates/pocket/src/explorer/preview/header.rs
+++ b/packages/desktop/crates/pocket/src/explorer/preview/header.rs
@@ -79,5 +79,5 @@ impl Desktop {
                 }
                 .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| this.copy_path(&copy, cx))),
-                ui::group_button("file-more", "more").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::More, window, cx))),
+                ui::group_button("file-more", "more").on_click(cx.listener(|this, e: &ClickEvent, window, cx| this.open_more(e.position(), window, cx))),
             ]));
         let (status_color, status_label) = status_word(self.file_status(path));
diff --git a/packages/desktop/crates/pocket/src/git_ui/diff.rs b/packages/desktop/crates/pocket/src/git_ui/diff.rs
index 16dc998..0d77970 100644
--- a/packages/desktop/crates/pocket/src/git_ui/diff.rs
+++ b/packages/desktop/crates/pocket/src/git_ui/diff.rs
@@ -9,5 +9,5 @@ use theme::*;
 use ui::{self, Segment, Variant, checkbox, dot};
 use crate::desktop::{Desktop, MAIN};
-use crate::desktop::chrome::{Overlay, Side, doc_bar, empty};
+use crate::desktop::chrome::{Side, doc_bar, empty};
 use crate::explorer::status_word;
 use crate::syntax::{Spans, language_for, line_spans};
@@ -512,5 +512,5 @@ impl Desktop {
             })
             .child(ui::icon_group([
-                ui::group_button("diff-more", "more").on_click(cx.listener(|this, _: &ClickEvent, window, cx| this.open(Overlay::More, window, cx))),
+                ui::group_button("diff-more", "more").on_click(cx.listener(|this, e: &ClickEvent, window, cx| this.open_more(e.position(), window, cx))),
             ]));
         let status = match &at {
diff --git a/packages/desktop/crates/pocket/src/palette.rs b/packages/desktop/crates/pocket/src/palette.rs
index 7ffbcb1..37ee864 100644
--- a/packages/desktop/crates/pocket/src/palette.rs
+++ b/packages/desktop/crates/pocket/src/palette.rs
@@ -14,4 +14,6 @@ use store::Sounds;
 use theme::*;
 use ui::{self, dot};
+use workspace::Place;
+use workspace::tree::Edge;
 
 #[derive(Clone, Debug, PartialEq)]
@@ -344,5 +346,5 @@ impl Desktop {
             }
             Pick::New => self.open(Overlay::NewSession, window, cx),
-            Pick::Split => self.new_shell(Some(false), cx),
+            Pick::Split => self.new_shell(Place::Split(self.focused_pane(), Edge::Right), cx),
             Pick::Next => self.next_needs_you(&crate::actions::NextNeedsYou, window, cx),
             Pick::Tree { project, tree } => {
diff --git a/packages/desktop/crates/pocket/src/sidebar/row_menu.rs b/packages/desktop/crates/pocket/src/sidebar/row_menu.rs
index d4780b4..db966eb 100644
--- a/packages/desktop/crates/pocket/src/sidebar/row_menu.rs
+++ b/packages/desktop/crates/pocket/src/sidebar/row_menu.rs
@@ -58,6 +58,6 @@ impl Desktop {
                 this.row_menu = (this.row_menu.as_ref() != Some(&toggle)).then(|| toggle.clone());
                 this.sidebar.menu_at = None;
-                this.terminal.tab_menu = false;
-                this.terminal.tab_actions = None;
+                this.panels.menu = None;
+                this.panels.actions = None;
                 cx.notify();
             },
diff --git a/packages/desktop/crates/pocket/src/creating.rs b/packages/desktop/crates/pocket/src/creating.rs
index 71c4368..4456f6e 100644
--- a/packages/desktop/crates/pocket/src/creating.rs
+++ b/packages/desktop/crates/pocket/src/creating.rs
@@ -363,5 +363,5 @@ impl Desktop {
         });
         let r = c.request.clone();
-        let output = self.workspaces.get(&c.path).is_some_and(|w| !w.tabs.is_empty()).then(|| {
+        let output = self.workspaces.get(&c.path).is_some_and(|w| w.tree.panes().iter().any(|p| !p.tabs.is_empty())).then(|| {
             let r = r.clone();
             ui::link("create-output", "View output").on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
diff --git a/packages/desktop/crates/pocket/src/desktop/alerts.rs b/packages/desktop/crates/pocket/src/desktop/alerts.rs
index 12dc4d4..a4601a4 100644
--- a/packages/desktop/crates/pocket/src/desktop/alerts.rs
+++ b/packages/desktop/crates/pocket/src/desktop/alerts.rs
@@ -181,11 +181,8 @@ impl Desktop {
     }
 
-    /// The terminals on screen: the worktree's active tab's.
+    /// The terminals on screen: those of the tabs the drawn panes show.
     fn visible_panes(&mut self) -> Vec<String> {
         let Some(tree) = self.cwd().filter(|_| self.screen == Screen::Sessions) else { return Vec::new() };
-        match self.workspace(&tree).active() {
-            Some(Tab::Term(rows)) => rows.concat(),
-            _ => Vec::new(),
-        }
+        self.workspace(&tree).shown().into_iter().filter_map(|(_, t)| if let Tab::Term(id) = t { Some(id.clone()) } else { None }).collect()
     }
diff --git a/packages/desktop/crates/pocket/src/capture.rs b/packages/desktop/crates/pocket/src/capture.rs
index a03f6bf..7d82ae0 100644
--- a/packages/desktop/crates/pocket/src/capture.rs
+++ b/packages/desktop/crates/pocket/src/capture.rs
@@ -42,5 +42,5 @@ const STEPS: [(&str, Step); 23] = [
         }
     }),
-    ("tab-menu", |d, _, _| d.terminal.tab_menu = true),
+    ("tab-menu", |d, _, _| d.panels.menu = Some(d.focused_pane())),
     ("browser", |d, window, cx| d.open_browser(None, window, cx)),
     ("file", |d, _, cx| {
@@ -154,5 +154,5 @@ fn reset(d: &mut Desktop, window: &mut Window, cx: &mut Context<Desktop>) {
     d.close_overlay(window, cx);
     (d.screen, d.side) = (Screen::Sessions, Side::Sessions);
-    (d.layout, d.widths, d.panel, d.terminal.tab_menu) = (Layout::Sidebars, [None; 2], false, false);
+    (d.layout, d.widths, d.panel, d.panels.menu) = (Layout::Sidebars, [None; 2], false, None);
     (d.session, d.worktree, d.terminal.focused) = (None, None, None);
     for v in d.diff.panes.values_mut() {
```

**Step 2: Run the new test**

Run: `cargo test -p pocket panels`
Expected: PASS, `only_panes_on_the_top_edge_take_the_window_bar_and_only_the_top_left_one_its_controls`.

**Step 3: Gate**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: builds; clippy shows no warnings beyond those on `main` before PR 1 (in scratch: 2× field assignment outside of initializer, 1× `Iterator::last`); all tests pass.

**Step 4: Screens after**

Run (repo root): `.ui-review/fixture/capture.sh /tmp/split-pr4-after session=session split=session,explore,file tab-menu=session,tab-menu browser=session,browser focus=session,focus`

Expected, compared with `/tmp/split-pr4-before`:
- `session`: one pane whose bar holds the tabs, the ring and the split/⋯ buttons.
- `split`: the file opens in a new pane right of the terminal, with a divider between them.
- `tab-menu`: the + menu drops from the focused pane's bar.
- `browser`: the page fills its pane.
- `focus`: the top-left pane's bar starts after the traffic lights, with the sidebar toggle.

Then check by hand (`cargo run --release -p pocket`):
- Drag a divider, and double-click it to even the split.
- Right-click a tab → "Move to new split down".
- Close the last tab of a pane; the pane goes.


## PR 5: Drag tabs between panels

**Scope:**
- A tab held on its strip still slides among its siblings, as it does today.
- Pulled off the strip, the tab follows the pointer as a ghost, and the others slide home. Where it would land lights up:
  - over another pane's strip: a caret between that strip's tabs
  - near a pane's edge: the half of the pane a split would take
  - in the middle of another pane: the whole pane
- Letting go there moves the tab with `Tree::move_tab`. Anywhere else, nothing moves.
- A page is a native view drawn above GPUI, so it would hide the highlight. While a tab is held, each shown page is swapped for a snapshot of itself.

**Not in this PR:** tear-off windows, dragging to another worktree, dragging onto the sidebar.

**Depends on:** PR 4.

**Done when:**
- The gate passes, with pocket tests at PR 4 + 10 (8 in `drop`, 2 in `strip`).
- By hand:
  - a tab dragged onto another strip, a pane's edge, or a pane's middle lands there
  - a tab dropped on its own pane's body, or outside every pane, stays put
  - pages show their snapshot while a tab is held
- The screens are unchanged from PR 4, since capture can't hold a drag.

**What was compiled:** the scratch copy has PR 4 applied.

| Tasks | Status |
|---|---|
| 5.1, 5.2 | Applied. `cargo test -p pocket panels` passes 23 tests. |
| 5.5 | Applied. `cargo clippy -p web --all-targets` is clean. |
| 5.3, 5.4, 5.6 (the view wiring) | **uncompiled**. Expect small type fixes. |

Until 5.3 and 5.4 use `drop` and `TabDrag::mid`, the bin warns that they are dead code. The gate runs only at the end of 5.6.

### Task 5.1: Where a held tab drops

**Files:**
- Create: `packages/desktop/crates/pocket/src/panels/drop.rs`
- Modify: `packages/desktop/crates/pocket/src/panels.rs` (`mod drop;` before `mod strip;`)
- Modify: `packages/desktop/crates/pocket/src/panels/strip.rs` (`GAP` becomes `pub(super)`)

**Context:**
- Pure geometry, in window pixels:
  - `layout`: each drawn pane's rect, from `Tree::layout(panels.bounds)`.
  - `slots`: each strip's tabs as drawn.
  - `mid`: the held tab's middle.
- The top `STRIP_H` (42px, the bar's height) of a pane is its strip:
  - Over another pane's strip, the tab joins past the tabs whose middle `mid` passed.
  - Over its own strip it is `Home`, and the strip keeps sliding it as today.
- Below the strip, the outer third of each side of the body splits that way, unless the pane is under `2 × MIN_W` / `2 × MIN_H` that way (`zone`).
  - In the middle the tab joins the pane at the end.
  - Over its own pane it drops only to split, and only when that leaves a tab behind, since moving a pane's only tab beside itself does nothing.
- `highlight` gives the lit rect: the half a split takes, else the body, inset 4px.
- `caret` gives where the caret sits: in the gap before a tab, else after the last.

**Status:** compiled; tests pass.

**Step 1: Write the failing tests.** They are the `mod tests` of the file below. Create the file with only `use`s, `STRIP_H`, `Slots`, `Aim`, and `todo!()` bodies.

**Step 2: Run them**

Run: `cargo test -p pocket panels::drop`
Expected: FAIL (`not yet implemented`).

**Step 3: Implement**

```rust
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
```

**Step 4: Run them**

Run: `cargo test -p pocket panels::drop`
Expected: PASS, 8 tests.

### Task 5.2: A held tab can leave its strip

**Files:**
- Modify: `packages/desktop/crates/pocket/src/panels/strip.rs`. Changes:
  - `TabDrag` gains `away` and `mid`
  - `from` and `pointer` become `pub(super)`
  - the `slots` measurement is extracted
  - tests

**Context:**
- `away` is set by Task 5.3 while the aim isn't `Home`:
  - The held tab is no longer lifted.
  - Its siblings slide home.
  - A release moves nothing within the strip, because 5.3 handles the drop.
- `mid` is the held tab's middle, unclamped. `aim` clamps the tab's left edge to the strip, and that clamp would stop the middle at the strip's end.
- `slots(scroll, n)` is the measurement `strip_tabs` already did, extracted for 5.3.

**Status:** compiled; tests pass.

**Step 1: Write the failing tests**

Add the two tests at the end of the diff below to `mod tests`.

**Step 2: Run them**

Run: `cargo test -p pocket panels::strip`
Expected: FAIL to compile (no field `away`, no method `mid`).

**Step 3: Implement**

```diff
diff --git a/packages/desktop/crates/pocket/src/panels/strip.rs b/packages/desktop/crates/pocket/src/panels/strip.rs
index 142ec83..d135601 100644
--- a/packages/desktop/crates/pocket/src/panels/strip.rs
+++ b/packages/desktop/crates/pocket/src/panels/strip.rs
@@ -11,7 +11,7 @@ use ui::{self, dot};
 use workspace::tree::{Pane, PaneId, Target};
 use workspace::{Doc, Tab};
 
-const GAP: f32 = 2.;
+pub(super) const GAP: f32 = 2.;
 /// How quickly a sliding tab closes on where it's going: about 90% of the way in 80ms.
 const SLIDE: f32 = 0.035;
 
@@ -20,10 +20,10 @@ struct DragTab;
 
 /// A tab held on the strip and the tabs sliding out of its way; once released, every tab sliding home.
 pub(crate) struct TabDrag {
-    from: usize,
+    pub(super) from: usize,
     /// Where the pointer holds the tab, from its left edge.
     grab: f32,
-    pointer: f32,
+    pub(super) pointer: f32,
     /// Each tab's left edge and width as laid out, before any slide.
     slots: Vec<(f32, f32)>,
     /// How far each tab is drawn from its slot.
@@ -31,11 +31,19 @@ pub(crate) struct TabDrag {
     /// The last frame, while any tab is on its way.
     last_frame: Option<Instant>,
     released: bool,
+    /// Held off its strip: the other tabs slide home and a release here moves nothing.
+    pub(super) away: bool,
+}
+
+/// Each of a strip's `n` tabs' left edge and width as laid out, unscrolled; fewer before the strip is first drawn.
+pub(super) fn slots(scroll: &ScrollHandle, n: usize) -> Vec<(f32, f32)> {
+    let scrolled = f32::from(scroll.offset().x);
+    (0..n).map_while(|i| scroll.bounds_for_item(i)).map(|b| (f32::from(b.left()) + scrolled, f32::from(b.size.width))).collect()
 }
 
 impl TabDrag {
     fn new(from: usize, grab: f32, pointer: f32) -> Self {
-        Self { from, grab, pointer, slots: Vec::new(), offsets: Vec::new(), last_frame: None, released: false }
+        Self { from, grab, pointer, slots: Vec::new(), offsets: Vec::new(), last_frame: None, released: false, away: false }
     }
 
     /// Takes where the strip laid its tabs out; false once it has gained or lost one.
@@ -65,7 +73,7 @@ impl TabDrag {
     /// How far each tab should be drawn from its slot: the held one under the pointer, the ones it passed moved over to fill its place.
     fn targets(&self) -> Vec<f32> {
         let n = self.slots.len();
-        let Some((left, to)) = self.aim().filter(|_| !self.released) else { return vec![0.; n] };
+        let Some((left, to)) = self.aim().filter(|_| !self.released && !self.away) else { return vec![0.; n] };
         let step = self.slots[self.from].1 + GAP;
         (0..n)
             .map(|i| match i {
@@ -78,7 +86,7 @@ impl TabDrag {
     }
 
     fn held(&self) -> Option<usize> {
-        (!self.released).then_some(self.from)
+        (!self.released && !self.away).then_some(self.from)
     }
 
     /// Moves each tab toward its target as of `now`, the held one straight there; true while any is still on its way.
@@ -94,6 +102,12 @@ impl TabDrag {
         moving
     }
 
+    /// The held tab's middle under the pointer, unclamped, so it follows the pointer past the strip.
+    pub(super) fn mid(&self) -> Option<f32> {
+        let &(_, w) = self.slots.get(self.from)?;
+        Some(self.pointer - self.grab + w / 2.)
+    }
+
     /// Lets the held tab go, once, and returns where it moves from and to; each tab then slides home from where it's drawn, in the new order.
     fn release(&mut self) -> Option<(usize, usize)> {
         if self.released {
@@ -101,7 +115,7 @@ impl TabDrag {
         }
         let targets = self.targets();
         self.released = true;
-        let (_, to) = self.aim()?;
+        let (_, to) = self.aim().filter(|_| !self.away)?;
         let (from, w) = (self.from, self.slots[self.from].1);
         let (l, lw) = self.slots[to];
         let landing = if to > from { l + lw - w } else { l };
@@ -213,8 +227,7 @@ impl Desktop {
             strip.revealed = shown;
         }
         if let Some((_, d)) = self.panels.drag.as_mut().filter(|(at, _)| *at == p) {
-            let scrolled = f32::from(scroll.offset().x);
-            let slots = (0..tabs.len()).map_while(|i| scroll.bounds_for_item(i)).map(|b| (f32::from(b.left()) + scrolled, f32::from(b.size.width))).collect();
+            let slots = slots(&scroll, tabs.len());
             // gpui ends a drag on any mouse up, even one the strip never sees; a hold outliving it would keep its tab lifted.
             if !d.lay(slots) || !(d.released || cx.has_active_drag()) {
                 self.panels.drag = None;
@@ -457,4 +470,18 @@ mod tests {
         assert!(d.lay(vec![(0., 100.), (102., 100.), (204., 100.)]));
         assert!(!d.lay(vec![(0., 100.), (102., 100.)]));
     }
+
+    #[test]
+    fn a_tab_held_off_its_strip_lets_the_others_slide_home_and_drops_nowhere() {
+        let mut d = holding(0, 160.);
+        d.away = true;
+        d.ease(Instant::now(), true);
+        assert_eq!((d.offsets.clone(), d.held()), (vec![0.; 3], None));
+        assert_eq!(d.release(), None);
+    }
+
+    #[test]
+    fn the_held_tab_s_middle_follows_the_pointer_past_the_strip() {
+        assert_eq!(holding(1, 500.).mid(), Some(540.));
+    }
 }
```

**Step 4: Run them**

Run: `cargo test -p pocket panels`
Expected: PASS, 23 tests.

### Task 5.3: Drop a tab on another pane

**Files:**
- Modify: `packages/desktop/crates/pocket/src/panels.rs` (`Panels`, `sync_panels`, `panels_view`; new `aim_tab`)
- Modify: `packages/desktop/crates/pocket/src/panels/strip.rs` (`DragTab`, `grab`, `release_tab`, `strip_tabs`)

**Context:**
- `on_drag_move` fires on every element registered for the type, hovered or not. One handler on the main area therefore sees the held tab everywhere.
- `aim_tab` returns early when the pointer hasn't moved. A drag move comes every frame, and notifying on each would redraw forever, as in the strip's handler.
- Each strip records its on-screen slots in `panels.slots` while a tab is held. `TabDrag`'s slots are unscrolled, but the drop needs window positions.
- A release:
  - `TabDrag::release` returns a move within the strip.
  - Otherwise, a `To` aim moves the tab across with `move_tab_to` (PR 4).

  Every strip's `on_mouse_up_out` calls `release_tab`, and both are once-only, so the first call wins.
- When the tab leaves its pane, the pane's count changes, `lay` fails, and the strip drops its drag.

**Status: uncompiled.**

**Step 1: Implement**

In `strip.rs`:
- Make `struct DragTab` and the field `grab` `pub(super)`.
- In `strip_tabs`, inside the `if let Some((_, d)) = self.panels.drag…` block, record the window slots before `lay`:

```rust
            let slots = slots(&scroll, tabs.len());
            let scrolled = f32::from(scroll.offset().x);
            self.panels.slots.insert(p, slots.iter().map(|&(l, w)| (l - scrolled, w)).collect());
```

- Replace `release_tab`. Its two callers in `strip_tabs` pass `window`:

```rust
    fn release_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let aim = self.panels.aim.take();
        let Some((pane, d)) = self.panels.drag.as_mut() else { return };
        let (pane, from) = (*pane, d.from);
        match (d.release(), aim) {
            (Some((from, to)), _) => self.move_tab(pane, from, to, cx),
            (None, Some(Aim::To(to))) => self.move_tab_to(pane, from, to, window, cx),
            _ => {}
        }
    }
```

In `panels.rs`:
- Add `use drop::Aim;`.
- Add these fields to `Panels`:

```rust
    /// Where the held tab would drop, once it has left its strip.
    pub(crate) aim: Option<Aim>,
    /// The pointer while a tab is held, in the window.
    at: Option<(f32, f32)>,
    /// Each strip's tabs on screen while a tab is held.
    slots: drop::Slots,
```

- In `sync_panels`:

```rust
        if self.panels.drag.is_none() {
            (self.panels.aim, self.panels.at) = (None, None);
            self.panels.slots.clear();
        }
```

- In `panels_view`, on the `div().relative().flex_1()…` that holds `measure` and `body`:

```rust
            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<strip::DragTab>, _, cx| this.aim_tab((e.event.position.x.into(), e.event.position.y.into()), cx)))
```

- Add to `impl Desktop`:

```rust
    /// Follows a held tab over the panes; off its own strip, its siblings slide home and the drop lights up.
    fn aim_tab(&mut self, at: (f32, f32), cx: &mut Context<Self>) {
        let Some(tree) = self.cwd() else { return };
        if self.panels.at == Some(at) {
            return;
        }
        let Some((from, d)) = self.panels.drag.as_mut() else { return };
        let t = &self.workspaces[&tree].tree;
        let layout = match t.zoomed {
            Some(z) => vec![(z, self.panels.bounds)],
            None => t.layout(self.panels.bounds),
        };
        let tabs = t.pane(*from).map_or(0, |p| p.tabs.len());
        let aim = d.mid().and_then(|mid| drop::aim(&layout, at, *from, tabs, mid, &self.panels.slots));
        d.away = aim != Some(Aim::Home);
        (self.panels.aim, self.panels.at) = (aim, Some(at));
        cx.notify();
    }
```

**Step 2: Build**

Run: `cargo build -p pocket`
Expected: builds. `drop::highlight`, `drop::caret` and `STRIP_H` still warn as unused.

### Task 5.4: Show where it lands

**Files:**
- Modify: `packages/desktop/crates/pocket/src/panels.rs` (`panels_view`; new `drop_marks`)

**Context:**
- The marks are drawn on the main area, positioned from `panels.bounds`. They are `deferred`, so they paint over every pane, and they don't take the mouse.
- For `Into` over a strip, the caret is a 2px `ACCENT` bar, 20px tall, centred in the strip. Otherwise the lit rect is `ACCENT` at 12% with a 1px `ACCENT` border, rounded 8px.
- While `away`, the held tab follows the pointer as a ghost, drawn with `tab_lead` in the raised tab style.

**Status: uncompiled.**

**Step 1: Implement**

Add after `.child(body)` in `panels_view`:

```rust
.children(self.drop_marks(tree))
```

Then add:

```rust
    /// The caret or lit pane where the held tab would drop, and the tab itself under the pointer.
    fn drop_marks(&self, tree: &str) -> Option<Deferred> {
        let ((from, d), aim, (x, y)) = (self.panels.drag.as_ref()?, self.panels.aim?, self.panels.at?);
        let Aim::To(target) = aim else { return None };
        let t = &self.workspaces[tree].tree;
        let o = self.panels.bounds;
        let rect_of = |p| t.layout(o).into_iter().find(|&(q, _)| q == p).map(|(_, r)| r);
        let mark = match target {
            Target::Into { pane, index } if rect_of(pane).is_some_and(|r| y - r.y < drop::STRIP_H) => {
                let r = rect_of(pane)?;
                let cx_ = drop::caret(self.panels.slots.get(&pane).map_or(&[][..], Vec::as_slice), index).unwrap_or(r.x + 12.);
                div().absolute().left(px(cx_ - 1. - o.x)).top(px(r.y + 11. - o.y)).w(px(2.)).h(px(20.)).rounded(px(1.)).bg(ACCENT)
            }
            Target::Into { pane, .. } | Target::Split { pane, .. } => {
                let edge = if let Target::Split { edge, .. } = target { Some(edge) } else { None };
                let h = drop::highlight(rect_of(pane)?, edge);
                div().absolute().left(px(h.x - o.x)).top(px(h.y - o.y)).w(px(h.w)).h(px(h.h)).rounded(px(8.)).border_1().border_color(ACCENT).bg(Hsla::from(ACCENT).opacity(0.12))
            }
        };
        let tab = t.pane(*from)?.tabs.get(d.from)?;
        let ghost = div()
            .absolute()
            .left(px(x - d.grab - o.x))
            .top(px(y - 14. - o.y))
            .h(px(28.))
            .px(px(10.))
            .flex()
            .items_center()
            .rounded(px(7.))
            .bg(SURFACE)
            .shadow(ui::row_shadow())
            .text_size(px(12.5))
            .font_weight(FontWeight::SEMIBOLD)
            .text_color(TEXT)
            .whitespace_nowrap()
            .child(self.tab_lead(tab, false, TEXT));
        Some(deferred(div().absolute().inset_0().child(mark).child(ghost)))
    }
```

`tab_lead` becomes `pub(super)` in `strip.rs`.

**Step 2: Build**

Run: `cargo build -p pocket`
Expected: builds. Only `TabDrag::mid` and the web snapshot may still be unused.

### Task 5.5: A page can snapshot itself

**Files:**
- Modify: `packages/desktop/Cargo.toml` (`objc2-web-kit` workspace dependency)
- Modify: `packages/desktop/crates/web/Cargo.toml`
- Modify: `packages/desktop/crates/web/src/page.rs` (`Page::snapshot`)
- Modify: `packages/desktop/crates/web/src/page/macos.rs`
- Create: `packages/desktop/crates/web/src/page/macos/snapshot.rs`

**Context:**
- `WKWebView.takeSnapshotWithConfiguration:completionHandler:` calls back on the main thread with an `NSImage`. The image's TIFF is re-encoded as PNG, which `gpui::Image` takes.
- `done` sits in a `Cell`, since WebKit's block is `Fn` and may in theory run more than once. On other platforms, `done(None)` is called at once.
- No test, because it needs a live WKWebView.

**Status:** compiled. `cargo clippy -p web --all-targets` is clean. `Cargo.lock` gains `objc2-web-kit 0.3.2`.

**Step 1: Implement**

```diff
diff --git a/packages/desktop/Cargo.toml b/packages/desktop/Cargo.toml
index c914323..7ed9561 100644
--- a/packages/desktop/Cargo.toml
+++ b/packages/desktop/Cargo.toml
@@ -49,6 +49,7 @@ merman = { version = "=0.8.0-alpha.6", default-features = false, features = ["sv
 objc2 = "0.6"
 objc2-app-kit = { version = "0.3", default-features = false, features = ["std", "NSAccessibility", "NSApplication", "NSDockTile", "NSResponder", "NSWorkspace"] }
 objc2-foundation = { version = "0.3", default-features = false, features = ["std", "NSString"] }
+objc2-web-kit = { version = "0.3", default-features = false, features = ["std"] }
 qrcode = { version = "0.14", default-features = false }
 resvg = "0.46"
 serde = { version = "1", features = ["derive"] }
diff --git a/packages/desktop/crates/web/Cargo.toml b/packages/desktop/crates/web/Cargo.toml
index be51959..d743c3d 100644
--- a/packages/desktop/crates/web/Cargo.toml
+++ b/packages/desktop/crates/web/Cargo.toml
@@ -13,5 +13,6 @@ wry.workspace = true
 [target.'cfg(target_os = "macos")'.dependencies]
 block2.workspace = true
 objc2.workspace = true
-objc2-app-kit = { workspace = true, features = ["block2", "NSEvent", "NSTrackingArea", "NSView", "NSWindow"] }
-objc2-foundation = { workspace = true, features = ["NSBundle", "NSGeometry"] }
+objc2-app-kit = { workspace = true, features = ["block2", "NSBitmapImageRep", "NSEvent", "NSImage", "NSImageRep", "NSTrackingArea", "NSView", "NSWindow"] }
+objc2-foundation = { workspace = true, features = ["NSBundle", "NSData", "NSDictionary", "NSError", "NSGeometry"] }
+objc2-web-kit = { workspace = true, features = ["block2", "objc2-app-kit", "WKWebView", "WKSnapshotConfiguration"] }
diff --git a/packages/desktop/crates/web/src/page.rs b/packages/desktop/crates/web/src/page.rs
index a503716..fef4090 100644
--- a/packages/desktop/crates/web/src/page.rs
+++ b/packages/desktop/crates/web/src/page.rs
@@ -157,6 +157,14 @@ impl Page {
         let _ = self.view.set_visible(visible);
     }
 
+    /// Calls `done` with a PNG of what the page shows, or `None` if it can't be taken.
+    pub fn snapshot(&self, done: impl FnOnce(Option<Vec<u8>>) + 'static) {
+        #[cfg(target_os = "macos")]
+        macos::snapshot(&self.view, done);
+        #[cfg(not(target_os = "macos"))]
+        done(None);
+    }
+
     pub fn load(&self, url: &str) {
         let _ = self.view.load_url(url);
     }
diff --git a/packages/desktop/crates/web/src/page/macos.rs b/packages/desktop/crates/web/src/page/macos.rs
index 7600022..d888bcb 100644
--- a/packages/desktop/crates/web/src/page/macos.rs
+++ b/packages/desktop/crates/web/src/page/macos.rs
@@ -1,5 +1,8 @@
 mod cursor;
 mod press;
+mod snapshot;
+
+pub(super) use snapshot::snapshot;
 
 use super::{Edit, Rect};
 use objc2::rc::Retained;
diff --git a/packages/desktop/crates/web/src/page/macos/snapshot.rs b/packages/desktop/crates/web/src/page/macos/snapshot.rs
new file mode 100644
index 0000000..2c589ca
--- /dev/null
+++ b/packages/desktop/crates/web/src/page/macos/snapshot.rs
@@ -0,0 +1,25 @@
+use block2::RcBlock;
+use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImage};
+use objc2_foundation::{NSDictionary, NSError};
+use std::cell::Cell;
+use wry::{WebView, WebViewExtMacOS};
+
+pub(crate) fn snapshot(view: &WebView, done: impl FnOnce(Option<Vec<u8>>) + 'static) {
+    let done = Cell::new(Some(done));
+    let block = RcBlock::new(move |image: *mut NSImage, _: *mut NSError| {
+        // SAFETY: WebKit passes a live image, or null when the snapshot failed.
+        let png = unsafe { image.as_ref() }.and_then(png);
+        if let Some(done) = done.take() {
+            done(png);
+        }
+    });
+    // SAFETY: on the main thread, like every call into the page; WebKit retains the block until it calls it.
+    unsafe { view.webview().takeSnapshotWithConfiguration_completionHandler(None, &block) };
+}
+
+fn png(image: &NSImage) -> Option<Vec<u8>> {
+    let rep = NSBitmapImageRep::imageRepWithData(&*image.TIFFRepresentation()?)?;
+    // SAFETY: no properties is a valid set of them.
+    let data = unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new()) }?;
+    Some(data.to_vec())
+}
```

**Step 2: Build**

Run: `cargo clippy -p web --all-targets`
Expected: no warnings.

### Task 5.6: Pages hold still under a held tab

**Files:**
- Modify: `packages/desktop/crates/pocket/src/browser.rs` (`Browser`, `Browsers::new`, `sync_browser`; new `freeze_pages`)
- Modify: `packages/desktop/crates/pocket/src/browser/view.rs` (`browser_view` draws the snapshot)
- Modify: `packages/desktop/crates/pocket/src/panels/strip.rs` (the `on_drag` start calls `freeze_pages`)

**Context:**
- When a tab drag starts, every shown page is asked for a snapshot, and its `shot` is cleared. `sync_browser` counts a held tab as covering the pages, so they park at once.
- Until a shot arrives (one or two frames), the page's place shows `SURFACE`.
- The snapshot comes back on an objc callback with no `App`. Like page events, it is sent down a channel that a task spawned in `Browsers::new` drains.
- Shots are dropped once the drag ends. Pages then place themselves again on the next `sync_browser`.

**Status: uncompiled.**

**Step 1: Implement**

In `browser.rs`:
- Add `pub(crate) shot: Option<Arc<Image>>` to `Browser`, documented "What the page showed when a tab was picked up; drawn in its place while the page is parked for the drag."
- Add `shots: UnboundedSender<(u64, Option<Vec<u8>>)>` to `Browsers`.
- Spawn the drain in `new`, beside the events task:

```rust
        let (shots, mut shots_rx) = unbounded();
        cx.spawn(async move |this, cx| {
            while let Some((id, png)) = shots_rx.next().await {
                let applied = this.update(cx, |d: &mut Desktop, cx| {
                    if let (Some(b), Some(png)) = (d.browsers.tabs.get_mut(&id), png) {
                        b.shot = Some(Arc::new(Image::from_bytes(ImageFormat::Png, png)));
                        cx.notify();
                    }
                });
                if applied.is_err() {
                    break;
                }
            }
        })
        .detach();
```

- In `sync_browser`, add `|| self.panels.drag.is_some()` to `covered`. After `self.browsers.show(...)`, add:

```rust
        if self.panels.drag.is_none() {
            self.browsers.tabs.values_mut().for_each(|b| b.shot = None);
        }
```

- Add:

```rust
    /// Asks each shown page for a picture of itself, to draw while a held tab parks the pages.
    pub(crate) fn freeze_pages(&mut self) {
        for &id in &self.browsers.shown {
            let Some(page) = self.browsers.tabs.get_mut(&id).and_then(|b| { b.shot = None; b.page.clone() }) else { continue };
            let shots = self.browsers.shots.clone();
            page.snapshot(move |png| {
                let _ = shots.unbounded_send((id, png));
            });
        }
    }
```

In `browser/view.rs`, inside the page's area, draw the shot over the placing canvas while one exists:

```rust
            .when_some(self.browsers.tabs.get(&id).and_then(|b| b.shot.clone()), |d, shot| d.child(img(shot).absolute().inset_0().size_full()))
```

In `strip.rs`, in the `on_drag` closure, after `this.panels.drag = Some(...)`:

```rust
                                this.freeze_pages();
```

`freeze_pages` must run before the next render's `sync_browser`, because `sync_browser` parks the pages, and `shown` must still name them.

**Step 2: Run the tests**

Run: `cargo test -p pocket panels`
Expected: PASS, 23 tests.

**Step 3: Gate**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: builds; clippy shows no warnings beyond PR 4's; all tests pass.

**Step 4: Screens**

Run (repo root): `.ui-review/fixture/capture.sh /tmp/split-pr5-after session=session split=session,explore,file browser=session,browser`
Expected: identical to `/tmp/split-pr4-after`.

**Step 5: By hand** (`cargo run --release -p pocket`)

With a terminal on the left and a file and a page on the right:
- Drag the file's tab:
  - onto the left strip between two tabs: the caret shows, and it lands there
  - to the bottom third of the left pane: the lower half lights, and a pane opens below
  - into the middle of the left pane: the whole body lights, and it joins at the end
- Drag a tab off its strip and back: its siblings slide home and back.
- Drop it on its own body: nothing moves.
- Drag the page's tab: the page shows as a still picture until the drop, then is live again where it landed.
- Drag a pane's only tab to that pane's edge: no highlight.

## PR 6: Remember each worktree's panels

**Scope:**
- `workspace` gains serde: `Tree` writes `{root, focused}` and reads back through a new `Tree::from_root`.
- `Workspace::saved` drops browser tabs and the unpinned preview; `Workspace::restored` drops terminals pocketd no longer runs.
- `store` gains `layouts`, keyed by worktree path.
- `pocket` restores layouts once, on pocketd's first terminal list, and saves them with the window geometry (debounced, plus on quit and on close).
- Not saved: zoom, browser tabs, unpinned preview tabs.
- Capture mode neither restores nor saves.

**Depends on:** PR 4 (`Workspace { tree, preview }`, `Tab::Term(String)`, `Panels`).
**Done when:**
- The gate is green.
- Split a worktree into three panels, quit, relaunch: the same panels, tabs, sizes and focus come back.
- Terminals that ended meanwhile are gone, and the panels they emptied are closed.

### Task 6.1: Trees read back sound

**Files:**
- Modify: `packages/desktop/crates/workspace/Cargo.toml` (whole file, 8 lines at HEAD)
- Modify: `packages/desktop/crates/workspace/src/tree.rs`:
  - the derives on `Axis`, `Pane`, `Node` and `Tree`;
  - add `Saved`, `Tree::from_root` and tests.

**Context:**
- `Tree::next` is private and must stay larger than every pane id, or a later split reuses an id.
- `desktop.json` can be stale or hand-edited. So reading a tree never trusts it as-is: it goes through `from_root`, which runs the same `tidy` every edit runs, then renumbers.
- `zoomed` is transient and is not written.
- `serde` and `serde_json` are already in `[workspace.dependencies]` (`packages/desktop/Cargo.toml`), with serde's `derive` feature on.

**Step 1: Write the failing tests** (append to `mod tests` in `tree.rs`)

```rust
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
```

What each test proves:
1. Ids continue after the saved ones.
2. A stale focus falls back to the first pane.
3. Saved empty panes close.
4. A fully empty saved tree can't hand out a colliding id. The lone pane keeps the stale focus id 7, and the next id must still be 8.
5. JSON round-trips without the zoom.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p workspace a_rebuilt_tree`
Expected: compile FAIL with `no function or associated item named 'from_root' found for struct 'Tree'`, plus unresolved `serde_json`.

**Step 3: Write the implementation**

Set `packages/desktop/crates/workspace/Cargo.toml` to:

```toml
[package]
name = "workspace"
version = "0.0.0"
edition.workspace = true
publish.workspace = true

[lib]
path = "src/workspace.rs"

[dependencies]
serde.workspace = true

[dev-dependencies]
serde_json.workspace = true
```

In `tree.rs`:
- Put `use serde::{Deserialize, Serialize};` above `pub type PaneId = u64;`.
- Add `Serialize, Deserialize` to the derives of `Axis`, `Pane<T>` and `Node<T>`. `Axis` becomes `#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]`; `Pane` and `Node` become `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]`.
- Replace the `Tree` struct with:

```rust
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
```

Add as the first method of `impl<T> Tree<T>`:

```rust
    /// The tree around `root`: empty panes closed, `focused` kept if it is still a pane, new panes numbered after every pane in it.
    pub fn from_root(root: Node<T>, focused: PaneId) -> Self {
        let mut t = Self { root, focused, zoomed: None, next: 0 };
        t.tidy();
        t.next = t.panes().iter().map(|p| p.id + 1).max().unwrap_or(0);
        t
    }
```

`next` is computed after `tidy`, because an emptied tree comes back as one pane carrying the old focus id.

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p workspace`
Expected: PASS, all tree tests included (`test result: ok.`).

### Task 6.2: What a workspace saves and what survives a relaunch

**Files:**
- Modify: `packages/desktop/crates/workspace/src/workspace.rs`:
  - the derives on `Doc`, `Tab` and `Workspace`;
  - add `is_empty`, `saved`, `restored` and tests.

**Context:**
- After PR 4 a workspace is `Workspace { pub tree: Tree<Tab>, pub preview: Option<Doc> }`.
- `preview` names the unpinned doc tab the next open takes over. It is a glance, so it is not kept.
- Browser pages (`Tab::Web`) are WKWebViews that die with the app.
- Terminals live in pocketd and may have ended while the app was closed. `restored` takes the ids pocketd lists and drops the rest; `Tree::retain` already closes the panels this empties.
- If PR 4 already added an emptiness check on `Workspace`, reuse it instead of adding `is_empty`.

**Step 1: Write the failing tests** (append to `mod tests` in `workspace.rs`; add `use crate::tree::Edge;` there if PR 4's tests don't already import it)

```rust
    fn term(id: &str) -> Tab {
        Tab::Term(id.into())
    }

    fn file(path: &str) -> Doc {
        Doc::File(path.into())
    }

    fn panes(w: &Workspace) -> Vec<Vec<Tab>> {
        w.tree.panes().iter().map(|p| p.tabs.clone()).collect()
    }

    #[test]
    fn saving_keeps_terminals_pinned_docs_and_the_layout_but_no_pages_or_preview() {
        let mut w = Workspace { preview: Some(file("/b")), ..Default::default() };
        w.tree.push(0, term("t1"));
        w.tree.push(0, Tab::Doc(file("/a")));
        let right = w.tree.split(0, Edge::Right, Tab::Doc(file("/b"))).unwrap();
        w.tree.push(right, Tab::Web(1));
        w.tree.split(0, Edge::Bottom, Tab::Web(2));
        let saved = w.saved();
        assert_eq!(panes(&saved), vec![vec![term("t1"), Tab::Doc(file("/a"))]]);
        assert_eq!(saved.preview, None);
    }

    #[test]
    fn saving_leaves_the_open_workspace_alone() {
        let mut w = Workspace::default();
        w.tree.push(0, Tab::Web(1));
        let _ = w.saved();
        assert_eq!(panes(&w), vec![vec![Tab::Web(1)]]);
    }

    #[test]
    fn restoring_drops_terminals_pocketd_no_longer_runs_and_closes_their_panels() {
        let mut w = Workspace::default();
        w.tree.push(0, term("gone"));
        w.tree.push(0, Tab::Doc(file("/a")));
        let right = w.tree.split(0, Edge::Right, term("dead")).unwrap();
        w.tree.split(right, Edge::Bottom, term("alive"));
        let back = w.restored(&["alive".into()]);
        assert_eq!(panes(&back), vec![vec![Tab::Doc(file("/a"))], vec![term("alive")]]);
    }

    #[test]
    fn a_workspace_whose_terminals_all_ended_restores_empty() {
        let mut w = Workspace::default();
        w.tree.push(0, term("gone"));
        assert!(w.restored(&[]).is_empty());
    }
```

If PR 4's test module already defines `term`, `file` or a tab-listing helper, reuse it and drop the duplicate.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p workspace saving`
Expected: compile FAIL with `no method named 'saved' found for struct 'Workspace'`.

**Step 3: Write the implementation**

Change the derives:
- `Doc` and `Tab` become `#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]`.
- `Workspace` becomes `#[derive(Debug, PartialEq, Default, Serialize, Deserialize)]`.
- Add `use serde::{Deserialize, Serialize};` at the top.

Add to `impl Workspace`:

```rust
    pub fn is_empty(&self) -> bool {
        self.tree.panes().iter().all(|p| p.tabs.is_empty())
    }

    /// What outlives the app: browser pages die with it and the unpinned preview is only a glance, so neither is kept.
    pub fn saved(&self) -> Self {
        let mut tree = self.tree.clone();
        tree.retain(|t| match t {
            Tab::Web(_) => false,
            Tab::Doc(d) => self.preview.as_ref() != Some(d),
            Tab::Term(_) => true,
        });
        Self { tree, preview: None }
    }

    /// A saved workspace without the terminals pocketd no longer runs; the panels they leave empty close.
    pub fn restored(mut self, live: &[String]) -> Self {
        self.tree.retain(|t| !matches!(t, Tab::Term(id) if !live.contains(id)));
        self
    }
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p workspace`
Expected: PASS.

### Task 6.3: `desktop.json` keeps each worktree's panels

**Files:**
- Modify: `packages/desktop/crates/store/Cargo.toml:10-12`
- Modify: `packages/desktop/crates/store/src/store.rs`:
  - `1-3` (imports);
  - `68-83` (`Store`);
  - `169-177` (old-json test);
  - add a test in `mod tests` (`147-265`).

**Context:**
- `Store` is `#[serde(default)]`, so a `desktop.json` written before this PR loads with no layouts.
- `layouts` is keyed by the worktree's path, the same key as `Desktop.workspaces`.
- `store` and `workspace` are both model crates, so the new dependency respects ADR 0003.
- `BTreeMap` mirrors `repos`, so the file diffs stably.

**Step 1: Write the failing tests**

Add to `mod tests`:

```rust
    #[test]
    fn panels_round_trip_through_desktop_json() {
        use workspace::tree::Edge;
        use workspace::{Doc, Tab, Workspace};
        let dir = std::env::temp_dir().join(format!("pocket-store-layouts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Store::load(&dir);
        let mut w = Workspace::default();
        w.tree.push(0, Tab::Term("t1".into()));
        w.tree.split(0, Edge::Right, Tab::Doc(Doc::File("/w/a.rs".into())));
        s.layouts.insert("/w".into(), w);
        s.save();
        assert_eq!(Store::load(&dir).layouts, s.layouts);
        std::fs::remove_dir_all(&dir).unwrap();
    }
```

In `an_old_desktop_json_loads_with_default_geometry`, add before `remove_dir_all`:

```rust
        assert!(s.layouts.is_empty());
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p store panels_round_trip`
Expected: compile FAIL with `unresolved import 'workspace'` and `no field 'layouts' on type 'Store'`.

**Step 3: Write the implementation**

`store/Cargo.toml` `[dependencies]` becomes:

```toml
[dependencies]
serde.workspace = true
serde_json.workspace = true
workspace.workspace = true
```

In `store.rs`, add `use workspace::Workspace;` after line 3, then add this field to `Store` after `pub sounds: Sounds,`:

```rust
    /// Each worktree's panels, keyed by the worktree's path.
    pub layouts: BTreeMap<String, Workspace>,
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p store`
Expected: PASS (10 tests).

### Task 6.4: Restore once pocketd has listed its terminals, save with the chrome

**Files:**
- Create: `packages/desktop/crates/pocket/src/panels/layouts.rs`
- Modify: `packages/desktop/crates/pocket/src/panels.rs`, which PR 4 adds: `mod layouts;` and a `restored` field on `Panels`.
- Modify: `packages/desktop/crates/pocket/src/terminals.rs:179-192` (`"terminals"` arm of `on_msg`; anchor by `"terminals" =>` if PR 4 moved it).
- Modify: `packages/desktop/crates/pocket/src/desktop/geometry.rs:65-85` (`save_soon`, `remember_chrome`).
- Modify: `packages/desktop/crates/pocket/src/desktop/project.rs:225-249` (`delete_worktree`).
- Modify: every pocket method that changes a worktree's panels (found with the grep in Step 3).

**Context:**

*Why restore waits for the list.* Saved terminal ids mean nothing until pocketd says which still run. Restoring earlier would draw dead terminals, and deciding later per poll would drop a just-spawned terminal pocketd hasn't listed yet. That race is why `Workspace::sync` keeps unknown ids. So restore runs exactly once, on the first `"terminals"` message:
- after `self.terminals.listed(...)`, so `self.terminals.sessions` holds the live ids;
- before `arrived()` adopts created terminals.

*Saving.* It mirrors the window geometry: `remember_chrome` copies state into `self.store` and encodes it. `save_soon` writes after 500ms of quiet, and the `on_release` and `on_app_quit` hooks in `Geometry::new` write on exit.
- Layouts are copied into the store only once restored. Otherwise a save before pocketd answers (or while it is down) would overwrite the saved layouts with empty ones.
- Only workspaces that still hold a tab after `saved()` are written. A worktree deleted outside the app thus drops out once its panels empty.

*Other cases.*
- If the user already opened something in a worktree before the first list arrived, that workspace wins over its saved layout.
- Capture mode (`self.capturing`) uses the real home's `desktop.json`, so it must neither restore nor save. `remember_chrome` already returns early when capturing.
- No new field goes on `Desktop`: the one-shot flag lives on the panels feature's own `Panels` state.

**Step 1: No unit test.** `Desktop` holds entities and can't be built in a test. The logic it calls is covered by tasks 6.1–6.3; this task is wiring only.

**Step 2:** Skipped. There is no failing test, because this task is wiring only.

**Step 3: Write the implementation**

Add the flag to `Panels` in `panels.rs`, and initialise it to `false` wherever PR 4 builds `Panels`:

```rust
    /// Whether the panels saved last launch were brought back; until then `desktop.json` keeps them as they were.
    pub(crate) restored: bool,
```

and declare the module next to PR 4's other `panels/` modules:

```rust
mod layouts;
```

Create `packages/desktop/crates/pocket/src/panels/layouts.rs`:

```rust
use crate::desktop::Desktop;
use workspace::Workspace;

impl Desktop {
    /// Brings back last launch's panels, once, after pocketd's first terminal list says which terminals still run.
    pub(crate) fn restore_layouts(&mut self) {
        if std::mem::replace(&mut self.panels.restored, true) || self.capturing {
            return;
        }
        let live: Vec<String> = self.terminals.sessions.items.iter().map(|s| s.info.id.clone()).collect();
        for (tree, saved) in std::mem::take(&mut self.store.layouts) {
            if self.workspaces.get(&tree).is_none_or(Workspace::is_empty) {
                self.workspaces.insert(tree, saved.restored(&live));
            }
        }
    }

    /// Copies every worktree's panels that hold a tab into the store, once they have been restored.
    pub(crate) fn remember_layouts(&mut self) {
        if self.panels.restored {
            self.store.layouts = self.workspaces.iter().map(|(t, w)| (t.clone(), w.saved())).filter(|(_, w)| !w.is_empty()).collect();
        }
    }
}
```

In `terminals.rs`, the `"terminals"` arm becomes:

```rust
            "terminals" => {
                for id in self.terminals.unclosed(&m.items) {
                    self.daemon.send(json!({"op": "close", "id": id}));
                }
                for id in self.terminals.listed(m.items) {
                    self.daemon.send(json!({"op": "attach", "id": id}));
                }
                self.restore_layouts();
                for (id, tree) in self.terminals.arrived() {
                    self.adopt(id, tree, None, window, cx);
                }
                if self.project.is_none() {
                    self.project = self.projects().into_iter().next();
                }
            }
```

Keep whatever argument PR 4 gave `adopt`; only the `restore_layouts` line is new.

In `geometry.rs`:
- Line 65's doc becomes `/// Writes \`desktop.json\` once the window, layout, widths and panels have held still for \`SAVE_DELAY\`.`
- Line 74's doc becomes `/// Copies the window, layout, widths and panels into the store and encodes it; capture mode keeps none of them.`
- In `remember_chrome`, add `self.remember_layouts();` on the line before `self.store.encode()`.

In `project.rs` `delete_worktree`, after `d.workspaces.remove(&tree);` (line 237), add:

```rust
                        d.store.layouts.remove(&tree);
                        d.save_soon(cx);
```

`d.store.layouts.remove` covers a delete made before restore, which `remember_layouts` would not touch.

Then make every panel change schedule a save. List the call sites:

```
grep -rnE "\.tree\.(split|move_tab|take|retain|resize|equalize|equalize_all|push|select|focus)\(|\.(open_term|open_doc|open_web|pin|close|remove)\(" crates/pocket/src
```

In each `Desktop` method this finds that mutates a `Workspace` or its `tree`, add `self.save_soon(cx);` next to its `cx.notify()`. Skip render paths and `Workspace::sync`. Expect at least:
- `adopt`, `select_tab`, `close_tab`, `close_pane`;
- `open_doc` and `pin_doc`;
- PR 4's divider drag move and double-click handlers;
- PR 5's tab drop (`release_tab` or its successor);
- `open_browser`.

`save_soon` debounces, so calling it on every divider move costs one timer reset.

**Step 4: Verify**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected:
- The build passes.
- Clippy shows only the baseline: 5× "a `Vec` of `Range` that is only one element", 2× "field assignment outside of initializer", 1× "`Iterator::last` on a `DoubleEndedIterator`", plus the `block v0.1.6` future-incompat note.
- All tests pass.

Manual check:
1. Run `cargo run --release -p pocket`.
2. In one worktree, open a shell, ⌘T a second one, split right with the strip's split button, and open a file pinned (double-click) in the right panel.
3. Drag the divider to roughly 30/70.
4. Quit (⌘Q) and relaunch.
5. Expect the same two panels, sizes, tabs and focused panel.
6. Now quit, end one of the terminals with `pocketctl` (or close it from another window), and relaunch. Its tab is gone, and if it was alone, so is its panel.
7. Run `grep -c layouts ~/…/desktop.json` on the file `Store::load` reads. Expect `1`.

## PR 7: Panel shortcuts

**Scope:**
- Bindings and thin handlers for split right/down, focus a panel by direction, previous/next tab, zoom and equalize.
- `Tree::step`.
- Focusing another pane ends a zoom.
- Only the zoomed pane is drawn, and pages hidden by a zoom are parked.
- ⌘W already closes the focused panel's active tab after PR 4; this PR only checks it.

**Depends on:** PR 4 (`Place`, `select_tab(pane, i)`, panels render) and PR 6 (`save_soon` after equalize).
**Done when:**
- The gate is green.
- Every shortcut below works from a terminal, a diff and the empty area. Browser pages that hold the keyboard keep their own keys, as ⌘T does today.

| Keys | Binding | Action |
|---|---|---|
| ⌘D | `cmd-d` | `SplitRight` |
| ⌘⇧D | `cmd-shift-d` | `SplitDown` |
| ⌘⌥←/→/↑/↓ | `cmd-alt-left` … | `FocusPane(Edge)` |
| ⌘⇧[ / ⌘⇧] | `cmd-{` / `cmd-}` | `PrevTab` / `NextTab` |
| ⌘⇧↩ | `cmd-shift-enter` | `ZoomPane` |
| ⌘⌃= | `cmd-ctrl-=` | `EqualizePanes` |

Conflicts checked:
- **⌘⇧[ and ⌘⇧] are written `cmd-{` and `cmd-}`.** `gpui-pre-macos` `events.rs::parse_keystroke` reports ⌘⇧[ as key `{` with shift cleared, so `cmd-shift-[` would never match. Zed's macOS keymap writes them the same way. They also don't collide with the browser's `cmd-[` (Back).
- **⌘⌥←/→ in a terminal** used to send `ESC b` / `ESC f` (`keys::key_bytes` checks `alt` before `platform`). Bindings dispatch before the terminal's `on_key_down`, so these keys now move panels. ⌥←/→ alone still sends word jumps.
- **No other clashes.** ⌘D, ⌘⇧D, ⌘⇧↩ and ⌘⌃= are bound nowhere in `actions.rs`, `keys`, `gpui-component` 0.6.6 or `gpui-kit` 0.6.6.
- **⌘D, ⌘⇧D in doc and browser panels:** they still open a **shell** beside the focused panel. A split is asked for to put a terminal next to the work, and an empty panel would need a second step. This is the same as the strip's split buttons.

### Task 7.1: Stepping through a panel's tabs; focus ends a zoom

**Files:**
- Modify: `packages/desktop/crates/workspace/src/tree.rs`:
  - `focus`, `select`;
  - add `step` and `drawn`;
  - add tests.

**Context:**
- `step` only answers which tab is beside the shown one. `Desktop::select_tab` then shows it and moves keyboard focus into it, so `step` takes `&self`.
- Zoom becomes reachable in this PR. A zoom hides every other pane, so focusing one of them (⌘⌥arrow, a sidebar jump via `focus_agent`, a click) must end the zoom.
- `drawn` is what's on screen. Pages and terminal fits must follow it, not `panes()`.

**Step 1: Write the failing tests** (append to `mod tests`)

```rust
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
```

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p workspace -- stepping focusing_another a_zoomed_tree`
Expected:
- compile FAIL with `no method named 'step'` and `no method named 'drawn'`;
- after adding stubs, `focusing_another_pane_ends_the_zoom` fails with `left: Some(1) right: None`.

**Step 3: Write the implementation**

Replace `focus` and `select`:

```rust
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
```

Add after `panes`:

```rust
    /// The panes on screen: the zoomed one alone, else all of them.
    pub fn drawn(&self) -> Vec<&Pane<T>> {
        match self.zoomed {
            Some(z) => self.panes().into_iter().filter(|p| p.id == z).collect(),
            None => self.panes(),
        }
    }
```

Add before `toggle_zoom`:

```rust
    /// The tab beside the focused pane's shown one, after it or before it, wrapping around.
    pub fn step(&self, forward: bool) -> Option<(PaneId, usize)> {
        let p = self.focused();
        let n = p.tabs.len();
        (n > 0).then(|| (p.id, if forward { (p.active + 1) % n } else { (p.active + n - 1) % n }))
    }
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p workspace`
Expected: PASS.

### Task 7.2: Bind the shortcuts

**Files:**
- Modify: `packages/desktop/crates/pocket/src/actions.rs`:
  - `1-13` (imports, `actions!`, action structs);
  - `15-50` (`bindings`);
  - `52-66` (tests).

**Context:**
- Bindings with `None` context work anywhere in the window. `JumpTo` and `PageEdit` show how to give an action a payload.
- The existing `no_two_bindings_share_a_keystroke_and_context` test also checks `keys::bindings()`, so it catches clashes.
- The new test pins the decided keymap, so a later edit can't silently drop one. It parses the keys the way GPUI does, so `cmd-{` is compared as typed.

**Step 1: Write the failing test** (in `mod tests`, change `use super::bindings;` to `use super::bindings;` plus `use gpui_kit::Keystroke;`)

```rust
    /// The actions bound to `keys` outside any context.
    fn bound(keys: &str) -> Vec<&'static str> {
        let want = vec![Keystroke::parse(keys).unwrap().unparse()];
        bindings().iter().filter(|b| b.predicate().is_none() && b.keystrokes().iter().map(|k| k.unparse()).collect::<Vec<_>>() == want).map(|b| b.action().name()).collect()
    }

    #[test]
    fn the_panel_shortcuts_work_everywhere() {
        let panel = [("cmd-d", "desktop::SplitRight"), ("cmd-shift-d", "desktop::SplitDown"), ("cmd-alt-left", "desktop::FocusPane"), ("cmd-alt-right", "desktop::FocusPane"), ("cmd-alt-up", "desktop::FocusPane"), ("cmd-alt-down", "desktop::FocusPane"), ("cmd-{", "desktop::PrevTab"), ("cmd-}", "desktop::NextTab"), ("cmd-shift-enter", "desktop::ZoomPane"), ("cmd-ctrl-=", "desktop::EqualizePanes"), ("cmd-w", "desktop::CloseTab")];
        for (keys, action) in panel {
            assert_eq!(bound(keys), [action], "{keys}");
        }
    }
```

**Step 2: Run the test to verify it fails**

Run: `cargo test -p pocket the_panel_shortcuts_work_everywhere`
Expected: FAIL with `assertion 'left == right' failed: cmd-d`, `left: []`.

**Step 3: Write the implementation**

Lines 1–13 become:

```rust
use crate::browser;
use gpui_kit::*;
use workspace::tree::Edge;

actions!(desktop, [OpenPalette, GoToFile, OpenSession, StartSession, NextNeedsYou, GoToUpNext, NextSession, PrevSession, ToggleRail, ToggleFocus, NewWorktree, ProjectSettings, NewTab, CopySelection, SelectAll, Paste, CloseTab, Save, Quit, NewBrowser, FocusAddress, Reload, Back, Forward, SplitRight, SplitDown, PrevTab, NextTab, ZoomPane, EqualizePanes]);

/// The nth session in the visible list, 1-based.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct JumpTo(pub usize);

#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct PageEdit(pub web::Edit);

/// Focuses the panel touching the focused one at this edge.
#[derive(Clone, PartialEq, Debug, Action)]
#[action(namespace = desktop, no_json)]
pub struct FocusPane(pub Edge);
```

If PR 1 re-exports `Edge` at the crate root, import `workspace::Edge` instead.

In `bindings()`, after `KeyBinding::new("cmd-shift-b", NewBrowser, None),` add:

```rust
        KeyBinding::new("cmd-d", SplitRight, None),
        KeyBinding::new("cmd-shift-d", SplitDown, None),
        KeyBinding::new("cmd-alt-left", FocusPane(Edge::Left), None),
        KeyBinding::new("cmd-alt-right", FocusPane(Edge::Right), None),
        KeyBinding::new("cmd-alt-up", FocusPane(Edge::Top), None),
        KeyBinding::new("cmd-alt-down", FocusPane(Edge::Bottom), None),
        // macOS reports ⌘⇧[ as `{` with shift already applied (gpui-pre-macos parse_keystroke), so `cmd-shift-[` would never match.
        KeyBinding::new("cmd-{", PrevTab, None),
        KeyBinding::new("cmd-}", NextTab, None),
        KeyBinding::new("cmd-shift-enter", ZoomPane, None),
        KeyBinding::new("cmd-ctrl-=", EqualizePanes, None),
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p pocket actions::tests`
Expected: PASS (2 tests). `pocket` will warn about the unused actions until Task 7.3; that is fine for this step.

### Task 7.3: The handlers

**Files:**
- Create: `packages/desktop/crates/pocket/src/panels/shortcuts.rs`
- Modify: `packages/desktop/crates/pocket/src/panels.rs` (`mod shortcuts;`)
- Modify: `packages/desktop/crates/pocket/src/desktop.rs:443-445` (the `on_action` chain in `Render`; anchor after `.on_action(cx.listener(Self::close_active_tab))`)

**Context:**
- Handlers stay thin: pick the worktree, ask the tree, call the existing `Desktop` methods. `select_tab(pane, i, window, cx)` (PR 4) shows a tab and moves keyboard focus into it.
- Like `close_active_tab` (`terminal_view.rs:273-280`), nothing fires while a modal is open or when the worktree has no panels (`session_tree()` is `None`).
- `new_shell(Place, cx)` (PR 4) opens a shell into a place, and the split edge picks the side.
- Focusing an empty panel moves keyboard focus to `self.root`, so typing doesn't keep going to the terminal that just lost focus.
- Zoom isn't saved, so it doesn't call `save_soon`; equalize does.

**Step 1: No unit test.** The choices live in `Tree::neighbour`, `step`, `toggle_zoom` and `equalize_all`, which are already tested. These handlers only wire them to the window.

**Step 2:** Skipped. There is no failing test, because this task is wiring only.

**Step 3: Write the implementation**

Create `packages/desktop/crates/pocket/src/panels/shortcuts.rs`:

```rust
use crate::actions::{EqualizePanes, FocusPane, NextTab, PrevTab, SplitDown, SplitRight, ZoomPane};
use crate::desktop::Desktop;
use gpui_kit::*;
use workspace::Place;
use workspace::tree::Edge;

impl Desktop {
    /// The worktree whose panels a shortcut acts on; none behind a modal or before any panel holds a tab.
    fn panel_tree(&mut self) -> Option<String> {
        self.session_tree().filter(|_| self.overlay.is_none())
    }

    pub(crate) fn split_right(&mut self, _: &SplitRight, _: &mut Window, cx: &mut Context<Self>) {
        self.split_focused(Edge::Right, cx);
    }

    pub(crate) fn split_down(&mut self, _: &SplitDown, _: &mut Window, cx: &mut Context<Self>) {
        self.split_focused(Edge::Bottom, cx);
    }

    fn split_focused(&mut self, edge: Edge, cx: &mut Context<Self>) {
        let Some(tree) = self.panel_tree() else { return };
        let pane = self.workspace(&tree).tree.focused;
        self.new_shell(Place::Split(pane, edge), cx);
    }

    pub(crate) fn focus_toward(&mut self, a: &FocusPane, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.panel_tree() else { return };
        let t = &mut self.workspace(&tree).tree;
        let Some(to) = t.neighbour(t.focused, a.0) else { return };
        let shown = t.pane(to).filter(|p| !p.tabs.is_empty()).map(|p| p.active);
        t.focus(to);
        match shown {
            Some(i) => self.select_tab(to, i, window, cx),
            None => {
                window.focus(&self.root, cx);
                cx.notify();
            }
        }
    }

    pub(crate) fn prev_tab(&mut self, _: &PrevTab, window: &mut Window, cx: &mut Context<Self>) {
        self.step_tab(false, window, cx);
    }

    pub(crate) fn next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        self.step_tab(true, window, cx);
    }

    fn step_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.panel_tree() else { return };
        if let Some((pane, i)) = self.workspace(&tree).tree.step(forward) {
            self.select_tab(pane, i, window, cx);
        }
    }

    pub(crate) fn zoom_pane(&mut self, _: &ZoomPane, _: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.panel_tree() else { return };
        self.workspace(&tree).tree.toggle_zoom();
        cx.notify();
    }

    pub(crate) fn equalize_panes(&mut self, _: &EqualizePanes, _: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.panel_tree() else { return };
        self.workspace(&tree).tree.equalize_all();
        self.save_soon(cx);
        cx.notify();
    }
}
```

In `panels.rs`, next to `mod layouts;`:

```rust
mod shortcuts;
```

In `desktop.rs` `Render`, after `.on_action(cx.listener(Self::close_active_tab))`:

```rust
            .on_action(cx.listener(Self::split_right))
            .on_action(cx.listener(Self::split_down))
            .on_action(cx.listener(Self::focus_toward))
            .on_action(cx.listener(Self::prev_tab))
            .on_action(cx.listener(Self::next_tab))
            .on_action(cx.listener(Self::zoom_pane))
            .on_action(cx.listener(Self::equalize_panes))
```

If PR 4's `select_tab` or `new_shell` signatures differ (argument order, or `window` taken), match them; the logic stays the same.

**Step 4: Verify**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets`
Expected: the build passes, and clippy shows the baseline only.

### Task 7.4: Draw only the zoomed panel

**Files:**
- Modify: PR 4's panels render entry: the function in `packages/desktop/crates/pocket/src/panels.rs` or `panels/body.rs` that starts drawing from `tree.root`.
- Modify: every pocket site that decides which pages and terminals are on screen from `tree.panes()`. Find them with `grep -rn "\.panes()" crates/pocket/src`; expect `sync_browser` (`browser.rs`) and the terminal fit pass.

**Context:**
- A zoom draws one panel over the whole main area. The others aren't drawn, so their WKWebViews must be parked (native views would otherwise float above the zoomed panel) and their terminals must not be refit to zero.
- `Tree::drawn()` is the single answer to what's on screen.
- Use `drawn()` only where on-screen matters; lookups such as `find`/`tab_of` keep searching every pane.

**Step 1: No unit test.** `drawn` is tested in Task 7.1; this is render wiring.

**Step 2:** Skipped. There is no failing test, because this task is render wiring only.

**Step 3: Write the implementation**

At the render entry, draw the zoomed pane alone. PR 4 names the pane and node renderers; `pane_view` and `node_view` below stand for them:

```rust
        let body = match w.tree.zoomed.and_then(|z| w.tree.pane(z).cloned()) {
            Some(pane) => self.pane_view(&pane, window, cx),
            None => self.node_view(&w.tree.root.clone(), Vec::new(), window, cx),
        };
```

At each on-screen site the grep finds, replace `tree.panes()` with `tree.drawn()`. Dividers come from the split nodes, so they disappear with the zoom without further change.

**Step 4: Verify**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: build passes, clippy at the baseline, all tests pass.

Then check the screens in capture mode. The `zoom` and `equalize` steps exist only if PR 4 added them to `capture.rs`. Otherwise run the app with `cargo run --release -p pocket` and check by hand.

```
.ui-review/fixture/capture.sh /tmp/pr7 split=new-tab,split-right zoomed=new-tab,split-right,zoom
```

Manual checks:
1. ⌘D, then ⌘⇧D: a shell opens right of the focused panel, then below the new one.
2. ⌘⌥← and ⌘⌥→ move focus between them, and typing goes to the focused shell.
3. ⌘⇧] and ⌘⇧[ cycle the focused panel's tabs and wrap.
4. With a browser panel open, ⌘⇧↩ on a shell panel hides the page; ⌘⇧↩ again brings it back. ⌘⌥→ while zoomed unzooms and focuses the neighbour.
5. Drag a divider, then ⌘⌃=: the panels even out.
6. ⌘W closes the focused panel's active tab, and closing its last tab closes the panel.
