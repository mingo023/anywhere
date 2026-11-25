//! The desktop's copy of what each pocketd owns: its projects and their settings, its tracked worktrees and, for this
//! Mac's, the shared settings. pocketd's `registry.list` replaces a machine's copy; a change made here goes to it.

use crate::{RepoConfig, Store};
use agents::registry::{List, Project, Shared, SharedGit};
use std::collections::{BTreeMap, BTreeSet};

/// What one machine's pocketd owns, as this desktop holds it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Slice {
    pub projects: BTreeMap<String, RepoConfig>,
    pub tracked: BTreeSet<String>,
    /// `None` for a host: only this Mac's pocketd keeps them.
    pub settings: Option<Shared>,
}

/// One message to the pocketd holding the path, which is the desktop's key for it.
#[derive(Clone, Debug, PartialEq)]
pub enum Change {
    Add(String, RepoConfig),
    Remove(String),
    Set(String, RepoConfig),
    Track(String, bool),
    Settings(Shared),
}

/// The messages that take a pocketd from `before` to `after`.
pub fn changes(before: &Slice, after: &Slice) -> Vec<Change> {
    let mut out = Vec::new();
    for (path, repo) in &after.projects {
        match before.projects.get(path) {
            None => out.push(Change::Add(path.clone(), repo.clone())),
            Some(was) if was != repo => out.push(Change::Set(path.clone(), repo.clone())),
            Some(_) => {}
        }
    }
    out.extend(before.projects.keys().filter(|p| !after.projects.contains_key(*p)).map(|p| Change::Remove(p.clone())));
    out.extend(after.tracked.difference(&before.tracked).map(|t| Change::Track(t.clone(), true)));
    out.extend(before.tracked.difference(&after.tracked).map(|t| Change::Track(t.clone(), false)));
    if let Some(settings) = after.settings.as_ref().filter(|s| before.settings.as_ref() != Some(*s)) {
        out.push(Change::Settings(settings.clone()));
    }
    out
}

/// `theirs` with each setting it leaves unset taken from `ours`.
fn filled(theirs: RepoConfig, ours: &RepoConfig) -> RepoConfig {
    let pick = |a: String, b: &String| if a.is_empty() { b.clone() } else { a };
    RepoConfig {
        name: pick(theirs.name, &ours.name),
        color: if theirs.color == 0 { ours.color } else { theirs.color },
        setup: pick(theirs.setup, &ours.setup),
        teardown: pick(theirs.teardown, &ours.teardown),
        launch: if theirs.launch == Default::default() { ours.launch.clone() } else { theirs.launch },
        dev_url: pick(theirs.dev_url, &ours.dev_url),
        icon: theirs.icon.or_else(|| ours.icon.clone()),
        gh_account: pick(theirs.gh_account, &ours.gh_account),
        accounts: if theirs.accounts.is_empty() { ours.accounts.clone() } else { theirs.accounts },
        hidden: if theirs.hidden.is_empty() { ours.hidden.clone() } else { theirs.hidden },
    }
}

/// Whether `key` is on `host`, or on this Mac for `None`.
fn on(host: Option<&str>, key: &str) -> bool {
    hosts::split(key).0 == host
}

impl Store {
    /// What `host`'s pocketd (this Mac's for `None`) owns, as this desktop holds it.
    pub fn registry(&self, host: Option<&str>) -> Slice {
        Slice {
            projects: self.projects.iter().filter(|p| on(host, p)).map(|p| (p.clone(), self.repos.get(p).cloned().unwrap_or_default())).collect(),
            tracked: self.tracked.iter().filter(|t| on(host, t)).cloned().collect(),
            settings: host.is_none().then(|| self.shared()),
        }
    }

    fn shared(&self) -> Shared {
        Shared {
            git: SharedGit { remote: self.git.remote.clone(), commit_all: Some(self.git.commit_all), force_with_lease: self.git.force_with_lease },
            access: self.agents.access.clone(),
            worktree_root: self.worktree.root.clone(),
            external_worktrees: self.sidebar.external_worktrees,
        }
    }

