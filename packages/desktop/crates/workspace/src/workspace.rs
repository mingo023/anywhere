pub mod tree;

use serde::{Deserialize, Serialize};
use tree::{Edge, Node, Pane, PaneId, Rect, Tree};

/// What a doc tab shows: a file by its absolute path, a file's changes by its path in the worktree, a file's changes in commit `sha`, every change of a commit, or a file's changes since the branch left `base`, as its PR shows them.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Doc {
    File(String),
    Diff(String),
    CommitFile { sha: String, path: String },
    Commit(String),
    PrFile { base: String, path: String },
}

/// A tab of one worktree: a pocketd terminal by its id, a doc, or a browser page by the app's id for it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Tab {
    Term(String),
    Doc(Doc),
    Web(u64),
}

/// Where a new terminal opens: in a pane, the terminal pane nearest the focus when none is given or it is gone, in a new pane at an edge of one, or in a new top tab.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    Pane(Option<PaneId>),
    Split(PaneId, Edge),
    Top,
}

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

impl Workspace {
    /// Gives each of `mine` not yet shown a tab in the top-left pane, leaving the focus alone, and drops terminals of `theirs`; unknown terminals stay, since a just-spawned terminal is not listed yet.
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

    fn show(&mut self, pane: PaneId, tab: Tab) {
        if let Some(i) = self.tree.push(pane, tab) {
            self.tree.select(pane, i);
        }
    }

    /// The pane nearest the focus that holds a terminal, else the focused pane.
    pub fn term_pane(&self) -> PaneId {
        self.tree.nearest(self.tree.focused, |p| p.tabs.iter().any(|t| matches!(t, Tab::Term(_)))).unwrap_or(self.tree.focused)
    }

    /// The pane a doc opens in, the tree drawn in `bounds`: the focused pane when empty, else the doc pane nearest the focus, else the focused pane when it has no room to split right; `None` opens a new pane right of it.
    pub fn doc_pane(&self, bounds: Rect) -> Option<PaneId> {
        let focused = self.tree.focused;
        if self.tree.focused().tabs.is_empty() {
            return Some(focused);
        }
        self.tree.nearest(focused, |p| p.tabs.iter().any(|t| matches!(t, Tab::Doc(_)))).or_else(|| (!self.tree.can_split(focused, Edge::Right, bounds)).then_some(focused))
    }

    /// Shows `doc` and returns its tab. A doc shown in any pane is selected there. Otherwise it takes over the preview tab if that is in `pane` and `pin` is off, else gets a tab in `pane`, else, with no `pane`, a new pane right of the focused one.
    pub fn open_doc(&mut self, doc: Doc, pin: bool, pane: Option<PaneId>) -> (PaneId, usize) {
        self.place_doc(doc, pin, pane, false)
    }

