use crate::Workspace;
use std::collections::HashMap;

/// Every worktree's panels, by the worktree's path.
#[derive(Default)]
pub struct Workspaces {
    trees: HashMap<String, Workspace>,
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
