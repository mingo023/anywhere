use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use workspace::Workspace;

pub mod prefs;

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
    /// What its last session started with; `agent` wins where it names a provider.
    pub launch: LaunchPick,
    /// The agent, model and effort set for this project; empty uses the app's.
    pub agent: LaunchPick,
    /// The dev server the browser's home page opens for this project.
    pub dev_url: String,
}

/// The agent a Project's next session starts with.
#[derive(Serialize, Deserialize, Default, Debug, PartialEq, Clone)]
#[serde(default)]
pub struct LaunchPick {
    pub provider: String,
    pub model: String,
    pub effort: String,
    /// One of `prefs::agents::ACCESSES`; empty uses the app's.
    pub access: String,
}

impl LaunchPick {
    /// The agents a session can start.
    pub const PROVIDERS: [&str; 2] = ["claude", "codex"];

    /// The agent a remembered pick names; anything else starts Claude.
    pub fn known_provider(provider: &str) -> &'static str {
        if provider == "codex" { "codex" } else { "claude" }
    }

    /// This pick with its agent one this app knows.
    pub fn known(self) -> Self {
        Self { provider: Self::known_provider(&self.provider).into(), ..self }
    }

    /// A model or effort remembered for one agent means nothing to another; access holds for any.
    pub fn switched(&self, provider: &str) -> Self {
        if self.provider == provider {
            return Self { provider: provider.into(), ..self.clone() };
        }
        Self { provider: provider.into(), access: self.access.clone(), ..Self::default() }
    }

    /// A create's agent, model, effort, access and `prompt` as given.
    pub fn spec(&self, project: &str, checkout: serde_json::Value, prompt: &str) -> serde_json::Value {
        let mut spec = serde_json::json!({"project": project, "checkout": checkout, "provider": self.provider, "plan": false});
        for (key, value) in [("model", &self.model), ("effort", &self.effort), ("access", &self.access)] {
            if !value.is_empty() {
                spec[key] = value.as_str().into();
            }
        }
        if !prompt.is_empty() {
            spec["prompt"] = prompt.into();
        }
        spec
    }
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
    #[serde(deserialize_with = "lenient")]
    pub needs_you_tone: Tone,
    #[serde(deserialize_with = "lenient")]
    pub done_tone: Tone,
    #[serde(deserialize_with = "lenient")]
    pub failed_tone: Tone,
    /// Percent of full volume.
    pub volume: u32,
}

impl Default for Sounds {
    fn default() -> Self {
        Self { all: true, needs_you: true, done: true, failed: true, needs_you_tone: Tone::Anywhere, done_tone: Tone::Anywhere, failed_tone: Tone::Anywhere, volume: 100 }
    }
}

/// A cue's sound: the app's own for that cue, or one of macOS's.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
pub enum Tone {
    #[default]
    Anywhere,
    Basso,
    Blow,
    Bottle,
    Frog,
    Funk,
    Glass,
    Hero,
    Morse,
    Ping,
    Pop,
    Purr,
    Sosumi,
    Submarine,
    Tink,
}

impl Tone {
    pub const ALL: [Tone; 15] = [
        Tone::Anywhere,
        Tone::Basso,
        Tone::Blow,
        Tone::Bottle,
        Tone::Frog,
        Tone::Funk,
        Tone::Glass,
        Tone::Hero,
        Tone::Morse,
        Tone::Ping,
        Tone::Pop,
        Tone::Purr,
        Tone::Sosumi,
        Tone::Submarine,
        Tone::Tink,
    ];

    /// Where macOS keeps it; `None` for the app's own.
    pub fn system_path(self) -> Option<String> {
        (self != Tone::Anywhere).then(|| format!("/System/Library/Sounds/{self:?}.aiff"))
    }
}

#[derive(Serialize, Deserialize, Debug, PartialEq, Clone, Copy)]
#[serde(default)]
pub struct Notifications {
    /// macOS banners for sessions that need you or finish.
    pub banners: bool,
    pub needs_you: bool,
    pub failed: bool,
    pub done: bool,
    /// Banners and sounds for sessions in a pane on screen too.
    pub on_screen: bool,
    #[serde(deserialize_with = "lenient")]
    pub badge: DockBadge,
}

