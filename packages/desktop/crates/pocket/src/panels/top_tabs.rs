use super::strip::{DragTop, GAP, TabDrag, slots};
use crate::desktop::Desktop;
use crate::desktop::chrome::drag_area;
use crate::terminal_view::context;
use crate::util::tilde;
use gpui_kit::prelude::FluentBuilder as _;
use gpui_kit::*;
use std::path::Path;
use std::time::Instant;
use theme::*;
use workspace::tree::Tree;
use workspace::{Doc, Tab};

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
    /// The bar over worktree `tree`'s panes: its top tabs, a + for a new agent in a new one, and the focused agent's context ring. It moves the window.
    pub(super) fn top_bar(&mut self, tree: &str, window: &mut Window, cx: &mut Context<Self>) -> Div {
        let (pad, toggle) = self.bar_start(6., cx);
        let at = self.workspaces[tree].at;
        let shown = Some((tree.to_string(), at));
        if self.panels.top_revealed != shown {
            self.panels.top_scroll.scroll_to_item(at);
            self.panels.top_revealed = shown;
        }
        let n = self.workspaces[tree].others.len() + 1;
        let reduce_motion = cx.reduce_motion();
        if let Some(d) = self.panels.top_drag.as_mut() {
            // gpui ends a drag on any mouse up, even one the bar never sees; a hold outliving it would keep its tab lifted.
            if !d.lay(slots(&self.panels.top_scroll, n)) || !(d.released || cx.has_active_drag()) {
                self.panels.top_drag = None;
                window.request_animation_frame();
            } else if d.ease(Instant::now(), reduce_motion) {
                window.request_animation_frame();
            } else if d.released {
                self.panels.top_drag = None;
                window.request_animation_frame();
            }
        }
        let held = self.panels.top_drag.as_ref().and_then(TabDrag::held);
        let offsets = self.panels.top_drag.as_ref().map(|d| d.offsets.clone()).unwrap_or_default();
        let tabs: Vec<_> = self.workspaces[tree]
            .tops()
            .into_iter()
            .enumerate()
            .map(|(i, t)| {
                let tab = self.top_tab(tree, i, t, cx).left(px(offsets.get(i).copied().unwrap_or(0.)));
                // The slot holds the tab's place while it slides, so the bar can measure where tabs sit; deferred, the held tab draws over the rest.
                div().w(px(224.)).min_w(px(112.)).flex_shrink(1.).child(if held == Some(i) { deferred(tab).into_any_element() } else { tab.into_any_element() })
            })
            .collect();
        let plus = ui::control(div().id("new-top"))
            .size(px(28.))
            .flex()
            .flex_none()
            .items_center()
            .justify_center()
            .rounded(px(7.))
            .cursor_pointer()
            .hover(|s| s.bg(FILL_3))
            .child(icon("plus", 15., TEXT_3))
            .on_click(cx.listener(|this, _: &ClickEvent, _, cx| {
                let provider = this.store.agents.default_agent().to_string();
                this.new_agent_tab(&provider, workspace::Place::Top, cx);
            }));
        let strip = div()
            .id("top-strip")
            .track_scroll(&self.panels.top_scroll)
            .overflow_x_scroll()
            .flex_1()
            .min_w_0()
            .h_full()
            .flex()
            .items_center()
            .gap(px(GAP))
            .on_drag_move(cx.listener(|this, e: &DragMoveEvent<DragTop>, _, cx| this.hold_top((e.event.position.x.into(), e.event.position.y.into()), cx)))
            .capture_any_mouse_up(cx.listener(|this, e: &MouseUpEvent, window, cx| {
                if e.button == MouseButton::Left {
                    this.release_top(window, cx);
                }
            }))
            .on_mouse_up_out(MouseButton::Left, cx.listener(|this, _: &MouseUpEvent, window, cx| this.release_top(window, cx)))
            .children(tabs)
            .child(plus);
        let ring = match self.workspaces[tree].tree.focused().active() {
            Some(Tab::Term(t)) => self.summary(t).and_then(|a| a.context()).map(|(used, window)| context::ring(used, window, agents::level(used, window, self.store.sidebar.warn_at, self.store.sidebar.critical_at))),
            _ => None,
        };
        drag_area(div())
            .h(px(40.))
            .flex_none()
            .flex()
            .items_center()
            .gap(px(6.))
            .pl(px(pad))
            .pr(px(6.))
            .border_b(px(0.5))
            .border_color(SEPARATOR)
            .children(toggle)
            .child(strip)
            .children(ring.map(|r| div().ml_auto().pl(px(8.)).flex().flex_none().items_center().child(r)))
    }

    /// Top tab `i` of worktree `tree`, holding the panes of `t`.
    fn top_tab(&self, tree: &str, i: usize, t: &Tree<Tab>, cx: &mut Context<Self>) -> Stateful<Div> {
        let w = &self.workspaces[tree];
        let (selected, closable) = (i == w.at, !w.others.is_empty());
        let ink = if selected { TEXT } else { TEXT_3 };
        let (head, meta) = copy(t);
        let panes = t.panes().len();
        let agent = [head, meta].into_iter().flatten().find_map(|tab| match tab {
            Tab::Term(id) => self.summary(id),
            _ => None,
        });
        let lead = match (agent, head) {
            (Some(a), _) => provider_icon(&a.provider, 14., ink).into_any_element(),
            (None, Some(Tab::Doc(Doc::Commit(_)))) => icon("diff-multiple", 14., ink).into_any_element(),
            (None, Some(Tab::Doc(Doc::File(p) | Doc::Diff(p) | Doc::CommitFile { path: p, .. } | Doc::PrFile { path: p, .. }))) => file_icon(p, false, false, 14.).into_any_element(),
            (None, Some(Tab::Web(_))) => icon("globe", 14., TEXT_3).into_any_element(),
            _ => icon("prompt", 14., TEXT_3).into_any_element(),
        };
        let italic = matches!(head, Some(Tab::Doc(d)) if w.preview.as_ref() == Some(d));
        let meta = match (meta, head) {
            (Some(m), _) => self.tab_text(m),
            _ if panes > 1 => format!("{panes} panes"),
            (None, Some(h)) => self.tab_kind(h),
            (None, None) => String::new(),
        };
        let close = ui::control(div().id(("close-top", i)))
            .absolute()
            .right(px(4.))
            .top(px(5.))
            .size(px(20.))
            .flex()
            .items_center()
            .justify_center()
            .rounded(px(4.))
            .cursor_pointer()
            .opacity(0.)
            .group_hover("top-tab", |s| s.opacity(1.))
            .hover(|s| s.bg(FILL_3))
            .child(icon("x", 12., TEXT_3))
            .on_click(cx.listener(move |this, _: &ClickEvent, _, cx| {
                cx.stop_propagation();
                this.close_top(i, cx);
            }));
        let text = div()
            .min_w_0()
            .flex()
            .flex_col()
            .justify_center()
            .line_height(relative(1.2))
            .child(div().text_size(px(12.5)).font_weight(FontWeight::MEDIUM).truncate().when(italic, |d| d.italic()).child(head.map(|h| self.tab_text(h)).unwrap_or_default()))
            .child(div().text_size(px(11.)).text_color(TEXT_3).truncate().child(meta));
        div()
            .id(("top-tab", i))
            .group("top-tab")
            .relative()
            .w_full()
            .h(px(30.))
            .flex()
            .items_center()
            .gap(px(7.))
            .pl(px(8.))
            .pr(px(28.))
            .rounded(px(6.))
            .cursor_pointer()
            .text_color(ink)
            .when(selected, |d| d.bg(FILL_3))
            .when(!selected, |d| d.hover(|s| s.bg(FILL_1).text_color(TEXT)))
            .child(div().flex_none().flex().when(!selected, |d| d.opacity(0.55)).child(lead))
            .child(text)
            .when(closable && self.panels.top_drag.is_none(), |d| d.child(close))
            // Else the bar's drag_area takes the press and moves the window.
            .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
            .on_click(cx.listener(move |this, _: &ClickEvent, window, cx| this.switch_top(i, window, cx)))
            .on_drag(DragTop, {
                let this = cx.weak_entity();
                move |_, grab, window, cx| {
                    let pointer = f32::from(window.mouse_position().x);
                    this.update(cx, |this, _| {
                        this.panels.top_drag = Some(TabDrag::new(i, grab.x.into(), pointer));
                        this.freeze_pages(window);
                    })
                    .ok();
                    cx.new(|_| EmptyView)
                }
            })
            .when(closable, |d| d.on_mouse_up(MouseButton::Middle, cx.listener(move |this, _: &MouseUpEvent, _, cx| this.close_top(i, cx))))
    }

    /// Follows a held top tab: along the bar it makes room among the others; below it, one not shown leaves the bar to join the shown one beside a pane.
    fn hold_top(&mut self, at: (f32, f32), cx: &mut Context<Self>) {
        let Some(tree) = self.place() else { return };
        let (shown, b) = (self.workspace(&tree).at, self.panels.bar);
        let Some(d) = self.panels.top_drag.as_mut() else { return };
        let away = at.1 >= b.y + b.h && d.from != shown;
        // A drag move comes every frame, moved or not; notifying on each would redraw forever.
        if d.away != away || (!away && d.pointer != at.0) {
            cx.notify();
        }
        (d.pointer, d.away) = (at.0, away);
        let from = d.from;
        if away {
            self.aim_top(from, &tree, at, cx);
        } else if self.panels.pane_drag.take().is_some() {
            cx.notify();
        }
    }

    fn release_top(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(d) = self.panels.top_drag.as_mut() else { return };
        let moved = d.release();
        if self.drop_top(window, cx) {
            return;
        }
        let Some((from, to)) = moved else { return };
        let Some(tree) = self.place() else { return };
        let at = {
            let w = self.workspace(&tree);
            w.move_top(from, to);
            w.at
        };
        // The shown tab was dragged into view already; scrolling to its new index would jump the bar.
        self.panels.top_revealed = Some((tree, at));
        self.save_soon(cx);
        cx.notify();
    }

    /// What kind of tab a lone tab is: its agent, a terminal, the folder of its file, its changes, or a page.
    fn tab_kind(&self, tab: &Tab) -> String {
        match tab {
            Tab::Term(id) => self.summary(id).map_or_else(|| "Terminal".to_string(), |a| self.agent_name(&a.provider)),
            Tab::Doc(Doc::File(p)) => {
                let dir = Path::new(p).parent().unwrap_or(Path::new(""));
                match self.explore_root().map(|root| dir.strip_prefix(root).map(Path::to_path_buf)) {
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
        let Some(tree) = self.place() else { return };
        self.workspace(&tree).switch(i);
        let (p, a) = {
            let t = &self.workspace(&tree).tree;
            (t.focused, t.focused().active)
        };
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
