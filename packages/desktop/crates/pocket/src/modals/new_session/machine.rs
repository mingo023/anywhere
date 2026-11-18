//! The composer's machine pill: the machine is the space on screen, so what a draft does is checked against that space.

use crate::spaces::{Reach, Space, space_of, twin};

/// Why the draft can't start a session on the space on screen.
#[derive(Debug, PartialEq)]
pub(crate) enum Blocker {
    /// The space's host can't be reached.
    Unreachable(Reach),
    /// The project, or with none a new Scratch folder on this Mac, isn't on the space.
    Elsewhere,
}

/// What stops a draft on `repo` starting in `space`; an unreachable machine first, as no project helps there.
pub(crate) fn blocker(repo: Option<&str>, space: &Space, reach: Reach) -> Option<Blocker> {
    if reach != Reach::Online {
        return Some(Blocker::Unreachable(reach));
    }
    (repo.map_or(Space::Local, space_of) != *space).then_some(Blocker::Elsewhere)
}

/// Where a draft goes when the space changes.
#[derive(Debug, PartialEq)]
pub(crate) enum Landing {
    Project(String),
    NoProject,
    Stay,
}

/// The draft's project in `to` if it has a twin there, else the space's first project, else No project where that is allowed (This Mac only).
pub(crate) fn landing(drafted: Option<&str>, to: &Space, projects: &[String]) -> Landing {
    if drafted.map_or(Space::Local, space_of) == *to {
        return Landing::Stay;
    }
    let first = || projects.iter().find(|p| space_of(p) == *to);
    match drafted.and_then(|p| twin(p, to, projects)).or_else(first) {
        Some(p) => Landing::Project(p.clone()),
        None if *to == Space::Local => Landing::NoProject,
        None => Landing::Stay,
    }
}

/// The notice over an unreachable machine's composer.
pub(crate) fn unreachable_notice(reach: Reach, name: &str) -> String {
    match reach {
        Reach::Connecting => format!("{name} is connecting. Sessions start once it is linked."),
        _ => format!("{name} is offline. Sessions start once it reconnects."),
    }
}

#[cfg(test)]
mod tests {
    use super::{Blocker, Landing, blocker, landing, unreachable_notice};
    use crate::spaces::{Reach, Space};

    fn projects() -> Vec<String> {
        ["/me/app-ios", "/me/app-android", "//devbox/code/api", "//devbox/code/app-ios", "//build/src/app-ios"].map(String::from).to_vec()
    }

    fn host(id: &str) -> Space {
        Space::Host(id.into())
    }

    #[test]
    fn switching_space_carries_the_draft_to_its_twin() {
        let projects = projects();
        assert_eq!(landing(Some("/me/app-ios"), &host("devbox"), &projects), Landing::Project("//devbox/code/app-ios".into()));
        assert_eq!(landing(Some("//build/src/app-ios"), &Space::Local, &projects), Landing::Project("/me/app-ios".into()));
    }

    #[test]
    fn switching_to_a_space_without_a_twin_picks_its_first_project() {
        assert_eq!(landing(Some("/me/app-android"), &host("devbox"), &projects()), Landing::Project("//devbox/code/api".into()));
    }

    #[test]
    fn a_space_with_no_projects_leaves_no_project_to_this_mac_and_keeps_the_draft_on_a_host() {
        let only_mac = ["/me/app-ios".to_string()];
        assert_eq!(landing(Some("//devbox/code/api"), &Space::Local, &[]), Landing::NoProject);
        assert_eq!(landing(Some("/me/app-ios"), &host("devbox"), &only_mac), Landing::Stay);
        assert_eq!(landing(None, &host("devbox"), &only_mac), Landing::Stay);
    }

    #[test]
    fn a_draft_already_in_the_space_stays_put() {
        let projects = projects();
        assert_eq!(landing(Some("//devbox/code/api"), &host("devbox"), &projects), Landing::Stay);
        assert_eq!(landing(None, &Space::Local, &projects), Landing::Stay);
    }

    #[test]
    fn a_project_the_space_lacks_blocks_the_send_and_no_project_belongs_to_this_mac() {
        assert_eq!(blocker(Some("/me/app-android"), &host("devbox"), Reach::Online), Some(Blocker::Elsewhere));
        assert_eq!(blocker(None, &host("devbox"), Reach::Online), Some(Blocker::Elsewhere));
        assert_eq!([blocker(Some("//devbox/code/api"), &host("devbox"), Reach::Online), blocker(None, &Space::Local, Reach::Online)], [None, None]);
    }

    #[test]
    fn an_unreachable_host_blocks_the_send_whatever_the_project_and_says_so() {
        assert_eq!(blocker(Some("//devbox/code/api"), &host("devbox"), Reach::Offline), Some(Blocker::Unreachable(Reach::Offline)));
        assert_eq!(blocker(Some("/me/app-ios"), &host("devbox"), Reach::Connecting), Some(Blocker::Unreachable(Reach::Connecting)));
        assert_eq!(unreachable_notice(Reach::Offline, "devbox"), "devbox is offline. Sessions start once it reconnects.");
        assert!(unreachable_notice(Reach::Connecting, "devbox").starts_with("devbox is connecting."));
    }
}