impl Default for Notifications {
    fn default() -> Self {
        Self { banners: true, needs_you: true, failed: true, done: true, on_screen: false, badge: DockBadge::Inbox }
    }
}

/// What the number on the Dock icon counts.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum DockBadge {
    Off,
    NeedsYou,
    /// Everything in the inbox: sessions that need you, and failed or done ones not yet seen.
    #[default]
    Inbox,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum Mode {
    #[default]
    System,
    Light,
    Dark,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Default)]
#[serde(default)]
pub struct Appearance {
    #[serde(deserialize_with = "lenient")]
    pub mode: Mode,
    /// `None` follows the macOS setting.
    pub reduce_motion: Option<bool>,
    #[serde(deserialize_with = "lenient")]
    pub diff_colors: DiffColors,
    #[serde(deserialize_with = "lenient")]
    pub syntax: SyntaxTheme,
    /// Custom diff colors as `0xRRGGBB`; `None` keeps the theme's.
    pub added: Option<u32>,
    pub removed: Option<u32>,
    /// `0xRRGGBB`; `None` is Graphite.
    #[serde(deserialize_with = "lenient")]
    pub accent: Option<u32>,
    /// Font families; `None` is the app's own.
    #[serde(deserialize_with = "lenient")]
    pub ui_font: Option<String>,
    #[serde(deserialize_with = "lenient")]
    pub code_font: Option<String>,
    /// Points; read it through `code_size()`.
    #[serde(deserialize_with = "lenient")]
    pub code_size: Option<u32>,
    /// Percent the whole window is drawn at; read it through `zoom()`.
    #[serde(deserialize_with = "lenient")]
    pub zoom: Option<u32>,
}

impl Appearance {
    pub const CODE_SIZES: (u32, u32) = (10, 20);
    /// Safari's steps, in percent.
    pub const ZOOMS: [u32; 11] = [50, 67, 75, 80, 90, 100, 110, 125, 150, 175, 200];

    /// The window's zoom: 100% unless set, kept to the steps' range.
    pub fn zoom(&self) -> u32 {
        self.zoom.unwrap_or(100).clamp(Self::ZOOMS[0], Self::ZOOMS[Self::ZOOMS.len() - 1])
    }

    /// `zoom()` as a scale, 1 at 100%.
    pub fn zoom_factor(&self) -> f32 {
        self.zoom() as f32 / 100.
    }

    /// The step after the current zoom, or the last.
    pub fn zoomed_in(&self) -> u32 {
        Self::ZOOMS.into_iter().find(|&z| z > self.zoom()).unwrap_or(self.zoom())
    }

    /// The step before the current zoom, or the first.
    pub fn zoomed_out(&self) -> u32 {
        Self::ZOOMS.into_iter().rev().find(|&z| z < self.zoom()).unwrap_or(self.zoom())
    }

    /// The diff and preview text size: 12pt unless set, kept in range.
    pub fn code_size(&self) -> u32 {
        self.code_size.unwrap_or(12).clamp(Self::CODE_SIZES.0, Self::CODE_SIZES.1)
    }
}

/// Code colors, in the order of `theme::SYNTAX_THEMES`.
#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum SyntaxTheme {
    /// Matches the app.
    #[default]
    Graphite,
    Github,
    One,
    Solarized,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Default)]
#[serde(rename_all = "camelCase")]
pub enum DiffColors {
    #[default]
    GreenRed,
    /// Easier to tell apart with red-green colour blindness.
    BlueOrange,
}

/// Where new worktrees and clones go when a project doesn't say, and what deleting a worktree takes with it.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct WorktreeDefaults {
    /// Empty means `~/.worktrees`; each project gets a folder in it.
    pub root: String,
    /// Empty means `~/code`.
    pub clone_root: String,
    /// Whether the delete sheet's "also delete its branch" starts ticked.
    pub delete_branch: bool,
    /// Moves a deleted worktree's files to the Trash instead of deleting them.
    pub trash: bool,
    pub teardown_secs: u32,
}

