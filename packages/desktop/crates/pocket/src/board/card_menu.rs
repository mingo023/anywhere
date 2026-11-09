use super::Menu;
use super::logic::{Action, actions};
use crate::desktop::Desktop;
use gpui_kit::*;

fn action_row(a: Action) -> Option<(&'static str, &'static str, bool)> {
    Some(match a {
        Action::Start => ("play", "Start", false),
        Action::StartAgain => ("reload", "Start again", false),
        Action::Edit => ("pencil", "Edit", false),
        Action::MoveBack => ("back", "Move back to To do", false),
        Action::Open => ("external", "Open", false),
        Action::CopyBranch => ("copy", "Copy branch name", false),
        Action::Restore => ("redo", "Restore", false),
        Action::Close => ("run-cancelled", "Close ticket", false),
        Action::Delete => ("trash", "Delete", true),
        Action::DeleteWorktree => ("trash", "Delete worktree…", true),
        Action::Separator => return None,
    })
}

impl Desktop {
    pub(super) fn card_menu(&self, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let Some(Menu::Card { id, at }) = self.board.menu.as_ref() else { return None };
        let t = self.ticket(id)?;
        let items = actions(t, self.ticket_reach(t));
        if items.is_empty() {
            return None;
        }
        let rows: Vec<AnyElement> = items
            .into_iter()
            .enumerate()
            .map(|(i, a)| {
                let Some((name, label, danger)) = action_row(a) else { return ui::menu_divider().into_any_element() };
                let id = id.clone();
                let row = if danger { ui::danger_row(("board-action", i), name, label) } else { ui::menu_row(("board-action", i), name, label, None) };
                row.on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.run_action(a, &id, window, cx))).into_any_element()
            })
            .collect();
        Some(deferred(anchored().position(*at).snap_to_window_with_margin(px(8.)).child(ui::menu_in("board-menu-in", self.popup("board-menu", 210., cx).children(rows)))).with_priority(2))
    }
}