    /**
     * Takes `list`, from `host`'s pocketd (this Mac's for `None`), as that machine's projects, their settings, its
     * tracked worktrees and their ids, and from this Mac's the shared settings. Projects already listed keep their
     * place on the sidebar; new ones go after them in `list`'s order.
     */
    pub fn apply_registry(&mut self, host: Option<&str>, list: &List) {
        let gone: Vec<String> = self.projects.iter().filter(|p| on(host, p) && !list.projects.iter().any(|l| l.path == **p)).cloned().collect();
        for path in &gone {
            self.remove(path);
        }
        for p in &list.projects {
            self.add(&p.path);
            // One this build can't read keeps what was cached.
            match serde_json::from_value(p.repo.clone()) {
                Ok(repo) => _ = self.repos.insert(p.path.clone(), repo),
                Err(_) => _ = self.repos.entry(p.path.clone()).or_default(),
            }
        }
        self.tracked.retain(|t| !on(host, t));
        self.tracked.extend(list.tracked.iter().filter(|t| on(host, t)).cloned());
        self.ids.retain(|path, _| !on(host, path));
        self.ids.extend(list.projects.iter().filter(|p| !p.id.is_empty()).map(|p| (p.path.clone(), p.id.clone())));
        if host.is_none() {
            let s = &list.settings;
            self.git.remote = if s.git.remote.is_empty() { crate::prefs::Git::default().remote } else { s.git.remote.clone() };
            self.git.commit_all = s.git.commit_all.unwrap_or(true);
            self.git.force_with_lease = s.git.force_with_lease;
            self.agents.access = s.access.clone();
            self.worktree.root = s.worktree_root.clone();
            self.sidebar.external_worktrees = s.external_worktrees;
        }
    }

    /**
     * The first time `host` sends its list, the projects and tracked worktrees cached for it that `list` lacks: its
     * pocketd is sent them once. The projects join `list` so they keep their place while it takes them. Nothing after
     * that first time.
     */
    pub fn adopt_cached(&mut self, host: &str, list: &mut List) -> Vec<Change> {
        if !self.registry_pushed.insert(host.to_string()) {
            return Vec::new();
        }
        let cached = self.registry(Some(host));
        // A host's desktop.json may have held only some of a project's settings: the ones it lacks come from this desktop.
        let mut set = Vec::new();
        for listed in &mut list.projects {
            let (Some(cached), Ok(theirs)) = (cached.projects.get(&listed.path), serde_json::from_value::<RepoConfig>(listed.repo.clone())) else { continue };
            let repo = filled(theirs.clone(), cached);
            if repo != theirs {
                listed.repo = serde_json::to_value(&repo).unwrap_or_default();
                set.push(Change::Set(listed.path.clone(), repo));
            }
        }
        let projects: Vec<(String, RepoConfig)> = cached.projects.into_iter().filter(|(p, _)| !list.projects.iter().any(|l| l.path == *p)).collect();
        let tracked: Vec<String> = cached.tracked.into_iter().filter(|t| !list.tracked.contains(t)).collect();
        list.projects.extend(projects.iter().map(|(path, repo)| Project {
            id: String::new(),
            path: path.clone(),
            repo: serde_json::to_value(repo).unwrap_or_default(),
        }));
        list.tracked.extend(tracked.iter().cloned());
        projects.into_iter().map(|(path, repo)| Change::Add(path, repo)).chain(set).chain(tracked.into_iter().map(|t| Change::Track(t, true))).collect()
    }