impl Default for WorktreeDefaults {
    fn default() -> Self {
        Self { root: String::new(), clone_root: String::new(), delete_branch: true, trash: false, teardown_secs: 120 }
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
    /// The Changes graph's dragged share; `None` splits evenly.
    pub graph_share: Option<f32>,
    pub sounds: Sounds,
    pub notifications: Notifications,
    pub appearance: Appearance,
    pub worktree: WorktreeDefaults,
    /// The worktrees projects list besides their main ones, by path.
    pub tracked: BTreeSet<String>,
    /// Each worktree's panels, keyed by the worktree's path.
    #[serde(deserialize_with = "readable_layouts")]
    pub layouts: BTreeMap<String, Workspace>,
    /// The app version that last launched, so a relaunch on another one says what's new.
    pub seen_version: Option<String>,
    /// Settings sections showing their advanced rows, by id.
    #[serde(deserialize_with = "lenient")]
    pub advanced: BTreeSet<String>,
    #[serde(deserialize_with = "lenient")]
    pub general: prefs::General,
    #[serde(deserialize_with = "lenient")]
    pub agents: prefs::Agents,
    #[serde(deserialize_with = "lenient")]
    pub automations: prefs::Automations,
    #[serde(deserialize_with = "lenient")]
    pub phone: prefs::Phone,
    #[serde(deserialize_with = "lenient")]
    pub sidebar: prefs::Sidebar,
    #[serde(deserialize_with = "lenient")]
    pub terminal: prefs::Terminal,
    #[serde(deserialize_with = "lenient")]
    pub files: prefs::Files,
    #[serde(deserialize_with = "lenient")]
    pub browser: prefs::Browser,
    #[serde(deserialize_with = "lenient")]
    pub git: prefs::Git,
    #[serde(deserialize_with = "lenient")]
    pub diff: prefs::Diff,
    /// Shortcuts changed on the Keyboard page: the new keys by binding id, empty for none.
    #[serde(deserialize_with = "lenient")]
    pub keys: BTreeMap<String, String>,
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

    /// The folder Pocket keeps its files in.
    pub fn home(&self) -> &Path {
        self.path.parent().unwrap_or(&self.path)
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

    pub fn track(&mut self, tree: &str) {
        self.tracked.insert(tree.to_string());
    }

    pub fn untrack(&mut self, tree: &str) {
        self.tracked.remove(tree);
    }

    pub fn is_tracked(&self, tree: &str) -> bool {
        self.tracked.contains(tree)
    }

    /// Whether the sidebar lists `tree`: one the app made or imported, or any while it lists those made elsewhere.
    pub fn lists(&self, tree: &str) -> bool {
        self.sidebar.external_worktrees || self.is_tracked(tree)
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

    /// The folder Add project › Clone starts in: the chosen one, else `~/code`.
    pub fn clone_root(&self, home: &str) -> String {
        if self.worktree.clone_root.is_empty() { format!("{home}/code") } else { self.worktree.clone_root.trim_end_matches('/').to_string() }
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

/// Reads a value this build doesn't understand as its default, since `Store::load` forgets everything on any error.
pub(crate) fn lenient<'de, D: serde::Deserializer<'de>, T: serde::de::DeserializeOwned + Default>(d: D) -> Result<T, D::Error> {
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
    fn the_last_agent_keeps_its_model_and_effort() {
        let pick = LaunchPick { provider: "claude".into(), model: "opus".into(), effort: "high".into(), access: String::new() };
        assert_eq!(pick.switched("claude"), pick);
    }

    #[test]
    fn another_agent_forgets_the_last_ones_model_and_effort_but_keeps_its_access() {
        let pick = LaunchPick { provider: "claude".into(), model: "opus".into(), effort: "high".into(), access: "auto".into() };
        assert_eq!(pick.switched("codex"), LaunchPick { provider: "codex".into(), access: "auto".into(), ..LaunchPick::default() });
    }

    #[test]
    fn an_agent_this_app_does_not_know_starts_claude() {
        let pick = LaunchPick { provider: "gemini".into(), model: "pro".into(), effort: String::new(), access: String::new() };
        assert_eq!(pick.known(), LaunchPick { provider: "claude".into(), model: "pro".into(), effort: String::new(), access: String::new() });
        assert_eq!(LaunchPick::known_provider("codex"), "codex");
    }

    #[test]
    fn a_launch_sends_only_the_model_effort_access_and_prompt_it_has() {
        let pick = LaunchPick { provider: "codex".into(), model: "gpt-5".into(), access: "edits".into(), ..LaunchPick::default() };
        let spec = pick.spec("/p", serde_json::json!({"worktree": "/p"}), "Fix the failing tests");
        assert_eq!(
            spec,
            serde_json::json!({"project": "/p", "checkout": {"worktree": "/p"}, "provider": "codex", "access": "edits", "plan": false, "model": "gpt-5", "prompt": "Fix the failing tests"})
        );
        assert_eq!(pick.spec("/p", serde_json::json!({"worktree": "/p"}), "").get("prompt"), None);
    }

    #[test]
    fn round_trips_through_desktop_json() {
        let dir = std::env::temp_dir().join(format!("pocket-store-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Store::load(&dir);
        s.projects.push("/w".into());
        s.collapsed.insert("/w".into());
        let launch = LaunchPick { provider: "claude".into(), model: "opus".into(), effort: "high".into(), access: "ask".into() };
        s.repos.insert("/w".into(), RepoConfig { name: "w".into(), color: 0xd97757ff, copy: vec![".env".into()], launch, ..Default::default() });
        s.window = Some(WindowGeometry { display: Some("D1".into()), x: 40., y: 60., width: 1200., height: 800. });
        s.layout = Layout::Compact;
        s.widths = ColumnWidths { projects: Some(260.), sessions: None };
        s.graph_share = Some(0.3);
        s.track("/t/fix");
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
        assert!(s.layouts.is_empty() && s.tracked.is_empty());
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
        assert_eq!((&old.repos["/w"].launch, &old.repos["/w"].agent), (&LaunchPick::default(), &LaunchPick::default()));
        let picked = RepoConfig { launch: LaunchPick { provider: "codex".into(), ..Default::default() }, ..Default::default() };
        assert!(serde_json::to_string(&picked).unwrap().contains(r#""launch":{"provider":"codex","model":"","effort":"","access":""}"#));
    }

    #[test]
    fn a_launch_pick_saved_before_model_and_effort_loads_with_them_empty() {
        let old: RepoConfig = serde_json::from_str(r#"{"launch":{"provider":"codex","access":"auto"}}"#).unwrap();
        assert_eq!(old.launch, LaunchPick { provider: "codex".into(), access: "auto".into(), ..LaunchPick::default() });
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
    fn an_unknown_syntax_theme_falls_back_to_graphite_and_keeps_the_rest() {
        let s: Store = serde_json::from_str(r#"{"appearance":{"mode":"dark","syntax":"dracula","added":9479878}}"#).unwrap();
        assert_eq!((s.appearance.syntax, s.appearance.mode, s.appearance.added), (SyntaxTheme::Graphite, Mode::Dark, Some(0x90a6c6)));
    }

    #[test]
    fn appearance_fields_of_the_wrong_kind_fall_back_and_code_size_stays_in_range() {
        let s: Store = serde_json::from_str(r#"{"appearance":{"accent":"blue","ui_font":3,"code_font":"Menlo","code_size":99}}"#).unwrap();
        assert_eq!((s.appearance.accent, s.appearance.ui_font.as_deref(), s.appearance.code_font.as_deref(), s.appearance.code_size()), (None, None, Some("Menlo"), 20));
        assert_eq!([Appearance::default().code_size(), Appearance { code_size: Some(3), ..Appearance::default() }.code_size()], [12, 10]);
    }

    #[test]
    fn settings_round_trip_through_desktop_json() {
        let dir = std::env::temp_dir().join(format!("pocket-store-settings-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut s = Store::load(&dir);
        s.notifications.banners = false;
        s.appearance = Appearance {
            mode: Mode::Dark,
            reduce_motion: Some(true),
            diff_colors: DiffColors::BlueOrange,
            syntax: SyntaxTheme::Solarized,
            added: Some(0x8e4ec6),
            removed: None,
            accent: Some(0x3e8ef7),
            ui_font: Some("Inter".into()),
            code_font: None,
            code_size: Some(14),
            zoom: Some(125),
        };
        s.sounds.all = false;
        s.worktree.root = "/wt".into();
        (s.agents.close_on_exit, s.agents.chat_to) = (true, "ask".into());
        s.agents.move_starter(0, 3);
        s.keys = BTreeMap::from([("desktop::OpenPalette".into(), "cmd-shift-p".into()), ("desktop::Reload in Browser".into(), String::new())]);
        s.save();
        assert_eq!(Store::load(&dir), s);
        std::fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn zoom_steps_through_the_list_and_stops_at_its_ends() {
        let at = |zoom| Appearance { zoom: Some(zoom), ..Appearance::default() };
        assert_eq!((Appearance::default().zoom(), Appearance::default().zoomed_in(), Appearance::default().zoomed_out()), (100, 110, 90));
        assert_eq!((at(200).zoomed_in(), at(50).zoomed_out()), (200, 50));
        assert_eq!((at(130).zoomed_in(), at(130).zoomed_out()), (150, 125));
        assert_eq!((at(999).zoom(), at(1).zoom()), (200, 50));
    }

    #[test]
    fn an_unknown_appearance_mode_falls_back_to_system_and_keeps_projects() {
        let s: Store = serde_json::from_str(r#"{"projects":["/w"],"appearance":{"mode":"sepia","reduce_motion":false}}"#).unwrap();
        assert_eq!((s.projects.len(), s.appearance), (1, Appearance { mode: Mode::System, reduce_motion: Some(false), ..Appearance::default() }));
        let s: Store = serde_json::from_str(r#"{"appearance":{"mode":3}}"#).unwrap();
        assert_eq!(s.appearance.mode, Mode::System);
    }

    #[test]
    fn a_desktop_json_from_before_tones_and_badges_keeps_todays_sounds_and_badge() {
        let s: Store = serde_json::from_str(r#"{"sounds":{"done":false},"notifications":{"banners":false}}"#).unwrap();
        assert_eq!((s.sounds.done_tone, s.sounds.volume, s.notifications.banners, s.notifications.done, s.notifications.badge), (Tone::Anywhere, 100, false, true, DockBadge::Inbox));
        let s: Store = serde_json::from_str(r#"{"projects":["/w"],"sounds":{"done_tone":"Kazoo"},"notifications":{"badge":"everything"}}"#).unwrap();
        assert_eq!((s.projects.len(), s.sounds.done_tone, s.notifications.badge), (1, Tone::Anywhere, DockBadge::Inbox));
    }

    #[test]
    fn a_macos_tone_plays_from_the_system_sounds_and_the_apps_own_from_its_bundle() {
        assert_eq!(Tone::Glass.system_path().as_deref(), Some("/System/Library/Sounds/Glass.aiff"));
        assert_eq!(Tone::Anywhere.system_path(), None);
    }

    #[test]
    fn a_tracked_worktree_stays_tracked_until_untracked() {
        let mut s = Store::default();
        s.track("/t/fix");
        s.track("/t/spike");
        s.untrack("/t/fix");
        assert!(!s.is_tracked("/t/fix") && s.is_tracked("/t/spike"));
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

    #[test]
    fn worktrees_made_elsewhere_are_listed_only_when_asked() {
        let mut s = Store::default();
        s.track("/w/mine");
        assert_eq!([s.lists("/w/mine"), s.lists("/w/theirs")], [true, false]);
        s.sidebar.external_worktrees = true;
        assert!(s.lists("/w/theirs"));
    }

    #[test]
    fn worktree_defaults_from_before_deletion_settings_keep_todays_deletes() {
        let s: Store = serde_json::from_str(r#"{"worktree":{"root":"/wt"}}"#).unwrap();
        assert_eq!(s.worktree, WorktreeDefaults { root: "/wt".into(), ..WorktreeDefaults::default() });
        assert_eq!((s.worktree.delete_branch, s.worktree.trash, s.worktree.teardown_secs), (true, false, 120));
        assert_eq!(s.clone_root("/h"), "/h/code");
    }

    #[test]
    fn a_desktop_json_from_before_updates_has_seen_no_version() {
        let s: Store = serde_json::from_str(r#"{"projects":["/w"]}"#).unwrap();
        assert_eq!(s.seen_version, None);
    }
}
