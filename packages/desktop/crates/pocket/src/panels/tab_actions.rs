use super::frame::TabFile;
use super::{Panels, PanelsEvent};
use crate::util::basename;
use gpui_kit::*;
use std::path::Path;
use workspace::tree::{Edge, PaneId, Target};
use workspace::{Doc, Tab};

/// Where Move left and Move right take tab `i` of `len`, if it can go that way.
fn neighbours(i: usize, len: usize) -> (Option<usize>, Option<usize>) {
    (i.checked_sub(1), (i + 1 < len).then_some(i + 1))
}

impl Panels {
    /// `pane`'s right-click menu, while the tab it was opened on is still among `tabs`.
    pub(super) fn tab_actions(&self, tree: &str, pane: PaneId, tabs: &[Tab], file: Option<&TabFile>, cx: &mut Context<Self>) -> Option<impl IntoElement> {
        let (_, tab, at) = self.actions.as_ref().filter(|(p, _, _)| *p == pane)?;
        let i = tabs.iter().position(|t| t == tab)?;
        let (left, right) = neighbours(i, tabs.len());
        let row = |id: &'static str, icon: &str, label: &str, to: usize, cx: &mut Context<Self>| {
            ui::menu_row(id, icon, label, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.actions = None;
                this.move_tab(pane, i, to, cx);
            }))
        };
        let bounds = self.bounds;
        let [can_right, can_down] = [Edge::Right, Edge::Bottom].map(|edge| self.workspaces.read(cx)[tree].tree.can_split(pane, edge, bounds));
        let split = |id: &'static str, icon: &str, label: &str, edge: Edge, can: bool, cx: &mut Context<Self>| {
            (tabs.len() > 1 && can).then(|| {
                ui::menu_row(id, icon, label, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.actions = None;
                    this.move_tab_to(pane, i, Target::Split { pane, edge }, cx);
                }))
            })
        };
        let moves: Vec<Stateful<Div>> = [
            left.map(|to| row("tab-move-left", "back", "Move left", to, cx)),
            right.map(|to| row("tab-move-right", "forward", "Move right", to, cx)),
            split("tab-split-right", "split-right", "Move to new split right", Edge::Right, can_right, cx),
            split("tab-split-down", "split-down", "Move to new split down", Edge::Bottom, can_down, cx),
        ]
        .into_iter()
        .flatten()
        .collect();
        let mut files = Vec::new();
        if let Some((path, rel)) = file.and_then(|f| f.0.clone()) {
            if matches!(tab, Tab::Doc(Doc::File(_))) {
                let prompt = format!("About {rel}: ");
                files.push(ui::menu_row("tab-ask", "sparkle", "Ask about this file", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.actions = None;
                    cx.emit(PanelsEvent::Ask(prompt.clone()));
                })));
            }
            let opened = path.clone();
            files.push(ui::menu_row("tab-open", "external", "Open in Default App", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.actions = None;
                cx.open_with_system(Path::new(&opened));
                cx.notify();
            })));
            let revealed = path.clone();
            files.push(ui::menu_row("tab-reveal", "folder", "Reveal in Finder", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.actions = None;
                cx.reveal_path(Path::new(&revealed));
                cx.notify();
            })));
            let name = basename(&path);
            for (id, label, text) in
                [("tab-copy-path", "Copy Path", path), ("tab-copy-relative", "Copy Relative Path", rel), ("tab-copy-name", "Copy File Name", name)]
            {
                files.push(ui::menu_row(id, "copy", label, None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                    this.actions = None;
                    cx.write_to_clipboard(ClipboardItem::new_string(text.clone()));
                    cx.notify();
                })));
            }
        }
        let mut closes = vec![ui::menu_row("tab-close", "x", "Close", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
            this.actions = None;
            cx.emit(PanelsEvent::Close(pane, i));
            cx.notify();
        }))];
        if tabs.len() > 1 {
            closes.push(ui::menu_row("tab-close-others", "x", "Close Others", None).on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                this.actions = None;
                cx.emit(PanelsEvent::CloseOthers(pane, i));
                cx.notify();
            })));
        }
        let groups = [moves, files, closes].into_iter().filter(|g| !g.is_empty()).enumerate().flat_map(|(n, g)| {
            (n > 0).then(ui::menu_divider).map(IntoElement::into_any_element).into_iter().chain(g.into_iter().map(IntoElement::into_any_element))
        });
        let menu = ui::pop(div().id("tab-actions"))
            .w(px(220.))
            .p(px(6.))
            .flex()
            .flex_col()
            .occlude()
            .on_mouse_down_out(cx.listener(|this, _: &MouseDownEvent, _, cx| {
                this.actions = None;
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
