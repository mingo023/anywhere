use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::desktop::Desktop;
use agents::locals::is_local;
use crate::sidebar::SidebarState;

/// A worktree's or Local's row name, edited in place; leaving it saves, as Enter does.
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
    /// Edits `tree`'s display name in its row, starting from the current one, all selected. `tree` is a worktree's path or a Local's id.
    pub(crate) fn start_rename(&mut self, tree: String, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.agents.given_name(&tree).cloned().unwrap_or_default();
        let input = cx.new(|cx| InputState::new(window, cx));
        input.update(cx, |s, cx| {
            s.set_value(current, window, cx);
            s.focus(window, cx);
            s.select_all(window, cx);
        });
        let sub = cx.subscribe_in(&input, window, |this, _, ev: &InputEvent, _, cx| match ev {
            InputEvent::PressEnter { .. } | InputEvent::Blur => this.save_rename(cx),
            _ => {}
        });
        self.row_menu = None;
        self.sidebar.rename = Some(Rename { tree, input, _sub: sub });
        cx.notify();
    }

    fn save_rename(&mut self, cx: &mut Context<Self>) {
        let Some(r) = self.sidebar.rename.take() else { return };
        let title = r.input.read(cx).value().trim().to_string();
        // Leaving an untouched name mustn't pin naming's title as the user's.
        if title == self.agents.given_name(&r.tree).map_or("", String::as_str) {
            return cx.notify();
        }
        if is_local(&r.tree) && !title.is_empty() {
            self.outbox.local_rename(&r.tree, &title);
            self.agents.rename_local(&r.tree, &title);
        } else if !is_local(&r.tree) {
            self.outbox.rename(&r.tree, &title);
            self.agents.set_name(&r.tree, &title);
        }
        cx.notify();
    }
}
