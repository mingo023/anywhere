use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::desktop::Desktop;
use crate::sidebar::SidebarState;

/// A worktree row's display name, edited in place.
pub(crate) struct Rename {
    pub(crate) tree: String,
    pub(crate) input: Entity<InputState>,
    _sub: Subscription,
}

impl SidebarState {
    pub(crate) fn renaming(&self, tree: &str) -> bool {
        self.rename.as_ref().is_some_and(|r| r.tree == tree)
    }

    /// Drops an open rename unsaved; true when there was one.
    pub(crate) fn cancel_rename(&mut self) -> bool {
        self.rename.take().is_some()
    }
}

impl Desktop {
    /// Edits `tree`'s display name in its row, starting from the current one, all selected.
    pub(crate) fn start_rename(&mut self, tree: String, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.agents.names.get(&tree).cloned().unwrap_or_default();
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |s, cx| {
            s.set_value(current, window, cx);
            s.focus(window, cx);
            s.select_all(window, cx);
        });
        let sub = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| match ev {
            InputEvent::PressEnter { .. } => this.save_rename(cx),
            InputEvent::Blur if this.sidebar.cancel_rename() => cx.notify(),
            _ => {}
        });
        self.row_menu = None;
        self.sidebar.rename = Some(Rename { tree, input, _sub: sub });
        cx.notify();
    }

    fn save_rename(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.sidebar.rename.take() else { return };
        let title = r.input.read(cx).value().trim().to_string();
        self.outbox.rename(&r.tree, &title);
        self.agents.set_name(&r.tree, &title);
        cx.notify();
    }
}
