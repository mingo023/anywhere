use crate::actions::{EqualizePanes, FocusPane, JumpToTab, NextTab, PrevTab, SplitDown, SplitRight, ZoomPane};
use crate::desktop::Desktop;
use gpui_kit::*;
use workspace::Place;
use workspace::tree::Edge;

impl Desktop {
    /// The worktree whose panels a shortcut acts on; none behind a modal or the page of a worktree being created, while pocketd is unreachable, or on the Inbox.
    fn panel_tree(&self) -> Option<String> {
        self.session_tree().filter(|_| self.overlay.is_none() && self.shown_create().is_none())
    }

    pub(crate) fn split_right(&mut self, _: &SplitRight, _: &mut Window, cx: &mut Context<Self>) {
        self.split_focused(Edge::Right, cx);
    }

    pub(crate) fn split_down(&mut self, _: &SplitDown, _: &mut Window, cx: &mut Context<Self>) {
        self.split_focused(Edge::Bottom, cx);
    }

    fn split_focused(&mut self, edge: Edge, cx: &mut Context<Self>) {
        let Some(tree) = self.panel_tree() else { return };
        let bounds = self.panels.bounds;
        let t = &self.workspace(&tree).tree;
        let pane = t.focused;
        if t.can_split(pane, edge, bounds) {
            self.release_page();
            self.open_tab(Place::Split(pane, edge), cx);
        }
    }

    pub(crate) fn focus_toward(&mut self, a: &FocusPane, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.panel_tree() else { return };
        let t = &self.workspace(&tree).tree;
        if let Some(to) = t.neighbour(t.focused, a.0) {
            self.focus_panel(to, window, cx);
        }
    }

    pub(crate) fn prev_tab(&mut self, _: &PrevTab, window: &mut Window, cx: &mut Context<Self>) {
        self.step_tab(false, window, cx);
    }

    pub(crate) fn next_tab(&mut self, _: &NextTab, window: &mut Window, cx: &mut Context<Self>) {
        self.step_tab(true, window, cx);
    }

    fn step_tab(&mut self, forward: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.panel_tree() else { return };
        if let Some((pane, i)) = self.workspace(&tree).tree.step(forward) {
            self.select_tab(pane, i, window, cx);
        }
    }

    pub(crate) fn jump_to_tab(&mut self, a: &JumpToTab, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.panel_tree() else { return };
        if let Some((pane, i)) = self.workspace(&tree).tree.nth_tab(a.0) {
            self.select_tab(pane, i, window, cx);
        }
    }

    pub(crate) fn zoom_pane(&mut self, _: &ZoomPane, _: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.panel_tree() else { return };
        self.workspace(&tree).tree.toggle_zoom();
        cx.notify();
    }

    pub(crate) fn equalize_panes(&mut self, _: &EqualizePanes, _: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.panel_tree() else { return };
        self.workspace(&tree).tree.equalize_all();
        self.save_soon(cx);
        cx.notify();
    }
}