    /// As `open_doc`, with a new tab going right after the shown one when `next`, else at the end.
    pub fn place_doc(&mut self, doc: Doc, pin: bool, pane: Option<PaneId>, next: bool) -> (PaneId, usize) {
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
            (None, None, Some(p)) if next => (p, self.tree.insert_next(p, tab).expect("checked above")),
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

    /// Drops the tabs of terminals `ids`, closing the panes and top tabs they leave empty.
    pub fn remove(&mut self, ids: &[String]) {
        self.retain_all(|t| !matches!(t, Tab::Term(id) if ids.contains(id)));
    }

    /// Points the tabs of files at or under `from` to where a rename moved them.
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

    /// Drops the tabs of files at or under `path` but those `keep` holds on to, closing the panes they leave empty.
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

    /// The tab each drawn pane shows: every pane's, or only the zoomed pane's.
    pub fn shown(&self) -> Vec<(PaneId, &Tab)> {
        self.tree.drawn().into_iter().filter_map(|p| Some((p.id, p.active()?))).collect()
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
        self.reserve();
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
        if let Some(d) = &self.preview {
            let tab = Tab::Doc(d.clone());
            if tabs.contains(&tab) && self.tops().iter().all(|t| t.find(|x| *x == tab).is_none()) {
                self.preview = None;
            }
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
}

/// `path` with its `from` prefix swapped for `to`, when it is `from` or lies under it.
pub fn moved(path: &str, from: &str, to: &str) -> Option<String> {
    under(path, from).then(|| format!("{to}{}", &path[from.len()..]))
}

fn under(path: &str, dir: &str) -> bool {
    path.strip_prefix(dir).is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
}

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

    const WINDOW: Rect = Rect { x: 0., y: 0., w: 1200., h: 800. };

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
        assert_eq!(w.doc_pane(WINDOW), Some(0));
        w.open_term("a".into(), Place::Pane(None));
        assert_eq!(w.doc_pane(WINDOW), None);
        let (d, _) = w.open_doc(diff("x"), true, None);
        w.tree.focus(0);
        assert_eq!(w.doc_pane(WINDOW), Some(d));
    }

    #[test]
    fn a_doc_opens_in_the_focused_pane_when_there_is_no_room_to_split_it() {
        let w = with(&["a"]);
        assert_eq!(w.doc_pane(Rect { w: 400., ..WINDOW }), Some(0));
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
    fn a_new_tab_can_open_right_after_the_shown_one() {
        let mut w = with(&["a"]);
        let (d, _) = w.open_doc(file("x"), true, None);
        w.open_doc(file("y"), true, Some(d));
        w.tree.select(d, 0);
        assert_eq!(w.place_doc(file("z"), true, Some(d), true), (d, 1));
        assert_eq!(tabs(&w)[1], vec![Tab::Doc(file("x")), Tab::Doc(file("z")), Tab::Doc(file("y"))]);
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

    #[test]
    fn saving_keeps_terminals_pinned_docs_and_the_layout_but_no_pages_or_preview() {
        let mut w = Workspace { preview: Some(file("/b")), ..Default::default() };
        w.tree.push(0, term("t1"));
        w.tree.push(0, Tab::Doc(file("/a")));
        let right = w.tree.split(0, Edge::Right, Tab::Doc(file("/b"))).unwrap();
        w.tree.push(right, Tab::Web(1));
        w.tree.split(0, Edge::Bottom, Tab::Web(2));
        let saved = w.saved();
        assert_eq!(tabs(&saved), vec![vec![term("t1"), Tab::Doc(file("/a"))]]);
        assert_eq!(saved.preview, None);
    }

    #[test]
    fn saving_leaves_the_open_workspace_alone() {
        let mut w = Workspace::default();
        w.tree.push(0, Tab::Web(1));
        let _ = w.saved();
        assert_eq!(tabs(&w), vec![vec![Tab::Web(1)]]);
    }

    #[test]
    fn restoring_drops_terminals_pocketd_no_longer_runs_and_closes_their_panels() {
        let mut w = Workspace::default();
        w.tree.push(0, term("gone"));
        w.tree.push(0, Tab::Doc(file("/a")));
        let right = w.tree.split(0, Edge::Right, term("dead")).unwrap();
        w.tree.split(right, Edge::Bottom, term("alive"));
        let back = w.restored(&["alive".into()]);
        assert_eq!(tabs(&back), vec![vec![Tab::Doc(file("/a"))], vec![term("alive")]]);
    }

    #[test]
    fn a_workspace_whose_terminals_all_ended_restores_empty() {
        let mut w = Workspace::default();
        w.tree.push(0, term("gone"));
        assert!(w.restored(&[]).is_empty());
    }

    #[test]
    fn restoring_drops_pages_a_saved_file_should_not_hold() {
        let mut w = Workspace::default();
        w.tree.push(0, Tab::Web(1));
        w.tree.push(0, term("alive"));
        assert_eq!(tabs(&w.restored(&["alive".into()])), vec![vec![term("alive")]]);
    }

    #[test]
    fn a_rename_moves_the_tabs_of_the_file_and_of_what_the_folder_held() {
        let mut w = Workspace::default();
        w.open_doc(file("/r/src/a.rs"), true, Some(0));
        w.open_doc(file("/r/srcs/b.rs"), true, Some(0));
        w.open_doc(file("/r/src"), false, Some(0));
        w.move_files("/r/src", "/r/lib");
        assert_eq!(tabs(&w), vec![vec![Tab::Doc(file("/r/lib/a.rs")), Tab::Doc(file("/r/srcs/b.rs")), Tab::Doc(file("/r/lib"))]]);
        assert_eq!(w.preview, Some(file("/r/lib")));
    }

    #[test]
    fn a_trash_closes_the_tabs_under_it_but_those_kept_and_empties_their_pane() {
        let mut w = with(&["t"]);
        let docs = w.open_doc(file("/r/src/a.rs"), true, None).0;
        w.open_doc(file("/r/src/b.rs"), false, Some(docs));
        w.open_doc(file("/r/src/c.rs"), true, Some(docs));
        w.close_files("/r/src", |p| p == "/r/src/c.rs");
        assert_eq!(tabs(&w), vec![vec![term("t")], vec![Tab::Doc(file("/r/src/c.rs"))]]);
        assert_eq!(w.preview, None);
        w.close_files("/r/src", |_| false);
        assert_eq!(tabs(&w), vec![vec![term("t")]]);
    }

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
    fn a_pane_split_after_one_left_for_the_bar_gets_an_id_of_its_own() {
        let mut w = with(&["a"]);
        let b = w.tree.split(w.tree.focused, Edge::Right, term("b")).unwrap();
        w.detach_pane(b, 1);
        w.open_term("c".into(), Place::Split(w.tree.focused, Edge::Right));
        let (d, _) = w.open_doc(file("/x"), true, None);
        w.detach_tab(d, 0, 2);
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
    fn closing_a_top_tab_keeps_the_preview_another_top_tab_still_shows() {
        let mut w = three();
        w.open_doc(file("/x"), false, None);
        w.switch(0);
        w.open_doc(file("/x"), false, None);
        w.close_top(1);
        assert_eq!(w.preview, Some(file("/x")));
        w.close_top(0);
        assert_eq!(w.preview, None);
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
}