    /// The project `host`'s pocketd (this Mac's for `None`) gave id `id`; each pocketd mints its own.
    pub fn project_with_id(&self, host: Option<&str>, id: &str) -> Option<&str> {
        self.ids.iter().find(|(path, i)| *i == id && on(host, path)).map(|(path, _)| path.as_str())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn project(id: &str, path: &str, repo: serde_json::Value) -> Project {
        Project { id: id.into(), path: path.into(), repo }
    }

    fn list(projects: &[(&str, &str)]) -> List {
        List { projects: projects.iter().map(|(id, path)| project(id, path, json!({}))).collect(), ..List::default() }
    }

    #[test]
    fn a_list_keeps_the_sidebar_order_drops_what_it_lacks_and_appends_what_is_new() {
        let mut s = Store { projects: ["/b", "/a", "/gone"].map(String::from).to_vec(), collapsed: ["/gone".to_string()].into(), ..Store::default() };
        s.apply_registry(None, &list(&[("p1", "/a"), ("p2", "/c"), ("p3", "/b"), ("p4", "/d")]));
        assert_eq!(s.projects, ["/b", "/a", "/c", "/d"]);
        assert!(s.collapsed.is_empty());
        assert_eq!((s.project_with_id(None, "p2"), s.project_with_id(None, "nope"), s.project_with_id(Some("devbox"), "p2")), (Some("/c"), None, None));
    }

    #[test]
    fn a_list_replaces_its_projects_settings_and_keeps_the_cached_ones_it_cannot_read() {
        let named = |name: &str| RepoConfig { name: name.into(), ..RepoConfig::default() };
        let mut s = Store {
            projects: ["/a", "/b"].map(String::from).to_vec(),
            repos: [("/a".to_string(), named("Old")), ("/b".to_string(), named("Kept"))].into(),
            ..Store::default()
        };
        let repo = json!({"name": "App", "setup": "make", "launch": {"provider": "claude", "model": "opus"}});
        s.apply_registry(None, &List { projects: vec![project("p1", "/a", repo), project("p2", "/b", json!(null))], ..List::default() });
        assert_eq!((s.repos["/a"].name.as_str(), s.repos["/a"].setup.as_str(), s.repos["/a"].launch.model.as_str()), ("App", "make", "opus"));
        assert_eq!(s.repos["/b"].name, "Kept");
    }

    #[test]
    fn a_hosts_list_leaves_this_macs_projects_alone_and_this_macs_leaves_the_hosts() {
        let mut s = Store {
            projects: ["/a", "//devbox/home/me/x", "/b"].map(String::from).to_vec(),
            tracked: ["/w/a-fox", "//devbox/home/me/w/x-fox"].map(String::from).into(),
            ..Store::default()
        };
        let mut there = list(&[("h1", "//devbox/home/me/y")]);
        there.tracked = vec!["//devbox/home/me/w/y-owl".into()];
        s.apply_registry(Some("devbox"), &there);
        assert_eq!(s.projects, ["/a", "/b", "//devbox/home/me/y"]);
        assert_eq!(s.tracked, ["/w/a-fox", "//devbox/home/me/w/y-owl"].map(String::from).into());
        s.apply_registry(None, &list(&[("p1", "/b")]));
        assert_eq!(s.projects, ["/b", "//devbox/home/me/y"]);
        assert_eq!(s.tracked, ["//devbox/home/me/w/y-owl"].map(String::from).into());
        assert_eq!((s.project_with_id(Some("devbox"), "h1"), s.project_with_id(None, "p1")), (Some("//devbox/home/me/y"), Some("/b")));
    }

    #[test]
    fn only_this_macs_list_sets_the_shared_settings_and_a_null_commit_all_is_on() {
        let mut s = Store { git: crate::prefs::Git { commit_all: false, ..Default::default() }, ..Store::default() };
        let mut there = list(&[]);
        there.settings = Shared {
            git: SharedGit { remote: "fork".into(), commit_all: Some(false), force_with_lease: true },
            access: "full".into(),
            worktree_root: "/w".into(),
            external_worktrees: true,
        };
        s.apply_registry(Some("devbox"), &there);
        assert_eq!((s.git.remote.as_str(), s.git.force_with_lease, s.worktree.root.as_str()), ("origin", false, ""));
        s.apply_registry(None, &there);
        assert_eq!(
            (s.git.remote.as_str(), s.git.commit_all, s.git.force_with_lease, s.agents.access.as_str(), s.worktree.root.as_str(), s.sidebar.external_worktrees),
            ("fork", false, true, "full", "/w", true)
        );
        s.apply_registry(None, &list(&[]));
        assert_eq!((s.git.remote.as_str(), s.git.commit_all), ("origin", true));
        assert_eq!(s.registry(None).settings, Some(s.shared()));
        assert_eq!(s.registry(Some("devbox")).settings, None);
    }

    #[test]
    fn a_hosts_first_list_keeps_its_cached_projects_and_they_are_pushed_once() {
        let mut s = Store {
            projects: ["//devbox/home/me/x", "/a", "//devbox/home/me/y"].map(String::from).to_vec(),
            repos: [("//devbox/home/me/x".to_string(), RepoConfig { setup: "make".into(), ..RepoConfig::default() })].into(),
            tracked: ["//devbox/home/me/w/x-fox".to_string()].into(),
            ..Store::default()
        };
        let mut first = list(&[("h2", "//devbox/home/me/y")]);
        let pushed = s.adopt_cached("devbox", &mut first);
        assert_eq!(
            pushed,
            [
                Change::Add("//devbox/home/me/x".into(), RepoConfig { setup: "make".into(), ..RepoConfig::default() }),
                Change::Track("//devbox/home/me/w/x-fox".into(), true)
            ]
        );
        s.apply_registry(Some("devbox"), &first);
        assert_eq!(s.projects, ["//devbox/home/me/x", "/a", "//devbox/home/me/y"]);
        assert_eq!(s.repos["//devbox/home/me/x"].setup, "make");
        assert!(s.is_tracked("//devbox/home/me/w/x-fox"));
        let back: Store = serde_json::from_slice(&serde_json::to_vec(&s).unwrap()).unwrap();
        assert!(back.registry_pushed.contains("devbox"));
        assert!(back.ids.is_empty(), "ids come from pocketd, not desktop.json");

        let mut again = list(&[("h2", "//devbox/home/me/y")]);
        assert!(s.adopt_cached("devbox", &mut again).is_empty());
        s.apply_registry(Some("devbox"), &again);
        assert_eq!(s.projects, ["/a", "//devbox/home/me/y"]);
    }

    #[test]
    fn a_hosts_first_list_gets_back_the_settings_this_desktop_cached_for_a_project_both_know() {
        let full = RepoConfig { setup: "make".into(), color: 7, teardown: "make clean".into(), ..RepoConfig::default() };
        let mut s = Store { projects: vec!["//devbox/home/me/x".into()], repos: [("//devbox/home/me/x".to_string(), full.clone())].into(), ..Store::default() };
        let mut first = List { projects: vec![project("h1", "//devbox/home/me/x", json!({"setup": "make"}))], ..List::default() };
        assert_eq!(s.adopt_cached("devbox", &mut first), [Change::Set("//devbox/home/me/x".into(), full.clone())]);
        s.apply_registry(Some("devbox"), &first);
        assert_eq!(s.repos["//devbox/home/me/x"], full);
    }

    #[test]
    fn a_hosts_first_list_keeps_the_settings_it_has_set() {
        let ours = RepoConfig { color: 7, teardown: "make clean".into(), ..RepoConfig::default() };
        let mut s = Store { projects: vec!["//devbox/home/me/x".into()], repos: [("//devbox/home/me/x".to_string(), ours)].into(), ..Store::default() };
        let mut first = List { projects: vec![project("h1", "//devbox/home/me/x", json!({"color": 3, "teardown": "rm -rf build"}))], ..List::default() };
        assert!(s.adopt_cached("devbox", &mut first).is_empty());
        s.apply_registry(Some("devbox"), &first);
        assert_eq!((s.repos["//devbox/home/me/x"].color, s.repos["//devbox/home/me/x"].teardown.as_str()), (3, "rm -rf build"));
    }

    #[test]
    fn a_change_here_becomes_the_messages_that_make_pocketds_copy_match() {
        let mut s = Store { projects: ["/a", "/b"].map(String::from).to_vec(), tracked: ["/w/a-fox".to_string()].into(), ..Store::default() };
        let before = s.registry(None);
        assert!(changes(&before, &s.registry(None)).is_empty());
        s.move_project("/b", "/a");
        assert!(changes(&before, &s.registry(None)).is_empty(), "the order is this desktop's own");
        s.add("/c");
        s.remove("/a");
        s.repos.entry("/b".into()).or_default().dev_url = "http://localhost:3000".into();
        s.untrack("/w/a-fox");
        s.track("/w/b-owl");
        s.git.force_with_lease = true;
        let b = RepoConfig { dev_url: "http://localhost:3000".into(), ..RepoConfig::default() };
        assert_eq!(
            changes(&before, &s.registry(None)),
            [
                Change::Set("/b".into(), b),
                Change::Add("/c".into(), RepoConfig::default()),
                Change::Remove("/a".into()),
                Change::Track("/w/b-owl".into(), true),
                Change::Track("/w/a-fox".into(), false),
                Change::Settings(s.shared())
            ]
        );
    }
}
