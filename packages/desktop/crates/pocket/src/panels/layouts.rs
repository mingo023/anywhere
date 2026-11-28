use crate::desktop::Desktop;
use gpui_kit::App;
use std::collections::BTreeMap;
use workspace::Workspace;

impl Desktop {
    /// Brings back last launch's panels, once, after pocketd's first terminal list says which terminals still run.
    /// A host's worktrees wait for that host's list instead.
    pub(crate) fn restore_layouts(&mut self, cx: &mut App) {
        if !self.workspaces.update(cx, |all, _| all.mark_restored()) || self.capturing {
            return;
        }
        if !self.store.general.restore_layouts {
            self.store.layouts.clear();
            return;
        }
        let (waiting, here): (BTreeMap<_, _>, BTreeMap<_, _>) =
            std::mem::take(&mut self.store.layouts).into_iter().partition(|(tree, _)| hosts::host_of(tree).is_some());
        self.workspaces.update(cx, |all, _| all.wait(waiting));
        self.restore(here, cx);
    }

    /// Brings back the panels of `host`'s worktrees, once it has listed its terminals.
    pub(crate) fn restore_host_layouts(&mut self, host: &str, cx: &mut App) {
        if self.workspaces.read(cx).is_restored() {
            let saved = self.workspaces.update(cx, |all, _| all.take_waiting(|tree| hosts::host_of(tree) == Some(host)));
            self.restore(saved, cx);
        }
    }

    fn restore(&mut self, saved: BTreeMap<String, Workspace>, cx: &mut App) {
        let live: Vec<String> = self.terminals.read(cx).sessions.items.iter().map(|s| s.info.id.clone()).collect();
        self.workspaces.update(cx, |all, _| all.restore(saved, &live));
        // The store gave its layouts up above; any `save()` before the next panel change would write none.
        self.remember_layouts(cx);
    }

    /// Copies every worktree's panels that hold a tab into the store, once they have been restored; hosts' still waiting stay as saved.
    pub(crate) fn remember_layouts(&mut self, cx: &App) {
        if let Some(saved) = self.workspaces.read(cx).saved() {
            self.store.layouts = saved;
        }
    }
}
