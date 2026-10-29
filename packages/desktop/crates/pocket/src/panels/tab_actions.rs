use crate::desktop::Desktop;
use crate::explorer::preview::relative;
use crate::util::basename;
use gpui_kit::*;
use std::path::Path;
use workspace::{Doc, Tab};
use workspace::tree::{Edge, PaneId, Target};

/// Where Move left and Move right take tab `i` of `len`, if it can go that way.
fn neighbours(i: usize, len: usize) -> (Option<usize>, Option<usize>) {
    (i.checked_sub(1), (i + 1 < len).then_some(i + 1))
}

impl Desktop {
    fn copy_path(&mut self, path: &str, cx: &mut Context<Self>) {
        cx.write_to_clipboard(ClipboardItem::new_string(path.to_string()));
    }

    /// Closes every tab of `pane` but tab `keep`, highest index first so the rest keep theirs, stopping at the first that asks to be confirmed.
    fn close_others(&mut self, pane: PaneId, keep: usize, cx: &mut Context<Self>) {
        let count = |this: &Self| this.place().and_then(|t| this.workspaces.get(&t)?.tree.pane(pane).map(|p| p.tabs.len())).unwrap_or_default();
        for i in (0..count(self)).rev().filter(|i| *i != keep) {
            let before = count(self);
            self.close_tab(pane, i, cx);
            if count(self) == before {
                return;
            }
        }
    }

    /// `pane`'s right-click menu, while the tab it was opened on is still among `tabs`.
    pub(crate) fn tab_actions(&self, tree: &str, pane: PaneId, tabs: &[Tab], cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (_, tab, at) = self.panels.actions.as_ref().filter(|(p, _, _)| *p == pane)?;
        let i = tabs.iter().position(|t| t == tab)?;
        let (left, right) = neighbours(i, tabs.len());
        let row = |id: &'static str, icon: &str, label: &str, to: usize, cx: &mut Context<Self>| {
            ui::menu_row(id, icon, label, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.panels.actions = None;
                this.move_tab(pane, i, to, cx);
            }))
        };
        let t = &self.workspaces[tree].tree;
        let split = |id: &'static str, icon: &str, label: &str, edge: Edge, cx: &mut Context<Self>| {
            (tabs.len() > 1 && t.can_split(pane, edge, self.panels.bounds)).then(|| {
                ui::menu_row(id, icon, label, None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.panels.actions = None;
                    this.move_tab_to(pane, i, Target::Split { pane, edge }, window, cx);
                }))
            })
        };
        let moves: Vec<Stateful<Div>> = [
            left.map(|to| row("tab-move-left", "back", "Move left", to, cx)),
            right.map(|to| row("tab-move-right", "forward", "Move right", to, cx)),
            split("tab-split-right", "split-right", "Move to new split right", Edge::Right, cx),
            split("tab-split-down", "split-down", "Move to new split down", Edge::Bottom, cx),
        ]
        .into_iter()
        .flatten()
        .collect();
        let file = match tab {
            Tab::Doc(Doc::File(p)) => Some(p.clone()),
            Tab::Doc(Doc::Diff(p) | Doc::CommitFile { path: p, .. } | Doc::PrFile { path: p, .. }) => self.cwd().map(|cwd| format!("{cwd}/{p}")),
            _ => None,
        };
        let mut files = Vec::new();
        if let Some(path) = file {
            let rel = relative(&path, &self.explore_root().unwrap_or_default());
            if matches!(tab, Tab::Doc(Doc::File(_))) {
                let prompt = format!("About {rel}: ");
                files.push(ui::menu_row("tab-ask", "sparkle", "Ask about this file", None).on_click(cx.listener(move |this, _: &ClickEvent, window, cx| {
                    this.panels.actions = None;
                    this.new_session(Some(prompt.clone()), false, window, cx);
                })));
            }
            let opened = path.clone();
            files.push(ui::menu_row("tab-open", "external", "Open in Default App", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.panels.actions = None;
                cx.open_with_system(Path::new(&opened));
                cx.notify();
            })));
            let revealed = path.clone();
            files.push(ui::menu_row("tab-reveal", "folder", "Reveal in Finder", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.panels.actions = None;
                cx.reveal_path(Path::new(&revealed));
                cx.notify();
            })));
            let name = basename(&path);
            for (id, label, text) in [("tab-copy-path", "Copy Path", path), ("tab-copy-relative", "Copy Relative Path", rel), ("tab-copy-name", "Copy File Name", name)] {
                files.push(ui::menu_row(id, "copy", label, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.panels.actions = None;
                    this.copy_path(&text, cx);
                    cx.notify();
                })));
            }
        }
        let mut closes = vec![ui::menu_row("tab-close", "x", "Close", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.panels.actions = None;
            this.close_tab(pane, i, cx);
            cx.notify();
        }))];
        if tabs.len() > 1 {
            closes.push(ui::menu_row("tab-close-others", "x", "Close Others", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.panels.actions = None;
                this.close_others(pane, i, cx);
                cx.notify();
            })));
        }
        let groups = [moves, files, closes].into_iter().filter(|g| !g.is_empty()).enumerate().flat_map(|(n, g)| (n > 0).then(ui::menu_divider).map(IntoElement::into_any_element).into_iter().chain(g.into_iter().map(IntoElement::into_any_element)));
        let menu = ui::pop(div().id("tab-actions"))
            .w(px(220.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.panels.actions = None;
                cx.notify();
            }))
            .children(groups);
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
