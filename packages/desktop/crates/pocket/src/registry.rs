//! The projects, their settings, tracked worktrees and shared settings each pocketd owns: its `registry.list` lands in
//! the store, and a change made here goes to it as the messages that make its copy match.

use crate::desktop::Desktop;
use agents::registry::List;
use gpui_kit::*;
use std::collections::HashMap;
use store::RepoConfig;
use store::registry::{Change, Slice, changes};

/// What each pocketd was last known to hold, this Mac's under `None`: a change is what the store holds beyond it.
#[derive(Default)]
pub(crate) struct Registry {
    synced: HashMap<Option<String>, Slice>,
}

impl Registry {
    /// Takes this Mac's pocketd to hold what `desktop.json` cached, so a change made before its first list is still sent.
    pub(crate) fn new(store: &store::Store) -> Self {
        let mut registry = Self::default();
        registry.listed(None, store.registry(None));
        registry
    }

    /// Notes that `host`'s pocketd holds `slice`, as it just listed or as an outdated one keeps.
    pub(crate) fn listed(&mut self, host: Option<&str>, slice: Slice) {
        self.synced.insert(host.map(str::to_string), slice);
    }

    pub(crate) fn forget(&mut self, host: &str) {
        self.synced.remove(&Some(host.to_string()));
    }

    pub(crate) fn machines(&self) -> Vec<Option<String>> {
        self.synced.keys().cloned().collect()
    }

    /// What takes `host`'s pocketd to `now`, which it is then taken to hold; nothing before it was heard from.
    pub(crate) fn changes(&mut self, host: Option<&str>, now: Slice) -> Vec<Change> {
        let Some(before) = self.synced.get_mut(&host.map(str::to_string)) else { return Vec::new() };
        let out = changes(before, &now);
        *before = now;
        out
    }
}

/// What a change aimed at a pocketd without `registry.v2` says: `host`'s name, or `None` for this Mac's.
pub(crate) fn outdated(host: Option<&str>) -> String {
    match host {
        Some(name) => format!("Update Anywhere on {name}"),
        None => agents::OUTDATED.into(),
    }
}

impl Desktop {
    /// Takes `host`'s list as what its machine holds; a host's first list also sends it the projects cached for it.
    pub(crate) fn on_registry(&mut self, host: Option<&str>, mut list: List, cx: &mut Context<Self>) {
        // Sent before the list replaces the store, which would otherwise undo a change not yet sent.
        self.publish_registry();
        let pushed = host.map(|h| self.store.adopt_cached(h, &mut list)).unwrap_or_default();
        pushed.iter().for_each(|c| self.send_change(c));
        let before = self.store.projects.clone();
        self.store.apply_registry(host, &list);
        self.registry.listed(host, self.store.registry(host));
        self.save_soon(cx);
        if self.store.projects != before {
            self.refresh_git(cx);
        }
        cx.notify();
    }

    /// Notes what an outdated pocketd that said hello holds, so a change aimed at it says it needs updating.
    pub(crate) fn registry_connected(&mut self, host: Option<&str>) {
        if self.agents.registry_outdated(host) {
            self.registry.listed(host, self.store.registry(host));
        }
    }

    /// Sends each pocketd the changes to what it owns made here since it was last heard from.
    pub(crate) fn publish_registry(&mut self) {
        for host in self.registry.machines() {
            let host = host.as_deref();
            let changes = self.registry.changes(host, self.store.registry(host));
            if changes.is_empty() {
                continue;
            }
            if self.agents.registry_outdated(host) {
                self.error = Some(outdated(host.map(|h| self.host_name(h)).as_deref()));
                continue;
            }
            changes.iter().for_each(|c| self.send_change(c));
        }
    }

    fn send_change(&self, change: &Change) {
        let repo = |r: &RepoConfig| serde_json::to_value(r).unwrap_or_default();
        match change {
            Change::Add(path, r) => _ = self.outbox.project_add(path, Some(&repo(r))),
            Change::Remove(path) => _ = self.outbox.project_remove(path),
            Change::Set(path, r) => _ = self.outbox.project_set(path, &repo(r)),
            Change::Track(path, tracked) => _ = self.outbox.worktree_track(path, *tracked),
            Change::Settings(settings) => self.outbox.local.settings_shared(settings),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Registry, outdated};
    use store::Store;
    use store::registry::Change;

    #[test]
    fn a_change_goes_to_a_pocketd_once_and_only_after_it_was_heard_from() {
        let (mut registry, mut store) = (Registry::default(), Store::default());
        store.add("//devbox/home/me/app");
        assert!(registry.changes(Some("devbox"), store.registry(Some("devbox"))).is_empty(), "nothing is known of what it holds");
        registry.listed(Some("devbox"), store.registry(Some("devbox")));
        store.track("//devbox/home/me/w/app-fox");
        store.track("/w/here-owl");
        assert_eq!(registry.changes(Some("devbox"), store.registry(Some("devbox"))), [Change::Track("//devbox/home/me/w/app-fox".into(), true)]);
        assert!(registry.changes(Some("devbox"), store.registry(Some("devbox"))).is_empty(), "a change is sent once");
        registry.forget("devbox");
        assert!(registry.machines().is_empty());
    }

    #[test]
    fn a_change_made_before_this_macs_first_list_is_sent() {
        let mut store = Store::default();
        store.add("/w/app");
        let mut registry = Registry::new(&store);
        store.add("/w/new");
        assert_eq!(registry.changes(None, store.registry(None)), [Change::Add("/w/new".into(), Default::default())]);
    }

    #[test]
    fn a_change_for_an_outdated_host_names_it() {
        assert_eq!(outdated(Some("devbox")), "Update Anywhere on devbox");
        assert_eq!(outdated(None), agents::OUTDATED);
    }
}
