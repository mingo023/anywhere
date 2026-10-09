use crate::desktop::Desktop;
use workspace::Workspace;

impl Desktop {
    /// Brings back last launch's panels, once, after pocketd's first terminal list says which terminals still run.
    pub(crate) fn restore_layouts(&mut self) {
        if std::mem::replace(&mut self.panels.restored, true) || self.capturing {
            return;
        }
        if !self.store.general.restore_layouts {
            self.store.layouts.clear();
            return;
        }
        let live: Vec<String> = self.terminals.sessions.items.iter().map(|s| s.info.id.clone()).collect();
        for (tree, saved) in std::mem::take(&mut self.store.layouts) {
            if self.workspaces.get(&tree).is_none_or(Workspace::is_empty) {
                self.workspaces.insert(tree, saved.restored(&live));
            }
        }
        // The store gave its layouts up above; any `save()` before the next panel change would write none.
        self.remember_layouts();
    }

    /// Copies every worktree's panels that hold a tab into the store, once they have been restored.
    pub(crate) fn remember_layouts(&mut self) {
        if self.panels.restored {
            self.store.layouts = self.workspaces.iter().map(|(t, w)| (t.clone(), w.saved())).filter(|(_, w)| !w.is_empty()).collect();
        }
    }
}
