use crate::desktop::Desktop;
use gpui_kit::*;
use workspace::Tab;

/// Where Move left and Move right take tab `i` of `len`, if it can go that way.
fn neighbours(i: usize, len: usize) -> (Option<usize>, Option<usize>) {
    (i.checked_sub(1), (i + 1 < len).then_some(i + 1))
}

impl Desktop {
    /// The right-click menu, while the tab it was opened on is still among `tabs`.
    pub(crate) fn tab_actions(&self, tabs: &[Tab], cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (tab, at) = self.terminal.tab_actions.as_ref()?;
        let i = tabs.iter().position(|t| t == tab)?;
        let (left, right) = neighbours(i, tabs.len());
        let row = |id: &'static str, icon: &str, label: &str, to: usize| {
            ui::menu_row(id, icon, label, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.terminal.tab_actions = None;
                this.move_tab(i, to, cx);
            }))
        };
        let menu = ui::pop(div().id("tab-actions"))
            .w(px(180.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.terminal.tab_actions = None;
                cx.notify();
            }))
            .children(left.map(|to| row("tab-move-left", "back", "Move left", to)))
            .children(right.map(|to| row("tab-move-right", "forward", "Move right", to)));
        Some(deferred(anchored().position(*at).snap_to_window_with_margin(px(8.)).child(ui::menu_in("tab-actions-in", menu))).with_priority(1))
    }
}

#[cfg(test)]
mod tests {
    use super::neighbours;

    #[test]
    fn a_tab_moves_only_toward_tabs_it_has() {
        assert_eq!(neighbours(0, 3), (None, Some(1)));
        assert_eq!(neighbours(1, 3), (Some(0), Some(2)));
        assert_eq!(neighbours(2, 3), (Some(1), None));
    }
}
