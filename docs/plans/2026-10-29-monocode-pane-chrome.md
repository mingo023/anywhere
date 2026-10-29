# Monocode-style Pane Chrome Implementation Plan

> **For Claude:** REQUIRED SUB-SKILL: Use /implement to execute this plan task-by-task.

**Goal:** Give the Pocket desktop monocode's pane chrome: a top bar of tabs that each hold their own pane tree, slim 36px pane headers (tabs only where a pane has tabs, a title only for a lone terminal in a split), no file header, and drag and drop that moves panes, splits them into top tabs and puts top tabs back into a split.

**Architecture:** The `workspace` crate gains the model: a worktree's `Workspace` holds several pane trees (`tree` is the shown one, `others` the rest, `at` its place in the bar), with pane ids unique across all of them, plus tree ops that move a whole pane. `pocket` gets one new view module, `panels/top_tabs.rs`, for the bar, and reworks `panels.rs` / `panels/strip.rs` / `panels/drop.rs` for the slimmer headers and the pane drag. Decisions stay in free functions over plain data (`top_tabs::copy`, `panels::head`, `drop::pane_aim`, `drop::top_index`) so they can be tested.

**Toolset** (run from `packages/desktop`):
- One crate's tests: `cargo test -p workspace` / `cargo test -p pocket`
- One test: `cargo test -p workspace -- tree::tests::moving_a_pane_puts_it_at_the_edge_of_another`
- Everything before a commit: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace` (clippy must add no new warnings)
- Screens: `.ui-review/fixture/capture.sh /tmp/chrome-before main=` before PR 2 starts, then `/tmp/chrome-after` after each view PR, and compare. Step syntax is in `crates/pocket/src/capture.rs`.

**Read first:**
- `CLAUDE.md` (repo root): layout rules, the test style (sentence names, no mocks, no render tests), the performance rules (every notify redraws the window; render costs only what shows).
- `docs/adr/0003-desktop-code-layout.md`: model crates hold logic, `pocket` holds views, one file per component.
- `.ui-review/prototypes/monocode-panes.html`: the prototype the user approved. Open it in Chrome; it holds every size, colour and behaviour this plan builds. Its JS `tabCopy`, `openFile`, `detachPane`, `detachFile`, `placeSession`, `startPaneDrag`, `edgeFromPoint` are the spec for the matching Rust.
- `crates/workspace/src/tree.rs` and `crates/workspace/src/workspace.rs`: the model this plan extends. Read `Tree::split`, `take`, `tidy`, `from_root`, `Node::split`, `Node::prune`.
- `crates/pocket/src/panels.rs`, `panels/strip.rs`, `panels/drop.rs`, `panels/drop_marks.rs`: today's pane bar, tab strip and tab drag. The new drags copy their shape.

## Decisions (settled with the user, or the default where they didn't say)

- Scope: everything in one go (user's pick). Shipped as 4 PRs so each one merges green.
- A top tab is a pane tree inside one worktree. The sidebar still picks the worktree; the top bar picks a tree inside it.
- New terminals pocketd reports for the worktree still land in the shown tree's top-left pane, as today. The top bar's **+** opens a new agent in a new top tab.
- Pane ids are unique across a workspace's trees, so per-pane state (`preview.panes`, `diff.panes`, …) can survive a top-tab switch. `pane_ids()` returns every tree's panes.
- A doc is looked up only in the shown tree (`doc_tab`), so the same file may be open in two top tabs. A terminal is looked up in all trees; focusing an agent switches to its top tab.
- Closing the last tab of a tree closes its top tab, unless it is the only one.
- Closing a top tab closes all its tabs, with the same checks `close_tab` does (unsaved file → its confirm; running terminals → `ask_close`).
- Pane head: a lone pane whose only tab is a terminal has **no** head. The same pane in a split gets a 36px **title** head (grip, focus dot, title, ×). Every other pane gets the 36px **tab strip**.
- Removed from pane bars: the changes chip (the worktree's +/− moves to its sidebar row), the split-right / split-down buttons (⌘D, ⌘⇧D and the tab menu still split), the More button. The context ring moves to the right end of the top bar, for the focused pane.
- The file header (`explorer/preview/header.rs`) goes. Preview/Source floats over a markdown body. Ask about this file, Open in default app, Reveal in Finder, Copy path / relative path / file name and Close others move into the tab's right-click menu, which now opens on any tab, not only when a pane has two.
- `more.rs` loses its non-file branch (New terminal tab, Close session); only the diff's More button opened it with no file, and the sidebar row menu already has Close session.
- Pocket's existing tab drag into other panes and its edge split stay. The new drags are added next to it.
- Drag and drop: dragging a pane by its grip, its strip's empty space or its title head moves it; edge by the dominant axis of the pointer within the target pane (monocode `edgeFromPoint`); dropping on the top bar makes it a top tab. A file tab dragged onto the top bar becomes a top tab. A top tab dragged down onto a pane joins the shown tree at that edge. Esc cancels any of them.

---

## PR 1: Workspaces hold several pane trees

**Scope:** `workspace` crate only, plus the three call sites whose meaning changes (`focus_agent`, the alerts check, `pane_ids`). Nothing draws a second tree yet, so the app looks and behaves as before.
**Depends on:** nothing
**Done when:** `cargo test -p workspace` passes with the new tests, the whole workspace builds and its tests pass, and an old `desktop.json` still loads.

### Task 1.1: Tree ops that move a whole pane

**Files:**
- Modify: `packages/desktop/crates/workspace/src/tree.rs` (the `impl<T> Tree<T>` block, ~line 286-551, and its tests at the bottom)

**Context:** `Tree<T>` is a split tree whose leaves are `Pane`s of tabs. `tidy()` restores the invariants after an edit: it prunes empty panes, merges a split into a parent of the same axis, and fixes the focus. `Node::split(target, edge, new)` puts a node at an edge of a pane, and `prune` merging the new two-way split into a same-axis parent is exactly monocode's "join the sibling split and halve the target", so reuse it. `next` is the next free pane id; it is private and `#[serde(skip)]`.

Add:
- `from_root_at(root, focused, start)`: `from_root`, numbering panes from `start`. `from_root` becomes `from_root_at(root, focused, 0)`.
- `lone(id, tab)`: a tree of one pane.
- `next_id()` and `reserve(next)`: read and raise `next`, so a workspace can keep ids clear across trees.
- `detach(pane) -> Option<Pane<T>>`: takes a pane out whole, unless it is the last.
- `attach(pane, edge, node, focus) -> Result<(), Node<T>>`: puts a node (already numbered clear of this tree) at an edge, and hands it back if `pane` is gone.
- `move_pane(from, to, edge)`.

**Step 1: Write the failing tests** (append inside `mod tests` in `tree.rs`)

```rust
    #[test]
    fn moving_a_pane_puts_it_at_the_edge_of_another_and_focuses_it() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        let c = t.split(b, Edge::Right, "c").unwrap();
        t.move_pane(c, 0, Edge::Bottom);
        assert_eq!((tabs(&t), t.focused), (vec![vec!["a"], vec!["c"], vec!["b"]], c));
        assert_eq!(t.layout(UNIT)[1].1, Rect { x: 0., y: 0.5, w: 2. / 3., h: 0.5 });
    }

    #[test]
    fn a_pane_moved_beside_a_sibling_along_its_split_halves_that_sibling() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Bottom, "b").unwrap();
        let c = t.split(0, Edge::Right, "c").unwrap();
        t.move_pane(c, b, Edge::Right);
        assert_eq!(tabs(&t), vec![vec!["a"], vec!["b"], vec!["c"]]);
        assert_eq!(t.layout(UNIT)[2].1, Rect { x: 0.5, y: 0.5, w: 0.5, h: 0.5 });
    }

    #[test]
    fn a_pane_cannot_move_beside_itself_or_to_a_missing_pane() {
        let mut t = with(&["a"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        t.move_pane(b, b, Edge::Left);
        t.move_pane(b, 9, Edge::Left);
        assert_eq!(tabs(&t), vec![vec!["a"], vec!["b"]]);
    }

    #[test]
    fn detaching_takes_a_pane_out_whole_but_never_the_last() {
        let mut t = with(&["a", "x"]);
        let b = t.split(0, Edge::Right, "b").unwrap();
        let p = t.detach(0).unwrap();
        assert_eq!((p.tabs, tabs(&t), t.focused), (vec!["a", "x"], vec![vec!["b"]], b));
        assert_eq!(t.detach(b), None);
    }

    #[test]
    fn attaching_to_a_missing_pane_hands_the_node_back() {
        let mut t = with(&["a"]);
        let node = Node::Pane(Pane { id: 5, tabs: vec!["b"], active: 0 });
        assert!(t.attach(9, Edge::Right, node, 5).is_err());
        assert_eq!(tabs(&t), vec![vec!["a"]]);
    }

    #[test]
    fn a_tree_numbered_from_a_start_opens_new_panes_past_it() {
        let mut t = Tree::from_root_at(with(&["a"]).root, 0, 10);
        assert_eq!((t.focused, t.next_id()), (10, 11));
        t.reserve(20);
        assert_eq!(t.split(10, Edge::Right, "b"), Some(20));
    }
```

