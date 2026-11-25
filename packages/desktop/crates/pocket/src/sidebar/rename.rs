use gpui_kit::component::input::{InputEvent, InputState};
use gpui_kit::*;

use crate::desktop::Desktop;
use crate::sidebar::{Sidebar, SidebarEvent};
use agents::locals::is_local;

/// A worktree's or Local's row name, edited in place; leaving it saves, as Enter does.
pub(crate) struct Rename {
    pub(crate) tree: String,
    pub(crate) input: Entity<InputState>,
    _sub: Subscription,
}

impl Sidebar {
    pub(crate) fn begin_rename(&mut self, tree: String, current: String, window: &mut Window, cx: &mut Context<Self>) {
        let (input, sub) = rename_field(current, window, cx, Self::finish_rename);
        self.rename = Some(Rename { tree, input, _sub: sub });
        cx.notify();
    }

    fn finish_rename(&mut self, cx: &mut Context<Self>) {
        if let Some(r) = self.rename.take() {
            let title = r.input.read(cx).value().trim().to_string();
            cx.emit(SidebarEvent::Renamed { tree: r.tree, title });
        }
        cx.notify();
    }

    pub(crate) fn renaming(&self, tree: &str) -> bool {
        self.rename.as_ref().is_some_and(|r| r.tree == tree)
    }

    /// Drops an open rename, a row's or a group's, unsaved; true when there was one.
    pub(crate) fn cancel_rename(&mut self, cx: &mut Context<Self>) -> bool {
        // `|`, not `||`: both must drop.
        let dropped = (self.rename.take().is_some() | self.organize.rename.take().is_some()) || self.space_header.update(cx, |h, cx| h.cancel_rename(cx));
        if dropped {
            cx.notify();
        }
        dropped
    }
}

/// A field holding `text`, focused and all selected, that calls `done` on Enter or blur.
pub(crate) fn rename_field<T: 'static>(
    text: String,
    window: &mut Window,
    cx: &mut Context<T>,
    done: fn(&mut T, &mut Context<T>),
) -> (Entity<InputState>, Subscription) {
    let input = cx.new(|cx| InputState::new(window, cx));
    input.update(cx, |s, cx| {
        s.set_value(text, window, cx);
        s.focus(window, cx);
        s.select_all(window, cx);
    });
    let sub = cx.subscribe_in(&input, window, move |this, _, ev: &InputEvent, _, cx| match ev {
        InputEvent::PressEnter { .. } | InputEvent::Blur => done(this, cx),
        _ => {}
    });
    (input, sub)
}

impl Desktop {
    /// Edits `tree`'s display name in its row, starting from the current one, all selected. `tree` is a worktree's path or a Local's id.
    pub(crate) fn start_rename(&mut self, tree: String, window: &mut Window, cx: &mut Context<Self>) {
        let current = self.agents.given_name(&tree).unwrap_or_default().to_string();
        self.row_menu = None;
        self.sidebar.update(cx, |s, cx| s.begin_rename(tree, current, window, cx));
        cx.notify();
    }

    pub(super) fn save_rename(&mut self, tree: &str, title: &str, cx: &mut Context<Self>) {
        // Leaving an untouched name mustn't pin naming's title as the user's.
        if title == self.agents.given_name(tree).unwrap_or_default() {
            return cx.notify();
        }
        if is_local(tree) && !title.is_empty() {
            self.outbox.local.local_rename(tree, title);
            self.agents.rename_local(tree, title);
        } else if !is_local(tree) {
            self.outbox.rename(tree, title);
            self.agents.set_name(tree, title);
        }
        cx.notify();
    }
}
