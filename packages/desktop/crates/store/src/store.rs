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
    pub teardown: String,
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
    /// Mutes every cue without forgetting which ones are on.
    pub all: bool,
    pub needs_you: bool,
    pub done: bool,
    pub failed: bool,
}

impl Default for Sounds {
    fn default() -> Self {
        Self { all: true, needs_you: true, done: true, failed: true }
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(default)]
pub struct Notifications {
    /// macOS banners for sessions that need you or finish.
    pub banners: bool,
}

impl Default for Notifications {
    fn default() -> Self {
        Self { banners: true }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Appearance {
    #[serde(deserialize_with = "lenient_mode")]
    pub mode: Mode,
    /// `None` follows the macOS setting.
    pub reduce_motion: Option<bool>,
}

/// Where new worktrees go when a project doesn't say.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct WorktreeDefaults {
    /// Empty means `~/.worktrees`; each project gets a folder in it.
    pub root: String,
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
    pub notifications: Notifications,
    pub appearance: Appearance,
    pub worktree: WorktreeDefaults,
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

    /// The folder `repo`'s new worktrees go in: its own setting, else a folder named for it under the root, else under `~/.worktrees`.
    pub fn worktrees_dir(&self, repo: &str, home: &str) -> String {
        if let Some(dir) = self.repos.get(repo).map(|r| r.worktrees.as_str()).filter(|w| !w.is_empty()) {
            return dir.to_string();
        }
        format!("{}/{}", self.worktree_root(home), self.repo_name(repo))
    }

    /// The name `repo` was given, else its folder's.
    pub fn repo_name(&self, repo: &str) -> String {
        let named = self.repos.get(repo).map(|r| r.name.clone()).filter(|n| !n.is_empty());
        named.unwrap_or_else(|| repo.trim_end_matches('/').rsplit('/').next().unwrap_or_default().to_string())
    }

    /// The folder new worktrees go under, one subfolder per project: the chosen root, else `~/.worktrees`.
    pub fn worktree_root(&self, home: &str) -> String {
        if self.worktree.root.is_empty() { format!("{home}/.worktrees") } else { self.worktree.root.trim_end_matches('/').to_string() }
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

/// Reads a mode this build doesn't know as `System`, since `Store::load` forgets everything on any error.
fn lenient_mode<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Mode, D::Error> {
    Ok(serde_json::from_value(serde_json::Value::deserialize(d)?).unwrap_or_default())
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
        assert_eq!(Store::load(&dir).sounds, Sounds::default());
        std::fs::write(dir.join("desktop.json"), r#"{"sounds":{"done":false}}"#).unwrap();
        assert_eq!(Store::load(&dir).sounds, Sounds { done: false, ..Sounds::default() });
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

    #[test]
    fn a_teardown_round_trips_and_an_old_desktop_json_loads_without_one() {
        let old: RepoConfig = serde_json::from_str(r#"{"name":"w","setup":"make"}"#).unwrap();
        assert_eq!(old.teardown, "");
        let cfg = RepoConfig { teardown: "docker compose down".into(), ..Default::default() };
        let back: RepoConfig = serde_json::from_str(&serde_json::to_string(&cfg).unwrap()).unwrap();
        assert_eq!(back, cfg);
    }

    #[test]
    fn a_desktop_json_from_before_settings_loads_with_defaults() {
        let s: Store = serde_json::from_str(r#"{"projects":["/w"],"sounds":{"done":false}}"#).unwrap();
        assert_eq!((s.notifications.banners, s.appearance, &s.worktree.root), (true, Appearance::default(), &String::new()));
        assert_eq!((s.sounds.all, s.sounds.done), (true, false));
    }

    #[test]
    fn settings_round_trip_through_desktop_json() {
        let dir = std::env::temp_dir().join(format!("pocket-store-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Store::load(&dir);
        s.notifications.banners = false;
        s.appearance = Appearance { mode: Mode::Dark, reduce_motion: Some(true) };
        s.sounds.all = false;
        s.worktree.root = "/wt".into();
        s.save();
        assert_eq!(Store::load(&dir), s);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn an_unknown_appearance_mode_falls_back_to_system_and_keeps_projects() {
        let s: Store = serde_json::from_str(r#"{"projects":["/w"],"appearance":{"mode":"sepia","reduce_motion":false}}"#).unwrap();
        assert_eq!((s.projects.len(), s.appearance), (1, Appearance { mode: Mode::System, reduce_motion: Some(false) }));
        let s: Store = serde_json::from_str(r#"{"appearance":{"mode":3}}"#).unwrap();
        assert_eq!(s.appearance.mode, Mode::System);
    }

    #[test]
    fn worktrees_dir_prefers_the_repo_then_the_root_then_home() {
        let mut s = Store::default();
        s.repos.insert("/w/own".into(), RepoConfig { worktrees: "/elsewhere".into(), ..Default::default() });
        s.repos.insert("/w/named".into(), RepoConfig { name: "Named".into(), ..Default::default() });
        assert_eq!(["/w/own", "/w/named", "/w/plain"].map(|r| s.worktrees_dir(r, "/h")), ["/elsewhere", "/h/.worktrees/Named", "/h/.worktrees/plain"]);
        s.worktree.root = "/wt/".into();
        assert_eq!(["/w/own", "/w/named", "/w/plain"].map(|r| s.worktrees_dir(r, "/h")), ["/elsewhere", "/wt/Named", "/wt/plain"]);
    }
}
