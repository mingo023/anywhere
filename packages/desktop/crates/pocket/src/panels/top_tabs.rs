use super::top_bar::{Bar, Face, TopBar, TopEvent};
use crate::desktop::Desktop;
use crate::util::tilde;
use gpui_kit::*;
use std::path::Path;
use theme::*;
use workspace::tree::Tree;
use workspace::{Doc, Tab, Workspace};

/// What a top tab says: the tab it is named after, and the tab its second line names. In a split, a focused doc leads and the first terminal shown follows; otherwise the focused terminal, else the first shown, leads and the first doc shown follows.
pub(crate) fn copy(t: &Tree<Tab>) -> (Option<&Tab>, Option<&Tab>) {
    let focused = t.focused().active();
    let shown: Vec<&Tab> = t.panes().into_iter().filter_map(|p| p.active()).collect();
    if shown.len() < 2 {
        return (focused, None);
    }
    let first = |doc: bool| shown.iter().copied().find(|tab| matches!(tab, Tab::Doc(_)) == doc);
    match focused {
        Some(tab @ Tab::Doc(_)) => (Some(tab), first(false)),
        Some(tab @ Tab::Term(_)) => (Some(tab), first(true)),
        _ => (first(false).or(focused), first(true)),
    }
}

impl Desktop {
    /// The bar over worktree `tree`'s panes: its top tabs and a + whose menu opens a new one. It moves the window.
    pub(super) fn top_bar(&mut self, tree: &str, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let (pad, toggle) = self.bar_start(6., cx);
        let w = &self.workspaces.read(cx)[tree];
        let (at, closable) = (w.at, !w.others.is_empty());
        let tabs = w.tops().into_iter().enumerate().map(|(i, t)| self.top_face(w, i, t, cx)).collect();
        let bar = Bar {
            tree: tree.to_string(),
            at,
            closable,
            tabs,
            pad,
            toggle,
            plus: self.new_tab_controls(workspace::Place::Top, TEXT_3, cx),
            trailing: [self.open_in_button(cx).map(IntoElement::into_any_element), self.top_actions(cx).map(IntoElement::into_any_element)]
                .into_iter()
                .flatten()
                .collect(),
        };
        self.panels.top.update(cx, |b, cx| b.view(bar, window, cx))
    }

    /// Top tab `i` of `w`, holding the panes of `t`.
    fn top_face(&self, w: &Workspace, i: usize, t: &Tree<Tab>, cx: &App) -> Face {
        let ink = if i == w.at { TEXT } else { TEXT_3 };
        let (head, meta) = copy(t);
        let panes = t.panes().len();
        let agent = [head, meta].into_iter().flatten().find_map(|tab| match tab {
            Tab::Term(id) => self.summary(id),
            _ => None,
        });
        let menu = agent.filter(|a| (self.chat_shown(&a.terminal_id) && self.can_fork(a)) || self.can_hand_off(a)).map(|a| a.id.clone());
        let lead = match (agent, head) {
            (Some(a), _) => provider_icon(&a.provider, 14., ink).into_any_element(),
            (None, Some(Tab::Doc(Doc::Commit(_)))) => icon("diff-multiple", 14., ink).into_any_element(),
            (
                None,
                Some(Tab::Doc(Doc::File(p) | Doc::Diff(p) | Doc::CommitFile { path: p, .. } | Doc::PrFile { path: p, .. } | Doc::CompareFile { path: p, .. })),
            ) => file_icon(p, false, false, 14.).into_any_element(),
            (None, Some(Tab::Web(_))) => icon("globe", 14., TEXT_3).into_any_element(),
            _ => icon("prompt", 14., TEXT_3).into_any_element(),
        };
        let italic = matches!(head, Some(Tab::Doc(d)) if w.preview.as_ref() == Some(d));
        let meta = match (meta, head) {
            (Some(m), _) => self.tab_text(m, cx),
            _ if panes > 1 => format!("{panes} panes"),
            (None, Some(h)) => self.tab_kind(h, cx),
            (None, None) => String::new(),
        };
        Face { lead, title: head.map(|h| self.tab_text(h, cx)).unwrap_or_default(), meta, italic, menu }
    }

