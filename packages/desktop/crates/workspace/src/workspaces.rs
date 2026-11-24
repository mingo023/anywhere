use crate::Workspace;
use std::collections::{BTreeMap, HashMap};

/// Every worktree's panels, by the worktree's path.
#[derive(Default)]
pub struct Workspaces {
    trees: HashMap<String, Workspace>,
    /// Whether the panels saved last launch were brought back; until then `desktop.json` keeps them as they were.
    restored: bool,
    /// Panels saved last launch for worktrees on hosts, until each host first lists its terminals.
    waiting: BTreeMap<String, Workspace>,
}

impl Workspaces {
    pub fn get(&self, tree: &str) -> Option<&Workspace> {
        self.trees.get(tree)
    }

    pub fn get_mut(&mut self, tree: &str) -> Option<&mut Workspace> {
        self.trees.get_mut(tree)
    }

    /// `tree`'s panels, made empty if it has none yet.
    pub fn entry(&mut self, tree: &str) -> &mut Workspace {
        self.trees.entry(tree.to_string()).or_default()
    }

    pub fn insert(&mut self, tree: String, workspace: Workspace) {
        self.trees.insert(tree, workspace);
    }

    pub fn remove(&mut self, tree: &str) {
        self.trees.remove(tree);
    }

    pub fn clear(&mut self) {
        self.trees.clear();
    }

    pub fn iter(&self) -> impl Iterator<Item = (&String, &Workspace)> {
        self.trees.iter()
    }

    pub fn values(&self) -> impl Iterator<Item = &Workspace> {
        self.trees.values()
    }

    pub fn is_restored(&self) -> bool {
        self.restored
    }

    /// Marks the saved panels as brought back; false if they already were.
    pub fn mark_restored(&mut self) -> bool {
        !std::mem::replace(&mut self.restored, true)
    }

    /// Holds saved panels back until `take_waiting` asks for them.
    pub fn wait(&mut self, saved: BTreeMap<String, Workspace>) {
        self.waiting = saved;
    }

    /// Takes out the waiting panels of the worktrees `pick` names.
    pub fn take_waiting(&mut self, pick: impl Fn(&str) -> bool) -> BTreeMap<String, Workspace> {
        let (taken, rest) = std::mem::take(&mut self.waiting).into_iter().partition(|(tree, _)| pick(tree));
        self.waiting = rest;
        taken
    }

    /// Brings back each of `saved` whose worktree has nothing open, keeping only the terminals of `live` that still run.
    pub fn restore(&mut self, saved: BTreeMap<String, Workspace>, live: &[String]) {
        for (tree, saved) in saved {
            if self.get(&tree).is_none_or(Workspace::is_empty) {
                self.insert(tree, saved.restored(live));
            }
        }
    }

    /// What to save: the panels still waiting, and every worktree's that hold a tab; none until the saved panels are back.
    pub fn saved(&self) -> Option<BTreeMap<String, Workspace>> {
        let shown = self.iter().map(|(t, w)| (t.clone(), w.saved())).filter(|(_, w)| !w.is_empty());
        let waiting = self.waiting.iter().map(|(t, w)| (t.clone(), w.saved()));
        self.restored.then(|| waiting.chain(shown).collect())
    }

    pub fn values_mut(&mut self) -> impl Iterator<Item = &mut Workspace> {
        self.trees.values_mut()
    }
}

impl std::ops::Index<&str> for Workspaces {
    type Output = Workspace;

    fn index(&self, tree: &str) -> &Workspace {
        &self.trees[tree]
    }
}

#[cfg(test)]
mod tests {
    use super::Workspaces;
    use crate::Workspace;
    use std::collections::BTreeMap;

    #[test]
    fn a_hosts_panels_wait_until_that_host_lists_its_terminals() {
        let mut all = Workspaces::default();
        all.wait(BTreeMap::from([("//devbox/app".to_string(), Workspace::default()), ("//gpu/app".to_string(), Workspace::default())]));
        let theirs = all.take_waiting(|tree| tree.starts_with("//devbox/"));
        assert_eq!(theirs.into_keys().collect::<Vec<_>>(), ["//devbox/app"]);
        assert_eq!(all.take_waiting(|_| true).into_keys().collect::<Vec<_>>(), ["//gpu/app"]);
    }

    #[test]
    fn nothing_is_saved_until_the_saved_panels_are_back() {
        let mut all = Workspaces::default();
        assert_eq!(all.saved(), None);
        assert!(all.mark_restored());
        assert!(!all.mark_restored());
        assert_eq!(all.saved(), Some(BTreeMap::new()));
    }
}
