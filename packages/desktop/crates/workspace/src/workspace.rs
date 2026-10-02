/// What a doc tab shows: a file by its absolute path, or a file's changes by its path in the worktree.
#[derive(Clone, Debug, PartialEq)]
pub enum Doc {
    File(String),
    Diff(String),
}

/// The tabs of one worktree: terminal tabs hold rows of panes, each a pocketd terminal id.
#[derive(Clone, Debug, PartialEq)]
pub enum Tab {
    Term(Vec<Vec<String>>),
    Doc(Doc),
}

#[derive(Debug, PartialEq, Default)]
pub struct Workspace {
    pub tabs: Vec<Tab>,
    pub active: usize,
    /// The doc whose tab the next unpinned open takes over.
    pub preview: Option<Doc>,
}

impl Workspace {
    pub fn active(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    /// Gives each of `mine` not yet shown its own tab and drops panes of `theirs`; unknown panes stay, since a just-spawned terminal is not listed yet.
    pub fn sync(&mut self, mine: &[String], theirs: &[String]) {
        for id in theirs {
            self.remove(id);
        }
        for id in mine {
            if self.tab_of(id).is_none() {
                self.tabs.push(Tab::Term(vec![vec![id.clone()]]));
            }
        }
    }

    /// Selects the tab showing terminal `id`, giving it one if none does.
    pub fn add_tab(&mut self, id: String) {
        self.active = self.tab_of(&id).unwrap_or_else(|| {
            self.tabs.push(Tab::Term(vec![vec![id]]));
            self.tabs.len() - 1
        });
    }

    /// Splits the active terminal tab; with a doc tab active the pane opens as a new tab.
    pub fn split(&mut self, id: String, down: bool) {
        match self.tabs.get_mut(self.active) {
            Some(Tab::Term(rows)) if down => rows.push(vec![id]),
            Some(Tab::Term(rows)) => rows.last_mut().expect("tabs keep a pane").push(id),
            _ => self.add_tab(id),
        }
    }

    pub fn tab_of(&self, id: &str) -> Option<usize> {
        self.tabs.iter().position(|t| matches!(t, Tab::Term(rows) if rows.iter().flatten().any(|p| p == id)))
    }

    /// Shows `doc` in its tab. A doc not shown yet takes over the preview tab unless `pin`, else gets a new tab.
    pub fn open_doc(&mut self, doc: Doc, pin: bool) {
        let shown = self.doc_tab(&doc);
        let preview = self.preview.as_ref().and_then(|p| self.doc_tab(p));
        self.active = match (shown, preview) {
            (Some(i), _) => i,
            (None, Some(i)) if !pin => {
                self.tabs[i] = Tab::Doc(doc.clone());
                i
            }
            (None, _) => {
                self.tabs.push(Tab::Doc(doc.clone()));
                self.tabs.len() - 1
            }
        };
        if pin {
            self.pin(&doc);
        } else if shown.is_none() {
            self.preview = Some(doc);
        }
    }

    /// Keeps `doc`'s tab from being taken over by the next preview.
    pub fn pin(&mut self, doc: &Doc) {
        if self.preview.as_ref() == Some(doc) {
            self.preview = None;
        }
    }

    pub fn doc_tab(&self, doc: &Doc) -> Option<usize> {
        self.tabs.iter().position(|t| matches!(t, Tab::Doc(d) if d == doc))
    }

    pub fn remove(&mut self, id: &str) {
        for tab in &mut self.tabs {
            if let Tab::Term(rows) = tab {
                rows.iter_mut().for_each(|r| r.retain(|p| p != id));
                rows.retain(|r| !r.is_empty());
            }
        }
        let before = self.tabs[..self.active.min(self.tabs.len())].iter().filter(|t| matches!(t, Tab::Term(r) if r.is_empty())).count();
        self.tabs.retain(|t| !matches!(t, Tab::Term(r) if r.is_empty()));
        self.active = self.active.saturating_sub(before).min(self.tabs.len().saturating_sub(1));
    }

    /// Moves tab `from` into `to`'s place, keeping the shown tab shown.
    pub fn move_tab(&mut self, from: usize, to: usize) {
        if from >= self.tabs.len() || to >= self.tabs.len() {
            return;
        }
        let tab = self.tabs.remove(from);
        self.tabs.insert(to, tab);
        self.active = match self.active {
            a if a == from => to,
            a if from < a && a <= to => a - 1,
            a if to <= a && a < from => a + 1,
            a => a,
        };
    }

    pub fn close_tab(&mut self, i: usize) -> Vec<String> {
        if i >= self.tabs.len() {
            return Vec::new();
        }
        let ids = match self.tabs.remove(i) {
            Tab::Term(rows) => rows.into_iter().flatten().collect(),
            Tab::Doc(doc) => {
                self.pin(&doc);
                Vec::new()
            }
        };
        if self.active > i || self.active == self.tabs.len() {
            self.active = self.active.saturating_sub(1);
        }
        ids
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn term(rows: &[&[&str]]) -> Tab {
        Tab::Term(rows.iter().map(|r| r.iter().map(|s| s.to_string()).collect()).collect())
    }

    fn diff(path: &str) -> Doc {
        Doc::Diff(path.into())
    }

    fn file(path: &str) -> Doc {
        Doc::File(path.into())
    }

    fn with(ids: &[&str]) -> Workspace {
        let mut w = Workspace::default();
        w.sync(&ids.iter().map(|s| s.to_string()).collect::<Vec<_>>(), &[]);
        w
    }

    #[test]
    fn sync_gives_each_new_terminal_a_tab_and_drops_moved_ones() {
        let mut w = with(&["a", "b"]);
        w.split("c".into(), true);
        w.active = 1;
        w.sync(&["a", "b", "c", "d"].map(String::from), &[]);
        assert_eq!(w.tabs, vec![term(&[&["a"], &["c"]]), term(&[&["b"]]), term(&[&["d"]])]);
        w.sync(&[], &["a".to_string()]);
        assert_eq!((w.tabs, w.active), (vec![term(&[&["c"]]), term(&[&["b"]]), term(&[&["d"]])], 1));
    }

    #[test]
    fn sync_keeps_a_pane_not_listed_yet() {
        let mut w = Workspace::default();
        w.add_tab("x".into());
        w.sync(&["a".to_string()], &[]);
        assert_eq!(w.tabs, vec![term(&[&["x"]]), term(&[&["a"]])]);
    }

    #[test]
    fn adding_a_terminal_a_tab_already_shows_selects_that_tab() {
        let mut w = with(&["a", "b"]);
        w.add_tab("a".into());
        assert_eq!((w.tabs, w.active), (vec![term(&[&["a"]]), term(&[&["b"]])], 0));
    }

    #[test]
    fn splits_right_into_the_last_row_and_down_into_a_new_one() {
        let mut w = with(&["a"]);
        w.split("b".into(), true);
        w.split("c".into(), false);
        assert_eq!(w.tabs, vec![term(&[&["a"], &["b", "c"]])]);
        w.open_doc(diff("x"), true);
        w.split("d".into(), false);
        assert_eq!(w.tabs.len(), 3);
        assert_eq!(w.active, 2);
    }

    #[test]
    fn removing_the_last_pane_drops_its_tab_and_keeps_the_selection() {
        let mut w = with(&["a", "b", "c"]);
        w.active = 2;
        w.remove("b");
        assert_eq!(w.tabs, vec![term(&[&["a"]]), term(&[&["c"]])]);
        assert_eq!(w.active, 1);
    }

    #[test]
    fn closing_a_tab_returns_its_sessions() {
        let mut w = with(&["a", "b"]);
        w.split("c".into(), true);
        w.open_doc(diff("x"), true);
        assert_eq!(w.close_tab(1), vec!["b".to_string()]);
        assert_eq!(w.active, 1);
        assert_eq!(w.close_tab(1), Vec::<String>::new());
        assert_eq!(w.active, 0);
    }

    #[test]
    fn a_moved_tab_takes_the_place_it_was_dropped_on() {
        let mut w = with(&["a", "b", "c"]);
        w.move_tab(0, 2);
        assert_eq!(w.tabs, vec![term(&[&["b"]]), term(&[&["c"]]), term(&[&["a"]])]);
        w.move_tab(2, 0);
        assert_eq!(w.tabs, vec![term(&[&["a"]]), term(&[&["b"]]), term(&[&["c"]])]);
    }

    #[test]
    fn moving_tabs_keeps_the_shown_tab_shown() {
        for (active, from, to) in [(0, 0, 2), (1, 0, 2), (1, 2, 0), (2, 0, 1), (0, 1, 2)] {
            let mut w = with(&["a", "b", "c"]);
            w.active = active;
            let shown = w.active().cloned();
            w.move_tab(from, to);
            assert_eq!(w.active().cloned(), shown, "showing {active}, moving {from} to {to}");
        }
    }

    #[test]
    fn a_move_out_of_range_does_nothing() {
        let mut w = with(&["a", "b"]);
        w.move_tab(0, 5);
        w.move_tab(5, 0);
        assert_eq!(w.tabs, vec![term(&[&["a"]]), term(&[&["b"]])]);
    }

    #[test]
    fn each_doc_gets_one_tab() {
        let mut w = with(&["a"]);
        w.open_doc(Doc::File("/r/x".into()), true);
        w.open_doc(diff("y"), true);
        w.open_doc(Doc::File("/r/x".into()), true);
        assert_eq!((&w.tabs[1..], w.active), (&[Tab::Doc(Doc::File("/r/x".into())), Tab::Doc(diff("y"))][..], 1));
    }

    #[test]
    fn an_unpinned_open_takes_over_the_preview_tab_whether_file_or_diff() {
        let mut w = with(&["a"]);
        w.open_doc(file("x"), false);
        w.open_doc(diff("y"), false);
        assert_eq!((&w.tabs[1..], w.active), (&[Tab::Doc(diff("y"))][..], 1));
        assert_eq!(w.preview, Some(diff("y")));
    }

    #[test]
    fn a_pinned_tab_stays_when_the_next_preview_opens() {
        let mut w = with(&["a"]);
        w.open_doc(file("x"), false);
        w.pin(&file("x"));
        w.open_doc(diff("y"), false);
        assert_eq!(&w.tabs[1..], &[Tab::Doc(file("x")), Tab::Doc(diff("y"))][..]);
    }

    #[test]
    fn a_pinned_open_leaves_the_preview_tab_alone() {
        let mut w = with(&["a"]);
        w.open_doc(file("x"), false);
        w.open_doc(file("y"), true);
        assert_eq!((&w.tabs[1..], w.active), (&[Tab::Doc(file("x")), Tab::Doc(file("y"))][..], 2));
        assert_eq!(w.preview, Some(file("x")));
        w.open_doc(file("x"), true);
        assert_eq!((w.active, w.preview.clone()), (1, None));
    }

    #[test]
    fn reopening_a_shown_doc_shows_its_tab_without_pinning_or_replacing() {
        let mut w = with(&["a"]);
        w.open_doc(file("x"), true);
        w.open_doc(diff("y"), false);
        w.open_doc(file("x"), false);
        assert_eq!((w.tabs.len(), w.active, w.preview.clone()), (3, 1, Some(diff("y"))));
    }

    #[test]
    fn closing_the_preview_tab_leaves_no_preview() {
        let mut w = with(&["a"]);
        w.open_doc(file("x"), false);
        w.close_tab(1);
        assert_eq!(w.preview, None);
    }

    #[test]
    fn finds_the_tab_holding_a_pane() {
        let mut w = with(&["a", "b"]);
        w.split("c".into(), true);
        w.open_doc(diff("x"), true);
        assert_eq!((w.tab_of("c"), w.tab_of("b"), w.tab_of("x")), (Some(0), Some(1), None));
    }
}