`fn moving_a_pane…`: `c` leaves the row and lands under `a`; its quarter is shared out in proportion as when a pane closes, so `a`'s column keeps 2/3. `fn a_pane_moved_beside_a_sibling…`: the column `[a|c] / b` becomes `a / [b|c]`, and `c` takes half of `b`'s row, not a nested split. The others pin the edge cases.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p workspace tree::tests`
Expected: FAIL to compile, "no method named `move_pane`" / "`detach`" / "`attach`" / "`from_root_at`" / "`reserve`" / "`next_id`".

**Step 3: Write the implementation**

Replace `from_root` and add the rest in `impl<T> Tree<T>` right after it:

```rust
    /// The tree around `root`: panes renumbered in reading order, so saved ids can neither repeat nor overflow, empty panes closed, and `focused` kept if it is still a pane.
    pub fn from_root(root: Node<T>, focused: PaneId) -> Self {
        Self::from_root_at(root, focused, 0)
    }

    /// As `from_root`, numbering panes from `start`, so the tree can sit beside others without sharing an id.
    pub fn from_root_at(mut root: Node<T>, focused: PaneId, start: PaneId) -> Self {
        let (mut next, mut found) = (start, None);
        root.repair(&mut next, &mut found, focused);
        let mut t = Self { root, focused: found.unwrap_or(next), zoomed: None, next: 0 };
        t.tidy();
        t.next = t.panes().iter().map(|p| p.id + 1).max().unwrap_or(start).max(start);
        t
    }

    /// A tree of one pane, numbered `id`, holding `tab`.
    pub fn lone(id: PaneId, tab: T) -> Self {
        Self { root: Node::Pane(Pane { id, tabs: vec![tab], active: 0 }), focused: id, zoomed: None, next: id + 1 }
    }

    /// The id the next pane opened here gets; every pane's is below it.
    pub fn next_id(&self) -> PaneId {
        self.next
    }

    /// Numbers panes opened from now on `next` or above, clear of another tree's.
    pub fn reserve(&mut self, next: PaneId) {
        self.next = self.next.max(next);
    }
```

Add after `move_tab`:

```rust
    /// Takes `pane` out whole, tabs and all, focusing the pane that took its place; the last pane stays.
    pub fn detach(&mut self, pane: PaneId) -> Option<Pane<T>> {
        if self.panes().len() < 2 {
            return None;
        }
        let p = self.pane_mut(pane)?;
        let taken = Pane { id: p.id, tabs: std::mem::take(&mut p.tabs), active: p.active };
        self.tidy();
        Some(taken)
    }

    /// Puts `node`, numbered clear of this tree's panes, at `edge` of `pane` and focuses `focus`, one of its panes; hands `node` back when `pane` is gone.
    pub fn attach(&mut self, pane: PaneId, edge: Edge, node: Node<T>, focus: PaneId) -> Result<(), Node<T>> {
        self.root.split(pane, edge, node)?;
        self.next = self.panes().iter().map(|p| p.id + 1).max().unwrap_or(0).max(self.next);
        self.focused = focus;
        self.zoomed = None;
        self.tidy();
        Ok(())
    }

    /// Moves pane `from` to `edge` of pane `to` and focuses it.
    pub fn move_pane(&mut self, from: PaneId, to: PaneId, edge: Edge) {
        if from == to || self.pane(to).is_none() {
            return;
        }
        let Some(p) = self.detach(from) else { return };
        let id = p.id;
        // `to` still holds its tabs, so detaching `from` left it in the tree.
        _ = self.attach(to, edge, Node::Pane(p), id);
    }
```

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p workspace tree::tests`
Expected: PASS, every old tree test included.

### Task 1.2: A workspace's top tabs

**Files:**
- Modify: `packages/desktop/crates/workspace/src/workspace.rs` (struct ~line 31-36, `sync`, `open_term`, `Place`, `remove`, `move_files`, `close_files`, `close`, `is_empty`, `saved`, `restored`; tests at the bottom)

**Context:** `Workspace` is one worktree's tabs. Today it has one `tree`; keep that field as the **shown** top tab, so the ~50 `w.tree` uses in `pocket` keep working, and add `others` (the hidden trees, in bar order) and `at` (where `tree` sits in the bar). The bar is `others` with `tree` inserted at `at`. Pane ids stay unique across trees: every time a tree becomes shown or joins, call `reserve()` so the shown tree opens new panes past every id in use. `desktop.json` stores workspaces through serde (`crates/store/src/store.rs:343`, `readable_layouts`). Old files have no `others`/`at`, so they default, and a custom `From` renumbers hidden trees clear of the shown one (each saved tree is read back numbered from 0).

**Step 1: Write the failing tests** (append inside `mod tests` in `workspace.rs`)

