use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

/// How a repository shows in the rail and how new worktrees of it are made.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct RepoConfig {
    pub name: String,
    pub color: u32,
    pub base: String,
    pub worktrees: String,
    pub setup: String,
    pub copy: Vec<String>,
}

/// Which status changes play a sound; a key missing from `desktop.json` reads as on.
#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(default)]
pub struct Sounds {
    pub needs_you: bool,
    pub done: bool,
    pub failed: bool,
}

impl Default for Sounds {
    fn default() -> Self {
        Self { needs_you: true, done: true, failed: true }
    }
}

/// What the desktop remembers across launches, in `desktop.json` next to pocketd's socket.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
#[serde(default)]
pub struct Store {
    pub projects: Vec<String>,
    /// Keyed by the repository's path in `projects`.
    pub repos: BTreeMap<String, RepoConfig>,
    /// Project paths whose worktrees are hidden on the sidebar.
    pub collapsed: BTreeSet<String>,
    pub sounds: Sounds,
    #[serde(skip)]
    path: PathBuf,
}

impl Store {
    pub fn load(home: &Path) -> Self {
        let path = home.join("desktop.json");
        let mut s: Self = std::fs::read(&path).ok().and_then(|raw| serde_json::from_slice(&raw).ok()).unwrap_or_default();
        s.path = path;
        s
    }

    pub fn save(&self) {
        if let Ok(raw) = serde_json::to_vec_pretty(self) {
            let _ = std::fs::write(&self.path, raw);
        }
    }

    pub fn add(&mut self, path: &str) {
        if !self.projects.iter().any(|p| p == path) {
            self.projects.push(path.to_string());
        }
    }

    /// Forgets `path`'s place on the sidebar but keeps its settings, so adding it back restores them.
    pub fn remove(&mut self, path: &str) {
        self.projects.retain(|p| p != path);
        self.collapsed.remove(path);
    }

    pub fn toggle(&mut self, path: &str) {
        if !self.collapsed.remove(path) {
            self.collapsed.insert(path.to_string());
        }
    }

    /// Puts project `from` where `to` is, shifting the ones between.
    pub fn move_project(&mut self, from: &str, to: &str) {
        let (Some(i), Some(j)) = (self.projects.iter().position(|p| p == from), self.projects.iter().position(|p| p == to)) else { return };
        let p = self.projects.remove(i);
        self.projects.insert(j, p);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_through_desktop_json() {
        let dir = std::env::temp_dir().join(format!("pocket-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Store::load(&dir);
        s.projects.push("/w".into());
        s.collapsed.insert("/w".into());
        s.repos.insert("/w".into(), RepoConfig { name: "w".into(), color: 0xd97757ff, copy: vec![".env".into()], ..Default::default() });
        s.save();
        let back = Store::load(&dir);
        assert_eq!(back, s);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn a_dragged_project_takes_the_place_of_the_one_it_lands_on() {
        let mut s = Store { projects: ["a", "b", "c", "d"].map(String::from).to_vec(), ..Default::default() };
        s.move_project("a", "c");
        assert_eq!(s.projects, ["b", "c", "a", "d"]);
        s.move_project("d", "b");
        assert_eq!(s.projects, ["d", "b", "c", "a"]);
        s.move_project("x", "b");
        assert_eq!(s.projects, ["d", "b", "c", "a"]);
    }

    #[test]
    fn removing_a_project_keeps_its_settings() {
        let mut s = Store::default();
        s.add("/w");
        s.add("/w");
        s.repos.insert("/w".into(), RepoConfig { name: "w".into(), ..Default::default() });
        s.toggle("/w");
        assert_eq!((s.projects.len(), s.collapsed.contains("/w")), (1, true));
        s.remove("/w");
        assert!(s.projects.is_empty() && s.collapsed.is_empty());
        assert!(s.repos.contains_key("/w"));
        s.toggle("/w");
        s.toggle("/w");
        assert!(!s.collapsed.contains("/w"));
    }

    #[test]
    fn sounds_default_on_when_desktop_json_predates_them() {
        let dir = std::env::temp_dir().join(format!("pocket-sounds-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("desktop.json"), r#"{"projects":["/w"]}"#).unwrap();
        assert_eq!(Store::load(&dir).sounds, Sounds { needs_you: true, done: true, failed: true });
        std::fs::write(dir.join("desktop.json"), r#"{"sounds":{"done":false}}"#).unwrap();
        assert_eq!(Store::load(&dir).sounds, Sounds { needs_you: true, done: false, failed: true });
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
