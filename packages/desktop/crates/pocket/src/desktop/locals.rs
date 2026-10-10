use crate::desktop::Desktop;
use crate::desktop::chrome::{Confirm, Overlay};
use agents::locals::{self, Local};
use gpui_kit::*;

impl Desktop {
    /// A tree's name as its Local row shows it: a Local's own, else the project checkout's given name, else "Local".
    pub(crate) fn local_name(&self, key: &str) -> String {
        self.agents.given_name(key).cloned().unwrap_or_else(|| "Local".into())
    }

    /// Adds a Local to `project`, named after the first free number, and returns its id.
    pub(crate) fn new_local(&mut self, project: &str) -> String {
        let main = self.tree_of(project).map(|t| self.local_name(&t));
        let taken: Vec<String> = self.agents.locals_in(project).map(|l| l.name.clone()).chain(main).collect();
        let local = Local { id: locals::new_id(), project: project.to_string(), name: locals::next_name(taken.iter().map(String::as_str)) };
        self.outbox.local_create(&local);
        self.agents.add_local(local.clone());
        local.id
    }

    pub(crate) fn add_local(&mut self, project: String, cx: &mut Context<Self>) {
        let id = self.new_local(&project);
        self.store.collapsed.remove(&project);
        self.store.save();
        self.select_tree(project, Some(id), cx);
    }

    pub(crate) fn ask_delete_local(&mut self, id: String, cx: &mut Context<Self>) {
        self.confirm = Some(Confirm::DeleteLocal(id));
        self.overlay = Some(Overlay::Confirm);
        cx.notify();
    }

    /// Closes its terminals; their files belong to the project and stay.
    pub(crate) fn delete_local(&mut self, id: &str, cx: &mut Context<Self>) {
        for term in self.tree_terminals(id) {
            self.close_pane(&term, cx);
        }
        self.outbox.local_delete(id);
        self.agents.remove_local(id);
        self.workspaces.remove(id);
        self.store.layouts.remove(id);
        self.leave_gone_local(cx);
        self.save_soon(cx);
        cx.notify();
    }

    /// Leaves the Local on screen once pocketd no longer has it, as when another client deleted it.
    pub(crate) fn leave_gone_local(&mut self, cx: &mut Context<Self>) {
        if self.worktree.as_deref().is_some_and(|t| locals::is_local(t) && self.agents.local(t).is_none()) {
            self.worktree = None;
            self.load_active(cx);
        }
    }
}