```rust
    fn bar(w: &Workspace) -> Vec<Vec<Vec<Tab>>> {
        w.tops().iter().map(|t| t.panes().iter().map(|p| p.tabs.clone()).collect()).collect()
    }

    fn ids_of(w: &Workspace) -> Vec<PaneId> {
        w.tops().iter().flat_map(|t| t.panes()).map(|p| p.id).collect()
    }

    /// Top tabs holding terminals `a`, `b` and `c`, the middle one shown.
    fn three() -> Workspace {
        let mut w = with(&["a"]);
        w.open_term("b".into(), Place::Top);
        w.open_term("c".into(), Place::Top);
        w.switch(1);
        w
    }

    #[test]
    fn a_new_top_tab_opens_after_the_shown_one_and_shows() {
        let mut w = with(&["a"]);
        w.open_term("b".into(), Place::Top);
        w.switch(0);
        w.open_term("c".into(), Place::Top);
        assert_eq!(bar(&w), vec![vec![vec![term("a")]], vec![vec![term("c")]], vec![vec![term("b")]]]);
        assert_eq!((w.at, shown(&w)), (1, Some(&term("c"))));
    }

    #[test]
    fn switching_keeps_the_bar_in_order() {
        let mut w = three();
        w.switch(0);
        assert_eq!((w.at, shown(&w)), (0, Some(&term("a"))));
        w.switch(2);
        assert_eq!((w.at, shown(&w)), (2, Some(&term("c"))));
        assert_eq!(bar(&w), vec![vec![vec![term("a")]], vec![vec![term("b")]], vec![vec![term("c")]]]);
    }

    #[test]
    fn pane_ids_never_repeat_across_top_tabs() {
        let mut w = three();
        w.open_term("d".into(), Place::Split(w.tree.focused, Edge::Right));
        w.switch(0);
        w.open_term("e".into(), Place::Split(w.tree.focused, Edge::Bottom));
        let ids = ids_of(&w);
        let mut unique = ids.clone();
        unique.sort();
        unique.dedup();
        assert_eq!(ids.len(), unique.len());
    }

    #[test]
    fn a_terminal_in_a_hidden_top_tab_is_not_given_a_second_tab() {
        let mut w = three();
        w.sync(&ids(&["a", "b", "c"]), &[]);
        assert_eq!(bar(&w).concat().concat().len(), 3);
    }

    #[test]
    fn showing_a_terminal_switches_to_the_top_tab_holding_it() {
        let mut w = three();
        assert_eq!(w.show_term("c"), Some((w.tree.focused, 0)));
        assert_eq!((w.at, w.holds_term("a"), w.holds_term("x")), (2, true, false));
        w.open_term("a".into(), Place::Pane(None));
        assert_eq!((w.at, shown(&w)), (0, Some(&term("a"))));
    }

    #[test]
    fn closing_the_shown_top_tab_shows_the_one_after_it_else_before() {
        let mut w = three();
        assert_eq!(w.close_top(1), vec![term("b")]);
        assert_eq!((w.at, shown(&w)), (1, Some(&term("c"))));
        w.close_top(1);
        assert_eq!((w.at, shown(&w)), (0, Some(&term("a"))));
        w.close_top(0);
        assert!(w.is_empty());
    }

    #[test]
    fn closing_a_hidden_top_tab_leaves_the_shown_one_shown() {
        let mut w = three();
        w.close_top(0);
        assert_eq!((w.at, shown(&w)), (0, Some(&term("b"))));
    }

    #[test]
    fn a_top_tab_whose_last_tab_closes_closes_too_unless_it_is_the_only_one() {
        let mut w = three();
        w.close(w.tree.focused, 0);
        assert_eq!((w.tops().len(), shown(&w)), (2, Some(&term("c"))));
        w.remove(&ids(&["a"]));
        assert_eq!(w.tops().len(), 1);
        w.remove(&ids(&["c"]));
        assert_eq!((w.tops().len(), w.is_empty()), (1, true));
    }

    #[test]
    fn a_top_tab_moves_and_the_shown_one_stays_shown() {
        let mut w = three();
        w.move_top(0, 2);
        assert_eq!(bar(&w), vec![vec![vec![term("b")]], vec![vec![term("c")]], vec![vec![term("a")]]]);
        assert_eq!((w.at, shown(&w)), (0, Some(&term("b"))));
        w.move_top(0, 1);
        assert_eq!((w.at, shown(&w)), (1, Some(&term("b"))));
    }

    #[test]
    fn a_pane_detached_to_the_bar_becomes_a_top_tab_there() {
        let mut w = with(&["a"]);
        let (d, _) = w.open_doc(file("/x"), true, None);
        w.detach_pane(d, 0);
        assert_eq!(bar(&w), vec![vec![vec![Tab::Doc(file("/x"))]], vec![vec![term("a")]]]);
        assert_eq!((w.at, shown(&w)), (1, Some(&term("a"))));
        w.detach_pane(w.tree.focused, 0);
        assert_eq!(w.tops().len(), 2);
    }

    #[test]
    fn a_tab_detached_to_the_bar_leaves_alone_unless_it_is_its_panes_only_tab() {
        let mut w = with(&["a"]);
        let (d, _) = w.open_doc(file("/x"), true, None);
        w.open_doc(file("/y"), true, Some(d));
        w.detach_tab(d, 1, 1);
        assert_eq!(bar(&w), vec![vec![vec![term("a")], vec![Tab::Doc(file("/x"))]], vec![vec![Tab::Doc(file("/y"))]]]);
        w.detach_tab(d, 0, 2);
        assert_eq!(bar(&w), vec![vec![vec![term("a")]], vec![vec![Tab::Doc(file("/y"))]], vec![vec![Tab::Doc(file("/x"))]]]);
        w.detach_tab(w.tree.focused, 0, 3);
        assert_eq!(w.tops().len(), 3);
    }

    #[test]
    fn a_top_tab_placed_on_a_pane_joins_the_shown_tree_at_that_edge() {
        let mut w = three();
        let b = w.tree.focused;
        w.place_top(2, b, Edge::Right);
        assert_eq!(bar(&w), vec![vec![vec![term("a")]], vec![vec![term("b")], vec![term("c")]]]);
        assert_eq!((w.at, shown(&w)), (1, Some(&term("c"))));
        w.place_top(0, b, Edge::Bottom);
        assert_eq!((w.at, tabs(&w)), (0, vec![vec![term("b")], vec![term("a")], vec![term("c")]]));
    }

    #[test]
    fn renames_trashes_and_saves_reach_every_top_tab() {
        let mut w = three();
        w.switch(0);
        w.open_doc(file("/r/a.rs"), true, None);
        w.open_doc(Doc::File("/r/web".into()), true, None);
        w.tree.push(w.tree.focused, Tab::Web(1));
        w.switch(2);
        w.move_files("/r", "/s");
        w.close_files("/s/web", |_| false);
        let saved = w.saved();
        assert_eq!(bar(&saved), vec![vec![vec![term("a")], vec![Tab::Doc(file("/s/a.rs"))]], vec![vec![term("b")]], vec![vec![term("c")]]]);
        assert_eq!(saved.at, 2);
        assert_eq!(bar(&saved.restored(&ids(&["b"]))), vec![vec![vec![Tab::Doc(file("/s/a.rs"))]], vec![vec![term("b")]]]);
    }

    #[test]
    fn a_saved_workspace_reads_back_with_its_top_tabs_and_unique_pane_ids() {
        let w = three();
        let back: Workspace = serde_json::from_str(&serde_json::to_string(&w.saved()).unwrap()).unwrap();
        assert_eq!((bar(&back), back.at), (bar(&w), 1));
        assert_eq!(ids_of(&back), vec![1, 0, 2]);
    }

    #[test]
    fn a_workspace_saved_before_top_tabs_still_reads_back() {
        let old = r#"{"tree":{"root":{"Pane":{"id":4,"tabs":[{"Term":"a"}],"active":0}},"focused":4},"preview":null}"#;
        let w: Workspace = serde_json::from_str(old).unwrap();
        assert_eq!((bar(&w), w.at), (vec![vec![vec![term("a")]]], 0));
    }
```

