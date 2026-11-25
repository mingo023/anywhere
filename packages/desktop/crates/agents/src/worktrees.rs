//! The worktrees a pocketd lists under each project, from `project.list`, and its answers about deleting one.

use crate::Event;
use crate::ids::{DELETE, INSPECT};
use serde::Deserialize;

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Project {
    pub path: String,
    pub worktrees: Vec<Tree>,
}

/// A worktree the sidebar lists: the project's main one, one Anywhere made for a person, or one imported.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Tree {
    pub path: String,
    pub branch: String,
    pub is_main: bool,
    /// When its folder was made, in Unix milliseconds.
    pub created_at: Option<u64>,
}

/// What deleting a worktree takes: the worktree first, then those its sessions' subagents worked in.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default, rename_all = "camelCase")]
pub struct Inspected {
    pub trees: Vec<InspectedTree>,
    /// Commits lost when every branch goes with its worktree.
    pub lost: usize,
    /// Commits only the subagents' detached HEADs hold, lost either way.
    pub lost_anyway: usize,
}

#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct InspectedTree {
    /// "" when detached.
    pub branch: String,
    pub dirty: usize,
    pub terminals: usize,
}

/// pocketd's refusal of a delete; `path` names the worktree whose teardown failed.
#[derive(Deserialize, Default, Clone, Debug, PartialEq)]
#[serde(default)]
pub struct Refusal {
    pub code: String,
    pub message: String,
    pub detail: String,
    pub path: String,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Frame {
    id: String,
    projects: Vec<Project>,
    message: String,
}

pub(super) fn decode(kind: &str, raw: &str) -> Option<Event> {
    if !matches!(kind, "project.list" | "worktree.inspect" | "ack" | "error") {
        return None;
    }
    let f: Frame = serde_json::from_str(raw).ok()?;
    let request = f.id.clone();
    match kind {
        "project.list" => Some(Event::Projects(f.projects)),
        "worktree.inspect" => Some(Event::WorktreeInspected { request, reply: serde_json::from_str(raw).map_err(|e| e.to_string()) }),
        "error" if f.id.starts_with(INSPECT) => Some(Event::WorktreeInspected { request, reply: Err(f.message) }),
        "ack" if f.id.starts_with(DELETE) => Some(Event::WorktreeDeleted { request, reply: Ok(()) }),
        "error" if f.id.starts_with(DELETE) => Some(Event::WorktreeDeleted { request, reply: Err(serde_json::from_str(raw).unwrap_or_default()) }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_project_list_decodes_each_projects_worktrees_with_when_their_folders_were_made() {
        let raw = r#"{"type":"project.list","projects":[{"id":"p1","path":"/r/app","name":"app","worktrees":[
            {"name":"app","path":"/r/app","branch":"main","isMain":true,"createdAt":1764000000000},
            {"name":"fix-x","path":"/wt/app/fix-x","branch":"","isMain":false}]}]}"#;
        let Some(Event::Projects(projects)) = decode("project.list", raw) else { panic!("no projects") };
        let tree = |path: &str, branch: &str, is_main, created_at| Tree { path: path.into(), branch: branch.into(), is_main, created_at };
        assert_eq!(
            projects,
            [Project { path: "/r/app".into(), worktrees: vec![tree("/r/app", "main", true, Some(1764000000000)), tree("/wt/app/fix-x", "", false, None)] }]
        );
    }

    #[test]
    fn an_inspect_reply_decodes_what_the_delete_takes() {
        let raw = golden_text!("server/worktree_inspect.json").replace(r#""id": "wi""#, r#""id": "wtinspect-1""#);
        let Some(Event::WorktreeInspected { request, reply: Ok(got) }) = decode("worktree.inspect", &raw) else { panic!("no inspect") };
        let tree = |branch: &str, dirty, terminals| InspectedTree { branch: branch.into(), dirty, terminals };
        assert_eq!(request, "wtinspect-1");
        assert_eq!(got, Inspected { trees: vec![tree("calm-otter", 2, 1), tree("", 1, 0)], lost: 3, lost_anyway: 1 });
    }

    #[test]
    fn a_delete_answers_with_an_ack_or_its_refusal_and_other_requests_replies_pass_by() {
        let raw = golden_text!("server/error_teardown_failed.json").replace(r#""id": "wd""#, r#""id": "wtdelete-2""#);
        let Some(Event::WorktreeDeleted { request, reply: Err(refusal) }) = decode("error", &raw) else { panic!("no refusal") };
        assert_eq!((request.as_str(), refusal.code.as_str(), refusal.detail.as_str()), ("wtdelete-2", "teardown_failed", "make: *** [down] Error 1"));
        assert_eq!(refusal.path, "/Users/me/.worktrees/pocket/calm-otter");
        assert!(matches!(decode("ack", r#"{"type":"ack","id":"wtdelete-3"}"#), Some(Event::WorktreeDeleted { reply: Ok(()), .. })));
        let refused = decode(
            "error",
            r#"{"type":"error","id":"wtinspect-4","code":"host_outdated","message":"Can't delete worktrees on devbox: Anywhere there is out of date. Update it on devbox, then try again."}"#,
        );
        assert!(
            matches!(refused, Some(Event::WorktreeInspected { reply: Err(m), .. }) if m == "Can't delete worktrees on devbox: Anywhere there is out of date. Update it on devbox, then try again.")
        );
        assert!(decode("ack", r#"{"type":"ack","id":"registry-5"}"#).is_none());
        assert!(decode("error", r#"{"type":"error","id":"create-6","message":"no"}"#).is_none());
    }
}
