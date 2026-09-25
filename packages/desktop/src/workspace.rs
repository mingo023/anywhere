/// The tabs of one session: terminal tabs hold rows of panes, each a pocketd session id.
#[derive(Debug, PartialEq)]
pub enum Tab {
    Term(Vec<Vec<String>>),
    Changes,
}

#[derive(Debug, PartialEq)]
pub struct Workspace {
    pub tabs: Vec<Tab>,
    pub active: usize,
}

impl Workspace {
    pub fn new<'a>(id: &'a str, children: impl Iterator<Item = &'a str>) -> Self {
        let tabs = std::iter::once(id).chain(children).map(|c| Tab::Term(vec![vec![c.to_string()]])).collect();
        Self { tabs, active: 0 }
    }

    pub fn active(&self) -> Option<&Tab> {
        self.tabs.get(self.active)
    }

    pub fn add_tab(&mut self, id: String) {
        self.tabs.push(Tab::Term(vec![vec![id]]));
        self.active = self.tabs.len() - 1;
    }

    /// Splits the active terminal tab; with the Changes tab active the pane opens as a new tab.
    pub fn split(&mut self, id: String, down: bool) {
        match self.tabs.get_mut(self.active) {
            Some(Tab::Term(rows)) if down => rows.push(vec![id]),
            Some(Tab::Term(rows)) => rows.last_mut().expect("tabs keep a pane").push(id),
            _ => self.add_tab(id),
        }
    }

    pub fn open_changes(&mut self) {
        self.active = match self.tabs.iter().position(|t| *t == Tab::Changes) {
            Some(i) => i,
            None => {
                self.tabs.push(Tab::Changes);
                self.tabs.len() - 1
            }
        };
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

    pub fn close_tab(&mut self, i: usize) -> Vec<String> {
        if i >= self.tabs.len() {
            return Vec::new();
        }
        let ids = match self.tabs.remove(i) {
            Tab::Term(rows) => rows.into_iter().flatten().collect(),
            Tab::Changes => Vec::new(),
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

    #[test]
    fn restores_children_as_tabs() {
        let w = Workspace::new("a", ["b", "c"].into_iter());
        assert_eq!(w.tabs, vec![term(&[&["a"]]), term(&[&["b"]]), term(&[&["c"]])]);
    }

    #[test]
    fn splits_right_into_the_last_row_and_down_into_a_new_one() {
        let mut w = Workspace::new("a", std::iter::empty());
        w.split("b".into(), true);
        w.split("c".into(), false);
        assert_eq!(w.tabs, vec![term(&[&["a"], &["b", "c"]])]);
        w.open_changes();
        w.split("d".into(), false);
        assert_eq!(w.tabs.len(), 3);
        assert_eq!(w.active, 2);
    }

    #[test]
    fn removing_the_last_pane_drops_its_tab_and_keeps_the_selection() {
        let mut w = Workspace::new("a", ["b", "c"].into_iter());
        w.active = 2;
        w.remove("b");
        assert_eq!(w.tabs, vec![term(&[&["a"]]), term(&[&["c"]])]);
        assert_eq!(w.active, 1);
    }

    #[test]
    fn closing_a_tab_returns_its_sessions() {
        let mut w = Workspace::new("a", ["b"].into_iter());
        w.split("c".into(), true);
        w.open_changes();
        assert_eq!(w.close_tab(1), vec!["b".to_string()]);
        assert_eq!(w.active, 1);
        assert_eq!(w.close_tab(1), Vec::<String>::new());
        assert_eq!(w.active, 0);
    }
}
