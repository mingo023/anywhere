use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// What the desktop remembers across launches, in `desktop.json` next to pocketd's socket.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq)]
#[serde(default)]
pub struct Store {
    pub projects: Vec<String>,
    /// (child, parent): shells opened as tabs of a session.
    pub children: Vec<(String, String)>,
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

    pub fn parent(&self, id: &str) -> Option<&str> {
        self.children.iter().find(|(c, _)| c == id).map(|(_, p)| p.as_str())
    }

    pub fn children_of<'a>(&'a self, parent: &'a str) -> impl Iterator<Item = &'a str> {
        self.children.iter().filter(move |(_, p)| p == parent).map(|(c, _)| c.as_str())
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
        s.children.push(("c".into(), "p".into()));
        s.save();
        let back = Store::load(&dir);
        assert_eq!(back, s);
        assert_eq!(back.parent("c"), Some("p"));
        assert_eq!(back.children_of("p").collect::<Vec<_>>(), vec!["c"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }
}
