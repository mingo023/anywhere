use crate::desktop::Desktop;
use gpui_kit::App;
use std::collections::BTreeMap;
use workspace::Workspace;

impl Desktop {
    /// Brings back last launch's panels, once, after pocketd's first terminal list says which terminals still run.
    /// A host's worktrees wait for that host's list instead.
    pub(crate) fn restore_layouts(&mut self, cx: &mut App) {
        if std::mem::replace(&mut self.panels.restored, true) || self.capturing {
            return;
        }
        if !self.store.general.restore_layouts {
            self.store.layouts.clear();
            return;
        }
        let (waiting, here): (BTreeMap<_, _>, BTreeMap<_, _>) =
            std::mem::take(&mut self.store.layouts).into_iter().partition(|(tree, _)| hosts::split(tree).0.is_some());
        self.panels.waiting = waiting;
        self.restore(here, cx);
    }

    /// Brings back the panels of `host`'s worktrees, once it has listed its terminals.
    pub(crate) fn restore_host_layouts(&mut self, host: &str, cx: &mut App) {
        if self.panels.restored {
            let saved = take_host(&mut self.panels.waiting, host);
            self.restore(saved, cx);
        }
    }

    fn restore(&mut self, saved: BTreeMap<String, Workspace>, cx: &mut App) {
        let live: Vec<String> = self.terminals.read(cx).sessions.items.iter().map(|s| s.info.id.clone()).collect();
        self.workspaces.update(cx, |all, _| {
            for (tree, saved) in saved {
                if all.get(&tree).is_none_or(Workspace::is_empty) {
                    all.insert(tree, saved.restored(&live));
                }
            }
        });
        // The store gave its layouts up above; any `save()` before the next panel change would write none.
        self.remember_layouts(cx);
    }

    /// Copies every worktree's panels that hold a tab into the store, once they have been restored; hosts' still waiting stay as saved.
    pub(crate) fn remember_layouts(&mut self, cx: &App) {
        if self.panels.restored {
            let shown = self.workspaces.read(cx).iter().map(|(t, w)| (t.clone(), w.saved())).filter(|(_, w)| !w.is_empty());
            let waiting = self.panels.waiting.iter().map(|(t, w)| (t.clone(), w.saved()));
            self.store.layouts = waiting.chain(shown).collect();
        }
    }
}

/// The saved panels of `host`'s worktrees, taken out of `waiting`.
fn take_host(waiting: &mut BTreeMap<String, Workspace>, host: &str) -> BTreeMap<String, Workspace> {
    let (theirs, rest) = std::mem::take(waiting).into_iter().partition(|(tree, _)| hosts::split(tree).0 == Some(host));
    *waiting = rest;
    theirs
}

#[cfg(test)]
mod tests {
    use super::take_host;
    use std::collections::BTreeMap;
    use workspace::Workspace;

    #[test]
    fn a_hosts_panels_wait_until_that_host_lists_its_terminals() {
        let mut waiting = BTreeMap::from([("//devbox/app".to_string(), Workspace::default()), ("//gpu/app".to_string(), Workspace::default())]);
        assert_eq!(take_host(&mut waiting, "devbox").into_keys().collect::<Vec<_>>(), ["//devbox/app"]);
        assert_eq!(waiting.into_keys().collect::<Vec<_>>(), ["//gpu/app"]);
    }
}
