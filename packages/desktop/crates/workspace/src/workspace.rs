pub mod tree;

use serde::{Deserialize, Serialize};
use tree::{Edge, PaneId, Rect, Tree};

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

/// Where a new terminal opens: in a pane, the terminal pane nearest the focus when none is given or it is gone, or in a new pane at an edge of one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Place {
    Pane(Option<PaneId>),
    Split(PaneId, Edge),
}

#[derive(Debug, PartialEq, Default, Serialize, Deserialize)]
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
        self.tree.drawn().into_iter().filter_map(|p| Some((p.id, p.active()?))).collect()
    }

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

    /// A saved workspace without the terminals pocketd no longer runs, or pages a hand-edited file kept; the panels they leave empty close.
    pub fn restored(mut self, live: &[String]) -> Self {
        self.tree.retain(|t| match t {
            Tab::Term(id) => live.contains(id),
            Tab::Web(_) => false,
            Tab::Doc(_) => true,
        });
        self
    }
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
}
