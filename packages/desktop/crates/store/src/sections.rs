use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A named box of projects on the sidebar.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Group {
    pub id: u32,
    pub name: String,
    /// 0xRRGGBB.
    pub color: u32,
    #[serde(default)]
    pub folded: bool,
}

/// A row pinned above the groups.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Pin {
    Project(String),
    /// `tree` is a worktree's path, a Local's id, or the project's main worktree for its own Local.
    Tree { project: String, tree: String },
}

impl Pin {
    pub fn project(&self) -> &str {
        match self {
            Pin::Project(p) | Pin::Tree { project: p, .. } => p,
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Sort {
    #[default]
    Manual,
    Name,
    Recent,
}

/// How the sidebar sorts its projects out: pinned rows on top, then groups, then the rest.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Sections {
    pub groups: Vec<Group>,
    /// Each grouped project's group, by project path.
    pub members: BTreeMap<String, u32>,
    /// In the order they were pinned.
    pub pinned: Vec<Pin>,
    pub pins_folded: bool,
    pub sort: Sort,
}

impl Sections {
    /// Adds a group named `name` and returns its id.
    pub fn add_group(&mut self, name: &str, color: u32) -> u32 {
        let id = self.groups.iter().map(|g| g.id + 1).max().unwrap_or(1);
        self.groups.push(Group { id, name: name.to_string(), color, folded: false });
        id
    }

    pub fn group(&self, id: u32) -> Option<&Group> {
        self.groups.iter().find(|g| g.id == id)
    }

    fn group_mut(&mut self, id: u32) -> Option<&mut Group> {
        self.groups.iter_mut().find(|g| g.id == id)
    }

    /// A blank name keeps the old one.
    pub fn rename_group(&mut self, id: u32, name: &str) {
        let name = name.trim();
        if let Some(g) = self.group_mut(id).filter(|_| !name.is_empty()) {
            g.name = name.to_string();
        }
    }

    pub fn recolor_group(&mut self, id: u32, color: u32) {
        if let Some(g) = self.group_mut(id) {
            g.color = color;
        }
    }

    pub fn toggle_group(&mut self, id: u32) {
        if let Some(g) = self.group_mut(id) {
            g.folded = !g.folded;
        }
    }

    /// Unfolds the section holding `project`'s row, or its `tree`'s.
    pub fn reveal(&mut self, project: &str, tree: Option<&str>) {
        let tree_pinned = tree.is_some_and(|t| self.is_pinned(&Pin::Tree { project: project.into(), tree: t.into() }));
        if tree_pinned || self.is_pinned(&Pin::Project(project.into())) {
            self.pins_folded = false;
        } else if let Some(g) = self.group_of(project).and_then(|id| self.group_mut(id)) {
            g.folded = false;
        }
    }

    /// Its projects stay, ungrouped.
    pub fn delete_group(&mut self, id: u32) {
        self.groups.retain(|g| g.id != id);
        self.members.retain(|_, g| *g != id);
    }

    /// `project`'s group, if it still exists.
    pub fn group_of(&self, project: &str) -> Option<u32> {
        self.members.get(project).copied().filter(|&id| self.group(id).is_some())
    }

    /// `None` ungroups it. Either way it leaves Pinned, which would hide it from its group.
    pub fn set_group(&mut self, project: &str, group: Option<u32>) {
        match group.filter(|&id| self.group(id).is_some()) {
            Some(id) => self.members.insert(project.to_string(), id),
            None => self.members.remove(project),
        };
        self.pinned.retain(|p| *p != Pin::Project(project.to_string()));
    }

    pub fn is_pinned(&self, pin: &Pin) -> bool {
        self.pinned.contains(pin)
    }

    pub fn pin(&mut self, pin: Pin) {
        if !self.is_pinned(&pin) {
            self.pinned.push(pin);
        }
    }

    pub fn toggle_pin(&mut self, pin: Pin) {
        match self.pinned.iter().position(|p| *p == pin) {
            Some(i) => _ = self.pinned.remove(i),
            None => self.pinned.push(pin),
        }
    }

    /// As when the worktree or Local `tree` is deleted.
    pub fn unpin_tree(&mut self, tree: &str) {
        self.pinned.retain(|p| !matches!(p, Pin::Tree { tree: t, .. } if t == tree));
    }

    /// Drops `project`'s group and pins, as when it leaves the sidebar.
    pub fn forget(&mut self, project: &str) {
        self.members.remove(project);
        self.pinned.retain(|p| p.project() != project);
    }

    /// `projects`' places as indices into it: the pinned ones' places drop out of the rest, each group in `groups` order, then the ungrouped.
    /// `grouped` says which projects can be grouped or pinned; the others always go with the ungrouped.
    pub fn arrange(&self, projects: &[String], grouped: impl Fn(&str) -> bool) -> Arrangement {
        let mut groups: Vec<(u32, Vec<usize>)> = self.groups.iter().map(|g| (g.id, Vec::new())).collect();
        let mut other = Vec::new();
        for (i, p) in projects.iter().enumerate() {
            let can = grouped(p);
            if can && self.is_pinned(&Pin::Project(p.clone())) {
                continue;
            }
            match self.group_of(p).filter(|_| can).and_then(|id| groups.iter_mut().find(|(g, _)| *g == id)) {
                Some((_, members)) => members.push(i),
                None => other.push(i),
            }
        }
        let pinned = self.pinned.iter().filter(|pin| projects.iter().any(|p| p == pin.project() && grouped(p))).cloned().collect();
        Arrangement { pinned, groups, other }
    }
}

#[derive(Debug, PartialEq)]
pub struct Arrangement {
    pub pinned: Vec<Pin>,
    pub groups: Vec<(u32, Vec<usize>)>,
    pub other: Vec<usize>,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn projects() -> Vec<String> {
        ["/a", "/b", "/c", "/d"].map(String::from).to_vec()
    }

    #[test]
    fn projects_sit_in_their_groups_in_list_order_and_the_rest_after() {
        let mut s = Sections::default();
        let (work, side) = (s.add_group("Work", 1), s.add_group("Side", 2));
        s.set_group("/c", Some(work));
        s.set_group("/a", Some(work));
        s.set_group("/b", Some(side));
        let got = s.arrange(&projects(), |_| true);
        assert_eq!(got, Arrangement { pinned: vec![], groups: vec![(work, vec![0, 2]), (side, vec![1])], other: vec![3] });
    }

    #[test]
    fn a_pinned_project_leaves_its_group_and_a_pinned_tree_leaves_its_project_in_place() {
        let mut s = Sections::default();
        let work = s.add_group("Work", 1);
        s.set_group("/a", Some(work));
        s.toggle_pin(Pin::Project("/a".into()));
        s.toggle_pin(Pin::Tree { project: "/b".into(), tree: "/wt".into() });
        let got = s.arrange(&projects(), |_| true);
        assert_eq!(got.pinned, vec![Pin::Project("/a".into()), Pin::Tree { project: "/b".into(), tree: "/wt".into() }]);
        assert_eq!((got.groups, got.other), (vec![(work, vec![])], vec![1, 2, 3]));
    }

    #[test]
    fn a_project_that_cant_be_grouped_stays_with_the_rest_and_its_pins_hide() {
        let mut s = Sections::default();
        let work = s.add_group("Work", 1);
        s.set_group("/a", Some(work));
        s.toggle_pin(Pin::Project("/b".into()));
        let got = s.arrange(&projects(), |p| p == "/c" || p == "/d");
        assert_eq!(got, Arrangement { pinned: vec![], groups: vec![(work, vec![])], other: vec![0, 1, 2, 3] });
    }

    #[test]
    fn pins_on_projects_no_longer_listed_hide() {
        let mut s = Sections::default();
        s.toggle_pin(Pin::Project("/gone".into()));
        assert_eq!(s.arrange(&projects(), |_| true).pinned, vec![]);
    }

    #[test]
    fn pinning_twice_unpins() {
        let mut s = Sections::default();
        let pin = Pin::Tree { project: "/a".into(), tree: "/wt".into() };
        s.toggle_pin(pin.clone());
        assert!(s.is_pinned(&pin));
        s.toggle_pin(pin.clone());
        assert!(!s.is_pinned(&pin));
    }

    #[test]
    fn moving_a_pinned_project_to_a_group_or_out_of_one_unpins_it_but_keeps_its_tree_pins() {
        let mut s = Sections::default();
        let work = s.add_group("Work", 1);
        let tree = Pin::Tree { project: "/a".into(), tree: "/wt".into() };
        for group in [Some(work), None] {
            s.pin(Pin::Project("/a".into()));
            s.pin(tree.clone());
            s.set_group("/a", group);
            assert_eq!(s.pinned, vec![tree.clone()]);
        }
    }

    #[test]
    fn pinning_what_is_already_pinned_keeps_one_pin() {
        let mut s = Sections::default();
        s.pin(Pin::Project("/a".into()));
        s.pin(Pin::Project("/a".into()));
        assert_eq!(s.pinned, vec![Pin::Project("/a".into())]);
    }

    #[test]
    fn revealing_a_row_unfolds_pinned_when_it_sits_there_and_else_its_group() {
        let mut s = Sections { pins_folded: true, ..Default::default() };
        let work = s.add_group("Work", 1);
        s.set_group("/a", Some(work));
        s.toggle_group(work);
        s.pin(Pin::Tree { project: "/a".into(), tree: "/wt".into() });
        s.reveal("/a", Some("/wt"));
        assert_eq!((s.pins_folded, s.group(work).unwrap().folded), (false, true));
        s.pins_folded = true;
        s.reveal("/a", Some("/other"));
        assert_eq!((s.pins_folded, s.group(work).unwrap().folded), (true, false));
    }

    #[test]
    fn deleting_a_tree_drops_only_its_pin() {
        let mut s = Sections::default();
        s.pin(Pin::Project("/a".into()));
        s.pin(Pin::Tree { project: "/a".into(), tree: "/wt".into() });
        s.pin(Pin::Tree { project: "/a".into(), tree: "/wt2".into() });
        s.unpin_tree("/wt");
        assert_eq!(s.pinned, vec![Pin::Project("/a".into()), Pin::Tree { project: "/a".into(), tree: "/wt2".into() }]);
    }

    #[test]
    fn deleting_a_group_ungroups_its_projects() {
        let mut s = Sections::default();
        let (work, side) = (s.add_group("Work", 1), s.add_group("Side", 2));
        s.set_group("/a", Some(work));
        s.set_group("/b", Some(side));
        s.delete_group(work);
        assert_eq!((s.group_of("/a"), s.group_of("/b")), (None, Some(side)));
        assert_eq!(s.groups.iter().map(|g| g.id).collect::<Vec<_>>(), vec![side]);
    }

    #[test]
    fn a_new_group_never_reuses_a_live_groups_id() {
        let mut s = Sections::default();
        let (a, b) = (s.add_group("A", 1), s.add_group("B", 1));
        s.delete_group(a);
        let c = s.add_group("C", 1);
        assert!(a != b && c != b);
    }

    #[test]
    fn a_project_joins_only_a_group_that_exists_and_moving_it_leaves_the_old_one() {
        let mut s = Sections::default();
        let (work, side) = (s.add_group("Work", 1), s.add_group("Side", 2));
        s.set_group("/a", Some(99));
        assert_eq!(s.group_of("/a"), None);
        s.set_group("/a", Some(work));
        s.set_group("/a", Some(side));
        assert_eq!(s.group_of("/a"), Some(side));
        s.set_group("/a", None);
        assert_eq!(s.group_of("/a"), None);
    }

    #[test]
    fn a_blank_rename_keeps_the_name() {
        let mut s = Sections::default();
        let id = s.add_group("Work", 1);
        s.rename_group(id, "  ");
        s.rename_group(id, " Clients ");
        s.rename_group(id, "");
        assert_eq!(s.group(id).unwrap().name, "Clients");
    }

    #[test]
    fn forgetting_a_project_drops_its_group_and_every_pin_in_it() {
        let mut s = Sections::default();
        let work = s.add_group("Work", 1);
        s.set_group("/a", Some(work));
        s.toggle_pin(Pin::Project("/a".into()));
        s.toggle_pin(Pin::Tree { project: "/a".into(), tree: "/wt".into() });
        s.toggle_pin(Pin::Project("/b".into()));
        s.forget("/a");
        assert_eq!((s.group_of("/a"), s.pinned.clone()), (None, vec![Pin::Project("/b".into())]));
    }
}