What each proves: the first eight pin bar order, switching, id uniqueness, cross-tree terminal lookup and closing; the next three are the drag-and-drop model (`detach_pane` / `detach_tab` / `place_top`); the last three pin that every whole-workspace op and the file format reach the hidden trees. In `a_saved_workspace_reads_back…`, `ids_of` is in bar order: hidden `a` gets 1 (numbered after the shown tree's 0), shown `b` 0, hidden `c` 2.

`serde_json` is already a dev-dependency of `workspace`.

**Step 2: Run the tests to verify they fail**

Run: `cargo test -p workspace`
Expected: FAIL to compile, "no variant `Top`", "no method named `tops` / `switch` / `show_term` / `close_top` …".

**Step 3: Write the implementation**

1. Add `Place::Top`:

```rust
/// Where a new terminal opens: in a pane, the terminal pane nearest the focus when none is given or it is gone, in a new pane at an edge of one, or in a new top tab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    Pane(Option<PaneId>),
    Split(PaneId, Edge),
    Top,
}
```

2. Replace the struct and add the read path (import `tree::Node` and `tree::Pane`):

```rust
#[derive(Debug, PartialEq, Default, Serialize, Deserialize)]
#[serde(from = "Saved")]
pub struct Workspace {
    /// The shown top tab's panes.
    pub tree: Tree<Tab>,
    /// The other top tabs' panes, in bar order; the shown one sits among them at `at`.
    pub others: Vec<Tree<Tab>>,
    pub at: usize,
    /// The doc whose tab the next unpinned open in its pane takes over.
    pub preview: Option<Doc>,
}

/// What a workspace is written as; a file from before top tabs has only `tree`.
#[derive(Deserialize)]
struct Saved {
    tree: Tree<Tab>,
    #[serde(default)]
    others: Vec<Tree<Tab>>,
    #[serde(default)]
    at: usize,
    #[serde(default)]
    preview: Option<Doc>,
}

/// Each saved tree reads back numbered from 0, so the hidden ones are renumbered past the shown one.
impl From<Saved> for Workspace {
    fn from(s: Saved) -> Self {
        let mut next = s.tree.next_id();
        let others: Vec<_> = s
            .others
            .into_iter()
            .filter(|t| !empty(t))
            .map(|t| {
                let t = Tree::from_root_at(t.root, t.focused, next);
                next = t.next_id();
                t
            })
            .collect();
        let mut w = Self { at: s.at.min(others.len()), tree: s.tree, others, preview: s.preview };
        w.reserve();
        w
    }
}

fn empty(t: &Tree<Tab>) -> bool {
    t.panes().iter().all(|p| p.tabs.is_empty())
}
```

3. In `impl Workspace`, add the bar ops:

```rust
    /// Every top tab's panes in bar order, the shown one at `at`.
    pub fn tops(&self) -> Vec<&Tree<Tab>> {
        let mut all: Vec<_> = self.others.iter().collect();
        all.insert(self.at, &self.tree);
        all
    }

    /// The id past every pane of every top tab.
    fn next_id(&self) -> PaneId {
        self.others.iter().map(Tree::next_id).fold(self.tree.next_id(), PaneId::max)
    }

    /// Keeps panes the shown tree opens clear of every other tree's.
    fn reserve(&mut self) {
        let next = self.next_id();
        self.tree.reserve(next);
    }

    /// Takes the bar apart, for edits easier on one list; `put_bar` puts it back.
    fn take_bar(&mut self) -> Vec<Tree<Tab>> {
        let mut bar = std::mem::take(&mut self.others);
        bar.insert(self.at, std::mem::take(&mut self.tree));
        bar
    }

    fn put_bar(&mut self, mut bar: Vec<Tree<Tab>>, at: usize) {
        if bar.is_empty() {
            bar.push(Tree::default());
        }
        self.at = at.min(bar.len() - 1);
        self.tree = bar.remove(self.at);
        self.others = bar;
        self.reserve();
    }

    /// Shows top tab `i`.
    pub fn switch(&mut self, i: usize) {
        if i != self.at && i <= self.others.len() {
            let bar = self.take_bar();
            self.put_bar(bar, i);
        }
    }

    /// Opens a top tab right after the shown one, holding `tab`, and shows it.
    fn open_top(&mut self, tab: Tab) {
        let tree = Tree::lone(self.next_id(), tab);
        self.others.insert(self.at, std::mem::replace(&mut self.tree, tree));
        self.at += 1;
    }

    /// Adds the panes of `root` as a top tab at bar index `to`, numbered clear of the rest, leaving the shown one shown.
    fn add_top(&mut self, root: Node<Tab>, focused: PaneId, to: usize) {
        let tree = Tree::from_root_at(root, focused, self.next_id());
        let to = to.min(self.others.len() + 1);
        let j = if to <= self.at {
            self.at += 1;
            to
        } else {
            to - 1
        };
        self.others.insert(j, tree);
    }

    /// Moves top tab `from` to bar index `to`, the shown one staying shown.
    pub fn move_top(&mut self, from: usize, to: usize) {
        let n = self.others.len() + 1;
        if from >= n || to >= n || from == to {
            return;
        }
        let a = self.at;
        let at = match a {
            a if a == from => to,
            a if from < a && a <= to => a - 1,
            a if to <= a && a < from => a + 1,
            a => a,
        };
        let mut bar = self.take_bar();
        let t = bar.remove(from);
        bar.insert(to, t);
        self.put_bar(bar, at);
    }

    /// Closes top tab `i` and hands back the tabs it held; the one after it shows, else the one before. The last top tab empties instead.
    pub fn close_top(&mut self, i: usize) -> Vec<Tab> {
        if i > self.others.len() {
            return Vec::new();
        }
        let mut bar = self.take_bar();
        let gone = bar.remove(i);
        let at = if i < self.at { self.at - 1 } else { self.at };
        self.put_bar(bar, at);
        let tabs: Vec<Tab> = gone.panes().iter().flat_map(|p| p.tabs.clone()).collect();
        if self.preview.as_ref().is_some_and(|d| tabs.contains(&Tab::Doc(d.clone()))) {
            self.preview = None;
        }
        tabs
    }

    /// Moves `pane` out of the shown top tab into a top tab of its own at bar index `to`; the last pane stays.
    pub fn detach_pane(&mut self, pane: PaneId, to: usize) {
        if let Some(p) = self.tree.detach(pane) {
            let id = p.id;
            self.add_top(Node::Pane(p), id, to);
        }
    }

    /// Moves tab `i` of `pane` into a top tab of its own at bar index `to`. A pane's only tab takes its pane along; the shown tree's last tab stays.
    pub fn detach_tab(&mut self, pane: PaneId, i: usize, to: usize) {
        let Some(n) = self.tree.pane(pane).map(|p| p.tabs.len()).filter(|&n| i < n) else { return };
        if n == 1 {
            return self.detach_pane(pane, to);
        }
        let tab = self.tree.take(pane, i).expect("checked above");
        self.add_top(Node::Pane(Pane { id: 0, tabs: vec![tab], active: 0 }), 0, to);
    }

    /// Moves top tab `i`'s panes into the shown one at `edge` of `pane`, focusing the pane it had focused.
    pub fn place_top(&mut self, i: usize, pane: PaneId, edge: Edge) {
        if i == self.at || i > self.others.len() || self.tree.pane(pane).is_none() {
            return;
        }
        let j = if i < self.at {
            self.at -= 1;
            i
        } else {
            i - 1
        };
        let t = self.others.remove(j);
        let t = Tree::from_root_at(t.root, t.focused, self.next_id());
        let focus = t.focused;
        // `pane` was checked above.
        _ = self.tree.attach(pane, edge, t.root, focus);
    }

    /// Whether any top tab holds terminal `id`.
    pub fn holds_term(&self, id: &str) -> bool {
        self.tops().iter().any(|t| t.find(|tab| matches!(tab, Tab::Term(t) if t == id)).is_some())
    }

    /// Switches to the top tab holding terminal `id` and returns where its tab is.
    pub fn show_term(&mut self, id: &str) -> Option<(PaneId, usize)> {
        let i = self.tops().iter().position(|t| t.find(|tab| matches!(tab, Tab::Term(t) if t == id)).is_some())?;
        self.switch(i);
        self.tab_of(id)
    }

    /// Applies `keep` to every top tab's tabs, then closes the top tabs left empty while another stays.
    fn retain_all(&mut self, mut keep: impl FnMut(&Tab) -> bool) {
        self.tree.retain(&mut keep);
        self.others.iter_mut().for_each(|t| t.retain(&mut keep));
        if !self.others.is_empty() {
            let before = self.tops()[..self.at].iter().filter(|t| empty(t)).count();
            let at = self.at - before;
            let mut bar = self.take_bar();
            bar.retain(|t| !empty(t));
            self.put_bar(bar, at);
        }
    }
```

4. Rewrite the whole-workspace ops to reach every tree:

```rust
    pub fn sync(&mut self, mine: &[String], theirs: &[String]) {
        self.remove(theirs);
        let first = self.tree.panes()[0].id;
        for id in mine {
            if !self.holds_term(id) {
                self.tree.push(first, Tab::Term(id.clone()));
            }
        }
    }

    /// Selects the tab showing terminal `id`, switching to its top tab, and gives it one at `place` if none does.
    pub fn open_term(&mut self, id: String, place: Place) {
        if let Some((pane, i)) = self.show_term(&id) {
            return self.tree.select(pane, i);
        }
        let tab = Tab::Term(id);
        match place {
            Place::Top => self.open_top(tab),
            Place::Split(pane, edge) if self.tree.pane(pane).is_some() => _ = self.tree.split(pane, edge, tab),
            Place::Pane(Some(pane)) if self.tree.pane(pane).is_some() => self.show(pane, tab),
            _ => self.show(self.term_pane(), tab),
        }
    }

    /// Drops the tabs of terminals `ids`, closing the panes and top tabs they leave empty.
    pub fn remove(&mut self, ids: &[String]) {
        self.retain_all(|t| !matches!(t, Tab::Term(id) if ids.contains(id)));
    }

    pub fn move_files(&mut self, from: &str, to: &str) {
        let retarget = |d: &mut Doc| {
            if let Doc::File(p) = d
                && let Some(moved) = moved(p, from, to)
            {
                *p = moved;
            }
        };
        let mut each = |t: &mut Tab| {
            if let Tab::Doc(d) = t {
                retarget(d);
            }
        };
        self.tree.each_tab_mut(&mut each);
        self.others.iter_mut().for_each(|t| t.each_tab_mut(&mut each));
        if let Some(d) = self.preview.as_mut() {
            retarget(d);
        }
    }

    pub fn close_files(&mut self, path: &str, keep: impl Fn(&str) -> bool) {
        let gone = |d: &Doc| matches!(d, Doc::File(p) if under(p, path) && !keep(p));
        self.retain_all(|t| !matches!(t, Tab::Doc(d) if gone(d)));
        if self.preview.as_ref().is_some_and(gone) {
            self.preview = None;
        }
    }

    /// Takes tab `i` out of `pane`; a closed doc stops being the preview, and a top tab left empty closes.
    pub fn close(&mut self, pane: PaneId, i: usize) -> Option<Tab> {
        let tab = self.tree.take(pane, i)?;
        if let Tab::Doc(doc) = &tab {
            self.pin(doc);
        }
        self.retain_all(|_| true);
        Some(tab)
    }

    pub fn is_empty(&self) -> bool {
        self.tops().iter().all(|t| empty(t))
    }

    /// What outlives the app: browser pages die with it and the unpinned preview is only a glance, so neither is kept.
    pub fn saved(&self) -> Self {
        let keep = |t: &Tab| match t {
            Tab::Web(_) => false,
            Tab::Doc(d) => self.preview.as_ref() != Some(d),
            Tab::Term(_) => true,
        };
        let mut w = Self { tree: self.tree.clone(), others: self.others.clone(), at: self.at, preview: None };
        w.retain_all(keep);
        w
    }

    /// A saved workspace without the terminals pocketd no longer runs, or pages a hand-edited file kept; the panels and top tabs they leave empty close.
    pub fn restored(mut self, live: &[String]) -> Self {
        self.retain_all(|t| match t {
            Tab::Term(id) => live.contains(id),
            Tab::Web(_) => false,
            Tab::Doc(_) => true,
        });
        self
    }
```

`each_tab_mut` takes `impl FnMut(&mut T)`; passing `&mut each` works because `&mut F` is `FnMut` too.

Leave the doc comment on `sync` as it is. `tab_of`, `doc_tab`, `open_doc`, `term_pane`, `doc_pane`, `shown`, `pin`, `open_web` keep reading only the shown tree.

**Step 4: Run the tests to verify they pass**

Run: `cargo test -p workspace`
Expected: PASS, every old workspace test included (`saving_keeps_terminals_pinned_docs…`, `restoring_drops…` and the rest must still pass unchanged).

### Task 1.3: Point pocket's cross-tree lookups at every top tab

**Files:**
- Modify: `packages/desktop/crates/pocket/src/desktop.rs:244-261` (`focus_agent`), `:331-333` (`pane_ids`)
- Modify: `packages/desktop/crates/pocket/src/desktop/alerts.rs:217`
- Modify: `packages/desktop/crates/pocket/src/panels.rs` (`Panels.tree` field and `sync_panels`)

**Context:** These are the only `pocket` call sites whose meaning changes. `focus_agent` must switch to the agent's top tab. The alerts check asks "is this terminal open anywhere". `pane_ids` feeds `load_active`'s `retain_panes` calls; returning every tree's panes keeps a hidden tree's loaded file/diff state for when you switch back. `Panels.tree` remembers which worktree the strips belong to so a held tab can't outlive a switch; a top-tab switch must end a hold too, since pane ids differ.

**Step 1: Make the changes**

`desktop.rs` `focus_agent`:

```rust
        let w = self.workspace(&tree);
        if let Some((pane, i)) = w.show_term(&term) {
            w.tree.select(pane, i);
        }
```

`desktop.rs` `pane_ids`:

```rust
    pub(crate) fn pane_ids(&self) -> Vec<PaneId> {
        self.place().and_then(|t| self.workspaces.get(&t)).map_or(vec![MAIN], |w| w.tops().iter().flat_map(|t| t.panes()).map(|p| p.id).collect())
    }
```

`alerts.rs:217`: replace `w.tab_of(t).is_some()` with `w.holds_term(t)`.

`panels.rs`: change the field to `tree: Option<(String, usize)>` with doc "The worktree and top tab whose panes were last synced; pane ids differ between them, so a held tab can't outlive a switch." In `sync_panels`:

```rust
        let at = self.workspace(&tree).at;
        if self.panels.tree.as_ref() != Some(&(tree.clone(), at)) {
            self.panels.drag = None;
            self.panels.tree = Some((tree.clone(), at));
        }
```

Find any other `self.panels.tree` use with `grep -rn "panels.tree" packages/desktop/crates/pocket/src` and update it to the tuple.

**Step 2: Verify**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`
Expected: builds, no new clippy warnings, all tests pass. Then launch (`cargo run --release -p pocket`) on a real `~/.pocket` and check panes and tabs restore as before (old `desktop.json`).

---

## PR 2: The top bar

**Scope:** A 40px bar over the panes, listing the worktree's top tabs with monocode's two-line copy; switch, close, reorder by drag, **+** for a new agent in a new top tab. The bar takes over moving the window, the Focus-layout sidebar toggle and the context ring from the pane bars. Pane bars keep their look otherwise (they stop being `drag_area`s).
**Depends on:** PR 1
**Done when:** the bar shows for every worktree, its tabs switch trees, ⌘-free mouse flows work, `cargo test -p pocket` passes, and before/after captures show only the new bar.

### Task 2.1: What a top tab says

**Files:**
- Create: `packages/desktop/crates/pocket/src/panels/top_tabs.rs`
- Modify: `packages/desktop/crates/pocket/src/panels.rs:1-8` (add `mod top_tabs;`)

**Context:** monocode's `TitleBar.tsx` `tabCopy` (prototype `tabCopy`): in a split, a focused file leads and the conversation follows; otherwise the conversation leads. Pocket's version is a free function over the tree, returning which tabs to name; the view turns them into text. Put the logic at the top of the new file and the view (Task 2.2) under it.

**Step 1: Write the failing test** (bottom of `top_tabs.rs`)

```rust
#[cfg(test)]
mod tests {
    use super::copy;
    use workspace::tree::{Edge, Tree};
    use workspace::{Doc, Tab};

    fn term(id: &str) -> Tab {
        Tab::Term(id.into())
    }

    fn file(p: &str) -> Tab {
        Tab::Doc(Doc::File(p.into()))
    }

    #[test]
    fn a_lone_pane_is_named_after_what_it_shows() {
        let t = Tree::lone(0, term("a"));
        assert_eq!(copy(&t), (Some(&term("a")), None));
    }

    #[test]
    fn in_a_split_a_focused_file_leads_and_the_terminal_follows() {
        let mut t = Tree::lone(0, term("a"));
        t.split(0, Edge::Right, file("/x.md"));
        assert_eq!(copy(&t), (Some(&file("/x.md")), Some(&term("a"))));
    }

    #[test]
    fn in_a_split_with_the_terminal_focused_it_leads_and_the_file_follows() {
        let mut t = Tree::lone(0, term("a"));
        t.split(0, Edge::Right, file("/x.md"));
        t.focus(0);
        assert_eq!(copy(&t), (Some(&term("a")), Some(&file("/x.md"))));
    }

    #[test]
    fn a_split_of_one_kind_leads_with_the_focus_and_counts_its_panes() {
        let mut t = Tree::lone(0, term("a"));
        t.split(0, Edge::Right, term("b"));
        assert_eq!(copy(&t), (Some(&term("b")), None));
    }
}
```

`None` as the second value means "say the kind" for a lone pane, or "N panes" in a split; the view decides which from `t.panes().len()`.

**Step 2: Run to verify it fails**

Run: `cargo test -p pocket top_tabs`
Expected: FAIL to compile, "cannot find function `copy`".

**Step 3: Write the implementation** (top of `top_tabs.rs`)

```rust
use workspace::Tab;
use workspace::tree::Tree;

/// What a top tab says: the tab it is named after, and the tab its second line names. In a split, a focused doc leads and the first terminal shown follows; otherwise the focused terminal, else the first shown, leads and the first doc shown follows.
pub(crate) fn copy(t: &Tree<Tab>) -> (Option<&Tab>, Option<&Tab>) {
    let focused = t.focused().active();
    let shown: Vec<&Tab> = t.panes().into_iter().filter_map(|p| p.active()).collect();
    if shown.len() < 2 {
        return (focused, None);
    }
    let first = |doc: bool| shown.iter().copied().find(|tab| matches!(tab, Tab::Doc(_)) == doc);
    match focused {
        Some(tab @ Tab::Doc(_)) => (Some(tab), first(false)),
        Some(tab @ Tab::Term(_)) => (Some(tab), first(true)),
        _ => (first(false).or(focused), first(true)),
    }
}
```

**Step 4: Run to verify it passes**

Run: `cargo test -p pocket top_tabs`
Expected: PASS.

### Task 2.2: Draw the bar and switch, close, open top tabs

**Files:**
- Modify: `packages/desktop/crates/pocket/src/panels/top_tabs.rs` (the view)
- Modify: `packages/desktop/crates/pocket/src/panels.rs` (`Panels` gets `top_scroll: ScrollHandle`; `panels_view` puts the bar on top; `pane_view` stops calling `drag_area`, `bar_start`, and drops the ring)
- Modify: `packages/desktop/crates/pocket/src/terminals.rs` (add `close_top`, beside `close_tab` at ~line 445)
- Modify: `packages/desktop/crates/pocket/src/panels/strip.rs` (extract `tab_text`)

**Context:**
- Look (from the prototype `.tbar` / `.st`): bar `h 40`, `px 6`, `border_b 0.5 SEPARATOR`, wrapped in `drag_area(..)` (`desktop/chrome.rs`) so it moves the window, starting with `self.bar_start(6., cx)`'s pad and toggle exactly as `pane_view` does today for the top-left pane. Then a horizontal scroll strip (`overflow_x_scroll`, `track_scroll(&self.panels.top_scroll)`, `gap 2`, `flex_1`, `min_w_0`, so the empty space right of the tabs is part of it: Task 4.3 drops on it). Then the **+** (28px, `rounded 7`, `icon("plus", 15., TEXT_3)`, hover `FILL_3`). Then, `ml_auto`, the ring of the focused pane's terminal (move the `ring` code from `pane_view` here, reading `w.tree.focused().active()`).
- Each tab: `w 224`, `min_w 112`, `flex_shrink`, `h 30`, `rounded 6`, `pl 8`, `pr 28`, `gap 7`, `relative`. Selected: `bg FILL_3`, text `TEXT`. Else: text `TEXT_3`, icon at `opacity 0.55`, hover `bg FILL_1` and text `TEXT`. Two lines, `line_height 1.2`: head `12.5px` `MEDIUM` `truncate` (italic when it is the preview doc), meta `11px` `TEXT_3` `truncate`. Icon: the provider icon (`provider_icon(&a.provider, 14., ink)`) when the head or meta is an agent's terminal, else `file_icon(path, false, false, 14.)` for a doc head, else `icon("prompt", 14., TEXT_3)`. × (only when there are 2+ top tabs): `absolute`, `right 4`, centred, 20px, `rounded 4`, `icon("x", 12., TEXT_3)`, hidden until the tab is hovered (`group` / `group_hover`, as `strip_tabs` does with `"term-tab"`; use group `"top-tab"`). Middle-click closes. Each tab's `on_mouse_down(Left)` stops propagation, as the strip's tabs do, else `drag_area` takes the press.
- Text: head = `self.tab_text(head)`. Meta = `self.tab_text(meta)` when `copy` gave one; else in a split `format!("{n} panes")`; else the kind: an agent's provider name (`self.agent_name(&a.provider)`) for an agent terminal, `"Terminal"` for a plain one, the file's folder relative to `self.explore_root()` (or the repo name at the root) for `Doc::File`, `"Changes"` for diff docs, `"Browser"` for a page.
- `tab_text(&Tab) -> String`, in `strip.rs` beside `tab_lead`, holds the label logic `tab_lead` already has (basename for docs, `Browser::label` for pages, `pane_label`/busy title for terminals), except that an agent's terminal reads as its summary's `title` when that is not empty, else `agent_name(provider)`. Have `tab_lead` keep its own agent label (`agent_name`) so the strip looks unchanged.
- Clicking a tab calls `self.switch_top(i, window, cx)`: `self.workspace(&tree).switch(i)`, then `let (p, a) = { let t = &self.workspace(&tree).tree; (t.focused, t.focused().active) }; self.select_tab(p, a, window, cx); self.load_active(cx);` (the same tail `move_tab_to` uses). Scroll the shown tab into view once per switch, as `Strip.revealed` does (keep a `top_revealed: Option<(String, usize)>` in `Panels`).
- **+** calls `self.new_agent_tab(self.store.agents.default_agent(), Place::Top, cx)`. `new_agent_tab` already passes its `Place` on to `open_term`; check `grep -n "fn new_agent_tab" -A30 packages/desktop/crates/pocket/src` that nothing in between matches on `Place` exhaustively, and add a `Place::Top` arm where the compiler asks.
- `close_top(i, cx)` in `terminals.rs`, modelled on `close_tab`: collect the tree's tabs from `w.tops()[i]`; if any is a `Doc::File` with `self.preview.dirty(path)`, switch to that top tab, select the file's tab, and open its `Confirm::CloseFile` as `close_tab` does, then return. Collect the terminal ids; return unless every one `may_close` and `!self.ask_close(ids.clone(), cx)`. Then `self.workspace(&tree).close_top(i)`, `close_pane` each id, `load_active`, `save_soon`, `cx.notify()`.
- `pane_view`: delete the `edges`, `top`, `left`, `pad`, `toggle` lines; use `.pl(px(10.))`; replace `.child(if top { drag_area(bar) } else { bar })` with `.child(bar)`; delete `ring` and `status`. Remove `edges` and its test `only_panes_on_the_top_edge_take_the_window_bar_and_only_the_top_left_one_its_controls` (nothing else calls it: `grep -rn "edges(" packages/desktop/crates/pocket/src`).
- `panels_view`: return `div().flex_1().min_h_0().flex().flex_col().bg(SURFACE_SUNKEN).child(self.top_bar(tree, window, cx)).child(<today's panes div>)…`.

**Step 1: Implement** the view pieces above. Write `top_bar` as `pub(super) fn top_bar(&mut self, tree: &str, window: &mut Window, cx: &mut Context<Self>) -> Div` in `impl Desktop` in `top_tabs.rs`, with a private `fn top_tab(&self, tree: &str, i: usize, t: &Tree<Tab>, cx) -> Stateful<Div>`. Use element ids `("top-tab", i)`, `("close-top", i)`, `"new-top"`, `"top-strip"`.

**Step 2: Verify**

Run: `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test -p pocket`
Expected: builds, no new warnings, tests pass.

Then `.ui-review/fixture/capture.sh /tmp/chrome-after main=` and compare with `/tmp/chrome-before/main.png`: one new 40px bar with one tab; pane bars without the toggle; traffic lights clear of the toggle in Focus layout. In the running app: + opens a second top tab with a new agent; clicking switches; × asks before killing a running agent; the bar's empty space drags the window, a double-click on it zooms the window.

### Task 2.3: Reorder top tabs by dragging

**Files:**
- Modify: `packages/desktop/crates/pocket/src/panels/strip.rs` (make `TabDrag::new`, `lay`, `ease`, `release`, `held`, `offsets` `pub(super)`)
- Modify: `packages/desktop/crates/pocket/src/panels/top_tabs.rs`
- Modify: `packages/desktop/crates/pocket/src/panels.rs` (`Panels.top_drag: Option<strip::TabDrag>`)

**Context:** `TabDrag` already does monocode's `useAnimatedReorder`: measure slots once, clamp to the strip, land past each crossed middle, slide the passed tabs, then settle. Reuse it as is for the bar. The strip measures its tabs with `strip::slots(&scroll, n)`; do the same with `top_scroll`. Copy the wiring from `strip_tabs`: `on_drag(DragTop, …)` on each top tab sets `top_drag = Some(TabDrag::new(i, grab.x, pointer))` and returns `cx.new(|_| EmptyView)`; the strip's `on_drag_move::<DragTop>` updates `pointer`; `capture_any_mouse_up` / `on_mouse_up_out` call `release_top`, which on `Some((from, to))` calls `self.workspace(&tree).move_top(from, to)`, `save_soon`, `notify`. Each frame: `lay`, then `ease(Instant::now(), cx.reduce_motion())`, request an animation frame while it moves, clear it once released and settled (same block as in `strip_tabs`). Draw each tab with `.relative().left(px(offsets[i]))`, the held one `deferred`. Add `pub(super) struct DragTop;` beside `DragTab`. The × hides while a drag is on (`[data-reordering] .x1{visibility:hidden}` in the prototype).

**Step 1: Implement.** **Step 2: Verify** with `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test -p pocket`, then by hand: three top tabs, drag the first past the third; tabs slide aside; it lands third; the shown one stays shown; with Reduce Motion on, tabs jump.

---

## PR 3: Slim pane heads, no file header

**Scope:** Pane heads become monocode's: none / title / tab strip, 36px, no chip, split or More buttons. The file header goes; Preview/Source floats; file actions move into the tab menu. The worktree's +/− moves to its sidebar row.
**Depends on:** PR 2 (the bar now carries the toggle and window drag a lone, headless pane would otherwise lose)
**Done when:** `cargo test -p pocket` passes with the new `head` tests, the captures match the prototype's panes, and every removed action is reachable from the tab menu or a shortcut.

### Task 3.1: What sits atop a pane

**Files:**
- Modify: `packages/desktop/crates/pocket/src/panels.rs` (add `Head` and `head` above `impl Desktop`, tests at the bottom)

**Step 1: Write the failing test** (in `panels.rs` `mod tests`; replace the `top_right` test, whose function Task 3.2 removes)

```rust
    use super::{Head, head};
    use workspace::{Doc, Tab};

    #[test]
    fn a_lone_terminal_has_no_head_and_a_title_in_a_split() {
        let mut t = Tree::lone(0, Tab::Term("a".into()));
        assert_eq!(head(&t, 0), Head::Bare);
        let b = t.split(0, Edge::Right, Tab::Term("b".into())).unwrap();
        assert_eq!((head(&t, 0), head(&t, b)), (Head::Title, Head::Title));
    }

    #[test]
    fn a_pane_with_docs_or_several_tabs_shows_its_tabs() {
        let mut t = Tree::lone(0, Tab::Term("a".into()));
        t.push(0, Tab::Term("b".into()));
        assert_eq!(head(&t, 0), Head::Tabs);
        let d = t.split(0, Edge::Right, Tab::Doc(Doc::File("/x".into()))).unwrap();
        assert_eq!(head(&t, d), Head::Tabs);
        assert_eq!(head(&Tree::default(), 0), Head::Tabs);
    }
```

**Step 2: Run to verify it fails** — `cargo test -p pocket panels::tests` → FAIL, "cannot find `Head`".

**Step 3: Implement**

```rust
/// What sits atop a pane: nothing over a lone pane whose one tab is a terminal, since its top tab names it; a title over such a pane in a split, to drag it by; else its tabs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Head {
    Bare,
    Title,
    Tabs,
}

pub(crate) fn head(t: &Tree<Tab>, pane: PaneId) -> Head {
    match (t.pane(pane).map(|p| p.tabs.as_slice()), t.panes().len()) {
        (Some([Tab::Term(_)]), 1) => Head::Bare,
        (Some([Tab::Term(_)]), _) => Head::Title,
        _ => Head::Tabs,
    }
}
```

**Step 4:** `cargo test -p pocket panels::tests` → PASS.

### Task 3.2: Draw the heads

**Files:**
- Modify: `packages/desktop/crates/pocket/src/panels.rs` (`pane_view`; delete `changes_chip`, `top_right`)
- Modify: `packages/desktop/crates/pocket/src/panels/strip.rs` (`strip_tabs`, `new_tab_controls`)
- Modify: `packages/desktop/crates/pocket/src/panels/drop.rs:5-6` (`STRIP_H` 42 → 36) and the tests whose numbers it changes
- Modify: `packages/desktop/crates/pocket/src/panels/drop_marks.rs` (caret top/height for a 36px strip: `top r.y + 8`, `h 20`)

**Context:** sizes from the prototype `.bm`, `.tm`, `.sh`, `.grip`, `.x1`:
- **Tabs head**: `h 36`, `border_b 0.5 SEPARATOR`, `pl 6`, `pr 10`. Children: the grip (only when `t.panes().len() > 1`): 20x30, `rounded 6`, `icon("grip", 14., TEXT_4)`, hover `bg FILL_1`, `cursor_grab`; then the strip; then a filler `flex_1 min_w 16 self_stretch` (the pane's drag handle in a split, Task 4.1); then the **+** menu (`new_tab_controls`, unchanged).
- **Tab** (rework the `visual`/`tab` pair in `strip_tabs`): outer `w 224 min_w 112 flex_shrink h_full relative group("term-tab")`; inner `h 30 flex_1 min_w_0 rounded 6 pl 8 pr 28 gap 6 text 13px`; selected `bg FILL_3` + `TEXT`; not selected `TEXT_3`, lead icon `opacity 0.55`, hover `bg FILL_1` + `TEXT`. Label `flex_1 truncate` (replace `max_w(150.)` in `tab_lead`'s `label`). Drop the `SURFACE` + `row_shadow` focused style and the `p(3) m(-3)` shadow padding. ×: `absolute right 4`, centred, 20px, `rounded 4`, `icon("x", 12., TEXT_3)`, hover `bg FILL_3`; `opacity 1` when selected, else only on group hover. Keep the existing drag, slide, scroll reveal and edge fades. Middle-click on a tab closes it (`on_mouse_down(MouseButton::Middle, … this.close_tab(p, i, cx))`). The right-click menu opens for any tab count (drop the `when(tabs.len() > 1, …)`).
- **Title head**: `h 36 px 8 gap 6 border_b 0.5 SEPARATOR flex items_center`: `icon("grip", 14., TEXT_4)`; an 8px dot, `ACCENT` when the pane is focused, else nothing (keep the 8px space); title `flex_1 truncate 12px` = `self.tab_text(tab)`; × 20px, `tooltip "Close Pane (⌘W)"` → `close_tab(p, 0, cx)`. Wrap it in `self.arriving(&agent, crate::creating::BAR, …)` as `pane_view` does for the bar today.
- **Bare**: no head. Apply `arriving` to nothing; the top tab is the head.
- `pane_view` picks with `head(t, p)`. Delete the `split` closure, `right`, `can`, the More icon group, `changes_chip` and `top_right` (and its test). Remove now-unused imports (`Layout`, `Side`, `Edge` if unused, `ui::icon_group` users).

**Step 1: Implement**, updating the `highlight` test in `drop.rs` for `STRIP_H` 36 (`r = Rect { x: 600., y: 0., w: 600., h: 442. }`): `Right` → `Rect { x: 904., y: 40., w: 292., h: 398. }`, `Bottom` → `Rect { x: 604., y: 243., w: 592., h: 195. }`, `None` → `Rect { x: 604., y: 40., w: 592., h: 398. }`. The `aim` tests still hold: their strip points sit at `y = 20` and their body points well below 36.

**Step 2: Verify** — `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test -p pocket`; capture `main=` and a split (`split=` with the step that splits; see `capture.rs`) and compare against the prototype at the same width: 36px heads, no chip/split/More, × always on the selected tab, a narrow pane still shows the selected tab's × (the bug the user hit).

### Task 3.3: Drop the file header; float Preview/Source; file actions in the tab menu

**Files:**
- Delete: `packages/desktop/crates/pocket/src/explorer/preview/header.rs`
- Modify: `packages/desktop/crates/pocket/src/explorer/preview.rs:5` and `:456-480` (`file_view`)
- Modify: `packages/desktop/crates/pocket/src/panels/tab_actions.rs`
- Modify: `packages/desktop/crates/pocket/src/modals/more.rs`

**Context:**
- `header.rs` holds `relative` (re-exported by `preview.rs:5` and used elsewhere: `grep -rn "relative(" packages/desktop/crates/pocket/src`), `copy_path`, and `file_header`. Move `relative` and its test into `preview.rs`, keep `copy_path` (move it to `tab_actions.rs`), delete `file_header` and the file. `preview.path_copied` stays (it drives the "copied" check in the menu row for 1.5s; if nothing reads it after the move, delete the field and the timer).
- `file_view`: drop `header`. When `f.markdown()`, put the Preview/Source switch over the body: `div().absolute().top(px(8.)).right(px(16.))` holding the existing `ui::segmented(…, small: true, …)` call moved from `file_header` unchanged (same `md-mode` id and the same `move |this, v, cx|` body). The body container must be `relative` for it to anchor.
- `tab_actions`: for a `Tab::Doc(Doc::File(path))` add, after the move rows, a separator (`ui::menu_sep()` if it exists, else `div().h(px(0.5)).my(px(4.)).bg(SEPARATOR)`) and: "Ask about this file" (`sparkle`; `this.new_session(Some(format!("About {rel}: ")), false, window, cx)`), "Open in Default App" (`external`; `cx.open_with_system(Path::new(&path))`), "Reveal in Finder" (`folder`; `cx.reveal_path`), "Copy Path" (`copy`; absolute), "Copy Relative Path" (`copy`; `relative(&path, &root)` with `root = self.explore_root().unwrap_or_default()`), "Copy File Name" (`copy`; `basename`). For every tab: "Close" and, when the pane has 2+ tabs, "Close Others" (close every other tab of the pane through `close_tab`, highest index first so indices stay valid; stop at the first that asks for confirmation). Each row clears `self.panels.actions` first, as the move rows do. Diff/commit docs get Open/Reveal/Copy for `cwd/path`, the paths `more.rs` already builds.
- `more.rs`: delete the `let Some(path) = … else { return menu … }` early branch (New terminal tab, Close session) and return an empty `menu` when there is no path. Remove imports this leaves unused.

**Step 1: Implement.** **Step 2: Verify** — `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test -p pocket`; capture a markdown file (`file=` steps that open `README.md`): no header row, the switch floats top-right, flipping it still swaps views; right-click a file tab shows the new rows and each works.

### Task 3.4: The worktree's +/− in its sidebar row

**Files:**
- Modify: `packages/desktop/crates/pocket/src/sidebar.rs:452` (`worktree_row`)

**Context:** `ui::row_trail(mark, buttons, open)` shows `mark` until the row is hovered, then the buttons. Pass the worktree's totals as the mark when it has changes; a click on the row's diff opens Changes as the chip did.

**Step 1: Implement**

```rust
        let diff = self.repos.get(tree).filter(|r| !r.files.is_empty()).map(|r| {
            let (added, removed) = r.totals();
            ui::meta_diff(added, removed, 11.).into_any_element()
        });
        let trail = ui::row_trail(diff, vec![self.row_menu_button(tree, menu.clone(), cx)], self.row_menu.as_ref() == Some(&menu) || self.sidebar.hover.on(tree));
```

Check the `repos` key for a worktree row is `tree` (the worktree path): `grep -n "repos.get(" packages/desktop/crates/pocket/src/sidebar*.rs`. If the row's key is not the cwd, use the same lookup `card` uses.

**Step 2: Verify** — build, clippy, `cargo test -p pocket`; capture `repositories-worktrees=`: rows with changes show `+N −M`, which turns into ⋯ on hover.

---

## PR 4: Drag and drop between panes and the top bar

**Scope:** Move a pane by its grip, its strip's empty space or its title head; drop it on another pane's edge, or on the top bar to make it a top tab. Drop a file tab on the top bar to make it a top tab. Drag a top tab down onto a pane to join the shown tree. Esc cancels. The existing tab drag stays.
**Depends on:** PR 3 (grip, filler and title head exist)
**Done when:** the aim tests pass and each flow in the prototype works the same in the app.

### Task 4.1: Where a pane, or a top tab, drops

**Files:**
- Modify: `packages/desktop/crates/pocket/src/panels/drop.rs`

**Context:** monocode `paneDrop.ts` / prototype `edgeFromPoint`: the edge is the dominant axis of the pointer's offset from the pane's centre, normalised by its size; no middle zone. A pane can't drop on itself. Over the top bar, the drop index is past the top tabs whose middle the pointer passed (prototype `titleDropFromPoint`). Both are free functions over rects.

**Step 1: Write the failing tests** (in `drop.rs` `mod tests`)

```rust
    #[test]
    fn a_pane_drops_at_the_edge_its_pointer_leans_to_most() {
        let (layout, _) = two();
        assert_eq!(pane_aim(&layout, (650., 200.), Some(0)), Some((1, Edge::Left)));
        assert_eq!(pane_aim(&layout, (1150., 200.), Some(0)), Some((1, Edge::Right)));
        assert_eq!(pane_aim(&layout, (900., 20.), Some(0)), Some((1, Edge::Top)));
        assert_eq!(pane_aim(&layout, (900., 390.), Some(0)), Some((1, Edge::Bottom)));
    }

    #[test]
    fn a_pane_never_drops_on_itself_or_outside_every_pane() {
        let (layout, _) = two();
        assert_eq!(pane_aim(&layout, (300., 200.), Some(0)), None);
        assert_eq!(pane_aim(&layout, (1300., 200.), Some(0)), None);
        assert_eq!(pane_aim(&layout, (100., 200.), None), Some((0, Edge::Left)));
    }

    #[test]
    fn on_the_bar_a_drop_lands_past_the_top_tabs_whose_middle_it_passed() {
        let slots = [(10., 200.), (212., 200.)];
        assert_eq!([0., 120., 330., 900.].map(|x| top_index(&slots, x)), [0, 1, 2, 2]);
    }

    #[test]
    fn the_pane_highlight_takes_the_half_at_its_edge() {
        let r = Rect { x: 0., y: 0., w: 600., h: 400. };
        assert_eq!(half(r, Edge::Right), Rect { x: 300., y: 0., w: 300., h: 400. });
        assert_eq!(half(r, Edge::Top), Rect { x: 0., y: 0., w: 600., h: 200. });
    }
```

**Step 2:** `cargo test -p pocket drop::tests` → FAIL, "cannot find function `pane_aim`".

**Step 3: Implement**

```rust
/// Where a pane, held by `from` or none for a top tab, drops at `(x, y)`: the pane under it, at the edge the pointer leans to most from its centre.
pub(crate) fn pane_aim(layout: &[(PaneId, Rect)], (x, y): (f32, f32), from: Option<PaneId>) -> Option<(PaneId, Edge)> {
    let &(pane, r) = layout.iter().find(|(p, r)| Some(*p) != from && r.x <= x && x < r.x + r.w && r.y <= y && y < r.y + r.h)?;
    let (nx, ny) = ((x - r.x) / r.w - 0.5, (y - r.y) / r.h - 0.5);
    let edge = match nx.abs() > ny.abs() {
        true if nx < 0. => Edge::Left,
        true => Edge::Right,
        false if ny < 0. => Edge::Top,
        false => Edge::Bottom,
    };
    Some((pane, edge))
}

/// Where a drop at `x` lands among the top tabs at `slots`: past each whose middle it passed.
pub(crate) fn top_index(slots: &[(f32, f32)], x: f32) -> usize {
    slots.iter().filter(|(l, w)| l + w / 2. < x).count()
}

/// The half of `r` a pane dropped at `edge` takes.
pub(crate) fn half(r: Rect, edge: Edge) -> Rect {
    match edge {
        Edge::Left => Rect { w: r.w / 2., ..r },
        Edge::Right => Rect { x: r.x + r.w / 2., w: r.w / 2., ..r },
        Edge::Top => Rect { h: r.h / 2., ..r },
        Edge::Bottom => Rect { y: r.y + r.h / 2., h: r.h / 2., ..r },
    }
}
```

With `from: Some(0)`, pane 0 is skipped, so `(300., 200.)` finds no pane → `None`; with `None` (a top tab is held) pane 0 counts, and `(100., 200.)` leans left.

**Step 4:** `cargo test -p pocket drop::tests` → PASS.

### Task 4.2: Drag a pane

**Files:**
- Create: `packages/desktop/crates/pocket/src/panels/pane_drag.rs` (state + handlers + marks; add `mod pane_drag;` in `panels.rs`)
- Modify: `packages/desktop/crates/pocket/src/panels.rs` (`Panels.pane_drag: Option<pane_drag::Held>`; `panels_view` wires moves and drops over bar + panes)
- Modify: `packages/desktop/crates/pocket/src/panels/strip.rs` (grip and filler start the drag), the title head in `panels.rs`
- Modify: the Esc handler (find it: `grep -rn "\"escape\"" packages/desktop/crates/pocket/src/actions.rs packages/desktop/crates/pocket/src/desktop*.rs`)

**Context:**
- `pub(crate) struct DragPane(pub PaneId);` is the GPUI drag payload; start with `.on_drag(DragPane(p), |_, _, _, cx| cx.new(|_| EmptyView))` on the grip, the filler and the title head (only in a split). Each of them stops left-press propagation like the tabs do. GPUI starts a drag only after the pointer moves a few pixels, which matches monocode's 5px threshold.
- `Held { from: Source, aim: Option<Drop>, at: (f32, f32) }` with `enum Source { Pane(PaneId), Top(usize) }` and `enum Drop { Pane(PaneId, Edge), Top(usize) }`.
- The container holding the top bar and the panes gets `.on_drag_move::<DragPane>(…)`: if the pointer is inside the top bar's bounds (measure them with a `canvas` like `panels.bounds`; store `Panels.bar: Rect`), aim `Drop::Top(top_index(&top_slots, x))` (top slots from `strip::slots(&self.panels.top_scroll, n)`), else `pane_aim(&t.drawn_layout(self.panels.bounds), at, Some(from)).map(|(p, e)| Drop::Pane(p, e))`. Notify only when the aim changes.
- `.on_drop(cx.listener(|this, d: &DragPane, window, cx| this.drop_pane(d.0, window, cx)))` on the same container: `Drop::Pane(to, edge)` → `self.workspace(&tree).tree.move_pane(from, to, edge)`; `Drop::Top(i)` → `self.workspace(&tree).detach_pane(from, i)`; then the `move_tab_to` tail (`select_tab` the focused pane's active tab, `load_active`), `save_soon`, clear `pane_drag`, notify. A mouse up outside the container gets no `on_drop`, so `panels_view` also clears `pane_drag` on any render where `!cx.has_active_drag()`, as `strip_tabs` does for a held tab.
- Marks (deferred, like `drop_marks`): while aimed at a pane, a `bg ACCENT opacity 0.15` rect over `half(rect, edge)` plus a 2px `ACCENT` line on that edge; while aimed at the bar, a 2px x 26px `ACCENT` caret at `drop::caret(&top_slots, i)` (prototype `.tmark`). The source pane draws at `opacity 0.4` (`pane_view` reads `self.panels.pane_drag`).
- Esc while `pane_drag` (or `top_drag` from Task 4.3) is set clears it and notifies; the drop handler then finds no `Held` and does nothing. Respect `cx.reduce_motion()` for nothing here: there is no motion beyond the marks.
- A new pane slides in from its edge in monocode (`pane-enter` 260ms). Skip it: GPUI has no cheap transform animation on a subtree here, and the user didn't ask for it.

**Step 1: Implement.** **Step 2: Verify** — build, clippy, `cargo test -p pocket`; in the app with three panes: drag a pane's grip onto each edge of another (half lights up, it lands there, the split halves when it is the same axis); drag it onto the top bar between two tabs (caret, a new top tab appears there, the split closes up); Esc mid-drag leaves everything in place; the last pane has no grip, so it can't be dragged.

### Task 4.3: Tabs to the bar, top tabs to panes

**Files:**
- Modify: `packages/desktop/crates/pocket/src/panels/drop.rs` (`Aim::Top(usize)`)
- Modify: `packages/desktop/crates/pocket/src/panels.rs` (`aim_tab`)
- Modify: `packages/desktop/crates/pocket/src/panels/strip.rs` (`release_tab`)
- Modify: `packages/desktop/crates/pocket/src/panels/top_tabs.rs` (a top tab held below the bar)
- Modify: `packages/desktop/crates/pocket/src/panels/pane_drag.rs` (`Source::Top` drop)

**Context:**
- **A tab onto the bar** (prototype `detachFile`): `aim_tab` today returns `None` above the panes. Add `Aim::Top(usize)`; before calling `drop::aim`, if the pointer is in `self.panels.bar`, aim `Aim::Top(top_index(&top_slots, x))`. `d.away` becomes true as for any aim off the strip. `release_tab` gets the arm `(None, Some(Aim::Top(i))) => { self.workspace(&tree).detach_tab(pane, from, i); …the move_tab_to tail… }`. `drop_marks` draws the bar caret for `Aim::Top` (share the caret code from Task 4.2). The strip's `on_drag_move::<DragTab>` already reaches `aim_tab` through the container; make sure the container now spans the bar too (move the `on_drag_move::<DragTab>` from the panes div to the container that holds the bar and the panes).
- **A top tab onto a pane** (prototype `placeSession`): in `top_tabs.rs`, while `top_drag` is held and the pointer is below the bar's bottom edge, set `TabDrag.away = true` (the other top tabs slide home, as a tab leaving its strip does) and aim with `pane_aim(&layout, at, None)` into `Panels.pane_drag = Some(Held { from: Source::Top(i), … })`, sharing the marks from Task 4.2. A release then calls `self.workspace(&tree).place_top(i, pane, edge)` instead of `move_top`. The shown top tab can't be placed on its own panes: skip the aim when `i == w.at`.
- Both reuse Esc from Task 4.2.

**Step 1: Implement.** **Step 2: Verify** — `cargo build --workspace && cargo clippy --workspace --all-targets && cargo test --workspace`; then by hand, the prototype's flows: drag `README.md`'s tab up into the bar → it becomes its own top tab at the caret, its pane keeps the other tabs; drag a pane's only tab up → the whole pane moves; drag a hidden top tab down onto the shown pane's right half → it joins on the right and its top tab disappears; drag the shown top tab down → nothing lights.

---

## Final check (after PR 4)

From `packages/desktop`: `cargo build --workspace`, `cargo clippy --workspace --all-targets` (no new warnings), `cargo test --workspace`. Capture `main=`, a split, a markdown file and `repositories-worktrees=` into `/tmp/chrome-after` and compare with `/tmp/chrome-before` and the prototype. Time a top-tab switch with 1000+ changed files in release (`[DEBUG-…]` timing around `switch_top`, removed afterwards): it must not call `refresh_git`, and the frame after must stay within the Changes-list baseline in `CLAUDE.md`.
