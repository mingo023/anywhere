//! The projects, their settings, tracked worktrees and shared settings a pocketd owns, from `registry.list`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// Everything a pocketd keeps that its clients share, sent whole.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct List {
    pub projects: Vec<Project>,
    pub tracked: Vec<String>,
    pub settings: Shared,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Project {
    pub id: String,
    pub path: String,
    /// The desktop's `RepoConfig` as pocketd keeps it.
    pub repo: Value,
}

/// The settings pocketd reads; `settings.shared` replaces them whole.
#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Shared {
    pub git: SharedGit,
    pub access: String,
    pub worktree_root: String,
}

#[derive(Serialize, Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct SharedGit {
    pub remote: String,
    /// `None` is on.
    pub commit_all: Option<bool>,
    pub force_with_lease: bool,
}

pub(super) fn decode(kind: &str, raw: &str) -> Option<List> {
    (kind == "registry.list").then(|| serde_json::from_str(raw).ok()).flatten()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_registry_list_decodes_its_projects_tracked_worktrees_and_settings() {
        let raw = r#"{"type":"registry.list",
            "projects":[{"id":"p1a2b3c4d5e6f7a8","path":"/Users/me/app",
              "repo":{"name":"","color":0,"setup":"","teardown":"","launch":{"provider":"claude","model":"opus","effort":"","access":"full","account":""},
                      "dev_url":"","icon":null,"gh_account":"","accounts":{},"hidden":[]}}],
            "tracked":["/Users/me/.anywhere/worktrees/app/cute-boa"],
            "settings":{"git":{"remote":"","commit_all":null,"force_with_lease":false},"access":"","worktree_root":"","external_worktrees":false}}"#;
        let list = decode("registry.list", raw).unwrap();
        assert_eq!((list.projects[0].id.as_str(), list.projects[0].path.as_str()), ("p1a2b3c4d5e6f7a8", "/Users/me/app"));
        assert_eq!(list.projects[0].repo["launch"]["model"], "opus");
        assert_eq!(list.tracked, ["/Users/me/.anywhere/worktrees/app/cute-boa"]);
        assert_eq!(list.settings.git.commit_all, None);
        assert!(decode("project.list", raw).is_none());
    }
}