    pub(crate) fn on_top_event(&mut self, _: &Entity<TopBar>, ev: &TopEvent, window: &mut Window, cx: &mut Context<Self>) {
        match ev {
            TopEvent::Switch(i) => self.switch_top(*i, window, cx),
            TopEvent::Close(i) => self.close_top(*i, cx),
            TopEvent::Grabbed => self.freeze_pages(window, cx),
            TopEvent::Aim { from, at } => {
                if let Some(tree) = self.place(cx) {
                    self.aim_top(*from, &tree, *at, cx);
                }
            }
            TopEvent::Level => {
                if self.place(cx).is_some() && self.panels.pane_drag.take().is_some() {
                    cx.notify();
                }
            }
            TopEvent::Released(moved) => self.release_top(*moved, window, cx),
            TopEvent::Menu(agent, at) => {
                self.close_menus(cx);
                self.panels.top_actions = Some((agent.clone(), *at));
                cx.notify();
            }
        }
    }

    fn release_top(&mut self, moved: Option<(usize, usize)>, window: &mut Window, cx: &mut Context<Self>) {
        if self.drop_top(window, cx) {
            return;
        }
        let Some((from, to)) = moved else { return };
        let Some(tree) = self.place(cx) else { return };
        let at = self.workspace(&tree, cx, |w| {
            w.move_top(from, to);
            w.at
        });
        // The shown tab was dragged into view already; scrolling to its new index would jump the bar.
        self.panels.top.update(cx, |b, _| b.revealed(tree, at));
        self.save_soon(cx);
        cx.notify();
    }

    /// What kind of tab a lone tab is: its agent, a terminal, the folder of its file, its changes, or a page.
    fn tab_kind(&self, tab: &Tab, cx: &App) -> String {
        match tab {
            Tab::Term(id) => self.summary(id).map_or_else(
                || "Terminal".to_string(),
                |a| {
                    let name = self.agent_name(&a.provider);
                    self.handoff_tab(&a.id, &name, cx).unwrap_or(name)
                },
            ),
            Tab::Doc(Doc::File(p)) => {
                let dir = Path::new(p).parent().unwrap_or(Path::new(""));
                match self.explore_root(cx).map(|root| dir.strip_prefix(root).map(Path::to_path_buf)) {
                    Some(Ok(rel)) if rel.as_os_str().is_empty() => self.project.as_deref().map(|p| self.repo_name(p)).unwrap_or_default(),
                    Some(Ok(rel)) => rel.to_string_lossy().into_owned(),
                    _ => tilde(&dir.to_string_lossy()),
                }
            }
            Tab::Doc(_) => "Changes".to_string(),
            Tab::Web(_) => "Browser".to_string(),
        }
    }

    /// Shows top tab `i`, and what its focused pane shows.
    fn switch_top(&mut self, i: usize, window: &mut Window, cx: &mut Context<Self>) {
        let Some(tree) = self.place(cx) else { return };
        let (p, a) = self.workspace(&tree, cx, |w| {
            w.switch(i);
            (w.tree.focused, w.tree.focused().active)
        });
        self.select_tab(p, a, window, cx);
        self.load_active(cx);
    }
}

#[cfg(test)]
mod tests {
    use super::copy;
    use workspace::tree::{Edge, Tree};
    use workspace::{Doc, Tab};

    fn term(id: &str) -> Tab {
        Tab::Term(id.into())
    }

    fn file(p: &str) -> Tab {
        Tab::Doc(Doc::File(p.into()))
    }

    #[test]
    fn a_lone_pane_is_named_after_what_it_shows() {
        let t = Tree::lone(0, term("a"));
        assert_eq!(copy(&t), (Some(&term("a")), None));
    }

    #[test]
    fn in_a_split_a_focused_file_leads_and_the_terminal_follows() {
        let mut t = Tree::lone(0, term("a"));
        t.split(0, Edge::Right, file("/x.md"));
        assert_eq!(copy(&t), (Some(&file("/x.md")), Some(&term("a"))));
    }

    #[test]
    fn in_a_split_with_the_terminal_focused_it_leads_and_the_file_follows() {
        let mut t = Tree::lone(0, term("a"));
        t.split(0, Edge::Right, file("/x.md"));
        t.focus(0);
        assert_eq!(copy(&t), (Some(&term("a")), Some(&file("/x.md"))));
    }

    #[test]
    fn a_split_of_one_kind_leads_with_the_focus_and_counts_its_panes() {
        let mut t = Tree::lone(0, term("a"));
        t.split(0, Edge::Right, term("b"));
        assert_eq!(copy(&t), (Some(&term("b")), None));
    }
}
