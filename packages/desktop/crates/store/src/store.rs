use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use workspace::Workspace;

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
    pub launch: LaunchPick,
}

/// The agent a Project's next session starts with.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct LaunchPick {
    pub provider: String,
    pub model: String,
    pub effort: String,
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

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Layout {
    #[default]
    Sidebars,
    Compact,
    Focus,
}

/// The window's last windowed rect; `x`/`y` are relative to the origin of the display `display` names.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct WindowGeometry {
    pub display: Option<String>,
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Dragged sidebar widths; `None` keeps the design's width.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub struct ColumnWidths {
    pub projects: Option<f32>,
    pub sessions: Option<f32>,
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
    pub window: Option<WindowGeometry>,
    pub layout: Layout,
    pub widths: ColumnWidths,
    pub sounds: Sounds,
    /// Each worktree's panels, keyed by the worktree's path.
    #[serde(deserialize_with = "readable_layouts")]
    pub layouts: BTreeMap<String, Workspace>,
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
        if let Some((path, raw)) = self.encode() {
            write(&path, &raw);
        }
    }

    /// The file and bytes `save` would write, for writing off the UI thread.
    pub fn encode(&self) -> Option<(PathBuf, Vec<u8>)> {
        serde_json::to_vec_pretty(self).ok().map(|raw| (self.path.clone(), raw))
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

/// Drops layouts that don't parse, since `Store::load` forgets everything on any error.
fn readable_layouts<'de, D: serde::Deserializer<'de>>(d: D) -> Result<BTreeMap<String, Workspace>, D::Error> {
    let raw = BTreeMap::<String, serde_json::Value>::deserialize(d)?;
    Ok(raw.into_iter().filter_map(|(path, v)| Some((path, serde_json::from_value(v).ok()?))).collect())
}

/// Replaces `path` in one rename, owner-only, so pocketd never reads half a file.
pub fn write(path: &Path, raw: &[u8]) {
    static WRITING: std::sync::Mutex<()> = std::sync::Mutex::new(());
    let _held = WRITING.lock();
    let _ = write_private(path, raw);
}

fn write_private(path: &Path, raw: &[u8]) -> std::io::Result<()> {
    use std::io::Write;
    use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
    let tmp = path.with_extension("json.tmp");
    let mut f = std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(&tmp)?;
    f.set_permissions(std::fs::Permissions::from_mode(0o600))?;
    f.write_all(raw)?;
    std::fs::rename(&tmp, path)
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
        let launch = LaunchPick { provider: "claude".into(), model: "opus".into(), effort: "high".into() };
        s.repos.insert("/w".into(), RepoConfig { name: "w".into(), color: 0xd97757ff, copy: vec![".env".into()], launch, ..Default::default() });
        s.window = Some(WindowGeometry { display: Some("D1".into()), x: 40., y: 60., width: 1200., height: 800. });
        s.layout = Layout::Compact;
        s.widths = ColumnWidths { projects: Some(260.), sessions: None };
        s.save();
        let back = Store::load(&dir);
        assert_eq!(back, s);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_old_desktop_json_loads_with_default_geometry() {
        let dir = std::env::temp_dir().join(format!("pocket-store-old-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join("desktop.json"), r#"{"projects":["/w"]}"#).unwrap();
        let s = Store::load(&dir);
        assert_eq!((s.projects.len(), &s.window, s.layout, s.widths), (1, &None, Layout::Sidebars, ColumnWidths::default()));
        assert!(s.layouts.is_empty());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn panels_round_trip_through_desktop_json() {
        use workspace::tree::Edge;
        use workspace::{Doc, Tab, Workspace};
        let dir = std::env::temp_dir().join(format!("pocket-store-layouts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Store::load(&dir);
        let mut w = Workspace::default();
        w.tree.push(0, Tab::Term("t1".into()));
        w.tree.split(0, Edge::Right, Tab::Doc(Doc::File("/w/a.rs".into())));
        s.layouts.insert("/w".into(), w);
        s.save();
        assert_eq!(Store::load(&dir).layouts, s.layouts);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unreadable_layout_is_dropped_and_the_rest_of_desktop_json_kept() {
        use workspace::{Tab, Workspace};
        let dir = std::env::temp_dir().join(format!("pocket-store-bad-layout-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut good = Workspace::default();
        good.tree.push(0, Tab::Term("t1".into()));
        let raw = serde_json::json!({
            "projects": ["/w"],
            "layouts": { "/w": serde_json::to_value(&good).unwrap(), "/x": { "tree": 5 } },
        });
        std::fs::write(dir.join("desktop.json"), raw.to_string()).unwrap();
        let s = Store::load(&dir);
        assert_eq!(s.projects, ["/w"]);
        assert_eq!(s.layouts.get("/w"), Some(&good));
        assert!(!s.layouts.contains_key("/x"));
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

    fn mode(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn save_writes_desktop_json_owner_only_and_leaves_no_temp_file() {
        let dir = std::env::temp_dir().join(format!("pocket-store-private-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Store::load(&dir);
        s.projects.push("/w".into());
        s.save();
        assert_eq!(mode(&dir.join("desktop.json")), 0o600);
        assert!(!dir.join("desktop.json.tmp").exists());
        assert_eq!(Store::load(&dir).projects, ["/w"]);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn save_makes_a_shared_desktop_json_and_a_stale_temp_file_owner_only() {
        use std::os::unix::fs::PermissionsExt;
        let dir = std::env::temp_dir().join(format!("pocket-store-shared-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        for name in ["desktop.json", "desktop.json.tmp"] {
            std::fs::write(dir.join(name), "{}").unwrap();
            std::fs::set_permissions(dir.join(name), std::fs::Permissions::from_mode(0o644)).unwrap();
        }
        Store::load(&dir).save();
        assert_eq!(mode(&dir.join("desktop.json")), 0o600);
        assert!(!dir.join("desktop.json.tmp").exists());
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_old_desktop_json_loads_with_empty_launch_picks() {
        let old: Store = serde_json::from_str(r#"{"projects":["/w"],"repos":{"/w":{"name":"w","color":0,"base":"main","worktrees":"","setup":"make","copy":[".env"]}}}"#).unwrap();
        assert_eq!(old.repos["/w"].launch, LaunchPick::default());
        let picked = RepoConfig { launch: LaunchPick { provider: "codex".into(), ..Default::default() }, ..Default::default() };
        assert!(serde_json::to_string(&picked).unwrap().contains(r#""launch":{"provider":"codex","model":"","effort":""}"#));
    }

    #[test]
    fn a_launch_pick_saved_before_model_and_effort_loads_with_them_empty() {
        let old: RepoConfig = serde_json::from_str(r#"{"launch":{"provider":"codex","access":"auto"}}"#).unwrap();
        assert_eq!(old.launch, LaunchPick { provider: "codex".into(), model: String::new(), effort: String::new() });
    }
}
