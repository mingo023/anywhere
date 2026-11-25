use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// A named box of projects on the sidebar.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Group {
    pub id: u32,
    pub name: String,
    /// An emoji.
    #[serde(default)]
    pub icon: Option<String>,
    #[serde(default)]
    pub folded: bool,
    /// The host it was made on; `None` for this Mac.
    #[serde(default)]
    pub host: Option<String>,
}

/// A row pinned above the groups.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "lowercase")]
pub enum Pin {
    Project(String),
    /// `tree` is a worktree's path, a Local's id, or the project's main worktree for its own Local.
    Tree {
        project: String,
        tree: String,
    },
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
    /// Tree row keys in dragged order, by project path.
    pub trees: BTreeMap<String, Vec<String>>,
}

impl Sections {
    /// Adds a group named `name` and returns its id.
    pub fn add_group(&mut self, name: &str, host: Option<&str>) -> u32 {
        let id = self.groups.iter().map(|g| g.id + 1).max().unwrap_or(1);
        self.groups.push(Group { id, name: name.to_string(), icon: None, folded: false, host: host.map(str::to_string) });
        id
    }

    /// The groups made on `host`, `None` for this Mac.
    pub fn groups_on<'a>(&'a self, host: Option<&'a str>) -> impl Iterator<Item = &'a Group> {
        self.groups.iter().filter(move |g| g.host.as_deref() == host)
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

    /// Picking the group's icon again takes it off.
    pub fn pick_group_icon(&mut self, id: u32, icon: &str) {
        if let Some(g) = self.group_mut(id) {
            g.icon = (g.icon.as_deref() != Some(icon)).then(|| icon.to_string());
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

    /// Moves each group on this Mac whose projects are all on one host to that host, as only groups saved before groups
    /// had a machine can hold a host's projects, then drops the projects left in a group on another machine.
    pub fn settle_hosts(&mut self) {
        for g in self.groups.iter_mut().filter(|g| g.host.is_none()) {
            let mut machines = self.members.iter().filter(|(_, id)| **id == g.id).map(|(p, _)| hosts::split(p).0);
            if let Some(Some(host)) = machines.next().filter(|first| machines.all(|m| m == *first)) {
                g.host = Some(host.to_string());
            }
        }
        let members = std::mem::take(&mut self.members);
        self.members = members.into_iter().filter(|(p, id)| self.can_hold(*id, p)).collect();
    }

    fn can_hold(&self, id: u32, project: &str) -> bool {
        self.group(id).is_some_and(|g| g.host.as_deref() == hosts::split(project).0)
    }

    /// `project`'s group, if it still exists on its machine.
    pub fn group_of(&self, project: &str) -> Option<u32> {
        self.members.get(project).copied().filter(|&id| self.can_hold(id, project))
    }

    /// `None`, or a group on another machine, ungroups it. Either way it leaves Pinned, which would hide it from its group.
    pub fn set_group(&mut self, project: &str, group: Option<u32>) {
        match group.filter(|&id| self.can_hold(id, project)) {
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

    /// Drops `project`'s group, pins and tree order, as when it leaves the sidebar.
    pub fn forget(&mut self, project: &str) {
        self.members.remove(project);
        self.pinned.retain(|p| p.project() != project);
        self.trees.remove(project);
    }

    /// `project`'s `rows` in the order they were dragged to: those it names trade the places they hold, the rest keep theirs.
    pub fn order_trees<T>(&self, project: &str, rows: Vec<T>, key: impl Fn(&T) -> &str) -> Vec<T> {
        let Some(order) = self.trees.get(project) else { return rows };
        let mut named = Vec::new();
        let slots: Vec<Option<T>> = rows
            .into_iter()
            .map(|row| match order.iter().position(|k| k == key(&row)) {
                Some(rank) => {
                    named.push((rank, row));
                    None
                }
                None => Some(row),
            })
            .collect();
        named.sort_by_key(|(rank, _)| *rank);
        let mut named = named.into_iter().map(|(_, row)| row);
        slots.into_iter().filter_map(|slot| slot.or_else(|| named.next())).collect()
    }

    /// Moves `from` to `to`'s place among `keys`, the project's rows as shown, shifting those between.
    pub fn move_tree(&mut self, project: &str, mut keys: Vec<String>, from: &str, to: &str) {
        let (Some(i), Some(j)) = (keys.iter().position(|k| k == from), keys.iter().position(|k| k == to)) else { return };
        let key = keys.remove(i);
        keys.insert(j, key);
        self.trees.insert(project.to_string(), keys);
    }

    /// `projects`' places as indices into it: the pinned ones' places drop out of the rest, each group in `groups` order, then the ungrouped.
    /// `grouped` says which projects can be grouped or pinned; the others always go with the ungrouped.
    pub fn arrange(&self, projects: &[String], host: Option<&str>, grouped: impl Fn(&str) -> bool) -> Arrangement {
        let mut groups: Vec<(u32, Vec<usize>)> = self.groups_on(host).map(|g| (g.id, Vec::new())).collect();
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
    fn a_group_shows_only_on_the_machine_it_was_made_on_and_holds_only_its_projects() {
        let mut s = Sections::default();
        let (mac, devbox) = (s.add_group("Mac", None), s.add_group("Box", Some("devbox")));
        s.set_group("//devbox/api", Some(mac));
        s.set_group("/a", Some(devbox));
        assert_eq!((s.group_of("//devbox/api"), s.group_of("/a")), (None, None));
        s.set_group("//devbox/api", Some(devbox));
        let on_box = s.arrange(&["//devbox/api".to_string(), "//devbox/web".to_string()], Some("devbox"), |_| true);
        assert_eq!(on_box.groups, vec![(devbox, vec![0])]);
        assert_eq!(s.arrange(&projects(), None, |_| true).groups, vec![(mac, vec![])]);
    }

    #[test]
    fn groups_saved_before_they_had_a_machine_move_to_the_host_holding_all_their_projects_else_stay_on_this_mac() {
        let raw = r#"{"groups": [{"id": 1, "name": "Mixed"}, {"id": 2, "name": "Box"}, {"id": 3, "name": "Empty"}],
            "members": {"/a": 1, "//devbox/api": 1, "//devbox/web": 2, "//devbox/db": 2}}"#;
        let mut s: Sections = serde_json::from_str(raw).unwrap();
        s.settle_hosts();
        let hosts: Vec<_> = s.groups.iter().map(|g| g.host.as_deref()).collect();
        assert_eq!(hosts, [None, Some("devbox"), None]);
        assert_eq!((s.group_of("/a"), s.group_of("//devbox/api"), s.group_of("//devbox/web")), (Some(1), None, Some(2)));
        s.set_group("/a", None);
        s.settle_hosts();
        assert_eq!(s.group(1).unwrap().host, None, "a later load doesn't move it for the host project it once held");
    }

    #[test]
    fn projects_sit_in_their_groups_in_list_order_and_the_rest_after() {
        let mut s = Sections::default();
        let (work, side) = (s.add_group("Work", None), s.add_group("Side", None));
        s.set_group("/c", Some(work));
        s.set_group("/a", Some(work));
        s.set_group("/b", Some(side));
        let got = s.arrange(&projects(), None, |_| true);
        assert_eq!(got, Arrangement { pinned: vec![], groups: vec![(work, vec![0, 2]), (side, vec![1])], other: vec![3] });
    }

    #[test]
    fn a_pinned_project_leaves_its_group_and_a_pinned_tree_leaves_its_project_in_place() {
        let mut s = Sections::default();
        let work = s.add_group("Work", None);
        s.set_group("/a", Some(work));
        s.toggle_pin(Pin::Project("/a".into()));
        s.toggle_pin(Pin::Tree { project: "/b".into(), tree: "/wt".into() });
        let got = s.arrange(&projects(), None, |_| true);
        assert_eq!(got.pinned, vec![Pin::Project("/a".into()), Pin::Tree { project: "/b".into(), tree: "/wt".into() }]);
        assert_eq!((got.groups, got.other), (vec![(work, vec![])], vec![1, 2, 3]));
    }

    #[test]
    fn a_project_that_cant_be_grouped_stays_with_the_rest_and_its_pins_hide() {
        let mut s = Sections::default();
        let work = s.add_group("Work", None);
        s.set_group("/a", Some(work));
        s.toggle_pin(Pin::Project("/b".into()));
        let got = s.arrange(&projects(), None, |p| p == "/c" || p == "/d");
        assert_eq!(got, Arrangement { pinned: vec![], groups: vec![(work, vec![])], other: vec![0, 1, 2, 3] });
    }

    #[test]
    fn pins_on_projects_no_longer_listed_hide() {
        let mut s = Sections::default();
        s.toggle_pin(Pin::Project("/gone".into()));
        assert_eq!(s.arrange(&projects(), None, |_| true).pinned, vec![]);
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
        let work = s.add_group("Work", None);
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
        let work = s.add_group("Work", None);
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
        let (work, side) = (s.add_group("Work", None), s.add_group("Side", None));
        s.set_group("/a", Some(work));
        s.set_group("/b", Some(side));
        s.delete_group(work);
        assert_eq!((s.group_of("/a"), s.group_of("/b")), (None, Some(side)));
        assert_eq!(s.groups.iter().map(|g| g.id).collect::<Vec<_>>(), vec![side]);
    }

    #[test]
    fn picking_a_groups_icon_sets_it_and_picking_it_again_takes_it_off() {
        let mut s = Sections::default();
        let g = s.add_group("Work", None);
        s.pick_group_icon(g, "🚀");
        assert_eq!(s.group(g).unwrap().icon.as_deref(), Some("🚀"));
        s.pick_group_icon(g, "🔥");
        assert_eq!(s.group(g).unwrap().icon.as_deref(), Some("🔥"));
        s.pick_group_icon(g, "🔥");
        assert_eq!(s.group(g).unwrap().icon, None);
    }

    #[test]
    fn a_new_group_never_reuses_a_live_groups_id() {
        let mut s = Sections::default();
        let (a, b) = (s.add_group("A", None), s.add_group("B", None));
        s.delete_group(a);
        let c = s.add_group("C", None);
        assert!(a != b && c != b);
    }

    #[test]
    fn a_project_joins_only_a_group_that_exists_and_moving_it_leaves_the_old_one() {
        let mut s = Sections::default();
        let (work, side) = (s.add_group("Work", None), s.add_group("Side", None));
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
        let id = s.add_group("Work", None);
        s.rename_group(id, "  ");
        s.rename_group(id, " Clients ");
        s.rename_group(id, "");
        assert_eq!(s.group(id).unwrap().name, "Clients");
    }

    #[test]
    fn forgetting_a_project_drops_its_group_and_every_pin_in_it() {
        let mut s = Sections::default();
        let work = s.add_group("Work", None);
        s.set_group("/a", Some(work));
        s.toggle_pin(Pin::Project("/a".into()));
        s.toggle_pin(Pin::Tree { project: "/a".into(), tree: "/wt".into() });
        s.toggle_pin(Pin::Project("/b".into()));
        s.move_tree("/a", keys(&["/x", "/y"]), "/y", "/x");
        s.forget("/a");
        assert_eq!((s.group_of("/a"), s.pinned.clone()), (None, vec![Pin::Project("/b".into())]));
        assert!(s.trees.is_empty());
    }

    fn keys(k: &[&str]) -> Vec<String> {
        k.iter().map(|k| k.to_string()).collect()
    }

    fn ordered(s: &Sections, project: &str, rows: &[&str]) -> Vec<String> {
        s.order_trees(project, keys(rows), |k| k.as_str())
    }

    #[test]
    fn a_tree_dragged_down_lands_below_the_row_it_was_dropped_on_and_up_above_it() {
        let mut s = Sections::default();
        s.move_tree("/a", keys(&["/w1", "/w2", "/w3"]), "/w1", "/w2");
        assert_eq!(ordered(&s, "/a", &["/w1", "/w2", "/w3"]), keys(&["/w2", "/w1", "/w3"]));
        s.move_tree("/a", ordered(&s, "/a", &["/w1", "/w2", "/w3"]), "/w3", "/w2");
        assert_eq!(ordered(&s, "/a", &["/w1", "/w2", "/w3"]), keys(&["/w3", "/w2", "/w1"]));
    }

    #[test]
    fn a_project_never_dragged_keeps_its_rows_and_another_projects_order_leaves_them_alone() {
        let mut s = Sections::default();
        s.move_tree("/b", keys(&["/w1", "/w2"]), "/w2", "/w1");
        assert_eq!(ordered(&s, "/a", &["/w1", "/w2"]), keys(&["/w1", "/w2"]));
    }

    #[test]
    fn rows_the_order_never_named_keep_their_places_around_the_dragged_ones() {
        let mut s = Sections::default();
        s.move_tree("/a", keys(&["/w1", "/w2"]), "/w2", "/w1");
        assert_eq!(ordered(&s, "/a", &["/own", "/w1", "/gone-and-back", "/w2", "/new"]), keys(&["/own", "/w2", "/gone-and-back", "/w1", "/new"]));
        assert_eq!(ordered(&s, "/a", &["/w1"]), keys(&["/w1"]), "a row deleted since drops out");
    }

    #[test]
    fn dropping_a_tree_on_itself_or_on_a_row_no_longer_shown_changes_nothing() {
        let mut s = Sections::default();
        s.move_tree("/a", keys(&["/w1", "/w2"]), "/w1", "/gone");
        s.move_tree("/a", keys(&["/w1", "/w2"]), "/gone", "/w1");
        assert!(s.trees.is_empty());
        s.move_tree("/a", keys(&["/w1", "/w2"]), "/w1", "/w1");
        assert_eq!(ordered(&s, "/a", &["/w1", "/w2"]), keys(&["/w1", "/w2"]));
    }

    #[test]
    fn sections_saved_before_trees_could_be_dragged_still_read() {
        let s: Sections = serde_json::from_str(r#"{"pins_folded": true, "sort": "name"}"#).unwrap();
        assert_eq!((s.pins_folded, s.sort, s.trees.is_empty()), (true, Sort::Name, true));
    }
}
